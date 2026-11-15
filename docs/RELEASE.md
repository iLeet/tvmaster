# Installing, operating and releasing TVMaster

## Install

Each platform has a real installer that sets everything up: the server, starting at boot, an access token, FFmpeg, and (where there is a firewall) the firewall rule.

| You have | Get | What it does |
|---|---|---|
| macOS | `TVMaster-<version>.dmg`: drag TVMaster to Applications | The app carries its own server and starts it. If FFmpeg is missing, the Home screen has an **Install FFmpeg** button (uses Homebrew). |
| Windows 10/11 (x64, arm64) | `TVMaster-<version>-<arch>-setup.exe` | A wizard (or `/S` for silent): installs to Program Files, makes an access token only Administrators can read, registers a startup task that restarts TVMaster if it stops, opens the firewall for **private** networks only, downloads FFmpeg, adds Start menu shortcuts. Uninstaller asks whether to keep your data. |
| Debian, Ubuntu | `tvmaster_<version>_<arch>.deb` (`sudo apt install ./tvmaster_*.deb`) | Pulls in FFmpeg, creates a `tvmaster` user, makes `/etc/tvmaster/token`, installs and starts a hardened systemd service. |
| Fedora, RHEL | `tvmaster-<version>-1.<arch>.rpm` (`sudo dnf install ./tvmaster-*.rpm`) | Same. Fedora's `ffmpeg-free` has no x264; install RPM Fusion's `ffmpeg` for software encoding, or use a hardware encoder. |
| Arch, CachyOS, Manjaro | `tvmaster-<version>-1-<arch>.pkg.tar.zst` (`sudo pacman -U ...`) | Same. |
| Any Linux or macOS, no root | `tvmaster-<version>-<target>.tar.gz`, then `./install.sh --service` | Installs under `~/.local` and runs as a systemd user service or a launchd agent. |
| Windows, no wizard | the `.zip`, then `.\install.ps1` in an elevated PowerShell | Same steps as the wizard. |
| Docker | `docker compose up -d` after editing the token and media path in `docker-compose.yml` | FFmpeg is in the image. |
| Rust | `cargo run -p tvmaster-cli -- serve` | Nothing is set up for you. |

On Linux the service reads settings from `/etc/tvmaster/tvmaster.env` (port, encoder). Packages never delete your data on removal (`/var/lib/tvmaster`, `/etc/tvmaster/token`), so a reinstall picks up where it stopped.

For GPU encoding on Linux, the installer adds the `tvmaster` user to the `video` and `render` groups. Run `tvmaster doctor` to see what works, then set `TVMASTER_ENCODER`.

### Signing: what is and is not possible

You can sign anything yourself on any platform. The difference is whether *other people's* computers trust your signature.

| | Signed by you (self-signed) | Signed by a trusted authority |
|---|---|---|
| **macOS** | `Scripts/make-selfsigned-cert.sh` creates a certificate; `SIGN_IDENTITY="TVMaster Self-Signed" Scripts/build-app.sh` signs with it. That gives your Mac a stable identity (permissions survive rebuilds). **Gatekeeper still rejects it** (checked: `spctl` says `rejected`, `origin=TVMaster Self-Signed`), so anyone you share it with must right-click > Open once. | A paid Apple Developer ID ($99/year) plus notarization (`NOTARIZE=1`). Only then does a downloaded app open with no warning. Apple does not allow any other route for apps downloaded from the internet. |
| **Windows** | `packaging/windows/sign.sh <installer> --selfsigned` signs with a certificate it generates (checked with `osslsigncode`). Windows shows "Unknown publisher" and SmartScreen warns unless the `.cer` is first imported into the Trusted Root and Trusted Publishers stores on that PC. Fine for your own PCs. | A code-signing certificate (or Azure Trusted Signing). SmartScreen also wants reputation, which builds up over downloads. |
| **Linux** | Nothing to sign: a `.deb`, `.rpm`, package or binary runs unsigned. Package repositories use GPG signatures, which matter if you host an apt/dnf repository. | |

The release workflow signs and notarizes automatically when the repository has the matching secrets (`MACOS_CERT_P12_BASE64`, `MACOS_CERT_PASSWORD`, `MACOS_SIGN_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`, `WINDOWS_CERT_PFX_BASE64`, `WINDOWS_CERT_PASSWORD`) and otherwise ships ad-hoc and unsigned builds. That signing path has not run.

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
| `software` (libx264) | everywhere | yes (macOS arm64; Debian 12 and Arch containers on x86_64) |
| `videotoolbox` | macOS | yes (Apple silicon) |
| `nvenc` | Windows, Linux with an NVIDIA GPU | **Linux only**: RTX 3080 Ti on CachyOS, 5.3x real time. Not on Windows. |
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
3. `.github/workflows/release.yml` builds the daemon for six targets (Linux on Ubuntu 22.04 so the binaries run on older distributions; macOS; Windows; each x64 and arm64), the `.deb`/`.rpm`/Arch packages, the Windows setup `.exe` for x64 and arm64, a universal `TVMaster.app` in a `.dmg`, builds and smoke-tests the Docker image, and publishes everything with checksums as a pre-release.

Locally: `scripts/package.sh [target] [out-dir]` (archive), `packaging/linux/build.sh <binary> <amd64|arm64>` (Linux packages, needs Docker), `apps/macos/Scripts/build-app.sh` then `build-dmg.sh`, and `makensis -DVERSION=... -DARCH=x64 -DEXE=tvmaster.exe packaging/windows/tvmaster.nsi` (Windows installer; the Homebrew `makensis` 3.13 crashes on Apple silicon, use Linux).

## What was tested, and where

| | Result |
|---|---|
| Whole test suite, clippy, fmt | passes on macOS (Apple silicon) and on **Linux x86_64** (Debian 12 container with real FFmpeg, including the soak, end-to-end and fault tests) |
| Windows | the workspace and all its tests **compile** for `x86_64-pc-windows-gnu` (cross-compiled on a Mac); nothing has been **run** on Windows |
| `.deb` | installed in a Debian 12 container: user, token (root:tvmaster 640), service command line with token auth (401 without, 200 with), `doctor`, reinstall keeps the token, removal keeps data |
| `.rpm` | installed in a Fedora container: same checks except running the server (the minimal image has no `su`) |
| Arch package | installed in an Arch container: same as the deb, including running the server |
| Real systemd | `install.sh --service` on CachyOS: the user service started, enforced the token, and came back with a new PID after `kill -9`. The packaged **system** unit was checked with `systemd-analyze verify` (syntax only) because installing it needs root |
| NVENC | `tvmaster doctor` on an RTX 3080 Ti: works, 5.3x real time |
| DMG | built, mounted, the app inside verifies; Gatekeeper rejection of ad-hoc and self-signed builds observed |
| Windows setup `.exe` | **compiled** (NSIS 3.08 on Linux, no warnings); **never run**. The PowerShell it calls has not been run either |
| Self-signed Authenticode | signed and verified with `osslsigncode` |
| Release workflow, Docker image | never run |

**Not done:** Developer ID and notarization (needs your Apple account), a trusted Windows certificate, an MSI, publishing the Docker image or apt/dnf repositories, and Tauri desktop-shell installers.

## CI status

When this was written the GitHub Actions workflows had never run: every job was rejected with "your account is locked due to a billing issue", an account-level lock on the GitHub side that making the repository public did not clear (the billing page itself showed no problem). Until GitHub accepts runs, the table above is the evidence.
