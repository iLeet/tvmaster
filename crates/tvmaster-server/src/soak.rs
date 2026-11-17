//! Soak test: weeks of channel time against a fake clock, with the things
//! that go wrong in a real house (the machine off for hours, files deleted, a
//! drive unplugged, new episodes arriving, a lineup edited), checking the
//! schedule's invariants after every pass. Runs in seconds because nothing
//! sleeps: the autopilot takes `now` as an argument.

use crate::autopilot::{self, HORIZON_MS};
use rusqlite::Connection;
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use tvmaster_core::{Channel, ItemKind, Lineup, RunOutPolicy, Strategy};
use tvmaster_db::{
    add_library, list_media_in_libraries, open_in_memory, set_breaks, upsert_channel, upsert_media,
    NewMedia,
};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;
/// A Monday at midnight UTC.
const T0: i64 = 1_794_787_200_000 - 1_794_787_200_000 % DAY;
/// The daemon's pass interval in the simulation (the real one is a minute;
/// five keeps three weeks quick without changing what is exercised).
const STEP: i64 = 5 * MIN;

struct World {
    conn: Connection,
    shows_lib: i64,
    ads_lib: i64,
    channels: Vec<Channel>,
}

fn media(lib: i64, path: String, show: &str, ep: i64, mins: i64) -> NewMedia {
    NewMedia {
        library_id: lib,
        path,
        show: Some(show.into()),
        season: Some(1),
        episode: Some(ep),
        duration_ms: mins * MIN,
        vcodec: Some("h264".into()),
        acodec: Some("aac".into()),
        ..Default::default()
    }
}

fn world() -> World {
    let conn = open_in_memory().unwrap();
    let shows_lib = add_library(&conn, "/tv", "shows").unwrap().id;
    let ads_lib = add_library(&conn, "/ads", "commercials").unwrap().id;
    for (show, eps, mins) in [("Alpha", 12, 22), ("Beta", 8, 44), ("Gamma", 20, 22)] {
        for e in 1..=eps {
            let id = upsert_media(
                &conn,
                &media(
                    shows_lib,
                    format!("/tv/{show} S01E{e:02}.mp4"),
                    show,
                    e,
                    mins,
                ),
            )
            .unwrap();
            // Break points so mid-show breaks are exercised.
            let points: Vec<u64> = (1..mins / 8).map(|k| (k * 8 * MIN) as u64).collect();
            let info = tvmaster_media::BreakInfo {
                src: "chapters".into(),
                points,
            };
            set_breaks(&conn, id, &info.to_json()).unwrap();
        }
    }
    for n in 0..40 {
        let mut m = media(ads_lib, format!("/ads/spot{n}.mp4"), "", n, 0);
        m.show = None;
        m.season = None;
        m.episode = None;
        m.duration_ms = (15 + (n % 4) * 15) * 1000;
        m.year = Some(1980 + n % 10);
        upsert_media(&conn, &m).unwrap();
    }

    let lineup = |json: String| serde_json::from_str::<Lineup>(&json).unwrap();
    let ads = format!(
        r#""ads":{{"library_ids":[{ads_lib}],"pod_seconds":120,"max_breaks":3,"no_repeat":5}}"#
    );
    let make = |id: &str, number: u32, strategy, run_out, chaos, lineup: Lineup| Channel {
        id: id.into(),
        number,
        name: id.into(),
        strategy,
        run_out,
        autopilot: true,
        chaos,
        seed: 7 + number as u64,
        epoch_ms: T0,
        always_on: false,
        library_ids: vec![shows_lib],
        lineup,
        kind: Default::default(),
        group: None,
        guide: Default::default(),
        info: Default::default(),
        options: Default::default(),
    };
    let channels = vec![
        // A grid with ads, two dayparts and a weekend special.
        make(
            "grid",
            2,
            Strategy::BlockGrid,
            RunOutPolicy::KeepGoing,
            0,
            lineup(format!(
                r#"{{"grid_minutes":30,{ads},"blocks":[
                   {{"name":"Morning","start":"06:00","end":"12:00","source":{{"type":"show","show":"Alpha"}}}},
                   {{"name":"Day","start":"12:00","end":"24:00","source":{{"type":"pool","shows":["Beta","Gamma"]}}}},
                   {{"name":"Night","start":"00:00","end":"06:00","source":{{"type":"chaos"}}}}]}}"#
            )),
        ),
        // Plain sequential: every episode in order, forever.
        make(
            "sequence",
            4,
            Strategy::Sequential,
            RunOutPolicy::KeepGoing,
            0,
            lineup("{}".into()),
        ),
        // Pure chaos.
        make(
            "chaos",
            5,
            Strategy::Chaos,
            RunOutPolicy::ChaosFill,
            100,
            lineup("{}".into()),
        ),
        // Shuffle that falls back to chaos for gaps, with "advance" catch-up.
        make(
            "shuffle",
            6,
            Strategy::Shuffle,
            RunOutPolicy::Hybrid,
            20,
            lineup(r#"{"catch_up":"advance"}"#.into()),
        ),
    ];
    for c in &channels {
        upsert_channel(&conn, c).unwrap();
    }
    World {
        conn,
        shows_lib,
        ads_lib,
        channels,
    }
}

