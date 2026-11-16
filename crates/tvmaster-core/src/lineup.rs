//! The programming description for a channel: a weekly block grid, seasonal
//! lineups, one-off or yearly events, and what to air when nothing is
//! scheduled. Stored as JSON; compiled (and thereby validated) before use.

use crate::ads::AdConfig;
use crate::clock::Zone;
use chrono::{Datelike, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Round-robin across shows, episodes in order.
    #[default]
    Sequential,
    /// A seeded shuffle bag: every episode once per pass, in random order.
    Shuffle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Advance {
    /// Swap to the next item every N weeks.
    Weeks(u32),
    /// Swap to the next item when the current show finishes a full pass.
    Finale,
}

/// Where a slot's programming comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// One show's episodes in order (the cursor remembers where it left off).
    Show { show: String },
    /// Several shows (empty list = every show).
    Pool {
        #[serde(default)]
        shows: Vec<String>,
        #[serde(default)]
        order: Order,
    },
    /// The slot's programming changes over time: this is how a new show gets
    /// introduced, a lineup rotates, or a show gets retired.
    Rotation {
        items: Vec<Source>,
        advance: Advance,
    },
    /// Anything, random.
    Chaos,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    #[serde(default)]
    pub name: String,
    /// 0 = Monday ... 6 = Sunday. Empty = every day.
    #[serde(default)]
    pub days: Vec<u8>,
    /// Local "HH:MM".
    pub start: String,
    /// Local "HH:MM" ("24:00" allowed). An end before the start runs overnight.
    pub end: String,
    pub source: Source,
    /// Air one episode at the start time, then leave the rest of the slot to the
    /// gap policy (the test screen, say) instead of running episodes back to back.
    #[serde(default, skip_serializing_if = "is_false")]
    pub once: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Season {
    pub name: String,
    /// "MM-DD", inclusive.
    pub from: String,
    /// "MM-DD", inclusive. A range that ends before it starts wraps the year.
    pub to: String,
    /// While the season is active these replace the base blocks.
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Unique. Events keep their own episode cursors, so a marathon starts at
    /// the first episode and does not disturb the regular rotation.
    pub name: String,
    /// Local "YYYY-MM-DDTHH:MM".
    pub start: String,
    pub end: String,
    /// Repeat every year (holiday specials).
    #[serde(default)]
    pub yearly: bool,
    pub source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatchUp {
    /// After downtime, carry on from where the channel left off.
    #[default]
    Resume,
    /// After downtime, act as if the channel kept airing (episodes advance).
    Advance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lineup {
    /// IANA name, e.g. "America/Toronto". Blank means the server fills in its
    /// own time zone when the lineup is saved (UTC if none).
    #[serde(default)]
    pub timezone: String,
    /// Start times snap to this many minutes (5, 10, 15, 20, 30 or 60). Unset:
    /// 30 when the lineup has blocks, otherwise items play back to back.
    #[serde(default)]
    pub grid_minutes: Option<u32>,
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub seasons: Vec<Season>,
    #[serde(default)]
    pub events: Vec<Event>,
    /// Programming for unscheduled time. Unset: derived from the channel's strategy.
    #[serde(default)]
    pub fill: Option<Source>,
    #[serde(default)]
    pub catch_up: CatchUp,
    /// Commercial breaks and bumpers. Absent = shows run clean with plain padding.
    #[serde(default)]
    pub ads: Option<AdConfig>,
    /// Hours the station goes off the air (bars and a tone, or snow).
    #[serde(default)]
    pub signoff: Option<SignOff>,
}

/// What airs while the station is signed off: the same screens as when nothing is on air.
pub type SignOffStyle = crate::options::Screen;

fn signoff_screen() -> SignOffStyle {
    SignOffStyle::Bars
}

/// Off the air from `from` to `to` every day (local time); a `to` before `from`
/// runs overnight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignOff {
    pub from: String,
    pub to: String,
    #[serde(default = "signoff_screen")]
    pub style: SignOffStyle,
}

/// The label sign-off items carry in the schedule.
pub const SIGNOFF_LABEL: &str = "Sign-off";
/// Label on time the channel spends on its off-air screen because nothing is scheduled or a show ran out.
pub const OFF_AIR_LABEL: &str = "Off air";

impl Default for Lineup {
    fn default() -> Self {
        Self {
            timezone: String::new(),
            grid_minutes: None,
            blocks: Vec::new(),
            seasons: Vec::new(),
            events: Vec::new(),
            fill: None,
            catch_up: CatchUp::Resume,
            ads: None,
            signoff: None,
        }
    }
}

const GRIDS: &[u32] = &[5, 10, 15, 20, 30, 60];

pub(crate) struct CBlock<'a> {
    pub name: &'a str,
    days: [bool; 7],
    start: u32,
    end: u32,
    pub once: bool,
    pub source: &'a Source,
}

impl CBlock<'_> {
    /// UTC start of the showing of this slot that contains `local` (call only while it is active).
    pub fn showing_start(&self, zone: &Zone, local: NaiveDateTime) -> i64 {
        let m = local.hour() * 60 + local.minute();
        let date = if self.start < self.end || m >= self.start {
            local.date()
        } else {
            local.date() - chrono::Duration::days(1)
        };
        let at = date
            .and_hms_opt(self.start / 60, self.start % 60, 0)
            .unwrap_or(local);
        zone.to_utc_ms(at)
    }

    pub fn active(&self, local: NaiveDateTime) -> bool {
        let m = local.hour() * 60 + local.minute();
        let wd = local.weekday().num_days_from_monday() as usize;
        if self.start < self.end {
            self.days[wd] && m >= self.start && m < self.end
        } else {
            (self.days[wd] && m >= self.start) || (self.days[(wd + 6) % 7] && m < self.end)
        }
    }
}

