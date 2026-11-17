//! Day parts: named stretches of the broadcast day with their own rating limit, clean (no
//! commercial) hours and look. One list for the station; each part covers every channel or only
//! the ones it names.

use crate::{ApiError, ApiResult, AppState};
use axum::{extract::State, http::StatusCode, Json};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use tvmaster_core::{
    validate_parts, Channel, ChannelKind, Daypart, DaypartSet, GraphicsStyle, GuideTheme,
    PartBreaks, Rating, ThemeChoice, Zone,
};

const KEY: &str = "dayparts";

/// The station's day parts, in priority order.
pub(crate) fn load(db: &Connection) -> Vec<Daypart> {
    tvmaster_db::meta_get(db, KEY)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

/// The day parts in force for a channel, in the channel's own time zone.
pub(crate) fn for_channel(db: &Connection, channel: &Channel) -> DaypartSet {
    let parts = load(db);
    let zone = Zone::parse(&channel.lineup.timezone).unwrap_or_else(Zone::utc);
    DaypartSet::new(zone, &parts, &channel.id)
}

/// How each show is rated, for the scheduler (blank or unfamiliar ratings are left out: unrated).
pub(crate) fn ratings(db: &Connection) -> std::collections::BTreeMap<String, Rating> {
    tvmaster_db::list_show_info(db)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, i)| Rating::from_text(&i.rating).map(|r| (k.to_lowercase(), r)))
        .collect()
}

fn kids_look() -> ThemeChoice {
    ThemeChoice {
        theme: GuideTheme::Kids,
        style: GraphicsStyle::Modern,
        gentle: true,
        ..Default::default()
    }
}

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

/// Starting points a station can add with one click.
fn presets() -> Vec<(&'static str, Daypart)> {
    vec![
        (
            "Cartoons in the morning, with a child-friendly look and gentle alerts",
            Daypart {
                max_rating: Some(Rating::Kids),
                look: Some(kids_look()),
                ..part("Kids' morning", "06:00", "09:00")
            },
        ),
        (
            "Home from school: children's shows and the kids look",
            Daypart {
                days: vec![0, 1, 2, 3, 4],
                max_rating: Some(Rating::Kids),
                look: Some(kids_look()),
                ..part("After school", "15:00", "18:00")
            },
        ),
        (
            "Weekend cartoons",
            Daypart {
                days: vec![5, 6],
                max_rating: Some(Rating::Kids),
                look: Some(kids_look()),
                ..part("Saturday cartoons", "07:00", "12:00")
            },
        ),
        (
            "Nothing stronger than parental guidance over dinner",
            Daypart {
                max_rating: Some(Rating::Pg),
                ..part("Family evening", "18:00", "21:00")
            },
        ),
        (
            "After the children are in bed: a darker look, and no limit",
            Daypart {
                look: Some(ThemeChoice {
                    theme: GuideTheme::Midnight,
                    style: GraphicsStyle::Modern,
                    ..Default::default()
                }),
                ..part("Late night", "23:00", "06:00")
            },
        ),
    ]
}

/// Show counts under each part's limit: how many may air, how many are held back and how many have
/// no rating (which is why they may be held back).
fn check(db: &Connection, parts: &[Daypart]) -> Vec<Value> {
    let infos = tvmaster_db::list_show_info(db).unwrap_or_default();
    let shows = tvmaster_db::list_shows(db, &[]).unwrap_or_default();
    let rating_of = |name: &str| {
        infos
            .get(&name.to_lowercase())
            .and_then(|i| Rating::from_text(&i.rating))
    };
    parts
        .iter()
        .map(|p| {
            let (mut ok, mut held, mut unrated) = (0, 0, 0);
            for s in &shows {
                match (p.max_rating, rating_of(&s.name)) {
                    (None, _) => ok += 1,
                    (Some(max), Some(r)) if r <= max => ok += 1,
                    (Some(_), Some(_)) => held += 1,
                    (Some(_), None) => {
                        unrated += 1;
                        if p.allow_unrated {
                            ok += 1;
                        } else {
                            held += 1;
                        }
                    }
                }
            }
            json!({ "shows": shows.len(), "airing": ok, "held_back": held, "unrated": unrated })
        })
        .collect()
}

