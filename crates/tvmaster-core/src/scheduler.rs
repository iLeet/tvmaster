//! The scheduler: turns a lineup, a catalog and the episode cursors into a
//! concrete, contiguous list of items. Pure and deterministic: the same inputs
//! always give the same schedule, so it can be simulated with a fake clock.
//!
//! Resolution order at every slot start: events > the active season's blocks >
//! the base blocks > the channel's fill programming. Each pick advances a
//! *reserved* cursor; the caller commits cursors as items actually air.

use crate::ads::{expand_slot, loose_budget_ms, AdConfig, AdInputs, BumperKind, SlotPlan};
use crate::catalog::{Catalog, Ep};
use crate::clock::Zone;
use crate::cursor::EpisodeCursor;
use crate::lineup::{Advance, CBlock, Compiled, Lineup, Order, Source};
use crate::model::{RunOutPolicy, Strategy};
use crate::rng::{fnv1a, shuffled, splitmix64, GOLDEN};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// Episode cursors by key (see [`ScheduledItem::cursor_key`]).
pub type Cursors = BTreeMap<String, EpisodeCursor>;

const IDLE_GAP_MS: i64 = 30 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A show, or one act of it (see `in_ms`).
    Show,
    Commercial,
    /// A station bumper: generated (`bumper` is set) or a user's clip
    /// (`bumper` is `None`, `media_id` is the clip).
    Bumper,
    /// Unfilled time (nothing to air, or a slot too short for a break).
    /// Playout shows a station slate.
    Filler,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledItem {
    pub kind: ItemKind,
    pub media_id: Option<i64>,
    pub start_ms: i64,
    pub end_ms: i64,
    /// Which cursor this pick advanced, and where. Committing the item moves
    /// that cursor's `committed` to `cursor_pos + 1`.
    pub cursor_key: Option<String>,
    pub cursor_pos: Option<u64>,
    /// Block or event name, for display.
    pub label: Option<String>,
    /// Where in the file this item starts (acts of a show start part-way in).
    pub in_ms: u64,
    /// For generated bumpers: which one. `media_id` then names the show it is about.
    pub bumper: Option<BumperKind>,
}

pub struct SchedConfig<'a> {
    /// Commercials, user bumper clips, break points and recent ads. Only used
    /// when the lineup has an `ads` section.
    pub ads: Option<&'a AdInputs>,
    pub catalog: &'a Catalog,
    pub lineup: &'a Lineup,
    pub strategy: Strategy,
    pub run_out: RunOutPolicy,
    /// 0-100: percent of picks swapped for a random one.
    pub chaos: u8,
    pub seed: u64,
    /// When the channel "started broadcasting" (anchors rotation weeks).
    pub epoch_ms: i64,
}

fn scoped(base: String, scope: Option<&str>) -> String {
    match scope {
        Some(s) => format!("{s}/{base}"),
        None => base,
    }
}

struct Pick {
    media_id: i64,
    dur_ms: u64,
    key: String,
    pos: u64,
}

struct Active<'l> {
    source: &'l Source,
    /// Set for events: gives them their own cursors.
    scope: Option<String>,
    label: String,
    /// For a "one episode" slot: when this showing of it began.
    once_from: Option<i64>,
}

fn resolve<'l>(c: &Compiled<'l>, t_ms: i64) -> Option<Active<'l>> {
    for e in &c.events {
        if e.active(&c.zone, t_ms) {
            return Some(Active {
                source: e.source,
                scope: Some(format!("event:{}", e.name.to_lowercase())),
                label: e.name.to_string(),
                once_from: None,
            });
        }
    }
    let local = c.zone.local(t_ms);
    let hit = |b: &CBlock<'l>| -> Option<Active<'l>> {
        b.active(local).then(|| Active {
            source: b.source,
            scope: None,
            label: b.name.to_string(),
            once_from: b.once.then(|| b.showing_start(&c.zone, local)),
        })
    };
    for s in c.seasons.iter().filter(|s| s.active(local)) {
        if let Some(a) = s.blocks.iter().find_map(hit) {
            return Some(Active {
                label: if a.label.is_empty() {
                    s.name.to_string()
                } else {
                    a.label
                },
                ..a
            });
        }
    }
    c.blocks.iter().find_map(hit)
}

fn default_fill(cfg: &SchedConfig, has_blocks: bool) -> Source {
    if let Some(f) = &cfg.lineup.fill {
        return f.clone();
    }
    // Hybrid: structure where there is a schedule, chaos for unscheduled gaps.
    if cfg.run_out == RunOutPolicy::Hybrid && has_blocks {
        return Source::Chaos;
    }
    match cfg.strategy {
        Strategy::Chaos => Source::Chaos,
        Strategy::Shuffle => Source::Pool {
            shows: vec![],
            order: Order::Shuffle,
        },
        Strategy::Sequential | Strategy::BlockGrid => Source::Pool {
            shows: vec![],
            order: Order::Sequential,
        },
    }
}

struct Gen<'a> {
    cfg: &'a SchedConfig<'a>,
    zone: Zone,
    cursors: &'a mut Cursors,
    perms: HashMap<(String, u64), Vec<usize>>,
}

