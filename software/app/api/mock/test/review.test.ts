/** Regression tests for the final-review findings (I1–I8 plus two re-graded minors). */
import { request } from 'node:http';
import { connect } from 'node:net';
import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import type { Server } from 'node:net';
import WS from 'ws';
import { buildServer } from '../src/server.js';
import { startPanelServer } from '../src/panel/server.js';
import { snapshot } from '../src/panel/metrics.js';
import { check, login, userWith } from './helpers.js';

let app: FastifyInstance;
let panel: Server | undefined;
afterEach(async () => {
  if (panel) await new Promise<void>((r) => panel!.close(() => r()));
  panel = undefined;
  await app?.close();
});

const patch = (cookie: string, payload: object) => app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload });
const fileOp = (cookie: string, name: string, payload: object) => app.inject({ method: 'POST', url: `/api/files/${name}`, headers: { cookie }, payload });
const upload = (cookie: string, root: string, path: string, body: Buffer) => app.inject({
  method: 'PUT', url: `/api/files/upload?root=${root}&path=${encodeURIComponent(path)}`,
  headers: { cookie, 'content-type': 'application/octet-stream' }, payload: body,
});

async function port(): Promise<number> {
  if (!app.server.listening) await app.listen({ host: '127.0.0.1', port: 0 });
  return (app.server.address() as { port: number }).port;
}
async function openWs(path: string, cookie: string): Promise<{ ws: WS; closed: Promise<number> }> {
  const ws = new WS(`ws://127.0.0.1:${await port()}${path}`, { headers: { cookie } });
  const closed = new Promise<number>((r) => ws.on('close', (c) => r(c)));
  await new Promise((r) => ws.on('open', r));
  return { ws, closed };
}

describe('I1/I2 thresholds and hidden metrics', () => {
  it('keys thresholds by metric id or id.* pattern and deletes a key with null', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const cfg = (await app.inject({ method: 'GET', url: '/api/config', headers: { cookie } })).json();
    expect(cfg.panel.thresholds).toEqual({ 'cpu.tctl': 85, 'gpu.mem_junction': 95, 'mem.dimm.*': 60 });
    expect(cfg.panel.hidden_metrics).toEqual([]);
    const set = await patch(cookie, { panel: { thresholds: { 'gpu.temp': 80 } } });
    expect(set.statusCode).toBe(200);
    expect(set.json().panel.thresholds['gpu.temp']).toBe(80);
    const del = await patch(cookie, { panel: { thresholds: { 'gpu.temp': null } } });
    expect(del.statusCode).toBe(200);
    expect(del.json().panel.thresholds).not.toHaveProperty('gpu.temp');
    expect((await patch(cookie, { panel: { thresholds: { 'Bad Key': 1 } } })).statusCode).toBe(400);
  });

  it('applies any threshold to the matching metrics', () => {
    const m = snapshot(1, { 'gpu.temp': 70, 'mem.dimm.*': 55, 'cpu.tctl': 85 }, ['cpu.bclk']);
    const by = (id: string) => m.find((x) => x.id === id);
    expect(by('gpu.temp')?.warn).toBe(70);
    expect(by('mem.dimm.b2')?.warn).toBe(55);
    expect(by('cpu.tctl')?.warn).toBe(85);
    expect(by('gpu.power')?.warn).toBeUndefined();
    expect(by('cpu.bclk')).toBeUndefined();
  });
});

