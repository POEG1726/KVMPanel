import Fastify, { type FastifyInstance } from 'fastify';
import cookie from '@fastify/cookie';
import websocket from '@fastify/websocket';
import { createState, type State } from './state.js';
import { registerAuth } from './routes/auth.js';
import { registerConfig } from './routes/config.js';

export interface ServerOptions {
  now?: () => number;
  /** Injected so tests can observe login back-off without waiting. */
  sleep?: (ms: number) => Promise<void>;
}

declare module 'fastify' {
  interface FastifyInstance {
    kvm: State;
  }
}

export async function buildServer(opts: ServerOptions = {}): Promise<FastifyInstance> {
  const app = Fastify({ logger: false });
  const state = createState({ now: opts.now, sleep: opts.sleep });
  app.decorate('kvm', state);
  await app.register(cookie);
  await app.register(websocket);

  app.get('/api/health', async () => ({ ok: true as const, version: state.version }));
  registerAuth(app, state);
  registerConfig(app, state);

  return app;
}
