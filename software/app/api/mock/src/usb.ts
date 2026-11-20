/** T113-i USB0 has EP1–EP5, each with one IN and one OUT (datasheet, USB2.0 OTG). */
export const EP_IN = 5;
export const EP_OUT = 5;

export interface UsbConfig {
  keyboard: boolean;
  mouse_rel: boolean;
  mouse_abs: boolean;
  mouse_merge: boolean;
  storage: boolean;
  net: boolean;
  net_mode: 'ncm' | 'rndis';
  acm: boolean;
  telemetry: boolean;
}

export interface UsbBudget {
  in: number;
  out: number;
  fits: boolean;
  in_missing: number;
  out_missing: number;
}

/** Endpoint cost of a USB gadget set, per the table in spec 4.5. */
export function usbBudget(u: UsbConfig): UsbBudget {
  let inEp = 0;
  let outEp = 0;
  if (u.keyboard) inEp += 1;
  if (u.mouse_rel) inEp += 1;
  if (u.mouse_abs && !(u.mouse_merge && u.mouse_rel)) inEp += 1;
  if (u.storage) { inEp += 1; outEp += 1; }
  if (u.net) { inEp += 2; outEp += 1; }
  if (u.acm) { inEp += 2; outEp += 1; }
  // Telemetry rides EP0 SET_REPORT on a mouse interface; standalone it needs its own interrupt IN.
  if (u.telemetry && !u.mouse_rel && !u.mouse_abs) inEp += 1;
  const in_missing = Math.max(0, inEp - EP_IN);
  const out_missing = Math.max(0, outEp - EP_OUT);
  return { in: inEp, out: outEp, fits: in_missing === 0 && out_missing === 0, in_missing, out_missing };
}
