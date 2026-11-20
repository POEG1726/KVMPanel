import { useEffect, useRef, useState, type ComponentType, type ReactNode } from 'react';
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
/** Panels near the right end open leftwards so they stay on screen. */
const ALIGN_RIGHT = new Set<MenuId>(['power', 'wol']);

export function MenuBar() {
  const { t } = useTranslation();
  const { status } = useEventsContext();
  const autoHide = useConfig()?.web.menubar === 'auto_hide';
  const openWorkspace = useUi((u) => u.openWorkspace);
  const openSettings = useUi((u) => u.openSettings);

  const [open, setOpen] = useState<MenuId | null>(null);
  const [hidden, setHidden] = useState(false);
  const [hover, setHover] = useState(false);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [fullscreen, setFullscreen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const drag = useRef<{ px: number; py: number; ox: number; oy: number } | null>(null);

  useEffect(() => {
    if (!autoHide || open) {
      setHidden(false);
      return;
    }
    if (hover || hidden) return;
    const timer = setTimeout(() => setHidden(true), AUTO_HIDE_MS);
    return () => clearTimeout(timer);
  }, [autoHide, open, hover, hidden]);

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
          <div className={`${s.dd} ${ALIGN_RIGHT.has(id) ? s.ddRight : ''}`}>
            <Panel />
          </div>
        )}
      </div>
    );
  };

  return (
    <>
      <div className={s.hotzone} data-testid="menubar-hotzone" onPointerEnter={() => setHidden(false)} />
      <div
        ref={root}
        role="toolbar"
        aria-label={t('menu.bar')}
        data-hidden={hidden}
        className={`${s.bar} ${hidden ? s.hidden : ''}`}
        style={{ transform: `translate(calc(-50% + ${offset.x}px), ${hidden ? '-120%' : `${offset.y}px`})` }}
        onPointerEnter={() => setHover(true)}
        onPointerLeave={() => setHover(false)}
      >
        <span
          className={s.logo}
          title={t('menu.drag')}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture?.(e.pointerId);
            drag.current = { px: e.clientX, py: e.clientY, ox: offset.x, oy: offset.y };
          }}
          onPointerMove={(e) => {
            const d = drag.current;
            if (d) setOffset({ x: d.ox + e.clientX - d.px, y: Math.max(0, d.oy + e.clientY - d.py) });
          }}
          onPointerUp={() => (drag.current = null)}
          onPointerCancel={() => (drag.current = null)}
        >
          KVM<i>Panel</i>
        </span>
        <span className={s.sep} />
        {BEFORE_WORKSPACE.map(menuButton)}
        <button type="button" className={s.btn} onClick={openWorkspace}>
          {ICONS.workspace}
          {t('menu.workspace')}
        </button>
        {AFTER_WORKSPACE.map(menuButton)}
        <span className={s.sep} />
        <span className={s.status}>{hdmi?.signal ? `${hdmi.width}×${hdmi.height} · ${status?.stream.fps ?? 0} fps` : t('menu.noSignal')}</span>
        <span className={s.locks} aria-hidden="true">
          <span className={locks?.num ? s.lockOn : undefined}>NUM</span>
          <span className={locks?.caps ? s.lockOn : undefined}>CAPS</span>
          <span className={locks?.scroll ? s.lockOn : undefined}>SCR</span>
        </span>
        <span className={s.sep} />
        <button type="button" className={s.btn} onClick={() => openSettings()}>
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
