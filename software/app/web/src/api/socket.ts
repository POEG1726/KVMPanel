import { emitLoginRequired, ownPasswordChangeActive } from './authEvents';

export type ClosePolicy = 'login' | 'forbidden' | 'no_serial' | 'disconnected' | 'restarting' | 'retry';

/** Maps a WebSocket close code to what the UI must do (see constraints: close codes). */
export function closePolicy(code: number): ClosePolicy {
  switch (code) {
    case 4401:
      return 'login';
    case 4403:
      return 'forbidden';
    case 4404:
      return 'no_serial';
    case 4409:
      return 'disconnected';
    case 1012:
      return 'restarting';
    default:
      return 'retry';
  }
}

/** The subset of the WebSocket API used here; satisfied by browser WebSocket, `ws` and test fakes. */
export interface WebSocketLike {
  readyState: number;
  binaryType: string;
  send(data: string | ArrayBufferLike | ArrayBufferView): void;
  close(code?: number): void;
  onopen: ((e: never) => void) | null;
  onmessage: ((e: { data: string | ArrayBuffer }) => void) | null;
  onclose: ((e: { code: number }) => void) | null;
}

export type SocketFactory = (url: string) => WebSocketLike;

interface SocketConfig {
  /** http(s) origin to connect to instead of `location` (tests). */
  base?: string;
  /** Custom socket constructor (tests: Node `ws` with a cookie header, or a fake). */
  create?: SocketFactory;
}

let config: SocketConfig = {};

/** Test hook: override the origin and/or socket constructor. Call with `{}` to reset. */
export function configureSockets(next: SocketConfig): void {
  config = next;
}

function wsUrl(path: string): string {
  const origin = config.base ?? `${location.protocol}//${location.host}`;
  return origin.replace(/^http/, 'ws') + path;
}

export interface SocketHandlers {
  onOpen?(): void;
  onMessage(data: string | ArrayBuffer): void;
  onClosed?(policy: ClosePolicy): void;
}

const registry = new Set<SocketHandle>();

/** Closes every live socket and stops their reconnects (session ended). */
export function closeAllSockets(): void {
  [...registry].forEach((h) => h.close());
}

export interface SocketHandle {
  send(d: string | ArrayBufferLike | ArrayBufferView): void;
  close(): void;
}

const OPEN = 1;
const BACKOFF_CAP_S = 10;
const STABLE_MS = 10_000;
const NO_RECONNECT: ReadonlySet<ClosePolicy> = new Set(['login', 'forbidden', 'no_serial', 'disconnected']);

/**
 * Opens a WebSocket and keeps it alive: 1, 2, 4, 8 s then 10 s backoff after 1012 or abnormal
 * closes, reset once a connection has stayed open for 10 s. 4401/4403/4404/4409 never reconnect.
 */
export function connectSocket(path: string, handlers: SocketHandlers, opts: { reconnect?: boolean } = {}): SocketHandle {
  const reconnect = opts.reconnect ?? true;
  let ws: WebSocketLike | null = null;
  let stopped = false;
  let attempt = 0;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let stableTimer: ReturnType<typeof setTimeout> | undefined;

  const connect = () => {
    const url = wsUrl(path);
    const sock = config.create ? config.create(url) : (new WebSocket(url) as unknown as WebSocketLike);
    sock.binaryType = 'arraybuffer';
    ws = sock;
    sock.onopen = () => {
      stableTimer = setTimeout(() => (attempt = 0), STABLE_MS);
      handlers.onOpen?.();
    };
    sock.onmessage = (e) => handlers.onMessage(e.data);
    sock.onclose = (e) => {
      clearTimeout(stableTimer);
      if (stopped || ws !== sock) return;
      ws = null;
      let policy = closePolicy(e.code);
      // Own password change: the server closed us with 4401 but re-issued our cookie, so reconnect.
      if (policy === 'login' && ownPasswordChangeActive()) policy = 'retry';
      handlers.onClosed?.(policy);
      if (policy === 'login') emitLoginRequired();
      if (!reconnect || NO_RECONNECT.has(policy)) {
        registry.delete(handle);
        return;
      }
      const delay = Math.min(2 ** attempt, BACKOFF_CAP_S) * 1000;
      attempt += 1;
      retryTimer = setTimeout(connect, delay);
    };
  };
  connect();

  const handle: SocketHandle = {
    send(d) {
      if (ws?.readyState === OPEN) ws.send(d);
    },
    close() {
      registry.delete(handle);
      stopped = true;
      clearTimeout(retryTimer);
      clearTimeout(stableTimer);
      if (ws) {
        const sock = ws;
        // Closing a still-connecting socket makes Node's `ws` throw asynchronously; wait for it to open.
        if (sock.readyState === 0) sock.onopen = () => sock.close();
        else sock.close();
      }
      ws = null;
    },
  };
  registry.add(handle);
  return handle;
}
