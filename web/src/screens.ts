// Screen designs: the shape the server draws (see docs/SCREENS.md) and the small helpers the editor uses.

export type Enter = 'none' | 'fade' | 'slide_up' | 'slide_down' | 'slide_left' | 'slide_right' | 'zoom' | 'type' | 'wipe';
export type Motion = 'none' | 'pulse' | 'float' | 'drift' | 'blink' | 'marquee';
export type Align = 'left' | 'center' | 'right';

export interface Anim {
  enter: Enter;
  enter_ms: number;
  delay_ms: number;
  motion: Motion;
  speed: number;
}

interface Placed {
  x: number;
  y: number;
  align: Align;
  opacity: number;
  anim: Anim;
}

export type Layer = Placed &
  (
    | { type: 'text'; text: string; size: number; bold: boolean; pixel: boolean; color: string; shadow: boolean; width: number; lines: number }
    | { type: 'clock'; size: number; bold: boolean; color: string; hour12: boolean; seconds: boolean; date: boolean }
    | { type: 'shape'; w: number; h: number; color: string; color2: string | null; radius: number; circle: boolean; glass: boolean }
    | { type: 'list'; size: number; color: string; time_color: string; rows: number; width: number }
    | { type: 'image'; path: string; w: number; h: number }
  );

export type LayerType = Layer['type'];

export type Backdrop =
  | { type: 'theme' }
  | { type: 'solid'; color: string }
  | { type: 'gradient'; top: string; bottom: string }
  | { type: 'aurora'; colors: string[] }
  | { type: 'bars' }
  | { type: 'pattern'; kind: 'dots' | 'grid' | 'stripes'; color: string; bg: string; size: number }
  | { type: 'picture'; path: string };

export interface Design {
  name: string;
  backdrop: Backdrop;
  layers: Layer[];
  fade_out_ms: number;
}

export interface ScreenPreset {
  id: string;
  name: string;
  about: string;
  design: Design;
}

export interface ScreenKind {
  id: string;
  name: string;
  about: string;
  words: string[];
  seconds: number;
  presets: ScreenPreset[];
  current: { preset: string; design: Design } | null;
  source: 'none' | 'station' | 'channel';
}

export interface ScreensState {
  kinds: ScreenKind[];
  colors: string[];
}

export const ENTERS: { value: Enter; label: string }[] = [
  { value: 'none', label: 'Just appears' },
  { value: 'fade', label: 'Fades in' },
  { value: 'slide_up', label: 'Slides up' },
  { value: 'slide_down', label: 'Slides down' },
  { value: 'slide_left', label: 'Slides in from the right' },
  { value: 'slide_right', label: 'Slides in from the left' },
  { value: 'zoom', label: 'Zooms in' },
  { value: 'type', label: 'Types itself out' },
  { value: 'wipe', label: 'Wipes across' },
];

export const MOTIONS: { value: Motion; label: string }[] = [
  { value: 'none', label: 'Stays still' },
  { value: 'pulse', label: 'Breathes' },
  { value: 'float', label: 'Floats' },
  { value: 'drift', label: 'Drifts around the screen' },
  { value: 'blink', label: 'Fades in and out' },
  { value: 'marquee', label: 'Scrolls across' },
];

/** Colour roles follow the station theme; anything starting with # is a colour of your own. */
export const COLOUR_ROLES: { value: string; label: string }[] = [
  { value: 'primary', label: 'Theme main colour' },
  { value: 'accent', label: 'Theme highlight' },
  { value: 'accent_text', label: 'Theme highlight (easy to read)' },
  { value: 'primary_text', label: 'Theme main (easy to read)' },
  { value: 'ink', label: 'Theme text' },
  { value: 'dark', label: 'Theme dark' },
  { value: 'bottom', label: 'Theme darkest' },
  { value: 'on_accent', label: 'Text on the highlight' },
  { value: 'white', label: 'White' },
  { value: 'black', label: 'Black' },
];

export const defaultAnim = (): Anim => ({ enter: 'fade', enter_ms: 600, delay_ms: 0, motion: 'none', speed: 100 });

