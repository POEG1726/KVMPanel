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

/** A private mock server (spawned by global-setup), so tests that change passwords or lock accounts cannot disturb other files. */
export async function startMock(): Promise<{ url: string; close(): Promise<void> }> {
  const control = process.env.MOCK_CONTROL_URL!;
  const response = await fetch(control, { method: 'POST' });
  if (!response.ok) throw new Error('Mock setup failed: ' + await response.text());
  const { url } = await response.json() as { url: string };
  return { url, close: async () => void (await fetch(`${control}/?url=${encodeURIComponent(url)}`, { method: 'DELETE' })) };
}

/** Minimal browser cookie jar over Node fetch: remembers Set-Cookie, sends Cookie, retargets same-origin URLs to `base`. */
export function browserFetch(base: string, initialCookie = '') {
  let cookie = initialCookie;
  const fetchFn: typeof fetch = async (input, init) => {
    const req = new Request(input, init);
    const url = new URL(req.url);
    const target = new URL(base);
    url.protocol = target.protocol;
    url.host = target.host;
    const headers = new Headers(req.headers);
    if (cookie) headers.set('cookie', cookie);
    const res = await fetch(url, { method: req.method, headers, body: req.body, duplex: 'half' } as RequestInit);
    for (const c of res.headers.getSetCookie()) {
      const [pair] = c.split(';');
      const [name] = pair.split('=');
      const rest = cookie.split('; ').filter((p) => p && !p.startsWith(`${name}=`));
      cookie = [...rest, pair].join('; ');
    }
    return res;
  };
  return { fetch: fetchFn, cookie: () => cookie, setCookie: (c: string) => (cookie = c) };
}

export async function loginAs(base: string, username: string, password: string): Promise<string> {
  const r = await fetch(`${base}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  if (!r.ok) throw new Error(`login failed: ${r.status}`);
  return r.headers.getSetCookie().map((c) => c.split(';')[0]).join('; ');
}

export async function changePasswordAs(base: string, cookie: string, old: string, next: string): Promise<string> {
  const r = await fetch(`${base}/api/users/me/password`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', cookie },
    body: JSON.stringify({ old, new: next }),
  });
  if (r.status !== 204) throw new Error(`change failed: ${r.status}`);
  return r.headers.getSetCookie().map((c) => c.split(';')[0]).join('; ');
}
