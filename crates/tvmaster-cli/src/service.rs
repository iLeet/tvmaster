//! Templates for running the daemon unattended (Autopilot): start on boot,
//! restart on crash. We print them rather than installing anything, so you can
//! review and place them yourself.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    Launchd,
    Systemd,
    /// Windows Task Scheduler (runs at startup, restarts on failure).
    Windows,
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn render(
    kind: Kind,
    bin: &Path,
    data_dir: &Path,
    bind: &str,
    token_file: Option<&Path>,
    tls: bool,
) -> String {
    let bin = bin.display().to_string();
    let data = data_dir.display().to_string();
    let token = token_file.map(|f| f.display().to_string());
    let tls_arg = if tls { " --tls" } else { "" };
    match kind {
        Kind::Launchd => format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.tvmaster.daemon</string>
  <key>ProgramArguments</key>
  <array>
    <string>{bin}</string>
    <string>serve</string>
    <string>--bind</string><string>{bind}</string>
    <string>--data-dir</string><string>{data}</string>{token_args}{tls_xml}
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{data}/tvmaster.log</string>
  <key>StandardErrorPath</key><string>{data}/tvmaster.log</string>
</dict>
</plist>
"#,
            bin = xml_escape(&bin),
            bind = xml_escape(bind),
            data = xml_escape(&data),
            tls_xml = if tls {
                "\n    <string>--tls</string>"
            } else {
                ""
            },
            token_args = token
                .as_deref()
                .map(|t| format!(
                    "\n    <string>--initial-token-file</string><string>{}</string>",
                    xml_escape(t)
                ))
                .unwrap_or_default(),
        ),
        Kind::Systemd => {
            let token_arg = token
                .as_deref()
                .map(|t| format!(" --initial-token-file {t}"))
                .unwrap_or_default();
            format!(
            "[Unit]\nDescription=TVMaster IPTV station\nAfter=network-online.target\nWants=network-online.target\n\n\
[Service]\nExecStart={bin} serve --bind {bind} --data-dir {data}{token_arg}{tls_arg}\nRestart=always\nRestartSec=3\n\n\
[Install]\nWantedBy=multi-user.target\n"
            )
        }
        Kind::Windows => {
            let token_arg = token
                .as_deref()
                .map(|t| format!(" --initial-token-file \\\"{t}\\\""))
                .unwrap_or_default();
            format!(
            "schtasks /Create /TN \"TVMaster\" /SC ONSTART /RU SYSTEM /RL HIGHEST /F ^\n  /TR \"\\\"{bin}\\\" serve --bind {bind} --data-dir \\\"{data}\\\"{token_arg}{tls_arg}\"\n"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn r(k: Kind) -> String {
        render(
            k,
            &PathBuf::from("/usr/local/bin/tvmaster"),
            &PathBuf::from("/var/lib/tvmaster"),
            "0.0.0.0:8787",
            None,
            false,
        )
    }

    #[test]
    fn launchd_restarts_and_starts_at_load() {
        let s = r(Kind::Launchd);
        assert!(s.contains("<key>KeepAlive</key><true/>"));
        assert!(s.contains("<key>RunAtLoad</key><true/>"));
        assert!(s.contains("/usr/local/bin/tvmaster"));
    }

    #[test]
    fn systemd_restarts_always() {
        let s = r(Kind::Systemd);
        assert!(s.contains("Restart=always"));
        assert!(s.contains("ExecStart=/usr/local/bin/tvmaster serve --bind 0.0.0.0:8787 --data-dir /var/lib/tvmaster"));
    }

    #[test]
    fn windows_task_runs_at_startup() {
        assert!(r(Kind::Windows).contains("/SC ONSTART"));
    }

    #[test]
    fn paths_are_xml_escaped_in_plist() {
        let s = render(
            Kind::Launchd,
            &PathBuf::from("/Apps/A&B/tvmaster"),
            &PathBuf::from("/d"),
            "x",
            None,
            false,
        );
        assert!(s.contains("A&amp;B"));
    }

    #[test]
    fn the_first_run_token_comes_from_a_file_not_the_command_line() {
        let t = std::path::Path::new("/home/me/token");
        let args = |k| {
            render(
                k,
                &PathBuf::from("/b/tvmaster"),
                &PathBuf::from("/d"),
                "x",
                Some(t),
                false,
            )
        };
        assert!(args(Kind::Systemd).contains("--initial-token-file /home/me/token"));
        assert!(args(Kind::Launchd)
            .contains("<string>--initial-token-file</string><string>/home/me/token</string>"));
        assert!(args(Kind::Windows).contains("--initial-token-file"));
        assert!(!args(Kind::Systemd).contains("--token "));
    }

    #[test]
    fn https_is_added_to_every_kind_of_definition_when_asked() {
        let args = |k, tls| {
            render(
                k,
                &PathBuf::from("/b/tvmaster"),
                &PathBuf::from("/d"),
                "x",
                None,
                tls,
            )
        };
        assert!(args(Kind::Systemd, true).contains(" --tls"));
        assert!(args(Kind::Launchd, true).contains("<string>--tls</string>"));
        assert!(args(Kind::Windows, true).contains(" --tls"));
        for k in [Kind::Systemd, Kind::Launchd, Kind::Windows] {
            assert!(!args(k, false).contains("--tls"));
        }
    }
}
