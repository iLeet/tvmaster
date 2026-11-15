# TVMaster

Self-hosted retro TV station and IPTV engine. Add your TV collections, and TVMaster runs real-feeling channels: complete chaos or scheduled time slots, retro commercials placed at natural breaks, "Up Next / Later Today" bumpers, a guide channel, and a phone/web remote. Runs headless or with a GUI on Windows, macOS and Linux (x64 and arm64).

**Status:** early scaffolding (milestone 1). See [docs/PLAN.md](docs/PLAN.md) for the full design and [docs/API.md](docs/API.md) for the API draft.

## Highlights (planned)

- One stream per channel; many TVs can watch different channels at once.
- Works with VLC, Plex/Jellyfin/Emby (HDHomeRun emulation), Kodi, TiviMate, plus a built-in web player.
- Phone/web remote to change channels on any registered TV.
- Autopilot: never runs out. Episodes continue in order and wrap, or fall back to chaos.
- Commercial pods and bumpers generated automatically from the schedule.
- Channel 3 guide channel that always shows what's next.
- Native Liquid Glass app on macOS; modern glass UI on Windows and Linux; web UI everywhere.

## Build

Requires Rust (stable) and FFmpeg on `PATH` for now.

```bash
cargo build
cargo test
cargo run -p tvmaster-cli -- serve
```

Then open http://localhost:8787/api/v1/health.

## License

TBD.
