import type { FastifyInstance } from 'fastify';
import type { State } from '../state.js';
import { accountForToken, COOKIE, hasPerm, requirePerm, sendError } from './auth.js';
import { trackSocket } from '../sockets.js';

const PROMPT = 'root@kvmpanel:~# ';
const SCRIPT_NAME = /^[A-Za-z0-9._-]{1,64}$/;
const LS_OUTPUT = 'data  etc  images  root\r\n';

export function registerTerminal(app: FastifyInstance, state: State) {
  app.get('/api/terminal', { websocket: true }, (socket, req) => {
    const acc = accountForToken(state, req.cookies[COOKIE]);
    if (!acc) return socket.close(4401, 'unauthenticated');
    if (!hasPerm(acc, 'terminal')) return socket.close(4403, 'forbidden');
    const kind = (req.query as { kind?: string }).kind ?? 'local';
    if (kind !== 'local' && kind !== 'serial') return socket.close(4400, 'bad_kind');
    if (kind === 'serial' && !state.config.serial.device) return socket.close(4404, 'no_serial_device');
    trackSocket(state, socket, req.cookies[COOKIE]!, 'terminal');

    const out = (s: string) => socket.readyState === 1 && socket.send(Buffer.from(s, 'utf8'), { binary: true });
    let line = '';
    out(kind === 'serial' ? `[serial ${state.config.serial.device} ${state.config.serial.baud}]\r\n` : `KVMPanel mock shell\r\n${PROMPT}`);
    socket.on('message', (data, isBinary) => {
      if (!isBinary) return; // {t:'resize',cols,rows}: a PTY would be resized; the mock has nothing to do
      for (const ch of (data as Buffer).toString('utf8')) {
        if (ch === '\r') {
          const cmd = line.trim();
          line = '';
          out('\r\n');
          if (cmd === 'ls') out(LS_OUTPUT);
          else if (cmd) out(`${cmd}: command not found in mock\r\n`);
          out(PROMPT);
        } else if (ch === '\x7f') {
          if (line) { line = line.slice(0, -1); out('\b \b'); }
        } else {
          line += ch;
          out(ch);
        }
      }
    });
  });

  const term = requirePerm(state, 'terminal');
  const nameOf = (req: { params: unknown }) => (req.params as { name: string }).name;

  app.get('/api/scripts', { preHandler: term }, async () =>
    [...state.scripts.entries()].map(([name, body]) => ({ name, size: Buffer.byteLength(body) })));

  app.put('/api/scripts/:name', { preHandler: term }, async (req, reply) => {
    const name = nameOf(req);
    if (!SCRIPT_NAME.test(name)) return sendError(reply, 400, 'bad_name');
    if (typeof req.body !== 'string') return sendError(reply, 400, 'invalid_body', { path: '' });
    state.scripts.set(name, req.body);
    return reply.code(201).send({ name, size: Buffer.byteLength(req.body) });
  });

  app.delete('/api/scripts/:name', { preHandler: term }, async (req, reply) => {
    const name = nameOf(req);
    if (!SCRIPT_NAME.test(name)) return sendError(reply, 400, 'bad_name');
    if (!state.scripts.delete(name)) return sendError(reply, 404, 'not_found');
    return reply.code(204).send();
  });

  app.post('/api/scripts/:name/run', { preHandler: term }, async (req, reply) => {
    const name = nameOf(req);
    if (!SCRIPT_NAME.test(name)) return sendError(reply, 400, 'bad_name');
    const body = state.scripts.get(name);
    if (body === undefined) return sendError(reply, 404, 'not_found');
    // The mock only interprets `echo`; kvmd runs the script with sh and a timeout.
    const output = body.split('\n').filter((l) => l.startsWith('echo ')).map((l) => l.slice(5)).join('\n') + '\n';
    return { exit_code: 0, output };
  });
}
