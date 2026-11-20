import { useEffect, useLayoutEffect, useRef, type ReactNode } from 'react';
import s from './FileManager.module.css';

interface Props {
  x: number;
  y: number;
  opener: HTMLElement;
  label: string;
  onClose(): void;
  children: ReactNode;
}

/** Position inside the file pane even if the floating window is partly off screen. */
export function FileActionMenu({ x, y, opener, label, onClose, children }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const menu = ref.current!;
    const pane = menu.offsetParent?.getBoundingClientRect();
    if (pane) {
      const right = Math.min(pane.width, window.innerWidth - pane.left);
      const bottom = Math.min(pane.height, window.innerHeight - pane.top);
      menu.style.left = Math.max(0, -pane.left, Math.min(x, right - menu.offsetWidth)) + 'px';
      menu.style.top = Math.max(0, -pane.top, Math.min(y, bottom - menu.offsetHeight)) + 'px';
    }
    menu.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [x, y]);
  useEffect(() => {
    const outside = (e: PointerEvent) => {
      if (e.target instanceof Node && !ref.current?.contains(e.target)) onClose();
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [onClose]);

  return <div role="menu" aria-label={label} className={s.actions} ref={ref} style={{ left: x, top: y }}
    onBlur={e => { if (!e.currentTarget.contains(e.relatedTarget)) onClose(); }}
    onKeyDown={e => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); opener.focus(); return; }
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
      const items = [...e.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]')];
      const index = items.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      items[(index + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length].focus();
    }}>{children}</div>;
}
