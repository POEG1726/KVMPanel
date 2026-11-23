import { EventEmitter } from 'node:events';
import { randomBytes, scryptSync, timingSafeEqual } from 'node:crypto';
import { contract } from './contract.js';
import { defaultConfig, type Config } from './configDefaults.js';
import { edid1080p } from './frames.js';
import { seedVolumes, type Root, type Volume } from './memfs.js';

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
export interface Power { on: boolean }
export interface MediaState { mounted: string | null; mode: 'cdrom' | 'disk' | null }
export interface MediaMount { root: 'sd'; path: string; mode: 'cdrom' | 'disk'; readonly: boolean }
export interface DownloadJob {
  id: string;
  url: string;
  path: string;
  state: 'running' | 'paused' | 'done' | 'failed' | 'canceled';
  received: number;
  total: number;
}

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
  volumes: Record<Root, Volume>;
  mediaMount: MediaMount | null;
  jobs: DownloadJob[];
  share: { exported_to_host: boolean };
  scripts: Map<string, string>;
  network: {
    mode: 'dhcp' | 'static'; ip?: string; mask?: string; gateway?: string;
    dns: { mode: 'dhcp' | 'manual'; servers: string[] };
    https: { enabled: boolean; port: number; cert_path?: string; key_path?: string };
    http_port: number; bind: string;
  };
  mcpKeys: Map<string, { hash: string; id: string; name: string; scopes: string[]; created: number; last_used: number | null; hint: string }>;
  mcpAudit: { ts: number; key_name: string | null; tool: string; args: object; result: string; elapsed_ms: number; count: number }[];
  mcpActivity: { active: boolean; key_name: string | null; tool: string | null };
  logo: { type: string; data: Buffer } | null;
  startedAt: number;
  /** PC host program link (drives panel telemetry). */
  pc: { connected: boolean; last_seen_ts: number | null };
  /** Number of open /api/ws console connections. */
  consoleSessions: number;
  /** User and source IP of each open /api/ws connection. */
  consoleClients: { user: string; ip: string }[];
  /** Open /api/stream sockets that are not paused. */
  activeStreams: number;
  mcpEnabled: boolean;
  mcpInputPolicy: { human_input_priority: boolean; human_input_wait_s: number };
  mcpLastHumanInput: number | null;
  update: { channel: 'stable' | 'preview'; server: string };
  updateJobs: Map<string, { id: string; state: 'running' | 'done' | 'failed'; progress: number }>;
  /** USB set the gadget is running (config.usb may be ahead until applied). */
  usbApplied: Record<string, unknown>;
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
  const config = defaultConfig();
  return {
    version: contract.info.version,
    events: new EventEmitter(),
    now: opts.now ?? Date.now,
    sleep: opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms))),
    accounts: new Map([[admin.name, admin]]),
    sessions: new Map(),
    loginGuards: new Map(),
    config,
    hdmi: { signal: true, width: 1920, height: 1080, fps: 60 },
    locks: { num: false, caps: false, scroll: false },
    power: { on: true },
    media: { mounted: null, mode: null },
    edid: { hex: edid1080p(), preset: '1080p' },
    wol: [],
    volumes: seedVolumes(),
    mediaMount: null,
    jobs: [],
    share: { exported_to_host: false },
    scripts: new Map(),
    network: { mode: 'dhcp', dns: { mode: 'dhcp', servers: [] }, https: { enabled: false, port: 443 }, http_port: 80, bind: '0.0.0.0' },
    mcpKeys: new Map(),
    mcpAudit: [],
    mcpActivity: { active: false, key_name: null, tool: null },
    logo: null,
    startedAt: (opts.now ?? Date.now)(),
    pc: { connected: true, last_seen_ts: null },
    consoleSessions: 0,
    consoleClients: [],
    activeStreams: 0,
    mcpEnabled: false,
    mcpInputPolicy: { human_input_priority: true, human_input_wait_s: 2 },
    mcpLastHumanInput: null,
    update: { channel: 'stable', server: 'https://updates.example.invalid/kvmpanel' },
    updateJobs: new Map(),
    usbApplied: structuredClone(config.usb),
  };
}
