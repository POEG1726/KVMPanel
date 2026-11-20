import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { closePolicy, configureSockets, connectSocket, type ClosePolicy, type WebSocketLike } from './socket';

class FakeSocket implements WebSocketLike {
  static all: FakeSocket[] = [];
  readyState = 0;
  binaryType = 'arraybuffer';
  sent: unknown[] = [];
  onopen: ((e: unknown) => void) | null = null;
  onmessage: ((e: { data: string | ArrayBuffer }) => void) | null = null;
  onclose: ((e: { code: number }) => void) | null = null;
  constructor(public url: string) {
    FakeSocket.all.push(this);
  }
  send(d: unknown) {
    this.sent.push(d);
  }
  close() {
    this.readyState = 3;
  }
  open() {
    this.readyState = 1;
    this.onopen?.({});
  }
  drop(code: number) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
}

describe('closePolicy', () => {
  it.each([
    [4401, 'login'],
    [4403, 'forbidden'],
    [4404, 'no_serial'],
    [4409, 'disconnected'],
    [1012, 'restarting'],
    [1006, 'retry'],
    [1000, 'retry'],
  ] as const)('maps %i to %s', (code, policy) => {
    expect(closePolicy(code)).toBe(policy);
  });
});

describe('connectSocket', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    FakeSocket.all = [];
    configureSockets({ base: 'http://kvm.test', create: (url) => new FakeSocket(url) });
  });
  afterEach(() => {
    vi.useRealTimers();
    configureSockets({});
  });

  it('derives the ws URL from the base and path', () => {
    connectSocket('/api/ws', { onMessage() {} });
    expect(FakeSocket.all[0].url).toBe('ws://kvm.test/api/ws');
  });

  it('derives the ws URL from location when no base is set', () => {
    configureSockets({ create: (url) => new FakeSocket(url) });
    connectSocket('/api/ws', { onMessage() {} });
    expect(FakeSocket.all[0].url).toBe(`ws://${location.host}/api/ws`);
  });

  it.each([4401, 4403, 4404, 4409])('does not reconnect after %i', (code) => {
    const policies: ClosePolicy[] = [];
    connectSocket('/api/ws', { onMessage() {}, onClosed: (p) => policies.push(p) });
    FakeSocket.all[0].open();
    FakeSocket.all[0].drop(code);
    vi.advanceTimersByTime(10_000);
    expect(FakeSocket.all).toHaveLength(1);
    expect(policies).toEqual([closePolicy(code)]);
  });

  it('reconnects after 1012 with 1, 2, 4, 8 then 10 second backoff', () => {
    connectSocket('/api/ws', { onMessage() {} });
    const delays = [1000, 2000, 4000, 8000, 10_000, 10_000];
    delays.forEach((d, i) => {
      FakeSocket.all[i].drop(i === 0 ? 1012 : 1006);
      vi.advanceTimersByTime(d - 1);
      expect(FakeSocket.all).toHaveLength(i + 1);
      vi.advanceTimersByTime(1);
      expect(FakeSocket.all).toHaveLength(i + 2);
    });
  });

  it('resets the backoff once a connection stayed open for 10 seconds', () => {
    connectSocket('/api/ws', { onMessage() {} });
    FakeSocket.all[0].drop(1006);
    vi.advanceTimersByTime(1000);
    FakeSocket.all[1].drop(1006);
    vi.advanceTimersByTime(2000);
    FakeSocket.all[2].open();
    vi.advanceTimersByTime(10_000);
    FakeSocket.all[2].drop(1006);
    vi.advanceTimersByTime(1000);
    expect(FakeSocket.all).toHaveLength(4);
  });

  it('keeps the backoff when a connection drops before 10 seconds', () => {
    connectSocket('/api/ws', { onMessage() {} });
    FakeSocket.all[0].drop(1006);
    vi.advanceTimersByTime(1000);
    FakeSocket.all[1].open();
    vi.advanceTimersByTime(5000);
    FakeSocket.all[1].drop(1006);
    vi.advanceTimersByTime(1999);
    expect(FakeSocket.all).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.all).toHaveLength(3);
  });

  it('close() stops reconnecting and cancels a pending retry', () => {
    const s = connectSocket('/api/ws', { onMessage() {} });
    FakeSocket.all[0].drop(1006);
    s.close();
    vi.advanceTimersByTime(20_000);
    expect(FakeSocket.all).toHaveLength(1);
  });

  it('does not reconnect when reconnect is false', () => {
    connectSocket('/api/ws', { onMessage() {} }, { reconnect: false });
    FakeSocket.all[0].drop(1006);
    vi.advanceTimersByTime(20_000);
    expect(FakeSocket.all).toHaveLength(1);
  });

  it('treats a trailing base slash as an origin and drops callbacks after closing while connecting', () => {
    configureSockets({ base: 'https://kvm.test/', create: url => new FakeSocket(url) });
    const onOpen = vi.fn();
    const onMessage = vi.fn();
    const handle = connectSocket('/api/ws', { onOpen, onMessage });
    const sock = FakeSocket.all[0];
    expect(sock.url).toBe('wss://kvm.test/api/ws');
    const lateOpen = sock.onopen;
    handle.close();
    sock.open();
    expect(sock.readyState).toBe(3);
    lateOpen?.({});
    sock.onmessage?.({ data: 'stale frame' });
    expect(onOpen).not.toHaveBeenCalled();
    expect(onMessage).not.toHaveBeenCalled();
  });

  it.each([[1011, 'device_unavailable'], [4400, 'bad_kind']] as const)('stops automatic terminal retries on %i', (code, policy) => {
    const onClosed = vi.fn();
    connectSocket('/api/terminal?kind=local', { onClosed, onMessage() {} });
    FakeSocket.all[0].drop(code);
    expect(onClosed).toHaveBeenCalledWith(policy);
    vi.advanceTimersByTime(10000);
    expect(FakeSocket.all).toHaveLength(1);
  });

  it('a consumer can close the handle inside its disconnect callback', () => {
    const handle = connectSocket('/api/ws', { onMessage() {}, onClosed() { handle.close(); } });
    FakeSocket.all[0].drop(1012);
    vi.advanceTimersByTime(2000);
    expect(FakeSocket.all).toHaveLength(1);
  });

  it('forwards messages and sends only while open', () => {
    const got: unknown[] = [];
    const s = connectSocket('/api/ws', { onMessage: (d) => got.push(d) });
    s.send('early');
    const f = FakeSocket.all[0];
    f.open();
    f.onmessage?.({ data: 'hello' });
    s.send('x');
    expect(got).toEqual(['hello']);
    expect(f.sent).toEqual(['x']);
  });
});
