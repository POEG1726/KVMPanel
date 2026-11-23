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

const digest = (key: string) => createHash('sha256').update(key).digest('hex');
function mcpLog(state: State, tool: string, key_name: string | null = null) {
  state.mcpAudit.unshift({ ts: state.now(), key_name, tool, args: {}, result: 'ok', elapsed_ms: 0, count: 1 });
}
export function stopMcp(state: State) {
  state.mcpEnabled = false;
  state.mcpActivity = { active: false, key_name: null, tool: null };
  mcpLog(state, 'stop');
  state.events.emit('status_changed');
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
  const publicKey = ({ hash: _hash, ...key }: State['mcpKeys'] extends Map<string, infer Entry> ? Entry : never) => key;
  const mcpState = () => ({ enabled: state.mcpEnabled, keys: [...state.mcpKeys.values()].map(publicKey), ...state.mcpInputPolicy });
  app.get('/api/mcp', { preHandler: mcp }, async () => mcpState());
  app.put('/api/mcp', { preHandler: [mcp, bodyGuard('putMcp')] }, async (req, reply) => {
    const { enabled } = req.body as { enabled: boolean };
    if (enabled && !state.mcpKeys.size) return sendError(reply, 409, 'no_key');
    if (!enabled) {
      state.mcpActivity = { active: false, key_name: null, tool: null };
      state.events.emit('status_changed');
    }
    state.mcpEnabled = enabled;
    return mcpState();
  });
  app.patch('/api/mcp', { preHandler: [mcp, bodyGuard('patchMcpInputPolicy')] }, async (req) => {
    Object.assign(state.mcpInputPolicy, req.body);
    return mcpState();
  });
  app.post('/api/mcp/keys', { preHandler: [mcp, bodyGuard('createMcpKey')] }, async (req, reply) => {
    if (state.mcpKeys.size >= 8) return sendError(reply, 409, 'key_limit');
    const { name, scopes } = req.body as { name: string; scopes: string[] };
    const key = 'kvp_' + randomBytes(32).toString('base64url');
    const entry = { id: randomUUID(), name, scopes, created: state.now(), last_used: null, hint: key.slice(-4), hash: digest(key) };
    state.mcpKeys.set(entry.id, entry);
    mcpLog(state, 'key_create', name);
    return reply.code(201).send({ key, entry: publicKey(entry) });
  });
  app.patch('/api/mcp/keys/:id', { preHandler: [mcp, bodyGuard('editMcpKey')] }, async (req, reply) => {
    const item = state.mcpKeys.get((req.params as { id: string }).id);
    if (!item) return sendError(reply, 404, 'not_found');
    const { name, scopes } = req.body as { name: string; scopes: string[] };
    Object.assign(item, { name, scopes });
    mcpLog(state, 'key_edit', name);
    return publicKey(item);
  });
  app.delete('/api/mcp/keys/:id', { preHandler: mcp }, async (req, reply) => {
    const id = (req.params as { id: string }).id;
    const key = state.mcpKeys.get(id);
    if (!key) return sendError(reply, 404, 'not_found');
    state.mcpKeys.delete(id);
    if (!state.mcpKeys.size) state.mcpEnabled = false;
    mcpLog(state, 'key_delete', key.name);
    return reply.code(204).send();
  });
  app.post('/api/mcp/stop', { preHandler: requirePerm(state, 'console') }, async (req, reply) => {
    if (req.account?.view_only) return sendError(reply, 403, 'forbidden');
    stopMcp(state);
    return reply.code(204).send();
  });
  app.get('/api/mcp/audit', { preHandler: mcp }, async (req) => {
    const limit = Number((req.query as { limit?: string }).limit ?? 100);
    return { entries: state.mcpAudit.slice(0, Math.max(1, Math.min(1000, limit || 100))) };
  });
  const bearer = (scope: string) => async (req: FastifyRequest, reply: FastifyReply) => {
    if (!state.mcpEnabled || !state.config.kvm.enabled) return sendError(reply, 403, 'mcp_disabled');
    const auth = req.headers.authorization ?? '';
    const key = auth.startsWith('Bearer ') && [...state.mcpKeys.values()].find((key) => key.hash === digest(auth.slice(7)));
    if (!key) return sendError(reply, 401, 'unauthenticated');
    if (!key.scopes.includes(scope)) return sendError(reply, 403, 'forbidden_scope');
    key.last_used = state.now();
  };
  app.get('/api/mcp/v1/screenshot', { preHandler: bearer('observe') }, async (_req, reply) => {
    if (!state.hdmi.signal) return sendError(reply, 503, 'no_signal');
    return reply.type('image/jpeg').send(mockFrame(0));
  });
  app.post('/api/mcp/v1/hid', { preHandler: bearer('input') }, async (req, reply) => {
    const m = req.body as { t?: string };
    if (!validInput(m) || m.t === 'ping') return sendError(reply, 400, 'bad_message');
    const { human_input_priority, human_input_wait_s } = state.mcpInputPolicy;
    if (human_input_priority && state.mcpLastHumanInput !== null
      && state.now() - state.mcpLastHumanInput < human_input_wait_s * 1000
      && state.consoleClients.some(({ user }) => state.accounts.get(user)?.view_only === false)) {
      return sendError(reply, 409, 'human_active');
    }
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
