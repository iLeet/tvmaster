# TVMaster for macOS

A native SwiftUI app for the TVMaster daemon, with real Liquid Glass on macOS 26 and later (system materials on macOS 15).

It talks to the same REST and WebSocket API as the web admin, so it can drive a local daemon or one running elsewhere on your network.

## Build

You need Xcode (or the Swift toolchain) with Swift 6.2 or later, and a built daemon if you want it bundled.

```bash
cargo build --release          # from the repo root, builds target/release/tvmaster
cd apps/macos
swift test                     # TVMasterKit tests
Scripts/build-app.sh           # builds build/TVMaster.app with the daemon inside
Scripts/build-dmg.sh           # wraps it in build/TVMaster-<version>.dmg (drag to Applications)
open build/TVMaster.app
```

For development, `swift run TVMaster` works too. Set `TVMASTER_BIN` to use a specific daemon binary and `TVMASTER_DATA_DIR` for its data folder.

## What it does

- **Home, Watch, Channels, Schedule, Libraries, TVs** in a sidebar, live over the WebSocket.
- **Watch** plays the channel's HLS stream in a native player with Picture in Picture.
- **Starts the daemon for you** when the server address is local and nothing is listening, restarts it if it exits, and stops it on quit.
- **Menu bar remote** and an option to make this Mac a TV the phone remote can control.
- Server address and token are in Settings; the token lives in the Keychain.

## Signing

`build-app.sh` signs ad hoc by default, which is enough to run on the Mac that built it. `SIGN_IDENTITY=...` uses a real identity; `Scripts/make-selfsigned-cert.sh` makes a local one (stable permissions on your own Macs; other Macs still show the Gatekeeper warning). `NOTARIZE=1 NOTARY_PROFILE=... Scripts/build-dmg.sh` notarizes with a paid Developer ID. See [docs/RELEASE.md](../../docs/RELEASE.md#signing-what-is-and-is-not-possible).

If FFmpeg is missing the Home screen offers an **Install FFmpeg** button (Homebrew).

## Layout

- `Sources/TVMasterKit`: models, client, events, keychain, daemon supervision (no UI).
- `Sources/TVMaster`: the app.
- `Scripts/build-app.sh`: bundles `TVMaster.app` (ad-hoc signed, not notarized).

## Limits

Lineup, ads and guide design still open the web admin. Not notarized. Verified visually on macOS 27 only.
