//! Station-wide settings and the controls a station owner reaches for: the
//! time zone every schedule is in, starting over, and the stream's on/off switch.

use crate::{autopilot, plan, ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tvmaster_core::{ChannelKind, Zone};

const TZ_KEY: &str = "timezone";

/// The zone new schedules are made in: the one the owner chose, else the TZ
/// environment variable, else the machine's own.
pub(crate) fn effective_timezone(db: &rusqlite::Connection) -> String {
    chosen_timezone(db).unwrap_or_else(crate::channels::system_timezone)
}

fn chosen_timezone(db: &rusqlite::Connection) -> Option<String> {
    tvmaster_db::meta_get(db, TZ_KEY)
        .ok()
        .flatten()
        .filter(|z| !z.is_empty() && Zone::parse(z).is_some())
}

fn timezone_state(s: &AppState) -> Value {
    let db = s.db();
    let chosen = chosen_timezone(&db);
    let env = std::env::var("TZ").ok().filter(|t| !t.trim().is_empty());
    let machine = iana_time_zone::get_timezone().ok();
    let zone = effective_timezone(&db);
    let source = if chosen.is_some() {
        "setting"
    } else if env.as_deref().map(|e| e.trim_start_matches(':')) == Some(zone.as_str()) {
        "environment"
    } else {
        "machine"
    };
    // Channels whose schedule is in some other zone than the station's.
    let elsewhere: Vec<Value> = tvmaster_db::list_channels(&db)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.kind == ChannelKind::Programming && c.lineup.timezone != zone)
        .map(|c| json!({ "id": c.id, "name": c.name, "zone": c.lineup.timezone }))
        .collect();
    json!({
        "zone": zone,
        "source": source,
        "setting": chosen,
        "environment": env,
        "machine": machine,
        "channels_elsewhere": elsewhere,
    })
}

/// `GET /api/v1/timezone`: the station's time zone and where it came from, and any channels set to another.
pub async fn get_timezone(State(s): State<AppState>) -> Json<Value> {
    Json(timezone_state(&s))
}

#[derive(Deserialize)]
pub struct TimezoneBody {
    /// The zone to use (`America/Toronto`); `null` goes back to automatic.
    #[serde(default)]
    zone: Option<String>,
    /// Also move every channel's schedule into that zone (its time slots keep their clock times).
    #[serde(default)]
    apply_to_channels: bool,
}

/// `PUT /api/v1/timezone`.
pub async fn put_timezone(
    State(s): State<AppState>,
    Json(body): Json<TimezoneBody>,
) -> ApiResult<Json<Value>> {
    let zone = body
        .zone
        .as_deref()
        .map(str::trim)
        .filter(|z| !z.is_empty());
    if let Some(z) = zone {
        if Zone::parse(z).is_none() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("'{z}' is not a time zone (try America/Toronto or Europe/Paris)"),
            ));
        }
    }
    {
        let db = s.db();
        tvmaster_db::meta_set(&db, TZ_KEY, zone.unwrap_or(""))?;
        if body.apply_to_channels {
            let target = effective_timezone(&db);
            let now = plan::now_ms();
            for mut ch in tvmaster_db::list_channels(&db)? {
                if ch.lineup.timezone == target {
                    continue;
                }
                ch.lineup.timezone = target.clone();
                tvmaster_db::upsert_channel(&db, &ch)?;
                if ch.kind == ChannelKind::Programming && ch.autopilot {
                    autopilot::reroll(&db, &ch, now)?;
                }
            }
        }
    }
    if body.apply_to_channels {
        for c in tvmaster_db::list_channels(&s.db())? {
            // The running encoders read the zone when a programme is chosen; a restart makes sure.
            if let Some(p) = &s.playout {
                p.stop(&c.id);
            }
            s.emit(json!({ "type": "schedule.changed", "channel": c.id }));
        }
    }
    Ok(Json(timezone_state(&s)))
}

#[derive(Deserialize)]
pub struct ResetBody {
    /// `schedule` (re-plan from now) or `progress` (every show starts again at its first episode, then re-plan).
    what: String,
}

/// `POST /api/v1/channels/{id}/reset`.
pub async fn reset(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ResetBody>,
) -> ApiResult<Json<Value>> {
    let db = s.db();
    let ch = tvmaster_db::get_channel(&db, &id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such channel"))?;
    if ch.kind != ChannelKind::Programming {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "only programming channels have a schedule",
        ));
    }
    match body.what.as_str() {
        "schedule" => {}
        "progress" => {
            tvmaster_db::save_cursors(&db, &ch.id, &Default::default())?;
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "what must be 'schedule' or 'progress'",
            ))
        }
    }
    let items = autopilot::reroll(&db, &ch, plan::now_ms())?;
    drop(db);
    s.emit(json!({ "type": "schedule.changed", "channel": ch.id }));
    Ok(Json(json!({ "reset": body.what, "items": items })))
}

#[derive(Deserialize)]
pub struct StreamBody {
    /// `start`, `stop` or `restart`.
    action: String,
}

