import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { useTranslation } from 'react-i18next';
import { connectSocket, type ClosePolicy, type SocketHandle } from '../api/socket';
import type { Config } from '../api/client';
import s from './workspace.module.css';

export interface TerminalActions {
  openDirectory(path: string): boolean;
}

interface ScreenProps {
  serial: boolean;
  active: boolean;
  onMissing(): void;
}

const TerminalScreen = forwardRef<TerminalActions, ScreenProps>(function TerminalScreen({ serial, active, onMissing }, ref) {
  const { t } = useTranslation();
  const host = useRef<HTMLDivElement>(null);
  const socket = useRef<SocketHandle | null>(null);
  const terminal = useRef<Terminal | null>(null);
  const online = useRef(false);
  const [policy, setPolicy] = useState<ClosePolicy | null>(null);
  const [attempt, setAttempt] = useState(0);

  useImperativeHandle(ref, () => ({
    openDirectory(path) {
      if (!online.current || !path.startsWith('/') || /[\x00-\x1f\x7f-\x9f]/.test(path)) return false;
      const quoted = "'" + path.replaceAll("'", "'\\''") + "'";
      socket.current?.send(new TextEncoder().encode('cd ' + quoted + '\r'));
      terminal.current?.focus();
      return true;
    },
  }), []);

  useEffect(() => {
    const el = host.current!;
    const term = new Terminal({
      fontFamily: '"Cascadia Mono", Consolas, monospace', fontSize: 13, cursorBlink: true,
      theme: { background: '#05070a', foreground: '#d9e1ec', cursor: '#5ec4ff' },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    terminal.current = term;
    let dimensions = '';
    const resize = () => {
      if (el.clientWidth && el.clientHeight) fit.fit();
      const next = JSON.stringify({ t: 'resize', cols: term.cols, rows: term.rows });
      if (online.current && next !== dimensions) {
        handle.send(next);
        dimensions = next;
      }
    };
    const handle = connectSocket('/api/terminal?kind=' + (serial ? 'serial' : 'local'), {
      onOpen() {
        online.current = true;
        setPolicy(null);
        dimensions = '';
        resize();
      },
      onMessage(data) {
        if (typeof data !== 'string') term.write(new Uint8Array(data));
      },
      onClosed(reason) {
        online.current = false;
        setPolicy(reason);
        if (reason === 'no_serial' && serial) onMissing();
      },
    });
    socket.current = handle;
    const input = term.onData(data => handle.send(new TextEncoder().encode(data)));
    const binary = term.onBinary(data => handle.send(Uint8Array.from(data, c => c.charCodeAt(0))));
    const observer = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(resize) : null;
    observer?.observe(el);
    return () => {
      online.current = false;
      observer?.disconnect();
      input.dispose();
      binary.dispose();
      handle.close();
      term.dispose();
      terminal.current = null;
      socket.current = null;
    };
  }, [serial, onMissing, attempt]);

  useEffect(() => { if (active) terminal.current?.focus(); }, [active, attempt]);
  return <div className={s.screen} hidden={!active} role="tabpanel">
    {policy && <div role="alert" className={s.notice}>
      {t('connection.' + policy)}
      {policy === 'device_unavailable' && <button onClick={() => setAttempt(n => n + 1)}>{t('workspace.retry')}</button>}
    </div>}
    <div className={s.xterm} ref={host} />
  </div>;
});

export const TerminalPane = forwardRef<TerminalActions, { serial: Config['serial'] }>(function TerminalPane({ serial }, ref) {
  const { t } = useTranslation();
  const [locals, setLocals] = useState([1]);
  const next = useRef(2);
  const [selected, select] = useState<number | 'serial'>(1);
  const [openedPort, setOpenedPort] = useState<string | null>(null);
  const [missing, setMissing] = useState<string | null>(null);
  const serialAvailable = !!serial.device && serial.device !== missing;
  const port = serial.device + ':' + serial.baud;
  const showSerial = serialAvailable && openedPort === port;
  const active = selected === 'serial' && !showSerial ? locals[0] : selected;
  const onMissing = useCallback(() => {
    setMissing(serial.device);
    setOpenedPort(null);
    select(1);
  }, [serial.device]);
  const localMissing = useCallback(() => {}, []);
  const newLocal = () => {
    const id = next.current++;
    setLocals(ids => [...ids, id]);
    select(id);
  };
  return <section className={s.terminal} aria-label={t('workspace.terminal')}>
    <div className={s.tabs}>
      <div role="tablist" aria-label={t('workspace.terminal')}>
        {locals.map(id => <button role="tab" key={id} aria-selected={active === id} onClick={() => select(id)}>
          {t('workspace.local')}{id > 1 && ' ' + id}
        </button>)}
        {serialAvailable && <button role="tab" aria-selected={active === 'serial'} onClick={() => {
          setOpenedPort(port); select('serial');
        }}>{t('workspace.serial')} · {serial.device} · {serial.baud}</button>}
      </div>
      <button aria-label={t('workspace.addTerminal')} title={t('workspace.addTerminal')} onClick={newLocal}>＋</button>
      {typeof active === 'number' && active > 1 && <button aria-label={t('workspace.closeTerminal')} onClick={() => {
        setLocals(ids => ids.filter(id => id !== active)); select(1);
      }}>×</button>}
    </div>
    {locals.map(id => <TerminalScreen key={id} active={active === id} serial={false} ref={active === id ? ref : null} onMissing={localMissing} />)}
    {showSerial && <TerminalScreen key={port} active={active === 'serial'} serial ref={active === 'serial' ? ref : null} onMissing={onMissing} />}
  </section>;
});