/// What is wrong with the schedule right now, if anything.
fn check(w: &World, now: i64, why: &str) -> Result<(), String> {
    for ch in &w.channels {
        let items = tvmaster_db::schedule_between(&w.conn, &ch.id, i64::MIN, i64::MAX).unwrap();
        let ctx = |msg: String| {
            format!(
                "{} at day {:.2} ({why}): {msg}",
                ch.id,
                (now - T0) as f64 / DAY as f64
            )
        };
        if items.is_empty() {
            return Err(ctx("nothing scheduled".into()));
        }
        for p in items.windows(2) {
            if p[0].end_ms != p[1].start_ms {
                return Err(ctx(format!(
                    "gap or overlap: {:?} ends {} but {:?} starts {}",
                    p[0].kind, p[0].end_ms, p[1].kind, p[1].start_ms
                )));
            }
        }
        for i in &items {
            if i.end_ms <= i.start_ms {
                return Err(ctx(format!("{:?} has no length", i.kind)));
            }
        }
        if !items.iter().any(|i| i.start_ms <= now && now < i.end_ms) {
            return Err(ctx("nothing on air".into()));
        }
        let end = items.last().unwrap().end_ms;
        if end < now + HORIZON_MS {
            return Err(ctx(format!("only filled {}h ahead", (end - now) / HOUR)));
        }
        // Nothing that is gone is scheduled to air (beyond what playout has already taken).
        let present: HashSet<i64> = list_media_in_libraries(&w.conn, &[w.shows_lib])
            .unwrap()
            .into_iter()
            .chain(tvmaster_db::list_media_of_kind(&w.conn, &[w.ads_lib], "commercials").unwrap())
            .map(|m| m.id)
            .collect();
        // The show already on air (with its later acts and ads) keeps what it
        // had, and playout airs the slate for a file that has vanished; the
        // next show onwards must be clean.
        let cut = items
            .iter()
            .find(|i| {
                i.kind == ItemKind::Show && i.cursor_pos.is_some() && i.start_ms >= now + 30_000
            })
            .map_or(i64::MAX, |i| i.start_ms);
        for i in items.iter().filter(|i| i.start_ms >= cut) {
            if let (ItemKind::Show | ItemKind::Commercial, Some(id)) = (i.kind, i.media_id) {
                if !present.contains(&id) {
                    return Err(ctx(format!(
                        "{:?} {id} is scheduled but its file is gone",
                        i.kind
                    )));
                }
            }
        }
    }
    Ok(())
}

fn committed(w: &World) -> HashMap<(String, String), u64> {
    let mut out = HashMap::new();
    for ch in &w.channels {
        for (k, c) in tvmaster_db::load_cursors(&w.conn, &ch.id).unwrap() {
            out.insert((ch.id.clone(), k), c.committed);
        }
    }
    out
}

fn fingerprint(w: &World) -> u64 {
    let mut h = DefaultHasher::new();
    for ch in &w.channels {
        for i in tvmaster_db::schedule_between(&w.conn, &ch.id, i64::MIN, i64::MAX).unwrap() {
            (i.kind as u8, i.media_id, i.start_ms, i.end_ms, i.in_ms).hash(&mut h);
        }
    }
    let mut cursors: Vec<_> = committed(w).into_iter().collect();
    cursors.sort();
    cursors.hash(&mut h);
    h.finish()
}

