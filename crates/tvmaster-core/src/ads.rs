//! Commercial breaks and bumpers.
//!
//! A show's slot (say 30 minutes) is longer than the show (22 minutes). The
//! difference is the *ad budget*. The planner splits the show at natural break
//! points, and fills each pod exactly: "we'll be right back", commercials
//! packed to fit, a station ID for any odd seconds, then "back to <show>" (or
//! "up next" after the last pod). Every piece has a fixed length, so the slot
//! always ends on the grid.

use crate::rng::{fnv1a, shuffled, splitmix64, GOLDEN};
use crate::scheduler::{ItemKind, ScheduledItem};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Station graphics shown around breaks. Content such as the show's name or
/// the upcoming programmes is filled in when the bumper airs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BumperKind {
    /// "We'll be right back" (start of a break).
    Brb,
    /// "Back to <show>" (end of a mid-show break).
    BackTo,
    /// "Up next: <program> at <time>" (end of the last break).
    UpNext,
    /// "Later today" / "Coming up tonight": the next few programmes.
    LaterToday,
    /// Channel identification; also absorbs leftover seconds.
    StationId,
}

impl BumperKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BumperKind::Brb => "brb",
            BumperKind::BackTo => "back_to",
            BumperKind::UpNext => "up_next",
            BumperKind::LaterToday => "later_today",
            BumperKind::StationId => "station_id",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "brb" => BumperKind::Brb,
            "back_to" => BumperKind::BackTo,
            "up_next" => BumperKind::UpNext,
            "later_today" => BumperKind::LaterToday,
            "station_id" => BumperKind::StationId,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BumperToggles {
    #[serde(default = "yes")]
    pub brb: bool,
    #[serde(default = "yes")]
    pub back_to: bool,
    #[serde(default = "yes")]
    pub up_next: bool,
    #[serde(default = "yes")]
    pub station_id: bool,
    /// Every Nth slot shows "Later today" instead of "Up next" (0 = never).
    #[serde(default = "default_later_every")]
    pub later_today_every: u32,
}

fn yes() -> bool {
    true
}
fn default_later_every() -> u32 {
    3
}

impl Default for BumperToggles {
    fn default() -> Self {
        Self {
            brb: true,
            back_to: true,
            up_next: true,
            station_id: true,
            later_today_every: 3,
        }
    }
}

/// Commercials and bumpers for a channel. Present in a lineup = on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdConfig {
    /// Air commercials (false = bumpers only).
    #[serde(default = "yes")]
    pub commercials: bool,
    /// Libraries (kind `commercials`) to draw commercials from.
    #[serde(default)]
    pub library_ids: Vec<i64>,
    /// Libraries (kind `bumpers`) with ready-made bumper clips.
    #[serde(default)]
    pub bumper_library_ids: Vec<i64>,
    /// Ad time per hour of programming when the channel has no time grid. On a
    /// grid the ad time is simply what is left of each slot.
    #[serde(default = "default_mph")]
    pub minutes_per_hour: u32,
    /// Target length of one commercial pod.
    #[serde(default = "default_pod")]
    pub pod_seconds: u32,
    /// Most breaks inside one show (a final break after the show is extra).
    #[serde(default = "default_breaks")]
    pub max_breaks: u32,
    /// Do not repeat a commercial within this many plays.
    #[serde(default = "default_no_repeat")]
    pub no_repeat: u32,
    /// Only air commercials from these years (inclusive), e.g. `[1980, 1989]`.
    #[serde(default)]
    pub years: Option<(i32, i32)>,
    /// Put breaks inside shows (false = all ads after the show).
    #[serde(default = "yes")]
    pub mid_breaks: bool,
    #[serde(default)]
    pub bumpers: BumperToggles,
}

fn default_mph() -> u32 {
    8
}
fn default_pod() -> u32 {
    120
}
fn default_breaks() -> u32 {
    4
}
fn default_no_repeat() -> u32 {
    12
}

impl Default for AdConfig {
    fn default() -> Self {
        Self {
            commercials: true,
            library_ids: Vec::new(),
            bumper_library_ids: Vec::new(),
            minutes_per_hour: default_mph(),
            pod_seconds: default_pod(),
            max_breaks: default_breaks(),
            no_repeat: default_no_repeat(),
            years: None,
            mid_breaks: true,
            bumpers: BumperToggles::default(),
        }
    }
}

impl AdConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.minutes_per_hour > 30 {
            return Err("ads.minutes_per_hour must be 0-30".into());
        }
        if !(20..=900).contains(&self.pod_seconds) {
            return Err("ads.pod_seconds must be 20-900".into());
        }
        if self.max_breaks > 8 {
            return Err("ads.max_breaks must be 0-8".into());
        }
        if let Some((a, b)) = self.years {
            if a > b {
                return Err("ads.years must be [from, to] with from <= to".into());
            }
        }
        Ok(())
    }
}

