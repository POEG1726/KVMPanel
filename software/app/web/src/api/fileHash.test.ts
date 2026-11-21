import { createHash } from 'node:crypto';
import { expect, it, vi } from 'vitest';
import { sha256File } from './fileHash';

it('hashes a large binary file in slices and works on HTTP without SubtleCrypto', async () => {
  const bytes = new Uint8Array(3 * 1024 * 1024 + 7);
  bytes.fill(255, 4000);
  const blob = new Blob([bytes]);
  const wholeFile = vi.spyOn(blob, 'arrayBuffer');
  expect(await sha256File(blob)).toBe(createHash('sha256').update(bytes).digest('hex'));
  expect(wholeFile).not.toHaveBeenCalled();
});

it('hashes empty files and respects an aborted request', async () => {
  const blob = new Blob();
  expect(await sha256File(blob)).toBe(createHash('sha256').digest('hex'));
  await expect(sha256File(blob, AbortSignal.abort())).rejects.toMatchObject({ name: 'AbortError' });
});
