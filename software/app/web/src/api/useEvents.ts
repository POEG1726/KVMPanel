import { useCallback, useEffect, useRef, useState } from 'react';
import type { User } from '../session/useSession';
import { connectSocket, type ClosePolicy, type SocketHandle } from './socket';
import type { ClientMessage, Error as ErrorMessage, StatusMessage } from './ws-types';

export interface EventsState {
  status: StatusMessage | null;
  connected: boolean;
  viewOnly: boolean;
  latencyMs: number | null;
  /** Why the socket is down (null while connected); drives the banner and redirects. */
  banner: ClosePolicy | null;
  send(m: ClientMessage): boolean;
  /** Calls `fn` for every `{t:'error'}` message the server sends; returns the unsubscribe function. */
  onServerError(fn: (error: ErrorMessage['error']) => void): () => void;
}

function isViewOnly(user?: User, status?: StatusMessage | null) {
  // Each console session for a username has the same account-level view_only setting.
  const session = status?.sessions?.find(s => s.user === user?.name);
  return session?.view_only ?? user?.view_only ?? false;
}

/** Subscribes to /api/ws: live status plus connection state; input messages go out via `send`. */
export function useEvents(enabled = true, user?: User): EventsState {
  const [status, setStatus] = useState<StatusMessage | null>(null);
  const [latencyMs, setLatencyMs] = useState<number | null>(null);
  const [connected, setConnected] = useState(false);
  const [banner, setBanner] = useState<ClosePolicy | null>(null);
  const handle = useRef<SocketHandle | null>(null);
  const latestStatus = useRef<StatusMessage | null>(null);
  const account = useRef(user);
  account.current = user;
  const errorListeners = useRef(new Set<(error: ErrorMessage['error']) => void>());

  useEffect(() => {
    if (!enabled) return;
    let pingTimer: ReturnType<typeof setInterval>;
    let sentAt: number | undefined;
    handle.current = connectSocket('/api/ws', {
      onOpen() {
        setConnected(true);
        setBanner(null);
        pingTimer = setInterval(() => {
          if (sentAt !== undefined) return;
          sentAt = performance.now();
          handle.current?.send(JSON.stringify({ t: 'ping' }));
        }, 3000);
      },
      onMessage(data) {
        if (typeof data !== 'string') return;
        try {
          const m = JSON.parse(data) as { t?: string };
          if (m.t === 'status') {
            latestStatus.current = m as StatusMessage;
            setStatus(latestStatus.current);
          } else if (m.t === 'pong' && sentAt !== undefined) {
            setLatencyMs(Math.round(performance.now() - sentAt));
            sentAt = undefined;
          } else if (m.t === 'kvm_restarting') setBanner('restarting');
          else if (m.t === 'error') errorListeners.current.forEach((fn) => fn((m as ErrorMessage).error));
        } catch {
          // ignore malformed frames
        }
      },
      onClosed(policy) {
        clearInterval(pingTimer);
        sentAt = undefined;
        setLatencyMs(null);
        setConnected(false);
        setBanner(policy);
      },
    });
    return () => {
      clearInterval(pingTimer);
      handle.current?.close();
      handle.current = null;
    };
  }, [enabled]);

  const send = useCallback((m: ClientMessage) => {
    if (m.t !== 'ping' && isViewOnly(account.current, latestStatus.current)) return false;
    return handle.current?.send(JSON.stringify(m)) ?? false;
  }, []);
  const onServerError = useCallback((fn: (error: ErrorMessage['error']) => void) => {
    errorListeners.current.add(fn);
    return () => void errorListeners.current.delete(fn);
  }, []);
  return { status, connected, viewOnly: isViewOnly(user, status), latencyMs, banner, send, onServerError };
}
