import { describe, expect, it } from 'vitest';
import { absMessage, buttonsMask, keyMessage, relMessage, wheelMessage } from './input';

const rect = { left: 0, top: 0, width: 1920, height: 1080 };

describe('input messages', () => {
  it('maps the middle of a 1920x1080 screen to about 32767', () => {
    const m = absMessage(960, 540, rect, 0);
    expect(m).toMatchObject({ t: 'mouse_abs', buttons: 0 });
    if (m.t !== 'mouse_abs') throw new Error('type');
    expect(Math.abs(m.x - 32767)).toBeLessThanOrEqual(1);
    expect(Math.abs(m.y - 32767)).toBeLessThanOrEqual(1);
  });

  it('offsets by the rect origin and clamps outside it', () => {
    const r = { left: 100, top: 50, width: 200, height: 100 };
    expect(absMessage(100, 50, r, 1)).toEqual({ t: 'mouse_abs', x: 0, y: 0, buttons: 1 });
    expect(absMessage(900, 900, r, 0)).toMatchObject({ x: 65535, y: 65535 });
  });

  it('clamps relative motion and wheel to +-127', () => {
    expect(relMessage(500, -500, 1)).toEqual({ t: 'mouse_rel', dx: 127, dy: -127, buttons: 1 });
  });

  describe('wheel', () => {
    it('turns 100 px into one notch and clamps to +-127', () => {
      expect(wheelMessage(100)).toEqual({ t: 'wheel', dy: 1 });
      expect(wheelMessage(-300)).toEqual({ t: 'wheel', dy: -3 });
      expect(wheelMessage(-100000)).toEqual({ t: 'wheel', dy: -127 });
      expect(wheelMessage(3)).toEqual({ t: 'wheel', dy: 0 });
    });

    it('normalises line and page deltas to pixels first (x40, x800)', () => {
      const LINE = 1;
      const PAGE = 2;
      expect(wheelMessage(3, { deltaMode: LINE })).toEqual({ t: 'wheel', dy: 1 }); // 120 px -> 1.2 -> 1
      expect(wheelMessage(5, { deltaMode: LINE })).toEqual({ t: 'wheel', dy: 2 }); // 200 px
      expect(wheelMessage(1, { deltaMode: PAGE })).toEqual({ t: 'wheel', dy: 8 }); // 800 px
      expect(wheelMessage(-1, { deltaMode: PAGE })).toEqual({ t: 'wheel', dy: -8 });
    });

    it('negates for reverse and multiplies by the speed before clamping', () => {
      expect(wheelMessage(200, { dir: 'reverse' })).toEqual({ t: 'wheel', dy: -2 });
      expect(wheelMessage(-200, { dir: 'reverse' })).toEqual({ t: 'wheel', dy: 2 });
      expect(wheelMessage(200, { dir: 'normal', speed: 5 })).toEqual({ t: 'wheel', dy: 10 });
      expect(wheelMessage(200, { dir: 'reverse', speed: 10 })).toEqual({ t: 'wheel', dy: -20 });
      expect(wheelMessage(1, { deltaMode: 2, speed: 10 })).toEqual({ t: 'wheel', dy: 80 });
      expect(wheelMessage(-2, { deltaMode: 2, speed: 10 })).toEqual({ t: 'wheel', dy: -127 });
    });

    it('never produces negative zero', () => {
      expect(Object.is((wheelMessage(-3) as { dy: number }).dy, 0)).toBe(true);
      expect(Object.is((wheelMessage(3, { dir: 'reverse' }) as { dy: number }).dy, 0)).toBe(true);
    });
  });

  it('uses the contract bit mask', () => {
    expect(buttonsMask(new MouseEvent('mousedown', { buttons: 4 }))).toBe(4);
    expect(buttonsMask(new MouseEvent('mousedown', { buttons: 1 | 2 | 16 }))).toBe(19);
  });

  it('builds key messages from code and ignores IME composition', () => {
    expect(keyMessage(new KeyboardEvent('keydown', { code: 'KeyA' }))).toEqual({ t: 'key', code: 'KeyA', down: true });
    expect(keyMessage(new KeyboardEvent('keyup', { code: 'KeyA' }))).toEqual({ t: 'key', code: 'KeyA', down: false });
    expect(keyMessage(new KeyboardEvent('keydown', { code: 'KeyA', isComposing: true }))).toBeNull();
  });
});
