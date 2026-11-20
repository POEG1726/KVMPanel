import { useEffect, useLayoutEffect, useRef, useState, type ComponentType, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useEventsContext } from '../api/EventsProvider';
import { useUi } from '../state/ui';
import { KeyboardMenu } from './menus/KeyboardMenu';
import { MediaMenu } from './menus/MediaMenu';
import { MouseMenu } from './menus/MouseMenu';
import { PowerMenu } from './menus/PowerMenu';
import { useConfig } from './menus/hooks';
import { VideoMenu } from './menus/VideoMenu';
import { WolMenu } from './menus/WolMenu';
import s from './MenuBar.module.css';

/** Idle time before an auto-hide bar slides away. */
export const AUTO_HIDE_MS = 3000;

type MenuId = 'video' | 'keyboard' | 'mouse' | 'media' | 'power' | 'wol';

const Icon = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 16 16" aria-hidden="true">
    {children}
  </svg>
);

const ICONS: Record<MenuId | 'workspace' | 'settings' | 'fullscreen', ReactNode> = {
  video: (
    <Icon>
      <rect x="1.5" y="2.5" width="13" height="9" rx="1.5" />
      <path d="M5.5 14h5" />
    </Icon>
  ),
  keyboard: (
    <Icon>
      <rect x="1" y="4" width="14" height="8" rx="1.5" />
      <path d="M4 7h1M7 7h1M10 7h1M5 9.5h6" />
    </Icon>
  ),
  mouse: (
    <Icon>
      <rect x="4" y="1.5" width="8" height="13" rx="4" />
      <path d="M8 4v3" />
    </Icon>
  ),
  media: (
    <Icon>
      <circle cx="8" cy="8" r="6.5" />
      <circle cx="8" cy="8" r="1.8" />
    </Icon>
  ),
  workspace: (
    <Icon>
      <rect x="1.5" y="2.5" width="13" height="11" rx="1.5" />
      <path d="M4.5 6.5l2 1.5-2 1.5M8 10h3.5" />
    </Icon>
  ),
  power: (
    <Icon>
      <path d="M8 1.5v6" />
      <path d="M4.5 4a5.5 5.5 0 1 0 7 0" />
    </Icon>
  ),
  wol: (
    <Icon>
      <path d="M2 9.5a8 8 0 0 1 12 0M4.5 11.5a4.5 4.5 0 0 1 7 0" />
      <circle cx="8" cy="13.5" r=".8" />
    </Icon>
  ),
  settings: (
    <Icon>
      <circle cx="8" cy="8" r="2.3" />
      <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
    </Icon>
  ),
  fullscreen: (
    <Icon>
      <path d="M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4" />
    </Icon>
  ),
};

const PANELS: Record<MenuId, ComponentType> = {
  video: VideoMenu,
  keyboard: KeyboardMenu,
  mouse: MouseMenu,
  media: MediaMenu,
  power: PowerMenu,
  wol: WolMenu,
};

/** Left of the workspace button; the rest follow it. */
const BEFORE_WORKSPACE: MenuId[] = ['video', 'keyboard', 'mouse', 'media'];
const AFTER_WORKSPACE: MenuId[] = ['power', 'wol'];
const EDGE = 4;
const PANEL_GAP = 6;
type Position = { x: number; y: number };

function keepInView(value: number, size: number, available: number) {
  return Math.max(EDGE, Math.min(value, available - size - EDGE));
}

// Use the wrapped toolbar's actual size and the open button as the panel's anchor.
function placeBar(bar: HTMLElement, panel: HTMLElement | null, preferred: Position | null) {
  const x = keepInView(preferred?.x ?? (innerWidth - bar.offsetWidth) / 2, bar.offsetWidth, innerWidth);
  const y = keepInView(preferred?.y ?? 10, bar.offsetHeight, innerHeight);
  bar.style.left = x + 'px';
  bar.style.top = y + 'px';
  if (!panel) return;

  const trigger = panel.previousElementSibling as HTMLElement;
  const left = keepInView(x + bar.clientLeft + trigger.offsetLeft, panel.offsetWidth, innerWidth);
  panel.style.left = (left - x - bar.clientLeft) + 'px';
  const above = y - EDGE - PANEL_GAP;
  const below = innerHeight - y - bar.offsetHeight - EDGE - PANEL_GAP;
  const opensUp = above > below && below < panel.scrollHeight + 2;
  const edge = `calc(100% + ${PANEL_GAP}px)`;
  panel.style.top = opensUp ? 'auto' : edge;
  panel.style.bottom = opensUp ? edge : 'auto';
  panel.style.maxHeight = Math.max(0, opensUp ? above : below) + 'px';
}

