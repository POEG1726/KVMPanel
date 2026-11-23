import { describe, expect, it } from 'vitest';
import { usbBudget, type UsbConfig } from '../src/usb.js';

const DEFAULT: UsbConfig = {
  keyboard: true, mouse_rel: true, mouse_abs: true, mouse_merge: false, storage: true, microphone: false,
  net: false, net_mode: 'ncm', acm: false, telemetry: true,
};

describe('usbBudget (spec 4.5)', () => {
  it('counts the microphone as one IN endpoint', () => {
    expect(usbBudget({ ...DEFAULT, microphone: true })).toMatchObject({ in: 5, out: 1, fits: true });
    expect(usbBudget({ ...DEFAULT, microphone: true, mouse_merge: true, net: true }))
      .toMatchObject({ in: 6, out: 2, fits: false, in_missing: 1 });
  });
  it('default set uses IN 4 / OUT 1', () => {
    expect(usbBudget(DEFAULT)).toEqual({ in: 4, out: 1, fits: true, in_missing: 0, out_missing: 0 });
  });

  it('adding the virtual NIC overflows IN by one', () => {
    expect(usbBudget({ ...DEFAULT, net: true })).toMatchObject({ in: 6, out: 2, fits: false, in_missing: 1, out_missing: 0 });
  });

  it('merging the mice frees one IN so the NIC fits', () => {
    expect(usbBudget({ ...DEFAULT, mouse_merge: true })).toMatchObject({ in: 3, fits: true });
    expect(usbBudget({ ...DEFAULT, mouse_merge: true, net: true })).toMatchObject({ in: 5, fits: true });
  });

  it('telemetry costs one IN only when the absolute mouse is off', () => {
    expect(usbBudget({ ...DEFAULT, mouse_abs: false })).toMatchObject({ in: 4, out: 1, fits: true });
    const only = { ...DEFAULT, mouse_rel: false, mouse_abs: false, storage: false };
    expect(usbBudget(only)).toMatchObject({ in: 2, out: 0, fits: true });
    expect(usbBudget({ ...only, telemetry: false })).toMatchObject({ in: 1 });
  });

  it('keyboard + NIC + ACM + standalone telemetry needs 6 IN', () => {
    const cfg = { ...DEFAULT, mouse_rel: false, mouse_abs: false, storage: false, net: true, acm: true };
    expect(usbBudget(cfg)).toMatchObject({ in: 6, out: 2, fits: false, in_missing: 1 });
  });

  it('merge has no effect unless both mice are on', () => {
    expect(usbBudget({ ...DEFAULT, mouse_abs: false, mouse_merge: true })).toMatchObject({ in: 4 });
  });
});
