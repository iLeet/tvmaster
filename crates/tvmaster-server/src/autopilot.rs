//! Autopilot: keeps every channel's schedule filled, endlessly, without anyone
//! touching it. Materializes the scheduler's output into the database a few
//! days ahead, advances the episode cursors as items air, trims history,
//! recovers from downtime, and re-rolls when programming changes.
//!
//! All functions take `now_ms` so they can be tested with a fake clock.

use rusqlite::Connection;
use std::collections::HashMap;
use tvmaster_core::{
    align_start, commit_aired, generate, rebuild_reservations, Catalog, CatchUp, Channel, MediaRef,
    SchedConfig,
};
use tvmaster_core::{AdClip, AdInputs, AdPool, ItemKind};
use tvmaster_media::BreakInfo;

/// How far ahead the schedule is kept materialized.
pub const HORIZON_MS: i64 = 72 * 3_600_000;
/// Items starting within this window are left alone by a re-roll, so the show
/// on air (and what is already encoded behind it) is not disturbed.
pub const LOCK_MS: i64 = 5 * 60_000;
/// A re-roll caused by an unplayable file locks only what playout has already
/// taken (it encodes about half a minute ahead), so the bad file leaves the
/// schedule right away.
const UNPLAYABLE_LOCK_MS: i64 = 30_000;
/// Aired items are kept this long (for the guide's recent history).
const HISTORY_MS: i64 = 6 * 3_600_000;
/// After a longer outage than this, "advance" catch-up gives up and resumes.
const MAX_CATCHUP_MS: i64 = 14 * 24 * 3_600_000;
/// The autopilot loop records a heartbeat each pass. If more than this has
/// passed since the last one, the daemon was not running, and items scheduled
/// during that time did not really air.
const DOWNTIME_MS: i64 = 3 * 60_000;
const HEARTBEAT_KEY: &str = "autopilot_heartbeat_ms";

pub fn catalog_for(conn: &Connection, channel: &Channel) -> rusqlite::Result<Catalog> {
    let rows = tvmaster_db::list_media_in_libraries(conn, &channel.library_ids)?;
    let refs: Vec<MediaRef> = rows
        .into_iter()
        .map(|m| MediaRef {
            id: m.id,
            show: m.show,
            season: m.season.map(|s| s as u32),
            episode: m.episode.map(|e| e as u32),
            sort_key: m.path,
            duration_ms: m.duration_ms.max(0) as u64,
        })
        .collect();
    Ok(Catalog::build(&refs))
}

/// Commercials, user bumper clips, break points and recent ads for a channel
/// whose lineup has an `ads` section.
pub fn ad_inputs_for(
    conn: &Connection,
    channel: &Channel,
    before_ms: i64,
) -> rusqlite::Result<Option<AdInputs>> {
    let Some(ads) = &channel.lineup.ads else {
        return Ok(None);
    };
    let clip = |m: tvmaster_db::MediaRow| AdClip {
        id: m.id,
        dur_ms: m.duration_ms.max(0) as u64,
        year: m.year.map(|y| y as i32),
    };
    let commercials = if ads.commercials {
        tvmaster_db::list_media_of_kind(conn, &ads.library_ids, "commercials")?
    } else {
        Vec::new()
    };
    let clips = tvmaster_db::list_media_of_kind(conn, &ads.bumper_library_ids, "bumpers")?;
    let breaks = tvmaster_db::list_media_in_libraries(conn, &channel.library_ids)?
        .into_iter()
        .filter_map(|m| {
            let info = BreakInfo::from_json(m.breaks_json.as_deref()?)?;
            (!info.points.is_empty()).then_some((m.id, info.points))
        })
        .collect();
    Ok(Some(AdInputs {
        pool: AdPool {
            ads: commercials.into_iter().map(clip).collect(),
            clips: clips.into_iter().map(clip).collect(),
        },
        breaks,
        recent: tvmaster_db::recent_commercials(
            conn,
            &channel.id,
            before_ms,
            ads.no_repeat.max(1) as usize * 2,
        )?,
    }))
}

/// "Now", as far as airing is concerned: after downtime this is the moment the
/// daemon last ran (plus a little slack), not the wall clock.
fn aired_until(conn: &Connection, now_ms: i64) -> rusqlite::Result<i64> {
    let beat = tvmaster_db::meta_get(conn, HEARTBEAT_KEY)?.and_then(|v| v.parse::<i64>().ok());
    Ok(match beat {
        Some(b) if now_ms - b > DOWNTIME_MS => b + DOWNTIME_MS / 3,
        _ => now_ms,
    })
}

