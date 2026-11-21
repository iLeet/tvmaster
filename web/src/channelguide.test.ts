import { describe, expect, it } from 'vitest';
import { adsFor, buildChannelBody, stepProblem, summarize, type GuideAnswers, type GuideWorld } from './channelguide';

const world: GuideWorld = {
  timezone: 'America/Toronto',
  showLibraryIds: [1],
  commercialLibraryIds: [2],
  presets: [
    { id: 'none', name: 'None', about: '', commercials: false, pod_seconds: 60, max_breaks: 0, minutes_per_hour: 0, mid_breaks: false },
    { id: 'classic', name: 'Classic', about: '', commercials: true, pod_seconds: 120, max_breaks: 4, minutes_per_hour: 8, mid_breaks: true },
  ],
};
const answers = (p: Partial<GuideAnswers> = {}): GuideAnswers => ({ name: 'Cartoons', id: 'cartoons', number: 5, group: '', shows: [], style: 'shuffle', breaks: 'clean', look: null, alwaysOn: false, chaos: 0, ...p });

describe('the new channel guide', () => {
  it('shuffles everything when no shows are picked, with no breaks', () => {
    const b = buildChannelBody(answers(), world) as { strategy: string; lineup: { fill: unknown; ads: unknown; timezone: string }; library_ids: number[]; options: { theme: unknown } };
    expect(b.strategy).toBe('shuffle');
    expect(b.lineup.fill).toBeNull();
    expect(b.lineup.ads).toBeNull();
    expect(b.lineup.timezone).toBe('America/Toronto');
    expect(b.library_ids).toEqual([1]);
    expect(b.options.theme).toBeNull();
  });

  it('plays only the shows picked, in the order asked', () => {
    const shuffled = buildChannelBody(answers({ shows: ['Clifford', 'Franklin'] }), world) as { lineup: { fill: unknown } };
    expect(shuffled.lineup.fill).toEqual({ type: 'pool', shows: ['Clifford', 'Franklin'], order: 'shuffle' });
    const ordered = buildChannelBody(answers({ shows: ['Clifford'], style: 'in_order' }), world) as { strategy: string; lineup: { fill: unknown } };
    expect(ordered.strategy).toBe('sequential');
    expect(ordered.lineup.fill).toEqual({ type: 'pool', shows: ['Clifford'], order: 'sequential' });
  });

  it('a weekly schedule starts empty on a half-hour grid and ignores surprises', () => {
    const b = buildChannelBody(answers({ style: 'weekly', shows: ['A'], chaos: 30 }), world) as { strategy: string; chaos: number; lineup: { grid_minutes: number; fill: unknown } };
    expect(b.strategy).toBe('block_grid');
    expect(b.chaos).toBe(0);
    expect(b.lineup.grid_minutes).toBe(30);
    expect(b.lineup.fill).toBeNull();
  });

  it('turns a break choice into the numbers behind it, drawing from the commercials found', () => {
    const ads = adsFor('classic', world)!;
    expect(ads.pod_seconds).toBe(120);
    expect(ads.max_breaks).toBe(4);
    expect(ads.library_ids).toEqual([2]);
    expect(adsFor('clean', world)).toBeNull();
    expect(adsFor('nonsense', world)).toBeNull();
    expect(adsFor('none', world)!.commercials).toBe(false);
  });

  it('gives a channel its own look only when asked', () => {
    const b = buildChannelBody(answers({ look: { theme: 'kids', colors: { primary: null, accent: null, background: null }, style: 'modern', gentle: true } }), world) as { options: { theme: { theme: string } } };
    expect(b.options.theme.theme).toBe('kids');
  });

  it('says what is wrong on the first step', () => {
    expect(stepProblem(0, answers({ name: ' ' }), [], null)).toMatch(/name/);
    expect(stepProblem(0, answers({ number: 5 }), [5], null)).toMatch(/already used/);
    expect(stepProblem(0, answers(), [], 'Use lowercase')).toBe('Use lowercase');
    expect(stepProblem(0, answers(), [1, 2], null)).toBeNull();
    expect(stepProblem(2, answers({ name: '' }), [], null)).toBeNull();
  });

  it('sums it up in plain lines', () => {
    const lines = summarize(answers({ shows: ['A', 'B', 'C', 'D', 'E'], breaks: 'classic', alwaysOn: true }), world, 19);
    expect(lines.join(' ')).toMatch(/Plays 5 shows: A, B, C, D and 1 more/);
    expect(lines.join(' ')).toMatch(/Breaks: Classic/);
    expect(lines.join(' ')).toMatch(/Always on/);
    expect(summarize(answers(), world, 19).join(' ')).toMatch(/everything in your libraries \(19 shows\)/);
  });
});