fn channels(db: &Connection) -> Vec<Value> {
    tvmaster_db::list_channels(db)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.kind == ChannelKind::Programming)
        .map(|c| json!({ "id": c.id, "number": c.number, "name": c.name }))
        .collect()
}

fn state(db: &Connection) -> Value {
    let parts = load(db);
    json!({
        "parts": parts,
        "check": check(db, &parts),
        "channels": channels(db),
        "ratings": Rating::ALL.iter().map(|(id, name, about)| json!({ "id": id, "name": name, "about": about })).collect::<Vec<_>>(),
        "presets": presets().into_iter().map(|(about, p)| json!({ "about": about, "part": p })).collect::<Vec<_>>(),
        "timezone": crate::station::effective_timezone(db),
    })
}

/// `GET /api/v1/dayparts`: the day parts, what each lets air, and the starting points on offer.
pub async fn get(State(s): State<AppState>) -> Json<Value> {
    Json(state(&s.db()))
}

#[derive(Deserialize)]
pub struct PutBody {
    parts: Vec<Daypart>,
}

/// `PUT /api/v1/dayparts`: set the day parts. Schedules are planned again so the limits show in
/// the guide, and channels on air pick up the looks.
pub async fn put(State(s): State<AppState>, Json(b): Json<PutBody>) -> ApiResult<Json<Value>> {
    validate_parts(&b.parts).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let mut parts = b.parts;
    for p in &mut parts {
        p.name = p.name.split_whitespace().collect::<Vec<_>>().join(" ");
        p.days.sort_unstable();
        p.days.dedup();
    }
    tvmaster_db::meta_set(
        &s.db(),
        KEY,
        &serde_json::to_string(&parts).unwrap_or_else(|_| "[]".into()),
    )?;
    replanned(&s)?;
    Ok(Json(state(&s.db())))
}

/// Plan every channel's future again and restart the streams that are on air.
pub(crate) fn replanned(s: &AppState) -> ApiResult<()> {
    let ids: Vec<String> = {
        let db = s.db();
        let now = crate::plan::now_ms();
        let channels = tvmaster_db::list_channels(&db)?;
        for c in channels
            .iter()
            .filter(|c| c.autopilot && c.kind == ChannelKind::Programming)
        {
            crate::autopilot::reroll(&db, c, now)?;
        }
        channels.into_iter().map(|c| c.id).collect()
    };
    crate::live::restart_running(s, &ids);
    Ok(())
}

/// Whether the station has any day parts at all.
pub(crate) fn any(db: &Connection) -> bool {
    !load(db).is_empty()
}

#[cfg(test)]
mod tests {
    use crate::test_util::call;
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn day_parts_are_saved_checked_and_offered() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let (st, v) = call(&app, "GET", "/api/v1/dayparts", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(v["parts"].as_array().unwrap().is_empty());
        assert!(v["presets"].as_array().unwrap().len() >= 4);
        assert_eq!(v["ratings"].as_array().unwrap().len(), 5);
        let kids = v["presets"][0]["part"].clone();
        assert_eq!(kids["name"], "Kids' morning");
        assert_eq!(kids["look"]["theme"], "kids");
        assert_eq!(kids["look"]["gentle"], true);
        let body = serde_json::json!({ "parts": [kids] }).to_string();
        let (st, v) = call(&app, "PUT", "/api/v1/dayparts", &body).await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["parts"][0]["max_rating"], "kids");
        assert_eq!(v["check"][0]["airing"], 0);
        let (st, v) = call(
            &app,
            "PUT",
            "/api/v1/dayparts",
            r#"{"parts":[{"name":"","start":"06:00","end":"09:00"}]}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("name"));
        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/dayparts",
            r#"{"parts":[{"name":"A","start":"6am","end":"09:00"}]}"#,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        // The earlier good list is still what is saved.
        let (_, v) = call(&app, "GET", "/api/v1/dayparts", "").await;
        assert_eq!(v["parts"].as_array().unwrap().len(), 1);
    }
}
