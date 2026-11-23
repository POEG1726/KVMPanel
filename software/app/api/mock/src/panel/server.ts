import { createServer, type Server, type Socket } from 'node:net';
import { messageValidator } from '../contract.js';
import { applyConfigPatch } from '../routes/config.js';
import { setPcPower } from '../routes/power.js';
import { deviceInfo } from '../routes/system.js';
import type { State } from '../state.js';
import { snapshot } from './metrics.js';

const validFromPanel = messageValidator('panel.schema.json', 'FromPanel');
const TELEMETRY_MS = 1000;
const STATUS_MS = 5000;

export type Listen = { port: number; host?: string } | { path: string };

function statusMessage(state: State) {
  return {
    t: 'status' as const,
    eth: { state: 'up' as const, mbps: 1000, ip: '192.168.1.50' },
    // USB-C carries HID and the host program; HID alone enumerates while the PC is on but the host app is not running.
    pc: { state: state.pc.connected ? ('ok' as const) : state.power.on ? ('partial' as const) : ('off' as const) },
    usb_a: { device: 'USB Flash Disk' },
    kvm: { enabled: Boolean(state.config.kvm.enabled), sessions: state.consoleSessions },
    lux: 212,
  };
}

const USB_FUNCTIONS = ['keyboard', 'mouse_rel', 'mouse_abs', 'storage', 'net', 'acm', 'telemetry'] as const;

/** The board's own state for the PC-offline page (panel.schema.json Board). */
export function boardMessage(state: State, tick: number) {
  const info = deviceInfo(state);
  const signal = state.hdmi.signal && state.power.on;
  const streaming = signal && state.activeStreams > 0;
  const fps = Number(state.config.video.fps_max);
  const wave = (period: number, phase = 0) => Math.sin((tick / period + phase) * 2 * Math.PI);
  // Board's own traffic: idle chatter, plus roughly 24 Mbit/s of MJPEG per open stream.
  const tx = 42e3 + 18e3 * Math.abs(wave(7)) + (streaming ? state.activeStreams * 24e6 * (0.9 + 0.1 * wave(5)) : 0);
  const rx = 31e3 + 12e3 * Math.abs(wave(9, 0.3)) + (streaming ? 0.4e6 : 0);
  const mounted = state.media.mounted;
  const https = state.network.https.enabled;
  return {
    t: 'board' as const,
    ts: state.now(),
    pc_power: state.power.on ? ('on' as const) : ('off' as const),
    hostname: info.hostname,
    version: info.version,
    uptime_s: info.uptime_s,
    cpu_pct: Math.round((14 + 6 * wave(11) + (streaming ? 38 : 0)) * 10) / 10,
    soc_temp_c: Math.round((info.soc_temp_c + 2 * wave(40) + (streaming ? 6 : 0)) * 10) / 10,
    mem: { used: info.mem.used + 4e6 * wave(23), total: info.mem.total },
    sd: { used: info.data.used, total: info.data.total },
    eth: { iface: 'eth0', link_mbps: info.link_mbps, duplex: 'full' as const, mac: info.mac, rx_bps: Math.round(rx), tx_bps: Math.round(tx) },
    access: { url: `${https ? 'https' : 'http'}://${info.hostname}.local`, tailscale_ip: null },
    kvm: {
      sessions: state.consoleClients.map((c) => ({ ...c })),
      hdmi: signal
        ? { signal: true, width: state.hdmi.width, height: state.hdmi.height, fps: state.hdmi.fps }
        : { signal: false, width: null, height: null, fps: null },
      stream: streaming ? { state: 'streaming' as const, fps } : { state: 'idle' as const, fps: null },
      media: { mounted: mounted ? mounted.split('/').pop()! : null, mode: state.media.mode },
      usb_functions: USB_FUNCTIONS.filter((f) => state.usbApplied[f] === true),
    },
  };
}