impl Gen<'_> {
    fn wrap(&self) -> bool {
        matches!(
            self.cfg.run_out,
            RunOutPolicy::KeepGoing | RunOutPolicy::NextInRotation
        )
    }

    fn pick_show(&mut self, show: &str, scope: Option<&str>) -> Option<Pick> {
        let eps = &self.cfg.catalog.show(show)?.eps;
        let key = scoped(format!("show:{}", show.to_lowercase()), scope);
        let wrap = self.wrap();
        let cur = self.cursors.entry(key.clone()).or_default();
        let idx = cur.reserve_next(eps.len() as u64, wrap)? as usize;
        Some(Pick {
            media_id: eps[idx].id,
            dur_ms: eps[idx].dur_ms,
            key,
            pos: cur.reserved - 1,
        })
    }

    fn pick_pool(&mut self, shows: &[String], order: Order, scope: Option<&str>) -> Option<Pick> {
        let catalog = self.cfg.catalog;
        let keys: Vec<String> = if shows.is_empty() {
            catalog.show_keys()
        } else {
            shows
                .iter()
                .map(|s| s.to_lowercase())
                .filter(|k| catalog.by_key(k).is_some())
                .collect()
        };
        if keys.is_empty() {
            return None;
        }
        let ident = if shows.is_empty() {
            "all".to_string()
        } else {
            format!("{:016x}", fnv1a(&keys.join("|")))
        };
        let key = scoped(format!("pool:{order:?}:{ident}"), scope);
        let pos = {
            let cur = self.cursors.entry(key.clone()).or_default();
            cur.reserved += 1;
            cur.reserved - 1
        };
        let ep: Ep = match order {
            Order::Sequential => {
                let n = keys.len() as u64;
                let eps = &catalog.by_key(&keys[(pos % n) as usize])?.eps;
                eps[((pos / n) % eps.len() as u64) as usize]
            }
            Order::Shuffle => {
                let mut bag: Vec<Ep> = keys
                    .iter()
                    .filter_map(|k| catalog.by_key(k))
                    .flat_map(|s| s.eps.iter().copied())
                    .collect();
                bag.sort_by_key(|e| e.id);
                let len = bag.len() as u64;
                let pass = pos / len;
                let seed = self.cfg.seed ^ fnv1a(&key) ^ pass.wrapping_mul(GOLDEN);
                let perm = self
                    .perms
                    .entry((key.clone(), pass))
                    .or_insert_with(|| shuffled(bag.len(), seed));
                bag[perm[(pos % len) as usize]]
            }
        };
        Some(Pick {
            media_id: ep.id,
            dur_ms: ep.dur_ms,
            key,
            pos,
        })
    }

    /// Index of the active item in a `finale` rotation: one step per completed
    /// pass of any item, derived from the cursors so it survives re-rolls.
    fn finale_index(&self, items: &[Source], scope: Option<&str>) -> usize {
        let passes: u64 = items
            .iter()
            .filter_map(|i| match i {
                Source::Show { show } => {
                    let total = self.cfg.catalog.show(show)?.eps.len() as u64;
                    let key = scoped(format!("show:{}", show.to_lowercase()), scope);
                    Some(self.cursors.get(&key).map_or(0, |c| c.reserved / total))
                }
                _ => None,
            })
            .sum();
        (passes % items.len() as u64) as usize
    }

    fn pick(&mut self, src: &Source, scope: Option<&str>, t_ms: i64) -> Option<Pick> {
        match src {
            Source::Show { show } => self.pick_show(show, scope),
            Source::Pool { shows, order } => self.pick_pool(shows, *order, scope),
            Source::Chaos => self.pick_pool(&[], Order::Shuffle, Some("chaos")),
            Source::Rotation { items, advance } => {
                let idx = match advance {
                    Advance::Weeks(n) => {
                        let week = self.zone.week_index(self.cfg.epoch_ms, t_ms).max(0) as u64;
                        ((week / *n as u64) % items.len() as u64) as usize
                    }
                    Advance::Finale => self.finale_index(items, scope),
                };
                self.pick(&items[idx], scope, t_ms)
            }
        }
    }

    /// The fallback ladder: the slot's own source, then (for chaos-fill
    /// policies) chaos, then the channel's fill programming, then chaos.
    fn pick_with_fallback(
        &mut self,
        src: &Source,
        fill: &Source,
        scope: Option<&str>,
        t_ms: i64,
    ) -> Option<Pick> {
        if let Some(p) = self.pick(src, scope, t_ms) {
            return Some(p);
        }
        if self.cfg.run_out == RunOutPolicy::OffAir {
            // Only a fill you set yourself; otherwise the slot is left to the off-air screen.
            if let Some(f) = self.cfg.lineup.fill.as_ref().filter(|f| *f != src) {
                return self.pick(f, None, t_ms);
            }
            return None;
        }
        let chaos_first = matches!(
            self.cfg.run_out,
            RunOutPolicy::ChaosFill | RunOutPolicy::Hybrid
        );
        if chaos_first {
            if let Some(p) = self.pick(&Source::Chaos, None, t_ms) {
                return Some(p);
            }
        }
        if fill != src {
            if let Some(p) = self.pick(fill, None, t_ms) {
                return Some(p);
            }
        }
        self.pick(&Source::Chaos, None, t_ms)
    }
}

/// Start time to begin scheduling from: the grid slot containing `ms`
/// (or `ms` itself when the lineup has no grid).
/// Sign-off is scheduled in pieces this long, so edits take effect promptly.
const SIGNOFF_CHUNK_MS: i64 = 30 * 60_000;

pub fn align_start(lineup: &Lineup, ms: i64) -> i64 {
    let empty = Lineup::default();
    let c = Compiled::new(lineup).or_else(|_| Compiled::new(&empty));
    match c {
        Ok(c) => match c.grid_ms {
            Some(g) => c.zone.floor_grid(ms, g),
            None => ms,
        },
        Err(_) => ms,
    }
}

