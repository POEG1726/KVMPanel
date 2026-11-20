import { describe, expect, it } from 'vitest';
import { buildServer } from '../src/server.js';
import { check, login, userWith } from './helpers.js';

describe('power', () => {
  it('toggles on a short press and forces off on a long press', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const get = () => app.inject({ method: 'GET', url: '/api/power', headers: { cookie } });
    const first = await get();
    check('getPower', first);
    expect(first.json()).toEqual({ on: true, hdd: false });

    const press = (button: string, duration_ms: number) =>
      app.inject({ method: 'POST', url: '/api/power/press', headers: { cookie }, payload: { button, duration_ms } });
    const short = await press('power', 300);
    expect(short.statusCode).toBe(204);
    check('pressPower', short);
    expect((await get()).json().on).toBe(false);
    await press('power', 300);
    expect((await get()).json().on).toBe(true);
    await press('reset', 300);
    expect((await get()).json().on).toBe(true);
    await press('power', 5000);
    expect((await get()).json().on).toBe(false);
    await press('power', 5000);
    expect((await get()).json().on).toBe(false);
  });

  it('rejects an out-of-range duration and a user without power', async () => {
    const app = await buildServer();
    const bad = await app.inject({
      method: 'POST', url: '/api/power/press', headers: { cookie: await login(app) },
      payload: { button: 'power', duration_ms: 50 },
    });
    expect(bad.statusCode).toBe(400);
    check('pressPower', bad);
    const denied = await app.inject({
      method: 'POST', url: '/api/power/press', headers: { cookie: await userWith(app, ['console']) },
      payload: { button: 'power', duration_ms: 300 },
    });
    expect(denied.statusCode).toBe(403);
  });
});

describe('wake-on-lan', () => {
  it('adds, lists, wakes and deletes devices', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const bad = await app.inject({ method: 'POST', url: '/api/wol/devices', headers: { cookie }, payload: { name: 'x', mac: 'zz' } });
    expect(bad.statusCode).toBe(400);
    expect(bad.json().error).toBe('bad_mac');
    check('addWolDevice', bad);

    const add = await app.inject({
      method: 'POST', url: '/api/wol/devices', headers: { cookie }, payload: { name: '本机', mac: 'A8-A1-59-12-34-3C' },
    });
    expect(add.statusCode).toBe(201);
    check('addWolDevice', add);
    expect(add.json()).toEqual({ name: '本机', mac: 'a8:a1:59:12:34:3c' });
    const dup = await app.inject({
      method: 'POST', url: '/api/wol/devices', headers: { cookie }, payload: { name: 'again', mac: 'a8:a1:59:12:34:3c' },
    });
    expect(dup.statusCode).toBe(409);

    const list = await app.inject({ method: 'GET', url: '/api/wol/devices', headers: { cookie } });
    check('listWolDevices', list);
    expect(list.json()).toEqual({ devices: [{ name: '本机', mac: 'a8:a1:59:12:34:3c' }] });

    const wake = await app.inject({ method: 'POST', url: '/api/wol/wake', headers: { cookie }, payload: { mac: 'a8:a1:59:12:34:3c' } });
    expect(wake.statusCode).toBe(204);
    check('wakeOnLan', wake);
    const wakeBad = await app.inject({ method: 'POST', url: '/api/wol/wake', headers: { cookie }, payload: { mac: 'nope' } });
    expect(wakeBad.statusCode).toBe(400);

    const del = await app.inject({ method: 'DELETE', url: '/api/wol/devices/a8:a1:59:12:34:3c', headers: { cookie } });
    expect(del.statusCode).toBe(204);
    const again = await app.inject({ method: 'DELETE', url: '/api/wol/devices/a8:a1:59:12:34:3c', headers: { cookie } });
    expect(again.statusCode).toBe(404);
  });
});
