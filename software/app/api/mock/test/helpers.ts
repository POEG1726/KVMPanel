import { expect } from 'vitest';
import type { FastifyInstance, LightMyRequestResponse } from 'fastify';
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
