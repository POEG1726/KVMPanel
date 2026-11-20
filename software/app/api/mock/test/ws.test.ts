import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import { buildServer } from '../src/server.js';
import { messageValidator } from '../src/contract.js';
import { closeCodeOf, inbox, login, userWith } from './helpers.js';

const validServer = messageValidator('ws.schema.json', 'ServerMessage');

let app: FastifyInstance;
afterEach(async () => { await app?.close(); });

describe('/api/ws', () => {
  it('pushes a contract-valid status right after connecting', async () => {
    app = await buildServer();
    const ws = await app.injectWS('/api/ws', { headers: { cookie: await login(app) } });
    const box = inbox(ws);
    const status = await box.next((m) => m.t === 'status', 1000);
    expect(validServer(status)).toBe(true);
    expect(status.sessions[0]).toMatchObject({ user: 'admin', view_only: false });
    ws.terminate();
  });

  it('toggles caps lock and reports it in the next status', async () => {
    app = await buildServer();
    const ws = await app.injectWS('/api/ws', { headers: { cookie: await login(app) } });
    const box = inbox(ws);
    await box.next((m) => m.t === 'status');
    ws.send(JSON.stringify({ t: 'key', code: 'CapsLock', down: true }));
    const s = await box.next((m) => m.t === 'status' && m.locks.caps === true);
    expect(s.locks).toEqual({ num: false, caps: true, scroll: false });
    ws.terminate();
  });

  it('answers malformed messages with bad_message and keeps the connection', async () => {
    app = await buildServer();
    const ws = await app.injectWS('/api/ws', { headers: { cookie: await login(app) } });
    const box = inbox(ws);
    ws.send(JSON.stringify({ t: 'key' }));
    expect(await box.next((m) => m.t === 'error')).toEqual({ t: 'error', error: 'bad_message' });
    ws.send('not json');
    expect(await box.next((m) => m.t === 'error')).toEqual({ t: 'error', error: 'bad_message' });
    ws.send(JSON.stringify({ t: 'mouse_abs', x: 70000, y: 0, buttons: 0 }));
    expect(await box.next((m) => m.t === 'error')).toEqual({ t: 'error', error: 'bad_message' });
    ws.send(JSON.stringify({ t: 'ping' }));
    expect(await box.next((m) => m.t === 'pong')).toEqual({ t: 'pong' });
    ws.terminate();
  });

  it('refuses input from a view-only session', async () => {
    app = await buildServer();
    const cookie = await userWith(app, ['console'], { view_only: true });
    const ws = await app.injectWS('/api/ws', { headers: { cookie } });
    const box = inbox(ws);
    const status = await box.next((m) => m.t === 'status');
    expect(status.sessions.some((s: { view_only: boolean }) => s.view_only)).toBe(true);
    ws.send(JSON.stringify({ t: 'key', code: 'KeyA', down: true }));
    expect(await box.next((m) => m.t === 'error')).toEqual({ t: 'error', error: 'forbidden' });
    ws.terminate();
  });

  it('closes anonymous connections with 4401 and users without console with 4403', async () => {
    app = await buildServer();
    expect(await closeCodeOf(app, '/api/ws')).toBe(4401);
    expect(await closeCodeOf(app, '/api/ws', await userWith(app, ['panel']))).toBe(4403);
  });

  it('forwards usb_reenumerated after an apply', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const ws = await app.injectWS('/api/ws', { headers: { cookie } });
    const box = inbox(ws);
    await box.next((m) => m.t === 'status');
    await app.inject({ method: 'POST', url: '/api/usb/apply', headers: { cookie } });
    expect(await box.next((m) => m.t === 'usb_reenumerated')).toEqual({ t: 'usb_reenumerated' });
    ws.terminate();
  });
});