/// A ready-made amount of advertising: one choice instead of four numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AdPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub commercials: bool,
    pub pod_seconds: u32,
    pub max_breaks: u32,
    pub minutes_per_hour: u32,
    pub mid_breaks: bool,
}

/// The presets, from none to heavy.
pub const AD_PRESETS: [AdPreset; 6] = [
    AdPreset {
        id: "none",
        name: "No commercials",
        about: "Shows run clean. Station cards still fill any spare time.",
        commercials: false,
        pod_seconds: 120,
        max_breaks: 4,
        minutes_per_hour: 8,
        mid_breaks: true,
    },
    AdPreset {
        id: "light",
        name: "Light",
        about: "A short break or two, about a minute each.",
        commercials: true,
        pod_seconds: 60,
        max_breaks: 2,
        minutes_per_hour: 4,
        mid_breaks: true,
    },
    AdPreset {
        id: "classic",
        name: "Classic TV",
        about: "Breaks of about two minutes inside the show and after it.",
        commercials: true,
        pod_seconds: 120,
        max_breaks: 4,
        minutes_per_hour: 8,
        mid_breaks: true,
    },
    AdPreset {
        id: "heavy",
        name: "Heavy",
        about: "Long, frequent breaks, like a busy cable channel.",
        commercials: true,
        pod_seconds: 180,
        max_breaks: 6,
        minutes_per_hour: 12,
        mid_breaks: true,
    },
    AdPreset {
        id: "short",
        name: "Short break between shows",
        about: "No breaks inside shows; a small break of about a minute after each one. Works on any channel, chaos and shuffle included.",
        commercials: true,
        pod_seconds: 60,
        max_breaks: 0,
        minutes_per_hour: 4,
        mid_breaks: false,
    },
    AdPreset {
        id: "after",
        name: "After the show",
        about: "No breaks inside shows; all the ads come together at the end.",
        commercials: true,
        pod_seconds: 120,
        max_breaks: 4,
        minutes_per_hour: 8,
        mid_breaks: false,
    },
];

impl AdConfig {
    /// Take on a preset's amount of advertising (libraries, cards and the rest are kept).
    pub fn apply_preset(&mut self, p: &AdPreset) {
        self.commercials = p.commercials;
        self.pod_seconds = p.pod_seconds;
        self.max_breaks = p.max_breaks;
        self.minutes_per_hour = p.minutes_per_hour;
        self.mid_breaks = p.mid_breaks;
    }

