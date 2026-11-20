import { useCallback, useEffect, useRef, type CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { api, unwrap } from '../api/client';
import { useEventsContext } from '../api/EventsProvider';
import type { ClientMessage } from '../api/ws-types';
import { CONFIG_KEY } from '../session/useSession';
import { absMessage, buttonsMask, createWheelAccumulator, keyMessage, relMessage } from './input';
import { useStream } from './useStream';

export type VideoScale = 'fit' | '1:1' | 'fill';

const SCALE_STYLE: Record<VideoScale, CSSProperties> = {
  fit: { maxWidth: '100%', maxHeight: '100%' },
  '1:1': {},
  fill: { width: '100%', height: '100%' },
};

interface Props {
  scale: VideoScale;
  /** Called with every pointer/wheel message that was sent. */
  onPointer?: (m: ClientMessage) => void;
  /** CSS cursor over the remote screen (the local cursor style setting). */
  cursor?: string;
}

/** Draws the newest decoded stream frame and turns focused keyboard and pointer input into HID messages. */
export function VideoCanvas({ scale, onPointer, cursor }: Props) {
  const { t } = useTranslation();
  const { send } = useEventsContext();
  const config = useQuery({ queryKey: CONFIG_KEY, queryFn: () => unwrap(api.GET('/api/config')), staleTime: Infinity });
  const relative = config.data?.hid.mouse_mode === 'rel';
  const wheelDir = config.data?.hid.wheel_dir;
  const wheelSpeed = config.data?.hid.wheel_speed;

  const canvas = useRef<HTMLCanvasElement>(null);
  const latest = useRef<ImageBitmap | null>(null);
  const decoding = useRef(false);
  const raf = useRef(0);
  const pressed = useRef(new Set<string>());
  const buttons = useRef(0);
  const lastPointer = useRef<Extract<ClientMessage, { t: 'mouse_abs' | 'mouse_rel' }> | null>(null);
  const pendingAbs = useRef<{ x: number; y: number } | null>(null);
  const pendingRel = useRef({ dx: 0, dy: 0 });
  const pointerRaf = useRef(0);
  const wheel = useRef(createWheelAccumulator());

  const draw = useCallback(() => {
    raf.current = 0;
    const el = canvas.current;
    const bmp = latest.current;
    latest.current = null;
    if (!el || !bmp) return;
    if (el.width !== bmp.width || el.height !== bmp.height) {
      el.width = bmp.width;
      el.height = bmp.height;
    }
    el.getContext('2d')?.drawImage(bmp, 0, 0);
    bmp.close?.();
  }, []);

  // Only the newest undecoded frame is kept: while one decodes, each arrival replaces the queued bytes.
  const pending = useRef<ArrayBuffer | null>(null);
  const alive = useRef(true);
  const decodeNext = useCallback(() => {
    const jpeg = pending.current;
    if (!jpeg || decoding.current) return;
    pending.current = null;
    decoding.current = true;
    createImageBitmap(new Blob([jpeg], { type: 'image/jpeg' }))
      .then((bmp) => {
        if (!alive.current) return bmp.close?.();
        latest.current?.close?.();
        latest.current = bmp;
        if (!raf.current) raf.current = requestAnimationFrame(draw);
      })
      .catch(() => undefined)
      .finally(() => {
        decoding.current = false;
        decodeNext();
      });
  }, [draw]);

  useStream((jpeg) => {
    pending.current = jpeg;
    decodeNext();
  });

  const emit = useCallback(
    (m: ClientMessage) => {
      if (m.t === 'mouse_abs' || m.t === 'mouse_rel') lastPointer.current = m;
      send(m);
      onPointer?.(m);
    },
    [send, onPointer],
  );

  const flushPointer = useCallback(() => {
    pointerRaf.current = 0;
    const el = canvas.current;
    if (pendingAbs.current && el) {
      emit(absMessage(pendingAbs.current.x, pendingAbs.current.y, el.getBoundingClientRect(), buttons.current));
      pendingAbs.current = null;
    }
    const { dx, dy } = pendingRel.current;
    if (dx || dy) {
      emit(relMessage(dx, dy, buttons.current));
      pendingRel.current = { dx: 0, dy: 0 };
    }
  }, [emit]);

  const schedule = () => {
    if (!pointerRaf.current) pointerRaf.current = requestAnimationFrame(flushPointer);
  };

  useEffect(() => {
    alive.current = true; // StrictMode runs mount, cleanup, mount
    return () => {
      alive.current = false;
      pending.current = null;
      cancelAnimationFrame(raf.current);
      cancelAnimationFrame(pointerRaf.current);
      latest.current?.close?.();
      latest.current = null;
      raf.current = 0;
      pointerRaf.current = 0;
    };
  }, []);

  // React registers wheel listeners as passive, so preventDefault needs a native one.
  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const m = wheel.current(e.deltaY, { deltaMode: e.deltaMode, dir: wheelDir, speed: wheelSpeed });
      if (m) emit(m);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [emit, wheelDir, wheelSpeed]);

  const releaseInput = useCallback(() => {
    pressed.current.forEach((code) => send({ t: 'key', code, down: false }));
    pressed.current.clear();
    cancelAnimationFrame(pointerRaf.current);
    pointerRaf.current = 0;
    pendingAbs.current = null;
    pendingRel.current = { dx: 0, dy: 0 };
    buttons.current = 0;
    const last = lastPointer.current;
    if (last?.buttons) emit(last.t === 'mouse_abs' ? { ...last, buttons: 0 } : relMessage(0, 0, 0));
  }, [emit, send]);

  useEffect(() => {
    const lockChanged = () => { if (document.pointerLockElement !== canvas.current) releaseInput(); };
    const hidden = () => { if (document.visibilityState === 'hidden') releaseInput(); };
    window.addEventListener('blur', releaseInput);
    document.addEventListener('pointerlockchange', lockChanged);
    document.addEventListener('visibilitychange', hidden);
    return () => {
      window.removeEventListener('blur', releaseInput);
      document.removeEventListener('pointerlockchange', lockChanged);
      document.removeEventListener('visibilitychange', hidden);
      releaseInput();
    };
  }, [releaseInput]);

  const onKey = (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    const m = keyMessage(e.nativeEvent);
    if (!m || m.t !== 'key') return;
    e.preventDefault();
    if (m.down) {
      if (e.repeat || pressed.current.has(m.code)) return;
      pressed.current.add(m.code);
    } else if (!pressed.current.delete(m.code)) {
      return;
    }
    send(m);
  };

  const onPointerEvent = (e: React.PointerEvent<HTMLCanvasElement>) => {
    // In relative mode motion only counts while the pointer is locked to this canvas.
    if (relative && e.type === 'pointermove' && document.pointerLockElement !== e.currentTarget) return;
    buttons.current = buttonsMask(e.nativeEvent);
    if (relative) {
      pendingRel.current.dx += e.nativeEvent.movementX || 0;
      pendingRel.current.dy += e.nativeEvent.movementY || 0;
      if (e.type === 'pointermove') return schedule();
    } else {
      pendingAbs.current = { x: e.clientX, y: e.clientY };
      if (e.type === 'pointermove') return schedule();
    }
    // Button changes go out immediately so a click is never reordered after later motion.
    cancelAnimationFrame(pointerRaf.current);
    if (relative && !pendingRel.current.dx && !pendingRel.current.dy) {
      emit(relMessage(0, 0, buttons.current));
      pointerRaf.current = 0;
    } else {
      flushPointer();
    }
  };

  return (
    <canvas
      ref={canvas}
      tabIndex={0}
      role="application"
      aria-label={t('console.screen')}
      style={{ display: 'block', margin: 'auto', outline: 'none', cursor, ...SCALE_STYLE[scale] }}
      onKeyDown={onKey}
      onKeyUp={onKey}
      onBlur={releaseInput}
      onPointerCancel={releaseInput}
      onContextMenu={(e) => e.preventDefault()}
      onPointerDown={(e) => {
        e.currentTarget.focus();
        if (relative && document.pointerLockElement !== e.currentTarget) void e.currentTarget.requestPointerLock?.();
        e.currentTarget.setPointerCapture?.(e.pointerId);
        onPointerEvent(e);
      }}
      onPointerUp={onPointerEvent}
      onPointerMove={onPointerEvent}
    />
  );
}