/// Generate items starting at `from_ms` until an item would start at or after
/// `to_ms`. `from_ms` should be grid-aligned (see [`align_start`]).
/// `cursors` is updated with the new reservations.
pub fn generate(
    cfg: &SchedConfig,
    cursors: &mut Cursors,
    from_ms: i64,
    to_ms: i64,
) -> Vec<ScheduledItem> {
    let empty = Lineup::default();
    let compiled = match Compiled::new(cfg.lineup) {
        Ok(c) => c,
        // A bad stored lineup must never take the channel off air.
        Err(_) => Compiled::new(&empty).expect("default lineup is valid"),
    };
    let has_blocks =
        !compiled.blocks.is_empty() || compiled.seasons.iter().any(|s| !s.blocks.is_empty());
    let fill = default_fill(cfg, has_blocks);
    let grid = compiled.grid_ms;
    let mut gen = Gen {
        cfg,
        zone: compiled.zone,
        cursors,
        perms: HashMap::new(),
    };

    let ads_cfg: Option<&AdConfig> = cfg.lineup.ads.as_ref();
    let default_inputs = AdInputs::default();
    let ad_inputs: &AdInputs = cfg.ads.unwrap_or(&default_inputs);
    let mut recent: VecDeque<i64> = ad_inputs.recent.iter().copied().collect();

    let mut out = Vec::new();
    let mut t = from_ms;
    while t < to_ms {
        // Off the air: bars and a tone until the sign-off window ends. It uses no
        // episode and moves no cursor, so what airs afterwards is undisturbed.
        if let Some(left) = compiled.signoff_left_ms(t) {
            let mut end = t + left.min(SIGNOFF_CHUNK_MS);
            // When the window ends, the next show starts on the grid.
            if left <= SIGNOFF_CHUNK_MS {
                if let Some(g) = grid {
                    end = end.max(compiled.zone.next_grid(end - 1, g));
                }
            }
            out.push(ScheduledItem {
                kind: ItemKind::Filler,
                media_id: None,
                start_ms: t,
                end_ms: end,
                cursor_key: None,
                cursor_pos: None,
                label: Some(crate::lineup::SIGNOFF_LABEL.to_string()),
                in_ms: 0,
                bumper: None,
            });
            t = end;
            continue;
        }
        let active = resolve(&compiled, t);
        let (src, scope, label) = match &active {
            Some(a) => (
                a.source,
                a.scope.clone(),
                Some(a.label.clone()).filter(|l| !l.is_empty()),
            ),
            None => (&fill, None, None),
        };

        let mut pick = None;
        // "Test screen" policy: time outside your time slots is the off-air screen, not more programmes.
        // A "one episode" slot airs only at its start; the rest of the slot is a gap.
        let spent = active
            .as_ref()
            .and_then(|a| a.once_from)
            .is_some_and(|from| t - from >= 60_000);
        let (src, scope, label) = if spent {
            (&fill, None, None)
        } else {
            (src, scope, label)
        };
        let leave_empty = (active.is_none() || spent)
            && cfg.run_out == RunOutPolicy::OffAir
            && cfg.lineup.fill.is_none()
            && !cfg.lineup.blocks.is_empty();
        if leave_empty {
            // nothing to pick: falls through to the filler below
        } else if scope.is_none() && cfg.chaos > 0 {
            let mut s = cfg.seed ^ (t as u64).wrapping_mul(GOLDEN) ^ 0xC4A0_5000;
            if splitmix64(&mut s) % 100 < cfg.chaos as u64 {
                pick = gen.pick(&Source::Chaos, None, t);
            }
        }
        if pick.is_none() && !leave_empty {
            pick = gen.pick_with_fallback(src, &fill, scope.as_deref(), t);
        }

        let Some(p) = pick else {
            // Nothing playable at all: hold the slot with filler.
            let end = match grid {
                Some(g) => compiled.zone.next_grid(t + 1, g),
                None => t + IDLE_GAP_MS,
            };
            out.push(ScheduledItem {
                kind: ItemKind::Filler,
                media_id: None,
                start_ms: t,
                end_ms: end,
                cursor_key: None,
                cursor_pos: None,
                label: Some(crate::lineup::OFF_AIR_LABEL.to_string()),
                in_ms: 0,
                bumper: None,
            });
            t = end;
            continue;
        };

        let end = t + p.dur_ms as i64;
        // With commercials on and no grid, the slot grows by the ad budget;
        // on a grid the ad budget is whatever is left of the slot.
        let slot_end = match (grid, ads_cfg) {
            (Some(g), _) => compiled.zone.next_grid(end, g),
            (None, Some(a)) => end + loose_budget_ms(a, &ad_inputs.pool, p.dur_ms) as i64,
            (None, None) => end,
        };
        if let Some(a) = ads_cfg {
            let plan = SlotPlan {
                start_ms: t,
                slot_end_ms: slot_end,
                media_id: p.media_id,
                runtime_ms: p.dur_ms,
                cursor_key: p.key,
                cursor_pos: p.pos,
                label,
            };
            out.extend(expand_slot(a, ad_inputs, &mut recent, cfg.seed, &plan));
        } else {
            out.push(ScheduledItem {
                kind: ItemKind::Show,
                media_id: Some(p.media_id),
                start_ms: t,
                end_ms: end,
                cursor_key: Some(p.key),
                cursor_pos: Some(p.pos),
                label: label.clone(),
                in_ms: 0,
                bumper: None,
            });
            if slot_end > end {
                out.push(ScheduledItem {
                    kind: ItemKind::Filler,
                    media_id: None,
                    start_ms: end,
                    end_ms: slot_end,
                    cursor_key: None,
                    cursor_pos: None,
                    label,
                    in_ms: 0,
                    bumper: None,
                });
            }
        }
        t = slot_end;
    }
    out
}

/// Mark everything in `items` that has started by `now_ms` as aired.
pub fn commit_aired(items: &[ScheduledItem], cursors: &mut Cursors, now_ms: i64) {
    for it in items.iter().filter(|i| i.start_ms <= now_ms) {
        if let (Some(k), Some(p)) = (&it.cursor_key, it.cursor_pos) {
            let c = cursors.entry(k.clone()).or_default();
            c.commit(p + 1);
            c.reserved = c.reserved.max(c.committed);
        }
    }
}

