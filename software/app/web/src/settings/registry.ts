import type { components } from '../api/schema';

type Permission = components['schemas']['Permission'];

/** Sidebar layout, as in design 5.2: the KVM group, then the system group. */
export const SETTINGS_GROUPS = [
  { id: 'kvm', pages: ['general', 'input', 'video', 'media'] },
  { id: 'system', pages: ['panel', 'usb', 'function_key', 'storage', 'network', 'tailscale', 'accounts', 'mcp', 'update', 'device', 'about'] },
] as const;

export type SettingsPageId = (typeof SETTINGS_GROUPS)[number]['pages'][number];

/** Permission a page needs (pages without an entry are open to every signed-in user). */
export const PAGE_PERMISSION: Partial<Record<SettingsPageId, Permission>> = {
  general: 'device',
  input: 'console',
  video: 'console',
  media: 'media',
  panel: 'panel',
  usb: 'usb',
  function_key: 'panel',
  storage: 'files',
  network: 'network',
  tailscale: 'tailscale',
  mcp: 'mcp',
  update: 'updates',
  device: 'device',
};

/** Metric ids the panel can show (contract README, "指标 id"), by group. */
export const METRIC_GROUPS: readonly { id: 'cpu' | 'gpu' | 'mem' | 'sys'; metrics: readonly string[] }[] = [
  {
    id: 'cpu',
    metrics: [
      'cpu.clock_eff', 'cpu.bclk', 'cpu.fclk', 'cpu.vcore', 'cpu.tctl', 'cpu.load', 'cpu.ppt',
      ...Array.from({ length: 16 }, (_, i) => `cpu.core.${i}`),
    ],
  },
  { id: 'gpu', metrics: ['gpu.clock', 'gpu.temp', 'gpu.mem_junction', 'gpu.load', 'gpu.vram_used', 'gpu.power'] },
  { id: 'mem', metrics: ['mem.used', 'mem.dimm.a1', 'mem.dimm.a2', 'mem.dimm.b1', 'mem.dimm.b2', 'mem.mclk', 'mem.uclk'] },
  { id: 'sys', metrics: ['sys.power', 'ssd.0.temp', 'ssd.1.temp', 'net.rx', 'net.tx'] },
];
