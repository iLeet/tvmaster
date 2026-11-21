# Channel options: audio, subtitles, picture and graphics

Everything here is per channel and lives in the channel's `options` (and the sign-off hours in its `lineup`). In the web admin it is the channel's **Picture & sound** tab; the Mac app has the main settings in the channel editor.

## Audio tracks

Video files often carry several audio tracks: the original language, dubs, a director's commentary. When a library is scanned TVMaster reads every audio and subtitle stream (language, codec, channels, default/forced/hearing-impaired flags and titles) and stores them with the file. Re-scan a library once after upgrading to read tracks from files scanned earlier (the Per-show settings list says "Tracks not read yet").

At air time the channel chooses one track per programme:

1. The first of the channel's **preferred languages** that the file has (`audio.languages`, in order; `jpn`, `ja` or `Japanese` all work).
2. Commentary and audio-description tracks are skipped (`avoid_commentary`, `avoid_description`) unless they are all there is.
3. Among tracks in one language: the file's default, then the one with more channels.
4. No language matches: the file's own default track, then the first.

Commercials and clips have no choice to make; they play what they have.

Also: `loudness` (`off`, `light` = gentle levelling, `broadcast` = EBU R128, so shows and commercials come out equally loud), `night_mode` (compress dynamics), `dialogue_boost` (lift speech when mixing 5.1 to stereo), `mono` (one speaker, like an old set).

## Subtitles

Subtitles are **burned into the picture**, so every player (IPTV apps, smart TVs, Plex, the web player) shows them with no setup. Sources: the video file's own subtitle tracks, and subtitle files beside it (`Show S01E01.srt`, `.en.srt`, `.eng.forced.srt`, `.English.sdh.srt`; `srt`, `ass`, `ssa` and `vtt`).

`subtitles.mode`:

| Mode | What shows |
|---|---|
| `off` | Nothing. |
| `always` | A full subtitle in your language. |
| `forced` | Only forced subtitles (foreign phrases and signs inside the programme). |
| `foreign` | Full subtitles when the audio is not in one of your languages (Japanese audio, English wanted); only forced ones when it is. The right setting for a channel that mixes anime and sitcoms. |
| `sdh` | Like `always`, preferring captions for the hard of hearing. |

`subtitles.languages` is the order of preference (blank = the audio languages). `size` (`small`, `medium`, `large`) scales with the picture; `style` is `outline` or a caption `box`.

**Requirements.** Text subtitles are drawn by FFmpeg's libass; the server detects whether your FFmpeg has it (`has_subtitles` in `/api/v1/system`, and the channel editor warns when it does not). Picture subtitles (PGS, VobSub) need no libass. Homebrew's FFmpeg on macOS is built without libass; most Linux builds have it. If a subtitle cannot be prepared or the filter fails, the programme airs without it rather than failing, and the file is not reported unplayable.

## Settings that follow a show onto every channel

The **Shows** page of the web admin (and `PUT /api/v1/show-settings`) sets audio languages, subtitle mode and languages, look and screen shape for a show **once, for every channel that airs it**: anime in Japanese with English subtitles everywhere, or a show whose picture already has subtitles burned in set to subtitles **Off**. The order of precedence is:

1. this channel's own override for the show (Picture & sound tab, "this channel"),
2. the show's own settings (Shows page),
3. the channel's base settings.

The show's settings are read when each programme goes to air, so a change applies from the next programme without restarting anything.

## Per-show settings on one channel

`options.shows` maps a show name (any case) to overrides of `audio_languages`, `subtitles`, `subtitle_languages`, `look` and `aspect`. Anime in Japanese with English subtitles and everything else plain on the same channel is one entry. The editor lists every show with the languages found in its files.

## When nothing is on air

`options.off_air` chooses what fills the screen whenever there is nothing to show: an empty schedule, a gap between programmes, a file that cannot be played, the padding after a short episode. Before M13 this was always a dark card.

| `screen` | What |
|---|---|
| `card` (default) | A station card in the channel's colours: its number and name and your message (`message`, blank = PLEASE STAND BY). Drawn by TVMaster itself, so it needs no FFmpeg text support. |
| `bars` | SMPTE colour bars. |
| `static` | Snow. |
| `black` | Plain black. |
| `image` | A picture file of your own on the server (`image`: png, jpg, gif, webp, bmp), fitted to the screen. If the file goes missing the card is shown instead. |

`tone` adds a steady 1 kHz tone to the bars, black or picture, and hiss to the snow; off = silence. The sign-off hours use the same screens (bars and snow come with their tone and hiss).

**Test pattern channel.** A channel of kind `test` (Channels, New channel, Test pattern) airs only its off-air screen: colour bars and a tone by default, or whatever you choose. It needs no library and no schedule, so it is a ready-made test signal for checking a TV, a tuner or an IPTV app. Use it however you like: bars with a tone to set levels, black for a screen saver, a picture of your own as a "channel logo" channel.