describe('I3/I4 file safety', () => {
  it('enforces free space while streaming a chunked upload', async () => {
    app = await buildServer();
    const cookie = await login(app);
    app.kvm.volumes.share.total = 200;
    const p = await port();
    const status = await new Promise<number>((resolve, reject) => {
      const req = request({
        host: '127.0.0.1', port: p, method: 'PUT', path: '/api/files/upload?root=share&path=%2Fbig.bin',
        headers: { cookie, 'content-type': 'application/octet-stream', 'transfer-encoding': 'chunked' },
      }, (res) => { res.resume(); resolve(res.statusCode!); });
      req.on('error', reject);
      req.end(Buffer.alloc(500));
    });
    expect(status).toBe(507);
    const st = (await app.inject({ method: 'GET', url: '/api/storage', headers: { cookie } })).json();
    expect(st.find((v: { root: string }) => v.root === 'share').free).toBeGreaterThanOrEqual(0);
  });

  it('refuses to overwrite a folder, move a folder into itself, or touch the mounted image', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const dir = await upload(cookie, 'sd', '/images', Buffer.from('x'));
    expect(dir.statusCode).toBe(409);
    expect(dir.json().error).toBe('is_dir');
    check('uploadFile', dir);
    const self = await fileOp(cookie, 'rename', { root: 'sd', path: '/backup', to: '/backup/inner' });
    expect(self.statusCode).toBe(400);
    expect(self.json().error).toBe('bad_path');
    await app.inject({ method: 'POST', url: '/api/media/mount', headers: { cookie }, payload: { root: 'sd', path: '/images/ubuntu-24.04.iso', mode: 'cdrom' } });
    for (const res of [
      await fileOp(cookie, 'delete', { root: 'sd', path: '/images' }),
      await fileOp(cookie, 'rename', { root: 'sd', path: '/images', to: '/imgs' }),
      await upload(cookie, 'sd', '/images/ubuntu-24.04.iso', Buffer.from('x')),
    ]) {
      expect(res.statusCode).toBe(409);
      expect(res.json().error).toBe('in_use');
    }
  });
});

describe('I5/I6 socket lifetime', () => {
  it('closes a user\'s sockets with 4401 when their sessions end', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const a = await openWs('/api/ws', cookie);
    const b = await openWs('/api/stream', cookie);
    const c = await openWs('/api/terminal?kind=local', cookie);
    await app.inject({ method: 'POST', url: '/api/users/me/password', headers: { cookie }, payload: { old: 'admin', new: 'n3w-pass' } });
    expect(await a.closed).toBe(4401);
    expect(await b.closed).toBe(4401);
    expect(await c.closed).toBe(4401);
  });

  it('closes sockets with 4403 when the permission is revoked', async () => {
    app = await buildServer();
    const admin = await login(app);
    const user = await userWith(app, ['console', 'terminal'], { name: 'tom' });
    const term = await openWs('/api/terminal?kind=local', user);
    const con = await openWs('/api/ws', user);
    await app.inject({ method: 'PATCH', url: '/api/users/tom', headers: { cookie: admin }, payload: { permissions: ['console'] } });
    expect(await term.closed).toBe(4403);
    expect(con.ws.readyState).toBe(WS.OPEN);
    con.ws.terminate();
  });

  it('disconnect_sessions closes console sockets with 4409 and does not announce a restart', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const { ws, closed } = await openWs('/api/ws', cookie);
    const got: string[] = [];
    ws.on('message', (d) => got.push(JSON.parse(d.toString()).t));
    await app.inject({ method: 'POST', url: '/api/device/action', headers: { cookie }, payload: { action: 'disconnect_sessions' } });
    expect(await closed).toBe(4409);
    expect(got).not.toContain('kvm_restarting');
  });
});

describe('I7 panel restart and id echo', () => {
  it('drops the panel socket after kvm_restarting and echoes ids on schema errors', async () => {
    app = await buildServer();
    panel = await startPanelServer(app.kvm, { port: 0 });
    const sock = connect((panel.address() as { port: number }).port, '127.0.0.1');
    const lines: any[] = [];
    let buf = '';
    sock.on('data', (d) => {
      buf += d.toString();
      let nl: number;
      while ((nl = buf.indexOf('\n')) >= 0) { lines.push(JSON.parse(buf.slice(0, nl))); buf = buf.slice(nl + 1); }
    });
    const closed = new Promise<void>((r) => sock.on('close', () => r()));
    await new Promise((r) => sock.on('connect', r));
    sock.write(JSON.stringify({ t: 'nope', id: 5 }) + '\n');
    await new Promise((r) => setTimeout(r, 100));
    expect(lines.find((l) => l.t === 'result')).toEqual({ t: 'result', id: 5, ok: false, error: 'bad_message' });
    sock.write(JSON.stringify({ t: 'action', id: 6, action: 'restart_kvm' }) + '\n');
    await closed;
    expect(lines.map((l) => l.t)).toContain('kvm_restarting');
  });
});

