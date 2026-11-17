//! Day parts: named stretches of the broadcast day ("Kids' morning", "Prime time", "Late night")
//! that decide what may air, whether there are commercial breaks, and what the station's
//! graphics look like. They belong to the station; each one can cover every channel or only some.
//!
//! Resolution is simple: the first day part (in the order they are listed) that covers a moment
//! and a channel is the one in force. Nothing is in force outside them, and the channel carries on
//! as usual.

use crate::clock::Zone;
use crate::model::ThemeChoice;
use chrono::{Datelike, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

/// How suitable something is, from everyone to adults only. Ratings are written all sorts of ways
/// (`TV-PG`, `PG`, `14+`, `C8`); [`Rating::from_text`] reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    /// Everyone: `G`, `TV-Y`, `TV-G`, `C`.
    AllAges,
    /// Older children: `TV-Y7`, `C8`, `7+`.
    Kids,
    /// Parental guidance: `PG`, `TV-PG`.
    Pg,
    /// Teens and up: `PG-13`, `TV-14`, `14+`, `14A`.
    Teen,
    /// Adults: `R`, `TV-MA`, `NC-17`, `18+`.
    Adult,
}

impl Rating {
    pub const ALL: [(&'static str, &'static str, &'static str); 5] = [
        ("all_ages", "All ages", "G, TV-Y, TV-G"),
        ("kids", "Kids 7+", "TV-Y7, C8"),
        ("pg", "Parental guidance", "PG, TV-PG"),
        ("teen", "Teens 14+", "PG-13, TV-14, 14+"),
        ("adult", "Adults", "R, TV-MA, 18+"),
    ];

    /// The rating a piece of free text means, or `None` when it is blank or not one we know.
    pub fn from_text(text: &str) -> Option<Rating> {
        let t: String = text
            .trim()
            .to_uppercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if t.is_empty() {
            return None;
        }
        let t = t.trim_start_matches("TV-").trim_start_matches("TV");
        let tv = text.trim().to_uppercase().starts_with("TV");
        Some(match t {
            "Y" | "G" | "C" | "E" | "ALL" | "ALLAGES" | "U" | "0+" => Rating::AllAges,
            "Y7" | "Y7FV" | "C8" | "E10+" => Rating::Kids,
            "PG" => Rating::Pg,
            "PG-13" | "PG13" | "14A" | "14+" | "13+" | "12A" | "12+" | "T" => Rating::Teen,
            "MA" | "R" | "NC-17" | "NC17" | "X" | "A" | "18A" | "18+" | "M" | "16+" => {
                Rating::Adult
            }
            _ => {
                // A bare age: `7+`, `10+`, `TV-14`.
                let n: u32 = t
                    .trim_end_matches(['+', 'A'])
                    .parse()
                    .ok()
                    .filter(|_| tv || t.ends_with('+') || t.ends_with('A'))?;
                match n {
                    0..=6 => Rating::AllAges,
                    7..=9 => Rating::Kids,
                    10..=11 => Rating::Pg,
                    12..=15 => Rating::Teen,
                    _ => Rating::Adult,
                }
            }
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Rating::AllAges => "all_ages",
            Rating::Kids => "kids",
            Rating::Pg => "pg",
            Rating::Teen => "teen",
            Rating::Adult => "adult",
        }
    }
}

/// Commercial breaks during a day part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartBreaks {
    /// As the channel is set up.
    #[default]
    Normal,
    /// Programmes run clean: no commercials, only the padding to the next time slot.
    Off,
}

/// One part of the day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Daypart {
    pub name: String,
    /// 0 = Monday ... 6 = Sunday. Empty = every day.
    #[serde(default)]
    pub days: Vec<u8>,
    /// Local "HH:MM".
    pub start: String,
    /// Local "HH:MM" ("24:00" allowed). An end before the start runs overnight.
    pub end: String,
    /// Channel ids this applies to. Empty = every channel.
    #[serde(default)]
    pub channels: Vec<String>,
    /// Nothing rated above this airs. Unset = no limit.
    #[serde(default)]
    pub max_rating: Option<Rating>,
    /// Shows without a rating may air under the limit. Off by default: a show nobody has rated
    /// is not known to be suitable.
    #[serde(default)]
    pub allow_unrated: bool,
    #[serde(default)]
    pub breaks: PartBreaks,
    /// The look of the station's graphics (cards, bumpers, bug, lower thirds, ticker, alerts)
    /// during this part. Unset = the channel's own.
    #[serde(default)]
    pub look: Option<ThemeChoice>,
}

