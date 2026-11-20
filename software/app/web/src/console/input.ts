import type { ClientMessage } from '../api/ws-types';

const clamp127 = (n: number): number => Math.max(-127, Math.min(127, Math.round(n)));

/** Key event -> message (`e.code`, down for keydown). Null while an IME composition is running. */
export function keyMessage(e: KeyboardEvent): ClientMessage | null {
  if (e.isComposing || e.keyCode === 229 || !e.code) return null;
  return { t: 'key', code: e.code, down: e.type !== 'keyup' };
}

/** Pointer position inside `rect` -> 0-65535 on both axes. */
export function absMessage(x: number, y: number, rect: Pick<DOMRect, 'left' | 'top' | 'width' | 'height'>, buttons: number): ClientMessage {
  const scale = (v: number, size: number) => Math.max(0, Math.min(65535, Math.round((v / (size || 1)) * 65535)));
  return { t: 'mouse_abs', x: scale(x - rect.left, rect.width), y: scale(y - rect.top, rect.height), buttons };
}

export function relMessage(dx: number, dy: number, buttons: number): ClientMessage {
  return { t: 'mouse_rel', dx: clamp127(dx), dy: clamp127(dy), buttons };
}

const PX_PER_LINE = 40;
const PX_PER_PAGE = 800;
const PX_PER_NOTCH = 100;

export interface WheelOptions {
  /** WheelEvent.deltaMode: 0 pixels, 1 lines, 2 pages. */
  deltaMode?: number;
  /** config.hid.wheel_dir: 'reverse' negates. */
  dir?: 'normal' | 'reverse';
  /** config.hid.wheel_speed, 1-10. */
  speed?: number;
}

const toPixels = (delta: number, deltaMode: number): number => delta * (deltaMode === 1 ? PX_PER_LINE : deltaMode === 2 ? PX_PER_PAGE : 1);

/**
 * Like wheelMessage, but keeps the sub-notch remainder between events so slow trackpad scrolling still
 * produces notches. Returns null while less than one notch has built up; a direction change drops the remainder.
 */
export function createWheelAccumulator(): (deltaY: number, opts?: WheelOptions) => ClientMessage | null {
  let rest = 0;
  return (deltaY, opts = {}) => {
    const px = toPixels(deltaY, opts.deltaMode ?? 0);
    if (px !== 0 && Math.sign(px) !== Math.sign(rest)) rest = 0;
    rest += px;
    const notches = Math.trunc(rest / PX_PER_NOTCH);
    if (notches === 0) return null;
    rest -= notches * PX_PER_NOTCH;
    return wheelMessage(notches * PX_PER_NOTCH, { ...opts, deltaMode: 0 });
  };
}

/** Wheel delta -> notches: normalise to pixels, 100 px per notch, scale by speed, clamp to +-127. */
export function wheelMessage(deltaY: number, { deltaMode = 0, dir = 'normal', speed = 1 }: WheelOptions = {}): ClientMessage {
  const px = toPixels(deltaY, deltaMode);
  const notches = Math.round(px / PX_PER_NOTCH) * speed * (dir === 'reverse' ? -1 : 1);
  // `|| 0` turns -0 into 0 so the JSON and equality checks never see a negative zero.
  return { t: 'wheel', dy: clamp127(notches) || 0 };
}

/** Contract bit mask (1 left, 2 right, 4 middle, 8 back, 16 forward); MouseEvent.buttons uses the same bits. */
export function buttonsMask(e: MouseEvent): number {
  return e.buttons & 0x1f;
}
