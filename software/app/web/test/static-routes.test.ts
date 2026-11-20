import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { expect, it } from 'vitest';

it('builds login and workspace entry points for kvmd static directory serving', async () => {
  const output = await mkdtemp(join(tmpdir(), 'kvm-web-routes-'));
  try {
    await promisify(execFile)(process.execPath, ['node_modules/vite/bin/vite.js', 'build', '--outDir', output], { cwd: process.cwd() });
    const index = await readFile(join(output, 'index.html'), 'utf8');
    for (const route of ['login', 'workspace']) {
      expect(await readFile(join(output, route, 'index.html'), 'utf8')).toBe(index);
    }
    const asset = index.match(/src="(\/assets\/[^"]+\.js)"/)?.[1];
    expect(asset).toBeTruthy();
    expect((await stat(join(output, asset!))).isFile()).toBe(true);
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});