    /// Which preset these numbers are, or `custom`.
    pub fn preset_id(&self) -> &'static str {
        AD_PRESETS
            .iter()
            .find(|p| {
                if !p.commercials {
                    !self.commercials
                } else {
                    self.commercials
                        && p.pod_seconds == self.pod_seconds
                        && p.max_breaks == self.max_breaks
                        && p.minutes_per_hour == self.minutes_per_hour
                        && p.mid_breaks == self.mid_breaks
                }
            })
            .map_or("custom", |p| p.id)
    }

    /// What these settings do to a typical slot, in plain words (the same arithmetic the planner uses).
    pub fn describe(&self, slot_minutes: u32, show_minutes: u32) -> String {
        let budget = slot_minutes.saturating_sub(show_minutes) as u64 * 60_000;
        let (slot, show) = (slot_minutes, show_minutes);
        if budget < MIN_BUDGET_MS {
            return format!(
                "A {slot}-minute slot with a {show}-minute show has no room for breaks."
            );
        }
        let target = (self.pod_seconds as u64 * 1000).max(MIN_POD_MS);
        let total = ((budget + target / 2) / target).max(1) as usize;
        let mut mid = if self.mid_breaks && total > 1 {
            (total - 1).min(self.max_breaks as usize)
        } else {
            0
        };
        while mid > 0 && budget / (mid as u64 + 1) < MIN_POD_MS {
            mid -= 1;
        }
        let pods = mid + 1;
        let each = budget / pods as u64 / 1000;
        let len = if each >= 90 {
            format!("about {} minutes", (each + 30) / 60)
        } else {
            format!("about {each} seconds")
        };
        let what = if self.commercials {
            "commercials"
        } else {
            "station cards"
        };
        let places = match mid {
            0 => "one break after the show".to_string(),
            1 => "one break inside the show and one after it".to_string(),
            n => format!("{n} breaks inside the show and one after it"),
        };
        let cards = {
            let b = &self.bumpers;
            let mut parts = Vec::new();
            if b.brb {
                parts.push("each break opens with “We'll be right back”");
            }
            if mid > 0 && b.back_to {
                parts.push("closes with “Back to the show”");
            }
            if b.up_next {
                parts.push("the slot ends with “Up next”");
            }
            if parts.is_empty() {
                String::new()
            } else {
                format!(" {}.", capitalise(&parts.join(", ")))
            }
        };
        format!(
            "A {slot}-minute slot with a {show}-minute show leaves {} minutes: {places}, {len} each, filled with {what}.{cards}",
            budget / 60_000
        )
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// A commercial or bumper clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdClip {
    pub id: i64,
    pub dur_ms: u64,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct AdPool {
    pub ads: Vec<AdClip>,
    /// Ready-made bumper clips from the user.
    pub clips: Vec<AdClip>,
}

/// Everything the planner needs besides the config.
#[derive(Debug, Clone, Default)]
pub struct AdInputs {
    pub pool: AdPool,
    /// Candidate break points (ms into the show) per media id, from chapters
    /// or black-frame detection.
    pub breaks: HashMap<i64, Vec<u64>>,
    /// The most recent commercials already on the schedule (oldest first), so
    /// a top-up does not repeat what just aired.
    pub recent: Vec<i64>,
}

pub(crate) const BRB_MS: u64 = 3_000;
pub(crate) const BACK_MS: u64 = 3_500;
pub(crate) const NEXT_MS: u64 = 5_000;
/// Closing bumpers may stretch by this much to soak up leftover seconds.
const MAX_STRETCH_MS: u64 = 4_000;
/// A budget below this is not worth a break; the slot just gets a station ID.
const MIN_BUDGET_MS: u64 = 8_000;
const MIN_POD_MS: u64 = 20_000;
const LEAD_MS: u64 = 90_000;
const TAIL_MS: u64 = 90_000;
const MIN_SPACING_MS: u64 = 120_000;
const MAX_PACK_CANDIDATES: usize = 40;

/// How much ad time a slot gets when there is no grid: `minutes_per_hour` of
/// every hour of programming, in whole seconds. Zero when nothing would air.
pub fn loose_budget_ms(cfg: &AdConfig, pool: &AdPool, runtime_ms: u64) -> u64 {
    if !cfg.commercials || pool.ads.is_empty() {
        return 0;
    }
    (runtime_ms * cfg.minutes_per_hour as u64 / 60) / 1000 * 1000
}

/// Pick up to `k` break points in a show of `runtime_ms`: real candidates
/// (chapters, detected blacks) where there are any near the ideal evenly
/// spaced positions, otherwise a synthetic act structure with a little
/// deterministic jitter. Never in the first or last 90 seconds, never closer
/// than two minutes to each other.
pub fn choose_break_points(candidates: &[u64], runtime_ms: u64, k: usize, seed: u64) -> Vec<u64> {
    if k == 0 || runtime_ms < LEAD_MS + TAIL_MS + MIN_SPACING_MS {
        return Vec::new();
    }
    let (lo, hi) = (LEAD_MS, runtime_ms - TAIL_MS);
    let valid: Vec<u64> = candidates
        .iter()
        .copied()
        .filter(|c| *c >= lo && *c <= hi)
        .collect();
    let window = runtime_ms / (2 * (k as u64 + 1));
    let mut used = vec![false; valid.len()];
    let mut rng = seed ^ fnv1a("breaks");
    let mut points = Vec::new();
    for j in 0..k {
        let ideal = runtime_ms * (j as u64 + 1) / (k as u64 + 1);
        let best = valid
            .iter()
            .enumerate()
            .filter(|(i, c)| !used[*i] && c.abs_diff(ideal) <= window)
            .min_by_key(|(_, c)| c.abs_diff(ideal));
        let p = match best {
            Some((i, c)) => {
                used[i] = true;
                *c
            }
            None => {
                let jitter = (splitmix64(&mut rng) % 60_000) as i64 - 30_000;
                (ideal as i64 + jitter).clamp(lo as i64, hi as i64) as u64
            }
        };
        points.push(p);
    }
    points.sort_unstable();
    let mut spaced: Vec<u64> = Vec::new();
    for p in points {
        if spaced.last().is_none_or(|l| p - l >= MIN_SPACING_MS) {
            spaced.push(p);
        }
    }
    spaced
}

/// Choose commercials whose total length is as close to `target_ms` as
/// possible without going over (a 0/1 knapsack over 100 ms units). Keeps the
/// given order. Returns indices into `durations_ms`.
pub fn pack_pod(durations_ms: &[u64], target_ms: u64) -> Vec<usize> {
    let cap = (target_ms / 100) as usize;
    if cap == 0 || durations_ms.is_empty() {
        return Vec::new();
    }
    // Round each length up so the true total can never exceed the target.
    let w: Vec<usize> = durations_ms
        .iter()
        .map(|d| d.div_ceil(100) as usize)
        .collect();
    let mut parent: Vec<Option<usize>> = vec![None; cap + 1];
    let mut reach = vec![false; cap + 1];
    reach[0] = true;
    for (i, wi) in w.iter().enumerate() {
        if *wi == 0 || *wi > cap {
            continue;
        }
        for s in (*wi..=cap).rev() {
            if !reach[s] && reach[s - wi] {
                reach[s] = true;
                parent[s] = Some(i);
            }
        }
    }
    let Some(best) = (0..=cap).rev().find(|s| reach[*s]) else {
        return Vec::new();
    };
    let mut chosen = Vec::new();
    let mut s = best;
    while s > 0 {
        let i = parent[s].expect("reachable sum has a parent");
        chosen.push(i);
        s -= w[i];
    }
    chosen.sort_unstable();
    chosen
}

/// The show part of a slot, before it is expanded into breaks.
pub(crate) struct SlotPlan {
    pub start_ms: i64,
    pub slot_end_ms: i64,
    pub media_id: i64,
    pub runtime_ms: u64,
    pub cursor_key: String,
    pub cursor_pos: u64,
    pub label: Option<String>,
}

fn item(kind: ItemKind, start: i64, dur: u64) -> ScheduledItem {
    ScheduledItem {
        kind,
        media_id: None,
        start_ms: start,
        end_ms: start + dur as i64,
        cursor_key: None,
        cursor_pos: None,
        label: None,
        in_ms: 0,
        bumper: None,
    }
}

fn bumper(kind: BumperKind, start: i64, dur: u64, about: Option<i64>) -> ScheduledItem {
    ScheduledItem {
        media_id: about,
        bumper: Some(kind),
        ..item(ItemKind::Bumper, start, dur)
    }
}

enum PodEnd {
    /// Mid-show break: "back to <show>".
    BackToShow,
    /// After the show: "up next" or "later today".
    Next(BumperKind),
}

struct Expander<'a> {
    cfg: &'a AdConfig,
    inputs: &'a AdInputs,
    recent: &'a mut VecDeque<i64>,
    seed: u64,
    plan: &'a SlotPlan,
    out: Vec<ScheduledItem>,
    cursor_ms: i64,
}

