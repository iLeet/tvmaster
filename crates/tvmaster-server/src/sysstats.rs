//! What this machine is and how hard it is working: the processor, memory, load, disks, what TVMaster
//! and its FFmpeg encoders use, and a rough guess at how many more channels it could carry.

use crate::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use sysinfo::{Disks, Pid, ProcessesToUpdate, System};

fn system() -> &'static Mutex<System> {
    static S: OnceLock<Mutex<System>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(System::new()))
}

fn in_container() -> bool {
    std::path::Path::new("/.dockerenv").exists()
        || std::fs::read_to_string("/proc/1/cgroup").is_ok_and(|t| {
            t.contains("docker") || t.contains("kubepods") || t.contains("containerd")
        })
}

/// The disk a path lives on: the one whose mount point is the longest prefix of it.
fn disk_for<'a>(disks: &'a Disks, path: &std::path::Path) -> Option<&'a sysinfo::Disk> {
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
}

/// A rough guess at how many more channels fit, from what the encoders cost now (or what one usually
/// costs when none are running) and the processor and memory that are free.
fn capacity(
    cores: f64,
    used_cores: f64,
    encoders: usize,
    ffmpeg_cores: f64,
    hardware: bool,
    avail_mem: u64,
) -> Value {
    let default_cost = if hardware { 0.35 } else { 1.6 };
    let per_channel = if encoders > 0 && ffmpeg_cores > 0.2 {
        (ffmpeg_cores / encoders as f64).clamp(0.2, 4.0)
    } else {
        default_cost
    };
    // keep a fifth of the machine free for everything else
    let free_cores = (cores * 0.8 - used_cores).max(0.0);
    let by_cpu = (free_cores / per_channel).floor() as i64;
    // about 150 MB for each encoder and what it reads
    let by_mem = (avail_mem as f64 / (150.0 * 1024.0 * 1024.0)).floor() as i64;
    let more = by_cpu.min(by_mem).max(0);
    json!({
        "per_channel_cores": (per_channel * 100.0).round() / 100.0,
        "measured": encoders > 0 && ffmpeg_cores > 0.2,
        "free_cores": (free_cores * 100.0).round() / 100.0,
        "more_channels": more,
        "limited_by": if by_mem < by_cpu { "memory" } else { "processor" },
    })
}

