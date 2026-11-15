# TVMaster — Project Plan

Self-hosted TV station and IPTV engine. Point it at your TV collections and it runs real-feeling channels: scheduled or chaotic, with commercials, bumpers, a guide channel, and a remote you can use from your phone. Headless or GUI, Windows/macOS/Linux, x64 and arm64.

## 1. Architecture

A **headless daemon does all the work**. Every GUI, the web app and the phone remote are clients of its API.

```
Media folders ─▶ Library scanner (ffprobe, NFO, TMDB opt.)
                      │
              Scheduler engine ──▶ Playout timeline (SQLite, 72h+ ahead)
                      │                      │
        Break/bumper planner         Playout runner (per channel)
                                       per-item ffmpeg → MPEG-TS → HLS segmenter
                                             │
   HTTP server: HLS · raw .ts · M3U · XMLTV · HDHomeRun emulation · REST+WS API · web UI
```

- **Core language:** Rust workspace (single binary per platform, easy x64/arm64 cross-builds).
- **Video:** FFmpeg does all media work. Bundled or auto-fetched (checksum-verified) per platform. Hardware encoders auto-detected (VideoToolbox, NVENC, QSV, AMF, VAAPI) with software fallback.
- **Time model:** The schedule is a pure function of (channel config, seed, wall-clock time). Tuning in lands mid-show, like real TV. Channels run always-on or start on first viewer.
- **Gapless playout:** each item is a segment (file, in-point, out-point) encoded to the channel's normalized profile, so retro 4:3 commercials and 1080p shows mix cleanly. A long-running segmenter stitches pieces with no glitches at ad breaks.

## 2. Streams, channels, clients, and TVs

- **One stream per channel.** Each channel is its own continuous stream (`/stream/{channel}/index.m3u8`). Many clients can watch the same channel (encoded once) and different clients can watch **different channels at the same time**. Multiple TVs in the house is a first-class case.
- **Two ways to watch:**
  1. **Any IPTV/HLS client** (VLC, Plex/Jellyfin/Emby via HDHomeRun emulation, Kodi, TiviMate): load the M3U + XMLTV and pick channels in the client's own UI.
  2. **TVMaster Virtual TVs** (the web player on a smart-TV browser, tablet, or a future Apple TV / Android TV app). Each registered TV has a stable ID (e.g. `living-room`) and a current channel that the **remote** changes.
- **Remote:** any phone/tablet/laptop opens the web app (installable PWA) and controls any registered TV: channel up/down, jump to number, see what's on now/next, power on/off the player, mute/volume where the player supports it. State syncs over WebSocket so every remote and TV stays in step.
- **Optional "TV endpoint" later:** a stable per-TV URL (`/tv/living-room.m3u8`) that follows that TV's selected channel, so dumb clients can be remote-controlled too.
- Remote access: LAN by default, optional token auth, optional reverse proxy or Tailscale guidance for away-from-home use.

## 3. Scheduling: complete chaos to real network

Each channel picks a strategy; strategies can be layered.

| Mode | Behavior |
|---|---|
| **Chaos** | Everything, weighted random, repeat cooldown. A chaos slider (0–100) blends structure into randomness. |
| **Shuffle / Sequential** | Per-show shuffle or in-order, with a persistent "where we left off" cursor. |
| **Block grid** | Time slots on a 30/60-minute grid. Each slot points at a show, smart collection, or pool. |
| **Dayparts** | Morning cartoons, afternoon, primetime, late night, each with its own rules. |
| **Seasons and rotation** | Date-ranged lineups (fall, midseason, summer reruns). Slots rotate or swap shows every N weeks. "New show intro" rules can premiere, spotlight, or marathon a show, then fold it into rotation. |
| **Events** | Marathons, theme nights, holiday specials. Events override normal programming. |
| **Time machine** | Locks a channel to an era so shows, ads and bumpers all match. |

Early-ending episodes are padded with ads, bumpers, shorts, or PSAs. Smart collections filter by genre, decade, rating, runtime and tags. A "mystery guide" option shows "???" for chaos channels.

## 4. Autopilot ("never dead air")

For days when nobody is touching it.

