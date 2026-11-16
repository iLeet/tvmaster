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

Status: **M1-M12 done** (see notes below). Still to come: Apple notarization and a trusted Windows certificate (need your accounts), native Windows and Linux shells, HTTPS.

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

## 16. M3 notes (scheduling and Autopilot)

- **Design:** the scheduler (`tvmaster-core`) is pure and deterministic. It resolves *events > season blocks > base blocks > fill* at every slot start, snaps starts to a local-time grid, pads with filler, and draws from per-show/pool/event cursors. The server materializes its output into SQLite 72 hours ahead; playout, the guide, XMLTV and now/next all read that one schedule. Full reference: [SCHEDULING.md](SCHEDULING.md).
- **Cursors:** reserved vs committed, as planned. Re-rolls rebuild reservations from what aired plus what the locked items hold, cutting at a show boundary so the grid stays aligned. Covered by a two-week fake-clock simulation and re-roll tests.
- **Downtime:** a heartbeat distinguishes "the machine was off" from "time passed normally", so items scheduled during an outage never count as aired (`resume`), unless you chose `advance`.
- **Playout:** follows the schedule's wall-clock times, covers gaps, scheduled filler and unplayable files with a slate, reports bad files (flagged missing, re-rolled within seconds), and resyncs to live if it falls behind. The slate has no text on FFmpeg builds without `drawtext` (the Homebrew build has none); proper station graphics arrive with the renderer in M4.
- **Mistakes the tests caught:** a re-roll that split a show from its padding and knocked the grid off alignment; stale schedules being pruned before downtime could be detected; and wall-clock-dependent assertions.
- **Known limits:** no mid-show ad breaks or bumpers yet (M4); scans still run in the foreground; no auth; the unplayable-file ladder covers missing/corrupt files but not slow storage.

## 17. M4 notes (commercials and bumpers)

- **Ad budget = slot minus show.** On a grid, the spare minutes of each slot become the ad budget; without a grid, `minutes_per_hour` extends the slot. The planner (`tvmaster-core/src/ads.rs`) splits the show at break points and fills every pod to the exact millisecond: opening bumper, commercials packed by a knapsack, station ID for odd seconds, closing bumper. Slots therefore always end on the grid. Reference: [ADS.md](ADS.md).
- **Break points:** chapters (free at scan time), then a background black-frame scan (`blackdetect` at 160px, two files at a time), then a synthetic act structure. Cached in `media_items.breaks_json`, reset if the file's length changes.
- **Schedule items grew up:** shows are stored in acts (`in_ms`), with commercials, bumpers and filler between. Only the first act carries the episode cursor, so cursor logic and re-rolls are unchanged. Re-rolls cut at the first act of a show.
- **Late-bound bumper text.** A generated bumper is stored as just a kind ("up_next"); its words are resolved when it airs from the live schedule, so "Up next" stays right after re-rolls and top-ups, and nothing has to know the next slot at generation time.
- **Renderer:** `tvmaster-render` draws 640x360 RGB frames with an embedded 8x8 pixel font (`font8x8`, MIT/Apache) and original stripes/scanline art, piped into FFmpeg and scaled up without smoothing. It needs no `drawtext` filter and no font files, so it works with any FFmpeg build and looks the same everywhere. `cargo run -p tvmaster-render --example preview -- <dir>` writes sample frames.
- **Playout:** acts start part-way into a file, rendered bumpers share the same gapless pipeline (a mixed item stream shows no gap longer than one frame in the end-to-end test).
- **Known limits:** commercials are not loudness-normalised; short test clips (under about 5 minutes) get fewer breaks because of the 90 s margins; no channel bug or lower third yet; bumpers are silent.

## 18. M5 notes (guide channel, tuners, live remote)

