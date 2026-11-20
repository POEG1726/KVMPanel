import { createHash } from 'node:crypto';
import type { FastifyInstance, FastifyReply, FastifyRequest } from 'fastify';
import type { Readable } from 'node:stream';
import { file as fileNode, dir as dirNode, lookup, parentOf, ROOTS, splitPath, usedBytes, type Root } from '../memfs.js';
import type { State } from '../state.js';
import { bodyGuard, requirePerm, sendError } from './auth.js';

type Target = { root: Root; parts: string[]; path: string };

/** Validates root/path and the caller's right to touch that root; replies with the error itself. */
export function resolveTarget(state: State, req: FastifyRequest, reply: FastifyReply, root: unknown, path: unknown): Target | null {
  if (typeof root !== 'string' || !ROOTS.includes(root as Root) || typeof path !== 'string') {
    sendError(reply, 400, 'bad_path');
    return null;
  }
  if (root === 'system' && req.account!.role !== 'admin') {
    sendError(reply, 403, 'forbidden');
    return null;
  }
  const parts = splitPath(root as Root, path);
  if (!parts) {
    sendError(reply, 400, 'bad_path');
    return null;
  }
  return { root: root as Root, parts, path: '/' + parts.join('/') };
}

export function isMounted(state: State, t: Target): boolean {
  const m = state.mediaMount;
  return !!m && m.root === t.root && m.path === t.path;
}

/** True when the mounted image is `t` itself or lives under `t`. */
function holdsMount(state: State, t: Target): boolean {
  const m = state.mediaMount;
  if (!m || m.root !== t.root) return false;
  return m.path === t.path || t.path === '/' || m.path.startsWith(t.path + '/');
}

function shareBusy(state: State, reply: FastifyReply, t: Target): boolean {
  if (t.root === 'share' && state.share.exported_to_host) {
    sendError(reply, 409, 'share_busy');
    return true;
  }
  return false;
}

const free = (state: State, root: Root) => state.volumes[root].total - usedBytes(state.volumes[root].tree);

