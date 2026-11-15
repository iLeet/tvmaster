# Installing, operating and releasing TVMaster

## Install

| You have | Do this |
|---|---|
| macOS | Unzip `TVMaster-<version>-macos.zip`, drag `TVMaster.app` to Applications. It carries its own daemon. The first launch needs right-click, Open: the app is ad-hoc signed, not notarized. |
| macOS, Linux (headless) | Unpack `tvmaster-<version>-<target>.tar.gz`, run `./install.sh` (or `./install.sh --service` to run at login/boot). |
| Windows | Unpack the `.zip`, run `.\install.ps1` (add `-Service` from an elevated PowerShell to start at boot). |
| Docker | `docker compose up -d` after editing the token and media path in `docker-compose.yml`. |
| Rust | `cargo run -p tvmaster-cli -- serve` |

All of them need FFmpeg and ffprobe, except the Docker image, which has them. The installers warn when they are missing.

`--service` generates an access token into `<data dir>/token` (readable by you only) and passes it to the service with `--token-file`, so the token is never on a command line. The admin page asks for it once.

## Check the machine: `tvmaster doctor`

```
FFmpeg 9.0.2 (ffmpeg)
  ffprobe works      yes
  ...
Encoders (each one encodes a 3 second 720p clip):
  software      works, 29.9x real time
  videotoolbox  works, 10.0x real time
  nvenc         not in this FFmpeg build
  ...
```

"In this FFmpeg build" only means FFmpeg was compiled with the encoder. Doctor actually encodes with each one, so a laptop with no NVIDIA card does not look like it can use NVENC. Anything under about 2x real time will struggle with a channel that has several viewers on different items.

At start-up, `serve` runs the same test on the encoder it is about to use. If a hardware encoder fails, it logs why and falls back to software rather than putting every channel on a slate.

### Encoder matrix

| Encoder | Platform | Tested on real hardware |
|---|---|---|
| `software` (libx264) | everywhere | yes (macOS arm64; Linux, Windows and Intel Mac in CI once it runs) |
| `videotoolbox` | macOS | yes (Apple silicon) |
| `nvenc` | Windows, Linux with an NVIDIA GPU | **no** |
| `qsv` | Intel GPU, Windows and Linux | **no** |
| `vaapi` | Linux, Intel/AMD GPU (`/dev/dri/renderD128`, override with `TVMASTER_VAAPI_DEVICE`) | **no** |
| `amf` | Windows, AMD GPU | **no** |

The untested ones have unit tests for the FFmpeg arguments they generate (pixel format, `hwupload`, device selection) and the start-up fallback, but nobody has run them against a GPU. If you have one, `tvmaster doctor` output in an issue is the most useful thing you can send.

## What happens when things break

These are exercised by tests against real FFmpeg (`crates/tvmaster-playout/tests/faults_e2e.rs`) or by the fake-clock soak (`crates/tvmaster-server/src/soak.rs`).

| Fault | What viewers see | What the server does |
|---|---|---|
| FFmpeg segmenter killed | A few seconds of nothing, then the channel is back and live | Watchdog restarts the channel with backoff |
| An item's FFmpeg killed (out of memory) | The rest of that item is skipped | The file is **not** blamed: a killed encoder says nothing about the file |
| File deleted, or corrupt | The station slate for that item's slot | Reported, flagged missing, schedule re-rolled without it |
| Drive or NAS unplugged for a while | The slate while it is gone; the channel never restarts | Each file that cannot be opened is flagged missing. Every two minutes the server looks for flagged files again: when they are back the flag is cleared and the channels re-roll, so shows return without a rescan |
| Library folder not reachable during a scan | Nothing | The scan refuses to run, existing items are kept |
| Machine off for hours or days | Nothing | On return, "resume" continues from where it left off; "advance" acts as if it had kept airing |
| Empty schedule | The slate | Autopilot fills it as soon as there is content |

Playout never encodes further than 45 s ahead of the clock. Without that, a slate (tiny to encode) raced through minutes of schedule while a drive was unplugged and was still airing after the files came back.

### Soak test

`cargo test -p tvmaster-server soak` runs 21 days of channel time in under a minute on a fake clock, on four channels (a grid with ads and dayparts, plain sequential, pure chaos, shuffle with "advance" catch-up), with a timetable of trouble: the machine off for 6 hours and then most of a day, three files deleted, a new season scanned in, a drive unplugged and plugged back in, a lineup edited. After every pass it checks that each schedule is gapless, covers now, is filled 72 hours ahead, contains nothing whose file is gone (beyond the show already on air), and that no episode cursor moves backwards. A second test runs the same inputs twice and requires identical schedules.

What it does not cover: real wall-clock drift over a week (nothing ran for a real week), and memory growth of a long-running daemon.

## Releasing

1. Bump `version` in `Cargo.toml` and `apps/macos/VERSION`, commit.
2. `git tag v0.1.0 && git push --tags`.
3. `.github/workflows/release.yml` builds the daemon for six targets (Linux, macOS, Windows, each x64 and arm64), a universal `TVMaster.app`, builds and smoke-tests the Docker image, and publishes everything with `SHA256SUMS` as a pre-release.

`scripts/package.sh [target] [out-dir]` does the per-target packaging, so you can build an archive locally.

**Not done:** Developer ID signing and notarization for macOS, Authenticode signing for Windows, `.deb`/`.rpm`/MSI packages, publishing the Docker image to a registry, and Tauri desktop-shell installers. The release workflow has never run (see below).

## CI status

When this milestone was written the GitHub Actions workflows had never run: every job was rejected with "your account is locked due to a billing issue", an account-level lock on the GitHub side that making the repository public did not clear. Until GitHub accepts runs, everything here was verified locally on one machine (macOS 27, Apple silicon): `cargo test --workspace`, clippy, `swift test`, `scripts/package.sh`, and `install.sh` into a temporary prefix. Windows, Linux and Intel-Mac results, the Windows installer, the Docker image and both workflows are untested.