- **Guide channel:** a channel of `kind: guide` is rendered live by `tvmaster-render` (`render_guide`): a Prevue-style grid with a "now" marker, a clock, scrolling pages when there are more than six channels, and a "now on" ticker. One long-running encoder is fed raw frames at 15 fps (FFmpeg duplicates up to 30), plus looping music. The listing is refreshed from the schedule every 5 seconds of stream time, and the on-screen clock follows the stream, not the encoder, so it stays right despite encode-ahead buffering. Music files are converted once to a common format in `cache/music` so different formats can share a playlist.
- **Tuner compatibility:** `/discover.json`, `/lineup.json`, `/lineup_status.json`, `/device.xml` imitate the HDHomeRun HTTP API, and `/stream/{id}.ts` serves each channel as continuous MPEG-TS by following the HLS segments. Built to the API those servers use; **not yet verified against a real Plex, Jellyfin or Emby** (none available here).
- **Live remote:** an event bus feeds WebSocket clients (`tv.state`, `channel.now`, `schedule.changed`). Players say `hello` over the socket so TVs show `online`; commands over the socket and REST share the same code. The web player and remote use it, with REST polling as a fallback. Verified in a real browser: a remote click changed the TV page's channel, volume, mute and power immediately.
- **A deadlock the live run found:** starting a guide channel built its spec (a database read) while the request already held the database lock, hanging the first request for the guide. Unit tests could not see it (no FFmpeg in the server tests). There is now a regression test; the fix is to build the spec after releasing the lock.
- **Other fixes:** channel updates are now partial (omitting `library_ids` or `chaos` used to reset them); music libraries scan audio files; very narrow guide cells draw as blocks without unreadable text.
- **Known limits:** no auth on the WebSocket or tuner endpoints; the guide is a fixed look (themes per channel colour only); `/tv/{id}.m3u8` does not follow later channel changes (players that need that should use the web player); channel groups are labels only (no per-group scheduling).

## 19. M6 notes (web admin UI, PWA remote, desktop shell, auth)

- **The UI** lives in `web/` (React 18 + TypeScript + Vite, no UI libraries; about 65 KB gzipped). `npm run build && npm run sync` compiles it into `crates/tvmaster-server/ui/`, which is **committed** and embedded in the binary (`rust-embed`), so `cargo build` never needs Node and a headless server serves the same admin UI at `/admin/`. Debug builds read the folder at run time, so a rebuilt UI shows up without recompiling Rust. CI builds the UI and fails if the committed copy is stale.
- **Pages:** dashboard (live over the WebSocket), libraries (folder picker, scan, break-analysis progress), channels (general, a visual lineup editor with blocks/seasons/events/rotations plus a raw-JSON escape hatch, ads and bumpers, guide channel), schedule (guide and breaks/bumpers timeline, re-roll, per-show progress), TVs (power, channel, volume, online), phone remote, settings (URLs to copy, tuner address).
- **Design:** one glass design system with platform flavour: macOS rounder and more translucent, Windows crisper and more opaque (Mica-like), Linux in between; light and dark follow the system.
- **PWA:** web app manifest, icons (drawn by the Rust renderer), service worker that caches the shell (never the API or streams), and `#/remote` as the start URL. The service worker could not be exercised in the in-app browser used for testing (it refuses to register one); the file is served with the right headers and scope.
- **Desktop shell:** `apps/desktop` is a Tauri 2 app (outside the main workspace) with a connect screen, an optional bundled daemon, and native glass (vibrancy on macOS, Mica on Windows 11) behind a transparent web UI. It compiles and starts on macOS; Windows and Linux builds, installers and signing are not done.
- **Authentication:** optional token (`--token`, `tvmaster token`); header, `?token=` for IPTV apps (playlists and HLS segments carry it), or an HTTP-only cookie for browsers. Constant-time comparison, short delay on a wrong login. This is not a user-account system and offers no TLS: put it behind HTTPS before exposing it beyond your LAN.
- **Bugs the testing found:** deleting a library failed when a schedule still referenced its files (now cleared first, and channels rebuild); a database lock held across an `if let` body deadlocked a handler (found by a hanging test); oversized SVG icons outside buttons.
- **Known limits:** the folder picker lists any folder the server user can read, so use a token on shared networks; the schedule view is a list and timeline (the lineup itself is edited on the M9 weekly grid); no mobile-specific editing polish beyond the responsive layout; the UI has unit tests for its pure helpers but no browser automation suite.