/// Give channels saved by an earlier version (no time zone) the server's.
pub fn fill_blank_timezones(conn: &Connection, timezone: &str) {
    let Ok(channels) = tvmaster_db::list_channels(conn) else {
        return;
    };
    for mut ch in channels
        .into_iter()
        .filter(|c| c.lineup.timezone.is_empty())
    {
        ch.lineup.timezone = timezone.to_string();
        if let Err(e) = tvmaster_db::upsert_channel(conn, &ch) {
            tracing::error!(channel = %ch.id, "setting time zone: {e}");
        }
    }
}

/// Mark what has aired. Idempotent.
pub fn commit(conn: &Connection, channel: &Channel, now_ms: i64) -> rusqlite::Result<()> {
    // Everything that has started by now, however old: after downtime in
    // "advance" mode the items generated for the gap are all in the past.
    let items = tvmaster_db::schedule_between(conn, &channel.id, i64::MIN, now_ms + 1)?;
    if items.is_empty() {
        return Ok(());
    }
    let mut cursors = tvmaster_db::load_cursors(conn, &channel.id)?;
    commit_aired(&items, &mut cursors, now_ms);
    tvmaster_db::save_cursors(conn, &channel.id, &cursors)
}

/// Fill the schedule up to the horizon. Returns how many items were added.
pub fn extend(conn: &Connection, channel: &Channel, now_ms: i64) -> rusqlite::Result<usize> {
    let catalog = catalog_for(conn, channel)?;
    let mut cursors = tvmaster_db::load_cursors(conn, &channel.id)?;
    let horizon = now_ms + HORIZON_MS;

    let from = match tvmaster_db::schedule_last_end(conn, &channel.id)? {
        Some(end) if end >= now_ms => end,
        Some(end)
            if channel.lineup.catch_up == CatchUp::Advance && now_ms - end <= MAX_CATCHUP_MS =>
        {
            // Act as if we never stopped: schedule through the gap; the
            // cursors advance as those past items are committed.
            end
        }
        Some(_) => {
            // Resume where we left off: the stale tail never aired.
            tvmaster_db::delete_schedule_from(conn, &channel.id, i64::MIN)?;
            for c in cursors.values_mut() {
                c.reset_reservations();
            }
            align_start(&channel.lineup, now_ms)
        }
        None => {
            // Nothing is scheduled, so nothing can still be reserved.
            for c in cursors.values_mut() {
                c.reset_reservations();
            }
            align_start(&channel.lineup, now_ms)
        }
    };
    if from >= horizon {
        return Ok(0);
    }

    let ad_inputs = ad_inputs_for(conn, channel, from)?;
    let cfg = SchedConfig {
        ads: ad_inputs.as_ref(),
        catalog: &catalog,
        lineup: &channel.lineup,
        strategy: channel.strategy,
        run_out: channel.run_out,
        chaos: channel.chaos,
        seed: channel.seed,
        epoch_ms: channel.epoch_ms,
    };
    let items = generate(&cfg, &mut cursors, from, horizon);
    tvmaster_db::insert_schedule(conn, &channel.id, &items)?;
    tvmaster_db::save_cursors(conn, &channel.id, &cursors)?;
    Ok(items.len())
}

/// Throw away the not-yet-aired future and regenerate it from the current
/// programming. What has aired (and the episodes it used) is untouched, so a
/// re-roll never skips or repeats episodes.
pub fn reroll(conn: &Connection, channel: &Channel, now_ms: i64) -> rusqlite::Result<usize> {
    reroll_locked(conn, channel, now_ms, LOCK_MS)
}

fn reroll_locked(
    conn: &Connection,
    channel: &Channel,
    now_ms: i64,
    lock_ms: i64,
) -> rusqlite::Result<usize> {
    commit(conn, channel, aired_until(conn, now_ms)?)?;
    // Cut at the first show that starts after the lock window, so the slot in
    // progress keeps its padding and everything after it stays on the grid.
    let lock = now_ms + lock_ms;
    let cut = tvmaster_db::schedule_between(conn, &channel.id, lock, i64::MAX)?
        .into_iter()
        .find(|i| i.kind == ItemKind::Show && i.cursor_pos.is_some() && i.start_ms >= lock)
        .map(|i| i.start_ms)
        .unwrap_or(lock);
    tvmaster_db::delete_schedule_from(conn, &channel.id, cut)?;
    let kept = tvmaster_db::schedule_between(conn, &channel.id, i64::MIN, cut)?;
    let mut cursors = tvmaster_db::load_cursors(conn, &channel.id)?;
    rebuild_reservations(&kept, &mut cursors);
    tvmaster_db::save_cursors(conn, &channel.id, &cursors)?;
    extend(conn, channel, now_ms)
}