pub(crate) struct CSeason<'a> {
    pub name: &'a str,
    from: (u32, u32),
    to: (u32, u32),
    pub blocks: Vec<CBlock<'a>>,
}

impl CSeason<'_> {
    pub fn active(&self, local: NaiveDateTime) -> bool {
        let md = (local.month(), local.day());
        if self.from <= self.to {
            md >= self.from && md <= self.to
        } else {
            md >= self.from || md <= self.to
        }
    }
}

pub(crate) struct CEvent<'a> {
    pub name: &'a str,
    start: NaiveDateTime,
    duration_ms: i64,
    yearly: bool,
    pub source: &'a Source,
}

impl CEvent<'_> {
    pub fn active(&self, zone: &Zone, t_ms: i64) -> bool {
        if !self.yearly {
            let s = zone.to_utc_ms(self.start);
            return t_ms >= s && t_ms < s + self.duration_ms;
        }
        let year = zone.local(t_ms).year();
        [year - 1, year].into_iter().any(|y| {
            self.start
                .with_year(y)
                .map(|s| {
                    let s = zone.to_utc_ms(s);
                    t_ms >= s && t_ms < s + self.duration_ms
                })
                .unwrap_or(false)
        })
    }
}

/// A lineup parsed into a form that is cheap to evaluate. Building one is also
/// how a lineup is validated.
pub(crate) struct Compiled<'a> {
    pub zone: Zone,
    pub grid_ms: Option<i64>,
    pub blocks: Vec<CBlock<'a>>,
    pub seasons: Vec<CSeason<'a>>,
    pub events: Vec<CEvent<'a>>,
    /// Sign-off window as minutes of the local day: (from, to).
    pub signoff: Option<(u32, u32)>,
}

impl Compiled<'_> {
    /// Milliseconds left of the sign-off window at `t_ms`, if it is on.
    pub fn signoff_left_ms(&self, t_ms: i64) -> Option<i64> {
        use chrono::Timelike;
        let (from, to) = self.signoff?;
        let local = self.zone.local(t_ms);
        let secs = local.time().num_seconds_from_midnight() as i64;
        let (from_s, to_s) = (from as i64 * 60, to as i64 * 60);
        let on = if from_s < to_s {
            secs >= from_s && secs < to_s
        } else {
            secs >= from_s || secs < to_s
        };
        on.then(|| (to_s - secs).rem_euclid(86_400) * 1000)
    }
}

fn parse_hm(s: &str, allow_24: bool) -> Result<u32, String> {
    if allow_24 && s == "24:00" {
        return Ok(1440);
    }
    NaiveTime::parse_from_str(s, "%H:%M")
        .map(|t| t.hour() * 60 + t.minute())
        .map_err(|_| format!("'{s}' is not a time like 20:00"))
}

fn parse_md(s: &str) -> Result<(u32, u32), String> {
    let bad = || format!("'{s}' is not a date like 09-01");
    let (m, d) = s.split_once('-').ok_or_else(bad)?;
    let (m, d): (u32, u32) = (m.parse().map_err(|_| bad())?, d.parse().map_err(|_| bad())?);
    // 2024 is a leap year, so 02-29 is accepted.
    chrono::NaiveDate::from_ymd_opt(2024, m, d).ok_or_else(bad)?;
    Ok((m, d))
}

