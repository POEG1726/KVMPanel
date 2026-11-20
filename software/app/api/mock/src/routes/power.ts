import type { FastifyInstance } from 'fastify';
import type { State } from '../state.js';
import { bodyGuard, requirePerm, sendError } from './auth.js';

export const LONG_PRESS_MS = 4000;

/** Normalises aa-bb-cc-dd-ee-ff / AA:BB:… to lowercase colon form, or null if malformed. */
export function normalizeMac(raw: string): string | null {
  const m = /^([0-9a-f]{2})[:-]([0-9a-f]{2})[:-]([0-9a-f]{2})[:-]([0-9a-f]{2})[:-]([0-9a-f]{2})[:-]([0-9a-f]{2})$/i.exec(raw.trim());
  return m ? m.slice(1).join(':').toLowerCase() : null;
}

export function registerPower(app: FastifyInstance, state: State) {
  app.get('/api/power', { preHandler: requirePerm(state, 'console') }, async () => ({ ...state.power }));

  app.post('/api/power/press', { preHandler: [requirePerm(state, 'power'), bodyGuard('pressPower')] }, async (req, reply) => {
    const { button, duration_ms } = req.body as { button: 'power' | 'reset'; duration_ms: number };
    if (button === 'power') {
      state.power.on = duration_ms >= LONG_PRESS_MS ? false : !state.power.on;
    }
    state.events.emit('status_changed');
    return reply.code(204).send();
  });

  app.get('/api/wol/devices', { preHandler: requirePerm(state, 'power') }, async () => ({ devices: [...state.wol] }));

  app.post('/api/wol/devices', { preHandler: [requirePerm(state, 'power'), bodyGuard('addWolDevice')] }, async (req, reply) => {
    const b = req.body as { name: string; mac: string };
    const mac = normalizeMac(b.mac);
    if (!mac) return sendError(reply, 400, 'bad_mac');
    if (state.wol.some((d) => d.mac === mac)) return sendError(reply, 409, 'device_exists');
    const dev = { name: b.name, mac };
    state.wol.push(dev);
    return reply.code(201).send(dev);
  });

  app.delete('/api/wol/devices/:mac', { preHandler: requirePerm(state, 'power') }, async (req, reply) => {
    const mac = normalizeMac((req.params as { mac: string }).mac);
    const i = mac ? state.wol.findIndex((d) => d.mac === mac) : -1;
    if (i < 0) return sendError(reply, 404, 'not_found');
    state.wol.splice(i, 1);
    return reply.code(204).send();
  });

  app.post('/api/wol/wake', { preHandler: [requirePerm(state, 'power'), bodyGuard('wakeOnLan')] }, async (req, reply) => {
    if (!normalizeMac((req.body as { mac: string }).mac)) return sendError(reply, 400, 'bad_mac');
    return reply.code(204).send();
  });
}
