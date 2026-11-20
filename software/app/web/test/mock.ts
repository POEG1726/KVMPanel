import WebSocket from 'ws';
import type { SocketFactory } from '../src/api/socket';

export const mockUrl = (): string => process.env.MOCK_URL!;

/** Logs in as admin/admin against the mock and returns the Cookie header value (Node has no cookie jar). */
export async function loginCookie(): Promise<string> {
  const r = await fetch(`${mockUrl()}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username: 'admin', password: 'admin' }),
  });
  if (!r.ok) throw new Error(`login failed: ${r.status}`);
  return r.headers.getSetCookie().map((c) => c.split(';')[0]).join('; ');
}

/** A socket factory for Node tests: real `ws` connection carrying the session cookie. */
export const wsFactory =
  (cookie: string): SocketFactory =>
  (url) =>
    new WebSocket(url, { headers: { cookie } }) as never;