fn parse_dt(s: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .map_err(|_| format!("'{s}' is not a date-time like 2026-12-24T18:00"))
}

fn check_source(s: &Source, depth: u32) -> Result<(), String> {
    if depth > 4 {
        return Err("sources are nested too deeply".into());
    }
    match s {
        Source::Show { show } if show.trim().is_empty() => Err("a show source needs a name".into()),
        Source::Show { .. } | Source::Chaos | Source::Pool { .. } => Ok(()),
        Source::Rotation { items, advance } => {
            if items.is_empty() || items.len() > 50 {
                return Err("a rotation needs 1-50 items".into());
            }
            if matches!(advance, Advance::Weeks(0)) {
                return Err("rotation weeks must be at least 1".into());
            }
            if matches!(advance, Advance::Finale)
                && !items.iter().all(|i| matches!(i, Source::Show { .. }))
            {
                return Err("a 'finale' rotation can only contain show sources".into());
            }
            items.iter().try_for_each(|i| check_source(i, depth + 1))
        }
    }
}

fn compile_block<'a>(b: &'a Block) -> Result<CBlock<'a>, String> {
    let (start, end) = (parse_hm(&b.start, false)?, parse_hm(&b.end, true)?);
    if start == end {
        return Err(format!(
            "block '{}' starts and ends at the same time",
            b.name
        ));
    }
    let mut days = [b.days.is_empty(); 7];
    for d in &b.days {
        *days
            .get_mut(*d as usize)
            .ok_or_else(|| format!("day {d} is not 0 (Mon) to 6 (Sun)"))? = true;
    }
    check_source(&b.source, 0)?;
    Ok(CBlock {
        name: &b.name,
        days,
        start,
        end,
        once: b.once,
        source: &b.source,
    })
}

impl<'a> Compiled<'a> {
    pub fn new(l: &'a Lineup) -> Result<Self, String> {
        let zone = if l.timezone.is_empty() {
            Zone::utc()
        } else {
            Zone::parse(&l.timezone).ok_or_else(|| format!("unknown timezone '{}'", l.timezone))?
        };
        if l.blocks.len() > 500 || l.seasons.len() > 50 || l.events.len() > 500 {
            return Err("lineup is too large".into());
        }
        if let Some(g) = l.grid_minutes {
            if !GRIDS.contains(&g) {
                return Err(format!("grid_minutes must be one of {GRIDS:?}"));
            }
        }
        let blocks = l
            .blocks
            .iter()
            .map(compile_block)
            .collect::<Result<Vec<_>, _>>()?;
        let mut seasons = Vec::new();
        for s in &l.seasons {
            seasons.push(CSeason {
                name: &s.name,
                from: parse_md(&s.from)?,
                to: parse_md(&s.to)?,
                blocks: s
                    .blocks
                    .iter()
                    .map(compile_block)
                    .collect::<Result<_, _>>()?,
            });
        }
        let mut events = Vec::new();
        let mut names = BTreeSet::new();
        for e in &l.events {
            if e.name.trim().is_empty() || !names.insert(e.name.to_lowercase()) {
                return Err("event names must be unique and not empty".into());
            }
            let (start, end) = (parse_dt(&e.start)?, parse_dt(&e.end)?);
            if end <= start {
                return Err(format!("event '{}' must end after it starts", e.name));
            }
            check_source(&e.source, 0)?;
            events.push(CEvent {
                name: &e.name,
                start,
                duration_ms: (end - start).num_milliseconds(),
                yearly: e.yearly,
                source: &e.source,
            });
        }
        if let Some(f) = &l.fill {
            check_source(f, 0)?;
        }
        if let Some(a) = &l.ads {
            a.validate()?;
        }
        let signoff = match &l.signoff {
            Some(so) => {
                let (f, t) = (parse_hm(&so.from, false)?, parse_hm(&so.to, true)?);
                if f == t % 1440 {
                    return Err("sign-off must start and end at different times".into());
                }
                Some((f, t % 1440))
            }
            None => None,
        };
        let has_blocks = !blocks.is_empty() || seasons.iter().any(|s| !s.blocks.is_empty());
        let grid_ms = l
            .grid_minutes
            .or(if has_blocks { Some(30) } else { None })
            .map(|m| m as i64 * 60_000);
        Ok(Self {
            zone,
            grid_ms,
            blocks,
            seasons,
            events,
            signoff,
        })
    }
}

