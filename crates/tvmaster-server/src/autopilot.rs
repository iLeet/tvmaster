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

/// Files get flagged missing when playout cannot open them, which is what an
/// unplugged drive or a rebooting NAS looks like. Look again: any that are
/// back are cleared, and every programming channel is returned for a re-roll
/// so the shows return to the schedule without anyone rescanning.
pub fn revive(conn: &Connection) -> Vec<String> {
    let missing = match tvmaster_db::list_missing_media(conn) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("autopilot: listing missing files: {e}");
            return Vec::new();
        }
    };
    let mut back = 0;
    for (id, path) in missing {
        if std::path::Path::new(&path).exists() && tvmaster_db::mark_media_present(conn, id).is_ok()
        {
            back += 1;
        }
    }
    if back == 0 {
        return Vec::new();
    }
    tracing::info!("{back} missing files are back; re-rolling channels");
    tvmaster_db::list_channels(conn)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.kind == tvmaster_core::ChannelKind::Programming)
        .map(|c| c.id)
        .collect()
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
    // Off the air right now with nothing to show: let the new plan start at once, even part way
    // through its slot, instead of waiting for the stand-by stretch to run out.
    let cut = match off_air_now(conn, &channel.id, now_ms)? {
        Some(start) => cut.min(start),
        None => cut,
    };
    tvmaster_db::delete_schedule_from(conn, &channel.id, cut)?;
    let kept = tvmaster_db::schedule_between(conn, &channel.id, i64::MIN, cut)?;
    let mut cursors = tvmaster_db::load_cursors(conn, &channel.id)?;
    rebuild_reservations(&kept, &mut cursors);
    tvmaster_db::save_cursors(conn, &channel.id, &cursors)?;
    extend(conn, channel, now_ms)
}

/// Start of the stand-by stretch (test screen, nothing scheduled) on air at `now_ms`, if that is what is on.
fn off_air_now(conn: &Connection, channel: &str, now_ms: i64) -> rusqlite::Result<Option<i64>> {
    Ok(
        tvmaster_db::schedule_between(conn, channel, now_ms - 3_600_000, now_ms + 1)?
            .into_iter()
            .rev()
            .find(|i| i.start_ms <= now_ms && i.end_ms > now_ms)
            .filter(|i| {
                i.kind == ItemKind::Filler
                    && i.media_id.is_none()
                    && i.label.as_deref() != Some(tvmaster_core::SIGNOFF_LABEL)
                    && (i.label.is_none()
                        || i.label.as_deref() == Some(tvmaster_core::OFF_AIR_LABEL))
            })
            .map(|i| i.start_ms),
    )
}

/// Plan again from the slot that is on air now, so a changed plan (or a fix) takes effect
/// immediately and joins part way through, instead of waiting for what is airing to end.
/// What aired before this slot is kept; the slot itself is planned afresh.
pub fn replan_now(conn: &Connection, channel: &Channel, now_ms: i64) -> rusqlite::Result<usize> {
    let items =
        tvmaster_db::schedule_between(conn, &channel.id, now_ms - 12 * 3_600_000, now_ms + 1)?;
    let cut = match items
        .iter()
        .rposition(|i| i.start_ms <= now_ms && i.end_ms > now_ms)
    {
        None => now_ms,
        Some(i) => {
            let first_act = items[..=i]
                .iter()
                .rposition(|x| x.kind == ItemKind::Show && x.cursor_pos.is_some());
            match first_act {
                // Acts, breaks and padding of one slot run without a gap up to the current item.
                Some(j)
                    if items[j..=i]
                        .windows(2)
                        .all(|w| w[0].end_ms == w[1].start_ms) =>
                {
                    items[j].start_ms
                }
                _ => items[i].start_ms,
            }
        }
    };
    commit(conn, channel, cut - 1)?;
    // What is planned afresh from `cut` may include the episode already counted as started: put it back.
    let dropped = tvmaster_db::schedule_between(conn, &channel.id, cut, i64::MAX)?;
    let mut cursors = tvmaster_db::load_cursors(conn, &channel.id)?;
    for it in dropped.iter().filter(|i| i.start_ms >= cut) {
        if let (Some(k), Some(p)) = (&it.cursor_key, it.cursor_pos) {
            let c = cursors.entry(k.clone()).or_default();
            c.committed = c.committed.min(p);
        }
    }
    tvmaster_db::save_cursors(conn, &channel.id, &cursors)?;
    tvmaster_db::delete_schedule_from(conn, &channel.id, i64::MIN)?;
    extend(conn, channel, now_ms)
}

