// Shapes of the daemon's JSON API (see docs/API.md).

export type LibraryKind = 'shows' | 'commercials' | 'bumpers' | 'music';

export interface Library {
  id: number;
  path: string;
  kind: LibraryKind;
  enabled: boolean;
  items: number;
  analysis?: { total: number; done: number; running: boolean } | null;
}

export type Order = 'sequential' | 'shuffle';
export type Advance = { weeks: number } | 'finale';

export type Source =
  | { type: 'show'; show: string }
  | { type: 'pool'; shows: string[]; order: Order }
  | { type: 'chaos' }
  | { type: 'rotation'; items: Source[]; advance: Advance };

export interface Block {
  name: string;
  /** 0 = Monday ... 6 = Sunday; empty = every day. */
  days: number[];
  start: string;
  end: string;
  source: Source;
}

export interface Season {
  name: string;
  from: string;
  to: string;
  blocks: Block[];
}

export interface EventDef {
  name: string;
  start: string;
  end: string;
  yearly: boolean;
  source: Source;
}

export interface BumperToggles {
  brb: boolean;
  back_to: boolean;
  up_next: boolean;
  station_id: boolean;
  later_today_every: number;
}

export interface AdConfig {
  commercials: boolean;
  library_ids: number[];
  bumper_library_ids: number[];
  minutes_per_hour: number;
  pod_seconds: number;
  max_breaks: number;
  no_repeat: number;
  years: [number, number] | null;
  mid_breaks: boolean;
  bumpers: BumperToggles;
}

export type CatchUp = 'resume' | 'advance';

export interface Lineup {
  timezone: string;
  grid_minutes: number | null;
  blocks: Block[];
  seasons: Season[];
  events: EventDef[];
  fill: Source | null;
  catch_up: CatchUp;
  ads: AdConfig | null;
  signoff: SignOff | null;
}

export type SignOffStyle = 'bars' | 'static' | 'card' | 'black' | 'image';

/** What fills the screen when nothing is on air. */
export type Screen = 'card' | 'bars' | 'static' | 'black' | 'image';

/** Off the air from `from` to `to` every day (local time). */
export interface SignOff {
  from: string;
  to: string;
  style: SignOffStyle;
}

export type Loudness = 'off' | 'light' | 'broadcast';
export type SubMode = 'off' | 'always' | 'forced' | 'foreign' | 'sdh';
export type Aspect = 'fit' | 'fill' | 'stretch';
export type Look = 'none' | 'vhs' | 'crt' | 'black_white' | 'sepia' | 'film';
export type Corner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';

export interface ShowOptions {
  audio_languages?: string[] | null;
  subtitles?: SubMode | null;
  subtitle_languages?: string[] | null;
  look?: Look | null;
  aspect?: Aspect | null;
}

export interface ChannelOptions {
  off_air: { screen: Screen; message: string; image: string | null; tone: boolean };
  audio: { languages: string[]; avoid_commentary: boolean; avoid_description: boolean; loudness: Loudness; night_mode: boolean; dialogue_boost: boolean; mono: boolean };
  subtitles: { mode: SubMode; languages: string[]; size: 'small' | 'medium' | 'large'; style: 'outline' | 'box' };
  video: { aspect: Aspect; deinterlace: 'auto' | 'off' | 'always'; quality: '480' | '720' | '1080'; fps: number | null; kbps: number | null; look: Look };
  overlay: {
    bug: { enabled: boolean; text: string; position: Corner; opacity: number };
    clock: { enabled: boolean; position: Corner; hour12: boolean };
    show_title: { enabled: boolean; seconds: number };
  };
  shows: Record<string, ShowOptions>;
}

export interface GuideConfig {
  hours: number;
  title: string | null;
  ticker: string[];
  music_library_ids: number[];
}

export type Strategy = 'sequential' | 'shuffle' | 'chaos' | 'block_grid';
export type RunOut = 'keep_going' | 'next_in_rotation' | 'chaos_fill' | 'hybrid';

export interface Channel {
  id: string;
  number: number;
  name: string;
  strategy: Strategy;
  run_out: RunOut;
  autopilot: boolean;
  chaos: number;
  seed: number;
  epoch_ms: number;
  always_on: boolean;
  library_ids: number[];
  lineup: Lineup;
  kind: 'programming' | 'guide' | 'test';
  group: string | null;
  guide: GuideConfig;
  options: ChannelOptions;
}

export interface Program {
  media_id: number;
  show: string | null;
  title: string;
  season: number | null;
  episode: number | null;
  start_ms: number;
  end_ms: number;
  runtime_end_ms: number;
  label: string | null;
}

export interface ChannelView extends Channel {
  on_air: boolean;
  now: Program | null;
  next: Program | null;
}

export interface Tv {
  id: string;
  name: string;
  channel: number | null;
  power: boolean;
  volume: number;
  muted: boolean;
  online: boolean;
}

export interface ShowSummary {
  name: string;
  episodes: number;
  total_ms: number;
  /** Audio languages found in the show's files (three-letter codes). */
  audio_languages: string[];
  subtitle_languages: string[];
  /** Files whose tracks have been read; fewer than `episodes` means a re-scan is due. */
  files_with_tracks: number;
  /** Settings that follow this show onto every channel (null = none). */
  defaults: ShowOptions | null;
}

export interface System {
  version: string;
  os: string;
  arch: string;
  timezone: string;
  ffmpeg: null | {
    version: string;
    encoder: string | null;
    has_libx264: boolean;
    has_videotoolbox: boolean;
    has_nvenc: boolean;
    has_drawtext: boolean;
    has_subtitles: boolean;
  };
  on_air: string[];
  auth_required: boolean;
  network?: Network;
}

/** Where the server can be reached, as it reports itself. */
export interface Network {
  hostname: string;
  /** Names first (they survive an address change), then IPv4 addresses. */
  hosts: string[];
  http_port: number | null;
  https_port: number | null;
  tls: { enabled: boolean; self_signed: boolean; fingerprint: string | null };
}

export interface FsList {
  path: string;
  parent: string | null;
  dirs: { name: string; path: string }[];
}

export interface ScheduleItem {
  kind: 'show' | 'commercial' | 'bumper' | 'filler';
  bumper: string | null;
  media_id: number | null;
  title: string | null;
  start_ms: number;
  end_ms: number;
  duration_ms: number;
  in_ms: number;
  label: string | null;
}

export interface Cursor {
  reserved: number;
  committed: number;
}

export interface AuthStatus {
  required: boolean;
  authenticated: boolean;
}
