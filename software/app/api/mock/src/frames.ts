import jpeg from 'jpeg-js';

const W = 1920;
const H = 1080;
export const FRAME_COUNT = 8;
const cache = new Map<number, Buffer>();

/** Mock HDMI frame `i` (mod FRAME_COUNT): dark gradient with a coloured bar moving right. Encoded lazily, then cached. */
export function mockFrame(i: number): Buffer {
  const idx = ((i % FRAME_COUNT) + FRAME_COUNT) % FRAME_COUNT;
  const hit = cache.get(idx);
  if (hit) return hit;
  const data = Buffer.alloc(W * H * 4);
  const barX = Math.floor((idx * W) / FRAME_COUNT);
  const barW = 120;
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const o = (y * W + x) * 4;
      const inBar = x >= barX && x < barX + barW;
      data[o] = inBar ? 94 : 15 + ((x * 40) / W) | 0;
      data[o + 1] = inBar ? 196 : 61 + ((y * 60) / H) | 0;
      data[o + 2] = inBar ? 255 : 110;
      data[o + 3] = 255;
    }
  }
  const buf = Buffer.from(jpeg.encode({ data, width: W, height: H }, 50).data);
  cache.set(idx, buf);
  return buf;
}

/** Built-in 128-byte EDID: preferred 1920×1080@30 (74.25 MHz) detailed timing, valid checksum. */
export function edid1080p(): string {
  const b = new Uint8Array(128);
  b.set([0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00], 0);
  b.set([0x2c, 0x56], 8); // manufacturer "KVP"
  b.set([0x01, 0x00, 0x01, 0x00, 0x00, 0x00], 10);
  b[16] = 46; b[17] = 36; // week 46 of 2026
  b[18] = 1; b[19] = 3; // EDID 1.3
  b.set([0x80, 0x35, 0x1e, 0x78, 0x0a], 20); // digital, 53×30 cm
  for (let i = 38; i < 54; i += 2) { b[i] = 0x01; b[i + 1] = 0x01; } // no standard timings
  // Preferred detailed timing: 74.25 MHz, 1920×1080@30, hblank 280, vblank 45 (capture bus limit)
  b.set([0x01, 0x1d, 0x80, 0x18, 0x71, 0x38, 0x2d, 0x40, 0x58, 0x2c, 0x45, 0x00, 0x13, 0x2b, 0x21, 0x00, 0x00, 0x1e], 54);
  // Monitor name descriptor "KVMPanel"
  b.set([0, 0, 0, 0xfc, 0, ...Buffer.from('KVMPanel\n    ', 'ascii').subarray(0, 13)], 72);
  b.set([0, 0, 0, 0x10, 0], 90);
  b.set([0, 0, 0, 0x10, 0], 108);
  const sum = b.subarray(0, 127).reduce((a, v) => (a + v) & 0xff, 0);
  b[127] = (256 - sum) & 0xff;
  return Buffer.from(b).toString('hex');
}
