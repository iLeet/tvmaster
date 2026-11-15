// Pure geometry for the weekly grid editor: blocks <-> rectangles on a
// Monday-to-Sunday by 24-hour grid. No React, so it is unit tested.
//
// A block is one entry in the lineup: days (empty = every day), a local
// start and end, and what airs. On the grid a block with several days shows
// once per day, and an overnight block (end before start) continues into the
// next day's column. The block's days always mean the day it STARTS.

import type { Block } from './types';

export const DAY_MINUTES = 1440;

export function toMin(s: string): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(s);
  if (!m) return null;
  const h = Number(m[1]);
  const mi = Number(m[2]);
  const v = h * 60 + mi;
  return h <= 24 && mi < 60 && v <= DAY_MINUTES ? v : null;
}

export function fmtMin(m: number): string {
  const v = Math.max(0, Math.min(DAY_MINUTES, Math.round(m)));
  return `${String(Math.floor(v / 60)).padStart(2, '0')}:${String(v % 60).padStart(2, '0')}`;
}

/** "8:00 PM" style, for labels on blocks. */
export function clockLabel(m: number): string {
  const v = ((Math.round(m) % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES;
  const h = Math.floor(v / 60);
  const mi = v % 60;
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return `${h12}${mi ? `:${String(mi).padStart(2, '0')}` : ''} ${h < 12 ? 'AM' : 'PM'}`;
}

export function snap(minutes: number, step: number): number {
  return Math.round(minutes / step) * step;
}

/** A block's days as an explicit sorted list (empty means every day). */
export function daysOf(b: Pick<Block, 'days'>): number[] {
  return b.days.length === 0 ? [0, 1, 2, 3, 4, 5, 6] : [...b.days].sort((a, c) => a - c);
}

/** Start and end in minutes from the start day's midnight; the end may pass 1440 (overnight). */
export function span(b: Pick<Block, 'start' | 'end'>): { start: number; end: number } | null {
  const s = toMin(b.start);
  const e = toMin(b.end);
  if (s === null || e === null || s === e || s >= DAY_MINUTES) return null;
  return { start: s, end: e > s ? e : e + DAY_MINUTES };
}

/** One rectangle: part of one block on one day's column. */
export interface Seg {
  block: number;
  /** The column this rectangle is in. */
  day: number;
  /** The day the block starts on. */
  origin: number;
  start: number;
  end: number;
  /** Continues from the previous day (no real start edge). */
  contFrom: boolean;
  /** Continues into the next day (no real end edge). */
  contTo: boolean;
}

export function segments(blocks: Block[]): Seg[] {
  const out: Seg[] = [];
  blocks.forEach((b, block) => {
    const sp = span(b);
    if (!sp) return;
    for (const origin of daysOf(b)) {
      if (sp.end <= DAY_MINUTES) {
        out.push({ block, day: origin, origin, start: sp.start, end: sp.end, contFrom: false, contTo: false });
      } else {
        out.push({ block, day: origin, origin, start: sp.start, end: DAY_MINUTES, contFrom: false, contTo: true });
        out.push({ block, day: (origin + 1) % 7, origin, start: 0, end: sp.end - DAY_MINUTES, contFrom: true, contTo: false });
      }
    }
  });
  return out;
}

/** Part of a segment that an EARLIER block in the list already covers. The
 * scheduler uses the first matching block, so that part will never air. */
export interface Shadow {
  block: number;
  day: number;
  start: number;
  end: number;
}

export function shadows(blocks: Block[]): Shadow[] {
  const segs = segments(blocks);
  const out: Shadow[] = [];
  for (const s of segs) {
    for (const o of segs) {
      if (o.day !== s.day || o.block >= s.block) continue;
      const start = Math.max(s.start, o.start);
      const end = Math.min(s.end, o.end);
      if (end > start) out.push({ block: s.block, day: s.day, start, end });
    }
  }
  return out;
}

/** A new block on one day, snapped to `step`, at least one step long. */
export function newBlock(day: number, a: number, b: number, step: number): Block {
  let start = snap(Math.min(a, b), step);
  let end = snap(Math.max(a, b), step);
  if (end - start < step) end = start + step;
  if (end > DAY_MINUTES) {
    end = DAY_MINUTES;
    start = Math.max(0, end - step);
  }
  start = Math.max(0, start);
  return { name: '', days: [day], start: fmtMin(start), end: fmtMin(end), source: { type: 'chaos' } };
}

/** Shift a block in time, keeping its length. Ordinary blocks stop at the
 * edges of the day; overnight blocks wrap around midnight. */
export function shift(b: Block, deltaMin: number, step: number): Block {
  const sp = span(b);
  if (!sp) return b;
  const len = sp.end - sp.start;
  let start = snap(sp.start + deltaMin, step);
  if (sp.end <= DAY_MINUTES) start = Math.max(0, Math.min(DAY_MINUTES - len, start));
  else start = ((start % DAY_MINUTES) + DAY_MINUTES) % DAY_MINUTES;
  const end = start + len;
  return { ...b, start: fmtMin(start), end: fmtMin(end > DAY_MINUTES ? end - DAY_MINUTES : end) };
}

/** Move one occurrence of a block from the day it starts on to another day. */
export function moveOccurrence(b: Block, from: number, to: number): Block {
  const to6 = Math.max(0, Math.min(6, to));
  if (to6 === from) return b;
  const days = daysOf(b);
  if (!days.includes(from) || days.includes(to6)) return b;
  const next = days.map((d) => (d === from ? to6 : d)).sort((x, y) => x - y);
  return { ...b, days: next.length === 7 ? [] : next };
}

/** Drag the end edge to `abs` (minutes from the start day's midnight). */
export function resizeEnd(b: Block, abs: number, step: number): Block {
  const sp = span(b);
  if (!sp) return b;
  const end = Math.max(sp.start + step, Math.min(sp.start + DAY_MINUTES - step, snap(abs, step)));
  return { ...b, end: fmtMin(end > DAY_MINUTES ? end - DAY_MINUTES : end) };
}

/** Drag the start edge to `abs`, keeping the end where it is. */
export function resizeStart(b: Block, abs: number, step: number): Block {
  const sp = span(b);
  if (!sp) return b;
  const start = Math.max(Math.max(0, sp.end - DAY_MINUTES + step), Math.min(sp.end - step, snap(abs, step)));
  if (start >= DAY_MINUTES) return b;
  return { ...b, start: fmtMin(start) };
}

/** Make one day of a multi-day block its own block, so it can differ from the rest. */
export function detachDay(blocks: Block[], index: number, day: number): { blocks: Block[]; index: number } {
  const b = blocks[index];
  const days = daysOf(b);
  if (!b || days.length < 2 || !days.includes(day)) return { blocks, index };
  const rest = days.filter((d) => d !== day);
  const kept: Block = { ...b, days: rest };
  const alone: Block = { ...structuredClone(b), days: [day] };
  const next = [...blocks.slice(0, index), kept, ...blocks.slice(index + 1), alone];
  return { blocks: next, index: next.length - 1 };
}

export function duplicate(blocks: Block[], index: number): { blocks: Block[]; index: number } {
  const copy = structuredClone(blocks[index]);
  if (copy.name) copy.name = `${copy.name} (copy)`;
  return { blocks: [...blocks, copy], index: blocks.length };
}

/** Hue (0-359) for a block's colour, stable for what it airs. */
export function hueFor(key: string): number {
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) >>> 0;
  return h % 360;
}
