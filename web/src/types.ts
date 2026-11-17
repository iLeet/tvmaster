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
  /** Air one episode at the start time, then the gap policy (test screen...) for the rest of the slot. */
  once?: boolean;
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
    bug: { enabled: boolean; text: string; image: string | null; position: Corner; opacity: number };
    clock: { enabled: boolean; position: Corner; hour12: boolean };
    show_title: { enabled: boolean; seconds: number };
    ticker: TickerOptions;
  };
  shows: Record<string, ShowOptions>;
  /** This channel's own theme; absent = the station theme. */
  theme?: ThemeChoice | null;
}

export type GuideTheme = 'auto' | 'prevue' | 'amber' | 'phosphor' | 'neon' | 'sunset' | 'ocean' | 'slate' | 'paper' | 'midnight' | 'graphite' | 'snow' | 'forest' | 'berry' | 'mono' | 'custom';
export interface TickerOptions {
  enabled: boolean;
  lines: string[];
  up_next: boolean;
  feeds: string[];
  label: string;
  position: 'bottom' | 'top';
  speed: 'slow' | 'normal' | 'fast';
  every_minutes: number;
}
export type GraphicsStyle = 'retro' | 'modern';
export interface ThemeColors { primary: string | null; accent: string | null; background: string | null }
/** One look for everything the station draws: guide, cards, bumpers, stand-by, alerts, graphics. */
export interface ThemeChoice { theme: GuideTheme; colors: ThemeColors; style: GraphicsStyle }
export interface ThemeCatalogue {
  themes: { id: GuideTheme; name: string; about: string; swatch: [string, string, string] }[];
  styles: { id: GraphicsStyle; name: string; about: string }[];
  station: ThemeChoice;
}
export interface AdPreset { id: string; name: string; about: string; commercials: boolean; pod_seconds: number; max_breaks: number; minutes_per_hour: number; mid_breaks: boolean }
export interface AdOptions { presets: AdPreset[]; cards: { id: 'brb' | 'back_to' | 'up_next' | 'later_today' | 'station_id'; name: string; about: string }[] }
export type PipMode = 'off' | 'channel' | 'rotate';
export type PipPlacement = 'top_left' | 'top_right' | 'side_left' | 'side_right';

export interface GuideConfig {
  hours: number;
  title: string | null;
  ticker: string[];
  music_library_ids: number[];
  theme: GuideTheme;
  colors: { primary: string | null; accent: string | null; background: string | null };
  layout: {
    style: 'grid' | 'now_next';
    rows: number;
    scroll: 'page' | 'smooth';
    page_seconds: number;
    time_step: 15 | 30 | 60;
    channel_numbers: boolean;
    channel_names: boolean;
    progress: boolean;
    scanlines: boolean;
  };
  look: {
    typeface: 'pixel' | 'smooth';
    corners: 'square' | 'rounded';
    text_size: 'small' | 'medium' | 'large';
    background: { style: 'gradient' | 'solid' | 'stripes' | 'image'; image: string | null; dim: number };
    header_style: 'bar' | 'minimal';
    ticker_position: 'bottom' | 'top';
    cell_times: boolean;
    now_marker: boolean;
  };
  header: { title: boolean; clock: boolean; date: boolean; hour12: boolean; logo: string | null };
  ticker_options: { enabled: boolean; speed: 'slow' | 'normal' | 'fast'; now_on: boolean };
  /** Only these channels, in this order; empty = every programming channel. */
  channels: string[];
  pip: {
    mode: PipMode;
    channel: string | null;
    channels: string[];
    seconds: number;
    placement: PipPlacement;
    size: 'small' | 'medium' | 'large';
    audio: 'music' | 'channel';
    info: boolean;
  };
}