/// After deleting future items, restore reservations: each cursor goes back to
/// what has aired, plus whatever the kept items still hold.
pub fn rebuild_reservations(kept: &[ScheduledItem], cursors: &mut Cursors) {
    for c in cursors.values_mut() {
        c.reset_reservations();
    }
    for it in kept {
        if let (Some(k), Some(p)) = (&it.cursor_key, it.cursor_pos) {
            let c = cursors.entry(k.clone()).or_default();
            c.reserved = c.reserved.max(p + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::MediaRef;
    use crate::lineup::SignOff;

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;
    const EP: u64 = 22 * 60_000;

    fn at(zone: &str, s: &str) -> i64 {
        Zone::parse(zone)
            .unwrap()
            .to_utc_ms(chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap())
    }

    /// shows: (name, episodes). Ids are show_index*100 + episode_number.
    fn catalog(shows: &[(&str, u32)]) -> Catalog {
        let mut m = Vec::new();
        for (si, (name, n)) in shows.iter().enumerate() {
            for e in 1..=*n {
                m.push(MediaRef {
                    id: si as i64 * 100 + e as i64,
                    show: Some(name.to_string()),
                    season: Some(1),
                    episode: Some(e),
                    sort_key: format!("{name}{e}"),
                    duration_ms: EP,
                });
            }
        }
        Catalog::build(&m)
    }

    fn lineup(json: &str) -> Lineup {
        serde_json::from_str(json).unwrap()
    }

    struct Setup {
        catalog: Catalog,
        lineup: Lineup,
        strategy: Strategy,
        run_out: RunOutPolicy,
        chaos: u8,
        seed: u64,
        epoch: i64,
        ads: Option<AdInputs>,
    }

    impl Setup {
        fn new(catalog: Catalog, lineup: Lineup) -> Self {
            Self {
                catalog,
                lineup,
                strategy: Strategy::BlockGrid,
                run_out: RunOutPolicy::KeepGoing,
                chaos: 0,
                seed: 7,
                epoch: at("UTC", "2026-11-16 00:00"),
                ads: None,
            }
        }

        fn gen(&self, cursors: &mut Cursors, from: i64, to: i64) -> Vec<ScheduledItem> {
            let cfg = SchedConfig {
                ads: self.ads.as_ref(),
                catalog: &self.catalog,
                lineup: &self.lineup,
                strategy: self.strategy,
                run_out: self.run_out,
                chaos: self.chaos,
                seed: self.seed,
                epoch_ms: self.epoch,
            };
            generate(&cfg, cursors, from, to)
        }
    }

    fn shows(items: &[ScheduledItem]) -> Vec<&ScheduledItem> {
        items.iter().filter(|i| i.kind == ItemKind::Show).collect()
    }

    fn ids(items: &[ScheduledItem]) -> Vec<i64> {
        shows(items).iter().filter_map(|i| i.media_id).collect()
    }

    fn assert_contiguous(items: &[ScheduledItem]) {
        for w in items.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms, "gap or overlap: {w:?}");
        }
    }

    #[test]
    fn grid_aligns_starts_and_pads_with_filler() {
        let s = Setup::new(catalog(&[("A", 4)]), lineup(r#"{"grid_minutes":30}"#));
        let from = at("UTC", "2026-11-16 20:00");
        let items = s.gen(&mut Cursors::new(), from, from + 4 * HOUR);
        assert_contiguous(&items);
        // 22 min show + 8 min filler, repeating
        assert_eq!(items[0].kind, ItemKind::Show);
        assert_eq!(items[1].kind, ItemKind::Filler);
        assert_eq!(items[1].end_ms - items[1].start_ms, 8 * MIN);
        for it in shows(&items) {
            assert_eq!(it.start_ms % (30 * MIN), 0);
        }
    }

    #[test]
    fn loose_lineups_play_back_to_back_without_filler() {
        let mut s = Setup::new(catalog(&[("A", 3)]), Lineup::default());
        s.strategy = Strategy::Sequential;
        let from = at("UTC", "2026-11-16 20:07");
        let items = s.gen(&mut Cursors::new(), from, from + 3 * HOUR);
        assert_contiguous(&items);
        assert!(items.iter().all(|i| i.kind == ItemKind::Show));
        assert_eq!(items[0].start_ms, from);
    }

    #[test]
    fn blocks_follow_weekday_and_time() {
        let l = lineup(
            r#"{"blocks":[
              {"name":"Weeknights","days":[0,1,2,3,4],"start":"20:00","end":"21:00","source":{"type":"show","show":"Alpha"}},
              {"name":"Weekends","days":[5,6],"start":"20:00","end":"21:00","source":{"type":"show","show":"Beta"}}
            ]}"#,
        );
        let mut s = Setup::new(catalog(&[("Alpha", 10), ("Beta", 10)]), l);
        s.strategy = Strategy::BlockGrid;
        let mut cur = Cursors::new();
        // Monday 2026-11-16 20:00 -> Alpha (ids 1..), Saturday 2026-11-21 20:00 -> Beta (ids 101..)
        let mon = s.gen(
            &mut cur,
            at("UTC", "2026-11-16 20:00"),
            at("UTC", "2026-11-16 20:01"),
        );
        assert_eq!(mon[0].media_id, Some(1));
        assert_eq!(mon[0].label.as_deref(), Some("Weeknights"));
        let sat = s.gen(
            &mut cur,
            at("UTC", "2026-11-21 20:00"),
            at("UTC", "2026-11-21 20:01"),
        );
        assert_eq!(sat[0].media_id, Some(101));
    }

    #[test]
    fn dayparts_hand_over_at_block_boundaries() {
        let l = lineup(
            r#"{"blocks":[
              {"name":"Morning","start":"06:00","end":"12:00","source":{"type":"show","show":"Alpha"}},
              {"name":"Afternoon","start":"12:00","end":"18:00","source":{"type":"show","show":"Beta"}}
            ],"fill":{"type":"show","show":"Gamma"}}"#,
        );
        let s = Setup::new(catalog(&[("Alpha", 30), ("Beta", 30), ("Gamma", 30)]), l);
        let items = s.gen(
            &mut Cursors::new(),
            at("UTC", "2026-11-16 05:00"),
            at("UTC", "2026-11-16 19:00"),
        );
        assert_contiguous(&items);
        let label_at = |h: &str| {
            let t = at("UTC", &format!("2026-11-16 {h}"));
            items
                .iter()
                .find(|i| i.start_ms <= t && t < i.end_ms)
                .unwrap()
                .label
                .clone()
        };
        assert_eq!(label_at("05:30"), None); // fill
        assert_eq!(label_at("08:00").as_deref(), Some("Morning"));
        assert_eq!(label_at("13:00").as_deref(), Some("Afternoon"));
        assert_eq!(label_at("18:30"), None);
    }

    #[test]
    fn cursors_continue_across_days_and_wrap_after_the_finale() {
        let l = lineup(
            r#"{"blocks":[{"name":"Prime","start":"20:00","end":"20:30","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let s = Setup::new(catalog(&[("Alpha", 3)]), l);
        let mut cur = Cursors::new();
        let items = s.gen(
            &mut cur,
            at("UTC", "2026-11-16 20:00"),
            at("UTC", "2026-11-23 00:00"),
        );
        // 7 nights at 20:00, only the 20:00 block has Alpha; the rest is fill (also Alpha, sequential pool)
        let alpha: Vec<i64> = shows(&items)
            .iter()
            .filter(|i| i.label.is_some())
            .filter_map(|i| i.media_id)
            .collect();
        // episodes 1,2,3,1,2,3,1 in order
        assert_eq!(alpha, vec![1, 2, 3, 1, 2, 3, 1]);
    }

    #[test]
    fn reroll_does_not_burn_episodes() {
        let l = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let s = Setup::new(catalog(&[("Alpha", 50)]), l);
        let start = at("UTC", "2026-11-16 00:00");
        let mut cur = Cursors::new();
        let first = s.gen(&mut cur, start, start + 2 * DAY);
        let original = ids(&first);
        assert_eq!(&original[..4], &[1, 2, 3, 4]);

        // 3 hours in: 6 items have started. Keep items starting before now+30min.
        let now = start + 3 * HOUR;
        commit_aired(&first, &mut cur, now);
        let cut = now + 30 * MIN;
        let kept: Vec<ScheduledItem> = first.iter().filter(|i| i.start_ms < cut).cloned().collect();
        rebuild_reservations(&kept, &mut cur);
        let resume = kept.last().unwrap().end_ms;
        let again = s.gen(&mut cur, resume, start + 2 * DAY);
        let rebuilt: Vec<i64> = ids(&kept).into_iter().chain(ids(&again)).collect();
        assert_eq!(
            rebuilt, original,
            "re-rolling must not skip or repeat episodes"
        );
    }

    #[test]
    fn rerolling_with_changed_lineup_resumes_from_what_aired() {
        let l = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("Alpha", 50)]), l);
        let start = at("UTC", "2026-11-16 00:00");
        let mut cur = Cursors::new();
        let first = s.gen(&mut cur, start, start + DAY);
        let now = start + 2 * HOUR;
        commit_aired(&first, &mut cur, now); // 5 items started (0,30,60,90,120 min)
                                             // edit: everything after now becomes a different show; Alpha's cursor must stay at the aired position
        let kept: Vec<ScheduledItem> = first
            .iter()
            .filter(|i| i.start_ms <= now)
            .cloned()
            .collect();
        rebuild_reservations(&kept, &mut cur);
        s.lineup = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let again = s.gen(&mut cur, kept.last().unwrap().end_ms, start + DAY);
        assert_eq!(ids(&again)[0], 6, "next episode after the 5 that aired");
    }

    #[test]
    fn run_out_keep_going_wraps_but_chaos_fill_switches_to_chaos() {
        let l = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"Short"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("Short", 2), ("Other", 5)]), l);
        let start = at("UTC", "2026-11-16 00:00");

        s.run_out = RunOutPolicy::KeepGoing;
        let kept = s.gen(&mut Cursors::new(), start, start + 4 * HOUR);
        assert_eq!(&ids(&kept)[..5], &[1, 2, 1, 2, 1]);

        s.run_out = RunOutPolicy::ChaosFill;
        let chaos = s.gen(&mut Cursors::new(), start, start + 4 * HOUR);
        let got = ids(&chaos);
        assert_eq!(&got[..2], &[1, 2]);
        // After the finale the slot is chaos: it reaches the other show and no
        // longer just wraps 1,2,1,2.
        assert!(got[2..].iter().any(|id| *id >= 101), "{got:?}");
        assert_ne!(&got[..4], &[1, 2, 1, 2], "{got:?}");
        assert!(got.len() > 4);
    }

    #[test]
    fn the_test_screen_policy_leaves_gaps_and_finished_shows_to_the_off_air_screen() {
        // Alpha has 2 episodes and is scheduled 20:00-23:00 (about six slots' worth); Beta exists
        // but is not in the lineup. Outside the slot, and once Alpha has run out, nothing else airs.
        let l = lineup(
            r#"{"blocks":[{"start":"20:00","end":"23:00","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("Alpha", 2), ("Beta", 20)]), l);
        s.run_out = RunOutPolicy::OffAir;
        let items = s.gen(
            &mut Cursors::new(),
            at("UTC", "2026-11-16 18:00"),
            at("UTC", "2026-11-17 01:00"),
        );
        let aired: Vec<i64> = shows(&items).iter().filter_map(|i| i.media_id).collect();
        assert_eq!(
            aired.len(),
            2,
            "each Alpha episode once, nothing else: {aired:?}"
        );
        assert!(aired.iter().all(|id| *id < 100), "no Beta, no chaos");
        // everything else is filler with no media: the off-air screen
        assert!(items
            .iter()
            .filter(|i| i.kind != ItemKind::Show)
            .all(|i| i.media_id.is_none()));
        let covered: i64 = items.iter().map(|i| i.end_ms - i.start_ms).sum();
        assert!(
            covered >= at("UTC", "2026-11-17 01:00") - at("UTC", "2026-11-16 18:00"),
            "the whole time is covered"
        );
        // The same channel with the default policy fills the gaps with programmes.
        let mut d = Setup::new(
            catalog(&[("Alpha", 2), ("Beta", 20)]),
            lineup(
                r#"{"blocks":[{"start":"20:00","end":"23:00","source":{"type":"show","show":"Alpha"}}]}"#,
            ),
        );
        d.run_out = RunOutPolicy::KeepGoing;
        let kept = d.gen(
            &mut Cursors::new(),
            at("UTC", "2026-11-16 18:00"),
            at("UTC", "2026-11-17 01:00"),
        );
        assert!(shows(&kept).len() > aired.len());
    }

    #[test]
    fn a_one_episode_slot_airs_once_then_the_gap_policy_takes_over() {
        let l = lineup(
            r#"{"blocks":[{"start":"15:30","end":"19:00","once":true,"source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("Alpha", 40), ("Beta", 20)]), l);
        s.run_out = RunOutPolicy::OffAir;
        let items = s.gen(
            &mut Cursors::new(),
            at("UTC", "2026-11-16 15:30"),
            at("UTC", "2026-11-18 00:00"),
        );
        // one episode on each of the two afternoons, nothing between
        let aired = shows(&items);
        assert_eq!(
            aired.len(),
            2,
            "{:?}",
            aired.iter().map(|i| i.start_ms).collect::<Vec<_>>()
        );
        assert_eq!(aired[0].start_ms, at("UTC", "2026-11-16 15:30"));
        assert_eq!(aired[1].start_ms, at("UTC", "2026-11-17 15:30"));
        assert!(items
            .iter()
            .filter(|i| i.kind != ItemKind::Show)
            .all(|i| i.media_id.is_none()));
        // with the ordinary policy the slot fills with the channel's fill programming instead
        let mut k = Setup::new(
            catalog(&[("Alpha", 40), ("Beta", 20)]),
            lineup(
                r#"{"blocks":[{"start":"15:30","end":"19:00","once":true,"source":{"type":"show","show":"Alpha"}}]}"#,
            ),
        );
        k.run_out = RunOutPolicy::KeepGoing;
        let kept = k.gen(
            &mut Cursors::new(),
            at("UTC", "2026-11-16 15:30"),
            at("UTC", "2026-11-16 19:00"),
        );
        assert!(shows(&kept).len() > 1);
    }

    #[test]
    fn a_fill_you_set_yourself_still_airs_under_the_test_screen_policy() {
        let l = lineup(
            r#"{"blocks":[{"start":"20:00","end":"21:00","source":{"type":"show","show":"Alpha"}}],"fill":{"type":"show","show":"Beta"}}"#,
        );
        let mut s = Setup::new(catalog(&[("Alpha", 20), ("Beta", 20)]), l);
        s.run_out = RunOutPolicy::OffAir;
        let items = s.gen(
            &mut Cursors::new(),
            at("UTC", "2026-11-16 18:00"),
            at("UTC", "2026-11-16 23:00"),
        );
        assert!(
            shows(&items).iter().any(|i| i.media_id.unwrap() >= 100),
            "Beta fills the gaps you asked it to"
        );
    }

    #[test]
    fn hybrid_uses_chaos_only_for_unscheduled_gaps() {
        let l = lineup(
            r#"{"blocks":[{"start":"20:00","end":"21:00","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("Alpha", 20), ("Beta", 20)]), l);
        s.run_out = RunOutPolicy::Hybrid;
        let items = s.gen(
            &mut Cursors::new(),
            at("UTC", "2026-11-16 18:00"),
            at("UTC", "2026-11-16 23:00"),
        );
        for it in shows(&items) {
            let in_block = it.label.is_some();
            let alpha = it.media_id.unwrap() < 100;
            if in_block {
                assert!(alpha, "block airs Alpha");
            }
        }
        // gap time (before 20:00) is chaos: uses both shows over enough picks
        let gap: Vec<i64> = shows(&items)
            .iter()
            .filter(|i| i.label.is_none() && i.start_ms < at("UTC", "2026-11-16 20:00"))
            .filter_map(|i| i.media_id)
            .collect();
        assert!(gap.iter().any(|id| *id >= 100) && gap.iter().any(|id| *id < 100));
    }

    #[test]
    fn rotation_by_weeks_introduces_a_new_show() {
        let l = lineup(
            r#"{"blocks":[{"name":"Prime","start":"20:00","end":"20:30","source":
              {"type":"rotation","advance":{"weeks":2},"items":[{"type":"show","show":"Old"},{"type":"show","show":"New"}]}}]}"#,
        );
        let s = Setup::new(catalog(&[("Old", 30), ("New", 30)]), l);
        // epoch is Monday 2026-11-16; weeks 0-1 -> Old, 2-3 -> New, 4-5 -> Old
        let which = |day: &str| {
            let t = at("UTC", &format!("{day} 20:00"));
            let items = s.gen(&mut Cursors::new(), t, t + 1);
            if items[0].media_id.unwrap() < 100 {
                "Old"
            } else {
                "New"
            }
        };
        assert_eq!(which("2026-11-16"), "Old");
        assert_eq!(which("2026-11-29"), "Old"); // end of week 1
        assert_eq!(which("2026-11-30"), "New"); // week 2
        assert_eq!(which("2026-12-13"), "New"); // end of week 3
        assert_eq!(which("2026-12-14"), "Old"); // week 4
    }

    #[test]
    fn finale_rotation_moves_on_after_each_full_pass() {
        let l = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":
              {"type":"rotation","advance":"finale","items":[{"type":"show","show":"A"},{"type":"show","show":"B"}]}}]}"#,
        );
        let s = Setup::new(catalog(&[("A", 2), ("B", 2)]), l);
        let start = at("UTC", "2026-11-16 00:00");
        let items = s.gen(&mut Cursors::new(), start, start + 5 * HOUR);
        // A (ids 1,2) then B (101,102) then A again
        assert_eq!(&ids(&items)[..8], &[1, 2, 101, 102, 1, 2, 101, 102]);
    }

    #[test]
    fn seasons_override_base_blocks_while_active() {
        let l = lineup(
            r#"{"blocks":[{"start":"20:00","end":"21:00","source":{"type":"show","show":"Rerun"}}],
              "seasons":[{"name":"Fall","from":"09-01","to":"05-31","blocks":[
                 {"start":"20:00","end":"21:00","source":{"type":"show","show":"Premiere"}}]}]}"#,
        );
        let s = Setup::new(catalog(&[("Rerun", 9), ("Premiere", 9)]), l);
        let first = |day: &str| {
            let t = at("UTC", &format!("{day} 20:00"));
            s.gen(&mut Cursors::new(), t, t + 1)[0].media_id.unwrap()
        };
        assert!(first("2026-11-16") >= 100, "fall: Premiere");
        assert!(first("2027-02-02") >= 100, "year wrap still fall");
        assert!(first("2027-07-12") < 100, "summer: base lineup");
    }

    #[test]
    fn events_override_everything_and_keep_their_own_cursor() {
        let l = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}],
              "events":[{"name":"Beta Marathon","start":"2026-11-16T10:00","end":"2026-11-16T12:00","source":{"type":"show","show":"Beta"}}]}"#,
        );
        let s = Setup::new(catalog(&[("Alpha", 100), ("Beta", 100)]), l);
        let mut cur = Cursors::new();
        let from = at("UTC", "2026-11-16 08:00");
        let items = s.gen(&mut cur, from, at("UTC", "2026-11-16 14:00"));
        let at_t = |h: &str| {
            let t = at("UTC", &format!("2026-11-16 {h}"));
            items
                .iter()
                .find(|i| i.start_ms <= t && t < i.end_ms && i.kind == ItemKind::Show)
                .unwrap()
        };
        assert!(at_t("09:00").media_id.unwrap() < 100);
        let marathon = at_t("10:30");
        assert!(marathon.media_id.unwrap() >= 100);
        assert_eq!(marathon.label.as_deref(), Some("Beta Marathon"));
        assert!(at_t("12:30").media_id.unwrap() < 100);
        // Alpha continues where it left off after the event (no episodes skipped)
        let alpha: Vec<i64> = ids(&items).into_iter().filter(|i| *i < 100).collect();
        for w in alpha.windows(2) {
            assert_eq!(w[1], w[0] + 1);
        }
        // the event's Beta cursor is separate from any regular Beta cursor
        assert!(cur.keys().any(|k| k.starts_with("event:beta marathon/")));
        assert!(!cur.contains_key("show:beta"));
        // marathon begins at episode 1 of Beta
        assert_eq!(
            shows(&items)
                .iter()
                .find(|i| i.label.as_deref() == Some("Beta Marathon"))
                .unwrap()
                .media_id,
            Some(101)
        );
    }

    #[test]
    fn yearly_events_recur() {
        let l = lineup(
            r#"{"events":[{"name":"Xmas","start":"2026-12-25T08:00","end":"2026-12-25T20:00","yearly":true,"source":{"type":"show","show":"Special"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("Special", 5), ("Alpha", 5)]), l);
        s.strategy = Strategy::Sequential;
        for day in ["2026-12-25", "2027-12-25", "2030-12-25"] {
            let t = at("UTC", &format!("{day} 12:00"));
            let it = s.gen(&mut Cursors::new(), t, t + 1);
            assert_eq!(it[0].label.as_deref(), Some("Xmas"), "{day}");
        }
        let t = at("UTC", "2027-12-26 12:00");
        assert_eq!(s.gen(&mut Cursors::new(), t, t + 1)[0].label, None);
    }

    #[test]
    fn times_follow_local_clock_across_daylight_saving() {
        let l = lineup(
            r#"{"timezone":"America/Toronto","blocks":[{"name":"Prime","start":"20:00","end":"21:00","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let s = Setup::new(catalog(&[("Alpha", 100)]), l);
        // DST ends Sunday 2026-11-01 in Toronto. 20:00 local must hold on both sides.
        let from = at("America/Toronto", "2026-10-30 12:00");
        let items = s.gen(&mut Cursors::new(), from, from + 4 * DAY);
        let z = Zone::parse("America/Toronto").unwrap();
        let blocks: Vec<&ScheduledItem> = items.iter().filter(|i| i.label.is_some()).collect();
        assert!(blocks.len() >= 3);
        // the first item of each night's block starts at 20:00 local
        let mut nights = std::collections::BTreeMap::new();
        for b in &blocks {
            nights
                .entry(z.local(b.start_ms).date())
                .or_insert(b.start_ms);
        }
        assert!(nights.len() >= 3);
        for (_, start) in nights {
            let l = z.local(start);
            assert_eq!(
                (chrono::Timelike::hour(&l), chrono::Timelike::minute(&l)),
                (20, 0)
            );
        }
    }

    #[test]
    fn chaos_slider_mixes_in_random_picks_deterministically() {
        let l = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("Alpha", 200), ("Beta", 200), ("Gamma", 200)]), l);
        let start = at("UTC", "2026-11-16 00:00");

        s.chaos = 0;
        let none = ids(&s.gen(&mut Cursors::new(), start, start + 2 * DAY));
        assert!(none.iter().all(|i| *i < 100), "chaos 0 is pure schedule");

        s.chaos = 100;
        let all = ids(&s.gen(&mut Cursors::new(), start, start + 2 * DAY));
        let from_alpha = all.iter().filter(|i| **i < 100).count();
        assert!(from_alpha < all.len(), "chaos 100 leaves the schedule");
        assert!(all.iter().any(|i| *i >= 200), "reaches every show");

        s.chaos = 30;
        let mix = ids(&s.gen(&mut Cursors::new(), start, start + 2 * DAY));
        let share = mix.iter().filter(|i| **i >= 100).count() as f64 / mix.len() as f64;
        assert!(
            (0.1..=0.35).contains(&share),
            "30% of picks are random and two thirds of those leave Alpha: {share}"
        );
        let again = ids(&s.gen(&mut Cursors::new(), start, start + 2 * DAY));
        assert_eq!(mix, again, "same seed, same schedule");
        s.seed = 8;
        let other = ids(&s.gen(&mut Cursors::new(), start, start + 2 * DAY));
        assert_ne!(mix, other, "different seed, different chaos");
    }

    #[test]
    fn shuffle_pool_shows_every_episode_once_per_pass() {
        let mut s = Setup::new(catalog(&[("A", 4), ("B", 3)]), Lineup::default());
        s.strategy = Strategy::Shuffle;
        let start = at("UTC", "2026-11-16 00:00");
        let items = ids(&s.gen(&mut Cursors::new(), start, start + 14 * 22 * MIN + 1));
        for pass in items.chunks(7).take(2) {
            let mut p = pass.to_vec();
            p.sort();
            assert_eq!(p, vec![1, 2, 3, 4, 101, 102, 103]);
        }
    }

    #[test]
    fn missing_show_falls_back_and_empty_catalog_yields_filler() {
        let l = lineup(
            r#"{"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"Deleted"}}]}"#,
        );
        let s = Setup::new(catalog(&[("Alpha", 5)]), l.clone());
        let start = at("UTC", "2026-11-16 00:00");
        let items = s.gen(&mut Cursors::new(), start, start + 2 * HOUR);
        assert!(
            ids(&items).iter().all(|i| *i < 100),
            "ladder: fill/chaos covers the missing show"
        );
        assert!(!ids(&items).is_empty());

        let empty = Setup::new(Catalog::default(), l);
        let items = empty.gen(&mut Cursors::new(), start, start + 2 * HOUR);
        assert_contiguous(&items);
        assert!(items.iter().all(|i| i.kind == ItemKind::Filler));
    }

    #[test]
    fn invalid_stored_lineup_still_produces_a_schedule() {
        let l = Lineup {
            timezone: "Not/AZone".into(),
            ..Default::default()
        };
        let mut s = Setup::new(catalog(&[("Alpha", 5)]), l);
        s.strategy = Strategy::Sequential;
        let start = at("UTC", "2026-11-16 00:00");
        let items = s.gen(&mut Cursors::new(), start, start + HOUR);
        assert!(!ids(&items).is_empty());
    }

    #[test]
    fn two_weeks_of_everything_stays_consistent() {
        let l = lineup(
            r#"{"timezone":"America/Toronto","grid_minutes":30,
              "blocks":[
                {"name":"Cartoons","days":[0,1,2,3,4],"start":"06:00","end":"09:00","source":{"type":"pool","shows":["Toon"],"order":"shuffle"}},
                {"name":"Prime","start":"20:00","end":"23:00","source":{"type":"rotation","advance":{"weeks":1},"items":[{"type":"show","show":"Drama"},{"type":"show","show":"Comedy"}]}},
                {"name":"Overnight","start":"23:00","end":"06:00","source":{"type":"chaos"}}],
              "seasons":[{"name":"Summer","from":"06-01","to":"08-31","blocks":[{"start":"20:00","end":"23:00","source":{"type":"show","show":"Rerun"}}]}],
              "events":[{"name":"Sat Marathon","start":"2026-11-21T10:00","end":"2026-11-21T16:00","source":{"type":"show","show":"Comedy"}}]}"#,
        );
        let mut s = Setup::new(
            catalog(&[("Toon", 12), ("Drama", 10), ("Comedy", 10), ("Rerun", 4)]),
            l,
        );
        s.chaos = 15;
        s.run_out = RunOutPolicy::Hybrid;
        let from = at("America/Toronto", "2026-11-16 00:00");
        let mut cur = Cursors::new();
        let items = s.gen(&mut cur, from, from + 14 * DAY);
        assert_contiguous(&items);
        assert!(items.len() > 14 * 40);
        let valid: std::collections::HashSet<i64> = (0..4i64)
            .flat_map(|si| (1..=12).map(move |e| si * 100 + e))
            .collect();
        for it in &items {
            assert!(it.end_ms > it.start_ms);
            if let Some(id) = it.media_id {
                assert!(valid.contains(&id));
            }
        }
        // cursors only ever point inside what was scheduled
        for c in cur.values() {
            assert!(c.reserved >= c.committed);
        }
        // deterministic
        let again = s.gen(&mut Cursors::new(), from, from + 14 * DAY);
        assert_eq!(items, again);
    }

    fn ad_pool(n: i64) -> AdInputs {
        use crate::ads::{AdClip, AdPool};
        AdInputs {
            pool: AdPool {
                ads: (0..n)
                    .map(|i| AdClip {
                        id: 1000 + i,
                        dur_ms: [15_000, 30_000, 30_000, 45_000, 60_000][(i % 5) as usize],
                        year: None,
                    })
                    .collect(),
                clips: vec![],
            },
            ..Default::default()
        }
    }

    #[test]
    fn ads_fill_every_grid_slot_exactly_and_only_the_first_act_carries_the_cursor() {
        let l = lineup(
            r#"{"grid_minutes":30,"ads":{},"blocks":[{"name":"Prime","start":"00:00","end":"24:00","source":{"type":"show","show":"A"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("A", 10)]), l);
        s.ads = Some(ad_pool(40));
        let from = at("UTC", "2026-11-16 00:00");
        let mut cur = Cursors::new();
        let items = s.gen(&mut cur, from, from + 2 * DAY);
        assert_contiguous(&items);
        // 30-minute slots, each starting on the grid with a show's first act
        let firsts: Vec<&ScheduledItem> = items
            .iter()
            .filter(|i| i.kind == ItemKind::Show && i.cursor_pos.is_some())
            .collect();
        assert_eq!(firsts.len(), 96);
        for f in &firsts {
            assert_eq!(f.start_ms % (30 * MIN), 0);
            assert_eq!(f.in_ms, 0);
        }
        // cursor positions are consecutive: acts do not advance the episode counter
        for (i, f) in firsts.iter().enumerate() {
            assert_eq!(f.cursor_pos, Some(i as u64));
        }
        assert_eq!(cur["show:a"].reserved, 96);
        assert!(items.iter().any(|i| i.kind == ItemKind::Commercial));
        assert!(items.iter().any(|i| i.bumper.is_some()));
        // deterministic
        assert_eq!(items, s.gen(&mut Cursors::new(), from, from + 2 * DAY));
    }

    #[test]
    fn ads_without_a_grid_extend_the_slot_by_minutes_per_hour() {
        let l = lineup(r#"{"ads":{"minutes_per_hour":8}}"#);
        let mut s = Setup::new(catalog(&[("A", 6)]), l);
        s.strategy = Strategy::Sequential;
        s.ads = Some(ad_pool(30));
        let from = at("UTC", "2026-11-16 00:00");
        let items = s.gen(&mut Cursors::new(), from, from + 6 * HOUR);
        assert_contiguous(&items);
        let (mut show, mut other) = (0i64, 0i64);
        for i in &items {
            if i.kind == ItemKind::Show {
                show += i.end_ms - i.start_ms;
            } else {
                other += i.end_ms - i.start_ms;
            }
        }
        // 22-minute episodes get 8/60 of their length as ad time: about 2m56s per slot
        let ratio = other as f64 / show as f64;
        assert!((ratio - 8.0 / 60.0).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn ads_survive_rerolls_without_burning_episodes() {
        let l = lineup(
            r#"{"grid_minutes":30,"ads":{},"blocks":[{"start":"00:00","end":"24:00","source":{"type":"show","show":"A"}}]}"#,
        );
        let mut s = Setup::new(catalog(&[("A", 50)]), l);
        s.ads = Some(ad_pool(40));
        let start = at("UTC", "2026-11-16 00:00");
        let mut cur = Cursors::new();
        let first = s.gen(&mut cur, start, start + DAY);
        let now = start + 3 * HOUR + 10 * MIN;
        commit_aired(&first, &mut cur, now);
        // cut at the first show start after a 5 minute lock, as the server does
        let lock = now + 5 * MIN;
        let cut = first
            .iter()
            .find(|i| i.kind == ItemKind::Show && i.cursor_pos.is_some() && i.start_ms >= lock)
            .unwrap()
            .start_ms;
        let kept: Vec<ScheduledItem> = first.iter().filter(|i| i.start_ms < cut).cloned().collect();
        rebuild_reservations(&kept, &mut cur);
        let again = s.gen(&mut cur, kept.last().unwrap().end_ms, start + DAY);
        let rebuilt: Vec<ScheduledItem> = kept.into_iter().chain(again).collect();
        let episodes = |v: &[ScheduledItem]| -> Vec<i64> {
            v.iter()
                .filter(|i| i.kind == ItemKind::Show && i.cursor_pos.is_some())
                .filter_map(|i| i.media_id)
                .collect()
        };
        assert_eq!(episodes(&rebuilt), episodes(&first));
        assert_contiguous(&rebuilt);
    }

    #[test]
    fn commit_and_rebuild_reservations() {
        let items = vec![
            ScheduledItem {
                kind: ItemKind::Show,
                media_id: Some(1),
                start_ms: 0,
                end_ms: 10,
                cursor_key: Some("k".into()),
                cursor_pos: Some(0),
                label: None,
                in_ms: 0,
                bumper: None,
            },
            ScheduledItem {
                kind: ItemKind::Show,
                media_id: Some(2),
                start_ms: 10,
                end_ms: 20,
                cursor_key: Some("k".into()),
                cursor_pos: Some(1),
                label: None,
                in_ms: 0,
                bumper: None,
            },
            ScheduledItem {
                kind: ItemKind::Show,
                media_id: Some(3),
                start_ms: 20,
                end_ms: 30,
                cursor_key: Some("k".into()),
                cursor_pos: Some(2),
                label: None,
                in_ms: 0,
                bumper: None,
            },
        ];
        let mut cur = Cursors::new();
        cur.insert(
            "k".into(),
            EpisodeCursor {
                reserved: 3,
                committed: 0,
            },
        );
        commit_aired(&items, &mut cur, 12);
        assert_eq!(cur["k"].committed, 2);
        // drop the last item, keep two
        rebuild_reservations(&items[..2], &mut cur);
        assert_eq!(cur["k"].reserved, 2);
        // drop everything not aired
        rebuild_reservations(&[], &mut cur);
        assert_eq!(cur["k"].reserved, 2, "never below committed");
    }

    #[test]
    fn the_station_signs_off_for_the_night_and_the_show_after_it_is_undisturbed() {
        let l = lineup(
            r#"{"timezone":"America/Toronto","grid_minutes":30,"signoff":{"from":"02:00","to":"05:00","style":"bars"},
            "blocks":[{"name":"All","start":"00:00","end":"24:00","source":{"type":"show","show":"A"}}]}"#,
        );
        let s = Setup::new(catalog(&[("A", 400)]), l);
        let from = at("America/Toronto", "2026-11-16 00:00");
        let mut cursors = Cursors::new();
        let items = s.gen(&mut cursors, from, from + DAY);
        assert_contiguous(&items);

        let win_start = at("America/Toronto", "2026-11-16 02:00");
        let win_end = at("America/Toronto", "2026-11-16 05:00");
        let off: Vec<&ScheduledItem> = items
            .iter()
            .filter(|i| i.label.as_deref() == Some("Sign-off"))
            .collect();
        assert!(!off.is_empty());
        assert!(off
            .iter()
            .all(|i| i.kind == ItemKind::Filler && i.media_id.is_none() && i.cursor_key.is_none()));
        assert!(off
            .iter()
            .all(|i| i.end_ms - i.start_ms <= SIGNOFF_CHUNK_MS));
        assert_eq!(
            off.first().unwrap().start_ms,
            win_start,
            "the 01:30 show ends at 02:00 on the grid, then bars"
        );
        assert_eq!(off.last().unwrap().end_ms, win_end);
        // nothing else airs inside the window
        assert!(items
            .iter()
            .filter(|i| i.start_ms >= win_start && i.start_ms < win_end)
            .all(|i| i.label.as_deref() == Some("Sign-off")));
        // the first show after it starts on the grid at 05:00, and episodes continue in order across the gap
        let after = items
            .iter()
            .find(|i| i.kind == ItemKind::Show && i.start_ms >= win_end)
            .unwrap();
        assert_eq!(after.start_ms, win_end);
        let eps = ids(&items);
        assert!(
            eps.windows(2).all(|w| w[1] == w[0] + 1),
            "no episode was burned during sign-off: {eps:?}"
        );
    }

    #[test]
    fn a_signoff_window_can_run_through_midnight_and_must_have_a_length() {
        let l = lineup(r#"{"timezone":"UTC","signoff":{"from":"23:30","to":"05:00"}}"#);
        let c = Compiled::new(&l).unwrap();
        let t = |s| at("UTC", s);
        assert!(c.signoff_left_ms(t("2026-11-16 23:45")).is_some());
        assert!(c.signoff_left_ms(t("2026-11-17 01:00")).is_some());
        assert_eq!(c.signoff_left_ms(t("2026-11-17 04:00")), Some(HOUR));
        assert!(c.signoff_left_ms(t("2026-11-17 12:00")).is_none());
        assert!(Lineup {
            signoff: Some(SignOff {
                from: "03:00".into(),
                to: "03:00".into(),
                style: Default::default()
            }),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Lineup {
            signoff: Some(SignOff {
                from: "nope".into(),
                to: "03:00".into(),
                style: Default::default()
            }),
            ..Default::default()
        }
        .validate()
        .is_err());
        // "24:00" is a fine end
        assert!(Lineup {
            signoff: Some(SignOff {
                from: "23:00".into(),
                to: "24:00".into(),
                style: Default::default()
            }),
            ..Default::default()
        }
        .validate()
        .is_ok());
    }
}
