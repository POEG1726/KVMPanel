import { connect, type Socket } from 'node:net';
import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import type { Server } from 'node:net';
import { buildServer } from '../src/server.js';
import { messageValidator } from '../src/contract.js';
import { simulateFunctionKey, startPanelServer } from '../src/panel/server.js';
import { login } from './helpers.js';

const validToPanel = messageValidator('panel.schema.json', 'ToPanel');

let app: FastifyInstance;
let panel: Server;
let sock: Socket;
afterEach(async () => {
  sock?.destroy();
  await new Promise<void>((r) => (panel ? panel.close(() => r()) : r()));
  await app?.close();
});

/** Connects to the panel socket and returns an NDJSON inbox. */
async function open() {
  app = await buildServer();
  panel = await startPanelServer(app.kvm, { port: 0 });
  const { port } = panel.address() as { port: number };
  sock = connect(port, '127.0.0.1');
  const queue: any[] = [];
  const waiters: { pred: (m: any) => boolean; resolve: (m: any) => void }[] = [];
  let buf = '';
  sock.on('data', (d) => {
    buf += d.toString('utf8');
    let nl: number;
    while ((nl = buf.indexOf('\n')) >= 0) {
      const m = JSON.parse(buf.slice(0, nl));
      buf = buf.slice(nl + 1);
      const i = waiters.findIndex((w) => w.pred(m));
      if (i >= 0) waiters.splice(i, 1)[0].resolve(m);
      else queue.push(m);
    }
  });
  await new Promise((r) => sock.on('connect', r));
  return {
    all: queue,
    send: (m: unknown) => sock.write(typeof m === 'string' ? m : JSON.stringify(m) + '\n'),
    next(pred: (m: any) => boolean, ms = 2000): Promise<any> {
      const i = queue.findIndex(pred);
      if (i >= 0) return Promise.resolve(queue.splice(i, 1)[0]);
      return new Promise((resolve, reject) => {
        const w = { pred, resolve };
        waiters.push(w);
        setTimeout(() => { const j = waiters.indexOf(w); if (j >= 0) { waiters.splice(j, 1); reject(new Error('timeout')); } }, ms);
      });
    },
  };
}