export function registerFiles(app: FastifyInstance, state: State) {
  const files = requirePerm(state, 'files');

  // Uploads are raw streams; the handler consumes the payload itself.
  app.addContentTypeParser('application/octet-stream', (_req, payload, done) => done(null, payload));

  app.get('/api/storage', { preHandler: files }, async () =>
    ROOTS.map((root) => ({ root, total: state.volumes[root].total, free: free(state, root), mounted: true })));

  app.get('/api/files', { preHandler: files }, async (req, reply) => {
    const q = req.query as Record<string, unknown>;
    const t = resolveTarget(state, req, reply, q.root, q.path);
    if (!t) return reply;
    const node = lookup(state.volumes[t.root], t.parts);
    if (!node || node.type !== 'dir') return sendError(reply, 404, 'not_found');
    const entries = [...node.children.entries()]
      .sort(([a, x], [b, y]) => (x.type === y.type ? a.localeCompare(b) : x.type === 'dir' ? -1 : 1))
      .map(([name, n]) => {
        const e: Record<string, unknown> = { name, type: n.type, size: n.type === 'file' ? n.size : 0, mtime: n.mtime };
        if (n.type === 'file' && isMounted(state, { ...t, path: `${t.path === '/' ? '' : t.path}/${name}` })) e.mounted = true;
        return e;
      });
    return { entries };
  });

  app.get('/api/files/download', { preHandler: files }, async (req, reply) => {
    const q = req.query as Record<string, unknown>;
    const t = resolveTarget(state, req, reply, q.root, q.path);
    if (!t) return reply;
    const node = lookup(state.volumes[t.root], t.parts);
    if (!node || node.type !== 'file') return sendError(reply, 404, 'not_found');
    reply.header('content-disposition', `attachment; filename*=UTF-8''${encodeURIComponent(t.parts.at(-1)!)}`);
    // Seeded large images have no bytes in the mock; they download as an empty placeholder.
    if (!node.content) reply.header('x-mock-placeholder', 'true');
    return reply.type('application/octet-stream').send(node.content ?? Buffer.alloc(0));
  });

  app.put('/api/files/upload', { preHandler: files }, async (req, reply) => {
    const q = req.query as Record<string, unknown>;
    const t = resolveTarget(state, req, reply, q.root, q.path);
    if (!t) return reply;
    if (shareBusy(state, reply, t)) return reply;
    const vol = state.volumes[t.root];
    const at = parentOf(vol, t.parts);
    if (!at) return sendError(reply, 404, 'not_found');
    if (at.parent.children.get(at.name)?.type === 'dir') return sendError(reply, 409, 'is_dir');
    if (isMounted(state, t)) return sendError(reply, 409, 'in_use');
    const declared = Number(req.headers['content-length'] ?? 0);
    if (declared > free(state, t.root)) {
      reply.header('connection', 'close');
      return sendError(reply, 507, 'no_space');
    }
    const expected = req.headers['x-sha256']?.toString().toLowerCase();
    const chunks: Buffer[] = [];
    const hash = createHash('sha256');
    // Bytes of the file being replaced are freed by the write.
    const old = at.parent.children.get(at.name);
    const budget = free(state, t.root) + (old?.type === 'file' ? old.size : 0);
    let received = 0;
    try {
      for await (const chunk of req.body as Readable) {
        received += (chunk as Buffer).length;
        if (received > budget) {
          reply.header('connection', 'close');
          return sendError(reply, 507, 'no_space');
        }
        hash.update(chunk as Buffer);
        chunks.push(chunk as Buffer);
      }
    } catch {
      return reply; // client went away: nothing is committed
    }
    if (req.raw.aborted) return reply;
    if (expected && hash.digest('hex') !== expected) return sendError(reply, 422, 'checksum_mismatch');
    const content = Buffer.concat(chunks);
    at.parent.children.set(at.name, fileNode(content.length, new Date(state.now()).toISOString(), content));
    return reply.code(201).send({ name: at.name, type: 'file', size: content.length, mtime: state.now() });
  });

  const write = (op: string) => ({ preHandler: [files, bodyGuard(op)] });

  app.post('/api/files/mkdir', write('makeDir'), async (req, reply) => {
    const b = req.body as { root: Root; path: string };
    const t = resolveTarget(state, req, reply, b.root, b.path);
    if (!t) return reply;
    if (shareBusy(state, reply, t)) return reply;
    const at = parentOf(state.volumes[t.root], t.parts);
    if (!at) return sendError(reply, 404, 'not_found');
    if (at.parent.children.has(at.name)) return sendError(reply, 409, 'exists');
    at.parent.children.set(at.name, dirNode({}, state.now()));
    return reply.code(201).send({ name: at.name, type: 'dir', size: 0, mtime: state.now() });
  });

  app.post('/api/files/rename', write('renamePath'), async (req, reply) => {
    const b = req.body as { root: Root; path: string; to: string };
    const t = resolveTarget(state, req, reply, b.root, b.path);
    if (!t) return reply;
    const to = resolveTarget(state, req, reply, b.root, b.to);
    if (!to) return reply;
    if (shareBusy(state, reply, t)) return reply;
    if (holdsMount(state, t)) return sendError(reply, 409, 'in_use');
    if (to.path === t.path || to.path.startsWith(t.path + '/')) return sendError(reply, 400, 'bad_path');
    const vol = state.volumes[t.root];
    const from = parentOf(vol, t.parts);
    const dest = parentOf(vol, to.parts);
    if (!from || !from.parent.children.has(from.name) || !dest) return sendError(reply, 404, 'not_found');
    if (dest.parent.children.has(dest.name)) return sendError(reply, 409, 'exists');
    dest.parent.children.set(dest.name, from.parent.children.get(from.name)!);
    from.parent.children.delete(from.name);
    return reply.code(204).send();
  });

  app.post('/api/files/delete', write('deletePath'), async (req, reply) => {
    const b = req.body as { root: Root; path: string };
    const t = resolveTarget(state, req, reply, b.root, b.path);
    if (!t) return reply;
    if (shareBusy(state, reply, t)) return reply;
    if (holdsMount(state, t)) return sendError(reply, 409, 'in_use');
    const at = parentOf(state.volumes[t.root], t.parts);
    if (!at || !at.parent.children.has(at.name)) return sendError(reply, 404, 'not_found');
    at.parent.children.delete(at.name);
    return reply.code(204).send();
  });

  app.get('/api/share', { preHandler: files }, async () => ({ ...state.share }));

  app.post('/api/share', { preHandler: [requirePerm(state, 'media'), bodyGuard('setShare')] }, async (req) => {
    state.share.exported_to_host = (req.body as { exported_to_host: boolean }).exported_to_host;
    return { ...state.share };
  });
}
