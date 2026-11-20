import type { FastifyInstance } from 'fastify';
import { firstErrorPath, schemaValidator } from '../contract.js';
import type { Config } from '../configDefaults.js';
import { usbBudget } from '../usb.js';
import type { Account, Permission, State } from '../state.js';
import { hasPerm, requirePerm, sendError } from './auth.js';

/** Permission each top-level config section needs to be changed. */
export const SECTION_PERM: Record<string, Permission> = {
  panel: 'panel', function_key: 'panel', usb: 'usb', video: 'console', hid: 'console',
  serial: 'terminal', web: 'device', device: 'device', kvm: 'device',
};

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** Open maps where a null value in a patch deletes the key instead of storing null. */
const DELETABLE_MAPS = new Set(['panel.thresholds']);

/** Recursive object merge; arrays and scalars (including null) replace. */
function merge(target: unknown, patch: unknown, path = ''): unknown {
  if (!isObj(patch) || !isObj(target)) return structuredClone(patch);
  const out: Record<string, unknown> = { ...target };
  for (const [k, v] of Object.entries(patch)) {
    if (v === null && DELETABLE_MAPS.has(path)) delete out[k];
    else out[k] = merge(target[k], v, path ? `${path}.${k}` : k);
  }
  return out;
}

export type PatchResult =
  | { ok: true; config: Config }
  | { ok: false; status: 400 | 403 | 409; error: string; detail?: Record<string, unknown> };

/** Shared by HTTP PATCH /api/config and the panel socket's patch_config. Never mutates on failure. */
export function applyConfigPatch(state: State, patch: unknown, acc: Account | null): PatchResult {
  if (!isObj(patch)) return { ok: false, status: 400, error: 'invalid_config', detail: { path: '' } };
  const next = merge(state.config, patch) as Config;
  const valid = schemaValidator('Config');
  if (!valid(next)) return { ok: false, status: 400, error: 'invalid_config', detail: { path: firstErrorPath(valid) } };
  if (acc) {
    for (const section of Object.keys(patch)) {
      if (!hasPerm(acc, SECTION_PERM[section])) return { ok: false, status: 403, error: 'forbidden' };
    }
  }
  const budget = usbBudget(next.usb as never);
  if (!budget.fits) {
    return { ok: false, status: 409, error: 'endpoints_full', detail: { in_missing: budget.in_missing, out_missing: budget.out_missing } };
  }
  state.config = next;
  state.events.emit('config_changed', next);
  return { ok: true, config: next };
}

export function registerConfig(app: FastifyInstance, state: State) {
  app.get('/api/config', { preHandler: requirePerm(state) }, async () => state.config);

  app.patch('/api/config', { preHandler: requirePerm(state) }, async (req, reply) => {
    const r = applyConfigPatch(state, req.body, req.account!);
    if (!r.ok) return sendError(reply, r.status, r.error, r.detail);
    return r.config;
  });

  app.get('/api/usb/budget', { preHandler: requirePerm(state) }, async () => usbBudget(state.config.usb as never));

  app.get('/api/usb/applied', { preHandler: requirePerm(state) }, async () => ({
    usb: state.usbApplied,
    pending: JSON.stringify(state.usbApplied) !== JSON.stringify(state.config.usb),
  }));

  app.post('/api/usb/apply', { preHandler: requirePerm(state, 'usb') }, async (_req, reply) => {
    state.usbApplied = structuredClone(state.config.usb);
    state.events.emit('usb_reenumerated');
    return reply.code(202).send({ reenumerating: true });
  });
}