describe('panel socket', () => {
  it('greets with hello and config, then streams valid telemetry', async () => {
    const p = await open();
    const hello = await p.next((m) => m.t === 'hello');
    expect(hello).toEqual({ t: 'hello', version: '0.1.0' });
    const cfg = await p.next((m) => m.t === 'config');
    expect(validToPanel(cfg)).toBe(true);
    expect(cfg.config.panel.language).toBe('en');
    const tel = await p.next((m) => m.t === 'telemetry', 1500);
    expect(validToPanel(tel), JSON.stringify(validToPanel.errors)).toBe(true);
    const ids = tel.metrics.map((m: { id: string }) => m.id);
    expect(ids.filter((id: string) => id.startsWith('cpu.core.'))).toHaveLength(16);
    expect(ids.filter((id: string) => id.startsWith('mem.dimm.'))).toEqual(['mem.dimm.a2', 'mem.dimm.b2']);
    expect(tel.metrics.find((m: { id: string }) => m.id === 'mem.dimm.a2').value).toEqual(expect.any(Number));
    expect(tel.metrics.find((m: { id: string }) => m.id === 'cpu.tctl')).toMatchObject({ unit: '°C', min: 30, max: 95, warn: 85, group: 'cpu' });
    expect(tel.metrics.find((m: { id: string }) => m.id === 'ssd.0.temp').label).toBe('SSD 2T');
    const status = await p.next((m) => m.t === 'status');
    expect(validToPanel(status)).toBe(true);
  });

  it('applies the same config rules as HTTP and pushes changes made over HTTP', async () => {
    const p = await open();
    await p.next((m) => m.t === 'config');
    p.send({ t: 'patch_config', id: 7, patch: { usb: { net: true } } });
    expect(await p.next((m) => m.t === 'result' && m.id === 7)).toEqual({
      t: 'result', id: 7, ok: false, error: 'endpoints_full', detail: { in_missing: 1, out_missing: 0 },
    });
    p.send({ t: 'patch_config', id: 8, patch: { panel: { language: 'zh' } } });
    const ok = await p.next((m) => m.t === 'result' && m.id === 8);
    expect(ok.ok).toBe(true);
    await p.next((m) => m.t === 'config' && m.config.panel.language === 'zh');

    const cookie = await login(app);
    await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload: { panel: { brightness: 33 } } });
    const pushed = await p.next((m) => m.t === 'config' && m.config.panel.brightness === 33);
    expect(validToPanel(pushed)).toBe(true);
  });

  it('pushes physical FUNC actions and the current display state', async () => {
    const p = await open();
    await p.next((m) => m.t === 'board');
    for (const [press, action, asleep] of [
      ['long', 'toggle_screen', true],
      ['short', 'toggle_settings', false],
      ['long', 'toggle_screen', true],
    ] as const) {
      simulateFunctionKey(app.kvm, press);
      const msg = await p.next((m) => m.t === 'function_key');
      expect(validToPanel(msg)).toBe(true);
      expect(msg).toEqual({ t: 'function_key', action, display_asleep: asleep });
    }
    p.send({ t: 'action', id: 4, action: 'wake_display' });
    expect((await p.next((m) => m.t === 'board' && !m.display_asleep)).display_asleep).toBe(false);
    p.send({ t: 'patch_config', id: 5, patch: { function_key: { short: 'cycle_brightness' } } });
    expect((await p.next((m) => m.t === 'result' && m.id === 5)).ok).toBe(true);
    for (const [auto, level] of [[false, 20], [false, 50], [false, 100], [true, 100]]) {
      simulateFunctionKey(app.kvm, 'short');
      const cfg = await p.next((m) => m.t === 'config' && m.config.panel.brightness === level
        && m.config.panel.auto_brightness === auto);
      expect(validToPanel(cfg)).toBe(true);
      expect(await p.next((m) => m.t === 'function_key')).toMatchObject({ action: 'cycle_brightness' });
    }
  });

  it('hides metrics listed in hidden_metrics', async () => {
    const p = await open();
    p.send({ t: 'patch_config', id: 1, patch: { panel: { hidden_metrics: ['cpu.bclk'] } } });
    await p.next((m) => m.t === 'result' && m.id === 1);
    await p.next((m) => m.t === 'telemetry' && !m.metrics.some((x: { id: string }) => x.id === 'cpu.bclk'), 2500);
  });

  it('stops telemetry and reports pc disconnected', async () => {
    const p = await open();
    await p.next((m) => m.t === 'telemetry');
    app.kvm.pc.connected = false;
    const pc = await p.next((m) => m.t === 'pc' && m.connected === false);
    expect(validToPanel(pc)).toBe(true);
    expect(pc.connected).toBe(false);
    p.all.length = 0;
    await new Promise((r) => setTimeout(r, 1500));
    expect(p.all.filter((m) => m.t === 'telemetry')).toHaveLength(0);
  });

  it('streams the board state every second, including while the PC is off', async () => {
    const p = await open();
    const first = await p.next((m) => m.t === 'board');
    expect(validToPanel(first), JSON.stringify(validToPanel.errors)).toBe(true);
    expect(first).toMatchObject({
      pc_power: 'on', hostname: 'kvmpanel', version: '0.1.0',
      eth: { iface: 'eth0', link_mbps: 1000, duplex: 'full' },
      access: { url: 'http://kvmpanel.local', tailscale_ip: null },
      kvm: { sessions: [], stream: { state: 'idle', fps: null }, media: { mounted: null, mode: null } },
    });
    expect(first.kvm.usb_functions).toEqual(['keyboard', 'mouse_rel', 'mouse_abs', 'storage', 'telemetry']);
    expect(first.kvm.hdmi).toEqual({ signal: true, width: 1920, height: 1080, fps: 60 });

    app.kvm.power.on = false;
    app.kvm.pc.connected = false;
    const off = await p.next((m) => m.t === 'board' && m.pc_power === 'off', 1500);
    expect(validToPanel(off), JSON.stringify(validToPanel.errors)).toBe(true);
    expect(off.kvm.hdmi).toEqual({ signal: false, width: null, height: null, fps: null });
    const status = await p.next((m) => m.t === 'status' && m.pc.state === 'off');
    expect(validToPanel(status)).toBe(true);
  });

  it('powers the PC on only while it is off', async () => {
    const p = await open();
    await p.next((m) => m.t === 'hello');
    expect(await p.next((m) => m.t === 'pc')).toMatchObject({ connected: true });
    p.send({ t: 'action', id: 5, action: 'pc_power_on' });
    expect(await p.next((m) => m.t === 'result' && m.id === 5)).toEqual({ t: 'result', id: 5, ok: false, error: 'pc_on' });
    expect(app.kvm.power.on).toBe(true);

    app.kvm.power.on = false;
    app.kvm.pc.connected = false;
    p.send({ t: 'action', id: 6, action: 'pc_power_on' });
    expect(await p.next((m) => m.t === 'result' && m.id === 6)).toEqual({ t: 'result', id: 6, ok: true });
    expect(app.kvm.power.on).toBe(true);
    expect(app.kvm.pc.connected).toBe(false);
    const status = await p.next((m) => m.t === 'status' && m.pc.state === 'partial');
    expect(validToPanel(status)).toBe(true);
  });

  it('answers a bad line with bad_message and keeps serving', async () => {
    const p = await open();
    p.send('{not json\n');
    expect(await p.next((m) => m.t === 'result')).toEqual({ t: 'result', ok: false, error: 'bad_message' });
    p.send({ t: 'nope', id: 2 });
    expect(await p.next((m) => m.t === 'result')).toEqual({ t: 'result', id: 2, ok: false, error: 'bad_message' });
    p.send({ t: 'device_info', id: 3 });
    const info = await p.next((m) => m.t === 'result' && m.id === 3);
    expect(info.ok).toBe(true);
    expect(info.data).toMatchObject({ hostname: 'kvmpanel', lux: 212 });
    p.send({ t: 'action', id: 4, action: 'usb_apply' });
    expect(await p.next((m) => m.t === 'result' && m.id === 4)).toEqual({ t: 'result', id: 4, ok: true });
  });
});
