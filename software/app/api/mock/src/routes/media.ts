import { randomUUID } from 'node:crypto';
import type { FastifyInstance } from 'fastify';
import { file as fileNode, lookup, parentOf, splitPath } from '../memfs.js';
import type { DownloadJob, State } from '../state.js';
import { bodyGuard, requirePerm, sendError } from './auth.js';

const MOCK_JOB_BYTES = 64 * 1024 * 1024;
const TICK_MS = 250;
const TICK_BYTES = 8 * 1024 * 1024;

function setMedia(state: State, m: State['mediaMount']) {
  state.mediaMount = m;
  state.media = m ? { mounted: m.path.split('/').at(-1)!, mode: m.mode } : { mounted: null, mode: null };
  state.events.emit('status_changed');
}

export function registerMedia(app: FastifyInstance, state: State) {
  const media = requirePerm(state, 'media');
  const timers = new Set<NodeJS.Timeout>();
  app.addHook('onClose', async () => timers.forEach(clearInterval));

  app.get('/api/media', { preHandler: media }, async () => ({
    mounted: state.mediaMount ? { root: state.mediaMount.root, path: state.mediaMount.path, mode: state.mediaMount.mode } : null,
    readonly: state.mediaMount ? state.mediaMount.readonly : true,
  }));

  app.post('/api/media/mount', { preHandler: [media, bodyGuard('mountMedia')] }, async (req, reply) => {
    const b = req.body as { root: 'sd'; path: string; mode: 'cdrom' | 'disk'; readonly?: boolean };
    const parts = splitPath('sd', b.path);
    if (!parts) return sendError(reply, 400, 'bad_path');
    if (state.mediaMount) return sendError(reply, 409, 'already_mounted');
    const path = '/' + parts.join('/');
    const node = lookup(state.volumes.sd, parts);
    if (!node || node.type !== 'file') return sendError(reply, 404, 'not_found');
    const ext = path.toLowerCase().split('.').at(-1);
    // CD-ROM mode presents ISO 9660 / UDF only; disk mode accepts ISO and raw IMG.
    if (!(ext === 'iso' || (ext === 'img' && b.mode === 'disk'))) return sendError(reply, 422, 'bad_image');
    const readonly = b.mode === 'cdrom' ? true : b.readonly ?? true;
    setMedia(state, { root: 'sd', path, mode: b.mode, readonly });
    return { mounted: { root: 'sd', path, mode: b.mode }, readonly };
  });

  app.post('/api/media/eject', { preHandler: media }, async (_req, reply) => {
    setMedia(state, null);
    return reply.code(204).send();
  });

  app.post('/api/media/download', { preHandler: [media, bodyGuard('startDownload')] }, async (req, reply) => {
    const b = req.body as { url: string; path: string; sha256?: string };
    let url: URL;
    try { url = new URL(b.url); } catch { return sendError(reply, 400, 'bad_url'); }
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return sendError(reply, 400, 'bad_url');
    const parts = splitPath('sd', b.path);
    if (!parts) return sendError(reply, 400, 'bad_path');
    const at = parentOf(state.volumes.sd, parts);
    if (!at) return sendError(reply, 404, 'not_found');
    if (at.parent.children.has(at.name)) return sendError(reply, 409, 'exists');
    const job: DownloadJob = {
      id: randomUUID(), url: b.url, path: '/' + parts.join('/'), state: 'running', received: 0, total: MOCK_JOB_BYTES,
    };
    state.jobs.push(job);
    const timer = setInterval(() => {
      if (job.state !== 'running') return;
      job.received = Math.min(job.total, job.received + TICK_BYTES);
      if (job.received === job.total) {
        job.state = 'done';
        at.parent.children.set(at.name, fileNode(job.total, new Date(state.now()).toISOString()));
        clearInterval(timer);
        timers.delete(timer);
      }
    }, TICK_MS);
    timer.unref();
    timers.add(timer);
    return reply.code(202).send({ job });
  });

  app.get('/api/media/jobs', { preHandler: media }, async () => ({ jobs: state.jobs }));

  app.post('/api/media/jobs/:id/:action', { preHandler: media }, async (req, reply) => {
    const { id, action } = req.params as { id: string; action: string };
    const job = state.jobs.find((j) => j.id === id);
    if (!job) return sendError(reply, 404, 'not_found');
    const allowed: Record<string, DownloadJob['state'][]> = {
      pause: ['running'], resume: ['paused'], cancel: ['running', 'paused'],
    };
    if (!allowed[action]) return sendError(reply, 404, 'not_found');
    if (!allowed[action].includes(job.state)) return sendError(reply, 409, 'bad_state');
    job.state = action === 'pause' ? 'paused' : action === 'resume' ? 'running' : 'canceled';
    return job;
  });
}
