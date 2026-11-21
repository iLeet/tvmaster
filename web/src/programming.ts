// Programming in plain words. The server stores a strategy, a "run out" policy and a fill; people
// think in three questions: how does the channel play, what happens when nothing is scheduled, and
// what happens when a show runs out. These helpers translate between the two.

import type { AdConfig, AdPreset, ChannelOptions, Lineup, RunOut, Source, Strategy } from './types';

export const STRATEGIES: { value: Strategy; label: string; help: string }[] = [
  { value: 'sequential', label: 'In order', help: 'Episodes in order, one show after another, remembering where each left off.' },
  { value: 'shuffle', label: 'Shuffle', help: 'Every episode once per pass, in random order.' },
  { value: 'chaos', label: 'Chaos', help: 'Anything, at random.' },
  { value: 'block_grid', label: 'Weekly schedule', help: 'A real TV schedule: shows at set times on set days, with seasonal changes and holiday specials.' },
];

/** What happens when a show has no episodes left. Old names are in the tooltips and the API. */
export const RUN_OUTS: { value: RunOut; label: string; help: string }[] = [
  { value: 'keep_going', label: 'Start over', help: 'After the last episode, go back to the first and carry on. (keep_going)' },
  { value: 'next_in_rotation', label: 'Move to the next show', help: 'A rotation hands over to its next show at each finale. (next_in_rotation)' },
  { value: 'chaos_fill', label: 'Play something random', help: 'A finished show’s slot airs random content instead. (chaos_fill)' },
  { value: 'off_air', label: 'Show the off-air screen', help: 'Each episode airs once; when a show has run out, its slot shows the off-air screen instead. (off_air)' },
];

/** Older channels may say "hybrid", which is "something random" plus random unscheduled time. */
export function runOutChoice(r: RunOut): RunOut {
  return r === 'hybrid' ? 'chaos_fill' : r;
}

export type GapChoice = 'screen' | 'keep' | 'random' | 'these';

export const GAPS: { value: GapChoice; label: string; help: string }[] = [
  { value: 'screen', label: 'Show the off-air screen', help: 'The title card, colour bars, snow or your own picture, until the next scheduled show. Nothing else plays.' },
  { value: 'keep', label: 'Keep playing shows', help: 'Carry on with the channel’s shows, as if it had no gaps.' },
  { value: 'random', label: 'Play something random', help: 'Anything from the libraries, at random.' },
  { value: 'these', label: 'Play these shows', help: 'Pick the shows (or a rotation) that fill the gaps.' },
];

/** Whether the channel has anything scheduled by time. */
export function hasSchedule(strategy: Strategy, l: Lineup): boolean {
  return strategy === 'block_grid' || l.blocks.length > 0 || l.seasons.some((s) => s.blocks.length > 0) || l.events.length > 0;
}

/** What airs in unscheduled time, as the person would put it. */
export function gapChoice(l: Lineup, runOut: RunOut): GapChoice {
  const f = l.fill;
  if (!f) return runOut === 'off_air' && l.blocks.length > 0 ? 'screen' : 'keep';
  if (f.type === 'off_air') return 'screen';
  if (f.type === 'chaos') return 'random';
  return 'these';
}

/** The fill a choice means. `keep` is "unset" unless an older "test screen" policy would still blank the gaps. */
export function gapFill(c: GapChoice, strategy: Strategy, runOut: RunOut, current: Source | null): Source | null {
  switch (c) {
    case 'screen':
      return { type: 'off_air' };
    case 'random':
      return { type: 'chaos' };
    case 'these':
      return current && current.type !== 'off_air' && current.type !== 'chaos' ? current : { type: 'pool', shows: [], order: strategy === 'shuffle' ? 'shuffle' : 'sequential' };
    case 'keep':
      // With the old test-screen policy an unset fill still means the screen, so say what "keep" is.
      return runOut === 'off_air' ? { type: 'pool', shows: [], order: strategy === 'shuffle' ? 'shuffle' : 'sequential' } : null;
  }
}

/** One line for the General tab. */
export function programmingSummary(strategy: Strategy, runOut: RunOut, l: Lineup, ads: string | null): string {
  const how = STRATEGIES.find((s) => s.value === strategy)?.label ?? strategy;
  const parts: string[] = [how];
  if (hasSchedule(strategy, l)) {
    const n = l.blocks.length + l.seasons.reduce((a, s) => a + s.blocks.length, 0);
    parts.push(`${n} slot${n === 1 ? '' : 's'}`);
    const g = gapChoice(l, runOut);
    parts.push(`nothing scheduled: ${GAPS.find((x) => x.value === g)!.label.toLowerCase()}`);
  }
  parts.push(`when a show runs out: ${RUN_OUTS.find((r) => r.value === runOutChoice(runOut))!.label.toLowerCase()}`);
  if (ads) parts.push(ads);
  return parts.join(' · ');
}

/** The screen's name for hints. */
export function offAirName(o: ChannelOptions): string {
  return { card: 'station card', bars: 'colour bars', static: 'snow', black: 'black screen', image: 'your picture' }[o.off_air.screen];
}

/** The amount of advertising in a few words, for the summary lines. */
export function breaksLabel(ads: AdConfig | null, presets: AdPreset[]): string {
  if (!ads) return 'No commercial breaks';
  if (!ads.commercials) return 'Station cards between shows, no commercials';
  const p = presets.find((x) => x.commercials && x.pod_seconds === ads.pod_seconds && x.max_breaks === ads.max_breaks && x.minutes_per_hour === ads.minutes_per_hour && x.mid_breaks === ads.mid_breaks);
  return p ? `Commercial breaks: ${p.name.toLowerCase()}` : 'Commercial breaks: custom';
}
