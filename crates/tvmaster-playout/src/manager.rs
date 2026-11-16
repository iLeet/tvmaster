//! Tracks which channels are on air, starts them on demand, and stops idle ones.

use crate::runner::{now_ms, run_forever};
use crate::{ChannelSpec, EngineConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use tokio_util::sync::CancellationToken;

struct Runner {
    cancel: CancellationToken,
    last_access_ms: Arc<AtomicI64>,
    always_on: bool,
    health: Arc<Health>,
}

/// How a channel's playout has been doing: failures it recovered from, and the last reason.
#[derive(Default)]
pub struct Health {
    failures: Mutex<Vec<(i64, String)>>,
}

/// What [`Manager::health`] reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelHealth {
    /// Times playout failed and had to restart in the last ten minutes.
    pub recent_failures: usize,
    /// Why it last failed.
    pub last_error: Option<String>,
}

impl Health {
    pub(crate) fn record_failure(&self, error: String) {
        let now = now_ms();
        let mut f = self.failures.lock().unwrap();
        f.retain(|(t, _)| now - t < 10 * 60_000);
        f.push((now, error));
    }

    fn snapshot(&self) -> ChannelHealth {
        let now = now_ms();
        let f = self.failures.lock().unwrap();
        let recent: Vec<&(i64, String)> = f.iter().filter(|(t, _)| now - t < 10 * 60_000).collect();
        ChannelHealth {
            recent_failures: recent.len(),
            last_error: recent.last().map(|(_, e)| e.clone()),
        }
    }
}

pub struct Manager {
    cfg: Arc<EngineConfig>,
    runners: Mutex<HashMap<String, Runner>>,
}

impl Manager {
    /// Create the manager and its idle reaper. Must be called inside a Tokio runtime.
    pub fn new(cfg: EngineConfig) -> Arc<Self> {
        let interval = cfg.reap_interval;
        let m = Arc::new(Self {
            cfg: Arc::new(cfg),
            runners: Mutex::new(HashMap::new()),
        });
        let weak: Weak<Manager> = Arc::downgrade(&m);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                match weak.upgrade() {
                    Some(m) => m.reap_idle(),
                    None => break,
                }
            }
        });
        m
    }

    pub fn config(&self) -> &EngineConfig {
        &self.cfg
    }

    /// Directory holding a channel's HLS output.
    pub fn hls_dir(&self, channel_id: &str) -> PathBuf {
        let safe: String = channel_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.cfg.root.join("hls").join(safe)
    }

    /// Record viewer activity. Returns `false` if the channel is not on air.
    pub fn touch(&self, channel_id: &str) -> bool {
        match self.runners.lock().unwrap().get(channel_id) {
            Some(r) => {
                r.last_access_ms.store(now_ms(), Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// How a channel on air has been doing (`None` when it is not on air).
    pub fn health(&self, channel_id: &str) -> Option<ChannelHealth> {
        self.runners
            .lock()
            .unwrap()
            .get(channel_id)
            .map(|r| r.health.snapshot())
    }

    pub fn is_running(&self, channel_id: &str) -> bool {
        self.runners.lock().unwrap().contains_key(channel_id)
    }

    pub fn running(&self) -> Vec<String> {
        let mut v: Vec<String> = self.runners.lock().unwrap().keys().cloned().collect();
        v.sort();
        v
    }

    /// Put a channel on air unless it already is. Returns `true` if started.
    pub fn start_if_absent(&self, spec: ChannelSpec) -> bool {
        let mut runners = self.runners.lock().unwrap();
        if runners.contains_key(&spec.id) {
            return false;
        }
        let cancel = CancellationToken::new();
        let health = Arc::new(Health::default());
        let last_access_ms = Arc::new(AtomicI64::new(now_ms()));
        runners.insert(
            spec.id.clone(),
            Runner {
                cancel: cancel.clone(),
                last_access_ms,
                always_on: spec.always_on,
                health: health.clone(),
            },
        );
        let dir = self.hls_dir(&spec.id);
        // Never serve a previous run's playlist: its segments are about to be
        // deleted, and a client that fetched it would get 404s.
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        tokio::spawn(run_forever(
            self.cfg.clone(),
            Arc::new(spec),
            dir,
            cancel,
            health,
        ));
        true
    }

    /// Take a channel off air (e.g. after its schedule or media changed).
    pub fn stop(&self, channel_id: &str) {
        if let Some(r) = self.runners.lock().unwrap().remove(channel_id) {
            r.cancel.cancel();
        }
    }

    pub fn stop_all(&self) {
        for (_, r) in self.runners.lock().unwrap().drain() {
            r.cancel.cancel();
        }
    }

    fn reap_idle(&self) {
        let now = now_ms();
        let idle_ms = self.cfg.idle_timeout.as_millis() as i64;
        let mut runners = self.runners.lock().unwrap();
        let stale: Vec<String> = runners
            .iter()
            .filter(|(_, r)| {
                !r.always_on && now - r.last_access_ms.load(Ordering::Relaxed) > idle_ms
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            if let Some(r) = runners.remove(&id) {
                tracing::info!(channel = %id, "no viewers, stopping");
                r.cancel.cancel();
            }
        }
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        if let Ok(mut runners) = self.runners.lock() {
            for (_, r) in runners.drain() {
                r.cancel.cancel();
            }
        }
    }
}
