import type { FastifyInstance } from 'fastify';
import { edid1080p, mockFrame } from '../frames.js';
import type { State } from '../state.js';
import { accountForToken, bodyGuard, COOKIE, hasPerm, requirePerm, sendError } from './auth.js';
import { trackSocket } from '../sockets.js';

const EDID_RE = /^00ffffffffffff00([0-9a-f]{240}|[0-9a-f]{496})$/;

export function registerStream(app: FastifyInstance, state: State) {
  const stops = new Set<() => void>();
  app.addHook('onClose', async () => stops.forEach((stop) => stop()));

  app.get('/api/stream', { websocket: true }, (socket, req) => {
    const acc = accountForToken(state, req.cookies[COOKIE]);
    if (!acc) return socket.close(4401, 'unauthenticated');
    if (!hasPerm(acc, 'console')) return socket.close(4403, 'forbidden');
    trackSocket(state, socket, req.cookies[COOKIE]!, 'console');

    let paused = false;
    let frame = 0;
    let lastSent = -1;
    let timer: NodeJS.Timeout;
    const tick = () => {
      const fps = Number(state.config.video.fps_max);
      timer = setTimeout(tick, 1000 / fps);
      if (paused || !state.hdmi.signal || socket.readyState !== 1) return;
      // Backpressure: drop the frame while the previous one is still in flight.
      if (socket.bufferedAmount > 0) return;
      frame += 1;
      // Frame-difference skip: identical consecutive frames are not resent.
      if (state.config.video.frame_diff && frame === lastSent) return;
      socket.send(mockFrame(frame), { binary: true });
      lastSent = frame;
    };
    const stop = () => clearTimeout(timer);
    stops.add(stop);
    tick();
    socket.on('close', () => { stop(); stops.delete(stop); });
    socket.on('message', (data, isBinary) => {
      if (isBinary) return;
      try {
        const m = JSON.parse(data.toString());
        if (m?.t === 'pause') paused = true;
        if (m?.t === 'resume') {
          paused = false;
          // Contract: resume always pushes the current frame at once, regardless of frame_diff.
          if (state.hdmi.signal && socket.readyState === 1) socket.send(mockFrame(frame), { binary: true });
        }
      } catch { /* ignore */ }
    });
  });

  app.get('/api/stream/snapshot.jpg', { preHandler: requirePerm(state, 'console') }, async (_req, reply) => {
    if (!state.hdmi.signal) return sendError(reply, 503, 'no_signal');
    return reply.type('image/jpeg').send(mockFrame(0));
  });

  app.get('/api/video/edid', { preHandler: requirePerm(state, 'console') }, async () => state.edid);

  app.put('/api/video/edid', { preHandler: [requirePerm(state, 'device'), bodyGuard('putEdid')] }, async (req, reply) => {
    const hex = String((req.body as { hex: string }).hex).toLowerCase();
    if (!EDID_RE.test(hex)) return sendError(reply, 400, 'bad_edid');
    state.edid = { hex, preset: hex === edid1080p() ? '1080p' : 'custom' };
    return state.edid;
  });

  app.post('/api/video/edid/reset', { preHandler: requirePerm(state, 'device') }, async () => {
    state.edid = { hex: edid1080p(), preset: '1080p' };
    return state.edid;
  });
}
