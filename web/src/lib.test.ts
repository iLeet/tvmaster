import { describe, expect, it } from 'vitest';
import {
  blockMinutes,
  daysLabel,
  defaultAds,
  emptyLineup,
  formatDuration,
  lineupSummary,
  nextFreeNumber,
  platformClass,
  slugify,
  sourceLabel,
  toggleDay,
  validateId,
} from './lib';
import { parseHash } from './router';

describe('formatting', () => {
  it('formats durations compactly', () => {
    expect(formatDuration(45_000)).toBe('45s');
    expect(formatDuration(22 * 60_000)).toBe('22m');
    expect(formatDuration(65 * 60_000)).toBe('1h 05m');
    expect(formatDuration(2 * 3_600_000)).toBe('2h');
    expect(formatDuration(-5)).toBe('0s');
  });
});

describe('channel ids', () => {
  it('slugifies names into valid ids', () => {
    expect(slugify('Retro TV')).toBe('retro-tv');
    expect(slugify('  Café  Noir!! ')).toBe('cafe-noir');
    expect(slugify('日本語')).toBe('channel');
    expect(slugify('a'.repeat(50)).length).toBe(32);
    expect(validateId(slugify('Toon Town'))).toBeNull();
  });

  it('validates ids like the server does', () => {
    expect(validateId('')).not.toBeNull();
    expect(validateId('Bad Id')).not.toBeNull();
    expect(validateId('-lead')).not.toBeNull();
    expect(validateId('ok_id-2')).toBeNull();
    expect(validateId('x'.repeat(33))).not.toBeNull();
  });

  it('finds the next free channel number', () => {
    expect(nextFreeNumber([])).toBe(2);
    expect(nextFreeNumber([2, 3, 5])).toBe(4);
    expect(nextFreeNumber([4, 5], 4)).toBe(6);
  });
});

describe('blocks', () => {
  it('measures blocks including overnight ones', () => {
    expect(blockMinutes({ start: '20:00', end: '22:30' })).toBe(150);
    expect(blockMinutes({ start: '23:00', end: '02:00' })).toBe(180);
    expect(blockMinutes({ start: '00:00', end: '24:00' })).toBe(1440);
    expect(blockMinutes({ start: '20:00', end: '20:00' })).toBeNull();
    expect(blockMinutes({ start: '25:00', end: '26:00' })).toBeNull();
    expect(blockMinutes({ start: 'x', end: '22:00' })).toBeNull();
  });

  it('toggles days and keeps them sorted', () => {
    expect(toggleDay([], 4)).toEqual([4]);
    expect(toggleDay([4, 0], 2)).toEqual([0, 2, 4]);
    expect(toggleDay([0, 2], 2)).toEqual([0]);
  });

  it('describes day sets', () => {
    expect(daysLabel([])).toBe('Every day');
    expect(daysLabel([0, 1, 2, 3, 4])).toBe('Weekdays');
    expect(daysLabel([6, 5])).toBe('Weekends');
    expect(daysLabel([0, 2])).toBe('Mon, Wed');
  });
});

describe('sources and lineups', () => {
  it('labels every kind of source', () => {
    expect(sourceLabel({ type: 'show', show: 'Cheers' })).toBe('Cheers');
    expect(sourceLabel({ type: 'chaos' })).toContain('chaos');
    expect(sourceLabel({ type: 'pool', shows: [], order: 'shuffle' })).toBe('All shows (shuffle)');
    expect(
      sourceLabel({
        type: 'rotation',
        advance: { weeks: 2 },
        items: [
          { type: 'show', show: 'A' },
          { type: 'show', show: 'B' },
        ],
      }),
    ).toBe('Rotation every 2 wk: A → B');
    expect(sourceLabel(null)).toBe('Nothing');
  });

  it('summarises a lineup', () => {
    expect(lineupSummary(emptyLineup())).toContain('follows the strategy');
    const l = emptyLineup('UTC');
    l.grid_minutes = 30;
    l.blocks.push({ name: '', days: [], start: '20:00', end: '21:00', source: { type: 'chaos' } });
    l.ads = defaultAds();
    expect(lineupSummary(l)).toBe('1 block · 30 min grid · ads on');
  });

  it('starts with the same ad defaults the server uses', () => {
    const a = defaultAds();
    expect([a.minutes_per_hour, a.pod_seconds, a.max_breaks, a.no_repeat]).toEqual([8, 120, 4, 12]);
    expect(a.bumpers.later_today_every).toBe(3);
  });
});

describe('platform and routes', () => {
  it('detects the platform for styling', () => {
    expect(platformClass('Mozilla/5.0 (Macintosh; Intel Mac OS X)', 'MacIntel')).toBe('mac');
    expect(platformClass('Mozilla/5.0 (Windows NT 10.0)', 'Win32')).toBe('win');
    expect(platformClass('Mozilla/5.0 (X11; Linux x86_64)', 'Linux x86_64')).toBe('linux');
  });

  it('parses hash routes', () => {
    expect(parseHash('#/channels/retro?tab=ads').parts).toEqual(['channels', 'retro']);
    expect(parseHash('#/channels/retro?tab=ads').query.get('tab')).toBe('ads');
    expect(parseHash('').parts).toEqual([]);
    expect(parseHash('#/remote/living%20room').parts).toEqual(['remote', 'living room']);
  });
});
