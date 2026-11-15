use crate::plan::{build_plan, now_ms, program, Program};
use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tvmaster_core::{Channel, RunOutPolicy, Strategy};

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
    let (mut now, mut next) = (None, None);
    if let Some(plan) = build_plan(&s.db(), channel.clone())? {
        let mut slots = plan.timeline.iter_from(now_ms());
        now = slots.next().map(|sl| program(&sl, &plan.media));
        next = slots.next().map(|sl| program(&sl, &plan.media));
    }
    Ok(ChannelView {
        channel,
        on_air,
        now,
        next,
    })
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
    let channel = tvmaster_db::get_channel(&s.db(), &id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such channel"))?;
    Ok(Json(view(&s, channel)?))
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
    #[serde(default)]
    library_ids: Vec<i64>,
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
    let chaos = body.chaos.unwrap_or(0);
    if chaos > 100 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "chaos must be 0-100",
        ));
    }

    let (channel, created) = {
        let db = s.db();
        for lib in &body.library_ids {
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
            chaos,
            seed: body
                .seed
                .or(existing.as_ref().map(|e| e.seed))
                .unwrap_or(now as u64),
            epoch_ms: existing.as_ref().map(|e| e.epoch_ms).unwrap_or(now),
            always_on: body
                .always_on
                .or(existing.as_ref().map(|e| e.always_on))
                .unwrap_or(false),
            library_ids: body.library_ids,
        };
        tvmaster_db::upsert_channel(&db, &channel)?;
        (channel, existing.is_none())
    };

    // Settings changed: rebuild on next tune-in (or immediately if always-on).
    if let Some(playout) = &s.playout {
        playout.stop(&channel.id);
    }
    if channel.always_on {
        s.start_always_on();
    }
    let status = if created {
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
        Err(ApiError::new(StatusCode::NOT_FOUND, "no such channel"))
    }
}

#[derive(Deserialize)]
pub struct ScheduleQuery {
    hours: Option<u32>,
}

/// Upcoming programmes (default next 6 hours, max 48).
pub async fn schedule(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ScheduleQuery>,
) -> ApiResult<Json<Vec<Program>>> {
    let channel = tvmaster_db::get_channel(&s.db(), &id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such channel"))?;
    let hours = q.hours.unwrap_or(6).clamp(1, 48) as i64;
    let start = now_ms();
    let end = start + hours * 3_600_000;
    let Some(plan) = build_plan(&s.db(), channel)? else {
        return Ok(Json(Vec::new()));
    };
    let programs = plan
        .timeline
        .iter_from(start)
        .take_while(|slot| slot.start_ms < end)
        .map(|slot| program(&slot, &plan.media))
        .collect();
    Ok(Json(programs))
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
        assert!(sched.as_array().unwrap().len() >= 18);
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
}
