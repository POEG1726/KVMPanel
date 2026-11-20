import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import type WebSocket from 'ws';
import { buildServer } from '../src/server.js';
import { check, login } from './helpers.js';

let app: FastifyInstance;
afterEach(async () => { await app?.close(); });

function frames(ws: WebSocket): Buffer[] {
  const got: Buffer[] = [];
  ws.on('message', (data, isBinary) => { if (isBinary) got.push(data as Buffer); });
  return got;
}
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

describe('/api/stream', () => {
  it('sends whole JPEG frames at the configured rate', async () => {
    app = await buildServer();
    const cookie = await login(app);
    await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload: { video: { fps_max: 5 } } });
    const ws = await app.injectWS('/api/stream', { headers: { cookie } });
    const got = frames(ws);
    await wait(2000);
    ws.terminate();
    expect(got.length).toBeGreaterThanOrEqual(8);
    expect(got.length).toBeLessThanOrEqual(12);
    for (const f of got) {
      expect(f.subarray(0, 2).toString('hex')).toBe('ffd8');
      expect(f.subarray(-2).toString('hex')).toBe('ffd9');
    }
  });

  it('stops sending while paused and resumes', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const ws = await app.injectWS('/api/stream', { headers: { cookie } });
    const got = frames(ws);
    await wait(300);
    ws.send(JSON.stringify({ t: 'pause' }));
    await wait(150);
    const atPause = got.length;
    await wait(1000);
    expect(got.length).toBe(atPause);
    ws.send(JSON.stringify({ t: 'resume' }));
    await wait(500);
    expect(got.length).toBeGreaterThan(atPause);
    ws.terminate();
  });
});

describe('snapshot and EDID', () => {
  it('returns a JPEG snapshot, or 503 no_signal without HDMI', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const ok = await app.inject({ method: 'GET', url: '/api/stream/snapshot.jpg', headers: { cookie } });
    expect(ok.statusCode).toBe(200);
    expect(ok.headers['content-type']).toBe('image/jpeg');
    expect(ok.rawPayload.subarray(0, 2).toString('hex')).toBe('ffd8');
    check('getSnapshot', ok);
    app.kvm.hdmi.signal = false;
    const none = await app.inject({ method: 'GET', url: '/api/stream/snapshot.jpg', headers: { cookie } });
    expect(none.statusCode).toBe(503);
    expect(none.json()).toEqual({ error: 'no_signal' });
    check('getSnapshot', none);
  });

  it('validates, stores and resets the EDID', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const initial = await app.inject({ method: 'GET', url: '/api/video/edid', headers: { cookie } });
    check('getEdid', initial);
    expect(initial.json().preset).toBe('1080p');
    expect(initial.json().hex).toMatch(/^00ffffffffffff00[0-9a-f]{240}$/);

    const bad = await app.inject({ method: 'PUT', url: '/api/video/edid', headers: { cookie }, payload: { hex: '00ff' } });
    expect(bad.statusCode).toBe(400);
    expect(bad.json().error).toBe('bad_edid');
    check('putEdid', bad);

    const custom = '00ffffffffffff00' + '11'.repeat(120);
    const put = await app.inject({ method: 'PUT', url: '/api/video/edid', headers: { cookie }, payload: { hex: custom } });
    expect(put.statusCode).toBe(200);
    check('putEdid', put);
    expect(put.json()).toEqual({ hex: custom, preset: 'custom' });

    const reset = await app.inject({ method: 'POST', url: '/api/video/edid/reset', headers: { cookie } });
    expect(reset.statusCode).toBe(200);
    check('resetEdid', reset);
    expect(reset.json()).toEqual(initial.json());
  });
});
