import { createHash, randomBytes, randomUUID } from 'node:crypto';
import type { Readable } from 'node:stream';
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify';
import { messageValidator } from '../contract.js';
import { mockFrame } from '../frames.js';
import type { State } from '../state.js';
import { bodyGuard, requirePerm, sendError } from './auth.js';

const LOGO_MAX = 256 * 1024;
const validInput = messageValidator('ws.schema.json', 'ClientMessage');
const TAILSCALE_ACTIONS = ['install', 'uninstall', 'up', 'down', 'login', 'logout'];

/** Device info shared by GET /api/device and the panel's device_info request. */
export function deviceInfo(state: State) {
  return {
    hostname: String(state.config.device.hostname),
    version: state.version,
    kernel: 'Linux 6.18.64',
    uptime_s: Math.floor((state.now() - state.startedAt) / 1000) + 273_600,
    ip: '192.168.1.50',
    mac: '02:1a:7c:3e:51:9d',
    link_mbps: 1000,
    soc_temp_c: 48,
    mem: { used: 138e6, total: 512e6 },
    data: { used: 1.2e9, total: 3.4e9 },
    lux: 212,
  };
}

export function registerSystem(app: FastifyInstance, state: State) {
  app.addContentTypeParser(['image/png', 'image/svg+xml'], { parseAs: 'buffer', bodyLimit: 4 * LOGO_MAX }, (_req, body, done) => done(null, body));

  app.get('/api/device', { preHandler: requirePerm(state, 'device') }, async () => deviceInfo(state));

  app.post('/api/device/action', { preHandler: [requirePerm(state, 'device'), bodyGuard('deviceAction')] }, async (req, reply) => {
    const { action } = req.body as { action: string };
    reply.code(202).send();
    // Effects happen after the reply, as on the device.
    if (action === 'restart_kvm') setImmediate(() => state.events.emit('kvm_restarting'));
    if (action === 'disconnect_sessions') setImmediate(() => state.events.emit('disconnect_sessions'));
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
  const newJob = () => {
    const job = { id: randomUUID(), state: 'running' as const, progress: 0 };
    state.updateJobs.set(job.id, job);
    return job;
  };
  app.get('/api/update', { preHandler: updates }, async () => ({ current: state.version, latest: '0.1.1', ...state.update }));
  app.put('/api/update/settings', { preHandler: [updates, bodyGuard('putUpdateSettings')] }, async (req) => {
    state.update = { ...(req.body as State['update']) };
    return state.update;
  });
  app.post('/api/update/check', { preHandler: updates }, async () => ({ current: state.version, latest: '0.1.1' }));
  app.post('/api/update/apply', { preHandler: updates }, async (_req, reply) => reply.code(202).send({ job: newJob() }));
  app.get('/api/update/jobs/:id', { preHandler: updates }, async (req, reply) => {
    const job = state.updateJobs.get((req.params as { id: string }).id);
    return job ? { job } : sendError(reply, 404, 'not_found');
  });
  app.put('/api/update/offline', { preHandler: updates }, async (req, reply) => {
    const expected = req.headers['x-sha256']?.toString().toLowerCase();
    if (!expected || !/^[0-9a-f]{64}$/.test(expected)) return sendError(reply, 400, 'invalid_body', { path: '/x-sha256' });
    const hash = createHash('sha256');
    for await (const chunk of req.body as Readable) hash.update(chunk as Buffer);
    if (hash.digest('hex') !== expected) return sendError(reply, 422, 'checksum_mismatch');
    return reply.code(202).send({ job: newJob() });
  });

  const mcp = requirePerm(state, 'mcp');
  const mcpState = () => ({ enabled: state.mcpEnabled, key_hint: state.mcpKey ? state.mcpKey.slice(-4) : null });
  app.get('/api/mcp', { preHandler: mcp }, async () => mcpState());
  app.put('/api/mcp', { preHandler: [mcp, bodyGuard('putMcp')] }, async (req, reply) => {
    const { enabled } = req.body as { enabled: boolean };
    if (enabled && !state.mcpKey) return sendError(reply, 409, 'no_key');
    state.mcpEnabled = enabled;
    return mcpState();
  });
  app.post('/api/mcp/regenerate', { preHandler: mcp }, async () => {
    state.mcpKey = randomBytes(24).toString('base64url');
    state.mcpEnabled = true;
    return { key: state.mcpKey };
  });
  app.delete('/api/mcp/key', { preHandler: mcp }, async (_req, reply) => {
    state.mcpKey = null;
    state.mcpEnabled = false;
    return reply.code(204).send();
  });
  const bearer = async (req: FastifyRequest, reply: FastifyReply) => {
    const auth = req.headers.authorization ?? '';
    if (!state.mcpEnabled || !state.mcpKey || auth !== `Bearer ${state.mcpKey}`) return sendError(reply, 401, 'unauthenticated');
  };
  app.get('/api/mcp/v1/screenshot', { preHandler: bearer }, async (_req, reply) => {
    if (!state.hdmi.signal) return sendError(reply, 503, 'no_signal');
    return reply.type('image/jpeg').send(mockFrame(0));
  });
  app.post('/api/mcp/v1/hid', { preHandler: bearer }, async (req, reply) => {
    const m = req.body as { t?: string };
    if (!validInput(m) || m.t === 'ping') return sendError(reply, 400, 'bad_message');
    return reply.code(204).send();
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
