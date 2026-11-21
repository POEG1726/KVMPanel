import { useEffect, useId, useRef, useState, type ComponentType } from 'react';
import { createPortal } from 'react-dom';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui';
import { ME_KEY, fetchMe } from '../session/useSession';
import { PAGE_PERMISSION, SETTINGS_GROUPS, type SettingsPageId } from './registry';
import { AboutPage } from './pages/AboutPage';
import { AccountsPage } from './pages/AccountsPage';
import { DevicePage } from './pages/DevicePage';
import { FunctionKeyPage } from './pages/FunctionKeyPage';
import { GeneralPage } from './pages/GeneralPage';
import { InputPage } from './pages/InputPage';
import { McpPage } from './pages/McpPage';
import { MediaPage } from './pages/MediaPage';
import { NetworkPage } from './pages/NetworkPage';
import { PanelPage } from './pages/PanelPage';
import { StoragePage } from './pages/StoragePage';
import { TailscalePage } from './pages/TailscalePage';
import { UpdatePage } from './pages/UpdatePage';
import { UsbFunctionsPage } from './pages/UsbFunctionsPage';
import { VideoPage } from './pages/VideoPage';
import s from './settings.module.css';

const PAGES: Record<SettingsPageId, ComponentType> = {
  general: GeneralPage,
  input: InputPage,
  video: VideoPage,
  media: MediaPage,
  panel: PanelPage,
  usb: UsbFunctionsPage,
  function_key: FunctionKeyPage,
  storage: StoragePage,
  network: NetworkPage,
  tailscale: TailscalePage,
  accounts: AccountsPage,
  mcp: McpPage,
  update: UpdatePage,
  device: DevicePage,
  about: AboutPage,
};

interface Props {
  /** The page to open first. */
  page?: SettingsPageId;
  onClose?(): void;
}

/** The settings window: a sidebar of grouped pages (design 5.2) and the selected page. Pages the user has no permission for are left out. */
export function SettingsWindow({ page = 'general', onClose }: Props) {
  const { t } = useTranslation();
  const titleId = useId();
  const win = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const opener = document.activeElement;
    win.current?.focus();
    return () => { if (opener instanceof HTMLElement && opener.isConnected) opener.focus(); };
  }, []);
  const [current, setCurrent] = useState<SettingsPageId>(page);
  useEffect(() => setCurrent(page), [page]);
  const me = useQuery({ queryKey: ME_KEY, queryFn: fetchMe, retry: false, staleTime: Infinity });

  const allowed = (id: SettingsPageId) => {
    const need = PAGE_PERMISSION[id];
    return !need || !me.data || me.data.role === 'admin' || me.data.permissions.includes(need);
  };
  const groups = SETTINGS_GROUPS.map((g) => ({ id: g.id, pages: g.pages.filter(allowed) })).filter((g) => g.pages.length > 0);
  const visible = groups.flatMap((g) => g.pages as readonly SettingsPageId[]);
  const shown = visible.includes(current) ? current : visible[0];
  const Page = shown ? PAGES[shown] : null;

  return createPortal(
    <div
      className={s.mask}
      data-keep-open
      // A dialog on top (confirm) has already used its Escape.
      onKeyDown={e => {
        const dialog = win.current;
        if (!dialog || e.defaultPrevented || !dialog.contains(e.target as Node)) return;
        if (e.key === 'Escape') { e.preventDefault(); onClose?.(); return; }
        if (e.key !== 'Tab') return;
        const controls = Array.from(dialog.querySelectorAll<HTMLElement>('button:enabled, input:enabled, select:enabled, textarea:enabled, a[href]'))
          .filter(control => control.tabIndex >= 0 && !control.closest('[hidden], input[type="hidden"]'));
        const first = controls[0], last = controls.at(-1);
        if (e.shiftKey && (e.target === dialog || e.target === first)) { e.preventDefault(); last?.focus(); }
        else if (!e.shiftKey && e.target === last) { e.preventDefault(); first?.focus(); }
      }}
    >
      <div ref={win} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby={titleId} className={s.window}>
        <div className={s.titlebar}>
          <h2 id={titleId}>{t('menu.settings')}</h2>
          <Button aria-label={t('common.close')} onClick={onClose}>
            ✕
          </Button>
        </div>
        <nav className={s.side} aria-label={t('menu.settings')}>
          {groups.map((g) => (
            <div key={g.id}>
              <div className={s.sideTitle}>{t(`settings.group.${g.id}`)}</div>
              {g.pages.map((id) => (
                <button key={id} type="button" className={s.nav} aria-current={id === shown ? 'page' : undefined} onClick={() => setCurrent(id)}>
                  {t(`settings.page.${id}`)}
                </button>
              ))}
            </div>
          ))}
        </nav>
        <div className={s.content}>
          {shown && <h2>{t(`settings.page.${shown}`)}</h2>}
          {Page && <Page key={shown} />}
        </div>
      </div>
    </div>,
    document.body,
  );
}