const place = (x = 500, y = 500): Placed => ({ x, y, align: 'center', opacity: 100, anim: defaultAnim() });

/** A new layer of a kind, with sensible settings. */
export function newLayer(type: LayerType): Layer {
  switch (type) {
    case 'text':
      return { ...place(), type, text: 'Your words', size: 64, bold: true, pixel: false, color: 'ink', shadow: false, width: 900, lines: 2 };
    case 'clock':
      return { ...place(500, 700), type, size: 72, bold: true, color: 'ink', hour12: true, seconds: false, date: false };
    case 'shape':
      return { ...place(), type, w: 500, h: 250, color: 'accent', color2: null, radius: 32, circle: false, glass: false };
    case 'list':
      return { ...place(), type, size: 44, color: 'ink', time_color: 'accent_text', rows: 4, width: 800 };
    case 'image':
      return { ...place(), type, path: '', w: 250, h: 250 };
  }
}

/** The text layers, in order, with their place in the design. */
export function textLayers(d: Design): { index: number; layer: Extract<Layer, { type: 'text' }> }[] {
  return d.layers.flatMap((l, index) => (l.type === 'text' ? [{ index, layer: l }] : []));
}

export function setLayer(d: Design, index: number, patch: Partial<Layer>): Design {
  return { ...d, layers: d.layers.map((l, i) => (i === index ? ({ ...l, ...patch } as Layer) : l)) };
}

export function moveLayer(d: Design, index: number, by: -1 | 1): Design {
  const j = index + by;
  if (j < 0 || j >= d.layers.length) return d;
  const layers = [...d.layers];
  [layers[index], layers[j]] = [layers[j], layers[index]];
  return { ...d, layers };
}

export function removeLayer(d: Design, index: number): Design {
  return { ...d, layers: d.layers.filter((_, i) => i !== index) };
}

export function addLayer(d: Design, type: LayerType): Design {
  return { ...d, layers: [...d.layers, newLayer(type)] };
}

/** How lively the motion is, 10-500 percent, set on every layer at once. */
export function setMotionSpeed(d: Design, speed: number): Design {
  const s = Math.max(10, Math.min(500, Math.round(speed)));
  return { ...d, layers: d.layers.map((l) => ({ ...l, anim: { ...l.anim, speed: s } })) };
}

/** The motion speed the design mostly has (for the slider). */
export function motionSpeed(d: Design): number {
  return d.layers[0]?.anim.speed ?? 100;
}

/** Turn every layer's continuing motion off (entrances stay). */
export function stillDesign(d: Design): Design {
  return { ...d, layers: d.layers.map((l) => ({ ...l, anim: { ...l.anim, motion: 'none' as Motion } })) };
}

export function hasMotion(d: Design): boolean {
  return d.layers.some((l) => l.anim.motion !== 'none');
}

/** A colour setting as a value `<input type="color">` can show. */
export function colourToHex(c: string, fallback = '#ffffff'): string {
  if (/^#[0-9a-f]{6}$/i.test(c)) return c;
  const m = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(c);
  return m ? `#${m[1]}${m[1]}${m[2]}${m[2]}${m[3]}${m[3]}` : fallback;
}

export const isRole = (c: string) => COLOUR_ROLES.some((r) => r.value === c);

/** Which of the gallery's designs a saved one started as (or "custom" once it was changed). */
export function presetFor(k: ScreenKind): string {
  return k.current?.preset ?? 'classic';
}

/** Whether two designs are the same, ignoring the name. */
export function sameDesign(a: Design, b: Design): boolean {
  return JSON.stringify({ ...a, name: '' }) === JSON.stringify({ ...b, name: '' });
}

/** What the words in the editor are for. */
export const WORD_HELP: Record<string, string> = {
  channel: 'the channel’s name',
  number: 'its number',
  message: 'the stand-by message',
  clock: 'the time',
  date: 'today’s date',
  show: 'the show that is coming back',
  title: 'the next programme',
  time: 'when it starts',
};