/// `POST /api/v1/channels/{id}/stream`: take a channel's stream on or off air, or restart it.
pub async fn stream_control(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StreamBody>,
) -> ApiResult<Json<Value>> {
    let playout = s.playout.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ffmpeg is not available; streaming is disabled",
        )
    })?;
    if tvmaster_db::get_channel(&s.db(), &id)?.is_none() {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "no such channel"));
    }
    match body.action.as_str() {
        "stop" => playout.stop(&id),
        "start" | "restart" => {
            if body.action == "restart" {
                playout.stop(&id);
            }
            crate::stream::ensure_on_air(&s, &id, true)?;
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "action must be start, stop or restart",
            ))
        }
    }
    let on_air = playout.is_running(&id);
    s.emit(json!({ "type": "schedule.changed", "channel": id }));
    Ok(Json(json!({ "on_air": on_air })))
}

/// `POST /api/v1/server/restart`: stop the server cleanly. Docker, systemd and the Windows service
/// start it again by themselves; a server started by hand just stops.
pub async fn server_restart(State(s): State<AppState>) -> Json<Value> {
    tracing::warn!("restart requested through the API");
    let playout = s.playout.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        if let Some(p) = playout {
            p.stop_all();
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        std::process::exit(0);
    });
    Json(json!({ "restarting": true }))
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn the_station_time_zone_is_a_setting_that_new_channels_and_old_ones_can_follow() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let (st, v) = call(&app, "GET", "/api/v1/timezone", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(v["setting"].is_null());

        // A channel made before the setting lives in the machine's zone.
        let lib = tvmaster_db::add_library(&state.db(), "/x", "shows").unwrap();
        let body = format!(
            r#"{{"id":"a","number":4,"name":"A","library_ids":[{}],"lineup":{{"timezone":"Etc/UTC"}}}}"#,
            lib.id
        );
        call(&app, "POST", "/api/v1/channels", &body).await;

        let (st, v) = call(
            &app,
            "PUT",
            "/api/v1/timezone",
            r#"{"zone":"America/Toronto"}"#,
        )
        .await;
        assert_eq!(
            (st, v["zone"].as_str(), v["source"].as_str()),
            (StatusCode::OK, Some("America/Toronto"), Some("setting"))
        );
        assert_eq!(
            v["channels_elsewhere"][0]["id"], "a",
            "it is told which channels are in another zone"
        );
        let (_, sys) = call(&app, "GET", "/api/v1/system", "").await;
        assert_eq!(sys["timezone"], "America/Toronto");
        // New channels get it without being asked.
        call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"b","number":5,"name":"B"}"#,
        )
        .await;
        let (_, b) = call(&app, "GET", "/api/v1/channels/b", "").await;
        assert_eq!(b["lineup"]["timezone"], "America/Toronto");
        // Existing ones follow when asked.
        let (_, a) = call(&app, "GET", "/api/v1/channels/a", "").await;
        assert_eq!(
            a["lineup"]["timezone"], "Etc/UTC",
            "not changed behind your back"
        );
        let (_, v) = call(
            &app,
            "PUT",
            "/api/v1/timezone",
            r#"{"zone":"America/Toronto","apply_to_channels":true}"#,
        )
        .await;
        assert!(v["channels_elsewhere"].as_array().unwrap().is_empty());
        let (_, a) = call(&app, "GET", "/api/v1/channels/a", "").await;
        assert_eq!(a["lineup"]["timezone"], "America/Toronto");

        assert_eq!(
            call(
                &app,
                "PUT",
                "/api/v1/timezone",
                r#"{"zone":"Mars/Olympus"}"#
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        let (_, v) = call(&app, "PUT", "/api/v1/timezone", r#"{"zone":null}"#).await;
        assert!(
            v["setting"].is_null() && v["source"] != "setting",
            "back to automatic"
        );
    }

    #[tokio::test]
    async fn a_channels_progress_can_be_reset_without_touching_its_settings() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let lib = tvmaster_db::add_library(&state.db(), "/x", "shows").unwrap();
        let body = format!(
            r#"{{"id":"a","number":4,"name":"A","library_ids":[{}]}}"#,
            lib.id
        );
        call(&app, "POST", "/api/v1/channels", &body).await;
        let mut cur = tvmaster_db::load_cursors(&state.db(), "a").unwrap();
        cur.insert(
            "Show".into(),
            tvmaster_core::EpisodeCursor {
                reserved: 5,
                committed: 5,
            },
        );
        tvmaster_db::save_cursors(&state.db(), "a", &cur).unwrap();
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/channels/a/reset",
            r#"{"what":"progress"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert!(tvmaster_db::load_cursors(&state.db(), "a")
            .unwrap()
            .get("Show")
            .is_none_or(|c| c.committed == 0));
        assert_eq!(
            call(
                &app,
                "POST",
                "/api/v1/channels/a/reset",
                r#"{"what":"schedule"}"#
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            call(
                &app,
                "POST",
                "/api/v1/channels/a/reset",
                r#"{"what":"everything"}"#
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(
                &app,
                "POST",
                "/api/v1/channels/nope/reset",
                r#"{"what":"schedule"}"#
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
        let (_, a) = call(&app, "GET", "/api/v1/channels/a", "").await;
        assert_eq!(a["name"], "A");
    }
}
