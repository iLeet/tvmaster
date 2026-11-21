import { describe, expect, it } from 'vitest';
import { emptyLineup } from './lib';
import { gapChoice, gapFill, hasSchedule, programmingSummary, runOutChoice } from './programming';
import type { Block } from './types';

const block: Block = { name: '', days: [], start: '13:00', end: '13:30', source: { type: 'show', show: 'A' } };

describe('programming in plain words', () => {
  it('an unset fill keeps playing shows, unless the old test-screen policy applies', () => {
    const l = { ...emptyLineup('UTC'), blocks: [block] };
    expect(gapChoice(l, 'next_in_rotation')).toBe('keep');
    expect(gapChoice(l, 'off_air')).toBe('screen');
    expect(gapChoice(emptyLineup('UTC'), 'off_air')).toBe('keep');
  });

  it('reads the fill the server stores', () => {
    const l = emptyLineup('UTC');
    expect(gapChoice({ ...l, fill: { type: 'off_air' } }, 'keep_going')).toBe('screen');
    expect(gapChoice({ ...l, fill: { type: 'chaos' } }, 'keep_going')).toBe('random');
    expect(gapChoice({ ...l, fill: { type: 'show', show: 'A' } }, 'keep_going')).toBe('these');
  });

  it('turns a choice into a fill and back', () => {
    for (const c of ['screen', 'random', 'these', 'keep'] as const) {
      const fill = gapFill(c, 'block_grid', 'keep_going', null);
      expect(gapChoice({ ...emptyLineup('UTC'), fill }, 'keep_going')).toBe(c);
    }
    // Under the old test-screen policy "keep playing" must be explicit or the gaps stay blank.
    const f = gapFill('keep', 'shuffle', 'off_air', null);
    expect(f).toEqual({ type: 'pool', shows: [], order: 'shuffle' });
    expect(gapChoice({ ...emptyLineup('UTC'), blocks: [block], fill: f }, 'off_air')).toBe('these');
    // picking "these" again keeps shows already chosen
    const mine = { type: 'show', show: 'B' } as const;
    expect(gapFill('these', 'block_grid', 'keep_going', mine)).toEqual(mine);
  });

  it('shows hybrid as plain "something random"', () => {
    expect(runOutChoice('hybrid')).toBe('chaos_fill');
    expect(runOutChoice('off_air')).toBe('off_air');
  });

  it('knows when a channel has a schedule', () => {
    expect(hasSchedule('sequential', emptyLineup('UTC'))).toBe(false);
    expect(hasSchedule('block_grid', emptyLineup('UTC'))).toBe(true);
    expect(hasSchedule('sequential', { ...emptyLineup('UTC'), blocks: [block] })).toBe(true);
  });

  it('summarises a weekly channel in one line', () => {
    const l = { ...emptyLineup('UTC'), blocks: [block, block], fill: { type: 'off_air' } as const };
    expect(programmingSummary('block_grid', 'next_in_rotation', l, 'short break between shows')).toBe(
      'Weekly schedule · 2 slots · nothing scheduled: show the off-air screen · when a show runs out: move to the next show · short break between shows',
    );
  });
});

import { breaksLabel } from './programming';
import type { AdPreset } from './types';

describe('breaks in words', () => {
  const short = { id: 'short', name: 'Short break between shows', commercials: true, pod_seconds: 60, max_breaks: 0, minutes_per_hour: 4, mid_breaks: false } as AdPreset;
  it('names the preset in force', () => {
    expect(breaksLabel(null, [short])).toBe('No commercial breaks');
    const ads = { commercials: true, pod_seconds: 60, max_breaks: 0, minutes_per_hour: 4, mid_breaks: false } as never;
    expect(breaksLabel(ads, [short])).toBe('Commercial breaks: short break between shows');
    expect(breaksLabel({ ...(ads as object), pod_seconds: 77 } as never, [short])).toBe('Commercial breaks: custom');
  });
});