/// Everything that goes wrong, on a timetable (days since the start).
fn events(w: &mut World, day: f64, prev_day: f64) -> Vec<String> {
    let crossed = |d: f64| prev_day < d && day >= d;
    let mut dirty = Vec::new();
    if crossed(6.0) {
        // Three episodes deleted from disk.
        let rows = list_media_in_libraries(&w.conn, &[w.shows_lib]).unwrap();
        for m in rows.iter().step_by(11).take(3) {
            tvmaster_db::mark_media_missing(&w.conn, m.id).unwrap();
        }
        dirty.extend(w.channels.iter().map(|c| c.id.clone()));
    }
    if crossed(9.0) {
        // A new season of Alpha is scanned in.
        for e in 13..=18 {
            upsert_media(
                &w.conn,
                &media(
                    w.shows_lib,
                    format!("/tv/Alpha S02E{e:02}.mp4"),
                    "Alpha",
                    e,
                    22,
                ),
            )
            .unwrap();
        }
        dirty.extend(w.channels.iter().map(|c| c.id.clone()));
    }
    if crossed(12.0) {
        // The drive with the shows is unplugged...
        tvmaster_db::mark_library_missing(&w.conn, w.shows_lib).unwrap();
        dirty.extend(w.channels.iter().map(|c| c.id.clone()));
    }
    if crossed(12.5) {
        // ...and plugged back in; a rescan finds everything again.
        for (show, eps, mins) in [("Alpha", 12, 22), ("Beta", 8, 44), ("Gamma", 20, 22)] {
            for e in 1..=eps {
                upsert_media(
                    &w.conn,
                    &media(
                        w.shows_lib,
                        format!("/tv/{show} S01E{e:02}.mp4"),
                        show,
                        e,
                        mins,
                    ),
                )
                .unwrap();
            }
        }
        dirty.extend(w.channels.iter().map(|c| c.id.clone()));
    }
    if crossed(15.0) {
        // The grid channel's lineup is edited: no more ads, and a different grid.
        let ch = &mut w.channels[0];
        ch.lineup.ads = None;
        ch.lineup.grid_minutes = Some(60);
        upsert_channel(&w.conn, ch).unwrap();
        dirty.push(ch.id.clone());
    }
    dirty
}

/// Run `days` of channel time. Returns the fingerprint of the final state.
fn run(days: i64) -> u64 {
    let mut w = world();
    let mut last_committed = HashMap::new();
    let end = T0 + days * DAY;
    let mut now = T0;
    let mut prev_day = -1.0;
    while now < end {
        let day = (now - T0) as f64 / DAY as f64;
        // The machine is off from day 3.25 to 3.5 and for most of day 17.
        let off = (3.25..3.5).contains(&day) || (17.0..17.9).contains(&day);
        if !off {
            let dirty = events(&mut w, day, prev_day);
            autopilot::tick(&w.conn, &dirty, now);
            if let Err(e) = check(&w, now, "tick") {
                panic!("{e}");
            }
            // Episode cursors only ever move forward: nothing is replayed or skipped back.
            let now_committed = committed(&w);
            for (k, v) in &now_committed {
                if let Some(old) = last_committed.get(k) {
                    assert!(
                        v >= old,
                        "cursor {k:?} went backwards: {old} -> {v} at day {day:.2}"
                    );
                }
            }
            last_committed = now_committed;
            // The checks above are only worth something if the schedule has
            // real content to check: ads on the grid, filler while the drive is out.
            if (5.0..5.0 + STEP as f64 / DAY as f64).contains(&day) {
                let items = tvmaster_db::schedule_between(&w.conn, "grid", now, i64::MAX).unwrap();
                for kind in [ItemKind::Commercial, ItemKind::Show] {
                    assert!(
                        items.iter().any(|i| i.kind == kind),
                        "no {kind:?} scheduled"
                    );
                }
            }
            if (12.25..12.25 + STEP as f64 / DAY as f64).contains(&day) {
                let items = tvmaster_db::schedule_between(&w.conn, "grid", now, i64::MAX).unwrap();
                assert!(
                    items.iter().any(|i| i.kind == ItemKind::Filler),
                    "an unplugged drive should leave filler on air"
                );
            }
        }
        prev_day = day;
        now += STEP;
    }
    // The sequential channel really did keep going: a long way through the library.
    let seq = last_committed
        .iter()
        .filter(|((c, _), _)| c == "sequence")
        .map(|(_, v)| *v)
        .max()
        .unwrap_or(0);
    assert!(
        seq > (days as u64) * 40,
        "sequential channel only aired {seq} episodes"
    );
    fingerprint(&w)
}

#[test]
fn three_weeks_of_channel_time_keep_every_invariant() {
    run(21);
}

#[test]
fn the_same_inputs_always_give_the_same_schedule() {
    assert_eq!(run(10), run(10));
}
