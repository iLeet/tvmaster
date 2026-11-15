//! TVMaster desktop shell.
//!
//! A native window around the admin UI. It can start the bundled `tvmaster`
//! daemon (when one sits next to the app) and applies the platform's native
//! glass: vibrancy on macOS, Mica on Windows 11. Everything else is the same
//! web UI the daemon serves, so the shell stays tiny.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::{Child, Command};
use std::sync::Mutex;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

/// The daemon we started, so we can stop it when the app quits.
struct Daemon(Mutex<Option<Child>>);

/// `tvmaster` next to this executable, if it is there.
fn bundled_daemon() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "tvmaster.exe" } else { "tvmaster" };
    let path = exe.parent()?.join(name);
    path.is_file().then_some(path)
}

fn daemon_is_up(addr: &str) -> bool {
    std::net::TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| ([127, 0, 0, 1], 8787).into()),
        std::time::Duration::from_millis(300),
    )
    .is_ok()
}

fn start_daemon() -> Option<Child> {
    if daemon_is_up("127.0.0.1:8787") {
        return None; // already running (a service, or another app window)
    }
    let bin = bundled_daemon()?;
    let data = dirs_data_dir();
    Command::new(bin)
        .args(["serve", "--bind", "127.0.0.1:8787", "--data-dir"])
        .arg(data)
        .spawn()
        .ok()
}

fn dirs_data_dir() -> std::path::PathBuf {
    let base = std::env::var_os("TVMASTER_DATA_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".tvmaster")))
        .or_else(|| std::env::var_os("APPDATA").map(|a| std::path::Path::new(&a).join("TVMaster")))
        .unwrap_or_else(|| std::path::PathBuf::from("tvmaster-data"));
    let _ = std::fs::create_dir_all(&base);
    base
}

fn main() {
    let app = tauri::Builder::default()
        .manage(Daemon(Mutex::new(None)))
        .setup(|app| {
            *app.state::<Daemon>().0.lock().unwrap() = start_daemon();

            let platform = if cfg!(target_os = "macos") {
                "mac"
            } else if cfg!(windows) {
                "win"
            } else {
                "linux"
            };
            // The script runs on every page the window loads (including the
            // server's UI), so the web app always knows it is inside the shell
            // and can let the native glass show through.
            #[allow(unused_mut)]
            let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("TVMaster")
                .inner_size(1280.0, 820.0)
                .min_inner_size(720.0, 520.0)
                .transparent(true)
                .initialization_script(&format!("window.__TVMASTER_SHELL__ = '{platform}';"));
            #[cfg(target_os = "macos")]
            {
                builder = builder
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true);
            }
            let window = builder.build()?;

            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                // macOS 26's Liquid Glass arrives with the native SwiftUI app; until
                // then, the system's own translucent material behind the UI.
                let _ = apply_vibrancy(&window, NSVisualEffectMaterial::UnderWindowBackground, None, None);
            }
            #[cfg(target_os = "windows")]
            {
                let _ = window_vibrancy::apply_mica(&window, None);
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let _ = &window;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the TVMaster desktop shell");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(mut child) = handle.state::<Daemon>().0.lock().unwrap().take() {
                let _ = child.kill();
            }
        }
    });
}
