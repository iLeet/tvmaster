//! What the station calls its shows: rename a detected show, put several
//! folders together as one show, number a folder's season, and describe a show.

use crate::{ApiError, ApiResult, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use tvmaster_db::ShowInfo;

fn bad(msg: &str) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, msg.to_string())
}

fn clean_title(t: &str) -> ApiResult<String> {
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() || t.chars().count() > 120 || t.chars().any(char::is_control) {
        return Err(bad("a show title is 1-120 characters"));
    }
    Ok(t)
}

/// After a rule changes: bring lineups and settings in line, then re-plan so the
/// new names show up in the guide.
fn applied(s: &AppState) -> ApiResult<usize> {
    let db = s.db();
    let changed = tvmaster_db::normalize_names(&db)?;
    let now = crate::plan::now_ms();
    for c in tvmaster_db::list_channels(&db)?
        .iter()
        .filter(|c| c.autopilot && c.kind == tvmaster_core::ChannelKind::Programming)
    {
        crate::autopilot::reroll(&db, c, now)?;
    }
    Ok(changed)
}

#[derive(Deserialize)]
pub struct FolderBody {
    /// The detected name (as listed under the show's folders).
    raw: String,
    /// What the station calls it; blank = leave the title as detected.
    #[serde(default)]
    title: Option<String>,
    /// The season this folder is (blank = whatever the files say).
    #[serde(default)]
    season: Option<i64>,
}

/// `PUT /api/v1/shows/folder`: call a detected show something else, or say which season a folder is.
pub async fn put_folder(
    State(s): State<AppState>,
    Json(b): Json<FolderBody>,
) -> ApiResult<Json<Value>> {
    if b.raw.trim().is_empty() {
        return Err(bad("which folder?"));
    }
    if b.season.is_some_and(|n| !(0..=99).contains(&n)) {
        return Err(bad("a season is 0-99"));
    }
    let title = match b.title.as_deref().filter(|t| !t.trim().is_empty()) {
        Some(t) => clean_title(t)?,
        None => b.raw.trim().to_string(),
    };
    tvmaster_db::set_name_rule(&s.db(), &b.raw, &title, b.season)?;
    let changed = applied(&s)?;
    Ok(Json(
        json!({ "raw": b.raw, "title": title, "season": b.season, "channels_updated": changed }),
    ))
}

#[derive(Deserialize)]
pub struct RawBody {
    raw: String,
}

/// `POST /api/v1/shows/folder/reset`: forget a rule, so the folder goes back to the automatic name.
pub async fn reset_folder(
    State(s): State<AppState>,
    Json(b): Json<RawBody>,
) -> ApiResult<Json<Value>> {
    let removed = tvmaster_db::delete_name_rule(&s.db(), &b.raw)?;
    applied(&s)?;
    Ok(Json(json!({ "removed": removed })))
}

#[derive(Deserialize)]
pub struct MergeBody {
    /// Shows (by title) to put together.
    shows: Vec<String>,
    /// What the combined show is called.
    title: String,
}

/// `POST /api/v1/shows/merge`: make several shows one. Each folder keeps its season.
pub async fn merge(State(s): State<AppState>, Json(b): Json<MergeBody>) -> ApiResult<Json<Value>> {
    let title = clean_title(&b.title)?;
    if b.shows.is_empty() {
        return Err(bad("pick the shows to put together"));
    }
    let want: std::collections::HashSet<String> =
        b.shows.iter().map(|x| x.trim().to_lowercase()).collect();
    let mut moved = 0;
    {
        let db = s.db();
        for show in tvmaster_db::list_shows(&db, &[])? {
            if !want.contains(&show.name.to_lowercase()) {
                continue;
            }
            for f in &show.folders {
                tvmaster_db::set_name_rule(&db, &f.raw, &title, f.season)?;
                moved += 1;
            }
        }
    }
    if moved == 0 {
        return Err(bad("none of those shows exist"));
    }
    let changed = applied(&s)?;
    Ok(Json(
        json!({ "title": title, "folders": moved, "channels_updated": changed }),
    ))
}

#[derive(Deserialize)]
pub struct InfoBody {
    show: String,
    #[serde(default)]
    info: ShowInfo,
}

/// `PUT /api/v1/shows/info`: a show's description, rating, tags and poster.
pub async fn put_info(
    State(s): State<AppState>,
    Json(b): Json<InfoBody>,
) -> ApiResult<Json<Value>> {
    let title = clean_title(&b.show)?;
    let i = &b.info;
    if i.description.chars().count() > 2000
        || i.rating.chars().count() > 20
        || i.tags.len() > 20
        || i.tags.iter().any(|t| t.chars().count() > 40)
        || i.poster
            .as_deref()
            .is_some_and(|p| p.len() > 500 || p.chars().any(char::is_control))
    {
        return Err(bad(
            "description up to 2000 characters, rating up to 20, up to 20 tags",
        ));
    }
    tvmaster_db::set_show_info(&s.db(), &title, &b.info)?;
    Ok(Json(json!({ "show": title, "info": b.info })))
}

#[derive(Deserialize)]
pub struct AutoBody {
    enabled: bool,
}

