use crate::autopilot;
use crate::plan::{now_and_next, now_ms, programs, Program};
use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tvmaster_core::{
    Channel, ChannelKind, GuideConfig, ItemKind, Lineup, RunOutPolicy, Strategy, Zone,
};

const VIEW_LOOKAHEAD_MS: i64 = 12 * 3_600_000;

#[derive(Serialize)]
pub struct ChannelView {
    #[serde(flatten)]
    channel: Channel,
    on_air: bool,
    now: Option<Program>,
    next: Option<Program>,
}

fn view(s: &AppState, channel: Channel) -> ApiResult<ChannelView> {
    let on_air = s
        .playout
        .as_ref()
        .map(|p| p.is_running(&channel.id))
        .unwrap_or(false);
    let now = now_ms();
    let progs = programs(&s.db(), &channel.id, now, now + VIEW_LOOKAHEAD_MS)?;
    let (now_p, next_p) = now_and_next(&progs, now);
    Ok(ChannelView {
        channel,
        on_air,
        now: now_p,
        next: next_p,
    })
}

/// The server's own time zone, so "8pm" in a new channel's lineup means 8pm
/// where the TVs are. Falls back to UTC if it can't be determined.
pub(crate) fn system_timezone() -> String {
    iana_time_zone::get_timezone()
        .ok()
        .filter(|tz| Zone::parse(tz).is_some())
        .unwrap_or_else(|| "UTC".to_string())
}

/// Give a lineup the server's time zone if it did not name one.
fn with_timezone(mut lineup: Lineup) -> Lineup {
    if lineup.timezone.is_empty() {
        lineup.timezone = system_timezone();
    }
    lineup
}

fn not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "no such channel")
}

fn load(s: &AppState, id: &str) -> ApiResult<Channel> {
    tvmaster_db::get_channel(&s.db(), id)?.ok_or_else(not_found)
}

pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<ChannelView>>> {
    let channels = tvmaster_db::list_channels(&s.db())?;
    let mut out = Vec::with_capacity(channels.len());
    for c in channels {
        out.push(view(&s, c)?);
    }
    Ok(Json(out))
}

