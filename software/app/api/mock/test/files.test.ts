import { createHash } from 'node:crypto';
import { request } from 'node:http';
import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import { buildServer } from '../src/server.js';
import { check, login, userWith } from './helpers.js';

let app: FastifyInstance;
afterEach(async () => { await app?.close(); });

const names = (res: { json(): { entries: { name: string }[] } }) => res.json().entries.map((e) => e.name);
const q = (root: string, path: string) => `root=${root}&path=${encodeURIComponent(path)}`;

function upload(cookie: string, root: string, path: string, body: Buffer, headers: Record<string, string> = {}) {
  return app.inject({
    method: 'PUT', url: `/api/files/upload?${q(root, path)}`,
    headers: { cookie, 'content-type': 'application/octet-stream', ...headers }, payload: body,
  });
}

async function listening(): Promise<number> {
  if (!app.server.listening) await app.listen({ host: '127.0.0.1', port: 0 });
  return (app.server.address() as { port: number }).port;
}

describe('storage and listing', () => {
  it('reports storage roots and lists the SD images folder', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const st = await app.inject({ method: 'GET', url: '/api/storage', headers: { cookie } });
    check('getStorage', st);
    const sd = st.json().find((s: { root: string }) => s.root === 'sd');
    expect(sd).toEqual({ root: 'sd', total: 58e9, free: 58e9 - 23.4e9, mounted: true });

    const ls = await app.inject({ method: 'GET', url: `/api/files?${q('sd', '/images')}`, headers: { cookie } });
    expect(ls.statusCode).toBe(200);
    check('listFiles', ls);
    expect(names(ls)).toEqual(expect.arrayContaining(['drivers', 'ubuntu-24.04.iso', 'win11-24h2.iso', 'bios-F12a.zip', 'notes.txt']));
  });

  it('blocks parent-directory escapes on sd and share, allows absolute system paths for admin', async () => {
    app = await buildServer();
    const cookie = await login(app);
    for (const [root, path] of [['sd', '/images/../../etc'], ['sd', '../x'], ['share', '/a/../b'], ['sd', 'images']]) {
      const res = await app.inject({ method: 'GET', url: `/api/files?${q(root, path)}`, headers: { cookie } });
      expect(res.statusCode, `${root}:${path}`).toBe(400);
      expect(res.json().error).toBe('bad_path');
      check('listFiles', res);
    }
    const sys = await app.inject({ method: 'GET', url: `/api/files?${q('system', '/etc')}`, headers: { cookie } });
    expect(sys.statusCode).toBe(200);
    expect(names(sys)).toContain('hostname');
  });

  it('forbids the system root to non-admins and all roots without files permission', async () => {
    app = await buildServer();
    const user = await userWith(app, ['files']);
    const sys = await app.inject({ method: 'GET', url: `/api/files?${q('system', '/etc')}`, headers: { cookie: user } });
    expect(sys.statusCode).toBe(403);
    const sd = await app.inject({ method: 'GET', url: `/api/files?${q('sd', '/')}`, headers: { cookie: user } });
    expect(sd.statusCode).toBe(200);
    const none = await userWith(app, ['console']);
    const denied = await app.inject({ method: 'GET', url: `/api/files?${q('sd', '/')}`, headers: { cookie: none } });
    expect(denied.statusCode).toBe(403);
  });

  it('returns 404 for a missing folder', async () => {
    app = await buildServer();
    const res = await app.inject({ method: 'GET', url: `/api/files?${q('sd', '/nope')}`, headers: { cookie: await login(app) } });
    expect(res.statusCode).toBe(404);
    expect(res.json()).toEqual({ error: 'not_found' });
  });
});