/// One maintenance pass for a channel: commit, trim, top up.
pub fn maintain(conn: &Connection, channel: &Channel, now_ms: i64) -> rusqlite::Result<usize> {
    let aired = aired_until(conn, now_ms)?;
    commit(conn, channel, aired)?;
    // Extend before trimming: the old tail tells `extend` whether there was downtime.
    let added = extend(conn, channel, now_ms)?;
    // After downtime, "advance" counts the missed airtime as aired; "resume"
    // does not (those items were never really shown).
    let commit_to = if channel.lineup.catch_up == CatchUp::Advance {
        now_ms
    } else {
        aired
    };
    commit(conn, channel, commit_to)?;
    tvmaster_db::prune_schedule_before(conn, &channel.id, now_ms - HISTORY_MS)?;
    Ok(added)
}

/// Run [`maintain`] for every channel, re-rolling the `dirty` ones first.
/// Errors on one channel never stop the others.
pub fn tick(conn: &Connection, dirty: &[String], now_ms: i64) {
    let channels = match tvmaster_db::list_channels(conn) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("autopilot: listing channels: {e}");
            return;
        }
    };
    let dirty: HashMap<&str, ()> = dirty.iter().map(|d| (d.as_str(), ())).collect();
    for ch in channels
        .into_iter()
        .filter(|c| c.kind == tvmaster_core::ChannelKind::Programming)
    {
        let result = if dirty.contains_key(ch.id.as_str()) {
            reroll_locked(conn, &ch, now_ms, UNPLAYABLE_LOCK_MS)
        } else {
            maintain(conn, &ch, now_ms)
        };
        if let Err(e) = result {
            tracing::error!(channel = %ch.id, "autopilot: {e}");
        }
    }
    if let Err(e) = tvmaster_db::meta_set(conn, HEARTBEAT_KEY, &now_ms.to_string()) {
        tracing::error!("autopilot: heartbeat: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tvmaster_core::{ItemKind, Lineup, RunOutPolicy, Strategy};
    use tvmaster_db::{add_library, open_in_memory, upsert_channel, upsert_media, NewMedia};

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;
    /// A Monday at midnight UTC.
    const T0: i64 = 1_794_787_200_000 - 1_794_787_200_000 % DAY;

    fn setup(episodes: i64, lineup_json: &str) -> (Connection, Channel) {
        let conn = open_in_memory().unwrap();
        let lib = add_library(&conn, "/tv", "shows").unwrap();
        for e in 1..=episodes {
            upsert_media(
                &conn,
                &NewMedia {
                    library_id: lib.id,
                    path: format!("/tv/Alpha S01E{e:02}.mp4"),
                    show: Some("Alpha".into()),
                    season: Some(1),
                    episode: Some(e),
                    duration_ms: 22 * MIN,
                    vcodec: Some("h264".into()),
                    acodec: Some("aac".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let ch = Channel {
            id: "c".into(),
            number: 1,
            name: "C".into(),
            strategy: Strategy::BlockGrid,
            run_out: RunOutPolicy::KeepGoing,
            autopilot: true,
            chaos: 0,
            seed: 1,
            epoch_ms: T0,
            always_on: false,
            library_ids: vec![lib.id],
            lineup: serde_json::from_str::<Lineup>(lineup_json).unwrap(),
            kind: Default::default(),
            group: None,
            guide: Default::default(),
        };
        upsert_channel(&conn, &ch).unwrap();
        (conn, ch)
    }

    const ALPHA_ALL_DAY: &str = r#"{"blocks":[{"name":"All","start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}]}"#;

    fn aired_episode_ids(conn: &Connection, from: i64, to: i64) -> Vec<i64> {
        tvmaster_db::schedule_between(conn, "c", from, to)
            .unwrap()
            .into_iter()
            .filter(|i| i.kind == ItemKind::Show)
            .filter_map(|i| i.media_id)
            .collect()
    }

    #[test]
    fn first_run_fills_the_horizon_and_topping_up_is_seamless() {
        let (conn, ch) = setup(5, ALPHA_ALL_DAY);
        assert!(extend(&conn, &ch, T0 + HOUR).unwrap() > 0);
        let end = tvmaster_db::schedule_last_end(&conn, "c").unwrap().unwrap();
        assert!(end >= T0 + HOUR + HORIZON_MS);
        // starts at the grid slot containing now
        let first = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap()[0].clone();
        assert_eq!(first.start_ms, T0 + HOUR);

        // two hours later a top-up adds items right after the old end, no gap or overlap
        extend(&conn, &ch, T0 + 3 * HOUR).unwrap();
        let all = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        for w in all.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms);
        }
        // nothing to do when already full
        assert_eq!(extend(&conn, &ch, T0 + 3 * HOUR).unwrap(), 0);
    }

    #[test]
    fn episodes_continue_in_order_forever_across_top_ups() {
        let (conn, ch) = setup(5, ALPHA_ALL_DAY);
        let mut now = T0;
        for _ in 0..10 {
            maintain(&conn, &ch, now).unwrap();
            now += 12 * HOUR;
        }
        let eps = aired_episode_ids(&conn, T0 - DAY, T0 + 4 * DAY);
        let nums: Vec<i64> = eps.iter().map(|id| (id - 1) % 5).collect();
        // media ids 1..=5 in order, wrapping: 0,1,2,3,4,0,1,...
        for (i, n) in nums.iter().enumerate() {
            assert_eq!(*n, (i as i64) % 5, "position {i}");
        }
    }

    #[test]
    fn commit_advances_cursors_and_history_is_trimmed() {
        let (conn, ch) = setup(50, ALPHA_ALL_DAY);
        maintain(&conn, &ch, T0).unwrap();
        // 3 hours later: 6 slots have started
        maintain(&conn, &ch, T0 + 3 * HOUR).unwrap();
        let cursors = tvmaster_db::load_cursors(&conn, "c").unwrap();
        assert_eq!(
            cursors["show:alpha"].committed, 7,
            "slots at 0:00..3:00 have started"
        );
        // a day later, items older than the history window are gone
        maintain(&conn, &ch, T0 + DAY).unwrap();
        let old =
            tvmaster_db::schedule_between(&conn, "c", 0, T0 + DAY - HISTORY_MS - HOUR).unwrap();
        assert!(old.is_empty());
    }

    #[test]
    fn reroll_changes_the_future_but_never_burns_episodes() {
        let (conn, mut ch) = setup(50, ALPHA_ALL_DAY);
        maintain(&conn, &ch, T0).unwrap();
        let now = T0 + 2 * HOUR + 10 * MIN;
        maintain(&conn, &ch, now).unwrap();
        let before = aired_episode_ids(&conn, T0, now + 1);
        assert_eq!(before, vec![1, 2, 3, 4, 5]);

        // The programming changes to a chaos-free shuffle of the same show.
        ch.strategy = Strategy::Shuffle;
        ch.lineup = serde_json::from_str(r#"{"blocks":[]}"#).unwrap();
        reroll(&conn, &ch, now).unwrap();

        let after_aired = aired_episode_ids(&conn, T0, now + 1);
        assert_eq!(after_aired, before, "what already aired is untouched");
        let cursors = tvmaster_db::load_cursors(&conn, "c").unwrap();
        assert_eq!(cursors["show:alpha"].committed, 5);
        // the locked item (the one on air plus the next within 5 minutes) stays; later ones are new
        let all = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        for w in all.windows(2) {
            assert_eq!(
                w[0].end_ms, w[1].start_ms,
                "still contiguous after the re-roll"
            );
        }
        assert!(all.last().unwrap().end_ms >= now + HORIZON_MS);
    }

    #[test]
    fn reroll_with_same_programming_gives_the_same_schedule() {
        let (conn, ch) = setup(50, ALPHA_ALL_DAY);
        maintain(&conn, &ch, T0).unwrap();
        let now = T0 + 5 * HOUR;
        maintain(&conn, &ch, now).unwrap();
        let before = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        reroll(&conn, &ch, now).unwrap();
        let after = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        if let Some(i) =
            (0..before.len().max(after.len())).find(|i| before.get(*i) != after.get(*i))
        {
            panic!(
                "first difference at {i} of {}/{}:\n before {:?}\n after  {:?}",
                before.len(),
                after.len(),
                before.get(i),
                after.get(i)
            );
        }
    }

    #[test]
    fn downtime_resume_continues_where_it_left_off() {
        let (conn, _ch) = setup(50, ALPHA_ALL_DAY);
        // The daemon ran for a bit (one pass a minute), then the machine was off.
        for m in 0..=60 {
            tick(&conn, &[], T0 + m * MIN);
        }
        // slots at 0:00, 0:30 and 1:00 have started -> episodes 1, 2, 3 aired
        let back = T0 + 10 * DAY;
        tick(&conn, &[], back);
        assert_eq!(
            tvmaster_db::load_cursors(&conn, "c").unwrap()["show:alpha"].committed,
            3
        );
        let first_after = aired_episode_ids(&conn, back, back + 1);
        assert_eq!(
            first_after,
            vec![4],
            "the next unaired episode, nothing skipped"
        );
        let all = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        assert!(
            all.iter().all(|i| i.end_ms > back - HISTORY_MS),
            "stale schedule is dropped"
        );
    }

    #[test]
    fn downtime_advance_acts_as_if_it_kept_airing() {
        let lineup = r#"{"catch_up":"advance","blocks":[{"name":"All","start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}]}"#;
        let (conn, _ch) = setup(1000, lineup);
        for m in 0..=60 {
            tick(&conn, &[], T0 + m * MIN);
        }
        // Off for 5 days (the schedule covered 3 of them; the last 2 were never scheduled).
        let back = T0 + 5 * DAY;
        tick(&conn, &[], back);
        let first_after = aired_episode_ids(&conn, back, back + 1)[0];
        // 5 days of 30-minute slots = 240 episodes aired in the meantime
        assert_eq!(first_after, 241, "advanced by the missed airtime");
    }

    #[test]
    fn a_short_pause_is_not_downtime() {
        let (conn, ch) = setup(50, ALPHA_ALL_DAY);
        tick(&conn, &[], T0);
        tick(&conn, &[], T0 + MIN); // normal one-minute cadence
        tick(&conn, &[], T0 + 2 * MIN);
        let cursors = tvmaster_db::load_cursors(&conn, "c").unwrap();
        assert_eq!(
            cursors["show:alpha"].committed, 1,
            "only the slot that has started"
        );
        // but a restart after 5 minutes is not treated as an outage of airtime that never happened
        maintain(&conn, &ch, T0 + 35 * MIN).unwrap();
        assert!(tvmaster_db::load_cursors(&conn, "c").unwrap()["show:alpha"].committed <= 3);
    }

    #[test]
    fn empty_library_airs_filler_and_recovers_when_content_arrives() {
        let (conn, ch) = setup(0, ALPHA_ALL_DAY);
        maintain(&conn, &ch, T0).unwrap();
        let items = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| i.kind == ItemKind::Filler));

        let lib = tvmaster_db::list_libraries(&conn).unwrap()[0].id;
        upsert_media(
            &conn,
            &NewMedia {
                library_id: lib,
                path: "/tv/new.mp4".into(),
                show: Some("Alpha".into()),
                season: Some(1),
                episode: Some(1),
                duration_ms: 22 * MIN,
                ..Default::default()
            },
        )
        .unwrap();
        reroll(&conn, &ch, T0 + HOUR).unwrap();
        assert!(!aired_episode_ids(&conn, T0 + HOUR + LOCK_MS, T0 + 2 * DAY).is_empty());
    }

    #[test]
    fn tick_maintains_every_channel_and_rerolls_dirty_ones() {
        let (conn, ch) = setup(10, ALPHA_ALL_DAY);
        let mut other = ch.clone();
        other.id = "d".into();
        other.number = 2;
        upsert_channel(&conn, &other).unwrap();
        tick(&conn, &[], T0);
        assert!(tvmaster_db::schedule_last_end(&conn, "c")
            .unwrap()
            .is_some());
        assert!(tvmaster_db::schedule_last_end(&conn, "d")
            .unwrap()
            .is_some());
        // mark one media missing, as playout would, then a dirty tick replaces it
        let missing = aired_episode_ids(&conn, T0 + HOUR, T0 + 2 * HOUR)[0];
        tvmaster_db::mark_media_missing(&conn, missing).unwrap();
        tick(&conn, &["c".to_string()], T0);
        let future = aired_episode_ids(&conn, T0 + LOCK_MS + 1, T0 + 3 * DAY);
        assert!(
            !future.contains(&missing),
            "the unplayable file is no longer scheduled"
        );
    }
}