export function MenuBar() {
  const { t } = useTranslation();
  const { status, latencyMs } = useEventsContext();
  const logoRevision = useUi(st => st.logoRevision);
  const web = useConfig()?.web;
  const autoHide = web?.menubar === 'auto_hide';
  const openWorkspace = useUi((u) => u.openWorkspace);
  const openSettings = useUi((u) => u.openSettings);

  const [open, setOpen] = useState<MenuId | null>(null);
  const [hidden, setHidden] = useState(false);
  const [focused, setFocused] = useState(false);
  const [hover, setHover] = useState(false);
  const preferred = useRef<Position | null>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const drag = useRef<{ pointer: number; dx: number; dy: number } | null>(null);

  useLayoutEffect(() => {
    const bar = root.current!;
    const dropdown = panel.current;
    const place = () => placeBar(bar, dropdown, preferred.current);
    place();
    // A language, status, logo or panel change can alter dimensions independently of the viewport.
    const observer = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(place) : null;
    observer?.observe(bar);
    if (dropdown) {
      observer?.observe(dropdown);
      if (dropdown.firstElementChild) observer?.observe(dropdown.firstElementChild);
    }
    window.addEventListener('resize', place);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', place);
    };
  }, [open]);

  useEffect(() => {
    if (!autoHide || open) {
      setHidden(false);
      return;
    }
    if (hover || focused || hidden) return;
    const timer = setTimeout(() => setHidden(true), AUTO_HIDE_MS);
    return () => clearTimeout(timer);
  }, [autoHide, open, hover, focused, hidden]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      const target = e.target as Element;
      if (!root.current?.contains(target) && !target.closest?.('[data-keep-open]')) setOpen(null);
    };
    // A handler that used the key (a dialog, the shortcut recorder) has called preventDefault.
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && !e.defaultPrevented && setOpen(null);
    document.addEventListener('pointerdown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  useEffect(() => {
    const sync = () => setFullscreen(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', sync);
    return () => document.removeEventListener('fullscreenchange', sync);
  }, []);

  const toggleFullscreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen?.();
    else void document.documentElement.requestFullscreen?.();
  };

  const hdmi = status?.hdmi;
  const locks = status?.locks;

  const menuButton = (id: MenuId) => {
    const Panel = PANELS[id];
    return (
      <div key={id} className={s.item}>
        <button
          type="button"
          className={`${s.btn} ${open === id ? s.on : ''}`}
          aria-expanded={open === id}
          aria-haspopup="true"
          onClick={() => setOpen(open === id ? null : id)}
        >
          {id === 'power' && <span className={`${s.dot} ${status?.power.on ? s.dotOn : ''}`} data-testid="power-dot" data-on={!!status?.power.on} />}
          {ICONS[id]}
          {t(`menu.${id}`)}
        </button>
        {open === id && (
          <div className={s.dd} ref={panel}>
            <Panel />
          </div>
        )}
      </div>
    );
  };

  return (
    <>
      <div className={s.hotzone} data-testid="menubar-hotzone" role={hidden ? 'button' : undefined}
        aria-label={hidden ? t('menu.bar') : undefined} tabIndex={hidden ? 0 : -1}
        onPointerEnter={() => setHidden(false)} onClick={() => setHidden(false)}
        onKeyDown={e => {
          if (e.key !== 'Enter' && e.key !== ' ') return;
          e.preventDefault();
          // Remove inert before moving focus into the revealed toolbar.
          root.current?.removeAttribute('inert');
          setHidden(false);
          root.current?.querySelector('button')?.focus();
        }} />
      <div
        ref={root}
        role="toolbar"
        aria-label={t('menu.bar')}
        data-hidden={hidden} {...(hidden ? { inert: '' } : {})}
        onFocus={() => setFocused(true)}
        onBlur={e => { if (!e.currentTarget.contains(e.relatedTarget)) setFocused(false); }}
        className={`${s.bar} ${hidden ? s.hidden : ''}`}
        onPointerEnter={() => setHover(true)}
        onPointerLeave={() => setHover(false)}
      >
        <span
          className={s.logo}
          title={t('menu.drag')}
          onPointerDown={(e) => {
            if (e.button !== 0 || drag.current) return;
            const bounds = root.current!.getBoundingClientRect();
            drag.current = { pointer: e.pointerId, dx: e.clientX - bounds.left, dy: e.clientY - bounds.top };
            e.currentTarget.setPointerCapture?.(e.pointerId);
          }}
          onPointerMove={(e) => {
            const d = drag.current;
            if (!d || d.pointer !== e.pointerId) return;
            preferred.current = { x: e.clientX - d.dx, y: e.clientY - d.dy };
            placeBar(root.current!, panel.current, preferred.current);
          }}
          onPointerUp={(e) => { if (drag.current?.pointer === e.pointerId) drag.current = null; }}
          onPointerCancel={(e) => { if (drag.current?.pointer === e.pointerId) drag.current = null; }}
          onLostPointerCapture={(e) => { if (drag.current?.pointer === e.pointerId) drag.current = null; }}
        >
          <img key={logoRevision} src={'/api/branding/logo?v=' + logoRevision} alt="" hidden
            onLoad={e => { e.currentTarget.hidden = false; }} onError={e => { e.currentTarget.hidden = true; }} />
          KVM<i>Panel</i>
        </span>
        <span className={s.sep} />
        {BEFORE_WORKSPACE.map(menuButton)}
        <button type="button" className={s.btn} onClick={() => { setOpen(null); openWorkspace(web?.workspace_open); }}>
          {ICONS.workspace}
          {t('menu.workspace')}
        </button>
        {AFTER_WORKSPACE.map(menuButton)}
        <span className={s.sep} />
        <span className={s.status}>{hdmi?.signal ? `${hdmi.width}×${hdmi.height} · ${status?.stream.fps ?? 0} fps` : t('menu.noSignal')}{latencyMs !== null && ` · ${latencyMs} ms`}</span>
        <span className={s.locks} aria-hidden="true">
          <span className={locks?.num ? s.lockOn : undefined}>NUM</span>
          <span className={locks?.caps ? s.lockOn : undefined}>CAPS</span>
          <span className={locks?.scroll ? s.lockOn : undefined}>SCR</span>
        </span>
        <span className={s.sep} />
        <button type="button" className={s.btn} onClick={() => { setOpen(null); openSettings(); }}>
          {ICONS.settings}
          {t('menu.settings')}
        </button>
        <button type="button" className={s.btn} aria-label={t(fullscreen ? 'menu.exitFullscreen' : 'menu.fullscreen')} onClick={toggleFullscreen}>
          {ICONS.fullscreen}
        </button>
      </div>
    </>
  );
}