impl Expander<'_> {
    fn push(&mut self, mut it: ScheduledItem) {
        it.label = self.plan.label.clone();
        self.cursor_ms = it.end_ms;
        self.out.push(it);
    }

    fn remember(&mut self, id: i64) {
        self.recent.push_back(id);
        while self.recent.len() > self.cfg.no_repeat.max(1) as usize * 2 {
            self.recent.pop_front();
        }
    }

    /// Commercial candidates for a pod: allowed year, not played recently,
    /// shuffled deterministically.
    fn candidates(&self, pod: usize) -> Vec<crate::ads::AdClip> {
        if !self.cfg.commercials {
            return Vec::new();
        }
        let ok_year = |c: &AdClip| match (self.cfg.years, c.year) {
            (Some((a, b)), Some(y)) => y >= a && y <= b,
            (Some(_), None) => false,
            _ => true,
        };
        let pool: Vec<AdClip> = self
            .inputs
            .pool
            .ads
            .iter()
            .copied()
            .filter(ok_year)
            .collect();
        let window = self.cfg.no_repeat as usize;
        let skip: Vec<i64> = self.recent.iter().rev().take(window).copied().collect();
        let mut fresh: Vec<AdClip> = pool
            .iter()
            .copied()
            .filter(|c| !skip.contains(&c.id))
            .collect();
        // A small library cannot honour the window: relax it rather than run dry.
        if fresh.len() < 6 {
            fresh = pool;
        }
        fresh.sort_by_key(|c| c.id);
        let seed = self.seed ^ fnv1a(&format!("pod{}/{}", self.plan.start_ms, pod)) ^ GOLDEN;
        shuffled(fresh.len(), seed)
            .into_iter()
            .take(MAX_PACK_CANDIDATES)
            .map(|i| fresh[i])
            .collect()
    }

    /// Fill one pod of exactly `budget` ms starting at the current cursor.
    fn pod(&mut self, pod_idx: usize, budget: u64, end: PodEnd) {
        let toggles = self.cfg.bumpers.clone();
        let show = Some(self.plan.media_id);
        let mid = matches!(end, PodEnd::BackToShow);

        // Opening: a ready-made clip if the user supplied one that fits, else "be right back".
        let mut open: Option<ScheduledItem> = None;
        let mut open_ms = 0;
        if mid && toggles.brb {
            let clips: Vec<AdClip> = self
                .inputs
                .pool
                .clips
                .iter()
                .copied()
                .filter(|c| c.dur_ms <= 15_000 && c.dur_ms * 3 <= budget)
                .collect();
            let mut s = self.seed ^ fnv1a(&format!("clip{}/{}", self.plan.start_ms, pod_idx));
            if !clips.is_empty() {
                let c = clips[(splitmix64(&mut s) % clips.len() as u64) as usize];
                open_ms = c.dur_ms;
                open = Some(ScheduledItem {
                    media_id: Some(c.id),
                    ..item(ItemKind::Bumper, 0, c.dur_ms)
                });
            } else {
                open_ms = BRB_MS;
                open = Some(bumper(BumperKind::Brb, 0, BRB_MS, show));
            }
        }
        let (close_kind, close_ms, close_enabled) = match end {
            PodEnd::BackToShow => (BumperKind::BackTo, BACK_MS, toggles.back_to),
            PodEnd::Next(k) => (k, NEXT_MS, toggles.up_next),
        };
        let close_ms = if close_enabled { close_ms } else { 0 };
        if open_ms + close_ms + 4_000 > budget {
            // Too small for the full dressing: just a station ID.
            self.push(bumper(BumperKind::StationId, self.cursor_ms, budget, None));
            return;
        }
        let ad_target = budget - open_ms - close_ms;

        let cands = self.candidates(pod_idx);
        let durs: Vec<u64> = cands.iter().map(|c| c.dur_ms).collect();
        let chosen: Vec<AdClip> = pack_pod(&durs, ad_target)
            .into_iter()
            .map(|i| cands[i])
            .collect();
        let used: u64 = chosen.iter().map(|c| c.dur_ms).sum();
        let leftover = ad_target - used;

        if let Some(mut o) = open {
            o.start_ms = self.cursor_ms;
            o.end_ms = o.start_ms + open_ms as i64;
            self.push(o);
        }
        for c in &chosen {
            self.remember(c.id);
            let mut it = item(ItemKind::Commercial, self.cursor_ms, c.dur_ms);
            it.media_id = Some(c.id);
            self.push(it);
        }
        // Odd seconds: stretch the closing bumper a little, and put anything
        // beyond that into a station ID.
        let stretch = if close_ms > 0 {
            leftover.min(MAX_STRETCH_MS)
        } else {
            0
        };
        let mut rest = leftover - stretch;
        let mut close_len = close_ms + stretch;
        if rest > 0 && close_ms > 0 && rest < 1_500 {
            // A sliver too short for its own card: the closing bumper absorbs it.
            close_len += rest;
            rest = 0;
        }
        if rest > 0 {
            if toggles.station_id || close_ms == 0 {
                self.push(bumper(BumperKind::StationId, self.cursor_ms, rest, None));
            } else {
                close_len += rest;
            }
        }
        if close_ms > 0 {
            self.push(bumper(close_kind, self.cursor_ms, close_len, show));
        }
    }
}

