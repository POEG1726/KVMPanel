import { useEffect, useId, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import s from './FloatingWindow.module.css';

interface Rect { x: number; y: number; w: number; h: number }
interface Props {
  id: string;
  title: string;
  initial: Rect;
  remember: boolean;
  onClose(): void;
  onPopOut(): void;
  children: ReactNode;
}

type Operation = 'move' | 'resize';
interface Gesture {
  pointer: number;
  op: Operation;
  startX: number;
  startY: number;
  start: Rect;
  latest: Rect;
}

const viewport = () => ({ width: window.innerWidth, height: window.innerHeight });
const clamp = (n: number, min: number, max: number) => Math.min(Math.max(min, n), max);

function fit(rect: Rect, { width, height }: ReturnType<typeof viewport>): Rect {
  const w = clamp(rect.w, 320, width);
  const h = clamp(rect.h, 180, height);
  return { x: clamp(rect.x, 0, width - w), y: clamp(rect.y, 0, height - h), w, h };
}

function load(key: string, initial: Rect): Rect {
  try {
    const rect = JSON.parse(localStorage.getItem(key) ?? 'null');
    if (rect && [rect.x, rect.y, rect.w, rect.h].every(Number.isFinite) && rect.w > 0 && rect.h > 0) return rect;
  } catch { /* Browser storage is optional. */ }
  return initial;
}

export function FloatingWindow({ id, title, initial, remember, onClose, onPopOut, children }: Props) {
  const { t } = useTranslation();
  const titleId = useId();
  const storageKey = 'kvm.float.' + id;
  const [bounds, setBounds] = useState(viewport);
  const [normal, setNormal] = useState(() => remember ? load(storageKey, initial) : initial);
  const [minimized, setMinimized] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const gesture = useRef<Gesture | null>(null);
  const win = useRef<HTMLDivElement>(null);
  const rect = fit(normal, bounds);
  const isMax = maximized && !minimized;
  const shown = isMax ? { x: 0, y: 0, w: bounds.width, h: bounds.height } : rect;

  useEffect(() => {
    if (!win.current?.contains(document.activeElement)) win.current?.focus();
    const resize = () => setBounds(viewport());
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);

  function persist(next: Rect) {
    if (!remember) return;
    try { localStorage.setItem(storageKey, JSON.stringify(next)); } catch { /* Keep the in-memory position. */ }
  }

  function adjust(start: Rect, op: Operation, dx: number, dy: number) {
    if (op === 'move') return fit({ ...start, x: start.x + dx, y: start.y + dy }, bounds);
    return {
      ...start,
      w: clamp(start.w + dx, 320, bounds.width - start.x),
      h: clamp(start.h + dy, 180, bounds.height - start.y),
    };
  }

  function start(e: PointerEvent<HTMLElement>, op: Operation) {
    if (isMax || e.button !== 0 || gesture.current) return;
    if (op === 'move' && (e.target as HTMLElement).closest('button')) return;
    e.currentTarget.focus();
    e.preventDefault();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    gesture.current = { pointer: e.pointerId, op, startX: e.clientX, startY: e.clientY, start: rect, latest: rect };
  }

  function move(e: PointerEvent) {
    const g = gesture.current;
    if (!g || g.pointer !== e.pointerId) return;
    g.latest = adjust(g.start, g.op, e.clientX - g.startX, e.clientY - g.startY);
    setNormal(g.latest);
  }

  function finish(e: PointerEvent) {
    const g = gesture.current;
    if (!g || g.pointer !== e.pointerId) return;
    persist(g.latest);
    gesture.current = null;
  }

  function arrows(e: KeyboardEvent, op: Operation) {
    if (isMax || e.target !== e.currentTarget) return;
    const offsets: Record<string, [number, number]> = {
      ArrowLeft: [-10, 0], ArrowRight: [10, 0], ArrowUp: [0, -10], ArrowDown: [0, 10],
    };
    const delta = offsets[e.key];
    if (!delta) return;
    e.preventDefault();
    const next = adjust(rect, op, ...delta);
    setNormal(next);
    persist(next);
  }

  return createPortal(
    <div ref={win} tabIndex={-1} role="dialog" aria-labelledby={titleId}
      className={s.window} data-maximized={isMax}
      style={{ left: shown.x, top: shown.y, width: shown.w, height: minimized ? 'auto' : shown.h }}
      onPointerMove={move} onPointerUp={finish} onPointerCancel={finish} onLostPointerCapture={finish}>
      <header className={s.bar} tabIndex={isMax ? -1 : 0} title={isMax ? undefined : t('float.move')}
        onPointerDown={(e) => start(e, 'move')} onKeyDown={(e) => arrows(e, 'move')}>
        <h2 id={titleId}>{title}</h2>
        <button type="button" className={s.pop} onClick={onPopOut} aria-label={t('float.popOut')} title={t('float.popOut')}>
          <span aria-hidden="true">⧉</span><span className={s.popLabel}>{t('float.newTab')}</span>
        </button>
        <button type="button" onClick={() => setMinimized(!minimized)}
          aria-label={t(minimized ? 'float.restore' : 'float.minimize')} title={t(minimized ? 'float.restore' : 'float.minimize')}>
          {minimized ? '▴' : '−'}
        </button>
        <button type="button" onClick={() => { setMaximized(!isMax); setMinimized(false); }}
          aria-label={t(isMax ? 'float.restore' : 'float.maximize')} title={t(isMax ? 'float.restore' : 'float.maximize')}>
          {isMax ? '❐' : '□'}
        </button>
        <button type="button" className={s.close} onClick={onClose} aria-label={t('common.close')} title={t('common.close')}>×</button>
      </header>
      <div hidden={minimized} className={s.body}>{children}</div>
      {!isMax && !minimized &&
        <button type="button" className={s.grip} aria-label={t('float.resize')} title={t('float.resize')}
          onPointerDown={(e) => start(e, 'resize')} onKeyDown={(e) => arrows(e, 'resize')} />}
    </div>,
    document.body,
  );
}
