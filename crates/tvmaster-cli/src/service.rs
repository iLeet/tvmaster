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

pub fn render(kind: Kind, bin: &Path, data_dir: &Path, bind: &str) -> String {
    let bin = bin.display().to_string();
    let data = data_dir.display().to_string();
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
    <string>--data-dir</string><string>{data}</string>
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
        ),
        Kind::Systemd => format!(
            "[Unit]\nDescription=TVMaster IPTV station\nAfter=network-online.target\nWants=network-online.target\n\n\
[Service]\nExecStart={bin} serve --bind {bind} --data-dir {data}\nRestart=always\nRestartSec=3\n\n\
[Install]\nWantedBy=multi-user.target\n"
        ),
        Kind::Windows => format!(
            "schtasks /Create /TN \"TVMaster\" /SC ONSTART /RU SYSTEM /RL HIGHEST /F ^\n  /TR \"\\\"{bin}\\\" serve --bind {bind} --data-dir \\\"{data}\\\"\"\n"
        ),
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
        );
        assert!(s.contains("A&amp;B"));
    }
}