/// Turn a slot into its show parts, commercial pods and bumpers. The result is
/// contiguous, starts at `plan.start_ms`, and ends exactly at
/// `plan.slot_end_ms`.
pub(crate) fn expand_slot(
    cfg: &AdConfig,
    inputs: &AdInputs,
    recent: &mut VecDeque<i64>,
    seed: u64,
    plan: &SlotPlan,
) -> Vec<ScheduledItem> {
    let runtime = plan.runtime_ms;
    let show_end = plan.start_ms + runtime as i64;
    let budget = (plan.slot_end_ms - show_end).max(0) as u64;

    let mut ex = Expander {
        cfg,
        inputs,
        recent,
        seed,
        plan,
        out: Vec::new(),
        cursor_ms: plan.start_ms,
    };

    let show_part = |from: u64, to: u64, first: bool, at: i64| ScheduledItem {
        kind: ItemKind::Show,
        media_id: Some(plan.media_id),
        start_ms: at,
        end_ms: at + (to - from) as i64,
        cursor_key: first.then(|| plan.cursor_key.clone()),
        cursor_pos: first.then_some(plan.cursor_pos),
        label: plan.label.clone(),
        in_ms: from,
        bumper: None,
    };

    if budget < MIN_BUDGET_MS {
        ex.push(show_part(0, runtime, true, plan.start_ms));
        if budget > 0 {
            let mut f = item(ItemKind::Filler, show_end, budget);
            f.label = plan.label.clone();
            ex.out.push(f);
        }
        return ex.out;
    }

    // How many pods, and where the mid-show ones go.
    let target = (cfg.pod_seconds as u64 * 1000).max(MIN_POD_MS);
    let mut pods = ((budget + target / 2) / target).max(1) as usize; // total pods
    let mut points = Vec::new();
    if cfg.mid_breaks && pods > 1 {
        let wanted = (pods - 1).min(cfg.max_breaks as usize);
        let cands = inputs
            .breaks
            .get(&plan.media_id)
            .map_or(&[][..], |v| &v[..]);
        points = choose_break_points(cands, runtime, wanted, seed ^ plan.start_ms as u64);
    }
    pods = points.len() + 1;
    while pods > 1 && budget / (pods as u64) < MIN_POD_MS {
        points.pop();
        pods -= 1;
    }

    let per = budget / pods as u64;
    let mut budgets = vec![per; pods];
    *budgets.last_mut().expect("at least one pod") += budget - per * pods as u64;

    let mut from = 0u64;
    for (i, p) in points.iter().enumerate() {
        let at = ex.cursor_ms;
        ex.push(show_part(from, *p, i == 0, at));
        ex.pod(i, budgets[i], PodEnd::BackToShow);
        from = *p;
    }
    let at = ex.cursor_ms;
    ex.push(show_part(from, runtime, points.is_empty(), at));

    // After the show: "up next", or every Nth slot "later today".
    let every = cfg.bumpers.later_today_every as u64;
    let later = every > 0 && (seed ^ fnv1a(&plan.start_ms.to_string())).is_multiple_of(every);
    let end = PodEnd::Next(if later {
        BumperKind::LaterToday
    } else {
        BumperKind::UpNext
    });
    ex.pod(points.len(), budgets[pods - 1], end);
    ex.out
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1000;

    fn clip(id: i64, secs: u64) -> AdClip {
        AdClip {
            id,
            dur_ms: secs * S,
            year: None,
        }
    }

    fn plan(runtime_s: u64, slot_s: u64) -> SlotPlan {
        SlotPlan {
            start_ms: 1_000_000,
            slot_end_ms: 1_000_000 + (slot_s * S) as i64,
            media_id: 7,
            runtime_ms: runtime_s * S,
            cursor_key: "show:x".into(),
            cursor_pos: 3,
            label: Some("Prime".into()),
        }
    }

    fn inputs(n_ads: i64) -> AdInputs {
        let ads = (0..n_ads)
            .map(|i| clip(100 + i, [15, 30, 30, 45, 60][(i % 5) as usize]))
            .collect();
        AdInputs {
            pool: AdPool {
                ads,
                clips: Vec::new(),
            },
            ..Default::default()
        }
    }

    fn assert_exact(items: &[ScheduledItem], p: &SlotPlan) {
        assert_eq!(items[0].start_ms, p.start_ms);
        for w in items.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms, "gap/overlap: {w:?}");
        }
        assert_eq!(
            items.last().unwrap().end_ms,
            p.slot_end_ms,
            "slot must end on the grid"
        );
        assert!(
            items.iter().all(|i| i.end_ms > i.start_ms),
            "no empty items"
        );
    }

    #[test]
    fn packing_fills_exactly_when_possible_and_never_overshoots() {
        let d = [15 * S, 30 * S, 30 * S, 45 * S, 60 * S];
        let pick = pack_pod(&d, 120 * S);
        let sum: u64 = pick.iter().map(|i| d[*i]).sum();
        assert_eq!(sum, 120 * S);
        // an impossible target takes the closest fit below it
        let pick = pack_pod(&d, 100 * S);
        let sum: u64 = pick.iter().map(|i| d[*i]).sum();
        assert!((90 * S..=100 * S).contains(&sum), "{sum}");
        assert!(pack_pod(&d, 5 * S).is_empty());
        assert!(pack_pod(&[], 60 * S).is_empty());
    }

    #[test]
    fn packing_never_exceeds_target_with_odd_millisecond_lengths() {
        let d = [14_950, 29_990, 30_010, 44_999];
        for target in [30_000, 45_000, 60_000, 75_000, 119_999] {
            let pick = pack_pod(&d, target);
            let sum: u64 = pick.iter().map(|i| d[*i]).sum();
            assert!(sum <= target, "{sum} > {target}");
        }
    }

    #[test]
    fn break_points_follow_real_candidates_and_respect_limits() {
        let runtime = 22 * 60 * S;
        let cands = [
            60 * S,
            5 * 60 * S + 3 * S,
            11 * 60 * S,
            16 * 60 * S,
            21 * 60 * S + 30 * S,
        ];
        let pts = choose_break_points(&cands, runtime, 3, 1);
        assert_eq!(pts.len(), 3);
        assert!(
            pts.contains(&(11 * 60 * S)),
            "uses the real candidate near the middle"
        );
        for p in &pts {
            assert!(*p >= 90 * S && *p <= runtime - 90 * S);
        }
        for w in pts.windows(2) {
            assert!(w[1] - w[0] >= 120 * S);
        }
    }

    #[test]
    fn break_points_fall_back_to_acts_and_are_deterministic() {
        let runtime = 22 * 60 * S;
        let a = choose_break_points(&[], runtime, 3, 5);
        let b = choose_break_points(&[], runtime, 3, 5);
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
        // roughly quarters of the show
        for (p, frac) in a.iter().zip([0.25, 0.5, 0.75]) {
            let ideal = runtime as f64 * frac;
            assert!(
                (*p as f64 - ideal).abs() <= 31.0 * S as f64,
                "{p} vs {ideal}"
            );
        }
        assert!(
            choose_break_points(&[], 3 * 60 * S, 3, 1).is_empty(),
            "too short for breaks"
        );
        assert!(choose_break_points(&[], runtime, 0, 1).is_empty());
    }

    #[test]
    fn a_22_minute_show_in_a_30_minute_slot_gets_breaks_that_fill_it_exactly() {
        let p = plan(22 * 60, 30 * 60);
        let cfg = AdConfig::default();
        let inp = inputs(40);
        let items = expand_slot(&cfg, &inp, &mut VecDeque::new(), 9, &p);
        assert_exact(&items, &p);

        // the show's parts add up to the whole episode, in order, with no overlap in media time
        let parts: Vec<&ScheduledItem> =
            items.iter().filter(|i| i.kind == ItemKind::Show).collect();
        assert!(parts.len() >= 3, "several acts: {}", parts.len());
        let mut at = 0;
        for part in &parts {
            assert_eq!(part.in_ms, at);
            at += (part.end_ms - part.start_ms) as u64;
        }
        assert_eq!(at, 22 * 60 * S);
        // only the first part carries the cursor
        assert_eq!(parts[0].cursor_key.as_deref(), Some("show:x"));
        assert!(parts[1..].iter().all(|p| p.cursor_key.is_none()));

        // commercials and bumpers are present and the slot ends with an up-next/later-today card
        assert!(items.iter().any(|i| i.kind == ItemKind::Commercial));
        let last = items.last().unwrap();
        assert!(matches!(
            last.bumper,
            Some(BumperKind::UpNext | BumperKind::LaterToday)
        ));
        // every mid break is bracketed by "right back" and "back to"
        let brb = items
            .iter()
            .filter(|i| i.bumper == Some(BumperKind::Brb))
            .count();
        let back = items
            .iter()
            .filter(|i| i.bumper == Some(BumperKind::BackTo))
            .count();
        assert_eq!(brb, back);
        assert_eq!(brb, parts.len() - 1);
        // the label carries onto everything
        assert!(items.iter().all(|i| i.label.as_deref() == Some("Prime")));
    }

    #[test]
    fn every_pod_ends_exactly_on_budget_across_many_shapes() {
        let cfg = AdConfig::default();
        for pool in [0, 1, 3, 40] {
            let inp = inputs(pool);
            for (runtime, slot) in [
                (22 * 60, 30 * 60),
                (44 * 60, 60 * 60),
                (25 * 60 + 17, 30 * 60),
                (10 * 60, 30 * 60),
                (21 * 60 + 41, 30 * 60),
                (50 * 60, 60 * 60),
            ] {
                for seed in 0..5 {
                    let p = plan(runtime, slot);
                    let items = expand_slot(&cfg, &inp, &mut VecDeque::new(), seed, &p);
                    assert_exact(&items, &p);
                    let show: u64 = items
                        .iter()
                        .filter(|i| i.kind == ItemKind::Show)
                        .map(|i| (i.end_ms - i.start_ms) as u64)
                        .sum();
                    assert_eq!(show, runtime * S);
                }
            }
        }
    }

    #[test]
    fn tiny_budgets_get_filler_not_a_break() {
        let cfg = AdConfig::default();
        let p = plan(29 * 60 + 55, 30 * 60); // 5 seconds spare
        let items = expand_slot(&cfg, &inputs(10), &mut VecDeque::new(), 1, &p);
        assert_exact(&items, &p);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].kind, ItemKind::Filler);
        // exactly full: just the show
        let p = plan(30 * 60, 30 * 60);
        let items = expand_slot(&cfg, &inputs(10), &mut VecDeque::new(), 1, &p);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn no_commercial_repeats_within_the_window_and_recent_is_honoured() {
        let cfg = AdConfig {
            no_repeat: 12,
            ..Default::default()
        };
        let inp = inputs(60);
        let mut recent: VecDeque<i64> = (100..106).collect();
        let mut played = Vec::new();
        for n in 0..6 {
            let mut p = plan(22 * 60, 30 * 60);
            p.start_ms += n * 1_800_000;
            p.slot_end_ms += n * 1_800_000;
            let items = expand_slot(&cfg, &inp, &mut recent, 3, &p);
            played.extend(
                items
                    .iter()
                    .filter(|i| i.kind == ItemKind::Commercial)
                    .filter_map(|i| i.media_id),
            );
        }
        for (i, id) in played.iter().enumerate() {
            let window = &played[i.saturating_sub(12)..i];
            assert!(
                !window.contains(id),
                "ad {id} repeated within 12 plays at {i}"
            );
        }
        assert!(played.len() > 30);
        assert!(
            !played[..3].iter().any(|id| (100..106).contains(id)),
            "just-aired ads are skipped"
        );
    }

    #[test]
    fn years_filter_limits_commercials() {
        let mut inp = inputs(0);
        inp.pool.ads = (0..30)
            .map(|i| AdClip {
                id: i,
                dur_ms: 30 * S,
                year: Some(1975 + (i as i32 % 20)),
            })
            .collect();
        let cfg = AdConfig {
            years: Some((1985, 1989)),
            ..Default::default()
        };
        let p = plan(22 * 60, 30 * 60);
        let items = expand_slot(&cfg, &inp, &mut VecDeque::new(), 1, &p);
        assert_exact(&items, &p);
        let ads: Vec<i64> = items
            .iter()
            .filter(|i| i.kind == ItemKind::Commercial)
            .filter_map(|i| i.media_id)
            .collect();
        assert!(!ads.is_empty());
        for id in ads {
            let year = inp
                .pool
                .ads
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .year
                .unwrap();
            assert!((1985..=1989).contains(&year));
        }
    }

    #[test]
    fn bumper_only_and_toggles() {
        // commercials off: bumpers and station IDs fill the time
        let cfg = AdConfig {
            commercials: false,
            ..Default::default()
        };
        let p = plan(22 * 60, 30 * 60);
        let items = expand_slot(&cfg, &inputs(20), &mut VecDeque::new(), 1, &p);
        assert_exact(&items, &p);
        assert!(!items.iter().any(|i| i.kind == ItemKind::Commercial));
        assert!(items
            .iter()
            .any(|i| i.bumper == Some(BumperKind::StationId)));

        // all breaks after the show
        let cfg = AdConfig {
            mid_breaks: false,
            ..Default::default()
        };
        let items = expand_slot(&cfg, &inputs(20), &mut VecDeque::new(), 1, &p);
        assert_exact(&items, &p);
        assert_eq!(items.iter().filter(|i| i.kind == ItemKind::Show).count(), 1);
        assert!(items.iter().any(|i| i.kind == ItemKind::Commercial));

        // up-next off
        let cfg = AdConfig {
            bumpers: BumperToggles {
                up_next: false,
                later_today_every: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let items = expand_slot(&cfg, &inputs(20), &mut VecDeque::new(), 1, &p);
        assert_exact(&items, &p);
        assert!(!items.iter().any(|i| i.bumper == Some(BumperKind::UpNext)));
    }

    #[test]
    fn user_bumper_clips_open_breaks_when_available() {
        let mut inp = inputs(20);
        inp.pool.clips = vec![clip(900, 6), clip(901, 8)];
        let p = plan(22 * 60, 30 * 60);
        let items = expand_slot(&AdConfig::default(), &inp, &mut VecDeque::new(), 1, &p);
        assert_exact(&items, &p);
        assert!(items.iter().any(|i| i.kind == ItemKind::Bumper
            && i.bumper.is_none()
            && matches!(i.media_id, Some(900 | 901))));
    }

    #[test]
    fn loose_budget_is_minutes_per_hour_of_runtime() {
        let cfg = AdConfig::default(); // 8 per hour
        let pool = inputs(5).pool;
        assert_eq!(loose_budget_ms(&cfg, &pool, 60 * 60 * S), 8 * 60 * S);
        assert_eq!(loose_budget_ms(&cfg, &pool, 30 * 60 * S), 4 * 60 * S);
        assert_eq!(
            loose_budget_ms(&cfg, &AdPool::default(), 30 * 60 * S),
            0,
            "nothing to air"
        );
        let off = AdConfig {
            commercials: false,
            ..Default::default()
        };
        assert_eq!(loose_budget_ms(&off, &pool, 30 * 60 * S), 0);
    }

    #[test]
    fn presets_apply_are_recognised_and_explain_themselves() {
        let mut c = AdConfig::default();
        assert_eq!(c.preset_id(), "classic");
        for p in AD_PRESETS.iter() {
            c.apply_preset(p);
            assert_eq!(c.preset_id(), p.id);
            assert!(c.validate().is_ok());
        }
        c.apply_preset(&AD_PRESETS[2]);
        c.pod_seconds = 77;
        assert_eq!(c.preset_id(), "custom");
        let classic = AdConfig::default().describe(30, 22);
        assert!(
            classic.contains("8 minutes") && classic.contains("inside the show"),
            "{classic}"
        );
        assert!(AdConfig::default().describe(30, 30).contains("no room"));
        let mut after = AdConfig::default();
        after.apply_preset(&AD_PRESETS[5]);
        assert!(after.describe(60, 44).contains("one break after the show"));
        let mut none = AdConfig::default();
        none.apply_preset(&AD_PRESETS[0]);
        assert!(none.describe(30, 22).contains("station cards"));
        let mut short = AdConfig::default();
        short.apply_preset(&AD_PRESETS[4]);
        assert_eq!(short.preset_id(), "short");
        assert!(!short.mid_breaks);
        assert!(short.describe(30, 22).contains("after the show"));
    }

    #[test]
    fn config_validation() {
        assert!(AdConfig::default().validate().is_ok());
        for bad in [
            AdConfig {
                minutes_per_hour: 40,
                ..Default::default()
            },
            AdConfig {
                pod_seconds: 5,
                ..Default::default()
            },
            AdConfig {
                max_breaks: 20,
                ..Default::default()
            },
            AdConfig {
                years: Some((1990, 1980)),
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err());
        }
        let c: AdConfig =
            serde_json::from_str(r#"{"library_ids":[2],"years":[1980,1989]}"#).unwrap();
        assert_eq!(c.years, Some((1980, 1989)));
        assert!(c.bumpers.brb && c.mid_breaks);
    }
}
