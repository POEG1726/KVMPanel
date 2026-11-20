/** Metric catalogue for the dashboard layout template (spec 4.1) and a sine-wave generator like the e2-v5 mockup. */

export type Group = 'cpu' | 'gpu' | 'mem' | 'sys';

export interface MetricDef {
  id: string;
  label: string;
  unit: string;
  min: number;
  max: number;
  warn?: number;
  group: Group;
}

export interface Metric extends MetricDef {
  value: number | null;
}

const cores: MetricDef[] = Array.from({ length: 16 }, (_, i) => ({
  id: `cpu.core.${i}`, label: `Core ${i} (CCD${i < 8 ? 0 : 1})`, unit: '%', min: 0, max: 100, group: 'cpu' as const,
}));

const dimm = (slot: string): MetricDef => ({
  id: `mem.dimm.${slot}`, label: `DIMM ${slot.toUpperCase()}`, unit: '°C', min: 30, max: 60, warn: 60, group: 'mem',
});

export const METRICS: MetricDef[] = [
  { id: 'cpu.clock_eff', label: 'Effective clock', unit: 'GHz', min: 3, max: 5.8, group: 'cpu' },
  { id: 'cpu.bclk', label: 'BCLK', unit: 'MHz', min: 99, max: 101, group: 'cpu' },
  { id: 'cpu.fclk', label: 'FCLK', unit: 'MHz', min: 1900, max: 2100, group: 'cpu' },
  { id: 'cpu.vcore', label: 'Vcore', unit: 'V', min: 0.9, max: 1.4, group: 'cpu' },
  { id: 'cpu.tctl', label: 'Tctl', unit: '°C', min: 30, max: 95, warn: 85, group: 'cpu' },
  { id: 'cpu.load', label: 'Load', unit: '%', min: 0, max: 100, group: 'cpu' },
  { id: 'cpu.ppt', label: 'PPT', unit: 'W', min: 0, max: 230, group: 'cpu' },
  ...cores,
  { id: 'gpu.clock', label: 'Core', unit: 'MHz', min: 0, max: 3000, group: 'gpu' },
  { id: 'gpu.temp', label: 'Temp', unit: '°C', min: 30, max: 90, group: 'gpu' },
  { id: 'gpu.mem_junction', label: 'Mem junction', unit: '°C', min: 30, max: 100, warn: 95, group: 'gpu' },
  { id: 'gpu.load', label: 'Load', unit: '%', min: 0, max: 100, group: 'gpu' },
  { id: 'gpu.vram_used', label: 'VRAM', unit: 'GB', min: 0, max: 32, group: 'gpu' },
  { id: 'gpu.power', label: 'Power', unit: 'W', min: 0, max: 600, group: 'gpu' },
  { id: 'mem.used', label: 'Used', unit: 'GB', min: 0, max: 64, group: 'mem' },
  // Only populated slots are reported; this PC has A2 and B2.
  dimm('a2'), dimm('b2'),
  { id: 'mem.mclk', label: 'MCLK', unit: 'MHz', min: 2900, max: 3100, group: 'mem' },
  { id: 'mem.uclk', label: 'UCLK', unit: 'MHz', min: 2900, max: 3100, group: 'mem' },
  { id: 'sys.power', label: 'System power', unit: 'W', min: 0, max: 900, group: 'sys' },
  { id: 'ssd.0.temp', label: 'SSD 2T', unit: '°C', min: 25, max: 80, group: 'sys' },
  { id: 'ssd.1.temp', label: '990 EVO 4T', unit: '°C', min: 25, max: 80, group: 'sys' },
  { id: 'net.rx', label: 'Net ↓', unit: 'MB/s', min: 0, max: 120, group: 'sys' },
  { id: 'net.tx', label: 'Net ↑', unit: 'MB/s', min: 0, max: 40, group: 'sys' },
];

const jitter = () => Math.random() - 0.5;

/** Values for tick `t` (seconds). CPU follows a 9 s sine, GPU a 13 s sine, as in the mockup. */
export function generate(t: number): Record<string, number | null> {
  const a = (Math.sin(t / 9) + 1) / 2;
  const b = (Math.sin(t / 13 + 1) + 1) / 2;
  const cpuLoad = 10 + 80 * a + 6 * jitter();
  const cpuPpt = 40 + 170 * a + 6 * jitter();
  const gpuPower = 40 + 520 * b + 10 * jitter();
  const v: Record<string, number | null> = {
    'cpu.clock_eff': 4.3 + 1.3 * a + 0.05 * jitter(),
    'cpu.bclk': 100 + 0.04 * jitter(),
    'cpu.fclk': 2000 + 2 * jitter(),
    'cpu.vcore': 1.0 + 0.3 * a + 0.01 * jitter(),
    'cpu.tctl': 42 + 48 * a + 1.5 * jitter(),
    'cpu.load': cpuLoad,
    'cpu.ppt': cpuPpt,
    'gpu.clock': 900 + 2000 * b + 20 * jitter(),
    'gpu.temp': 38 + 34 * b + jitter(),
    'gpu.mem_junction': 44 + 42 * b + jitter(),
    'gpu.load': 15 + 83 * b + 4 * jitter(),
    'gpu.vram_used': 4 + 20 * b,
    'gpu.power': gpuPower,
    'mem.used': 24 + 5 * a + 0.2 * jitter(),
    'mem.dimm.a2': 38 + 10 * a + 0.2 * jitter(),
    'mem.dimm.b2': 40 + 10 * a + 0.2 * jitter(),
    'mem.mclk': 3000,
    'mem.uclk': 3000,
    'sys.power': cpuPpt + gpuPower + 70 + 5 * jitter(),
    'ssd.0.temp': 38 + 8 * a,
    'ssd.1.temp': 36 + 6 * b,
    'net.rx': Math.max(0, 40 + 40 * jitter() + 30 * b),
    'net.tx': Math.max(0, 5 + 6 * jitter()),
  };
  for (let i = 0; i < 16; i++) v[`cpu.core.${i}`] = Math.min(100, Math.max(3, (i < 8 ? cpuLoad * 1.15 : cpuLoad * 0.7) + 30 * jitter()));
  return v;
}

const round = (x: number | null) => (x === null ? null : Math.round(x * 1000) / 1000);

/** Threshold for `id`: exact key first, then the longest matching `prefix.*` pattern, else the catalogue default. */
export function warnFor(id: string, thresholds: Record<string, number>, fallback?: number): number | undefined {
  if (thresholds[id] !== undefined) return thresholds[id];
  let best: string | undefined;
  for (const key of Object.keys(thresholds)) {
    if (!key.endsWith('.*')) continue;
    const prefix = key.slice(0, -1);
    if (id.startsWith(prefix) && (!best || key.length > best.length)) best = key;
  }
  return best ? thresholds[best] : fallback;
}

/** Builds the telemetry metric list, applying config thresholds and hidden ids. */
export function snapshot(t: number, thresholds: Record<string, number>, hidden: string[]): Metric[] {
  const values = generate(t);
  return METRICS.filter((d) => !hidden.includes(d.id)).map((d) => {
    const m: Metric = { ...d, value: round(values[d.id] ?? null) };
    const warn = warnFor(d.id, thresholds, d.warn);
    if (warn !== undefined) m.warn = warn;
    else delete m.warn;
    return m;
  });
}
