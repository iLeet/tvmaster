# TVMaster desktop shell

A small native window (Tauri 2) around the admin UI, so TVMaster feels like an app on Windows, macOS and Linux.

- Opens a connect screen (remembers your server) and then the UI at `/admin/`.
- If a `tvmaster` binary sits next to the app, it starts it on `127.0.0.1:8787` and stops it on quit.
- Applies native glass behind the UI: vibrancy on macOS, Mica on Windows 11. The web UI switches to a transparent background inside the shell (`window.__TVMASTER_SHELL__`).
- On macOS the real Liquid Glass look is planned for the native SwiftUI app (milestone 7).

## Build

Needs Rust and Tauri's platform dependencies (https://v2.tauri.app/start/prerequisites/).

```bash
cd apps/desktop/src-tauri
cargo run            # development
cargo install tauri-cli --version "^2" && cargo tauri build    # installers
```

Generate the platform icon set from `icons/icon.png` with `cargo tauri icon icons/icon.png` before building installers.
