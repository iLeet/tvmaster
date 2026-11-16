# TVMaster

Self-hosted retro TV station and IPTV engine. Add your TV collections, and TVMaster runs real-feeling channels: complete chaos or scheduled time slots, retro commercials placed at natural breaks, "Up Next / Later Today" bumpers, a guide channel, and a phone/web remote. Runs headless or with a GUI on Windows, macOS and Linux (x64 and arm64).

**Status:** milestone 14 complete (a far more configurable guide channel with a live video window, an optional access token, picture uploads, a smart-TV player, watched folders and alerts, on top of M1-M13). See [docs/PLAN.md](docs/PLAN.md) for the full design and roadmap, and [docs/API.md](docs/API.md) for the API.

## What works today

- Library scanner: point it at a folder of shows; parses `Show/Season N/Show - S01E02 - Title.mkv` (and flat / `1x02` styles), probes with ffprobe, skips broken files.
- Channels: sequential, shuffle or chaos, or a full TV-style lineup (weekly blocks and dayparts, seasonal lineups, rotating or new shows, events and marathons, yearly holiday specials) on a local-time grid. A chaos dial blends structure into randomness. See [docs/SCHEDULING.md](docs/SCHEDULING.md).
- Autopilot: schedules are kept 72 hours ahead forever, episodes continue in order and wrap (or switch to chaos), cursors never skip or repeat across edits, downtime is handled (resume or advance), and bad files are covered and replaced automatically.
- Commercials and bumpers: shows are split at natural breaks (chapters or detected black frames), and each break is filled exactly with retro commercials plus "we'll be right back", "back to", "up next", "later today" and station-ID bumpers drawn by TVMaster itself. Filter commercials by year. See [docs/ADS.md](docs/ADS.md).
- Normalized HLS output (1280x720, H.264/AAC) from mismatched sources, gapless across items, about 0.7 s tune-in.
- A modern admin UI at `/admin/` (compiled into the binary, works headless from any browser): add libraries with a folder picker, build channels and lineups visually (lay out the week by dragging blocks on a Monday-to-Sunday grid), set up ads and bumpers, design the guide channel, view schedules, manage TVs. Installable as a phone remote app. Glass styling that follows the platform.
- Multiple audio tracks and subtitles: every channel chooses a language order (Japanese audio for anime, English for sitcoms, per show if you like), skips commentary tracks, and burns subtitles in (from the file or a `.srt` beside it) always, only when the audio is foreign, forced-only, or for the hard of hearing. See [docs/CHANNEL-OPTIONS.md](docs/CHANNEL-OPTIONS.md).
- What shows when nothing is on air is yours to choose (a station card with your words, colour bars with a tone, snow, black, or a picture of your own), and a test-pattern channel airs only that. Audio and subtitle settings can be set once per show on the Shows page and follow it onto every channel, with a per-channel override.
- TV-style channel options: volume levelling and night mode, 480p/720p/1080p, screen shape, deinterlacing, retro looks (CRT, VHS, film, black and white, sepia), a channel logo, a now-playing card, a clock, and signing off for the night with colour bars and a tone.
- Optional access token for the whole server: switch it on, off or replace it from Settings while the server runs (or fix it with `--token`).
- The guide channel: a live scrolling listing of your channels with a clock, a ticker and background music, always accurate because it reads the same schedule as everything else. Ten colour themes (or two colours of your own), a time grid or big "now and next" rows, 3-10 channels at once, paged or smooth rolling, 12- or 24-hour times, a logo, an optional ticker, a chosen list of channels, and a live picture-in-picture window that shows one channel or flips through several (with a card saying what is on, and either the music or the channel's own sound). Both the web admin and the Mac app edit it with a live preview. See [docs/GUIDE.md](docs/GUIDE.md).
- Upload a picture instead of typing a path: your own off-air screen and your channel logo.
- Smart TVs: a full-screen page for a TV's browser that works with CH ▲▼, number keys, OK for a channel list and Info, plus the IPTV and tuner routes. See [docs/SMART-TV.md](docs/SMART-TV.md).
- Watched folders: libraries are re-read every few minutes (only what changed), so new episodes appear by themselves. Problems (a library that goes offline, a channel that keeps failing, FFmpeg missing) show up on the Home screen and can be sent to ntfy, Slack, Discord or any web address.
- Many TVs can watch different channels at once; one encode per channel is shared by all viewers. Register each TV, and control it from a phone: the remote updates live over a WebSocket (channel, volume, mute, power, and whether the TV's player is connected).
- Tuner mode: Plex, Jellyfin and Emby can add TVMaster as an HDHomeRun-style network tuner. Channels can be grouped (Kids, Movies, ...).
- M3U playlist and XMLTV guide for VLC, Kodi, TiviMate and similar clients.
- Web player with channel list, now/next, and a phone remote that controls any registered TV.
- Watchdog restarts a crashed channel; config and TV state persist in SQLite. Unplugged drives, deleted or corrupt files and killed FFmpeg processes are survived without taking a channel off the air (see [docs/RELEASE.md](docs/RELEASE.md)).
- Service templates for launchd, systemd and Windows Task Scheduler.

Not yet: Apple-notarized and Windows-trusted signing (you need the certificates), and native Windows and Linux desktop apps (the web admin and the Mac app cover those jobs today).

## Quick start

Requires Rust (stable) and FFmpeg (`ffmpeg` and `ffprobe` on `PATH`).

```bash
cargo run -p tvmaster-cli -- serve
```

Then, from another terminal (or any HTTP client):

```bash
B=http://localhost:8787
curl -XPOST $B/api/v1/libraries -H 'content-type: application/json' -d '{"path":"/path/to/your/tv"}'
curl -XPOST $B/api/v1/libraries/1/scan
curl -XPOST $B/api/v1/channels -H 'content-type: application/json' \
  -d '{"id":"retro","number":4,"name":"Retro TV","strategy":"sequential","library_ids":[1]}'
```

For a real lineup (blocks, seasons, events), see the example in [docs/SCHEDULING.md](docs/SCHEDULING.md).

- Manage everything: `http://localhost:8787/admin/`
- Watch in a browser: `http://localhost:8787/`
- IPTV clients: playlist `http://<host>:8787/iptv/playlist.m3u`, guide `http://<host>:8787/iptv/guide.xml`
- Make a screen a TV: open `http://<host>:8787/?tv=living-room` on it (smart-TV browser, tablet, laptop)
- Phone remote for that TV: open `http://<host>:8787/?remote=living-room`
- Plex/Jellyfin/Emby: add a network tuner at `<host>:8787` (HDHomeRun) and use `http://<host>:8787/iptv/guide.xml` for guide data. This follows the HDHomeRun API those servers speak; it has not been tested against a real one yet.
- A guide channel: make one in the web admin (Channels, New channel) or `POST /api/v1/channels` with `"kind": "guide"` (see [docs/API.md](docs/API.md) and [docs/GUIDE.md](docs/GUIDE.md)).
- A smart TV: open `http://<host>:8787/?ui=tv` in its browser, or use an IPTV app (see [docs/SMART-TV.md](docs/SMART-TV.md)).

Protect it: turn the access token on in **Settings** (web admin or Mac app), or start the server with `tvmaster serve --token $(tvmaster token)`, which fixes it. Browsers sign in once; IPTV apps use the `?token=` that the playlist already contains. The packages start protected with a first-run token you can turn off or replace in Settings.

Encrypt it: `tvmaster serve --tls` also serves HTTPS on port 8788 with a self-signed certificate made once (kept in the data folder; `--tls-bind`, `--no-http` to change ports or serve only HTTPS). Bring your own with `--tls-cert` and `--tls-key` (Let's Encrypt, `tailscale cert`): it is re-read every few hours so renewals are picked up. The Mac app and the web admin show the certificate fingerprint to compare. IPTV apps and Plex usually refuse self-signed certificates, so leave HTTP on for them unless you use a certificate from a real authority.

Share it: the Mac app's **Share** screen and the web admin's **Settings** page list every address other apps and devices need (IPTV playlist and guide, Plex/Jellyfin/Emby tuner, each channel's stream, phone remote with a QR code), with copy buttons and a choice of address, HTTP or HTTPS, and whether to include the token.

Run unattended: `tvmaster service launchd|systemd|windows` prints a definition that starts on boot and restarts on failure.

Hardware encoding: VideoToolbox is used automatically on macOS; use `--encoder nvenc|qsv|vaapi|amf` to opt in to the others, `--encoder software` for libx264. `tvmaster doctor` encodes a test clip with every encoder in your FFmpeg and tells you which work and how fast. A hardware encoder that fails its start-up test falls back to software.

Install without Rust: a `.dmg` for macOS, a setup `.exe` for Windows, and `.deb`, `.rpm` and Arch packages for Linux, each of which sets up the server at boot with a first-run access token and FFmpeg (details and what is tested in [docs/RELEASE.md](docs/RELEASE.md)). There is also a `tar.gz` with `install.sh`, and a `Dockerfile`.

## Develop

The admin UI is in `web/` (`npm run dev` proxies to a running daemon; `npm run build && npm run sync` updates the copy embedded in the server, which is committed). The cross-platform desktop shell is in `apps/desktop`; the native macOS app is in `apps/macos` (see its README).

```bash
cargo test --workspace      # unit tests, the 3-week fake-clock soak, and the real-FFmpeg end-to-end and fault tests (those skip without FFmpeg)
cargo clippy --workspace --all-targets -- -D warnings
```

## License

TBD.
