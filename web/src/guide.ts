// Pure helpers for the guide channel's settings.

import type { GuideConfig, GuideTheme } from './types';

/** A new guide channel's settings: today's look, nothing extra switched on. */
export function emptyGuide(): GuideConfig {
  return {
    hours: 3,
    title: null,
    ticker: [],
    music_library_ids: [],
    theme: 'auto',
    colors: { primary: null, accent: null },
    layout: { style: 'grid', rows: 6, scroll: 'page', page_seconds: 9, time_step: 30, channel_numbers: true, channel_names: true, progress: false, scanlines: true },
    header: { title: true, clock: true, date: true, hour12: true, logo: null },
    ticker_options: { enabled: true, speed: 'normal', now_on: true },
    channels: [],
    pip: { mode: 'off', channel: null, channels: [], seconds: 30, placement: 'side_right', size: 'medium', audio: 'music', info: true },
  };
}

/** Fill in anything an older server did not send, so the editor never meets a missing option. */
export function withGuideDefaults(g: Partial<GuideConfig> | undefined): GuideConfig {
  const d = emptyGuide();
  const x = g ?? {};
  return {
    ...d,
    ...x,
    colors: { ...d.colors, ...x.colors },
    layout: { ...d.layout, ...x.layout },
    header: { ...d.header, ...x.header },
    ticker_options: { ...d.ticker_options, ...x.ticker_options },
    pip: { ...d.pip, ...x.pip },
  } as GuideConfig;
}

/** The themes, with the two colours that sum each one up for its swatch. */
export const GUIDE_THEMES: { value: GuideTheme; label: string; help: string; colors: [string, string] }[] = [
  { value: 'auto', label: 'Station', help: "Colours made from the channel itself, so each guide looks like its own station.", colors: ['#6b3cc4', '#f2d03c'] },
  { value: 'prevue', label: 'Cable blue', help: 'Deep blue and yellow, like the classic cable listings.', colors: ['#2c54e0', '#ffe040'] },
  { value: 'amber', label: 'Amber', help: 'Amber on black, like an old monochrome monitor.', colors: ['#ffb000', '#ffd666'] },
  { value: 'phosphor', label: 'Green screen', help: 'Green phosphor.', colors: ['#28dc50', '#aaffaa'] },
  { value: 'neon', label: 'Neon', help: 'Hot pink and cyan on purple-black.', colors: ['#ff28c8', '#00f0ff'] },
  { value: 'sunset', label: 'Sunset', help: 'Orange and gold on deep plum.', colors: ['#ff6e3c', '#ffc850'] },
  { value: 'ocean', label: 'Ocean', help: 'Teal and aqua on deep blue.', colors: ['#00a0c8', '#78f0dc'] },
  { value: 'slate', label: 'Slate', help: 'Cool grey with a warm yellow highlight.', colors: ['#707c94', '#f0c83c'] },
  { value: 'paper', label: 'Paper', help: 'Light, like a printed listing.', colors: ['#fffcf0', '#c83c28'] },
  { value: 'custom', label: 'Your colours', help: 'Pick the main colour and the highlight.', colors: ['#3366ff', '#ffcc00'] },
];

export const PIP_PLACEMENTS: { value: GuideConfig['pip']['placement']; label: string }[] = [
  { value: 'side_right', label: 'Side, right' },
  { value: 'side_left', label: 'Side, left' },
  { value: 'top_left', label: 'Top, left' },
  { value: 'top_right', label: 'Top, right' },
];

/** Add `id` to the end of an ordered list, or remove it if it is there. */
export function toggleId(list: string[], id: string): string[] {
  return list.includes(id) ? list.filter((x) => x !== id) : [...list, id];
}

/** `#rrggbb` as typed, or null when it is not one. */
export function cleanColor(s: string): string | null {
  const t = s.trim();
  return /^#[0-9a-fA-F]{6}$/.test(t) ? t.toLowerCase() : null;
}

/** What the video window needs before the server will accept the guide. */
export function guideProblem(g: GuideConfig): string | null {
  if (g.pip.mode === 'channel' && !g.pip.channel) return 'Choose the channel to show in the video window.';
  if (g.theme === 'custom' && ((g.colors.primary && !cleanColor(g.colors.primary)) || (g.colors.accent && !cleanColor(g.colors.accent)))) return 'Colours look like #3366ff.';
  return null;
}