pub async fn get(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ChannelView>> {
    Ok(Json(view(&s, load(&s, &id)?)?))
}

#[derive(Deserialize)]
pub struct ChannelInput {
    id: String,
    number: u32,
    name: String,
    strategy: Option<Strategy>,
    run_out: Option<RunOutPolicy>,
    autopilot: Option<bool>,
    chaos: Option<u8>,
    seed: Option<u64>,
    always_on: Option<bool>,
    library_ids: Option<Vec<i64>>,
    lineup: Option<Lineup>,
    kind: Option<ChannelKind>,
    group: Option<String>,
    guide: Option<GuideConfig>,
    /// Audio tracks, subtitles, picture and on-screen graphics. Replaces the old settings as a whole.
    options: Option<tvmaster_core::ChannelOptions>,
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 32
        && chars
            .next()
            .map(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            .unwrap_or(false)
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Do these two channels differ in anything that changes what airs?
fn programming_differs(a: &Channel, b: &Channel) -> bool {
    a.kind != b.kind
        || a.strategy != b.strategy
        || a.run_out != b.run_out
        || a.chaos != b.chaos
        || a.seed != b.seed
        || a.library_ids != b.library_ids
        || a.lineup != b.lineup
}

/// Does a running channel have to be restarted to pick up this change?
fn restart_needed(old: &Channel, new: &Channel) -> bool {
    old.options != new.options
        || old.lineup.signoff != new.lineup.signoff
        || (new.options.overlay.bug.enabled && (old.name != new.name || old.number != new.number))
}

/// The libraries named in a lineup's `ads` must exist and be of the right kind.
fn check_ad_libraries(db: &rusqlite::Connection, lineup: &Lineup) -> ApiResult<()> {
    let Some(ads) = &lineup.ads else {
        return Ok(());
    };
    for (ids, kind) in [
        (&ads.library_ids, "commercials"),
        (&ads.bumper_library_ids, "bumpers"),
    ] {
        for id in ids {
            match tvmaster_db::get_library(db, *id)? {
                Some(l) if l.kind == kind => {}
                Some(l) => {
                    return Err(ApiError::new(
                        StatusCode::BAD_REQUEST,
                        format!("library {id} is a '{}' library, expected '{kind}'", l.kind),
                    ))
                }
                None => {
                    return Err(ApiError::new(
                        StatusCode::BAD_REQUEST,
                        format!("library {id} does not exist"),
                    ))
                }
            }
        }
    }
    Ok(())
}

fn bad_lineup(reason: String) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, format!("invalid lineup: {reason}"))
}

/// Apply a change to a channel's programming: re-roll the future if the
/// channel is on autopilot, otherwise leave the locked schedule alone (it is
/// still topped up so the channel never runs dry).
fn apply_programming_change(s: &AppState, channel: &Channel, existed: bool) -> ApiResult<()> {
    if channel.kind == ChannelKind::Guide {
        return Ok(()); // rendered live; nothing to schedule
    }
    {
        let db = s.db();
        let now = now_ms();
        if existed && channel.autopilot {
            autopilot::reroll(&db, channel, now)?;
        } else {
            autopilot::maintain(&db, channel, now)?;
        }
    }
    s.emit(json!({ "type": "schedule.changed", "channel": channel.id }));
    Ok(())
}

/// Create or update a channel. Updating keeps its epoch and seed unless a new
/// seed is given, so the programme does not jump around on every edit.
pub async fn upsert(
    State(s): State<AppState>,
    Json(body): Json<ChannelInput>,
) -> ApiResult<(StatusCode, Json<Channel>)> {
    if !valid_id(&body.id) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "id must be 1-32 chars of a-z, 0-9, '-' or '_', starting with a letter or digit",
        ));
    }
    if body.number == 0 || body.name.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "number must be > 0 and name must not be empty",
        ));
    }
    if body.chaos.is_some_and(|c| c > 100) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "chaos must be 0-100",
        ));
    }
    if let Some(l) = &body.lineup {
        l.validate().map_err(bad_lineup)?;
        check_ad_libraries(&s.db(), l)?;
    }
    if let Some(o) = &body.options {
        o.validate()
            .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("invalid options: {e}")))?;
    }
    if let Some(g) = &body.guide {
        g.validate()
            .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
        for id in &g.music_library_ids {
            match tvmaster_db::get_library(&s.db(), *id)? {
                Some(l) if l.kind == "music" => {}
                _ => {
                    return Err(ApiError::new(
                        StatusCode::BAD_REQUEST,
                        format!("library {id} is not a 'music' library"),
                    ))
                }
            }
        }
    }
    let group = body
        .group
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_string);
    if group.as_ref().is_some_and(|g| g.chars().count() > 40) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "group must be at most 40 characters",
        ));
    }

    let (channel, existing) = {
        let db = s.db();
        for lib in body.library_ids.iter().flatten() {
            if tvmaster_db::get_library(&db, *lib)?.is_none() {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("library {lib} does not exist"),
                ));
            }
        }
        // Channel numbers are unique across channels.
        for other in tvmaster_db::list_channels(&db)? {
            if other.number == body.number && other.id != body.id {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    format!("channel number {} is used by '{}'", body.number, other.id),
                ));
            }
        }
        let existing = tvmaster_db::get_channel(&db, &body.id)?;
        let now = now_ms();
        let channel = Channel {
            id: body.id,
            number: body.number,
            name: body.name.trim().to_string(),
            strategy: body
                .strategy
                .or(existing.as_ref().map(|e| e.strategy))
                .unwrap_or_default(),
            run_out: body
                .run_out
                .or(existing.as_ref().map(|e| e.run_out))
                .unwrap_or_default(),
            autopilot: body
                .autopilot
                .or(existing.as_ref().map(|e| e.autopilot))
                .unwrap_or(true),
            chaos: body
                .chaos
                .or(existing.as_ref().map(|e| e.chaos))
                .unwrap_or(0),
            seed: body
                .seed
                .or(existing.as_ref().map(|e| e.seed))
                .unwrap_or(now as u64),
            epoch_ms: existing.as_ref().map(|e| e.epoch_ms).unwrap_or(now),
            always_on: body
                .always_on
                .or(existing.as_ref().map(|e| e.always_on))
                .unwrap_or(false),
            library_ids: body
                .library_ids
                .or(existing.as_ref().map(|e| e.library_ids.clone()))
                .unwrap_or_default(),
            lineup: with_timezone(
                body.lineup
                    .or(existing.as_ref().map(|e| e.lineup.clone()))
                    .unwrap_or_default(),
            ),
            kind: body
                .kind
                .or(existing.as_ref().map(|e| e.kind))
                .unwrap_or_default(),
            group: group.or(existing.as_ref().and_then(|e| e.group.clone())),
            guide: body
                .guide
                .or(existing.as_ref().map(|e| e.guide.clone()))
                .unwrap_or_default(),
            options: body
                .options
                .or(existing.as_ref().map(|e| e.options.clone()))
                .unwrap_or_default(),
        };
        tvmaster_db::upsert_channel(&db, &channel)?;
        (channel, existing)
    };

    let changed = existing
        .as_ref()
        .map(|e| programming_differs(e, &channel))
        .unwrap_or(true);
    if changed {
        apply_programming_change(&s, &channel, existing.is_some())?;
    }
    // A running channel took its picture and sound settings when it went on air:
    // take it off so the next viewer (or the always-on start below) uses the new ones.
    if existing
        .as_ref()
        .is_some_and(|e| restart_needed(e, &channel))
    {
        if let Some(p) = &s.playout {
            p.stop(&channel.id);
        }
    }
    if channel.always_on {
        s.start_always_on();
    }
    let status = if existing.is_none() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(channel)))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if let Some(playout) = &s.playout {
        playout.stop(&id);
    }
    if tvmaster_db::delete_channel(&s.db(), &id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

pub async fn get_lineup(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Lineup>> {
    Ok(Json(load(&s, &id)?.lineup))
}

/// Replace a channel's lineup (blocks, seasons, events, fill, catch-up).
pub async fn put_lineup(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(lineup): Json<Lineup>,
) -> ApiResult<Json<Lineup>> {
    lineup.validate().map_err(bad_lineup)?;
    check_ad_libraries(&s.db(), &lineup)?;
    let lineup = with_timezone(lineup);
    let mut channel = load(&s, &id)?;
    let changed = channel.lineup != lineup;
    let signoff_changed = channel.lineup.signoff != lineup.signoff;
    channel.lineup = lineup;
    tvmaster_db::upsert_channel(&s.db(), &channel)?;
    if signoff_changed {
        if let Some(p) = &s.playout {
            p.stop(&channel.id);
        }
        if channel.always_on {
            s.start_always_on();
        }
    }
    if changed {
        apply_programming_change(&s, &channel, true)?;
    }
    Ok(Json(channel.lineup))
}

/// Regenerate the not-yet-aired schedule now. Never skips or repeats episodes
/// that have aired. Works on locked (non-autopilot) channels too.
pub async fn reroll(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let channel = load(&s, &id)?;
    let items = autopilot::reroll(&s.db(), &channel, now_ms())?;
    s.emit(json!({ "type": "schedule.changed", "channel": channel.id }));
    Ok(Json(json!({ "items": items })))
}

/// Where each show (and pool, and event) is up to: `committed` episodes have
/// aired, `reserved` ones are on the schedule.
pub async fn cursors(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    load(&s, &id)?;
    let cursors = tvmaster_db::load_cursors(&s.db(), &id)?;
    Ok(Json(json!(cursors)))
}

#[derive(Deserialize)]
pub struct ScheduleQuery {
    hours: Option<u32>,
    /// `items`: every show act, commercial, bumper and filler instead of the guide view.
    detail: Option<String>,
}

fn kind_name(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Show => "show",
        ItemKind::Commercial => "commercial",
        ItemKind::Bumper => "bumper",
        ItemKind::Filler => "filler",
    }
}

/// Upcoming programmes (default next 6 hours, max 72). With `detail=items`,
/// the raw timeline including commercials and bumpers.
pub async fn schedule(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ScheduleQuery>,
) -> ApiResult<Json<Value>> {
    load(&s, &id)?;
    let hours = q.hours.unwrap_or(6).clamp(1, 72) as i64;
    let start = now_ms();
    let end = start + hours * 3_600_000;
    let db = s.db();
    if q.detail.as_deref() == Some("items") {
        let items = tvmaster_db::schedule_between(&db, &id, start, end)?;
        let mut titles: HashMap<i64, Option<String>> = HashMap::new();
        let mut out = Vec::with_capacity(items.len());
        for it in items {
            let title = match it.media_id {
                Some(m) => titles
                    .entry(m)
                    .or_insert_with(|| {
                        tvmaster_db::get_media(&db, m).ok().flatten().map(|r| {
                            r.title
                                .or(r.show)
                                .unwrap_or_else(|| r.path.rsplit('/').next().unwrap_or("").into())
                        })
                    })
                    .clone(),
                None => None,
            };
            out.push(json!({
                "kind": kind_name(it.kind),
                "bumper": it.bumper.map(|b| b.as_str()),
                "media_id": it.media_id,
                "title": title,
                "start_ms": it.start_ms,
                "end_ms": it.end_ms,
                "duration_ms": it.end_ms - it.start_ms,
                "in_ms": it.in_ms,
                "label": it.label,
            }));
        }
        return Ok(Json(Value::Array(out)));
    }
    Ok(Json(json!(programs(&db, &id, start, end)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::call;
    use crate::{router, AppState};
    use tvmaster_db::{add_library, upsert_media, NewMedia};

    fn app_with_media() -> (axum::Router, i64) {
        let state = AppState::in_memory();
        let lib_id = {
            let db = state.db();
            let lib = add_library(&db, "/tv", "shows").unwrap();
            for (i, (show, ep)) in [("Alpha", 1), ("Alpha", 2), ("Beta", 1)].iter().enumerate() {
                upsert_media(
                    &db,
                    &NewMedia {
                        library_id: lib.id,
                        path: format!("/tv/{i}.mp4"),
                        show: Some(show.to_string()),
                        season: Some(1),
                        episode: Some(*ep),
                        title: Some(format!("{show} ep {ep}")),
                        duration_ms: 20 * 60 * 1000,
                        acodec: Some("aac".into()),
                        vcodec: Some("h264".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            lib.id
        };
        (router(state), lib_id)
    }

    const ALPHA_ALL_DAY: &str = r#"{"blocks":[{"name":"All","start":"00:00","end":"24:00","source":{"type":"show","show":"Alpha"}}]}"#;

    #[tokio::test]
    async fn create_validate_and_list_with_now_next() {
        let (app, lib) = app_with_media();
        let body = format!(
            r#"{{"id":"retro","number":4,"name":"Retro TV","strategy":"sequential","library_ids":[{lib}]}}"#
        );
        let (status, ch) = call(&app, "POST", "/api/v1/channels", &body).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(ch["number"], 4);

        let (_, list) = call(&app, "GET", "/api/v1/channels", "").await;
        assert_eq!(list[0]["id"], "retro");
        assert!(list[0]["now"]["title"].is_string());
        assert!(list[0]["next"]["title"].is_string());
        assert_eq!(list[0]["on_air"], false);

        // 3 x 20 min of programming -> 6 hours of schedule is 18 items.
        let (_, sched) = call(&app, "GET", "/api/v1/channels/retro/schedule?hours=6", "").await;
        assert!(sched.as_array().unwrap().len() >= 17);
    }

    #[tokio::test]
    async fn lineup_with_blocks_is_validated_scheduled_and_visible() {
        let (app, lib) = app_with_media();
        let bad = format!(
            r#"{{"id":"a","number":1,"name":"A","library_ids":[{lib}],"lineup":{{"timezone":"Nope/Land"}}}}"#
        );
        let (st, err) = call(&app, "POST", "/api/v1/channels", &bad).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(err["error"].as_str().unwrap().contains("timezone"));

        let good = format!(
            r#"{{"id":"a","number":1,"name":"A","strategy":"block_grid","library_ids":[{lib}],"lineup":{ALPHA_ALL_DAY}}}"#
        );
        assert_eq!(
            call(&app, "POST", "/api/v1/channels", &good).await.0,
            StatusCode::CREATED
        );
        let (_, sched) = call(&app, "GET", "/api/v1/channels/a/schedule?hours=3", "").await;
        let sched = sched.as_array().unwrap();
        assert!(sched.len() >= 5);
        for p in sched {
            assert_eq!(p["show"], "Alpha", "the all-day block airs Alpha only");
            assert_eq!(p["label"], "All");
            // 20 minute episodes occupy 30 minute slots; the guide shows the full slot
            assert_eq!(
                p["end_ms"].as_i64().unwrap() - p["start_ms"].as_i64().unwrap(),
                30 * 60_000
            );
            assert_eq!(
                p["runtime_end_ms"].as_i64().unwrap() - p["start_ms"].as_i64().unwrap(),
                20 * 60_000
            );
        }
        let (st, lineup) = call(&app, "GET", "/api/v1/channels/a/lineup", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(lineup["blocks"][0]["name"], "All");
    }

    #[tokio::test]
    async fn put_lineup_rerolls_the_future_without_burning_episodes() {
        let (app, lib) = app_with_media();
        let body = format!(
            r#"{{"id":"a","number":1,"name":"A","strategy":"block_grid","library_ids":[{lib}],"lineup":{ALPHA_ALL_DAY}}}"#
        );
        call(&app, "POST", "/api/v1/channels", &body).await;
        let (_, before) = call(&app, "GET", "/api/v1/channels/a/schedule?hours=24", "").await;

        let beta = r#"{"blocks":[{"name":"Beta","start":"00:00","end":"24:00","source":{"type":"show","show":"Beta"}}]}"#;
        let (st, _) = call(&app, "PUT", "/api/v1/channels/a/lineup", beta).await;
        assert_eq!(st, StatusCode::OK);
        let (_, after) = call(&app, "GET", "/api/v1/channels/a/schedule?hours=24", "").await;
        // the slot on air (locked) is unchanged; later slots are now Beta
        assert_eq!(before[0], after[0]);
        assert_eq!(after.as_array().unwrap().last().unwrap()["show"], "Beta");

        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/channels/a/lineup",
            r#"{"grid_minutes":7}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn manual_lock_keeps_the_schedule_until_you_reroll() {
        let (app, lib) = app_with_media();
        let body = format!(
            r#"{{"id":"a","number":1,"name":"A","strategy":"block_grid","autopilot":false,"library_ids":[{lib}],"lineup":{ALPHA_ALL_DAY}}}"#
        );
        call(&app, "POST", "/api/v1/channels", &body).await;
        let beta = r#"{"blocks":[{"name":"Beta","start":"00:00","end":"24:00","source":{"type":"show","show":"Beta"}}]}"#;
        call(&app, "PUT", "/api/v1/channels/a/lineup", beta).await;
        let (_, locked) = call(&app, "GET", "/api/v1/channels/a/schedule?hours=24", "").await;
        assert!(
            locked
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["show"] == "Alpha"),
            "manual lock: edits do not touch the schedule"
        );
        let (st, r) = call(&app, "POST", "/api/v1/channels/a/reroll", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(r["items"].as_i64().unwrap() > 0);
        let (_, now) = call(&app, "GET", "/api/v1/channels/a/schedule?hours=24", "").await;
        assert_eq!(now.as_array().unwrap().last().unwrap()["show"], "Beta");
    }

    /// A server with 3 shows' episodes, 30 commercials and 2 bumper clips.
    fn app_with_ads() -> (AppState, axum::Router, i64, i64, i64) {
        let state = AppState::in_memory();
        let (shows, ads, clips) = {
            let db = state.db();
            let shows = add_library(&db, "/tv", "shows").unwrap();
            let ads = add_library(&db, "/ads", "commercials").unwrap();
            let clips = add_library(&db, "/clips", "bumpers").unwrap();
            for i in 0..6 {
                upsert_media(
                    &db,
                    &NewMedia {
                        library_id: shows.id,
                        path: format!("/tv/Alpha S01E0{i}.mp4"),
                        show: Some("Alpha".into()),
                        season: Some(1),
                        episode: Some(i),
                        title: Some(format!("Ep {i}")),
                        duration_ms: 22 * 60_000,
                        vcodec: Some("h264".into()),
                        acodec: Some("aac".into()),
                        breaks_json: Some(
                            r#"{"src":"black","points":[330000,660000,990000]}"#.into(),
                        ),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            for i in 0..30 {
                upsert_media(
                    &db,
                    &NewMedia {
                        library_id: ads.id,
                        path: format!("/ads/{i}.mp4"),
                        duration_ms: [15_000, 30_000, 30_000, 45_000, 60_000][i % 5],
                        year: Some(1985 + (i % 5) as i64),
                        vcodec: Some("h264".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            for i in 0..2 {
                upsert_media(
                    &db,
                    &NewMedia {
                        library_id: clips.id,
                        path: format!("/clips/{i}.mp4"),
                        duration_ms: 6_000 + 2_000 * i as i64,
                        vcodec: Some("h264".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            (shows.id, ads.id, clips.id)
        };
        (state.clone(), router(state), shows, ads, clips)
    }

    #[tokio::test]
    async fn commercials_and_bumpers_fill_each_slot_and_show_in_the_guide() {
        let (state, app, shows, ads, _clips) = app_with_ads();
        let body = format!(
            r#"{{"id":"a","number":1,"name":"Retro TV","strategy":"block_grid","library_ids":[{shows}],
               "lineup":{{"grid_minutes":30,"ads":{{"library_ids":[{ads}]}},
                          "blocks":[{{"name":"All","start":"00:00","end":"24:00","source":{{"type":"show","show":"Alpha"}}}}]}}}}"#
        );
        let (st, _) = call(&app, "POST", "/api/v1/channels", &body).await;
        assert_eq!(st, StatusCode::CREATED);

        let (_, items) = call(
            &app,
            "GET",
            "/api/v1/channels/a/schedule?hours=3&detail=items",
            "",
        )
        .await;
        let items = items.as_array().unwrap();
        let kinds = |k: &str| items.iter().filter(|i| i["kind"] == k).count();
        assert!(kinds("commercial") > 10, "commercials air");
        assert!(kinds("bumper") > 10, "bumpers air");
        assert!(kinds("show") > 10, "shows air in acts");
        for w in items.windows(2) {
            assert_eq!(w[0]["end_ms"], w[1]["start_ms"], "contiguous");
        }
        // acts split at the detected break points (about 5:30, 11:00, 16:30)
        let act2 = items
            .iter()
            .find(|i| i["kind"] == "show" && i["in_ms"].as_u64().unwrap() > 0)
            .expect("a later act");
        assert!(
            [330_000, 660_000, 990_000].contains(&act2["in_ms"].as_u64().unwrap()),
            "{act2}"
        );

        // the guide folds acts, ads and bumpers into 30-minute slots
        let (_, guide) = call(&app, "GET", "/api/v1/channels/a/schedule?hours=3", "").await;
        for p in guide.as_array().unwrap() {
            assert_eq!(
                p["end_ms"].as_i64().unwrap() - p["start_ms"].as_i64().unwrap(),
                30 * 60_000
            );
            // the last act ends after the show's 22 minutes (mid-show breaks come
            // first) and before the slot ends (the final break comes after)
            let (start, run_end, end) = (
                p["start_ms"].as_i64().unwrap(),
                p["runtime_end_ms"].as_i64().unwrap(),
                p["end_ms"].as_i64().unwrap(),
            );
            assert!(run_end - start > 22 * 60_000 && run_end < end, "{p}");
        }

        // what playout sees: up-next resolves from the live schedule, with local times
        let ch = tvmaster_db::get_channel(&state.db(), "a").unwrap().unwrap();
        let source = state.channel_spec(&ch).source;
        let now = crate::plan::now_ms();
        let up = source.upcoming("a", now, 3);
        assert_eq!(up.items.len(), 3);
        assert!(up.items[0].0.ends_with("AM") || up.items[0].0.ends_with("PM"));
        assert_eq!(up.items[0].1, "Alpha");
    }

    #[tokio::test]
    async fn ad_libraries_must_exist_and_be_the_right_kind() {
        let (_state, app, shows, ads, clips) = app_with_ads();
        let post = |ads_json: String| {
            let app = app.clone();
            async move {
                let body = format!(
                    r#"{{"id":"a","number":1,"name":"A","library_ids":[{shows}],"lineup":{{"ads":{ads_json}}}}}"#
                );
                call(&app, "POST", "/api/v1/channels", &body).await
            }
        };
        assert_eq!(
            post(format!(r#"{{"library_ids":[{shows}]}}"#)).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            post(format!(r#"{{"bumper_library_ids":[{ads}]}}"#)).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            post(r#"{"library_ids":[999]}"#.into()).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            post(r#"{"pod_seconds":3}"#.into()).await.0,
            StatusCode::BAD_REQUEST
        );
        let ok = post(format!(
            r#"{{"library_ids":[{ads}],"bumper_library_ids":[{clips}],"years":[1985,1987]}}"#
        ))
        .await;
        assert_eq!(ok.0, StatusCode::CREATED, "{:?}", ok.1);
        // ads can be switched on later with PUT, and the future is re-rolled
        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/channels/a/lineup",
            &format!(r#"{{"ads":{{"library_ids":[{shows}]}}}}"#),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn only_commercials_from_the_chosen_years_air() {
        let (state, app, shows, ads, _clips) = app_with_ads();
        let body = format!(
            r#"{{"id":"a","number":1,"name":"A","library_ids":[{shows}],
               "lineup":{{"grid_minutes":30,"ads":{{"library_ids":[{ads}],"years":[1985,1986]}}}}}}"#
        );
        call(&app, "POST", "/api/v1/channels", &body).await;
        let (_, items) = call(
            &app,
            "GET",
            "/api/v1/channels/a/schedule?hours=6&detail=items",
            "",
        )
        .await;
        let db = state.db();
        let mut n = 0;
        for i in items
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["kind"] == "commercial")
        {
            let m = tvmaster_db::get_media(&db, i["media_id"].as_i64().unwrap())
                .unwrap()
                .unwrap();
            assert!(matches!(m.year, Some(1985 | 1986)), "{m:?}");
            n += 1;
        }
        assert!(n > 5);
    }

    #[tokio::test]
    async fn a_guide_channel_lists_every_channel_and_needs_no_schedule() {
        let (state, app, shows, _ads, _clips) = app_with_ads();
        for (id, n, g) in [("retro", 4, "Sitcoms"), ("toons", 5, "Kids")] {
            let body = format!(
                r#"{{"id":"{id}","number":{n},"name":"{id} tv","group":"{g}","library_ids":[{shows}],"strategy":"sequential"}}"#
            );
            assert_eq!(
                call(&app, "POST", "/api/v1/channels", &body).await.0,
                StatusCode::CREATED
            );
        }
        let g = r#"{"id":"guide","number":3,"name":"Retro","kind":"guide","always_on":false,
                    "guide":{"hours":3,"ticker":["Welcome to Retro TV"]}}"#;
        let (st, ch) = call(&app, "POST", "/api/v1/channels", g).await;
        assert_eq!(st, StatusCode::CREATED, "{ch}");
        assert_eq!(ch["kind"], "guide");
        assert_eq!(ch["guide"]["hours"], 3);

        // no schedule is generated for a guide channel
        assert!(tvmaster_db::schedule_last_end(&state.db(), "guide")
            .unwrap()
            .is_none());
        let (_, all) = call(&app, "GET", "/api/v1/channels", "").await;
        assert_eq!(all.as_array().unwrap().len(), 3);

        // what it draws: every programming channel, in number order, with live programmes
        let now = crate::plan::now_ms();
        let data = crate::plan::build_guide(&state.db(), "guide", now).unwrap();
        assert_eq!(data.title, "Retro GUIDE");
        let numbers: Vec<u32> = data.rows.iter().map(|r| r.number).collect();
        assert_eq!(numbers, vec![4, 5], "the guide does not list itself");
        assert!(data.rows.iter().all(|r| r.cells.len() > 3));
        assert_eq!(data.marks.len(), 6, "a mark every half hour for 3 hours");
        assert_eq!(data.window_ms, 3 * 3_600_000);
        assert!(data.window_start_ms <= now && now - data.window_start_ms < 30 * 60_000);
        assert!(data.ticker.contains("Welcome to Retro TV"));
        assert!(data.ticker.contains("NOW ON 4: Alpha"));
        assert!(data.clock.ends_with("AM") || data.clock.ends_with("PM"));

        // playlist: groups and the guide; XMLTV: a programme for the guide
        let (_, m3u) = crate::test_util::call_raw(&app, "GET", "/iptv/playlist.m3u", "").await;
        assert!(m3u.contains("group-title=\"Sitcoms\"") && m3u.contains("group-title=\"Kids\""));
        assert!(m3u.contains("tvg-chno=\"3\""));
        assert!(
            m3u.contains("group-title=\"TVMaster\""),
            "ungrouped channels keep the default"
        );
        let (_, xml) = crate::test_util::call_raw(&app, "GET", "/iptv/guide.xml", "").await;
        assert!(xml.contains("<title>Retro Guide</title>"));
    }

    #[tokio::test]
    async fn guide_and_group_settings_are_validated() {
        let (_state, app, shows, ads, clips) = app_with_ads();
        let post = |body: String| {
            let app = app.clone();
            async move { call(&app, "POST", "/api/v1/channels", &body).await }
        };
        assert_eq!(
            post(r#"{"id":"g","number":3,"name":"G","kind":"guide","guide":{"hours":9}}"#.into())
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            post(format!(r#"{{"id":"g","number":3,"name":"G","kind":"guide","guide":{{"music_library_ids":[{shows}]}}}}"#)).await.0,
            StatusCode::BAD_REQUEST,
            "music libraries only"
        );
        assert_eq!(
            post(format!(
                r#"{{"id":"g","number":3,"name":"G","group":"{}"}}"#,
                "x".repeat(41)
            ))
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let _ = (ads, clips);
        // a group is trimmed, kept on update, and can be changed
        let (_, c) = post(r#"{"id":"a","number":1,"name":"A","group":"  Movies  "}"#.into()).await;
        assert_eq!(c["group"], "Movies");
        let (_, c) = post(r#"{"id":"a","number":1,"name":"A2"}"#.into()).await;
        assert_eq!(c["group"], "Movies", "an update without a group keeps it");
        let (_, c) = post(r#"{"id":"a","number":1,"name":"A2","group":"Kids"}"#.into()).await;
        assert_eq!(c["group"], "Kids");
    }

    #[tokio::test]
    async fn lineups_without_a_time_zone_get_the_servers() {
        let (app, lib) = app_with_media();
        let body = format!(
            r#"{{"id":"a","number":1,"name":"A","library_ids":[{lib}],"lineup":{{"grid_minutes":30}}}}"#
        );
        let (_, ch) = call(&app, "POST", "/api/v1/channels", &body).await;
        let tz = ch["lineup"]["timezone"].as_str().unwrap();
        assert!(!tz.is_empty(), "a blank zone is filled in on save");
        assert!(Zone::parse(tz).is_some());
        // an explicit zone is kept, and a PUT without one is filled in too
        let (_, l) = call(
            &app,
            "PUT",
            "/api/v1/channels/a/lineup",
            r#"{"timezone":"Asia/Tokyo"}"#,
        )
        .await;
        assert_eq!(l["timezone"], "Asia/Tokyo");
        let (_, l) = call(&app, "PUT", "/api/v1/channels/a/lineup", r#"{}"#).await;
        assert!(!l["timezone"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cursors_endpoint_shows_progress() {
        let (app, lib) = app_with_media();
        let body = format!(
            r#"{{"id":"a","number":1,"name":"A","strategy":"block_grid","library_ids":[{lib}],"lineup":{ALPHA_ALL_DAY}}}"#
        );
        call(&app, "POST", "/api/v1/channels", &body).await;
        let (st, cur) = call(&app, "GET", "/api/v1/channels/a/cursors", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(
            cur["show:alpha"]["reserved"].as_u64().unwrap() > 100,
            "72h of slots reserved"
        );
        assert_eq!(
            call(&app, "GET", "/api/v1/channels/nope/cursors", "")
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn rejects_bad_input_and_duplicate_numbers() {
        let (app, lib) = app_with_media();
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"Bad Id","number":1,"name":"x"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"a","number":0,"name":"x"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"a","number":1,"name":"x","library_ids":[999]}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);

        let ok = format!(r#"{{"id":"a","number":1,"name":"A","library_ids":[{lib}]}}"#);
        assert_eq!(
            call(&app, "POST", "/api/v1/channels", &ok).await.0,
            StatusCode::CREATED
        );
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"b","number":1,"name":"B"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn omitted_fields_keep_their_values_on_update() {
        let (app, lib) = app_with_media();
        let create = format!(
            r#"{{"id":"a","number":1,"name":"A","strategy":"chaos","chaos":40,"library_ids":[{lib}]}}"#
        );
        call(&app, "POST", "/api/v1/channels", &create).await;
        let (_, c) = call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"a","number":1,"name":"Renamed"}"#,
        )
        .await;
        assert_eq!(c["library_ids"], serde_json::json!([lib]), "libraries kept");
        assert_eq!(c["chaos"], 40);
        assert_eq!(c["strategy"], "chaos");
        // but an explicit value replaces them
        let (_, c) = call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"a","number":1,"name":"Renamed","library_ids":[],"chaos":0}"#,
        )
        .await;
        assert_eq!(c["library_ids"], serde_json::json!([]));
        assert_eq!(c["chaos"], 0);
    }

    #[tokio::test]
    async fn update_keeps_epoch_and_seed() {
        let (app, lib) = app_with_media();
        let create = format!(
            r#"{{"id":"a","number":1,"name":"A","strategy":"shuffle","library_ids":[{lib}]}}"#
        );
        let (_, first) = call(&app, "POST", "/api/v1/channels", &create).await;
        let rename = format!(r#"{{"id":"a","number":1,"name":"Renamed","library_ids":[{lib}]}}"#);
        let (st, second) = call(&app, "POST", "/api/v1/channels", &rename).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(second["name"], "Renamed");
        assert_eq!(second["epoch_ms"], first["epoch_ms"]);
        assert_eq!(second["seed"], first["seed"]);
        assert_eq!(second["strategy"], "shuffle");
    }

    #[tokio::test]
    async fn delete_and_missing() {
        let (app, _) = app_with_media();
        call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"a","number":1,"name":"A"}"#,
        )
        .await;
        assert_eq!(
            call(&app, "DELETE", "/api/v1/channels/a", "").await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&app, "DELETE", "/api/v1/channels/a", "").await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(&app, "GET", "/api/v1/channels/a", "").await.0,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn picture_and_sound_options_are_saved_validated_and_kept_across_edits() {
        let (app, lib) = app_with_media();
        let create = format!(
            r#"{{"id":"anime","number":7,"name":"Anime","library_ids":[{lib}],"options":{{
                "audio":{{"languages":["jpn","eng"],"loudness":"broadcast"}},
                "subtitles":{{"mode":"foreign","languages":["eng"],"style":"box"}},
                "video":{{"aspect":"fill","quality":"1080","look":"crt"}},
                "overlay":{{"bug":{{"enabled":true,"text":"ANIME 7"}}}},
                "shows":{{"Alpha":{{"audio_languages":["eng"],"subtitles":"off"}}}}}}}}"#
        );
        let (st, c) = call(&app, "POST", "/api/v1/channels", &create).await;
        assert_eq!(st, StatusCode::CREATED, "{c}");
        assert_eq!(c["options"]["audio"]["languages"][0], "jpn");
        assert_eq!(c["options"]["video"]["quality"], "1080");

        // saved
        let (_, got) = call(&app, "GET", "/api/v1/channels/anime", "").await;
        assert_eq!(got["options"]["subtitles"]["mode"], "foreign");
        assert_eq!(got["options"]["shows"]["Alpha"]["subtitles"], "off");

        // an edit that does not mention options keeps them
        let rename =
            format!(r#"{{"id":"anime","number":7,"name":"Anime Night","library_ids":[{lib}]}}"#);
        let (st, c) = call(&app, "POST", "/api/v1/channels", &rename).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(c["options"]["video"]["look"], "crt");

        // nonsense is refused with a reason
        for bad in [
            r#""options":{"audio":{"languages":["en;rm -rf /"]}}"#,
            r#""options":{"video":{"fps":900}}"#,
            r#""options":{"overlay":{"bug":{"opacity":3}}}"#,
            r#""options":{"video":{"aspect":"sideways"}}"#,
        ] {
            let body = format!(r#"{{"id":"anime","number":7,"name":"Anime",{bad}}}"#);
            let (st, _) = call(&app, "POST", "/api/v1/channels", &body).await;
            // out-of-range values are 400 with a reason; an unknown word is refused while parsing (422)
            assert!(
                st == StatusCode::BAD_REQUEST || st == StatusCode::UNPROCESSABLE_ENTITY,
                "{bad}: {st}"
            );
        }
    }

    #[tokio::test]
    async fn sign_off_hours_are_part_of_the_lineup_and_validated() {
        let (app, lib) = app_with_media();
        let ok = format!(
            r#"{{"id":"night","number":8,"name":"Night","library_ids":[{lib}],"lineup":{{"timezone":"UTC","signoff":{{"from":"02:00","to":"05:00","style":"static"}}}}}}"#
        );
        let (st, c) = call(&app, "POST", "/api/v1/channels", &ok).await;
        assert_eq!(st, StatusCode::CREATED, "{c}");
        assert_eq!(c["lineup"]["signoff"]["style"], "static");
        let bad = ok
            .replace(r#""to":"05:00""#, r#""to":"02:00""#)
            .replace("night", "night2")
            .replace("8,", "9,");
        let (st, _) = call(&app, "POST", "/api/v1/channels", &bad).await;
        assert_eq!(
            st,
            StatusCode::BAD_REQUEST,
            "a window with no length is refused"
        );
    }

    #[tokio::test]
    async fn shows_report_the_languages_found_in_their_files() {
        let state = AppState::in_memory();
        {
            let db = state.db();
            let lib = add_library(&db, "/tv", "shows").unwrap();
            let tracks = tvmaster_core::Tracks {
                audio: vec![
                    tvmaster_core::audio_track(1, "jpn", "aac", 2, "", true, false, false),
                    tvmaster_core::audio_track(2, "eng", "aac", 2, "", false, false, false),
                ],
                subs: vec![tvmaster_core::sub_track(
                    3, "eng", "subrip", "", false, false, false,
                )],
                external: vec![tvmaster_core::ExternalSub {
                    path: "/tv/x.fr.srt".into(),
                    lang: "fr".into(),
                    forced: false,
                    sdh: false,
                }],
                ..Default::default()
            };
            upsert_media(
                &db,
                &NewMedia {
                    library_id: lib.id,
                    path: "/tv/x.mkv".into(),
                    show: Some("Bebop".into()),
                    duration_ms: 1_200_000,
                    tracks_json: Some(tracks.to_json()),
                    ..Default::default()
                },
            )
            .unwrap();
            upsert_media(
                &db,
                &NewMedia {
                    library_id: lib.id,
                    path: "/tv/y.mkv".into(),
                    show: Some("Old Show".into()),
                    duration_ms: 1_200_000,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let app = router(state);
        let (_, shows) = call(&app, "GET", "/api/v1/shows", "").await;
        let bebop = shows
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "Bebop")
            .unwrap();
        assert_eq!(bebop["audio_languages"], serde_json::json!(["eng", "jpn"]));
        assert_eq!(
            bebop["subtitle_languages"],
            serde_json::json!(["eng", "fra"])
        );
        assert_eq!(bebop["files_with_tracks"], 1);
        let old = shows
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "Old Show")
            .unwrap();
        assert_eq!(
            old["files_with_tracks"], 0,
            "a file scanned before tracks were read says so"
        );
    }
}
