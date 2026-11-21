// The new-channel guide: plain answers in, the channel the server expects out.

import { defaultAds, emptyLineup } from './lib';
import { emptyOptions } from './options';
import type { AdConfig, AdPreset, Channel, ChannelOptions, Source, ThemeChoice } from './types';

export type PlayStyle = 'shuffle' | 'in_order' | 'weekly';

export const PLAY_STYLES: { value: PlayStyle; title: string; about: string }[] = [
  { value: 'shuffle', title: 'Mix it up', about: 'Episodes in a random order, shows mixed together, like channel surfing. Every episode comes round before any repeats.' },
  { value: 'in_order', title: 'In order, like a series', about: 'Each show goes episode by episode, remembering where it left off. Shows take turns.' },
  { value: 'weekly', title: 'A real TV schedule', about: 'Shows at set times on set days: cartoons at 8, a movie at 9. You lay out the week next.' },
];

export interface GuideAnswers {
  name: string;
  id: string;
  number: number;
  group: string;
  /** Shows to play; empty = everything in the libraries. */
  shows: string[];
  style: PlayStyle;
  /** `clean` = no breaks, else the id of a break preset. */
  breaks: string;
  /** This channel's own look; null = follow the station. */
  look: ThemeChoice | null;
  alwaysOn: boolean;
  /** Surprises, 0-100. */
  chaos: number;
}

export interface GuideWorld {
  timezone: string;
  /** Libraries of kind shows. */
  showLibraryIds: number[];
  /** Libraries of kind commercials. */
  commercialLibraryIds: number[];
  presets: AdPreset[];
}

/** The breaks the answers ask for (null = none). */
export function adsFor(breaks: string, world: GuideWorld): AdConfig | null {
  if (breaks === 'clean') return null;
  const p = world.presets.find((x) => x.id === breaks);
  if (!p) return null;
  return {
    ...defaultAds(),
    commercials: p.commercials,
    pod_seconds: p.pod_seconds,
    max_breaks: p.max_breaks,
    minutes_per_hour: p.minutes_per_hour,
    mid_breaks: p.mid_breaks,
    library_ids: world.commercialLibraryIds,
  };
}

/** The body for `POST /channels`. */
export function buildChannelBody(a: GuideAnswers, world: GuideWorld): Record<string, unknown> {
  const order = a.style === 'shuffle' ? 'shuffle' : 'sequential';
  // A weekly schedule has real gaps, and what goes in them is the title card; the other styles just keep playing.
  const fill: Source | null = a.style === 'weekly' ? { type: 'off_air' } : a.shows.length > 0 ? { type: 'pool', shows: a.shows, order } : null;
  const lineup: Channel['lineup'] = { ...emptyLineup(world.timezone), fill, ads: adsFor(a.breaks, world) };
  if (a.style === 'weekly') lineup.grid_minutes = 30;
  const options: ChannelOptions = { ...emptyOptions(), theme: a.look };
  return {
    id: a.id,
    number: a.number,
    name: a.name.trim(),
    group: a.group.trim() || undefined,
    kind: 'programming',
    strategy: a.style === 'shuffle' ? 'shuffle' : a.style === 'in_order' ? 'sequential' : 'block_grid',
    run_out: 'keep_going',
    chaos: a.style === 'weekly' ? 0 : a.chaos,
    autopilot: true,
    always_on: a.alwaysOn,
    library_ids: world.showLibraryIds,
    lineup,
    options,
  };
}

/** What the answers add up to, in a few plain lines. */
export function summarize(a: GuideAnswers, world: GuideWorld, totalShows: number): string[] {
  const out: string[] = [];
  out.push(`Channel ${a.number}, ${a.name.trim() || 'unnamed'}.`);
  out.push(a.shows.length === 0 ? `Plays everything in your libraries (${totalShows} show${totalShows === 1 ? '' : 's'}).` : `Plays ${a.shows.length} show${a.shows.length === 1 ? '' : 's'}: ${a.shows.slice(0, 4).join(', ')}${a.shows.length > 4 ? ` and ${a.shows.length - 4} more` : ''}.`);
  out.push(PLAY_STYLES.find((s) => s.value === a.style)!.title + (a.style === 'weekly' ? ': you lay out the week next, and time you leave empty shows the off-air screen.' : '.'));
  const p = world.presets.find((x) => x.id === a.breaks);
  out.push(a.breaks === 'clean' ? 'No commercial breaks.' : p ? `Breaks: ${p.id === 'none' ? 'station cards only' : p.name}.` : 'No commercial breaks.');
  out.push(a.look ? `Its own look (${a.look.theme}${a.look.gentle || a.look.theme === 'kids' ? ', gentle alerts' : ''}).` : 'Follows the station look.');
  if (a.alwaysOn) out.push('Always on: it keeps running even when nobody is watching.');
  return out;
}

/** The first thing wrong with the answers so far, for the step they are on. */
export function stepProblem(step: number, a: GuideAnswers, takenNumbers: number[], idError: string | null): string | null {
  if (step === 0) {
    if (!a.name.trim()) return 'Give the channel a name.';
    if (a.number < 1) return 'Pick a channel number.';
    if (takenNumbers.includes(a.number)) return `Channel ${a.number} is already used.`;
    if (idError) return idError;
  }
  return null;
}
