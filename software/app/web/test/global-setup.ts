import { createServer, type Server } from 'node:http';
import { randomInt } from 'node:crypto';
import { buildServer } from '../../api/mock/src/server';

type Mock = Awaited<ReturnType<typeof buildServer>>;

let app: Mock | undefined;
let control: Server | undefined;
const extra = new Map<string, Mock>();

// Some Windows ephemeral-port ranges include ports blocked by browser and Node fetch.
async function listenOnHighPort(bind: (port: number) => Promise<unknown>): Promise<string> {
  for (let attempt = 0; ; attempt++) {
    const port = randomInt(20000, 60000);
    try {
      await bind(port);
      return 'http://127.0.0.1:' + port;
    } catch (err) {
      const code = (err as NodeJS.ErrnoException).code;
      // Windows also reserves entire port ranges with EACCES.
      if ((code !== 'EADDRINUSE' && code !== 'EACCES') || attempt >= 10) throw err;
    }
  }
}

const listen = (m: Mock) => listenOnHighPort(port => m.listen({ host: '127.0.0.1', port }));

export async function setup(): Promise<void> {
  app = await buildServer();
  process.env.MOCK_URL = await listen(app);

  // Tests that change passwords or lock accounts ask for a private mock (the test files run in
  // jsdom, which cannot load the mock itself): POST /new -> {url}, DELETE /?url=... closes it.
  control = createServer((req, res) => {
    void (async () => {
      if (req.method === 'POST') {
        const m = await buildServer({ sleep: async () => {} });
        const url = await listen(m);
        extra.set(url, m);
        res.end(JSON.stringify({ url }));
      } else {
        const url = new URL(req.url ?? '/', 'http://x').searchParams.get('url') ?? '';
        await extra.get(url)?.close();
        extra.delete(url);
        res.end('{}');
      }
    })().catch((err: unknown) => {
      res.statusCode = 500;
      res.end(JSON.stringify({ error: String(err) }));
    });
  });
  const server = control;
  process.env.MOCK_CONTROL_URL = await listenOnHighPort(port => new Promise<void>((resolve, reject) => {
    const onError = (err: Error) => {
      server.off('listening', onListen);
      reject(err);
    };
    const onListen = () => {
      server.off('error', onError);
      resolve();
    };
    server.once('error', onError);
    server.listen(port, '127.0.0.1', onListen);
  }));
}

export async function teardown(): Promise<void> {
  await Promise.all([...extra.values()].map((m) => m.close()));
  await app?.close();
  await new Promise((r) => control?.close(r));
}
