import { expect } from 'vitest';
import type { FastifyInstance, LightMyRequestResponse } from 'fastify';
import WS from 'ws';
import type WebSocket from 'ws';
import { validateResponse } from '../src/contract.js';

export function cookieOf(res: LightMyRequestResponse): string {
  const c = res.cookies.find((x) => x.name === 'kvm_session');
  if (!c) throw new Error('no session cookie');
  return `kvm_session=${c.value}`;
}

export async function login(app: FastifyInstance, username = 'admin', password = 'admin'): Promise<string> {
  const res = await app.inject({ method: 'POST', url: '/api/auth/login', payload: { username, password } });
  expect(res.statusCode).toBe(200);
  return cookieOf(res);
}

/** Creates a `user`-role account with the given permissions and returns its session cookie. */
export async function userWith(app: FastifyInstance, permissions: string[], opts: { view_only?: boolean; name?: string } = {}): Promise<string> {
  const admin = await login(app);
  const name = opts.name ?? `u${Math.random().toString(36).slice(2, 8)}`;
  const res = await app.inject({
    method: 'POST', url: '/api/users', headers: { cookie: admin },
    payload: { name, password: 'pw123456', role: 'user', permissions, view_only: opts.view_only ?? false },
  });
  expect(res.statusCode).toBe(201);
  return login(app, name, 'pw123456');
}

export function check(op: string, res: LightMyRequestResponse): void {
  const isJson = (res.headers['content-type'] ?? '').toString().includes('application/json');
  validateResponse(op, res.statusCode, isJson && res.body ? res.json() : undefined);
}

/** Collects JSON messages and lets a test await the next one matching a predicate. */
export function inbox(ws: WebSocket) {
  const queue: any[] = [];
  const waiters: { pred: (m: any) => boolean; resolve: (m: any) => void }[] = [];
  ws.on('message', (data, isBinary) => {
    if (isBinary) return;
    const m = JSON.parse(data.toString());
    const i = waiters.findIndex((w) => w.pred(m));
    if (i >= 0) waiters.splice(i, 1)[0].resolve(m);
    else queue.push(m);
  });
  return {
    next(pred: (m: any) => boolean = () => true, timeoutMs = 2000): Promise<any> {
      const i = queue.findIndex(pred);
      if (i >= 0) return Promise.resolve(queue.splice(i, 1)[0]);
      return new Promise((resolve, reject) => {
        const w = { pred, resolve };
        waiters.push(w);
        setTimeout(() => {
          const j = waiters.indexOf(w);
          if (j >= 0) { waiters.splice(j, 1); reject(new Error('timeout waiting for message')); }
        }, timeoutMs);
      });
    },
  };
}

/**
 * Opens a real WebSocket against a listening copy of the app and resolves with the close code.
 * (injectWS's in-memory transport never completes the close handshake.)
 */
export async function closeCodeOf(app: FastifyInstance, path: string, cookie?: string): Promise<number> {
  if (!app.server.listening) await app.listen({ host: '127.0.0.1', port: 0 });
  const { port } = app.server.address() as { port: number };
  const ws = new WS(`ws://127.0.0.1:${port}${path}`, cookie ? { headers: { cookie } } : {});
  return new Promise((resolve, reject) => {
    ws.on('close', (code) => resolve(code));
    ws.on('error', reject);
  });
}
