import { buildServer } from './server.js';

const HTTP_PORT = Number(process.env.KVMPANEL_MOCK_PORT ?? 8800);

const app = await buildServer();
await app.listen({ host: '127.0.0.1', port: HTTP_PORT });
console.log(`KVMPanel mock: http://127.0.0.1:${HTTP_PORT}`);
