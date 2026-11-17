//! The station theme: one look for the guide, station and break cards, the
//! stand-by screen, alerts, the now-playing card, the logo and the clock.
//! Set once here, and a channel may choose its own.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tvmaster_core::{ChannelOptions, GraphicsStyle, GuideColors, GuideConfig, GuideTheme, ThemeChoice};
use tvmaster_playout::{card_theme, palette_for};
use tvmaster_render::alert::{AlertLook, AlertStyle, AlertTheme, Severity};
use tvmaster_render::Scene;

const KEY: &str = "station_theme";

/// The station's theme (automatic colours, retro style until someone chooses).
pub(crate) fn station(db: &rusqlite::Connection) -> ThemeChoice {
    tvmaster_db::meta_get(db, KEY)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

/// The theme a channel uses: its own if it has one, else the station's.
pub(crate) fn effective(db: &rusqlite::Connection, options: &ChannelOptions) -> ThemeChoice {
    options.theme.clone().unwrap_or_else(|| station(db))
}

/// How an alert looks when it follows the theme: the classic pixel alert in the retro style, and
/// the smooth one in the theme's colours in the modern style.
pub(crate) fn alert_theme(choice: &ThemeChoice, channel_id: &str) -> AlertTheme {
    match choice.style {
        GraphicsStyle::Retro => AlertTheme::Retro,
        GraphicsStyle::Modern => {
            AlertTheme::Station(palette_for(choice.theme, &choice.colors, channel_id))
        }
    }
}

/// The themes on offer: id, name and what it is like.
const THEMES: &[(&str, &str, &str)] = &[
    ("auto", "Automatic", "Colours made from each channel, so every channel has its own look"),
    ("prevue", "Prevue", "Deep blue with yellow, the classic cable guide"),
    ("amber", "Amber", "Amber on black, like an old monitor"),
    ("phosphor", "Phosphor", "Green phosphor"),
    ("neon", "Neon", "Bright colours on black"),
    ("sunset", "Sunset", "Warm oranges and purples"),
    ("ocean", "Ocean", "Blues and teals"),
    ("slate", "Slate", "Cool grey with a warm accent"),
    ("paper", "Paper", "Light, like a printed listing"),
    ("midnight", "Midnight", "Deep navy with a cyan highlight"),
    ("graphite", "Graphite", "Charcoal with an orange highlight"),
    ("snow", "Snow", "Clean white with a blue highlight"),
    ("forest", "Forest", "Deep green with an amber highlight"),
    ("berry", "Berry", "Plum with a pink highlight"),
    ("mono", "Mono", "Black and white"),
    ("custom", "Custom", "Two colours of your choosing"),
];

fn rgb_hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn catalogue() -> Value {
    let list: Vec<Value> = THEMES
        .iter()
        .map(|(id, name, about)| {
            let theme: GuideTheme =
                serde_json::from_value(json!(id)).unwrap_or(GuideTheme::Auto);
            let p = palette_for(theme, &GuideColors::default(), "preview");
            json!({
                "id": id, "name": name, "about": about,
                "swatch": [rgb_hex(p.bg_top), rgb_hex(p.tick), rgb_hex(p.accent)],
            })
        })
        .collect();
    json!(list)
}

fn options() -> Value {
    json!({
        "themes": catalogue(),
        "styles": [
            { "id": "retro", "name": "Retro", "about": "Chunky pixel type and scanlines, like a broadcast from another era" },
            { "id": "modern", "name": "Modern", "about": "Smooth type, clean shapes and gentle motion" },
        ],
    })
}

/// `GET /api/v1/theme`: the station's theme and everything it can be.
pub async fn get(State(s): State<AppState>) -> Json<Value> {
    let current = station(&s.db());
    let mut v = options();
    v["station"] = json!(current);
    Json(v)
}

/// `PUT /api/v1/theme`: choose the station's theme. Channels that follow it pick it up within seconds.
pub async fn put(State(s): State<AppState>, Json(choice): Json<ThemeChoice>) -> ApiResult<Json<Value>> {
    choice
        .validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let json = serde_json::to_string(&choice).unwrap_or_else(|_| "{}".into());
    tvmaster_db::meta_set(&s.db(), KEY, &json)?;
    restart_following(&s);
    Ok(Json(json!(choice)))
}

/// Streams that are on air restart so the new look shows (channels with a theme of their own are left alone).
pub(crate) fn restart_following(s: &AppState) {
    let ids: Vec<String> = tvmaster_db::list_channels(&s.db())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.options.theme.is_none())
        .map(|c| c.id)
        .collect();
    crate::live::restart_running(s, &ids);
}

#[derive(Deserialize)]
pub struct PreviewBody {
    /// `card`, `bumper`, `guide`, `alert` or `bar`.
    #[serde(default = "d_what")]
    what: String,
    #[serde(default)]
    theme: ThemeChoice,
    #[serde(default)]
    name: String,
    #[serde(default)]
    number: Option<u32>,
    /// Which channel, so automatic colours match what will air.
    #[serde(default)]
    id: String,
    #[serde(default)]
    t: f32,
    /// For `bumper`: `brb`, `back_to`, `up_next`, `later_today` or `station_id`.
    #[serde(default)]
    scene: Option<String>,
}

fn d_what() -> String {
    "card".into()
}

fn png_of(c: &tvmaster_render::Canvas) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(c.w * c.h * 4);
    for px in c.px.as_chunks::<3>().0 {
        rgba.extend_from_slice(px);
        rgba.push(255);
    }
    tvmaster_render::png::rgba_png(c.w as u32, c.h as u32, &rgba)
}

