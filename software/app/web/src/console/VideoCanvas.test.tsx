import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { EventsProvider } from '../api/EventsProvider';
import { configureSockets, type SocketFactory, type WebSocketLike } from '../api/socket';
import { loginCookie, mockUrl, wsFactory } from '../../test/mock';
import { VideoCanvas } from './VideoCanvas';
import { useStream } from './useStream';

interface Fake extends WebSocketLike {
  url: string;
  sent: string[];
}

function fakeFactory() {
  const socks: Fake[] = [];
  const create: SocketFactory = (url) => {
    const s: Fake = {
      url,
      sent: [],
      readyState: 1,
      binaryType: '',
      send: (d) => void s.sent.push(String(d)),
      close() {},
      onopen: null,
      onmessage: null,
      onclose: null,
    };
    socks.push(s);
    setTimeout(() => s.onopen?.(undefined as never));
    return s;
  };
  return { create, socks, sentOn: (path: string) => socks.filter((s) => s.url.endsWith(path)).flatMap((s) => s.sent) };
}

const setVisibility = (v: 'hidden' | 'visible') => {
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => v });
  document.dispatchEvent(new Event('visibilitychange'));
};

afterEach(() => {
  cleanup();
  configureSockets({});
  setVisibility('visible');
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('useStream', () => {
  it('sends pause then resume when the tab hides and shows', () => {
    const f = fakeFactory();
    configureSockets({ base: 'http://x', create: f.create });
    const { unmount } = renderHook(() => useStream(() => undefined));
    setVisibility('hidden');
    setVisibility('visible');
    expect(f.sentOn('/api/stream').map((m) => JSON.parse(m).t)).toEqual(['pause', 'resume']);
    unmount();
  });
});

function wrap(children: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <EventsProvider>{children}</EventsProvider>
    </QueryClientProvider>
  );
}

describe('VideoCanvas', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 500 })));
  });

  it('sends nothing to the remote PC while an input has focus, and keys when the canvas has focus', async () => {
    const f = fakeFactory();
    configureSockets({ base: 'http://x', create: f.create });
    render(wrap(<><input aria-label="field" /><VideoCanvas scale="fit" /></>));
    await waitFor(() => expect(f.socks.some((s) => s.url.endsWith('/api/ws'))).toBe(true));
    await act(async () => void (await new Promise((r) => setTimeout(r, 10))));

    const input = screen.getByLabelText('field');
    input.focus();
    fireEvent.keyDown(input, { code: 'KeyA' });
    fireEvent.keyUp(input, { code: 'KeyA' });
    expect(f.sentOn('/api/ws')).toEqual([]);

    const canvas = screen.getByRole('application');
    canvas.focus();
    fireEvent.keyDown(canvas, { code: 'KeyA' });
    fireEvent.keyUp(canvas, { code: 'KeyA' });
    expect(f.sentOn('/api/ws').map((m) => JSON.parse(m))).toEqual([
      { t: 'key', code: 'KeyA', down: true },
      { t: 'key', code: 'KeyA', down: false },
    ]);
  });

  it('releases held keys when focus leaves the canvas', async () => {
    const f = fakeFactory();
    configureSockets({ base: 'http://x', create: f.create });
    render(wrap(<VideoCanvas scale="fit" />));
    await act(async () => void (await new Promise((r) => setTimeout(r, 10))));
    const canvas = screen.getByRole('application');
    fireEvent.keyDown(canvas, { code: 'ShiftLeft' });
    fireEvent.blur(canvas);
    expect(f.sentOn('/api/ws').map((m) => JSON.parse(m).down)).toEqual([true, false]);
  });

  it('draws a frame from the mock stream within 2 seconds', async () => {
    vi.unstubAllGlobals();
    const cookie = await loginCookie();
    // Only the video stream is real here; the shared /api/ws connection is a fake.
    const real = wsFactory(cookie);
    const fake = fakeFactory().create;
    configureSockets({ base: mockUrl(), create: (url) => (url.endsWith('/api/stream') ? real(url) : fake(url)) });
    const drawImage = vi.fn();
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ drawImage } as never);
    vi.stubGlobal('createImageBitmap', vi.fn(async () => ({ width: 64, height: 48, close() {} })));
    render(wrap(<VideoCanvas scale="fit" />));
    await waitFor(() => expect(drawImage).toHaveBeenCalled(), { timeout: 2000 });
  });
});