impl Lineup {
    /// Check the lineup is usable. Returns a human-readable reason if not.
    pub fn validate(&self) -> Result<(), String> {
        Compiled::new(self).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Lineup {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn empty_lineup_is_valid_and_loose() {
        let l = parse("{}");
        assert_eq!(l, Lineup::default());
        let c = Compiled::new(&l).unwrap();
        assert_eq!(c.grid_ms, None);
    }

    #[test]
    fn blocks_imply_a_30_minute_grid() {
        let l = parse(
            r#"{"blocks":[{"start":"20:00","end":"22:00","source":{"type":"show","show":"A"}}]}"#,
        );
        assert_eq!(Compiled::new(&l).unwrap().grid_ms, Some(30 * 60_000));
    }

    #[test]
    fn rejects_bad_input() {
        let bad = [
            r#"{"timezone":"Nope/Land"}"#,
            r#"{"grid_minutes":7}"#,
            r#"{"blocks":[{"start":"25:00","end":"26:00","source":{"type":"chaos"}}]}"#,
            r#"{"blocks":[{"start":"20:00","end":"20:00","source":{"type":"chaos"}}]}"#,
            r#"{"blocks":[{"days":[7],"start":"20:00","end":"21:00","source":{"type":"chaos"}}]}"#,
            r#"{"blocks":[{"start":"20:00","end":"21:00","source":{"type":"show","show":" "}}]}"#,
            r#"{"seasons":[{"name":"x","from":"13-01","to":"02-01","blocks":[]}]}"#,
            r#"{"events":[{"name":"e","start":"2026-12-24T18:00","end":"2026-12-24T17:00","source":{"type":"chaos"}}]}"#,
            r#"{"events":[{"name":"e","start":"2026-12-24T18:00","end":"2026-12-24T19:00","source":{"type":"chaos"}},{"name":"E","start":"2026-12-25T18:00","end":"2026-12-25T19:00","source":{"type":"chaos"}}]}"#,
            r#"{"fill":{"type":"rotation","items":[],"advance":"finale"}}"#,
            r#"{"fill":{"type":"rotation","items":[{"type":"chaos"}],"advance":"finale"}}"#,
            r#"{"fill":{"type":"rotation","items":[{"type":"chaos"}],"advance":{"weeks":0}}}"#,
        ];
        for json in bad {
            assert!(parse(json).validate().is_err(), "should reject: {json}");
        }
    }

    #[test]
    fn accepts_a_full_featured_lineup() {
        let l = parse(
            r#"{
              "timezone": "America/Toronto",
              "grid_minutes": 30,
              "blocks": [
                {"name":"Morning toons","days":[0,1,2,3,4],"start":"06:00","end":"10:00","source":{"type":"pool","shows":["Looney"],"order":"shuffle"}},
                {"name":"Late","start":"23:00","end":"02:00","source":{"type":"chaos"}}
              ],
              "seasons": [{"name":"Fall","from":"09-01","to":"05-31","blocks":[
                {"start":"20:00","end":"21:00","source":{"type":"rotation","advance":{"weeks":4},"items":[{"type":"show","show":"A"},{"type":"show","show":"B"}]}}]}],
              "events": [{"name":"Xmas","start":"2026-12-25T08:00","end":"2026-12-25T20:00","yearly":true,"source":{"type":"show","show":"Special"}}],
              "fill": {"type":"pool","shows":[],"order":"sequential"},
              "catch_up": "advance"
            }"#,
        );
        l.validate().unwrap();
    }

    #[test]
    fn overnight_blocks_span_midnight_and_respect_days() {
        let l = parse(
            r#"{"blocks":[{"days":[4],"start":"23:00","end":"02:00","source":{"type":"chaos"}}]}"#,
        );
        let c = Compiled::new(&l).unwrap();
        let at = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        // 2026-11-20 is a Friday
        assert!(c.blocks[0].active(at("2026-11-20 23:30")));
        assert!(c.blocks[0].active(at("2026-11-21 01:30"))); // Saturday early hours still Friday's block
        assert!(!c.blocks[0].active(at("2026-11-21 02:00")));
        assert!(!c.blocks[0].active(at("2026-11-21 23:30"))); // Saturday night is not Friday
        assert!(!c.blocks[0].active(at("2026-11-20 22:59")));
    }

    #[test]
    fn season_range_can_wrap_the_year() {
        let l = parse(r#"{"seasons":[{"name":"Fall","from":"09-01","to":"05-31","blocks":[]}]}"#);
        let c = Compiled::new(&l).unwrap();
        let at = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        assert!(c.seasons[0].active(at("2026-11-15 12:00")));
        assert!(c.seasons[0].active(at("2027-02-01 12:00")));
        assert!(!c.seasons[0].active(at("2026-07-15 12:00")));
    }
}