describe('upload, download and file operations', () => {
  it('round-trips an upload and verifies a matching checksum', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const body = Buffer.from('hello kvmpanel\n'.repeat(1000));
    const sha = createHash('sha256').update(body).digest('hex');
    const up = await upload(cookie, 'sd', '/images/hello.txt', body, { 'x-sha256': sha });
    expect(up.statusCode).toBe(201);
    check('uploadFile', up);
    const ls = await app.inject({ method: 'GET', url: `/api/files?${q('sd', '/images')}`, headers: { cookie } });
    const entry = ls.json().entries.find((e: { name: string }) => e.name === 'hello.txt');
    expect(entry).toMatchObject({ type: 'file', size: body.length });
    const down = await app.inject({ method: 'GET', url: `/api/files/download?${q('sd', '/images/hello.txt')}`, headers: { cookie } });
    expect(down.statusCode).toBe(200);
    expect(down.rawPayload.equals(body)).toBe(true);
  });

  it('rejects a checksum mismatch and keeps nothing', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const up = await upload(cookie, 'sd', '/images/bad.bin', Buffer.from('abc'), { 'x-sha256': '0'.repeat(64) });
    expect(up.statusCode).toBe(422);
    expect(up.json().error).toBe('checksum_mismatch');
    check('uploadFile', up);
    const ls = await app.inject({ method: 'GET', url: `/api/files?${q('sd', '/images')}`, headers: { cookie } });
    expect(names(ls)).not.toContain('bad.bin');
  });

  it('answers 507 no_space when the declared size exceeds free space', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const port = await listening();
    const status = await new Promise<{ code: number; body: string }>((resolve, reject) => {
      const req = request({
        host: '127.0.0.1', port, method: 'PUT', path: `/api/files/upload?${q('sd', '/images/huge.img')}`,
        headers: { cookie, 'content-type': 'application/octet-stream', 'content-length': String(40e9) },
      }, (res) => {
        let body = '';
        res.on('data', (c) => { body += c; });
        res.on('end', () => resolve({ code: res.statusCode!, body }));
      });
      req.on('error', reject);
      req.write(Buffer.alloc(1024));
    });
    expect(status.code).toBe(507);
    expect(JSON.parse(status.body)).toEqual({ error: 'no_space' });
  });

  it('drops a half-sent upload when the client disconnects', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const port = await listening();
    await new Promise<void>((resolve) => {
      const req = request({
        host: '127.0.0.1', port, method: 'PUT', path: `/api/files/upload?${q('sd', '/images/partial.bin')}`,
        headers: { cookie, 'content-type': 'application/octet-stream', 'content-length': '200000' },
      });
      req.on('error', () => resolve());
      req.write(Buffer.alloc(100000), () => setTimeout(() => { req.destroy(); resolve(); }, 50));
    });
    await new Promise((r) => setTimeout(r, 200));
    const ls = await app.inject({ method: 'GET', url: `/api/files?${q('sd', '/images')}`, headers: { cookie } });
    expect(names(ls)).not.toContain('partial.bin');
  });

  it('creates, renames and deletes, and refuses writes to an exported share', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const op = (name: string, payload: object) => app.inject({ method: 'POST', url: `/api/files/${name}`, headers: { cookie }, payload });
    const mk = await op('mkdir', { root: 'sd', path: '/isos' });
    expect(mk.statusCode).toBe(201);
    check('makeDir', mk);
    expect((await op('mkdir', { root: 'sd', path: '/isos' })).statusCode).toBe(409);
    const mv = await op('rename', { root: 'sd', path: '/isos', to: '/iso-store' });
    expect(mv.statusCode).toBe(204);
    check('renamePath', mv);
    const rm = await op('delete', { root: 'sd', path: '/iso-store' });
    expect(rm.statusCode).toBe(204);
    check('deletePath', rm);
    expect((await op('delete', { root: 'sd', path: '/iso-store' })).statusCode).toBe(404);

    const share = await app.inject({ method: 'POST', url: '/api/share', headers: { cookie }, payload: { exported_to_host: true } });
    expect(share.statusCode).toBe(200);
    check('setShare', share);
    const busy = await upload(cookie, 'share', '/note.txt', Buffer.from('x'));
    expect(busy.statusCode).toBe(409);
    expect(busy.json().error).toBe('share_busy');
    expect((await op('mkdir', { root: 'share', path: '/d' })).json().error).toBe('share_busy');
    const get = await app.inject({ method: 'GET', url: '/api/share', headers: { cookie } });
    check('getShare', get);
    expect(get.json()).toEqual({ exported_to_host: true });
  });
});