fn parse_hm(s: &str, allow_24: bool) -> Result<u32, String> {
    if allow_24 && s == "24:00" {
        return Ok(1440);
    }
    NaiveTime::parse_from_str(s, "%H:%M")
        .map(|t| t.hour() * 60 + t.minute())
        .map_err(|_| format!("'{s}' is not a time like 20:00"))
}

impl Daypart {
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control) {
            return Err("a day part needs a name of up to 40 characters".into());
        }
        let (s, e) = (parse_hm(&self.start, false)?, parse_hm(&self.end, true)?);
        if s == e {
            return Err(format!("'{name}' starts and ends at the same time"));
        }
        if self.days.iter().any(|d| *d > 6) {
            return Err(format!("'{name}': days are 0 (Monday) to 6 (Sunday)"));
        }
        if self.channels.len() > 200 || self.channels.iter().any(|c| c.len() > 100) {
            return Err("a day part lists at most 200 channels".into());
        }
        if let Some(l) = &self.look {
            l.validate()?;
        }
        Ok(())
    }

    /// Whether this part's channel list covers `channel`.
    pub fn covers(&self, channel: &str) -> bool {
        self.channels.is_empty() || self.channels.iter().any(|c| c == channel)
    }
}

/// The most day parts a station can have.
pub const MAX_PARTS: usize = 40;

/// Every day part, checked.
pub fn validate_parts(parts: &[Daypart]) -> Result<(), String> {
    if parts.len() > MAX_PARTS {
        return Err(format!("at most {MAX_PARTS} day parts"));
    }
    parts.iter().try_for_each(Daypart::validate)
}

#[derive(Clone)]
struct Compiled {
    days: [bool; 7],
    start: u32,
    end: u32,
    part: Daypart,
}

impl Compiled {
    fn active(&self, local: chrono::NaiveDateTime) -> bool {
        let m = local.hour() * 60 + local.minute();
        let wd = local.weekday().num_days_from_monday() as usize;
        if self.start < self.end {
            self.days[wd] && m >= self.start && m < self.end
        } else {
            (self.days[wd] && m >= self.start) || (self.days[(wd + 6) % 7] && m < self.end)
        }
    }
}

/// The day parts that apply to one channel, ready to look up by time.
#[derive(Clone)]
pub struct DaypartSet {
    zone: Zone,
    parts: Vec<Compiled>,
}

impl Default for DaypartSet {
    fn default() -> Self {
        Self {
            zone: Zone::utc(),
            parts: Vec::new(),
        }
    }
}

impl std::fmt::Debug for DaypartSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DaypartSet({} parts)", self.parts.len())
    }
}

