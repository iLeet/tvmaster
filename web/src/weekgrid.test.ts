import { describe, expect, it } from 'vitest';
import {
  detachDay,
  duplicate,
  fmtMin,
  moveOccurrence,
  newBlock,
  resizeEnd,
  resizeStart,
  segments,
  shadows,
  shift,
  snap,
  span,
  toMin,
} from './weekgrid';
import type { Block } from './types';

const blk = (start: string, end: string, days: number[] = [], name = ''): Block => ({
  name,
  days,
  start,
  end,
  source: { type: 'chaos' },
});

describe('times', () => {
  it('parses and formats, with 24:00 as midnight', () => {
    expect(toMin('08:30')).toBe(510);
    expect(toMin('24:00')).toBe(1440);
    expect(toMin('24:30')).toBeNull();
    expect(toMin('nope')).toBeNull();
    expect(fmtMin(1440)).toBe('24:00');
    expect(fmtMin(65)).toBe('01:05');
    expect(snap(37, 30)).toBe(30);
    expect(snap(44, 30)).toBe(30);
    expect(snap(46, 30)).toBe(60);
  });
});

describe('segments', () => {
  it('shows an every-day block once per day', () => {
    const s = segments([blk('20:00', '22:00')]);
    expect(s).toHaveLength(7);
    expect(s.every((x) => x.start === 1200 && x.end === 1320)).toBe(true);
  });

  it('only draws the days a block names', () => {
    const s = segments([blk('08:00', '09:00', [5, 6])]);
    expect(s.map((x) => x.day)).toEqual([5, 6]);
  });

  it('splits an overnight block across midnight into the next column', () => {
    const s = segments([blk('23:00', '02:00', [4])]);
    expect(s).toEqual([
      { block: 0, day: 4, origin: 4, start: 1380, end: 1440, contFrom: false, contTo: true },
      { block: 0, day: 5, origin: 4, start: 0, end: 120, contFrom: true, contTo: false },
    ]);
  });

  it('wraps Sunday night into Monday', () => {
    const s = segments([blk('23:00', '01:00', [6])]);
    expect(s[1].day).toBe(0);
  });

  it('skips blocks whose times are invalid or empty', () => {
    expect(segments([blk('10:00', '10:00'), blk('xx', '10:00'), blk('24:00', '02:00')])).toEqual([]);
    expect(span(blk('08:00', '24:00'))).toEqual({ start: 480, end: 1440 });
  });
});

describe('creating', () => {
  it('snaps and orders a dragged range', () => {
    const b = newBlock(2, 1230, 1170, 30);
    expect([b.days, b.start, b.end]).toEqual([[2], '19:30', '20:30']);
  });
  it('is at least one step long', () => {
    const b = newBlock(0, 600, 601, 30);
    expect([b.start, b.end]).toEqual(['10:00', '10:30']);
  });
  it('stops at midnight', () => {
    const b = newBlock(0, 1425, 1500, 30);
    expect([b.start, b.end]).toEqual(['23:30', '24:00']);
  });
});

describe('moving', () => {
  it('shifts in time keeping the length, and stops at the day edges', () => {
    const b = blk('20:00', '22:00', [1]);
    expect(shift(b, 60, 30)).toMatchObject({ start: '21:00', end: '23:00' });
    expect(shift(b, 600, 30)).toMatchObject({ start: '22:00', end: '24:00' });
    expect(shift(b, -2000, 30)).toMatchObject({ start: '00:00', end: '02:00' });
    expect(shift(b, 7, 30)).toMatchObject({ start: '20:00', end: '22:00' }); // under half a step
  });

  it('wraps an overnight block around midnight', () => {
    const b = blk('23:00', '02:00', [4]); // 3 hours
    expect(shift(b, 120, 30)).toMatchObject({ start: '01:00', end: '04:00' });
    expect(shift(b, 60, 30)).toMatchObject({ start: '00:00', end: '03:00' });
  });

  it('moves one occurrence to another day', () => {
    const b = blk('20:00', '21:00', [0, 1, 2]);
    expect(moveOccurrence(b, 1, 4).days).toEqual([0, 2, 4]);
    expect(moveOccurrence(b, 1, 2)).toBe(b); // that day is taken
    expect(moveOccurrence(b, 1, 99).days).toEqual([0, 2, 6]); // clamped to Sunday
    expect(moveOccurrence(b, 3, 4)).toBe(b); // not one of its days
  });

  it('cannot move an occurrence of an every-day block: every other day is taken', () => {
    const b = blk('20:00', '21:00');
    expect(moveOccurrence(b, 1, 2)).toBe(b);
  });
});

describe('resizing', () => {
  const b = blk('20:00', '22:00', [1]);
  it('moves the end edge', () => {
    expect(resizeEnd(b, 1380, 30).end).toBe('23:00');
    expect(resizeEnd(b, 0, 30).end).toBe('20:30'); // never shorter than a step
    expect(resizeEnd(b, 1440, 30).end).toBe('24:00');
  });
  it('lets the end go past midnight to make an overnight block', () => {
    expect(resizeEnd(b, 1440 + 90, 30).end).toBe('01:30');
  });
  it('moves the start edge and keeps the end', () => {
    const r = resizeStart(b, 1110, 30);
    expect([r.start, r.end]).toEqual(['18:30', '22:00']);
    expect(resizeStart(b, 5000, 30).start).toBe('21:30');
    expect(resizeStart(b, -50, 30).start).toBe('00:00');
  });
  it('keeps overnight blocks under 24 hours', () => {
    const o = blk('23:00', '02:00', [1]);
    expect(resizeStart(o, 0, 30).start).toBe('02:30');
  });
});

describe('overlaps', () => {
  it('shades the part a higher-priority (earlier) block covers', () => {
    const s = shadows([blk('08:00', '10:00', [0], 'A'), blk('09:00', '12:00', [0], 'B')]);
    expect(s).toEqual([{ block: 1, day: 0, start: 540, end: 600 }]);
  });
  it('ignores other days and non-overlapping blocks', () => {
    expect(shadows([blk('08:00', '10:00', [0]), blk('09:00', '12:00', [1])])).toEqual([]);
    expect(shadows([blk('08:00', '09:00', [0]), blk('09:00', '12:00', [0])])).toEqual([]);
  });
  it('sees an overnight tail overlapping the next morning', () => {
    const s = shadows([blk('23:00', '02:00', [0]), blk('01:00', '03:00', [1])]);
    expect(s).toEqual([{ block: 1, day: 1, start: 60, end: 120 }]);
  });
});

describe('detach and duplicate', () => {
  it('turns one day of a multi-day block into its own block', () => {
    const { blocks, index } = detachDay([blk('20:00', '21:00', [0, 1, 2], 'Sitcoms')], 0, 1);
    expect(blocks[0].days).toEqual([0, 2]);
    expect(blocks[index].days).toEqual([1]);
    expect(blocks[index].name).toBe('Sitcoms');
    expect(blocks[index].source).not.toBe(blocks[0].source); // a real copy
  });
  it('detaching a single-day block does nothing', () => {
    const list = [blk('20:00', '21:00', [3])];
    expect(detachDay(list, 0, 3).blocks).toBe(list);
  });
  it('duplicates with a copy label', () => {
    const { blocks, index } = duplicate([blk('20:00', '21:00', [3], 'Prime')], 0);
    expect(blocks).toHaveLength(2);
    expect(blocks[index].name).toBe('Prime (copy)');
  });
});
