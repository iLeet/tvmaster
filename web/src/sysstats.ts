// What the Server page says about the machine, in plain words.

export interface SysStats {
  host: { name: string | null; os: string | null; os_version: string | null; kernel: string | null; arch: string; cpu_model: string | null; threads: number; cores: number | null; uptime_s: number; container: boolean };
  cpu: { usage_pct: number; per_core: number[]; load: [number, number, number] };
  memory: { total: number; available: number; used: number; swap_total: number; swap_used: number; limited_by_container: boolean };
  tvmaster: { pid: number; cpu_pct: number; memory: number; uptime_s: number } | null;
  ffmpeg: { processes: number; cpu_cores: number; memory: number; top: { pid: number; cpu_pct: number; memory: number }[] };
  disks: { mount: string; name: string; fs: string; total: number; free: number; used_for: string[] }[];
  capacity: { per_channel_cores: number; measured: boolean; free_cores: number; more_channels: number; limited_by: 'memory' | 'processor' };
  encoder: string | null;
  on_air: string[];
  version: string;
}

export function bytes(n: number): string {
  if (n >= 1024 ** 4) return `${(n / 1024 ** 4).toFixed(1)} TB`;
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`;
  if (n >= 1024 ** 2) return `${Math.round(n / 1024 ** 2)} MB`;
  return `${Math.round(n / 1024)} KB`;
}

export function uptime(s: number): string {
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
}

/** How hard the machine is working, as a word. `load` is the one-minute load average. */
export function busyWord(usage: number, load1: number, threads: number): { word: string; tone: 'ok' | 'warn' | 'bad' } {
  const l = threads > 0 ? load1 / threads : 0;
  const worst = Math.max(usage / 100, l);
  if (worst < 0.55) return { word: 'Plenty of room', tone: 'ok' };
  if (worst < 0.85) return { word: 'Busy', tone: 'warn' };
  return { word: 'Struggling: expect buffering', tone: 'bad' };
}

/** The guess at how many more channels fit, as a sentence. */
export function capacitySentence(c: SysStats['capacity'], encoder: string | null): string {
  const kind = encoder && encoder !== 'software' ? `hardware (${encoder}) ` : '';
  const basis = c.measured ? `at what each one is using now (about ${c.per_channel_cores} of a core)` : `at the usual ${c.per_channel_cores} of a core for a ${kind}720p channel`;
  if (c.more_channels <= 0) return `Probably no more live channels at once, ${basis}. ${c.limited_by === 'memory' ? 'Memory is the limit.' : 'The processor is the limit.'}`;
  return `Room for about ${c.more_channels} more live channel${c.more_channels === 1 ? '' : 's'} at once, ${basis}; ${c.limited_by === 'memory' ? 'memory' : 'the processor'} runs out first. A rough guess: channels only encode while someone is watching (or when set to Always on).`;
}
