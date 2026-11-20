import Fastify, { type FastifyInstance } from 'fastify';
import cookie from '@fastify/cookie';
import websocket from '@fastify/websocket';
import { createState, type State } from './state.js';

export interface ServerOptions {
  now?: () => number;
}

declare module 'fastify' {
  interface FastifyInstance {
    kvm: State;
  }
}

export async function buildServer(opts: ServerOptions = {}): Promise<FastifyInstance> {
  const app = Fastify({ logger: false });
  const state = createState({ now: opts.now });
  app.decorate('kvm', state);
  await app.register(cookie);
  await app.register(websocket);

  app.get('/api/health', async () => ({ ok: true as const, version: state.version }));

  return app;
}
