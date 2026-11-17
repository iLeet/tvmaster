//! First run: any app can walk a new station from nothing to a channel on air. The
//! steps are the same everywhere (add media, look, starter channels); this is what
//! they all ask the server.

use crate::{channels, ApiError, ApiResult, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

const DONE: &str = "setup_done";

fn done(db: &rusqlite::Connection) -> bool {
    tvmaster_db::meta_get(db, DONE).ok().flatten().is_some()
}

/// `GET /api/v1/setup`: where a new station stands.
pub async fn get(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let db = s.db();
    let libs = tvmaster_db::list_libraries(&db)?;
    let channels = tvmaster_db::list_channels(&db)?;
    let mut items = 0;
    for l in &libs {
        items += tvmaster_db::count_media(&db, l.id).unwrap_or(0);
    }
    let by_kind = |k: &str| libs.iter().filter(|l| l.kind == k).count();
    let station_folder = s.data_dir.as_ref().is_some_and(|d| {
        let root = d.join("media").to_string_lossy().replace('\\', "/");
        libs.iter()
            .any(|l| l.path.replace('\\', "/").starts_with(&root))
    });
    Ok(Json(json!({
        // Offered until a station has channels or someone says "not now".
        "needed": channels.is_empty() && !done(&db),
        "libraries": { "shows": by_kind("shows"), "commercials": by_kind("commercials"), "bumpers": by_kind("bumpers"), "music": by_kind("music") },
        "items": items,
        "shows": tvmaster_db::list_shows(&db, &[])?.len(),
        "channels": channels.len(),
        "station_folder": station_folder,
        "can_upload": s.data_dir.is_some(),
        "theme": crate::theme::station(&db),
    })))
}

#[derive(Deserialize)]
pub struct Starter {
    /// A guide channel listing what is on.
    #[serde(default = "yes")]
    guide: bool,
    /// One channel that shuffles every show.
    #[serde(default = "yes")]
    mix: bool,
}
fn yes() -> bool {
    true
}

/// `POST /api/v1/setup/starter`: make the first channels from the media added so far: a guide, and a
/// channel that shuffles every show. Channels that exist are left alone.
pub async fn starter(State(s): State<AppState>, Json(b): Json<Starter>) -> ApiResult<Json<Value>> {
    let (libs, taken) = {
        let db = s.db();
        let libs: Vec<i64> = tvmaster_db::list_libraries(&db)?
            .into_iter()
            .filter(|l| l.kind == "shows" && l.enabled)
            .map(|l| l.id)
            .collect();
        let taken: Vec<(String, u32)> = tvmaster_db::list_channels(&db)?
            .into_iter()
            .map(|c| (c.id, c.number))
            .collect();
        (libs, taken)
    };
    if b.mix && libs.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "add some shows first (a folder, or upload files), then make the channels",
        ));
    }
    let free = |from: u32, used: &[(String, u32)]| {
        (from..)
            .find(|n| !used.iter().any(|(_, u)| u == n))
            .unwrap_or(from)
    };
    let mut made = Vec::new();
    let mut used = taken.clone();
    let wanted: Vec<(&str, &str, Value)> = [
        (b.guide, "guide", "Guide", json!({ "kind": "guide" })),
        (b.mix, "mix", "Mix", json!({ "kind": "programming", "strategy": "shuffle", "run_out": "keep_going", "library_ids": libs })),
    ]
    .into_iter()
    .filter(|(on, ..)| *on)
    .map(|(_, id, name, extra)| (id, name, extra))
    .collect();
    for (id, name, extra) in wanted {
        if taken.iter().any(|(c, _)| c == id) {
            continue;
        }
        let number = free(if id == "guide" { 1 } else { 2 }, &used);
        used.push((id.to_string(), number));
        let mut body = json!({ "id": id, "number": number, "name": name });
        if let (Some(o), Some(e)) = (body.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        let input: channels::ChannelInput = serde_json::from_value(body)
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let _ = channels::upsert(State(s.clone()), Json(input)).await?;
        made.push(json!({ "id": id, "number": number, "name": name }));
    }
    tvmaster_db::meta_set(&s.db(), DONE, "1")?;
    Ok(Json(json!({ "channels": made })))
}

/// `POST /api/v1/setup/skip`: stop offering the setup.
pub async fn skip(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    tvmaster_db::meta_set(&s.db(), DONE, "1")?;
    Ok(Json(json!({ "done": true })))
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn a_new_station_is_offered_setup_and_gets_a_guide_and_a_mix_channel() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let (_, v) = call(&app, "GET", "/api/v1/setup", "").await;
        assert_eq!(v["needed"], true);
        // Nothing to air yet.
        let (st, _) = call(&app, "POST", "/api/v1/setup/starter", "{}").await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        {
            let db = state.db();
            let lib = tvmaster_db::add_library(&db, "/media/tv", "shows").unwrap();
            tvmaster_db::upsert_media(
                &db,
                &tvmaster_db::NewMedia {
                    library_id: lib.id,
                    path: "/media/tv/Cheers/a.mkv".into(),
                    show: Some("Cheers".into()),
                    duration_ms: 1_320_000,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let (st, v) = call(&app, "POST", "/api/v1/setup/starter", "{}").await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["channels"].as_array().unwrap().len(), 2);
        let (_, list) = call(&app, "GET", "/api/v1/channels", "").await;
        let numbers: Vec<i64> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["number"].as_i64().unwrap())
            .collect();
        assert_eq!(numbers, [1, 2]);
        let (_, v) = call(&app, "GET", "/api/v1/setup", "").await;
        assert_eq!(v["needed"], false);
        assert_eq!(v["channels"], 2);
        // Again changes nothing.
        let (_, again) = call(&app, "POST", "/api/v1/setup/starter", "{}").await;
        assert_eq!(again["channels"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn setup_can_be_put_off() {
        let app = router(AppState::in_memory());
        let (st, _) = call(&app, "POST", "/api/v1/setup/skip", "").await;
        assert_eq!(st, StatusCode::OK);
        let (_, v) = call(&app, "GET", "/api/v1/setup", "").await;
        assert_eq!(v["needed"], false);
    }
}