/// What to air right now.
pub enum PlayTarget {
    /// The show's next episode (and the show carries on from the one after).
    Show(String),
    /// One particular file, without disturbing any show's place.
    Media(i64),
    /// Any episode the channel has.
    Random,
}

/// Cut whatever is airing and air `target` now; the schedule carries on after it.
pub fn play_now(
    conn: &Connection,
    channel: &Channel,
    target: PlayTarget,
    now_ms: i64,
) -> Result<usize, String> {
    let db = |e: rusqlite::Error| format!("database: {e}");
    let catalog = catalog_for(conn, channel).map_err(db)?;
    check_target(conn, &catalog, &target)?;
    // What has already started counts as aired, so the cursors are where they should be.
    commit(conn, channel, now_ms - 1).map_err(db)?;
    let mut cursors = tvmaster_db::load_cursors(conn, &channel.id).map_err(db)?;

    // The item on air is cut short where it stands; everything after it goes.
    let on_air = tvmaster_db::schedule_between(conn, &channel.id, now_ms - 3_600_000, now_ms + 1)
        .map_err(db)?
        .into_iter()
        .rev()
        .find(|i| i.start_ms < now_ms && i.end_ms > now_ms);
    let cut_from = on_air.as_ref().map(|i| i.start_ms).unwrap_or(now_ms);
    tvmaster_db::delete_schedule_from(conn, &channel.id, cut_from).map_err(db)?;
    if let Some(mut cut) = on_air {
        cut.end_ms = now_ms;
        tvmaster_db::insert_schedule(conn, &channel.id, &[cut]).map_err(db)?;
    }
    let kept = tvmaster_db::schedule_between(conn, &channel.id, i64::MIN, now_ms).map_err(db)?;
    rebuild_reservations(&kept, &mut cursors);

    let (media_id, dur_ms, key, pos) = match target {
        PlayTarget::Show(name) => {
            let eps = catalog
                .show(&name)
                .ok_or_else(|| format!("this channel has no show called '{name}'"))?;
            let key = format!("show:{}", name.to_lowercase());
            let cur = cursors.entry(key.clone()).or_default();
            // Past the last episode, start the show again.
            let pos = cur.reserved;
            let ep = eps.eps[(pos % eps.eps.len() as u64) as usize];
            cur.reserved = pos + 1;
            (ep.id, ep.dur_ms, key, pos)
        }
        PlayTarget::Media(id) => {
            let m = tvmaster_db::get_media(conn, id)
                .map_err(db)?
                .filter(|m| m.duration_ms > 0 && std::path::Path::new(&m.path).exists())
                .ok_or("that file is not available right now")?;
            (
                m.id,
                m.duration_ms as u64,
                "playnow".to_string(),
                now_ms as u64,
            )
        }
        PlayTarget::Random => {
            let keys = catalog.show_keys();
            if keys.is_empty() {
                return Err("this channel has nothing to play".into());
            }
            let pick = (now_ms as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 11;
            let show = catalog
                .by_key(&keys[(pick % keys.len() as u64) as usize])
                .expect("key came from the catalog");
            let ep = show.eps[((pick >> 8) % show.eps.len() as u64) as usize];
            (ep.id, ep.dur_ms, "playnow".to_string(), now_ms as u64)
        }
    };
    tvmaster_db::insert_schedule(
        conn,
        &channel.id,
        &[tvmaster_core::ScheduledItem {
            kind: ItemKind::Show,
            media_id: Some(media_id),
            start_ms: now_ms,
            end_ms: now_ms + dur_ms as i64,
            cursor_key: Some(key),
            cursor_pos: Some(pos),
            label: Some("Playing now".to_string()),
            in_ms: 0,
            bumper: None,
        }],
    )
    .map_err(db)?;
    tvmaster_db::save_cursors(conn, &channel.id, &cursors).map_err(db)?;
    extend(conn, channel, now_ms).map_err(db)
}

/// Change what airs in `[from_ms, to_ms)`: put `target` there (a show's next episode, a particular
/// file, anything), or leave it off the air (`None`). Items around the range are trimmed to fit and
/// everything else stays. An episode longer than the range is cut at its end; a shorter one is
/// padded like any slot. Edits last until the channel is re-rolled.
pub fn edit_range(
    conn: &Connection,
    channel: &Channel,
    from_ms: i64,
    to_ms: i64,
    target: Option<PlayTarget>,
    now_ms: i64,
) -> Result<usize, String> {
    let db = |e: rusqlite::Error| format!("database: {e}");
    let from_ms = from_ms.max(now_ms);
    if to_ms <= from_ms {
        return Err("the end must be after the start (and after now)".into());
    }
    if to_ms - from_ms > 48 * 3_600_000 {
        return Err("change at most 48 hours at a time".into());
    }
    let off_air = |start: i64, end: i64| tvmaster_core::ScheduledItem {
        kind: ItemKind::Filler,
        media_id: None,
        start_ms: start,
        end_ms: end,
        cursor_key: None,
        cursor_pos: None,
        label: Some(tvmaster_core::OFF_AIR_LABEL.to_string()),
        in_ms: 0,
        bumper: None,
    };
    let catalog = catalog_for(conn, channel).map_err(db)?;
    if let Some(t) = &target {
        check_target(conn, &catalog, t)?;
    }
    commit(conn, channel, now_ms).map_err(db)?;

    // Everything from the first item the range touches is rewritten.
    let touched = tvmaster_db::schedule_between(conn, &channel.id, from_ms, to_ms).map_err(db)?;
    let first = touched
        .iter()
        .map(|i| i.start_ms)
        .min()
        .unwrap_or(from_ms)
        .min(from_ms);
    let rest = tvmaster_db::schedule_between(conn, &channel.id, first, i64::MAX).map_err(db)?;
    let before = tvmaster_db::schedule_between(conn, &channel.id, i64::MIN, first).map_err(db)?;
    tvmaster_db::delete_schedule_from(conn, &channel.id, first).map_err(db)?;

    let mut kept: Vec<tvmaster_core::ScheduledItem> =
        before.into_iter().filter(|i| i.end_ms <= first).collect();
    for it in rest {
        if it.end_ms <= from_ms || it.start_ms >= to_ms {
            kept.push(it);
            continue;
        }
        if it.start_ms < from_ms {
            let mut head = it.clone();
            head.end_ms = from_ms;
            kept.push(head);
        }
        if it.end_ms > to_ms {
            // What was left of it after the range is not worth showing half-way: the screen stands by.
            kept.push(off_air(to_ms, it.end_ms));
        }
    }
    kept.sort_by_key(|i| i.start_ms);

    let mut cursors = tvmaster_db::load_cursors(conn, &channel.id).map_err(db)?;
    rebuild_reservations(&kept, &mut cursors);
    let mut added: Vec<tvmaster_core::ScheduledItem> = Vec::new();
    match target {
        None => added.push(off_air(from_ms, to_ms)),
        Some(t) => {
            let (media_id, dur_ms, key, pos, label) =
                pick_target(conn, &catalog, &mut cursors, t, now_ms)?;
            let end = (from_ms + dur_ms as i64).min(to_ms);
            added.push(tvmaster_core::ScheduledItem {
                kind: ItemKind::Show,
                media_id: Some(media_id),
                start_ms: from_ms,
                end_ms: end,
                cursor_key: Some(key),
                cursor_pos: Some(pos),
                label: label.clone(),
                in_ms: 0,
                bumper: None,
            });
            if end < to_ms {
                let mut pad = off_air(end, to_ms);
                pad.label = label;
                added.push(pad);
            }
        }
    }
    kept.extend(added);
    kept.sort_by_key(|i| i.start_ms);
    let from_first: Vec<_> = kept.into_iter().filter(|i| i.start_ms >= first).collect();
    tvmaster_db::insert_schedule(conn, &channel.id, &from_first).map_err(db)?;
    tvmaster_db::save_cursors(conn, &channel.id, &cursors).map_err(db)?;
    extend(conn, channel, now_ms).map_err(db)?;
    Ok(from_first.len())
}

/// Check a target can be aired before anything is changed.
fn check_target(conn: &Connection, catalog: &Catalog, target: &PlayTarget) -> Result<(), String> {
    match target {
        PlayTarget::Show(name) => catalog
            .show(name)
            .map(|_| ())
            .ok_or_else(|| format!("this channel has no show called '{name}'")),
        PlayTarget::Media(id) => tvmaster_db::get_media(conn, *id)
            .map_err(|e| format!("database: {e}"))?
            .filter(|m| m.duration_ms > 0 && std::path::Path::new(&m.path).exists())
            .map(|_| ())
            .ok_or_else(|| "that file is not available right now".to_string()),
        PlayTarget::Random => {
            if catalog.show_keys().is_empty() {
                Err("this channel has nothing to play".into())
            } else {
                Ok(())
            }
        }
    }
}

/// Choose the file for a [`PlayTarget`] and reserve its place: (media, duration, cursor key, position, label).
fn pick_target(
    conn: &Connection,
    catalog: &Catalog,
    cursors: &mut tvmaster_core::Cursors,
    target: PlayTarget,
    now_ms: i64,
) -> Result<(i64, u64, String, u64, Option<String>), String> {
    match target {
        PlayTarget::Show(name) => {
            let eps = catalog
                .show(&name)
                .ok_or_else(|| format!("this channel has no show called '{name}'"))?;
            let key = format!("show:{}", name.to_lowercase());
            let cur = cursors.entry(key.clone()).or_default();
            let pos = cur.reserved;
            let ep = eps.eps[(pos % eps.eps.len() as u64) as usize];
            cur.reserved = pos + 1;
            Ok((ep.id, ep.dur_ms, key, pos, Some(eps.name.clone())))
        }
        PlayTarget::Media(id) => {
            let m = tvmaster_db::get_media(conn, id)
                .map_err(|e| format!("database: {e}"))?
                .filter(|m| m.duration_ms > 0 && std::path::Path::new(&m.path).exists())
                .ok_or("that file is not available right now")?;
            Ok((
                m.id,
                m.duration_ms as u64,
                "playnow".to_string(),
                now_ms as u64,
                m.show,
            ))
        }
        PlayTarget::Random => {
            let keys = catalog.show_keys();
            if keys.is_empty() {
                return Err("this channel has nothing to play".into());
            }
            let pick = (now_ms as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 11;
            let show = catalog
                .by_key(&keys[(pick % keys.len() as u64) as usize])
                .expect("key came from the catalog");
            let ep = show.eps[((pick >> 8) % show.eps.len() as u64) as usize];
            Ok((
                ep.id,
                ep.dur_ms,
                "playnow".to_string(),
                now_ms as u64,
                Some(show.name.clone()),
            ))
        }
    }
}

/// Throw away the whole schedule and plan again from now. For when the files
/// behind it are gone. Episode cursors keep what has aired.
pub fn rebuild(conn: &Connection, channel: &Channel, now_ms: i64) -> rusqlite::Result<usize> {
    commit(conn, channel, aired_until(conn, now_ms)?)?;
    tvmaster_db::delete_schedule_from(conn, &channel.id, i64::MIN)?;
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
            options: Default::default(),
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

    #[test]
    fn files_that_come_back_are_revived_and_channels_rerolled() {
        let (conn, _ch) = setup(3, ALPHA_ALL_DAY);
        // Point one episode at a real file, flag it missing like playout would.
        let dir = std::env::temp_dir().join(format!("tvmaster-revive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ep.mp4");
        std::fs::write(&file, b"x").unwrap();
        let lib = tvmaster_db::list_libraries(&conn).unwrap()[0].id;
        let id = upsert_media(
            &conn,
            &NewMedia {
                library_id: lib,
                path: file.to_string_lossy().into(),
                show: Some("Alpha".into()),
                season: Some(2),
                episode: Some(1),
                duration_ms: 22 * MIN,
                ..Default::default()
            },
        )
        .unwrap();
        tvmaster_db::mark_media_missing(&conn, id).unwrap();
        // The fake episodes at /tv/... are missing too (never existed) and stay missing.
        tvmaster_db::mark_media_missing(&conn, 1).unwrap();

        // File exists again: revived, channels come back for a re-roll.
        assert_eq!(revive(&conn), vec!["c".to_string()]);
        let missing: Vec<i64> = tvmaster_db::list_missing_media(&conn)
            .unwrap()
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            missing,
            vec![1],
            "only the file that is really gone stays flagged"
        );
        // Nothing changed since: nothing to do.
        assert!(revive(&conn).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
    fn contiguous(conn: &Connection) {
        let all = tvmaster_db::schedule_between(conn, "c", 0, i64::MAX).unwrap();
        for w in all.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms, "no gap or overlap: {w:?}");
        }
    }

    #[test]
    fn a_plan_changed_while_the_stand_by_screen_is_up_airs_at_once_part_way_through() {
        let lineup = r#"{"blocks":[{"name":"Night","start":"20:00","end":"22:00","source":{"type":"show","show":"Alpha"}}]}"#;
        let (conn, mut ch) = setup(50, lineup);
        ch.run_out = tvmaster_core::RunOutPolicy::OffAir;
        let now = T0 + 10 * HOUR + 10 * MIN;
        maintain(&conn, &ch, now).unwrap();
        let on = |c: &Connection| {
            tvmaster_db::schedule_between(c, "c", now, now + 1)
                .unwrap()
                .into_iter()
                .find(|i| i.start_ms <= now && i.end_ms > now)
                .unwrap()
        };
        assert_eq!(
            on(&conn).kind,
            ItemKind::Filler,
            "nothing is scheduled at ten in the morning"
        );

        // The plan now has a show on at that hour: the re-roll does not wait for the stand-by to end.
        ch.lineup = serde_json::from_str(r#"{"blocks":[{"name":"Morning","start":"08:00","end":"12:00","source":{"type":"show","show":"Alpha"}}]}"#).unwrap();
        reroll(&conn, &ch, now).unwrap();
        let item = on(&conn);
        assert_eq!(item.kind, ItemKind::Show);
        assert!(item.start_ms <= now, "it joins part way through its slot");
        contiguous(&conn);
    }

    #[test]
    fn refresh_now_replans_the_slot_on_air_without_burning_an_episode() {
        let (conn, ch) = setup(50, ALPHA_ALL_DAY);
        let now = T0 + 2 * HOUR + 10 * MIN;
        maintain(&conn, &ch, T0).unwrap();
        maintain(&conn, &ch, now).unwrap();
        let before = tvmaster_db::schedule_between(&conn, "c", now, now + 1).unwrap();
        let airing = before
            .iter()
            .find(|i| i.kind == ItemKind::Show)
            .unwrap()
            .media_id;
        replan_now(&conn, &ch, now).unwrap();
        let after = tvmaster_db::schedule_between(&conn, "c", now, now + 1).unwrap();
        let show = after.iter().find(|i| i.kind == ItemKind::Show).unwrap();
        assert_eq!(
            show.media_id, airing,
            "the same plan puts the same episode back on"
        );
        assert!(show.start_ms <= now);
        contiguous(&conn);
    }

    #[test]
    fn air_now_cuts_what_is_on_and_the_schedule_carries_on_after_it() {
        let (conn, ch) = setup(50, ALPHA_ALL_DAY);
        let now = T0 + 2 * HOUR + 10 * MIN;
        maintain(&conn, &ch, now).unwrap();
        play_now(&conn, &ch, PlayTarget::Show("Alpha".into()), now).unwrap();
        let on: Vec<_> = tvmaster_db::schedule_between(&conn, "c", now, now + 1).unwrap();
        let item = on
            .iter()
            .find(|i| i.start_ms <= now && i.end_ms > now)
            .unwrap();
        assert_eq!(item.start_ms, now);
        assert_eq!(item.label.as_deref(), Some("Playing now"));
        assert!(play_now(&conn, &ch, PlayTarget::Show("Nope".into()), now).is_err());
        let all = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        assert!(
            all.last().unwrap().end_ms >= now + HORIZON_MS / 2,
            "the plan continues after it: {:?}",
            all.iter()
                .map(|i| (i.start_ms - now, i.end_ms - now))
                .collect::<Vec<_>>()
        );
        let ids: Vec<_> = all
            .iter()
            .filter(|i| i.kind == ItemKind::Show)
            .filter_map(|i| i.cursor_pos)
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "no episode is scheduled twice");
        contiguous(&conn);
    }

    #[test]
    fn a_stretch_of_the_schedule_can_be_changed_or_left_off_the_air() {
        let (conn, ch) = setup(50, ALPHA_ALL_DAY);
        let now = T0 + HOUR;
        maintain(&conn, &ch, now).unwrap();
        let (from, to) = (T0 + 3 * HOUR + 10 * MIN, T0 + 4 * HOUR);
        edit_range(&conn, &ch, from, to, None, now).unwrap();
        let mid = tvmaster_db::schedule_between(&conn, "c", from, to).unwrap();
        assert!(
            mid.iter().any(|i| i.kind == ItemKind::Filler
                && i.label.as_deref() == Some(tvmaster_core::OFF_AIR_LABEL)
                && i.start_ms == from
                && i.end_ms == to),
            "{mid:?}"
        );
        contiguous(&conn);

        // a show goes in its place, longer than the space it was given
        let (from2, to2) = (T0 + 6 * HOUR, T0 + 6 * HOUR + 10 * MIN);
        edit_range(
            &conn,
            &ch,
            from2,
            to2,
            Some(PlayTarget::Show("Alpha".into())),
            now,
        )
        .unwrap();
        let put = tvmaster_db::schedule_between(&conn, "c", from2, to2).unwrap();
        assert_eq!(
            put.iter().find(|i| i.start_ms == from2).unwrap().kind,
            ItemKind::Show
        );
        assert_eq!(
            put.iter().map(|i| i.end_ms).min().unwrap(),
            to2,
            "cut where the space ends"
        );
        contiguous(&conn);
        let all = tvmaster_db::schedule_between(&conn, "c", 0, i64::MAX).unwrap();
        let pos: Vec<_> = all
            .iter()
            .filter(|i| i.kind == ItemKind::Show)
            .filter_map(|i| i.cursor_pos)
            .collect();
        let mut u = pos.clone();
        u.sort();
        u.dedup();
        assert_eq!(u.len(), pos.len(), "no episode twice");

        assert!(
            edit_range(&conn, &ch, to, from, None, now).is_err(),
            "end before start"
        );
        assert!(edit_range(&conn, &ch, from, from + 49 * HOUR, None, now).is_err());
    }
}
