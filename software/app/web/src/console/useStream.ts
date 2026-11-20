import { useEffect, useRef } from 'react';
import { connectSocket } from '../api/socket';

/**
 * Subscribes to /api/stream and hands each binary JPEG frame to `onFrame`.
 * Sends pause/resume when the tab hides/shows (and re-pauses after a reconnect while hidden).
 */
export function useStream(onFrame: (jpeg: ArrayBuffer) => void): void {
  const cb = useRef(onFrame);
  cb.current = onFrame;

  useEffect(() => {
    const visibility = () => JSON.stringify({ t: document.visibilityState === 'hidden' ? 'pause' : 'resume' });
    let closed = false;
    const handle = connectSocket('/api/stream', {
      onOpen() {
        if (document.visibilityState === 'hidden') handle.send(visibility());
      },
      onMessage(data) {
        if (!closed && typeof data !== 'string') cb.current(data);
      },
    });
    const onChange = () => handle.send(visibility());
    document.addEventListener('visibilitychange', onChange);
    return () => {
      document.removeEventListener('visibilitychange', onChange);
      closed = true;
      handle.close();
    };
  }, []);
}