## 20. M7 notes (native macOS app)

- **Where:** `apps/macos`, a SwiftPM package (swift-tools 6.2, macOS 15+, outside the Cargo workspace). `TVMasterKit` is a UI-free library (models, REST client, WebSocket event stream, Keychain token store, daemon locator and supervisor, formatting) with 22 Swift Testing tests; `TVMaster` is the SwiftUI app. `Scripts/build-app.sh` builds `TVMaster.app`, bundles the release `tvmaster` daemon, makes the icon and Info.plist, and ad-hoc signs it.
- **Same API as the web UI:** nothing server-side changed. The app speaks REST plus the `/ws` event bus, so it is live like the web dashboard and works against a remote server as well as a local one. The token is kept in the Keychain per server.
- **Daemon:** if the address is local and nothing answers, the app starts the bundled daemon (or one found via `TVMASTER_BIN`, Homebrew, cargo or PATH), restarts it with backoff if it dies, and stops it on quit. The log tail is in Settings.
- **Screens:** Home, Watch (native `AVPlayerView` on the channel's HLS stream, Picture in Picture, channel list and overlay), Channels (list and general editor, re-roll, delete), Schedule (guide and breaks), Libraries (folder picker when local, scan, ad-break analysis), TVs (power, channel, volume, mute, step). Menu bar remote, and the Mac can register itself as a TV so the phone remote can change what it plays.
- **Liquid Glass:** `glassEffect` and `GlassEffectContainer` plus the `.glass` button styles on macOS 26+, wrapped in `#available` helpers that fall back to system materials on macOS 15.
- **Known limits:** the lineup, ads and guide designers still open the web admin; no notarization or Developer ID signing; checked visually only on macOS 27 (the macOS 15 fallback and Intel builds are untested); no CI job for the Swift package yet.

## 21. M8 notes (hardening and release)

- **Findings from fault injection, all fixed:** (1) an FFmpeg that was *killed* flagged a perfectly good file as unplayable (now only a real failure does); (2) five unplayable items in a row restarted the channel, so an unplugged drive took the stream off the air (missing files no longer count, the slate covers each one); (3) a slate encodes so cheaply that playout raced minutes ahead through a dead drive and kept airing slate after the files returned (encode lead is capped at 45 s); (4) files flagged missing stayed missing until someone rescanned (the server now re-checks them every two minutes and re-rolls the channels when they return).
- **Soak:** `crates/tvmaster-server/src/soak.rs`, 21 fake-clock days over four channels plus a second run for determinism, about 45 s in a debug build. It found one wrong assumption in its own checks (the show already on air legitimately keeps its later acts after its file vanishes) and nothing in the scheduler.
- **Faults against real FFmpeg:** `crates/tvmaster-playout/tests/faults_e2e.rs`: segmenter killed, item encoder killed, file deleted, corrupt file, drive unplugged and back (checks the playlist never disappears), empty schedule. About 80 s.
- **Encoders:** QSV, VAAPI and AMF added next to VideoToolbox and NVENC. `tvmaster doctor` and a start-up probe really encode a clip with each, and a failing hardware encoder falls back to software. Only software and VideoToolbox have been run on real hardware.
- **Release:** `scripts/package.sh`, `install.sh`, `install.ps1`, `Dockerfile`, `docker-compose.yml`, `.github/workflows/release.yml`. `serve --token-file` keeps the token out of command lines and service definitions. See [RELEASE.md](RELEASE.md).
- **Known limits:** no real week-long run or memory-growth measurement; the release workflow, Windows installer and Docker image have never been run; no signing or notarization; the GitHub Actions account lock meant CI never ran, so Windows, Linux and Intel results are unknown.

## 22. M9 notes (weekly grid editor)

- **Where:** `web/src/pages/WeekGrid.tsx` (the component), `web/src/weekgrid.ts` (pure geometry, 23 unit tests in `weekgrid.test.ts`). It replaces nothing: the block list stays as a "List" view, and the choice is remembered in the browser. It edits `lineup.blocks` and each season's blocks, so the daemon and the API are unchanged.
- **How it maps to the model:** a block with several days is drawn once per day; an overnight block (end before start) continues into the next day's column, and its days still mean the day it starts. Dragging a block moves its time for every day; dragging it sideways moves just that occurrence to another day; "Only Tue" splits one day off into its own block. Drag on empty space to add a block (a plain click adds an hour), drag the top or bottom edge to resize, Undo (50 steps), and keyboard: arrows move, Shift+up/down resizes, Delete removes. Everything snaps to the channel's time grid (30 minutes when automatic).
- **Overlaps:** the scheduler takes the first matching block in list order, so where two blocks overlap the earlier one wins. The grid hatches the part of the later block that can never air and says why.
- **Verified:** unit tests for the geometry, and in a real browser against a real daemon: drag-create, drag-move across days and hours, drag-resize (including the tail of an overnight block), click-create, "Only day", undo, delete (Duplicate and the List/Grid toggle are covered by unit tests or by reading the code only), overlap hatching, save and reload round trip, and the phone layout (the grid scrolls sideways inside its card). Pointer drags in the browser were driven with synthetic pointer events computed from the DOM, because the pane's own drag tool was imprecise when the window was resized mid-test.
- **Known limits:** touch drags on a phone were not tried on a real touch screen (empty-space taps create a block, blocks are `touch-action: none`, columns pan); no time-zone-aware "now" marker; two overlapping blocks are stacked rather than shown side by side; no multi-select or copy-paste of blocks; the native macOS app still opens the web admin for lineups.

## 23. M10 notes (installers)

- **Formats:** macOS `.dmg`; Windows NSIS setup `.exe` (x64, arm64); Linux `.deb`, `.rpm` and Arch `.pkg.tar.zst` from one `nfpm.yaml`; the `tar.gz`/`zip` with `install.sh`/`install.ps1` from M8 remain for no-root installs. See [RELEASE.md](RELEASE.md) for what each one sets up and for the signing options.
- **Design choices:** every installer ends with a running, auto-restarting server behind an access token that is never on a command line (`--token-file`). Linux packages run a hardened system service as a `tvmaster` user; the Windows installer registers a SYSTEM startup task (a plain `.exe` cannot be a Windows service without a wrapper) and opens the firewall for private networks only; the Mac app supervises its own server. FFmpeg: a dependency on Linux packages, downloaded by the Windows installer, and an **Install FFmpeg** button (Homebrew) in the Mac app. The server also prefers an FFmpeg found beside `tvmaster` (`ffmpeg/bin`) over `PATH`.
- **Real-machine testing (a Linux box with an RTX 3080 Ti, reached over SSH):** the full suite passes on Linux for the first time; NVENC works at 5.3x real time; `tvmaster doctor` now reports the cause of a failure (`Cannot load libcuda.so.1`) rather than FFmpeg's last line; `install.sh --service` ran under real systemd and survived `kill -9`. Packages were installed in Debian, Fedora and Arch containers.
- **Self-signing:** answered in RELEASE.md with evidence: a self-signed Mac app is rejected by Gatekeeper exactly like an ad-hoc one; self-signed Windows installers work for PCs that import your certificate.
- **Known limits:** the Windows installer and its PowerShell have never run on Windows; the release workflow and Docker image have never run; the packaged system unit was verified for syntax only; no apt/dnf repository, MSI or Docker registry image; the Mac FFmpeg button needs Homebrew.

## 24. M11 notes (HTTPS and sharing)

- **Server:** `crates/tvmaster-server/src/tls.rs` (rustls via axum-server, rcgen for the self-signed certificate; no OpenSSL or C toolchain needed beyond `ring`). Two listeners share one router; an outermost layer stamps which scheme a request used (and deletes any client-supplied stamp), so links and cookies are right. `/api/v1/system` advertises the ports, hosts and certificate fingerprint. Flags: `--tls`, `--tls-bind`, `--tls-cert`, `--tls-key`, `--no-http`; the same as `TVMASTER_*` environment variables, wired into the Linux service file, `install.sh --tls`, `install.ps1 -Https` and a Windows setup option.
- **Bugs the testing found:** over HTTP/2 (every browser on HTTPS) the host arrives as the URI authority and there is no `Host` header, so M3U and HDHomeRun links named `localhost:8787` (fixed by filling `Host` in); a reformatting by `cargo fmt` had silently stopped a patch from applying, caught only because a test asserted the result; the Mac app, when it started a server with a freshly made token, asked the user for that token (the client was built before the token existed).
- **Mac app:** HTTPS with trust on first use: untrusted certificate, fingerprint shown, the user's choice pinned per server and checked on every connection (REST, WebSocket); a loopback relay lets AVPlayer play pinned servers. **Share** screen: IPTV playlist and guide, tuner address, phone remote and per-TV remote with QR codes (verified by scanning them back with a QR detector), every channel's HLS and MPEG-TS URL, copy and the system share sheet, address and HTTP/HTTPS pickers and an optional token. Settings can let the bundled server listen on the network (it makes a token) and serve HTTPS; the menu bar copies the playlist link.
- **Web admin:** Settings now has the same Share card, the certificate fingerprint, and says when HTTPS is off.
- **Testing:** `scripts/system-test.sh` is a 44-check end-to-end test of a real daemon and also runs in CI on Linux and macOS.
- **Known limits:** the Windows setup option, Windows/Linux packaging of HTTPS and the real-certificate reload were not run; IPv6 is left out of the address lists and certificate; HTTP is still on by default (HTTPS is opt-in); the relay buffers whole responses (fine for HLS, not for an endless TS stream); FFmpeg's HLS client cannot use a self-signed certificate.

## 25. M12 notes (audio tracks, subtitles, TV-style channel options)

- **Reading tracks:** `tvmaster-media` parses every audio and subtitle stream from ffprobe (language, codec, channels, default/forced/hearing-impaired/commentary flags, titles, interlacing) and finds subtitle files beside the video; the scan stores them as `tracks_json` (migrated column). Languages are normalised to three letters (`en`, `English`, `fre`/`fra` all agree).
- **Choosing:** `tvmaster-core::tracks` (pure): first preferred language wins, commentary/description skipped unless alone, default then more channels; five subtitle modes including "foreign" (full subtitles only when the audio is not in a language you chose); text subtitles are only offered when FFmpeg can draw them.
- **Options:** `tvmaster-core::options::ChannelOptions` (audio, subtitles, video, overlays, per-show overrides) stored as `options_json`, validated, and applied by playout per item: `-map` by stream index, libass subtitles (cut out of the video once and cached under a safe file name, so no quoting hazards), picture subtitles overlaid before scaling, loudness and night-mode filters, aspect/deinterlace/look chains, logo and title card as PNGs written by a tiny PNG encoder in the render crate and overlaid, per-channel output profile (the whole channel switches size, not just items). Sign-off is a scheduler feature: filler items labelled "Sign-off" that play as bars and a tone.
- **Hardening:** if an item fails to encode with its extras (subtitle filter, overlays, clock), it is retried without them before the file can be reported unplayable. Verified on a Mac whose FFmpeg has no libass: the programme airs, the file is not blamed.
- **UI:** web channel editor tab **Picture & sound** (language lists, every option, sign-off, per-show overrides with the languages found in each show's files and a re-scan hint); the Mac channel editor has the main settings, reading and writing the server's raw JSON so unseen settings (per-show overrides) survive an edit.
- **Verified:** see the end of [CHANNEL-OPTIONS.md](CHANNEL-OPTIONS.md): measurements on real streams from real multi-track files (pitch of the chosen audio, burned subtitle brightness, logo/card timing, no colour in black and white, 480p/24fps, sign-off bars and tone).
- **Known limits:** picture (bitmap) subtitles could not be generated here, so only their FFmpeg arguments are tested; the clock needs FFmpeg's drawtext (absent from Homebrew's build); text-subtitle encodings other than UTF-8 are not converted; per-show settings are edited in the web admin only; no per-file track override; a channel restart is needed for option changes (it happens automatically on save).
