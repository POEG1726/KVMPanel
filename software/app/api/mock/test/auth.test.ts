import { describe, expect, it } from 'vitest';
import { buildServer } from '../src/server.js';
import { check, cookieOf, login } from './helpers.js';

describe('auth', () => {
  it('logs in and returns the admin from /me', async () => {
    const app = await buildServer();
    const res = await app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'admin', password: 'admin' } });
    expect(res.statusCode).toBe(200);
    check('login', res);
    expect(res.json().user.must_change_password).toBe(true);
    const me = await app.inject({ method: 'GET', url: '/api/auth/me', headers: { cookie: cookieOf(res) } });
    expect(me.statusCode).toBe(200);
    check('getMe', me);
    expect(me.json().user).toMatchObject({ name: 'admin', role: 'admin', view_only: false, disabled: false });
  });

  it('rejects a wrong password and an anonymous /me', async () => {
    const app = await buildServer();
    const bad = await app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'admin', password: 'x' } });
    expect(bad.statusCode).toBe(401);
    expect(bad.json().error).toBe('invalid_credentials');
    check('login', bad);
    const me = await app.inject({ method: 'GET', url: '/api/auth/me' });
    expect(me.statusCode).toBe(401);
    expect(me.json().error).toBe('unauthenticated');
    check('getMe', me);
  });

  it('delays from the 3rd failure and locks on the 6th attempt for 300 s', async () => {
    const sleeps: number[] = [];
    const app = await buildServer({ sleep: async (ms) => void sleeps.push(ms) });
    const attempt = () =>
      app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'admin', password: 'wrong' } });
    for (let i = 1; i <= 5; i++) expect((await attempt()).statusCode).toBe(401);
    expect(sleeps).toEqual([1000, 1000, 1000]);
    const locked = await attempt();
    expect(locked.statusCode).toBe(429);
    expect(locked.json()).toEqual({ error: 'login_locked', detail: { retry_after_s: 300 } });
    check('login', locked);
    // the right password is refused while locked
    const right = await app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'admin', password: 'admin' } });
    expect(right.statusCode).toBe(429);
  });

  it('unlocks after 300 s', async () => {
    let t = 1_000_000;
    const app = await buildServer({ now: () => t, sleep: async () => {} });
    for (let i = 0; i < 6; i++)
      await app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'admin', password: 'wrong' } });
    t += 301_000;
    const ok = await app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'admin', password: 'admin' } });
    expect(ok.statusCode).toBe(200);
  });

  it('invalidates old sessions after a password change and logs out', async () => {
    const app = await buildServer();
    const a = await login(app);
    const b = await login(app);
    const chg = await app.inject({
      method: 'POST', url: '/api/users/me/password', headers: { cookie: a },
      payload: { old: 'admin', new: 'n3w-pass' },
    });
    expect(chg.statusCode).toBe(204);
    expect((await app.inject({ method: 'GET', url: '/api/auth/me', headers: { cookie: a } })).statusCode).toBe(401);
    expect((await app.inject({ method: 'GET', url: '/api/auth/me', headers: { cookie: b } })).statusCode).toBe(401);
    // the caller receives a fresh cookie
    const fresh = cookieOf(chg);
    const me = await app.inject({ method: 'GET', url: '/api/auth/me', headers: { cookie: fresh } });
    expect(me.statusCode).toBe(200);
    expect(me.json().user.must_change_password).toBe(false);
    const out = await app.inject({ method: 'POST', url: '/api/auth/logout', headers: { cookie: fresh } });
    expect(out.statusCode).toBe(204);
    expect((await app.inject({ method: 'GET', url: '/api/auth/me', headers: { cookie: fresh } })).statusCode).toBe(401);
  });

  it('rejects a password change with the wrong old password', async () => {
    const app = await buildServer();
    const a = await login(app);
    const res = await app.inject({
      method: 'POST', url: '/api/users/me/password', headers: { cookie: a }, payload: { old: 'nope', new: 'abcdef' },
    });
    expect(res.statusCode).toBe(403);
    expect(res.json().error).toBe('wrong_password');
  });
});

describe('users', () => {
  it('lets admin manage users and forbids the user role', async () => {
    const app = await buildServer();
    const admin = await login(app);
    const created = await app.inject({
      method: 'POST', url: '/api/users', headers: { cookie: admin },
      payload: { name: 'guest', password: 'guestpw', role: 'user', permissions: ['console'], view_only: true },
    });
    expect(created.statusCode).toBe(201);
    check('createUser', created);
    const dup = await app.inject({
      method: 'POST', url: '/api/users', headers: { cookie: admin },
      payload: { name: 'guest', password: 'guestpw', role: 'user' },
    });
    expect(dup.statusCode).toBe(409);
    expect(dup.json().error).toBe('user_exists');

    const list = await app.inject({ method: 'GET', url: '/api/users', headers: { cookie: admin } });
    check('listUsers', list);
    expect(list.json().users.map((u: { name: string }) => u.name)).toEqual(['admin', 'guest']);

    const guest = await login(app, 'guest', 'guestpw');
    const forbidden = await app.inject({ method: 'GET', url: '/api/users', headers: { cookie: guest } });
    expect(forbidden.statusCode).toBe(403);
    expect(forbidden.json().error).toBe('forbidden');
    check('listUsers', forbidden);

    const patched = await app.inject({
      method: 'PATCH', url: '/api/users/guest', headers: { cookie: admin }, payload: { disabled: true },
    });
    expect(patched.statusCode).toBe(200);
    check('updateUser', patched);
    // disabling ends the guest's session immediately
    expect((await app.inject({ method: 'GET', url: '/api/auth/me', headers: { cookie: guest } })).statusCode).toBe(401);

    const self = await app.inject({ method: 'DELETE', url: '/api/users/admin', headers: { cookie: admin } });
    expect(self.statusCode).toBe(409);
    expect(self.json().error).toBe('cannot_delete_self');
    const del = await app.inject({ method: 'DELETE', url: '/api/users/guest', headers: { cookie: admin } });
    expect(del.statusCode).toBe(204);
    const missing = await app.inject({ method: 'DELETE', url: '/api/users/guest', headers: { cookie: admin } });
    expect(missing.statusCode).toBe(404);
  });

  it('refuses login for a disabled user', async () => {
    const app = await buildServer();
    const admin = await login(app);
    await app.inject({ method: 'POST', url: '/api/users', headers: { cookie: admin }, payload: { name: 'bob', password: 'bobbob', role: 'user' } });
    await app.inject({ method: 'PATCH', url: '/api/users/bob', headers: { cookie: admin }, payload: { disabled: true } });
    const res = await app.inject({ method: 'POST', url: '/api/auth/login', payload: { username: 'bob', password: 'bobbob' } });
    expect(res.statusCode).toBe(403);
    expect(res.json().error).toBe('user_disabled');
  });
});
