# TVMaster

Self-hosted retro TV station and IPTV engine. Add your TV collections, and TVMaster runs real-feeling channels: complete chaos or scheduled time slots, retro commercials placed at natural breaks, "Up Next / Later Today" bumpers, a guide channel, and a phone/web remote. Runs headless or with a GUI on Windows, macOS and Linux (x64 and arm64).

**Status:** milestone 5 complete: guide channel, tuner support, live remote. See [docs/PLAN.md](docs/PLAN.md) for the full design and roadmap, and [docs/API.md](docs/API.md) for the API.

## What works today

- Library scanner: point it at a folder of shows; parses `Show/Season N/Show - S01E02 - Title.mkv` (and flat / `1x02` styles), probes with ffprobe, skips broken files.
- Channels: sequential, shuffle or chaos, or a full TV-style lineup (weekly blocks and dayparts, seasonal lineups, rotating or new shows, events and marathons, yearly holiday specials) on a local-time grid. A chaos dial blends structure into randomness. See [docs/SCHEDULING.md](docs/SCHEDULING.md).
- Autopilot: schedules are kept 72 hours ahead forever, episodes continue in order and wrap (or switch to chaos), cursors never skip or repeat across edits, downtime is handled (resume or advance), and bad files are covered and replaced automatically.
- Commercials and bumpers: shows are split at natural breaks (chapters or detected black frames), and each break is filled exactly with retro commercials plus "we'll be right back", "back to", "up next", "later today" and station-ID bumpers drawn by TVMaster itself. Filter commercials by year. See [docs/ADS.md](docs/ADS.md).
- Normalized HLS output (1280x720, H.264/AAC) from mismatched sources, gapless across items, about 0.7 s tune-in.
- Channel 3, the guide channel: a live Prevue-style scrolling grid of every channel with a clock, a "now on" ticker and background music. Always accurate because it reads the same schedule as everything else.
- Many TVs can watch different channels at once; one encode per channel is shared by all viewers. Register each TV, and control it from a phone: the remote updates live over a WebSocket (channel, volume, mute, power, and whether the TV's player is connected).
- Tuner mode: Plex, Jellyfin and Emby can add TVMaster as an HDHomeRun-style network tuner. Channels can be grouped (Kids, Movies, ...).
- M3U playlist and XMLTV guide for VLC, Kodi, TiviMate and similar clients.
- Web player with channel list, now/next, and a phone remote that controls any registered TV.
- Watchdog restarts a crashed channel; config and TV state persist in SQLite.
- Service templates for launchd, systemd and Windows Task Scheduler.

Not yet: the web admin UI, desktop/native apps, authentication (see the roadmap).

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

- Watch in a browser: `http://localhost:8787/`
- IPTV clients: playlist `http://<host>:8787/iptv/playlist.m3u`, guide `http://<host>:8787/iptv/guide.xml`
- Make a screen a TV: open `http://<host>:8787/?tv=living-room` on it (smart-TV browser, tablet, laptop)
- Phone remote for that TV: open `http://<host>:8787/?remote=living-room`
- Plex/Jellyfin/Emby: add a network tuner at `<host>:8787` (HDHomeRun) and use `http://<host>:8787/iptv/guide.xml` for guide data. This follows the HDHomeRun API those servers speak; it has not been tested against a real one yet.
- A guide channel: `POST /api/v1/channels` with `"kind": "guide"` (see [docs/API.md](docs/API.md)).

Run unattended: `tvmaster service launchd|systemd|windows` prints a definition that starts on boot and restarts on failure.

Hardware encoding: VideoToolbox is used automatically on macOS; use `--encoder nvenc` to opt in to NVENC, `--encoder software` for libx264.

## Develop

```bash
cargo test --workspace      # unit tests; the HLS end-to-end test runs when FFmpeg is installed
cargo clippy --workspace --all-targets -- -D warnings
```

## License

TBD.
