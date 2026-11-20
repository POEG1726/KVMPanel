import type { AddressInfo } from 'node:net';
import { buildServer } from '../../api/mock/src/server';

let app: Awaited<ReturnType<typeof buildServer>> | undefined;

export async function setup(): Promise<void> {
  app = await buildServer();
  await app.listen({ host: '127.0.0.1', port: 0 });
  const { port } = app.server.address() as AddressInfo;
  process.env.MOCK_URL = `http://127.0.0.1:${port}`;
}

export async function teardown(): Promise<void> {
  await app?.close();
}