/** NDJSON server for the dashboard process (TCP on a PC, a Unix socket on the board). */
export function startPanelServer(state: State, listen: Listen): Promise<Server> {
  const clients = new Set<Socket>();
  const send = (s: Socket, m: unknown) => { if (!s.destroyed) s.write(JSON.stringify(m) + '\n'); };
  const broadcast = (m: unknown) => clients.forEach((s) => send(s, m));

  let tick = 0;
  let wasConnected = state.pc.connected;
  const telemetry = setInterval(() => {
    tick += 1;
    if (state.pc.connected !== wasConnected) {
      wasConnected = state.pc.connected;
      broadcast({ t: 'pc', connected: state.pc.connected, last_seen_ts: state.pc.last_seen_ts });
      broadcast(statusMessage(state));
    }
    broadcast(boardMessage(state, tick));
    if (!state.pc.connected) return;
    state.pc.last_seen_ts = state.now();
    const panel = state.config.panel as { thresholds: Record<string, number>; hidden_metrics: string[] };
    broadcast({ t: 'telemetry', ts: state.now(), metrics: snapshot(tick, panel.thresholds, panel.hidden_metrics) });
  }, TELEMETRY_MS);
  const status = setInterval(() => broadcast(statusMessage(state)), STATUS_MS);

  const onConfig = (config: unknown) => broadcast({ t: 'config', config });
  // kvmd serves this socket, so a restart drops it; the panel reconnects and a new hello means kvmd is back.
  const onRestart = () => {
    broadcast({ t: 'kvm_restarting' });
    setImmediate(() => clients.forEach((s) => s.end()));
  };
  const onStatus = () => broadcast(statusMessage(state));
  state.events.on('config_changed', onConfig);
  state.events.on('kvm_restarting', onRestart);
  state.events.on('console_sessions', onStatus);
  state.events.on('status_changed', onStatus);

  const server = createServer((sock) => {
    clients.add(sock);
    send(sock, { t: 'hello', version: state.version });
    send(sock, { t: 'config', config: state.config });
    send(sock, statusMessage(state));
    send(sock, { t: 'pc', connected: state.pc.connected, last_seen_ts: state.pc.last_seen_ts });
    send(sock, boardMessage(state, tick));
    let buf = '';
    sock.on('data', (d) => {
      buf += d.toString('utf8');
      let nl: number;
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl).trim();
        buf = buf.slice(nl + 1);
        if (line) handle(sock, line);
      }
    });
    sock.on('close', () => clients.delete(sock));
    sock.on('error', () => clients.delete(sock));
  });

  function handle(sock: Socket, line: string) {
    let m: { t: string; id: number; patch?: unknown; action?: string };
    try { m = JSON.parse(line); } catch { return send(sock, { t: 'result', ok: false, error: 'bad_message' }); }
    if (!validFromPanel(m)) {
      const id = (m as { id?: unknown })?.id;
      return send(sock, Number.isInteger(id) && (id as number) >= 0
        ? { t: 'result', id, ok: false, error: 'bad_message' }
        : { t: 'result', ok: false, error: 'bad_message' });
    }
    const reply = (extra: Record<string, unknown>) => send(sock, { t: 'result', id: m.id, ...extra });
    switch (m.t) {
      case 'get_config':
        return reply({ ok: true, data: state.config });
      case 'patch_config': {
        // The panel is a local trusted process: no per-section permission check.
        const r = applyConfigPatch(state, m.patch, null);
        return r.ok ? reply({ ok: true, data: r.config }) : reply({ ok: false, error: r.error, ...(r.detail ? { detail: r.detail } : {}) });
      }
      case 'device_info':
        return reply({ ok: true, data: deviceInfo(state) });
      case 'action':
        if (m.action === 'eject_media') {
          state.mediaMount = null;
          state.media = { mounted: null, mode: null };
          state.events.emit('status_changed');
        } else if (m.action === 'usb_apply' || m.action === 'reset_hid') {
          state.events.emit('usb_reenumerated');
        } else if (m.action === 'restart_kvm') {
          setImmediate(() => state.events.emit('kvm_restarting'));
        } else if (m.action === 'disconnect_sessions') {
          state.events.emit('disconnect_sessions');
        } else if (m.action === 'pc_power_on') {
          // Never let the panel switch the PC off: a short press while it is on would.
          if (state.power.on) return reply({ ok: false, error: 'pc_on' });
          setPcPower(state, true);
          state.events.emit('status_changed');
        }
        return reply({ ok: true });
    }
  }

  server.on('close', () => {
    clearInterval(telemetry);
    clearInterval(status);
    state.events.off('config_changed', onConfig);
    state.events.off('kvm_restarting', onRestart);
    state.events.off('console_sessions', onStatus);
    state.events.off('status_changed', onStatus);
    clients.forEach((s) => s.destroy());
  });

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    const done = () => resolve(server);
    if ('path' in listen) server.listen(listen.path, done);
    else server.listen(listen.port, listen.host ?? '127.0.0.1', done);
  });
}
