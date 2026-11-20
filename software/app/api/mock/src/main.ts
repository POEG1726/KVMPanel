import { buildServer } from './server.js';
import { startPanelServer } from './panel/server.js';

const HTTP_PORT = Number(process.env.KVMPANEL_MOCK_PORT ?? 8800);
const PANEL_PORT = Number(process.env.KVMPANEL_PANEL_PORT ?? 7701);
const PANEL_SOCK = process.env.KVMPANEL_PANEL_SOCK;

const app = await buildServer();
await app.listen({ host: '127.0.0.1', port: HTTP_PORT });
await startPanelServer(app.kvm, PANEL_SOCK ? { path: PANEL_SOCK } : { port: PANEL_PORT });
console.log(`KVMPanel mock: http://127.0.0.1:${HTTP_PORT}  panel socket: ${PANEL_SOCK ?? `tcp://127.0.0.1:${PANEL_PORT}`}`);