/// `GET /api/v1/system/stats`: the machine and what it is doing right now. Poll it every few seconds.
pub async fn stats(State(s): State<AppState>) -> Json<Value> {
    let on_air: Vec<String> = s
        .playout
        .as_ref()
        .map(|p| p.running())
        .unwrap_or_default()
        .into_iter()
        // the guide window's commercials are an internal stream, not a channel
        .filter(|c| c != tvmaster_core::COMMERCIAL_CHANNEL)
        .collect();
    let hardware = s
        .playout
        .as_ref()
        .is_some_and(|p| p.config().encoder.name() != "software");
    let encoder = s.playout.as_ref().map(|p| p.config().encoder.name());
    let data_dir = s.data_dir.clone();
    let lib_paths: Vec<(i64, String)> = tvmaster_db::list_libraries(&s.db())
        .unwrap_or_default()
        .into_iter()
        .map(|l| (l.id, l.path))
        .collect();
    let v = tokio::task::spawn_blocking(move || {
        let mut sys = system().lock().unwrap_or_else(|p| p.into_inner());
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let me = sysinfo::get_current_pid().ok();
        let cores = sys.cpus().len().max(1);
        let cpu_pct = sys.global_cpu_usage();
        let load = System::load_average();
        let cg = sys.cgroup_limits();
        let (mem_total, mem_avail) = match &cg {
            Some(c) if c.total_memory > 0 && c.total_memory < sys.total_memory() => (c.total_memory, c.free_memory),
            _ => {
                let avail = sys.available_memory();
                // some systems (macOS) do not say what is available: it is what is not used
                let avail = if avail == 0 {
                    sys.total_memory().saturating_sub(sys.used_memory())
                } else {
                    avail
                };
                (sys.total_memory(), avail)
            }
        };
        // TVMaster itself, and every FFmpeg (its encoders and probes)
        let mine = me.and_then(|p| sys.process(p)).map(|p| {
            json!({
                "pid": p.pid().as_u32(), "cpu_pct": p.cpu_usage(), "memory": p.memory(), "uptime_s": p.run_time(),
            })
        });
        let mut ff: Vec<(Pid, f32, u64)> = sys
            .processes()
            .iter()
            .filter(|(_, p)| {
                // threads are listed as processes on Linux: count each encoder once
                p.thread_kind().is_none()
                    && p.name().to_string_lossy().to_lowercase().starts_with("ffmpeg")
            })
            .map(|(pid, p)| (*pid, p.cpu_usage(), p.memory()))
            .collect();
        ff.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let ff_cores: f64 = ff.iter().map(|f| f.1.max(0.0) as f64 / 100.0).sum();
        let ff_mem: u64 = ff.iter().map(|f| f.2).sum();
        let used_cores = (cpu_pct as f64 / 100.0) * cores as f64;

        let disks = Disks::new_with_refreshed_list();
        let mut shown: Vec<Value> = Vec::new();
        let mut add = |what: String, path: &std::path::Path| {
            if let Some(d) = disk_for(&disks, path) {
                let mount = d.mount_point().to_string_lossy().to_string();
                if let Some(existing) = shown.iter_mut().find(|x| x["mount"] == mount.as_str()) {
                    let uses = existing["used_for"].as_array_mut().unwrap();
                    uses.push(json!(what));
                } else {
                    shown.push(json!({
                        "mount": mount, "name": d.name().to_string_lossy(), "fs": d.file_system().to_string_lossy(),
                        "total": d.total_space(), "free": d.available_space(), "used_for": [what],
                    }));
                }
            }
        };
        if let Some(d) = &data_dir {
            add("TVMaster's data".into(), d);
        }
        for (_, p) in &lib_paths {
            add(format!("media: {}", p.rsplit(['/', '\\']).find(|x| !x.is_empty()).unwrap_or(p)), std::path::Path::new(p));
        }

        json!({
            "host": {
                "name": System::host_name(), "os": System::name(), "os_version": System::os_version(),
                "kernel": System::kernel_version(), "arch": std::env::consts::ARCH,
                "cpu_model": sys.cpus().first().map(|c| c.brand().trim().to_string()),
                "threads": cores, "cores": sys.physical_core_count(),
                "uptime_s": System::uptime(), "container": in_container(),
            },
            "cpu": {
                "usage_pct": cpu_pct,
                "per_core": sys.cpus().iter().map(|c| c.cpu_usage()).collect::<Vec<_>>(),
                "load": [load.one, load.five, load.fifteen],
            },
            "memory": {
                "total": mem_total, "available": mem_avail, "used": mem_total.saturating_sub(mem_avail),
                "swap_total": sys.total_swap(), "swap_used": sys.used_swap(),
                "limited_by_container": cg.is_some_and(|c| c.total_memory > 0 && c.total_memory < sys.total_memory()),
            },
            "tvmaster": mine,
            "ffmpeg": {
                "processes": ff.len(), "cpu_cores": ff_cores, "memory": ff_mem,
                "top": ff.iter().take(6).map(|f| json!({ "pid": f.0.as_u32(), "cpu_pct": f.1, "memory": f.2 })).collect::<Vec<_>>(),
            },
            "disks": shown,
            "capacity": capacity(cores as f64, used_cores, ff.len(), ff_cores, hardware, mem_avail),
        })
    })
    .await
    .unwrap_or(Value::Null);
    let mut v = v;
    v["encoder"] = json!(encoder);
    v["on_air"] = json!(on_air);
    v["version"] = json!(env!("CARGO_PKG_VERSION"));
    Json(v)
}