/// `PUT /api/v1/shows/auto-clean`: split "season N" off folder names by itself (on by default).
pub async fn put_auto_clean(
    State(s): State<AppState>,
    Json(b): Json<AutoBody>,
) -> ApiResult<Json<Value>> {
    tvmaster_db::set_auto_clean(&s.db(), b.enabled)?;
    applied(&s)?;
    Ok(Json(json!({ "enabled": b.enabled })))
}

/// `GET /api/v1/shows/names`: the automatic clean-up switch and the rules made by hand.
pub async fn get_names(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let db = s.db();
    Ok(Json(json!({
        "auto_clean": tvmaster_db::auto_clean_enabled(&db),
        "rules": tvmaster_db::list_name_rules(&db)?,
    })))
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    fn seed(state: &AppState) -> i64 {
        let db = state.db();
        let lib = tvmaster_db::add_library(&db, "/media/tv", "shows").unwrap();
        for (path, show, ep) in [
            ("/media/tv/Bleach season 01/a.mkv", "Bleach season 01", 1),
            ("/media/tv/Bleach season 01/b.mkv", "Bleach season 01", 2),
            ("/media/tv/Bleach season 02/a.mkv", "Bleach season 02", 1),
            ("/media/tv/Cheers/a.mkv", "Cheers", 1),
        ] {
            tvmaster_db::upsert_media(
                &db,
                &tvmaster_db::NewMedia {
                    library_id: lib.id,
                    path: path.into(),
                    show: Some(show.into()),
                    episode: Some(ep),
                    duration_ms: 1_320_000,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        lib.id
    }

    #[tokio::test]
    async fn folders_named_by_season_become_one_show_and_can_be_renamed_and_merged() {
        let state = AppState::in_memory();
        seed(&state);
        let app = router(state.clone());
        let (_, v) = call(&app, "GET", "/api/v1/shows", "").await;
        let names: Vec<&str> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["Bleach", "Cheers"],
            "season folders are cleaned up automatically"
        );
        let bleach = &v[0];
        assert_eq!(bleach["episodes"], 3);
        let seasons: Vec<i64> = bleach["folders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["season"].as_i64().unwrap())
            .collect();
        assert_eq!(seasons, [1, 2]);

        // Rename a show by hand.
        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/shows/folder",
            r#"{"raw":"Cheers","title":"Cheers (1982)"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (_, v) = call(&app, "GET", "/api/v1/shows", "").await;
        assert!(v
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "Cheers (1982)"));

        // Merge them into one.
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/shows/merge",
            r#"{"shows":["Bleach","Cheers (1982)"],"title":"Mixed bag"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (_, v) = call(&app, "GET", "/api/v1/shows", "").await;
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["name"], "Mixed bag");
        assert_eq!(v[0]["folders"].as_array().unwrap().len(), 3);

        // A bad request changes nothing.
        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/shows/folder",
            r#"{"raw":"Cheers","title":"   "}"#,
        )
        .await;
        assert_eq!(
            st,
            StatusCode::OK,
            "blank means leave the title as detected"
        );
        let (st, _) = call(
            &app,
            "POST",
            "/api/v1/shows/merge",
            r#"{"shows":["Nope"],"title":"X"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn lineups_and_positions_follow_a_show_to_its_new_name() {
        let state = AppState::in_memory();
        let lib = seed(&state);
        let app = router(state.clone());
        let lineup = r#"{"blocks":[{"name":"x","start":"20:00","end":"21:00","source":{"type":"show","show":"Cheers"}},
            {"name":"y","start":"21:00","end":"22:00","source":{"type":"pool","shows":["Cheers","Bleach season 01"]}}]}"#;
        let body = format!(
            r#"{{"id":"c1","number":3,"name":"C","strategy":"block_grid","run_out":"keep_going","library_ids":[{lib}],"lineup":{lineup}}}"#
        );
        let (st, _) = call(&app, "POST", "/api/v1/channels", &body).await;
        assert_eq!(st, StatusCode::CREATED, "channel made");
        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/shows/folder",
            r#"{"raw":"Cheers","title":"Norm's Bar"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (_, ch) = call(&app, "GET", "/api/v1/channels/c1", "").await;
        let blocks = ch["lineup"]["blocks"].as_array().unwrap();
        assert_eq!(blocks[0]["source"]["show"], "Norm's Bar");
        let pool: Vec<&str> = blocks[1]["source"]["shows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            pool,
            ["Norm's Bar", "Bleach"],
            "a detected name in an old lineup becomes the show's title"
        );
    }

    #[tokio::test]
    async fn a_show_keeps_its_description_and_the_automatic_cleanup_can_be_turned_off() {
        let state = AppState::in_memory();
        seed(&state);
        let app = router(state.clone());
        let (st, _) = call(&app, "PUT", "/api/v1/shows/info", r#"{"show":"Cheers","info":{"description":"A bar.","rating":"TV-PG","tags":["sitcom"]}}"#).await;
        assert_eq!(st, StatusCode::OK);
        let (_, v) = call(&app, "GET", "/api/v1/shows", "").await;
        let cheers = v
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "Cheers")
            .unwrap();
        assert_eq!(cheers["info"]["rating"], "TV-PG");
        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/shows/auto-clean",
            r#"{"enabled":false}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (_, v) = call(&app, "GET", "/api/v1/shows", "").await;
        assert_eq!(
            v.as_array().unwrap().len(),
            3,
            "with the clean-up off every folder is its own show"
        );
    }
}
