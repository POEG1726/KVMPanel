import { createHash } from 'node:crypto';
import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import WS from 'ws';
import { buildServer } from '../src/server.js';
import { check, login, userWith } from './helpers.js';

let app: FastifyInstance;
afterEach(async () => { await app?.close(); });

// 1×1 transparent PNG
const PNG = Buffer.from('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360000002000154a24f5d0000000049454e44ae426082', 'hex');

describe('device and actions', () => {
  it('reports device info and validates every GET against the contract', async () => {
    app = await buildServer();
    const cookie = await login(app);
    for (const [op, url] of [
      ['getDevice', '/api/device'], ['getNetwork', '/api/network'], ['getTailscale', '/api/tailscale'],
      ['getUpdate', '/api/update'], ['getMcp', '/api/mcp'],
    ]) {
      const res = await app.inject({ method: 'GET', url, headers: { cookie } });
      expect(res.statusCode, url).toBe(200);
      check(op, res);
    }
    const about = await app.inject({ method: 'GET', url: '/api/about' });
    expect(about.statusCode).toBe(200);
    check('getAbout', about);
    expect(about.json()).toMatchObject({ name: 'KVMPanel', version: '0.1.0' });
    const dev = await app.inject({ method: 'GET', url: '/api/device', headers: { cookie } });
    expect(dev.json()).toMatchObject({ hostname: 'kvmpanel', link_mbps: 1000 });
  });

  it('closes open console sockets with 1012 on restart_kvm', async () => {
    app = await buildServer();
    const cookie = await login(app);
    await app.listen({ host: '127.0.0.1', port: 0 });
    const { port } = app.server.address() as { port: number };
    const ws = new WS(`ws://127.0.0.1:${port}/api/ws`, { headers: { cookie } });
    const got: string[] = [];
    ws.on('message', (d) => got.push(JSON.parse(d.toString()).t));
    const closed = new Promise<number>((resolve) => ws.on('close', (code) => resolve(code)));
    await new Promise((r) => ws.on('open', r));
    const res = await app.inject({ method: 'POST', url: '/api/device/action', headers: { cookie }, payload: { action: 'restart_kvm' } });
    expect(res.statusCode).toBe(202);
    check('deviceAction', res);
    expect(await closed).toBe(1012);
    expect(got).toContain('kvm_restarting');
  });

  it('needs device permission for actions', async () => {
    app = await buildServer();
    const res = await app.inject({
      method: 'POST', url: '/api/device/action', headers: { cookie: await userWith(app, ['console']) }, payload: { action: 'reboot' },
    });
    expect(res.statusCode).toBe(403);
  });
});

describe('network', () => {
  it('rejects a static config without an IP and accepts a complete one', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const cur = (await app.inject({ method: 'GET', url: '/api/network', headers: { cookie } })).json();
    const bad = await app.inject({ method: 'PUT', url: '/api/network', headers: { cookie }, payload: { ...cur, mode: 'static' } });
    expect(bad.statusCode).toBe(400);
    check('putNetwork', bad);
    const good = { ...cur, mode: 'static', ip: '192.168.1.50', mask: '255.255.255.0', gateway: '192.168.1.1' };
    const ok = await app.inject({ method: 'PUT', url: '/api/network', headers: { cookie }, payload: good });
    expect(ok.statusCode).toBe(200);
    check('putNetwork', ok);
    expect(ok.json()).toEqual(good);
  });
});

describe('updates, MCP and branding', () => {
  it('rejects an offline package whose checksum does not match', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const pkg = Buffer.from('kvmpanel-app-0.1.1.tar');
    const up = (sha: string) => app.inject({
      method: 'PUT', url: '/api/update/offline',
      headers: { cookie, 'content-type': 'application/octet-stream', 'x-sha256': sha }, payload: pkg,
    });
    const bad = await up('0'.repeat(64));
    expect(bad.statusCode).toBe(422);
    expect(bad.json().error).toBe('checksum_mismatch');
    check('uploadUpdate', bad);
    const ok = await up(createHash('sha256').update(pkg).digest('hex'));
    expect(ok.statusCode).toBe(202);
    check('uploadUpdate', ok);
    const check1 = await app.inject({ method: 'POST', url: '/api/update/check', headers: { cookie } });
    check('checkUpdate', check1);
    const apply = await app.inject({ method: 'POST', url: '/api/update/apply', headers: { cookie } });
    expect(apply.statusCode).toBe(202);
    check('applyUpdate', apply);
  });

  it('shows only the last 4 characters of the MCP key after creating', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const regen = await app.inject({ method: 'POST', url: '/api/mcp/keys', headers: { cookie }, payload: { name: 'Studio', scopes: ['observe'] } });
    check('createMcpKey', regen);
    const key: string = regen.json().key;
    expect(key.length).toBeGreaterThanOrEqual(32);
    const mcp = await app.inject({ method: 'GET', url: '/api/mcp', headers: { cookie } });
    expect(mcp.json().keys[0].hint).toBe(key.slice(-4));
    expect(JSON.stringify(mcp.json())).not.toContain(key);
  });

  it('limits the logo to 256 KiB and serves it without login', async () => {
    app = await buildServer();
    const cookie = await login(app);
    expect((await app.inject({ method: 'GET', url: '/api/branding/logo' })).statusCode).toBe(404);
    const big = await app.inject({
      method: 'PUT', url: '/api/branding/logo', headers: { cookie, 'content-type': 'image/png' }, payload: Buffer.alloc(300 * 1024),
    });
    expect(big.statusCode).toBe(413);
    expect(big.json().error).toBe('too_large');
    check('putLogo', big);
    const put = await app.inject({ method: 'PUT', url: '/api/branding/logo', headers: { cookie, 'content-type': 'image/png' }, payload: PNG });
    expect(put.statusCode).toBe(204);
    const get = await app.inject({ method: 'GET', url: '/api/branding/logo' });
    expect(get.statusCode).toBe(200);
    expect(get.headers['content-type']).toBe('image/png');
    expect(get.rawPayload.equals(PNG)).toBe(true);
  });
});

describe('tailscale', () => {
  it('accepts lifecycle actions', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const res = await app.inject({ method: 'POST', url: '/api/tailscale/up', headers: { cookie } });
    expect(res.statusCode).toBe(202);
    check('tailscaleAction', res);
    expect((await app.inject({ method: 'POST', url: '/api/tailscale/explode', headers: { cookie } })).statusCode).toBe(400);
  });
});
