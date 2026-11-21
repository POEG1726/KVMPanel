import { useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useConfig } from '../console/menus/hooks';
import { ME_KEY, fetchMe } from '../session/useSession';
import { useUi } from '../state/ui';
import { FloatingWindow } from '../ui/FloatingWindow';
import { toast } from '../ui/Toast';
import { FileManager } from './FileManager';
import { TerminalPane, type TerminalActions } from './TerminalPane';
import s from './workspace.module.css';

export default function Workspace({ mode }: { mode: 'float' | 'tab' }) {
  const { t } = useTranslation();
  const config = useConfig();
  const { data: user, error } = useQuery({ queryKey: ME_KEY, queryFn: fetchMe, staleTime: Infinity });
  const close = useUi(st => st.closeWorkspace);
  const panes = useRef<HTMLDivElement>(null);
  const terminal = useRef<TerminalActions>(null);
  const [percent, setPercent] = useState(48);
  const [initial] = useState(() => ({
    x: Math.round(innerWidth * 0.05), y: Math.round(innerHeight * 0.14),
    w: Math.round(innerWidth * 0.90), h: Math.round(innerHeight * 0.80),
  }));
  if (error) return <p role="alert">{t('error.unknown')}</p>;
  if (!user || !config) return null;
  const canTerm = user.role === 'admin' || user.permissions.includes('terminal');
  const canFiles = user.role === 'admin' || user.permissions.includes('files');
  const both = canTerm && canFiles;
  const adjust = (v: number) => setPercent(Math.max(20, Math.min(80, Math.round(v))));
  const body = <div className={s.work} ref={panes} style={{ gridTemplateColumns: both ? `minmax(0, ${percent}fr) 7px minmax(0, ${100-percent}fr)` : 'minmax(0,1fr)' }}>
    {canTerm && <TerminalPane ref={terminal} serial={config.serial} />}
    {both && <div role="separator" tabIndex={0} aria-label={t('workspace.split')} aria-orientation="vertical"
      aria-valuemin={20} aria-valuemax={80} aria-valuenow={percent} className={s.split}
      onPointerDown={e => { if (e.button === 0) e.currentTarget.setPointerCapture(e.pointerId); }}
      onPointerMove={e => {
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
        const rect = panes.current!.getBoundingClientRect();
        if (rect.width) adjust(100 * (e.clientX - rect.left) / rect.width);
      }}
      onKeyDown={e => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
        e.preventDefault(); adjust(percent + (e.key === 'ArrowRight' ? 5 : -5));
      }} />}
    {canFiles && <FileManager onOpenTerminal={canTerm ? path => {
      if (!terminal.current?.openDirectory(path)) toast(t('workspace.terminalUnavailable'), 'error');
    } : undefined} />}
    {!canTerm && !canFiles && <p role="alert">{t('error.forbidden')}</p>}
  </div>;
  if (mode === 'tab') return <main className={s.tabPage} aria-label={t('menu.workspace')}>{body}</main>;
  return <FloatingWindow id="workspace" title={t('menu.workspace') + ' · ' + config.device.hostname} initial={initial}
    remember={config.web.remember_float} onClose={close} onPopOut={() => {
      if (window.open('/workspace')) close();
      else toast(t('workspace.popupBlocked'), 'error');
    }}>{body}</FloatingWindow>;
}