impl DaypartSet {
    /// The parts that cover `channel`, in `zone`. A part that does not check out is left out
    /// rather than taking the channel off the air.
    pub fn new(zone: Zone, parts: &[Daypart], channel: &str) -> Self {
        let parts = parts
            .iter()
            .filter(|p| p.covers(channel) && p.validate().is_ok())
            .filter_map(|p| {
                let mut days = [p.days.is_empty(); 7];
                for d in &p.days {
                    days[*d as usize] = true;
                }
                Some(Compiled {
                    days,
                    start: parse_hm(&p.start, false).ok()?,
                    end: parse_hm(&p.end, true).ok()?,
                    part: p.clone(),
                })
            })
            .collect();
        Self { zone, parts }
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The part in force at `ms`, if any.
    pub fn at(&self, ms: i64) -> Option<&Daypart> {
        if self.parts.is_empty() {
            return None;
        }
        let local = self.zone.local(ms);
        self.parts.iter().find(|c| c.active(local)).map(|c| &c.part)
    }

    /// The look in force at `ms`, if a part sets one.
    pub fn look_at(&self, ms: i64) -> Option<&ThemeChoice> {
        self.at(ms).and_then(|p| p.look.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, start: &str, end: &str) -> Daypart {
        Daypart {
            name: name.into(),
            days: vec![],
            start: start.into(),
            end: end.into(),
            channels: vec![],
            max_rating: None,
            allow_unrated: false,
            breaks: PartBreaks::Normal,
            look: None,
        }
    }

    #[test]
    fn ratings_are_read_from_the_ways_people_write_them() {
        let r = Rating::from_text;
        assert_eq!(r("TV-Y"), Some(Rating::AllAges));
        assert_eq!(r("g"), Some(Rating::AllAges));
        assert_eq!(r("TV-G"), Some(Rating::AllAges));
        assert_eq!(r("TV-Y7"), Some(Rating::Kids));
        assert_eq!(r("C8"), Some(Rating::Kids));
        assert_eq!(r("TV-PG"), Some(Rating::Pg));
        assert_eq!(r("PG"), Some(Rating::Pg));
        assert_eq!(r("PG-13"), Some(Rating::Teen));
        assert_eq!(r("TV-14"), Some(Rating::Teen));
        assert_eq!(r(" 14+ "), Some(Rating::Teen));
        assert_eq!(r("14A"), Some(Rating::Teen));
        assert_eq!(r("TV-MA"), Some(Rating::Adult));
        assert_eq!(r("R"), Some(Rating::Adult));
        assert_eq!(r("18+"), Some(Rating::Adult));
        assert_eq!(r("7+"), Some(Rating::Kids));
        assert_eq!(r("10+"), Some(Rating::Pg));
        assert_eq!(r(""), None);
        assert_eq!(r("great"), None);
        assert_eq!(r("14"), None, "a bare number is not a rating");
        assert!(Rating::AllAges < Rating::Kids && Rating::Teen < Rating::Adult);
    }

    #[test]
    fn the_first_part_that_covers_the_moment_wins() {
        let mut kids = part("Kids", "06:00", "09:00");
        kids.max_rating = Some(Rating::AllAges);
        let all = part("Daytime", "06:00", "18:00");
        let set = DaypartSet::new(Zone::utc(), &[kids, all], "c1");
        let at = |h: i64, m: i64| set.at((h * 60 + m) * 60_000).map(|p| p.name.clone());
        assert_eq!(at(7, 0).as_deref(), Some("Kids"));
        assert_eq!(at(8, 59).as_deref(), Some("Kids"));
        assert_eq!(at(9, 0).as_deref(), Some("Daytime"));
        assert_eq!(at(18, 0), None);
        assert_eq!(at(3, 0), None);
    }

    #[test]
    fn a_part_can_run_overnight_and_pick_its_days() {
        let mut late = part("Late night", "22:00", "02:00");
        // Friday and Saturday nights: the early hours belong to the night that began the evening before.
        late.days = vec![4, 5];
        let set = DaypartSet::new(Zone::utc(), &[late], "c1");
        // 1970-01-01 was a Thursday; 1970-01-02 Friday.
        let day = 86_400_000i64;
        let hm = |h: i64| h * 3_600_000;
        assert!(set.at(day + hm(23)).is_some(), "Friday 23:00");
        assert!(set.at(2 * day + hm(1)).is_some(), "Saturday 01:00");
        assert!(set.at(2 * day + hm(23)).is_some(), "Saturday 23:00");
        assert!(set.at(3 * day + hm(1)).is_some(), "Sunday 01:00");
        assert!(set.at(hm(23)).is_none(), "Thursday 23:00");
        assert!(
            set.at(day + hm(1)).is_none(),
            "Friday 01:00 follows a Thursday night"
        );
    }

    #[test]
    fn a_part_for_some_channels_leaves_the_others_alone() {
        let mut p = part("Kids", "06:00", "09:00");
        p.channels = vec!["toons".into()];
        let parts = [p];
        assert!(DaypartSet::new(Zone::utc(), &parts, "toons")
            .at(7 * 3_600_000)
            .is_some());
        assert!(DaypartSet::new(Zone::utc(), &parts, "movies")
            .at(7 * 3_600_000)
            .is_none());
    }

    #[test]
    fn nonsense_is_refused_with_a_reason() {
        assert!(part("", "06:00", "09:00").validate().is_err());
        assert!(part("A", "06:00", "06:00").validate().is_err());
        assert!(part("A", "6am", "09:00").validate().is_err());
        let mut p = part("A", "06:00", "24:00");
        assert!(p.validate().is_ok());
        p.days = vec![7];
        assert!(p.validate().unwrap_err().contains("days"));
        let many: Vec<Daypart> = (0..MAX_PARTS + 1)
            .map(|_| part("A", "1:00", "2:00"))
            .collect();
        assert!(validate_parts(&many).is_err());
    }

    #[test]
    fn a_part_saved_without_the_newer_options_loads() {
        let p: Daypart =
            serde_json::from_str(r#"{"name":"Kids","start":"06:00","end":"09:00"}"#).unwrap();
        assert!(!p.allow_unrated && p.max_rating.is_none() && p.look.is_none());
        assert_eq!(p.breaks, PartBreaks::Normal);
        let j = serde_json::to_value(Rating::AllAges).unwrap();
        assert_eq!(j, "all_ages");
    }
}
