// Pure helpers for the guide channel's settings.

import type { GuideConfig } from './types';

/** A new guide channel's settings: today's look, nothing extra switched on. */
export function emptyGuide(): GuideConfig {
  return {
    hours: 3,
    title: null,
    ticker: [],
    music_library_ids: [],
    theme: 'auto',
    colors: { primary: null, accent: null, background: null },
    layout: { style: 'grid', rows: 6, scroll: 'page', page_seconds: 9, time_step: 30, channel_numbers: true, channel_names: true, progress: false, scanlines: true },
    look: { typeface: 'pixel', corners: 'square', text_size: 'medium', background: { style: 'gradient', image: null, dim: 45 }, header_style: 'bar', ticker_position: 'bottom', cell_times: false, now_marker: true, glass: false },
    header: { title: true, clock: true, date: true, hour12: true, logo: null },
    ticker_options: { enabled: true, speed: 'normal', now_on: true },
    channels: [],
    pip: { mode: 'off', channel: null, channels: [], seconds: 30, placement: 'side_right', size: 'medium', audio: 'music', info: true, libraries: [], commercial_seconds: 30, ad_title: '', ad_lines: [], ad_rotate_seconds: 6 },
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
    look: { ...d.look, ...x.look, background: { ...d.look.background, ...x.look?.background } },
    header: { ...d.header, ...x.header },
    ticker_options: { ...d.ticker_options, ...x.ticker_options },
    pip: { ...d.pip, ...x.pip },
  } as GuideConfig;
}

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
  if (g.look.background.style === 'image' && !g.look.background.image) return 'Choose a picture for the background.';
  if (g.theme === 'custom' && ((g.colors.primary && !cleanColor(g.colors.primary)) || (g.colors.accent && !cleanColor(g.colors.accent)))) return 'Colours look like #3366ff.';
  return null;
}

/** One-click looks: a bundle of colours and style options. */
export const GUIDE_PRESETS: { id: string; label: string; help: string; apply: (g: GuideConfig) => GuideConfig }[] = [
  {
    id: 'retro', label: 'Retro', help: 'The original: pixel letters, scanlines, square cells.',
    apply: (g) => ({ ...g, theme: 'auto', look: { ...g.look, typeface: 'pixel', corners: 'square', text_size: 'medium', header_style: 'bar', background: { ...g.look.background, style: 'gradient' } }, layout: { ...g.layout, scanlines: true } }),
  },
  {
    id: 'modern-dark', label: 'Modern dark', help: 'Smooth letters on midnight blue with rounded cells.',
    apply: (g) => ({ ...g, theme: 'midnight', look: { ...g.look, typeface: 'smooth', corners: 'rounded', text_size: 'medium', header_style: 'bar', background: { ...g.look.background, style: 'gradient' } }, layout: { ...g.layout, scanlines: false } }),
  },
  {
    id: 'modern-light', label: 'Clean white', help: 'Smooth letters on white, with programme times.',
    apply: (g) => ({ ...g, theme: 'snow', look: { ...g.look, typeface: 'smooth', corners: 'rounded', text_size: 'medium', header_style: 'bar', cell_times: true, background: { ...g.look.background, style: 'gradient' } }, layout: { ...g.layout, scanlines: false } }),
  },
  {
    id: 'minimal', label: 'Minimal', help: 'Graphite, no header bar, big readable rows.',
    apply: (g) => ({ ...g, theme: 'graphite', look: { ...g.look, typeface: 'smooth', corners: 'rounded', text_size: 'large', header_style: 'minimal', background: { ...g.look.background, style: 'solid' } }, layout: { ...g.layout, style: 'now_next', scanlines: false } }),
  },
];