1. **Endless generation.** The scheduler tops up the timeline in the background so at least 72 hours are always materialized (vacation mode: 7–14 days). Bumpers, ad breaks, "Up Next" and "Later Today" are generated from the timeline, so they run on autopilot too.
2. **Run-out policy** per channel or slot:
   - **Keep going (default):** cursor continues in episode order and wraps to S01E01 after the finale, tracking pass count.
   - **Next in rotation:** hand the slot to the next show in the queue.
   - **Chaos fill:** random from a chosen pool or the whole library.
   - **Hybrid:** sequential where there's content, chaos for gaps and overflow.
3. **Cursors that never skip or repeat.** A *reserved* cursor (what's scheduled) and a *committed* cursor (what actually aired). Re-rolls and setting changes can't burn episodes. After downtime you choose: resume where it stopped, or advance as if it kept airing. New episodes slot in order via the folder watcher; missing files are skipped.
4. **Fallback ladder.** Missing/corrupt file → next item in the slot's pool → channel fallback pool → chaos from whole library → station ID/bumper loop → test pattern. A watchdog restarts a crashed FFmpeg within seconds and covers the gap. If the ad pool runs low, the no-repeat window relaxes so breaks are never skipped. Missing bumper data falls back to generic cards.
5. **Unattended ops.** Runs as a service (launchd/systemd/Windows Service), starts on boot, recovers state after power loss, retries offline libraries (unmounted NAS) while airing cached filler, rotates logs, watches disk space, can pin the FFmpeg version, exposes a health endpoint, and can alert via webhook/ntfy/email.
6. **Autopilot / Manual-lock toggle** per channel controls whether edits re-roll the timeline.

## 5. Commercials

- **Break points:** chapter markers first, then black-frame + silence detection (run once at scan time, cached), then an act-structure fallback (cold open, then roughly every 6–8 min, with jitter).
- **Pods:** configurable ad minutes per hour, pod length and count. Commercials are packed to fit exactly (era-matched, no repeats within N plays). Remaining time is padded with a "we'll be right back" card or station ID.
- Optional pre-transcode cache for commercials and bumpers.

## 6. Bumpers and graphics

One renderer (tiny-skia + text shaping) draws all generated graphics:

- **Types:** Up Next, Later Today, Coming Up Tonight, Now Playing, Back to [show], station ID, sign-on/sign-off, test pattern, off-air card.
- **Overlays:** channel bug and now-playing lower third.
- **Templates** are data-driven JSON/TOML plus assets and read the real schedule (`{next_title} {next_time}`).
- **Premade bumpers:** users can drop in their own clips (import and tagging tools provided). Copyrighted material from defunct channels is not bundled; built-in packs are original retro-style designs.

## 7. Channel 3 — the guide channel

A rendered video channel with a Prevue-style scrolling grid of the next ~3 hours for every channel, a clock, promo ticker, background music folder, and optional picture-in-guide video. It reads the same timeline as everything else, so it is always accurate. Multiple themes; layout and speed are programmable.

## 8. Output and compatibility

HLS, raw MPEG-TS, M3U, XMLTV, HDHomeRun emulation (Plex/Jellyfin/Emby see a tuner), built-in web player with channel surfing, optional RTMP/SRT/UDP multicast later.

## 9. Interfaces and design

- **Web app (React + TypeScript + PWA):** the complete admin UI and the phone remote. Served by the daemon, so it works headless from any browser. Used on Windows and Linux (wrapped in a Tauri shell for desktop feel) and on phones.
- **macOS native app (SwiftUI):** real **Liquid Glass** on macOS 26+, vibrancy fallback on older macOS. Talks to the same API, can start and supervise the bundled daemon. Built after the web UI so the API is stable.
- **Windows:** Mica/Acrylic. **Linux:** translucent surfaces with a CSS glass system.
- **Shared design tokens** (blur, specular edges, rounded geometry) so every platform reads as one family.
- **Main screens:** Libraries, Channels, drag-and-drop schedule grid, Commercials and Bumpers, Guide designer, Live monitor (what each channel and TV is airing now), TVs and Remote, Settings.

## 10. Cross-platform, headless, distribution

- Targets: Windows, macOS, Linux × x64, arm64.
- `tvmaster serve` for headless; `tvmaster` CLI for config import/export and admin.
- Service installers: systemd unit, launchd agent, Windows Service. Multi-arch Docker image.
- CI: GitHub Actions across all six OS/arch targets; signing and notarization on macOS.
- Packaging: DMG, MSI, AppImage/deb/rpm, Homebrew, winget.

## 11. Repo layout

```
crates/
  tvmaster-core/      models, scheduler, autopilot, cursors (pure logic, deterministic)
  tvmaster-db/        SQLite schema + migrations
  tvmaster-media/     ffprobe/ffmpeg manager, scanner, break detection
  tvmaster-playout/   per-channel runner, watchdog, HLS segmenter
  tvmaster-render/    bumper/guide graphics renderer
  tvmaster-server/    axum HTTP + WebSocket API, M3U/XMLTV/HDHomeRun
  tvmaster-cli/       `tvmaster` binary (serve, scan, etc.)
web/                  React web app + PWA remote (later milestones)
apps/macos/           native SwiftUI app (later milestone)
docs/                 plan, API, design
```

## 12. Milestones

Status: **M1 done. M2 done** (see notes below). M3 next.

1. **Foundation:** repo, workspace, CI matrix, FFmpeg manager, library scanner, SQLite schema, API skeleton.
2. **One channel on air:** playout runner, HLS/M3U/XMLTV, shuffle + sequential, web player, watchdog, service installers, state recovery.
3. **Real scheduling and Autopilot:** block grid, dayparts, rotation, seasons, events, chaos slider, run-out policies, reserved/committed cursors, rolling top-up.
4. **Commercials and bumpers:** break detection, pod packing, renderer, template packs, overlays.
5. **Multi-channel, Channel 3, TVs/Remote:** guide renderer, channel groups, HDHomeRun emulation, Virtual TV registry, remote WebSocket API.
6. **Web UI:** full admin, PWA remote, Tauri desktop shell with Mica/Linux glass.
7. **macOS native app:** SwiftUI + Liquid Glass.
8. **Hardening and release:** hardware-encode matrix, week-long fake-clock soak, fault injection (delete files, kill FFmpeg, unmount libraries), installers, docs.

## 13. Testing

Golden-file schedule tests (same seed → same schedule), fake-FFmpeg harness for CI, fake-clock simulator (days of channel time in seconds), long-run playout soak tests, fault-injection suite.

## 14. Risks

- A webview cannot do true Liquid Glass lensing; that is why macOS gets a native SwiftUI app and Windows/Linux get the best close approximation.
- Bundled FFmpeg licensing: ship an LGPL build, or invoke GPL builds (libx264) only as an external subprocess.
- Copyrighted bumpers/commercials are BYO; the repo ships only original assets.

## 15. M2 notes (what was built and learned)

- **Pipeline:** each item is encoded by its own FFmpeg process to MPEG-TS, pushed through a bounded buffer into one long-running HLS segmenter. The segmenter reads at 1x with an initial burst, so tune-in is about 0.7 s and the next item is always already encoded when the current one ends.
- **Gotcha fixed:** concatenating separately encoded transport streams resets the per-PID continuity counters at every item boundary, which makes demuxers drop packets of the first keyframe. The playout crate rewrites the counters so the joined stream is one continuous TS (`crates/tvmaster-playout/src/ts.rs`). The end-to-end test confirms no gap longer than one frame across items with different sizes, frame rates and a clip with no audio.
- **Timeline:** M2 uses an endless deterministic timeline anchored at the channel's epoch (shuffle re-seeded per pass). This already gives "keeps going and wraps" behaviour; the reserved/committed cursors, run-out policies and block grids replace it in M3.
- **Autopilot basics in place:** watchdog with exponential backoff, restart on segmenter crash, unplayable files skipped, idle channels stop, config and TV state persist, service templates. Still to come: cursors, fallback ladder beyond skip-to-next, health alerts.
- **Known limits:** scans are synchronous (large libraries should become a background job), drift between wall clock and encode is not corrected yet (M3 resync), only software/VideoToolbox/NVENC encoders are wired (NVENC untested), no auth.
