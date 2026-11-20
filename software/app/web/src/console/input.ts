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

export function wheelMessage(deltaY: number): ClientMessage {
  return { t: 'wheel', dy: clamp127(deltaY) };
}

/** Contract bit mask (1 left, 2 right, 4 middle, 8 back, 16 forward); MouseEvent.buttons uses the same bits. */
export function buttonsMask(e: MouseEvent): number {
  return e.buttons & 0x1f;
}
