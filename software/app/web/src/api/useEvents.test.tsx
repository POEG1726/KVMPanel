import { afterEach, describe, expect, it } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { configureSockets } from './socket';
import { useEvents } from './useEvents';
import { loginAs, loginCookie, mockUrl, startMock, wsFactory } from '../../test/mock';

afterEach(() => configureSockets({}));

describe('useEvents', () => {
  it('receives a status within a second of connecting', async () => {
    const cookie = await loginCookie();
    configureSockets({ base: mockUrl(), create: wsFactory(cookie) });
    const { result, unmount } = renderHook(() => useEvents());
    await waitFor(() => expect(result.current.status).not.toBeNull(), { timeout: 1000 });
    expect(result.current.connected).toBe(true);
    expect(result.current.banner).toBeNull();
    expect(result.current.status?.t).toBe('status');
    unmount();
  });

  it('reports login policy when unauthenticated and does not retry', async () => {
    let attempts = 0;
    const create = wsFactory('');
    configureSockets({ base: mockUrl(), create: url => { attempts++; return create(url); } });
    const { result, unmount } = renderHook(() => useEvents());
    await waitFor(() => expect(result.current.banner).toBe('login'), { timeout: 2000 });
    expect(result.current.connected).toBe(false);
    await new Promise(resolve => setTimeout(resolve, 1100));
    expect(attempts).toBe(1);
    unmount();
  });

  it('shows disconnected after the admin disconnects all sessions', async () => {
    const mock = await startMock();
    try {
      const cookie = await loginAs(mock.url, 'admin', 'admin');
      configureSockets({ base: mock.url, create: wsFactory(cookie) });
      const { result, unmount } = renderHook(() => useEvents());
      await waitFor(() => expect(result.current.connected).toBe(true));
      const r = await fetch(mock.url + '/api/device/action', {
        method: 'POST',
        headers: { 'content-type': 'application/json', cookie },
        body: JSON.stringify({ action: 'disconnect_sessions' }),
      });
      expect(r.status).toBe(202);
      await waitFor(() => expect(result.current.banner).toBe('disconnected'), { timeout: 2000 });
      expect(result.current.connected).toBe(false);
      unmount();
    } finally {
      await mock.close();
    }
  });
});
