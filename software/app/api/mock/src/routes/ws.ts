import type { FastifyInstance } from 'fastify';
import type WebSocket from 'ws';
import { messageValidator } from '../contract.js';
import type { Account, State } from '../state.js';
import { accountForToken, COOKIE, hasPerm } from './auth.js';
import { trackSocket } from '../sockets.js';

const validClient = messageValidator('ws.schema.json', 'ClientMessage');
const STATUS_INTERVAL_MS = 1000;

/** Live console sessions: one entry per open /api/ws connection. */
export function statusMessage(state: State, live: Set<Conn>) {
  return {
    t: 'status' as const,
    hdmi: { ...state.hdmi },
    stream: { fps: state.hdmi.signal ? Math.min(28, Number(state.config.video.fps_max)) : 0, kbps: state.hdmi.signal ? 24800 : 0 },
    power: { ...state.power },
    locks: { ...state.locks },
    sessions: [...live].map((c) => ({ user: c.account.name, ip: c.ip, since: c.since, view_only: c.account.view_only })),
    media: { ...state.media },
  };
}

interface Conn {
  socket: WebSocket;
  account: Account;
  ip: string;
  since: number;
}

const LOCK_KEYS: Record<string, keyof State['locks']> = { NumLock: 'num', CapsLock: 'caps', ScrollLock: 'scroll' };

export function registerWs(app: FastifyInstance, state: State) {
  const live = new Set<Conn>();
  const send = (c: Conn, m: unknown) => c.socket.readyState === 1 && c.socket.send(JSON.stringify(m));
  const broadcastStatus = () => live.forEach((c) => send(c, statusMessage(state, live)));

  state.events.on('status_changed', broadcastStatus);
  state.events.on('usb_reenumerated', () => live.forEach((c) => send(c, { t: 'usb_reenumerated' })));
  state.events.on('kvm_restarting', () => live.forEach((c) => send(c, { t: 'kvm_restarting' })));

  const timer = setInterval(broadcastStatus, STATUS_INTERVAL_MS);
  timer.unref();
  app.addHook('onClose', async () => {
    clearInterval(timer);
    live.forEach((c) => c.socket.terminate());
  });

  app.get('/api/ws', { websocket: true }, (socket, req) => {
    const account = accountForToken(state, req.cookies[COOKIE]);
    if (!account) return socket.close(4401, 'unauthenticated');
    if (!hasPerm(account, 'console')) return socket.close(4403, 'forbidden');

    trackSocket(state, socket, req.cookies[COOKIE]!, 'console');
    const conn: Conn = { socket, account, ip: req.ip || req.socket?.remoteAddress || '0.0.0.0', since: state.now() };
    live.add(conn);
    state.consoleSessions = live.size;
    state.events.emit('console_sessions');
    socket.on('close', () => {
      live.delete(conn);
      state.consoleSessions = live.size;
      state.events.emit('console_sessions');
      broadcastStatus();
    });
    broadcastStatus();

    socket.on('message', (data, isBinary) => {
      let msg: { t: string; [k: string]: unknown };
      try {
        if (isBinary) throw new Error('binary');
        msg = JSON.parse(data.toString());
      } catch {
        return send(conn, { t: 'error', error: 'bad_message' });
      }
      if (!validClient(msg)) return send(conn, { t: 'error', error: 'bad_message' });
      if (msg.t === 'ping') return send(conn, { t: 'pong' });
      // A disabled account or ended session loses input rights immediately.
      if (!accountForToken(state, req.cookies[COOKIE])) return socket.close(4401, 'unauthenticated');
      if (account.view_only) return send(conn, { t: 'error', error: 'forbidden' });
      if (msg.t === 'key' && msg.down === true && typeof msg.code === 'string' && msg.code in LOCK_KEYS) {
        const k = LOCK_KEYS[msg.code];
        state.locks[k] = !state.locks[k];
        state.events.emit('status_changed');
      }
      // Other input would be written to the HID gadget on the real device.
    });
  });
}
