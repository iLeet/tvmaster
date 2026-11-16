// Pure helpers: formatting, defaults and small bits of validation. Kept free
// of React so they can be unit tested.

import type { AdConfig, Block, Lineup, Source } from './types';

export const DAY_NAMES = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

/** `1.4 GB`, `820 MB`, `12 KB`. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

/** Where an uploaded file goes inside a library: the folder it was picked from (when picking a whole folder) or the chosen sub-folder, then its name. */
export function uploadPath(subfolder: string, file: { name: string; webkitRelativePath?: string }): string {
  const rel = file.webkitRelativePath || file.name;
  const sub = subfolder.trim().replace(/^[\\/]+|[\\/]+$/g, '');
  return sub ? `${sub}/${rel}` : rel;
}

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  const rem = m % 60;
  return rem === 0 ? `${h}h` : `${h}h ${String(rem).padStart(2, '0')}m`;
}

export function formatClock(ms: number, timeZone?: string): string {
  return new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
    timeZone,
  }).format(new Date(ms));
}

export function formatDay(ms: number, timeZone?: string): string {
  return new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    timeZone,
  }).format(new Date(ms));
}

/** A URL-safe channel id from a name: lowercase letters, digits, - and _. */
export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32)
    .replace(/-+$/g, '');
  return s || 'channel';
}

export function validateId(id: string): string | null {
  if (!id) return 'An id is required';
  if (id.length > 32) return 'At most 32 characters';
  if (!/^[a-z0-9][a-z0-9_-]*$/.test(id)) return 'Use lowercase letters, digits, - and _';
  return null;
}

export function nextFreeNumber(taken: number[], start = 2): number {
  const set = new Set(taken);
  let n = start;
  while (set.has(n)) n++;
  return n;
}

export function toggleDay(days: number[], day: number): number[] {
  return days.includes(day) ? days.filter((d) => d !== day) : [...days, day].sort((a, b) => a - b);
}

const hm = (s: string): number | null => {
  const m = /^(\d{1,2}):(\d{2})$/.exec(s);
  if (!m) return null;
  const v = Number(m[1]) * 60 + Number(m[2]);
  return Number(m[1]) <= 24 && Number(m[2]) < 60 && v <= 1440 ? v : null;
};

/** Length of a block in minutes (an end before the start runs overnight). */
export function blockMinutes(b: Pick<Block, 'start' | 'end'>): number | null {
  const s = hm(b.start);
  const e = hm(b.end);
  if (s === null || e === null || s === e) return null;
  return e > s ? e - s : e + 1440 - s;
}

export function daysLabel(days: number[]): string {
  if (days.length === 0 || days.length === 7) return 'Every day';
  const sorted = [...days].sort((a, b) => a - b);
  if (sorted.join() === '0,1,2,3,4') return 'Weekdays';
  if (sorted.join() === '5,6') return 'Weekends';
  return sorted.map((d) => DAY_NAMES[d]).join(', ');
}

export function sourceLabel(s: Source | null | undefined): string {
  if (!s) return 'Nothing';
  switch (s.type) {
    case 'show':
      return s.show || '(pick a show)';
    case 'chaos':
      return 'Anything (chaos)';
    case 'pool':
      return `${s.shows.length ? s.shows.join(', ') : 'All shows'} (${s.order})`;
    case 'rotation': {
      const how = s.advance === 'finale' ? 'on finale' : `every ${s.advance.weeks} wk`;
      return `Rotation ${how}: ${s.items.map(sourceLabel).join(' → ')}`;
    }
  }
}

export function emptyLineup(timezone = ''): Lineup {
  return {
    timezone,
    grid_minutes: null,
    blocks: [],
    seasons: [],
    events: [],
    fill: null,
    catch_up: 'resume',
    ads: null,
    signoff: null,
  };
}

export function defaultAds(): AdConfig {
  return {
    commercials: true,
    library_ids: [],
    bumper_library_ids: [],
    minutes_per_hour: 8,
    pod_seconds: 120,
    max_breaks: 4,
    no_repeat: 12,
    years: null,
    mid_breaks: true,
    bumpers: { brb: true, back_to: true, up_next: true, station_id: true, later_today_every: 3 },
  };
}

export function defaultBlock(): Block {
  return {
    name: '',
    days: [],
    start: '20:00',
    end: '22:00',
    source: { type: 'chaos' },
  };
}

export function lineupSummary(l: Lineup): string {
  const parts: string[] = [];
  const n = (c: number, w: string) => `${c} ${w}${c === 1 ? '' : 's'}`;
  if (l.blocks.length) parts.push(n(l.blocks.length, 'block'));
  if (l.seasons.length) parts.push(n(l.seasons.length, 'season'));
  if (l.events.length) parts.push(n(l.events.length, 'event'));
  if (l.grid_minutes) parts.push(`${l.grid_minutes} min grid`);
  if (l.ads) parts.push('ads on');
  return parts.length ? parts.join(' · ') : 'No lineup (follows the strategy)';
}

export function timeZones(): string[] {
  const intl = Intl as unknown as { supportedValuesOf?: (k: string) => string[] };
  try {
    const zones = intl.supportedValuesOf?.('timeZone');
    if (zones && zones.length) return zones;
  } catch {
    /* fall through */
  }
  return ['UTC'];
}

/** "mac", "win" or "linux", for platform-flavoured styling. */
export function platformClass(ua: string, platform: string): 'mac' | 'win' | 'linux' {
  const s = `${platform} ${ua}`.toLowerCase();
  if (s.includes('mac') || s.includes('iphone') || s.includes('ipad')) return 'mac';
  if (s.includes('win')) return 'win';
  return 'linux';
}