export type Strategy = 'sequential' | 'shuffle' | 'chaos' | 'block_grid';
export type RunOut = 'keep_going' | 'next_in_rotation' | 'chaos_fill' | 'hybrid' | 'off_air';

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
  /** The folders (detected names) this show is made of. */
  folders: ShowFolder[];
  /** Description, rating, tags and poster (null = none). */
  info: ShowInfo | null;
}

export interface ShowFolder {
  raw: string;
  episodes: number;
  /** The season this folder stands for, if known. */
  season: number | null;
  /** A rule of your own covers it, rather than the automatic clean-up. */
  custom: boolean;
}

export interface ShowInfo {
  title?: string;
  description: string;
  rating: string;
  tags: string[];
  poster: string | null;
}

export interface System {
  /** The server's data folder (database, caches, uploads). */
  data_dir?: string | null;
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

export interface Alert {
  id: string;
  severity: 'error' | 'warning';
  title: string;
  detail: string;
  since_ms: number;
}

export interface MonitorSettings {
  watch_minutes: number;
  alerts: boolean;
  webhook: string | null;
  webhook_text: boolean;
  notify_recovery: boolean;
}

export interface MonitorState {
  settings: MonitorSettings;
  alerts: Alert[];
  rescans: Record<string, { at_ms: number; added: number; removed: number; back: number }>;
  /** `curl` is available to send the webhook. */
  curl: boolean;
}

export interface TimezoneState {
  /** The zone new schedules are made in. */
  zone: string;
  source: 'setting' | 'environment' | 'machine';
  setting: string | null;
  environment: string | null;
  machine: string | null;
  channels_elsewhere: { id: string; name: string; zone: string }[];
}

export interface SecurityStatus {
  /** Anyone may watch (streams, playlists, the player) without the token. */
  open_viewing?: boolean;
  enabled: boolean;
  /** Set with --token on the command line, so it cannot be changed here. */
  locked: boolean;
  token: string | null;
}

export interface AuthStatus {
  required: boolean;
  authenticated: boolean;
}

export type LiveMode = 'test_screen' | 'off_air' | 'black' | 'static' | 'alert';

export interface LiveOverride {
  channel: string;
  mode: LiveMode;
  title: string;
  message: string;
  style?: string;
  theme?: string;
  severity?: string;
  sound?: string;
  started_ms: number;
  until_ms: number | null;
}

export interface LiveSample {
  id: string;
  name: string;
  event: string;
  text: string;
  severity?: string;
  sound?: string;
}

export interface LiveChoice {
  value: string;
  label: string;
}

export interface LiveState {
  overrides: LiveOverride[];
  samples: LiveSample[];
  options?: { styles: LiveChoice[]; themes: LiveChoice[]; severities: LiveChoice[]; sounds: LiveChoice[] };
}

export interface SetupState {
  needed: boolean;
  libraries: { shows: number; commercials: number; bumpers: number; music: number };
  items: number;
  shows: number;
  channels: number;
  station_folder: boolean;
  can_upload: boolean;
  theme: ThemeChoice;
}

export interface BackupInfo { name: string; size: number; created_ms: number; label: string; auto: boolean; version: string; counts: { channels: number; libraries: number; files: number } }
export interface BackupList { auto: boolean; keep: number; backups: BackupInfo[] }
export interface BackupRestored { channels: number; libraries: number; files: number; pictures: number; missing_folders: string[]; safety_copy: string }

export interface PromoOptions {
  kinds: { id: 'sponsor' | 'coming_up' | 'slides'; name: string; about: string }[];
  looks: { id: string; name: string }[];
  sounds: { id: 'jingle' | 'music' | 'none'; name: string }[];
  seconds: number[];
}
export interface PromoSpot { id: number; name: string; seconds: number; path: string; library: number }
export interface PromoDraft {
  kind: 'sponsor' | 'coming_up' | 'slides';
  title: string; tagline: string; contact: string; slides: string[];
  seconds: number; picture: string | null; theme: ThemeChoice | null; look: string; sound: 'jingle' | 'music' | 'none'; music: string | null; year: number | null; name: string;
}
