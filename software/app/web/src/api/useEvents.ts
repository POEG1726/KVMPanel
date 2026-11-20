import { useCallback, useEffect, useRef, useState } from 'react';
import { connectSocket, type ClosePolicy, type SocketHandle } from './socket';
import type { ClientMessage, StatusMessage } from './ws-types';

export interface EventsState {
  status: StatusMessage | null;
  connected: boolean;
  /** Why the socket is down (null while connected); drives the banner and redirects. */
  banner: ClosePolicy | null;
  send(m: ClientMessage): void;
}

/** Subscribes to /api/ws: live status plus connection state; input messages go out via `send`. */
export function useEvents(): EventsState {
  const [status, setStatus] = useState<StatusMessage | null>(null);
  const [connected, setConnected] = useState(false);
  const [banner, setBanner] = useState<ClosePolicy | null>(null);
  const handle = useRef<SocketHandle | null>(null);

  useEffect(() => {
    handle.current = connectSocket('/api/ws', {
      onOpen() {
        setConnected(true);
        setBanner(null);
      },
      onMessage(data) {
        if (typeof data !== 'string') return;
        try {
          const m = JSON.parse(data) as { t?: string };
          if (m.t === 'status') setStatus(m as StatusMessage);
        } catch {
          // ignore malformed frames
        }
      },
      onClosed(policy) {
        setConnected(false);
        setBanner(policy);
      },
    });
    return () => {
      handle.current?.close();
      handle.current = null;
    };
  }, []);

  const send = useCallback((m: ClientMessage) => handle.current?.send(JSON.stringify(m)), []);
  return { status, connected, banner, send };
}
