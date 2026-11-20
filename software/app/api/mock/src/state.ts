import { EventEmitter } from 'node:events';
import { randomBytes, scryptSync, timingSafeEqual } from 'node:crypto';
import { contract } from './contract.js';
import { defaultConfig, type Config } from './configDefaults.js';
import { edid1080p } from './frames.js';

export const ALL_PERMISSIONS = [
  'console', 'power', 'media', 'terminal', 'files', 'device',
  'network', 'updates', 'mcp', 'tailscale', 'panel', 'usb',
] as const;
export type Permission = (typeof ALL_PERMISSIONS)[number];
export type Role = 'admin' | 'user';

export interface Account {
  name: string;
  role: Role;
  permissions: Permission[];
  view_only: boolean;
  disabled: boolean;
  must_change_password: boolean;
  salt: string;
  hash: string;
  /** Bumped on password change / disable; sessions carrying an older value are dead. */
  token_version: number;
}

export interface Session {
  token: string;
  user: string;
  token_version: number;
  ip: string;
  since: number;
}

export interface Hdmi { signal: boolean; width: number; height: number; fps: number }
export interface Locks { num: boolean; caps: boolean; scroll: boolean }
export interface Power { on: boolean; hdd: boolean }
export interface MediaState { mounted: string | null; mode: 'cdrom' | 'disk' | null }

export interface LoginGuard {
  failures: number;
  locked_until: number;
}

/** In-memory state of the mock kvmd. Each route module adds the fields it owns. */
export interface State {
  version: string;
  /** In-process event bus; WebSocket and panel socket servers forward its events. */
  events: EventEmitter;
  now: () => number;
  sleep: (ms: number) => Promise<void>;
  accounts: Map<string, Account>;
  sessions: Map<string, Session>;
  loginGuards: Map<string, LoginGuard>;
  config: Config;
  hdmi: Hdmi;
  locks: Locks;
  power: Power;
  media: MediaState;
  edid: { hex: string; preset: '1080p' | 'custom' };
  wol: { name: string; mac: string }[];
}

export function hashPassword(password: string, salt = randomBytes(16).toString('hex')): { salt: string; hash: string } {
  return { salt, hash: scryptSync(password, salt, 32).toString('hex') };
}

export function passwordMatches(acc: Account, password: string): boolean {
  const { hash } = hashPassword(password, acc.salt);
  return timingSafeEqual(Buffer.from(hash, 'hex'), Buffer.from(acc.hash, 'hex'));
}

export function createState(opts: { now?: () => number; sleep?: (ms: number) => Promise<void> } = {}): State {
  const admin: Account = {
    name: 'admin', role: 'admin', permissions: [...ALL_PERMISSIONS], view_only: false, disabled: false,
    must_change_password: true, token_version: 0, ...hashPassword('admin'),
  };
  return {
    version: contract.info.version,
    events: new EventEmitter(),
    now: opts.now ?? Date.now,
    sleep: opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms))),
    accounts: new Map([[admin.name, admin]]),
    sessions: new Map(),
    loginGuards: new Map(),
    config: defaultConfig(),
    hdmi: { signal: true, width: 1920, height: 1080, fps: 60 },
    locks: { num: false, caps: false, scroll: false },
    power: { on: true, hdd: false },
    media: { mounted: null, mode: null },
    edid: { hex: edid1080p(), preset: '1080p' },
    wol: [],
  };
}