describe('I8 contract coverage', () => {
  it('serves MCP clients with a bearer key and lets admins disable MCP', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const key = (await app.inject({ method: 'POST', url: '/api/mcp/regenerate', headers: { cookie } })).json().key;
    const shot = await app.inject({ method: 'GET', url: '/api/mcp/v1/screenshot', headers: { authorization: `Bearer ${key}` } });
    expect(shot.statusCode).toBe(200);
    expect(shot.headers['content-type']).toBe('image/jpeg');
    const hid = await app.inject({ method: 'POST', url: '/api/mcp/v1/hid', headers: { authorization: `Bearer ${key}` }, payload: { t: 'key', code: 'KeyA', down: true } });
    expect(hid.statusCode).toBe(204);
    check('mcpHid', hid);
    expect((await app.inject({ method: 'GET', url: '/api/mcp/v1/screenshot', headers: { authorization: 'Bearer wrong' } })).statusCode).toBe(401);
    const off = await app.inject({ method: 'PUT', url: '/api/mcp', headers: { cookie }, payload: { enabled: false } });
    expect(off.statusCode).toBe(200);
    check('putMcp', off);
    expect((await app.inject({ method: 'GET', url: '/api/mcp/v1/screenshot', headers: { authorization: `Bearer ${key}` } })).statusCode).toBe(401);
  });

  it('sets the update channel/server and polls an update job', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const set = await app.inject({ method: 'PUT', url: '/api/update/settings', headers: { cookie }, payload: { channel: 'preview', server: 'https://my.example/kvm' } });
    expect(set.statusCode).toBe(200);
    check('putUpdateSettings', set);
    expect((await app.inject({ method: 'GET', url: '/api/update', headers: { cookie } })).json()).toMatchObject({ channel: 'preview', server: 'https://my.example/kvm' });
    const job = (await app.inject({ method: 'POST', url: '/api/update/apply', headers: { cookie } })).json().job;
    const poll = await app.inject({ method: 'GET', url: `/api/update/jobs/${job.id}`, headers: { cookie } });
    expect(poll.statusCode).toBe(200);
    check('getUpdateJob', poll);
    expect((await app.inject({ method: 'GET', url: '/api/update/jobs/nope', headers: { cookie } })).statusCode).toBe(404);
  });

  it('reports the applied USB set and whether an apply is pending', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const get = () => app.inject({ method: 'GET', url: '/api/usb/applied', headers: { cookie } });
    const first = await get();
    check('getUsbApplied', first);
    expect(first.json().pending).toBe(false);
    await patch(cookie, { usb: { mouse_abs: false } });
    expect((await get()).json()).toMatchObject({ pending: true, usb: { mouse_abs: true } });
    await app.inject({ method: 'POST', url: '/api/usb/apply', headers: { cookie } });
    expect((await get()).json()).toMatchObject({ pending: false, usb: { mouse_abs: false } });
  });
});

describe('re-graded minors', () => {
  it('uses the Error schema for malformed JSON and unknown routes', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const bad = await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie, 'content-type': 'application/json' }, payload: '{nope' });
    expect(bad.statusCode).toBe(400);
    expect(bad.json()).toEqual({ error: 'invalid_json' });
    const nf = await app.inject({ method: 'GET', url: '/api/does-not-exist', headers: { cookie } });
    expect(nf.statusCode).toBe(404);
    expect(nf.json()).toEqual({ error: 'not_found' });
  });
});
