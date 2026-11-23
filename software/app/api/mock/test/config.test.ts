import { describe, expect, it } from 'vitest';
import { buildServer } from '../src/server.js';
import { check, login, userWith } from './helpers.js';

const DEFAULTS = {
  panel: {
    language: 'en', orientation: 'normal', auto_brightness: true, brightness: 70, offline_sleep_min: 5,
    thresholds: { 'cpu.tctl': 85, 'gpu.mem_junction': 95, 'mem.dimm.*': 60 },
    hidden_metrics: [],
  },
  function_key: { short: 'toggle_settings', long: 'toggle_screen' },
  usb: {
    keyboard: true, mouse_rel: true, mouse_abs: true, mouse_merge: false, storage: true,
    net: false, net_mode: 'ncm', acm: false, telemetry: true,
  },
  web: { language: 'auto', workspace_open: 'float', remember_float: true, menubar: 'auto_hide', title: 'KVMPanel' },
  video: { quality: 80, fps_max: 30, scale: 'fit', frame_diff: true },
  hid: { keyboard_layout: 'us', mouse_mode: 'abs', wheel_dir: 'normal', wheel_speed: 1, jiggler: false, leader_key: null },
  serial: { device: null, baud: 115200 },
  device: { hostname: 'kvmpanel', ssh: true, mdns: true, watchdog: false },
  kvm: { enabled: true },
};

describe('config', () => {
  it('returns the spec defaults', async () => {
    const app = await buildServer();
    const res = await app.inject({ method: 'GET', url: '/api/config', headers: { cookie: await login(app) } });
    expect(res.statusCode).toBe(200);
    check('getConfig', res);
    expect(res.json()).toEqual(DEFAULTS);
  });

  it('merge-patches a single field', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const res = await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload: { panel: { brightness: 30 } } });
    expect(res.statusCode).toBe(200);
    check('patchConfig', res);
    expect(res.json()).toEqual({ ...DEFAULTS, panel: { ...DEFAULTS.panel, brightness: 30 } });
  });

  it('refuses a USB set that exceeds the endpoint budget and keeps the config', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const res = await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload: { usb: { net: true } } });
    expect(res.statusCode).toBe(409);
    check('patchConfig', res);
    expect(res.json()).toEqual({ error: 'endpoints_full', detail: { in_missing: 1, out_missing: 0 } });
    const after = await app.inject({ method: 'GET', url: '/api/config', headers: { cookie } });
    expect(after.json().usb.net).toBe(false);
  });

  it('rejects out-of-range and unknown fields with their path and keeps the config', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    for (const [payload, path] of [
      [{ panel: { brightness: 200 } }, '/panel/brightness'],
      [{ panel: { nope: 1 } }, '/panel/nope'],
      [{ nosuch: {} }, '/nosuch'],
    ] as const) {
      const res = await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload });
      expect(res.statusCode).toBe(400);
      check('patchConfig', res);
      expect(res.json()).toEqual({ error: 'invalid_config', detail: { path } });
    }
    const after = await app.inject({ method: 'GET', url: '/api/config', headers: { cookie } });
    expect(after.json()).toEqual(DEFAULTS);
  });

  it('requires the section permission to patch it', async () => {
    const app = await buildServer();
    const cookie = await userWith(app, ['panel']);
    const denied = await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload: { usb: { acm: false } } });
    expect(denied.statusCode).toBe(403);
    check('patchConfig', denied);
    const ok = await app.inject({ method: 'PATCH', url: '/api/config', headers: { cookie }, payload: { panel: { brightness: 10 } } });
    expect(ok.statusCode).toBe(200);
  });

  it('reports the budget and accepts an apply request', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const budget = await app.inject({ method: 'GET', url: '/api/usb/budget', headers: { cookie } });
    check('getUsbBudget', budget);
    expect(budget.json()).toEqual({ in: 4, out: 1, fits: true, in_missing: 0, out_missing: 0 });
    let fired = false;
    app.kvm.events.once('usb_reenumerated', () => { fired = true; });
    const apply = await app.inject({ method: 'POST', url: '/api/usb/apply', headers: { cookie } });
    expect(apply.statusCode).toBe(202);
    check('applyUsb', apply);
    expect(apply.json()).toEqual({ reenumerating: true });
    expect(fired).toBe(true);
  });
});
