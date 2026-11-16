// Pure helpers for the channel's picture, sound and on-screen options.
// Kept free of React so they can be unit tested.

import type { ChannelOptions, ShowOptions, ShowSummary } from './types';

/** Languages offered in the pickers: code (as the server stores it) and name. */
export const LANGUAGES: [string, string][] = [
  ['eng', 'English'], ['jpn', 'Japanese'], ['fra', 'French'], ['spa', 'Spanish'], ['deu', 'German'], ['ita', 'Italian'],
  ['por', 'Portuguese'], ['kor', 'Korean'], ['zho', 'Chinese'], ['rus', 'Russian'], ['ara', 'Arabic'], ['hin', 'Hindi'],
  ['nld', 'Dutch'], ['swe', 'Swedish'], ['nor', 'Norwegian'], ['dan', 'Danish'], ['fin', 'Finnish'], ['pol', 'Polish'],
  ['tur', 'Turkish'], ['ell', 'Greek'], ['heb', 'Hebrew'], ['ces', 'Czech'], ['hun', 'Hungarian'], ['ron', 'Romanian'],
  ['ukr', 'Ukrainian'], ['tha', 'Thai'], ['vie', 'Vietnamese'], ['ind', 'Indonesian'], ['fil', 'Filipino'], ['cat', 'Catalan'],
];

const NAMES = new Map(LANGUAGES);

/** "eng" -> "English"; anything unknown is shown as written. */
export function langName(code: string): string {
  const c = code.trim().toLowerCase();
  return NAMES.get(c) ?? (code.trim() || 'Unknown');
}

export function emptyOptions(): ChannelOptions {
  return {
    audio: { languages: [], avoid_commentary: true, avoid_description: true, loudness: 'off', night_mode: false, dialogue_boost: false, mono: false },
    subtitles: { mode: 'off', languages: [], size: 'medium', style: 'outline' },
    video: { aspect: 'fit', deinterlace: 'auto', quality: '720', fps: null, kbps: null, look: 'none' },
    overlay: {
      bug: { enabled: false, text: '', position: 'top_right', opacity: 60 },
      clock: { enabled: false, position: 'bottom_right', hour12: true },
      show_title: { enabled: false, seconds: 8 },
    },
    shows: {},
  };
}

/** Fill anything an older server did not send, so the editor never sees a hole. */
export function withDefaults(o: Partial<ChannelOptions> | null | undefined): ChannelOptions {
  const d = emptyOptions();
  return {
    audio: { ...d.audio, ...o?.audio },
    subtitles: { ...d.subtitles, ...o?.subtitles },
    video: { ...d.video, ...o?.video },
    overlay: {
      bug: { ...d.overlay.bug, ...o?.overlay?.bug },
      clock: { ...d.overlay.clock, ...o?.overlay?.clock },
      show_title: { ...d.overlay.show_title, ...o?.overlay?.show_title },
    },
    shows: { ...o?.shows },
  };
}

export function addLanguage(list: string[], code: string): string[] {
  const c = code.trim().toLowerCase();
  return !c || list.includes(c) ? list : [...list, c];
}

export function removeLanguage(list: string[], code: string): string[] {
  return list.filter((l) => l !== code);
}

/** Move the language at `i` one place earlier (-1) or later (+1) in the order of preference. */
export function moveLanguage(list: string[], i: number, dir: -1 | 1): string[] {
  const j = i + dir;
  if (i < 0 || i >= list.length || j < 0 || j >= list.length) return list;
  const out = [...list];
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}

const isEmpty = (o: ShowOptions) =>
  !(o.audio_languages && o.audio_languages.length) && !o.subtitles && !(o.subtitle_languages && o.subtitle_languages.length) && !o.look && !o.aspect;

/** Change one show's override; an override with nothing in it is removed. Names match regardless of case. */
export function setShowOption(opts: ChannelOptions, show: string, patch: ShowOptions): ChannelOptions {
  const key = Object.keys(opts.shows).find((k) => k.toLowerCase() === show.trim().toLowerCase()) ?? show.trim();
  const next: ShowOptions = { ...opts.shows[key], ...patch };
  for (const k of Object.keys(next) as (keyof ShowOptions)[]) if (next[k] === undefined || next[k] === null) delete next[k];
  const shows = { ...opts.shows };
  if (isEmpty(next)) delete shows[key];
  else shows[key] = next;
  return { ...opts, shows };
}

export function showOverride(opts: ChannelOptions, show: string): ShowOptions {
  const key = Object.keys(opts.shows).find((k) => k.toLowerCase() === show.trim().toLowerCase());
  return key ? opts.shows[key] : {};
}

/** What the files of a show contain, in a sentence; says when a re-scan is needed to know. */
export function trackNote(s: Pick<ShowSummary, 'episodes' | 'audio_languages' | 'subtitle_languages' | 'files_with_tracks'>): string {
  if (s.files_with_tracks === 0) return 'Tracks not read yet: re-scan the library';
  const a = s.audio_languages.length ? s.audio_languages.map(langName).join(', ') : 'no language tags';
  const sub = s.subtitle_languages.length ? s.subtitle_languages.map(langName).join(', ') : 'none';
  const partial = s.files_with_tracks < s.episodes ? ` (${s.files_with_tracks} of ${s.episodes} files read)` : '';
  return `Audio: ${a}. Subtitles: ${sub}.${partial}`;
}

/** Is this show's audio available in a language the channel asks for? Used to warn about a wasted preference. */
export function audioMatches(s: Pick<ShowSummary, 'audio_languages'>, languages: string[]): boolean {
  if (languages.length === 0 || s.audio_languages.length === 0) return true;
  return languages.some((l) => s.audio_languages.includes(l.trim().toLowerCase()));
}
