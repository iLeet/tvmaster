// Pure helpers for the info channel's settings.

import type { InfoConfig, InfoSlide, SlideKind } from './types';

export function emptyInfo(): InfoConfig {
  return { about: null, title: null, slides: [], seconds: 12, feeds: [], ticker: [], ticker_enabled: true, clock: true, hour12: true, music_library_ids: [] };
}

/** Fill in anything an older server did not send. */
export function withInfoDefaults(i: Partial<InfoConfig> | undefined): InfoConfig {
  return { ...emptyInfo(), ...(i ?? {}) } as InfoConfig;
}

export function newSlide(kind: SlideKind): InfoSlide {
  return {
    kind,
    title: kind === 'headlines' ? 'Headlines' : kind === 'tonight' ? 'On tonight' : kind === 'text' ? 'Notice' : '',
    lines: [],
    picture: null,
    seconds: null,
  };
}

/** What the server will refuse, said before it does. */
export function infoProblem(i: InfoConfig): string | null {
  if (i.slides.some((s) => s.kind === 'headlines') && i.feeds.length === 0) return 'A headlines page needs at least one news feed: add one under Headlines.';
  const n = i.slides.findIndex((s) => s.kind === 'photo' && !s.picture);
  if (n >= 0) return `Page ${n + 1} is a picture page and needs a picture.`;
  const t = i.slides.findIndex((s) => s.kind === 'text' && !s.title.trim() && s.lines.every((l) => !l.trim()));
  if (t >= 0) return `Page ${t + 1} is a notice with nothing in it.`;
  const f = i.feeds.find((u) => !/^https?:\/\/\S+$/.test(u));
  if (f !== undefined) return `“${f}” is not a web address: news feeds start with http:// or https://`;
  return null;
}

/** Move the item at `from` to `to` (a copy). */
export function move<T>(list: T[], from: number, to: number): T[] {
  if (to < 0 || to >= list.length || from === to) return list;
  const out = list.slice();
  const [x] = out.splice(from, 1);
  out.splice(to, 0, x);
  return out;
}
