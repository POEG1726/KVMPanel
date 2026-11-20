import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { buildServer } from '../../api/mock/src/server';

type Mock = Awaited<ReturnType<typeof buildServer>>;

let app: Mock | undefined;
let control: Server | undefined;
const extra = new Map<string, Mock>();

async function listen(m: Mock): Promise<string> {
  await m.listen({ host: '127.0.0.1', port: 0 });
  return `http://127.0.0.1:${(m.server.address() as AddressInfo).port}`;
}

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
    })();
  });
  await new Promise<void>((r) => control!.listen(0, '127.0.0.1', r));
  process.env.MOCK_CONTROL_URL = `http://127.0.0.1:${(control.address() as AddressInfo).port}`;
}

export async function teardown(): Promise<void> {
  await Promise.all([...extra.values()].map((m) => m.close()));
  await app?.close();
  await new Promise((r) => control?.close(r));
}
