import { afterEach, describe, expect, it } from 'vitest';
import type { FastifyInstance } from 'fastify';
import type WebSocket from 'ws';
import { buildServer } from '../src/server.js';
import { check, closeCodeOf, login, userWith } from './helpers.js';

let app: FastifyInstance;
afterEach(async () => { await app?.close(); });

/** Accumulates terminal output and waits until it contains `needle`. */
function screen(ws: WebSocket) {
  let text = '';
  const waiters: { needle: string; resolve: () => void }[] = [];
  ws.on('message', (data, isBinary) => {
    if (!isBinary) return;
    text += (data as Buffer).toString('utf8');
    for (const w of [...waiters]) if (text.includes(w.needle)) { waiters.splice(waiters.indexOf(w), 1); w.resolve(); }
  });
  return {
    get text() { return text; },
    until(needle: string, ms = 2000) {
      if (text.includes(needle)) return Promise.resolve();
      return new Promise<void>((resolve, reject) => {
        waiters.push({ needle, resolve });
        setTimeout(() => reject(new Error(`no "${needle}" in: ${JSON.stringify(text)}`)), ms);
      });
    },
  };
}

describe('/api/terminal', () => {
  it('shows a prompt, runs ls and survives a resize', async () => {
    app = await buildServer();
    let out!: ReturnType<typeof screen>;
    const ws = await app.injectWS('/api/terminal?kind=local', { headers: { cookie: await login(app) } }, {
      onInit: (sock: WebSocket) => { out = screen(sock); },
    });
    await out.until('root@kvmpanel:~# ');
    ws.send(JSON.stringify({ t: 'resize', cols: 120, rows: 40 }));
    ws.send(Buffer.from('ls\r'), { binary: true });
    await out.until('images');
    ws.send(Buffer.from('\r'), { binary: true });
    await new Promise((r) => setTimeout(r, 100));
    expect(ws.readyState).toBe(1);
    ws.terminate();
  });

  it('closes serial with 4404 when no serial device is configured, 4403 without terminal', async () => {
    app = await buildServer();
    expect(await closeCodeOf(app, '/api/terminal?kind=serial', await login(app))).toBe(4404);
    expect(await closeCodeOf(app, '/api/terminal?kind=local', await userWith(app, ['console']))).toBe(4403);
    expect(await closeCodeOf(app, '/api/terminal?kind=local')).toBe(4401);
  });
});

describe('/api/scripts', () => {
  it('stores, lists, runs and deletes scripts', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const put = await app.inject({
      method: 'PUT', url: '/api/scripts/hello.sh', headers: { cookie, 'content-type': 'text/plain' }, payload: 'echo hello\n',
    });
    expect(put.statusCode).toBe(201);
    check('putScript', put);
    const list = await app.inject({ method: 'GET', url: '/api/scripts', headers: { cookie } });
    check('listScripts', list);
    expect(list.json()).toEqual([{ name: 'hello.sh', size: 11 }]);
    const run = await app.inject({ method: 'POST', url: '/api/scripts/hello.sh/run', headers: { cookie } });
    expect(run.statusCode).toBe(200);
    check('runScript', run);
    expect(run.json().exit_code).toBe(0);
    expect(run.json().output).toContain('hello');
    const del = await app.inject({ method: 'DELETE', url: '/api/scripts/hello.sh', headers: { cookie } });
    expect(del.statusCode).toBe(204);
    expect((await app.inject({ method: 'POST', url: '/api/scripts/hello.sh/run', headers: { cookie } })).statusCode).toBe(404);
  });

  it('rejects names outside [A-Za-z0-9._-]{1,64}', async () => {
    app = await buildServer();
    const cookie = await login(app);
    const bad = await app.inject({
      method: 'PUT', url: `/api/scripts/${encodeURIComponent('../x')}`, headers: { cookie, 'content-type': 'text/plain' }, payload: 'x',
    });
    expect(bad.statusCode).toBe(400);
    expect(bad.json().error).toBe('bad_name');
    check('putScript', bad);
    const denied = await app.inject({ method: 'GET', url: '/api/scripts', headers: { cookie: await userWith(app, ['console']) } });
    expect(denied.statusCode).toBe(403);
  });
});
