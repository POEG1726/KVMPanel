import { useCallback, useEffect, useRef, type CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { api, unwrap } from '../api/client';
import { useEventsContext } from '../api/EventsProvider';
import type { ClientMessage } from '../api/ws-types';
import { CONFIG_KEY } from '../session/useSession';
import { absMessage, buttonsMask, keyMessage, relMessage, wheelMessage } from './input';
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
}

/** Draws the newest decoded stream frame and turns focused keyboard and pointer input into HID messages. */
export function VideoCanvas({ scale, onPointer }: Props) {
  const { t } = useTranslation();
  const { send } = useEventsContext();
  const config = useQuery({ queryKey: CONFIG_KEY, queryFn: () => unwrap(api.GET('/api/config')), staleTime: Infinity });
  const relative = config.data?.hid.mouse_mode === 'rel';

  const canvas = useRef<HTMLCanvasElement>(null);
  const latest = useRef<ImageBitmap | null>(null);
  const decoding = useRef(false);
  const raf = useRef(0);
  const pressed = useRef(new Set<string>());
  const buttons = useRef(0);
  const pendingAbs = useRef<{ x: number; y: number } | null>(null);
  const pendingRel = useRef({ dx: 0, dy: 0 });
  const pointerRaf = useRef(0);

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

  // A frame that arrives while the previous one is still decoding is dropped.
  useStream((jpeg) => {
    if (decoding.current) return;
    decoding.current = true;
    createImageBitmap(new Blob([jpeg], { type: 'image/jpeg' }))
      .then((bmp) => {
        latest.current?.close?.();
        latest.current = bmp;
        if (!raf.current) raf.current = requestAnimationFrame(draw);
      })
      .catch(() => undefined)
      .finally(() => (decoding.current = false));
  });

  const emit = useCallback(
    (m: ClientMessage) => {
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

  useEffect(
    () => () => {
      cancelAnimationFrame(raf.current);
      cancelAnimationFrame(pointerRaf.current);
      latest.current?.close?.();
    },
    [],
  );

  // React registers wheel listeners as passive, so preventDefault needs a native one.
  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      emit(wheelMessage(e.deltaY));
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [emit]);

  const releaseKeys = () => {
    pressed.current.forEach((code) => send({ t: 'key', code, down: false }));
    pressed.current.clear();
  };

  const onKey = (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    const m = keyMessage(e.nativeEvent);
    if (!m || m.t !== 'key') return;
    e.preventDefault();
    if (m.down) pressed.current.add(m.code);
    else pressed.current.delete(m.code);
    send(m);
  };

  const onPointerEvent = (e: React.PointerEvent<HTMLCanvasElement>) => {
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
    flushPointer();
    if (relative && !pendingRel.current.dx && !pendingRel.current.dy) {
      emit(relMessage(0, 0, buttons.current));
    }
  };

  return (
    <canvas
      ref={canvas}
      tabIndex={0}
      role="application"
      aria-label={t('console.screen')}
      style={{ display: 'block', margin: 'auto', outline: 'none', ...SCALE_STYLE[scale] }}
      onKeyDown={onKey}
      onKeyUp={onKey}
      onBlur={releaseKeys}
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