## Picture

| Setting | Values |
|---|---|
| `video.quality` | `480` (854x480), `720` (default), `1080`; sets the channel's own output size and default bitrate |
| `video.fps`, `video.kbps` | frame rate 15-60 and bitrate 300-40000 kbit/s; blank = defaults |
| `video.aspect` | `fit` (bars where shapes differ), `fill` (crop the edges), `stretch` |
| `video.deinterlace` | `auto` (when the file says it is interlaced), `always`, `off` |
| `video.look` | `none`, `crt` (scanlines, vignette), `vhs` (noise, softness, washed colour), `film` (grain, warmth), `black_white`, `sepia` |

A look can also be set per show.

## On-screen graphics

| Setting | What |
|---|---|
| `overlay.bug` | The channel logo in a corner: `enabled`, `text` (blank = number and name), `image` (optional: a logo picture on the server, uploaded in the editor; shown instead of the text, fitted into a corner-sized box, any common format, its transparency kept), `position`, `opacity` 10-100. The text is drawn in the same chunky pixel font as the bumpers. A logo picture that cannot be read falls back to the text. |
| `overlay.show_title` | A "now playing" card with the show's name and episode when a programme starts (not after each commercial break), for `seconds` 3-30. |
| `overlay.clock` | A clock in a corner (`hour12`). **Needs FFmpeg with the `drawtext` filter** (the editor says when yours has none; no clock appears then). |

## Sign-off

`lineup.signoff` = `{ "from": "02:00", "to": "05:00", "style": "bars" }` (local time of the channel, a `to` before `from` runs overnight). During those hours the channel airs SMPTE colour bars with a 1 kHz tone (`bars`), snow and hiss (`static`) or the plain station card (`card`), and the schedule shows "Sign-off". It uses no episodes and moves no cursors, so what airs afterwards is undisturbed; a programme already running at the start finishes first, and the first show afterwards starts on the grid.

## Where the settings are

In the web admin: **Picture & sound** tab of each channel (the basics first: quality, subtitles, volume levelling and the on-screen extras; the rest under More options) (and a summary of it on the General tab), **Shows** page for per-show settings, **Screen** tab on a test channel. In the Mac app: the channel editor's Picture & sound tab (it says so when the server is too old to have these settings; the web admin is served by the server itself, so an old server shows the old pages).

## Applying changes

Saving a channel with different options restarts that channel's encode so the next viewer (or the always-on start) uses the new ones; viewers' players reconnect as for any restart. Changing options never re-rolls the schedule. Changing sign-off hours does (it is part of the lineup).

## What is verified

Unit tests cover track choice (languages, commentary, defaults, every subtitle mode, picture versus text), language normalisation, the FFmpeg arguments and filter graphs, sidecar file discovery and the sign-off schedule. Tests against real FFmpeg and real multi-track files measure the result on the stream: the chosen audio's pitch (Japanese 880 Hz versus English 440 Hz versus a commentary track), brightness of burned-in subtitles (embedded and beside-the-video), the logo and title card appearing and the card disappearing, a black-and-white picture having no colour, 480p/24 fps output, the sign-off bars and tone, and the fall-back when a subtitle filter is missing. Not tested: picture subtitles (no way to build a bitmap subtitle track here; the FFmpeg arguments are unit tested), hardware encoders with these filters, the clock (no drawtext on the macOS build used).

## When a show runs out

`run_out` decides what a time slot does once its show has aired every episode: `keep_going` (wrap to the first), `next_in_rotation`, `chaos_fill` (random content), `hybrid`, or **`off_air` ("Test screen")**. With `off_air` each episode airs once; when a show has run out, and for time outside your time slots (unless you set a `lineup.fill` yourself), the channel shows its off-air screen: the colour bars and tone, snow, black, a station card or your picture, whichever you chose under "When nothing is scheduled". "Start every show from episode 1" (Controls) makes the shows air again.

## Ticker

A strip of scrolling text across the picture, like a news channel's crawl (Picture & sound → On-screen extras → Ticker).

- **Messages**, one per line; **Add what is on next** ("Up next: Cheers at 8:30 PM"); **News feeds** (RSS or Atom
  addresses, up to 5) whose latest headlines are added, refreshed every 15 minutes.
- **Tab**: a word in a coloured tab at the left (NEWS, NOTICE...). **Where**: bottom or top. **Speed**: slow, normal, fast.
- **Every N minutes**: 0 keeps it on all the time; otherwise it appears every N minutes and crosses the screen once.
- It is drawn in the station's theme (Retro pixels or Modern smooth). The channel logo, the now-playing card and the
  clock keep out of its way. It is not drawn on guide channels (they have their own ticker) and gives way to an
  alert bar. Options: `options.overlay.ticker`.

