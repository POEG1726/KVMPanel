import Fastify, { type FastifyInstance } from 'fastify';
import cookie from '@fastify/cookie';
import websocket from '@fastify/websocket';
import { createState, type State } from './state.js';
import { registerAuth } from './routes/auth.js';
import { registerConfig } from './routes/config.js';
import { registerWs } from './routes/ws.js';
import { registerStream } from './routes/stream.js';
import { registerPower } from './routes/power.js';
import { registerFiles } from './routes/files.js';
import { registerMedia } from './routes/media.js';
import { registerTerminal } from './routes/terminal.js';
import { registerSystem } from './routes/system.js';
import { installSocketPolicy } from './sockets.js';

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
  registerWs(app, state);
  registerStream(app, state);
  registerPower(app, state);
  registerFiles(app, state);
  registerMedia(app, state);
  registerTerminal(app, state);
  registerSystem(app, state);
  installSocketPolicy(state);

  // Keep every error body on the contract's Error shape.
  app.setNotFoundHandler((_req, reply) => reply.code(404).send({ error: 'not_found' }));
  app.setErrorHandler((err: { code?: string; statusCode?: number }, _req, reply) => {
    if (err.code === 'FST_ERR_CTP_INVALID_JSON_BODY' || err.code === 'FST_ERR_CTP_EMPTY_JSON_BODY') return reply.code(400).send({ error: 'invalid_json' });
    if (err.code === 'FST_ERR_CTP_INVALID_MEDIA_TYPE') return reply.code(415).send({ error: 'unsupported_type' });
    if (err.code === 'FST_ERR_CTP_BODY_TOO_LARGE') return reply.code(413).send({ error: 'too_large' });
    if (err.statusCode && err.statusCode < 500) return reply.code(err.statusCode).send({ error: 'bad_request' });
    return reply.code(500).send({ error: 'internal' });
  });

  return app;
}
