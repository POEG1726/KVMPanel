/** Tiny in-memory file tree used by the mock for the sd, share and system roots. */

export type Root = 'sd' | 'share' | 'system';
export const ROOTS: Root[] = ['sd', 'share', 'system'];

export interface FileNode { type: 'file'; size: number; mtime: number; content: Buffer | null }
export interface DirNode { type: 'dir'; mtime: number; children: Map<string, Node> }
export type Node = FileNode | DirNode;

export interface Volume { total: number; tree: DirNode }

const GB = 1e9;

export function dir(entries: Record<string, Node> = {}, mtime = Date.parse('2026-11-01T10:00:00Z')): DirNode {
  return { type: 'dir', mtime, children: new Map(Object.entries(entries)) };
}
export function file(size: number, isoDate: string, content: Buffer | null = null): FileNode {
  return { type: 'file', size, mtime: Date.parse(isoDate), content };
}

/** The volumes from the design mockup: SD 58 GB with 23.4 GB used. */
export function seedVolumes(): Record<Root, Volume> {
  return {
    sd: {
      total: 58 * GB,
      tree: dir({
        images: dir({
          drivers: dir({ 'chipset.zip': file(0.002 * GB, '2026-11-02T14:20:00Z') }),
          'ubuntu-24.04.iso': file(5.8 * GB, '2026-11-18T21:04:00Z'),
          'win11-24h2.iso': file(5.4 * GB, '2026-11-12T09:31:00Z'),
          'bios-F12a.zip': file(0.032 * GB, '2026-10-30T22:15:00Z'),
          'notes.txt': file(2000, '2026-10-28T08:02:00Z', Buffer.alloc(2000, 'n')),
        }),
        backup: dir({ 'pc-2026-10.img': file(23.4 * GB - (5.8 + 5.4 + 0.032 + 0.002) * GB - 2000, '2026-10-15T03:00:00Z') }),
      }),
    },
    share: { total: 8 * GB, tree: dir({ 'readme.txt': file(64, '2026-11-01T10:00:00Z', Buffer.alloc(64, 'r')) }) },
    system: {
      total: 0.5 * GB,
      tree: dir({
        etc: dir({ hostname: file(9, '2026-11-20T08:00:00Z', Buffer.from('kvmpanel\n')) }),
        data: dir({ kvmd: dir() }),
        root: dir(),
      }),
    },
  };
}

export function usedBytes(n: Node): number {
  if (n.type === 'file') return n.size;
  let sum = 0;
  for (const c of n.children.values()) sum += usedBytes(c);
  return sum;
}

/**
 * Splits a root-relative POSIX path. sd/share reject `..` segments and relative paths;
 * system accepts any absolute path (normalised).
 */
export function splitPath(root: Root, path: string): string[] | null {
  if (!path.startsWith('/')) return null;
  const parts = path.split('/').filter((p) => p !== '' && p !== '.');
  if (root !== 'system') return parts.includes('..') ? null : parts;
  const out: string[] = [];
  for (const p of parts) {
    if (p === '..') out.pop();
    else out.push(p);
  }
  return out;
}

export function lookup(vol: Volume, parts: string[]): Node | undefined {
  let cur: Node = vol.tree;
  for (const p of parts) {
    if (cur.type !== 'dir') return undefined;
    const next = cur.children.get(p);
    if (!next) return undefined;
    cur = next;
  }
  return cur;
}

/** Parent directory and leaf name, or undefined if the parent is missing. */
export function parentOf(vol: Volume, parts: string[]): { parent: DirNode; name: string } | undefined {
  if (parts.length === 0) return undefined;
  const parent = lookup(vol, parts.slice(0, -1));
  if (!parent || parent.type !== 'dir') return undefined;
  return { parent, name: parts[parts.length - 1] };
}
