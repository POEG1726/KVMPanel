import { randomBytes } from 'node:crypto';
import type { FastifyInstance, FastifyReply, FastifyRequest, preHandlerHookHandler } from 'fastify';
import { checkRequest } from '../contract.js';
import {
  ALL_PERMISSIONS, hashPassword, passwordMatches,
  type Account, type Permission, type State,
} from '../state.js';

export const COOKIE = 'kvm_session';
const DELAY_FROM_FAILURE = 3;
const LOCK_AFTER_FAILURES = 5;
const DELAY_MS = 1000;
const LOCK_S = 300;

declare module 'fastify' {
  interface FastifyRequest {
    account?: Account;
  }
}

export function publicUser(a: Account) {
  return {
    name: a.name,
    role: a.role,
    permissions: a.role === 'admin' ? [...ALL_PERMISSIONS] : [...a.permissions],
    view_only: a.view_only,
    disabled: a.disabled,
    must_change_password: a.must_change_password,
  };
}

export function sendError(reply: FastifyReply, status: number, error: string, detail?: Record<string, unknown>) {
  return reply.code(status).send(detail ? { error, detail } : { error });
}

/** Resolves the account behind a session token, or undefined if the session is dead. */
export function accountForToken(state: State, token: string | undefined): Account | undefined {
  if (!token) return undefined;
  const s = state.sessions.get(token);
  if (!s) return undefined;
  const acc = state.accounts.get(s.user);
  if (!acc || acc.disabled || acc.token_version !== s.token_version) return undefined;
  return acc;
}

export function hasPerm(acc: Account, perm: Permission): boolean {
  return acc.role === 'admin' || acc.permissions.includes(perm);
}

function authenticate(state: State, req: FastifyRequest): Account | undefined {
  const acc = accountForToken(state, req.cookies[COOKIE]);
  if (acc) req.account = acc;
  return acc;
}

/** preHandler: requires a live session, and `perm` when given ('admin' requires the admin role). */
export function requirePerm(state: State, perm?: Permission | 'admin'): preHandlerHookHandler {
  return async (req, reply) => {
    const acc = authenticate(state, req);
    if (!acc) return sendError(reply, 401, 'unauthenticated');
    if (perm === 'admin' ? acc.role !== 'admin' : perm && !hasPerm(acc, perm)) {
      return sendError(reply, 403, 'forbidden');
    }
  };
}

/** Rejects the body with 400 invalid_body when it violates the contract of `operationId`. */
export function bodyGuard(operationId: string): preHandlerHookHandler {
  return async (req, reply) => {
    const bad = checkRequest(operationId, req.body);
    if (bad) return sendError(reply, 400, 'invalid_body', { path: bad.path });
  };
}

function issueSession(state: State, req: FastifyRequest, reply: FastifyReply, acc: Account) {
  const token = randomBytes(24).toString('hex');
  state.sessions.set(token, { token, user: acc.name, token_version: acc.token_version, ip: req.ip, since: state.now() });
  reply.setCookie(COOKIE, token, { path: '/', httpOnly: true, sameSite: 'strict' });
}

function endSessionsOf(acc: Account) {
  acc.token_version += 1;
}

export function registerAuth(app: FastifyInstance, state: State) {
  app.post('/api/auth/login', { preHandler: bodyGuard('login') }, async (req, reply) => {
    const { username, password } = req.body as { username: string; password: string };
    const guard = state.loginGuards.get(username) ?? { failures: 0, locked_until: 0 };
    const now = state.now();
    if (guard.locked_until > now) {
      return sendError(reply, 429, 'login_locked', { retry_after_s: Math.ceil((guard.locked_until - now) / 1000) });
    }
    if (guard.failures >= LOCK_AFTER_FAILURES) {
      guard.locked_until = now + LOCK_S * 1000;
      guard.failures = 0;
      state.loginGuards.set(username, guard);
      return sendError(reply, 429, 'login_locked', { retry_after_s: LOCK_S });
    }
    const acc = state.accounts.get(username);
    if (!acc || !passwordMatches(acc, password)) {
      guard.failures += 1;
      state.loginGuards.set(username, guard);
      if (guard.failures >= DELAY_FROM_FAILURE) await state.sleep(DELAY_MS);
      return sendError(reply, 401, 'invalid_credentials');
    }
    state.loginGuards.delete(username);
    if (acc.disabled) return sendError(reply, 403, 'user_disabled');
    issueSession(state, req, reply, acc);
    return { user: publicUser(acc) };
  });

  app.post('/api/auth/logout', async (req, reply) => {
    const token = req.cookies[COOKIE];
    if (token) state.sessions.delete(token);
    reply.clearCookie(COOKIE, { path: '/' });
    return reply.code(204).send();
  });

  app.get('/api/auth/me', { preHandler: requirePerm(state) }, async (req) => ({ user: publicUser(req.account!) }));

  app.post('/api/users/me/password', { preHandler: [requirePerm(state), bodyGuard('changePassword')] }, async (req, reply) => {
    const { old, new: next } = req.body as { old: string; new: string };
    const acc = req.account!;
    if (!passwordMatches(acc, old)) return sendError(reply, 403, 'wrong_password');
    Object.assign(acc, hashPassword(next));
    acc.must_change_password = false;
    endSessionsOf(acc);
    issueSession(state, req, reply, acc);
    return reply.code(204).send();
  });

  app.get('/api/users', { preHandler: requirePerm(state, 'admin') }, async () => ({
    users: [...state.accounts.values()].map(publicUser),
  }));

  app.post('/api/users', { preHandler: [requirePerm(state, 'admin'), bodyGuard('createUser')] }, async (req, reply) => {
    const b = req.body as { name: string; password: string; role: Account['role']; permissions?: Permission[]; view_only?: boolean };
    if (state.accounts.has(b.name)) return sendError(reply, 409, 'user_exists');
    const acc: Account = {
      name: b.name, role: b.role, permissions: b.permissions ?? [], view_only: b.view_only ?? false,
      disabled: false, must_change_password: false, token_version: 0, ...hashPassword(b.password),
    };
    state.accounts.set(acc.name, acc);
    return reply.code(201).send(publicUser(acc));
  });

  app.patch('/api/users/:name', { preHandler: [requirePerm(state, 'admin'), bodyGuard('updateUser')] }, async (req, reply) => {
    const acc = state.accounts.get((req.params as { name: string }).name);
    if (!acc) return sendError(reply, 404, 'not_found');
    const b = req.body as Partial<Pick<Account, 'role' | 'permissions' | 'view_only' | 'disabled'>> & { password?: string };
    if (b.role) acc.role = b.role;
    if (b.permissions) acc.permissions = b.permissions;
    if (b.view_only !== undefined) acc.view_only = b.view_only;
    if (b.disabled !== undefined) {
      acc.disabled = b.disabled;
      if (b.disabled) endSessionsOf(acc);
    }
    if (b.password) {
      Object.assign(acc, hashPassword(b.password));
      endSessionsOf(acc);
    }
    return publicUser(acc);
  });

  app.delete('/api/users/:name', { preHandler: requirePerm(state, 'admin') }, async (req, reply) => {
    const name = (req.params as { name: string }).name;
    if (name === req.account!.name) return sendError(reply, 409, 'cannot_delete_self');
    const acc = state.accounts.get(name);
    if (!acc) return sendError(reply, 404, 'not_found');
    endSessionsOf(acc);
    state.accounts.delete(name);
    return reply.code(204).send();
  });
}