/// `POST /api/v1/theme/preview`: one of the station's graphics in a theme, as a PNG. Nothing goes on air.
pub async fn preview(State(s): State<AppState>, Json(b): Json<PreviewBody>) -> ApiResult<Response> {
    b.theme
        .validate()
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let id = if b.id.is_empty() { "preview".to_string() } else { b.id.clone() };
    let name = if b.name.trim().is_empty() { "Retro TV".to_string() } else { b.name.chars().take(30).collect() };
    let number = b.number.unwrap_or(4);
    let t = b.t.clamp(0.0, 3600.0);
    let png: Vec<u8> = match b.what.as_str() {
        "card" => {
            let th = card_theme(&b.theme, &id);
            tokio::task::spawn_blocking(move || {
                tvmaster_render::overlay::station_card(&th, number, &name, "").png
            })
            .await
            .map_err(internal)?
        }
        "bumper" => {
            let th = card_theme(&b.theme, &id);
            let kind = b.scene.clone().unwrap_or_else(|| "up_next".into());
            tokio::task::spawn_blocking(move || {
                let show = "The Late Show".to_string();
                let scene = match kind.as_str() {
                    "brb" => Scene::Brb { name, show },
                    "back_to" => Scene::BackTo { name, show },
                    "station_id" => Scene::StationId { name },
                    "later_today" => Scene::LaterToday {
                        name,
                        evening: false,
                        items: vec![
                            ("8:30 PM".into(), "Taxi".into()),
                            ("9:00 PM".into(), show),
                            ("9:30 PM".into(), "Cheers".into()),
                        ],
                    },
                    _ => Scene::UpNext { name, title: show, time: "9:00 PM".into() },
                };
                png_of(&tvmaster_render::render(&scene, &th, t.max(1.6), 6.0))
            })
            .await
            .map_err(internal)?
        }
        "alert" | "bar" => {
            let look = AlertLook {
                theme: alert_theme(&b.theme, &id),
                severity: Severity::Emergency,
            };
            let style = if b.what == "bar" { AlertStyle::Bar } else { AlertStyle::Full };
            tokio::task::spawn_blocking(move || {
                png_of(&tvmaster_render::alert::alert_preview(
                    &look,
                    style,
                    "Emergency alert test",
                    "This is a test. No action is required.",
                    t,
                ))
            })
            .await
            .map_err(internal)?
        }
        "guide" => {
            let mut guide = GuideConfig::default();
            b.theme.apply_to_guide(&mut guide);
            crate::guide_preview::render_png(&s, &id, &name, &guide, t)
                .await
                .ok_or_else(|| internal(()))?
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "what must be card, bumper, guide, alert or bar",
            ))
        }
    };
    let mut res = png.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

fn internal<E>(_: E) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "could not draw that")
}

#[cfg(test)]
mod tests {
    use crate::test_util::{call, call_raw};
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn the_station_theme_is_saved_and_every_graphic_previews_in_it() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let (st, v) = call(&app, "GET", "/api/v1/theme", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["station"]["theme"], "auto");
        assert!(v["themes"].as_array().unwrap().len() >= 16);
        let (st, _) = call(&app, "PUT", "/api/v1/theme", r#"{"theme":"midnight","style":"modern"}"#).await;
        assert_eq!(st, StatusCode::OK);
        let (_, v) = call(&app, "GET", "/api/v1/theme", "").await;
        assert_eq!((v["station"]["theme"].as_str(), v["station"]["style"].as_str()), (Some("midnight"), Some("modern")));
        let (st, _) = call(&app, "PUT", "/api/v1/theme", r#"{"theme":"custom","colors":{"primary":"nope"}}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        for scene in ["brb", "back_to", "up_next", "later_today", "station_id"] {
            let body = format!(r#"{{"what":"bumper","scene":"{scene}","theme":{{"theme":"neon","style":"modern"}}}}"#);
            let (st, _) = call_raw(&app, "POST", "/api/v1/theme/preview", &body).await;
            assert_eq!(st, StatusCode::OK, "{scene}");
        }
        for what in ["card", "bumper", "guide", "alert", "bar"] {
            let body = format!(r#"{{"what":"{what}","theme":{{"theme":"snow","style":"modern"}}}}"#);
            let (st, png) = call_raw(&app, "POST", "/api/v1/theme/preview", &body).await;
            assert_eq!(st, StatusCode::OK, "{what}");
            assert!(png.starts_with("\u{fffd}PNG"), "{what} is a PNG");
        }
        let (st, _) = call_raw(&app, "POST", "/api/v1/theme/preview", r#"{"what":"nope"}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_channel_follows_the_station_theme_unless_it_has_its_own() {
        let state = AppState::in_memory();
        let db = state.db();
        tvmaster_db::meta_set(&db, "station_theme", r#"{"theme":"forest","style":"modern"}"#).unwrap();
        let mut opts = tvmaster_core::ChannelOptions::default();
        assert_eq!(super::effective(&db, &opts).theme, tvmaster_core::GuideTheme::Forest);
        opts.theme = Some(tvmaster_core::ThemeChoice { theme: tvmaster_core::GuideTheme::Amber, ..Default::default() });
        let own = super::effective(&db, &opts);
        assert_eq!((own.theme, own.style), (tvmaster_core::GuideTheme::Amber, tvmaster_core::GraphicsStyle::Retro));
    }
}
