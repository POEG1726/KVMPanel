import { createHash, randomBytes, randomUUID } from 'node:crypto';
import type { Readable } from 'node:stream';
import type { FastifyInstance } from 'fastify';
import type { State } from '../state.js';
import { bodyGuard, requirePerm, sendError } from './auth.js';

const LOGO_MAX = 256 * 1024;
const TAILSCALE_ACTIONS = ['install', 'uninstall', 'up', 'down', 'login', 'logout'];

export function registerSystem(app: FastifyInstance, state: State) {
  const startedAt = state.now();

  app.addContentTypeParser(['image/png', 'image/svg+xml'], { parseAs: 'buffer', bodyLimit: 4 * LOGO_MAX }, (_req, body, done) => done(null, body));

  app.get('/api/device', { preHandler: requirePerm(state, 'device') }, async () => ({
    hostname: state.config.device.hostname,
    version: state.version,
    kernel: 'Linux 6.18.64',
    uptime_s: Math.floor((state.now() - startedAt) / 1000) + 273_600,
    ip: '192.168.1.50',
    mac: '02:1a:7c:3e:51:9d',
    link_mbps: 1000,
    soc_temp_c: 48,
    mem: { used: 138e6, total: 512e6 },
    data: { used: 1.2e9, total: 3.4e9 },
    lux: 212,
  }));

  app.post('/api/device/action', { preHandler: [requirePerm(state, 'device'), bodyGuard('deviceAction')] }, async (req, reply) => {
    const { action } = req.body as { action: string };
    reply.code(202).send();
    // Effects happen after the reply, as on the device.
    if (action === 'restart_kvm' || action === 'disconnect_sessions') setImmediate(() => state.events.emit('kvm_restarting'));
    if (action === 'reset_hid') setImmediate(() => state.events.emit('usb_reenumerated'));
    return reply;
  });

  app.get('/api/network', { preHandler: requirePerm(state, 'network') }, async () => state.network);

  app.put('/api/network', { preHandler: [requirePerm(state, 'network'), bodyGuard('putNetwork')] }, async (req) => {
    state.network = structuredClone(req.body) as State['network'];
    return state.network;
  });

  app.get('/api/tailscale', { preHandler: requirePerm(state, 'tailscale') }, async () => ({ installed: true, running: false }));

  app.post('/api/tailscale/:action', { preHandler: requirePerm(state, 'tailscale') }, async (req, reply) => {
    if (!TAILSCALE_ACTIONS.includes((req.params as { action: string }).action)) return sendError(reply, 400, 'bad_action');
    return reply.code(202).send();
  });

  const updates = requirePerm(state, 'updates');
  app.get('/api/update', { preHandler: updates }, async () => ({
    current: state.version, latest: '0.1.1', channel: 'stable', server: 'https://updates.example.invalid/kvmpanel',
  }));
  app.post('/api/update/check', { preHandler: updates }, async () => ({ current: state.version, latest: '0.1.1' }));
  app.post('/api/update/apply', { preHandler: updates }, async (_req, reply) =>
    reply.code(202).send({ job: { id: randomUUID(), state: 'running', progress: 0 } }));
  app.put('/api/update/offline', { preHandler: updates }, async (req, reply) => {
    const expected = req.headers['x-sha256']?.toString().toLowerCase();
    if (!expected || !/^[0-9a-f]{64}$/.test(expected)) return sendError(reply, 400, 'invalid_body', { path: '/x-sha256' });
    const hash = createHash('sha256');
    for await (const chunk of req.body as Readable) hash.update(chunk as Buffer);
    if (hash.digest('hex') !== expected) return sendError(reply, 422, 'checksum_mismatch');
    return reply.code(202).send({ job: { id: randomUUID(), state: 'running', progress: 0 } });
  });

  app.get('/api/mcp', { preHandler: requirePerm(state, 'mcp') }, async () => ({
    enabled: state.mcpKey !== null, key_hint: state.mcpKey ? state.mcpKey.slice(-4) : null,
  }));
  app.post('/api/mcp/regenerate', { preHandler: requirePerm(state, 'mcp') }, async () => {
    state.mcpKey = randomBytes(24).toString('base64url');
    return { key: state.mcpKey };
  });

  app.get('/api/about', async () => ({ name: 'KVMPanel' as const, version: state.version, licenses_url: '/licenses.txt' }));

  app.get('/api/branding/logo', async (_req, reply) => {
    if (!state.logo) return sendError(reply, 404, 'not_found');
    return reply.type(state.logo.type).send(state.logo.data);
  });
  app.put('/api/branding/logo', { preHandler: requirePerm(state, 'device') }, async (req, reply) => {
    const data = req.body as Buffer;
    if (!Buffer.isBuffer(data)) return sendError(reply, 415, 'unsupported_type');
    if (data.length > LOGO_MAX) return sendError(reply, 413, 'too_large');
    state.logo = { type: req.headers['content-type']!.split(';')[0], data };
    return reply.code(204).send();
  });
}
