//! Looks: a whole appearance in one choice. A look is the station theme (colours and style), the
//! screens (stand-by, up next, ...) and how the guides are laid out. A few come with the station
//! (Cable box, Liquid glass, Kids' TV...); you can save the current appearance as your own, take
//! one out as a file and bring one in.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tvmaster_core::{ChannelKind, GuideConfig, ThemeChoice};

const MINE: &str = "my_looks";

/// How a guide is laid out unless a look says otherwise: what every look starts from, so changing
/// from one look to another changes everything and leaves nothing of the last one behind.
fn base_guide() -> Value {
    json!({
        "layout": { "style": "grid", "rows": 6, "scroll": "page", "scanlines": true, "progress": false, "channel_numbers": true, "channel_names": true },
        "look": { "typeface": "pixel", "corners": "square", "text_size": "medium", "header_style": "bar", "ticker_position": "bottom", "cell_times": false, "now_marker": true, "glass": false, "background": { "style": "gradient", "image": null, "dim": 45 } },
        "pip": { "placement": "side_right", "size": "medium", "info": true },
    })
}

fn merge(a: &mut Value, b: &Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (a, b) => *a = b.clone(),
    }
}

struct Builtin {
    id: &'static str,
    name: &'static str,
    about: &'static str,
    theme: &'static str,
    style: &'static str,
    gentle: bool,
    guide: Value,
    /// (screen, gallery design id)
    screens: &'static [(&'static str, &'static str)],
}

fn builtins() -> Vec<Builtin> {
    let smooth = json!({ "look": { "typeface": "smooth" }, "layout": { "scanlines": false } });
    let with = |extra: Value| {
        let mut g = smooth.clone();
        merge(&mut g, &extra);
        g
    };
    vec![
        Builtin {
            id: "classic-cable",
            name: "Classic cable",
            about: "Deep blue and yellow, chunky pixel type and scanlines: the guide channel of old.",
            theme: "prevue",
            style: "retro",
            gentle: false,
            guide: json!({}),
            screens: &[],
        },
        Builtin {
            id: "cable-box",
            name: "Cable box, 2000s",
            about: "Blue programme cells with the one on now in orange, a light channel column, the live picture top right with a cream info card beside it.",
            theme: "cable",
            style: "modern",
            gentle: false,
            guide: with(json!({ "layout": { "rows": 5 }, "pip": { "placement": "top_right", "size": "medium", "info": true } })),
            screens: &[("up_next", "slide"), ("later_today", "list"), ("standby", "clock")],
        },
        Builtin {
            id: "liquid-glass",
            name: "Liquid glass",
            about: "Frosted, see-through panels over soft colour that drifts, with smooth type and rounded corners; the screens match.",
            theme: "glass",
            style: "modern",
            gentle: false,
            guide: with(json!({ "layout": { "rows": 5 }, "look": { "corners": "rounded", "glass": true, "header_style": "minimal", "background": { "style": "aurora" } }, "pip": { "placement": "top_right", "size": "medium", "info": true } })),
            screens: &[("standby", "glass"), ("signoff", "glass"), ("up_next", "glass"), ("later_today", "glass"), ("brb", "glass"), ("back_to", "glass"), ("station_id", "glass")],
        },
        Builtin {
            id: "kids-tv",
            name: "Kids' TV",
            about: "Bright, friendly colours, big rounded cells, gentle alerts and playful screens.",
            theme: "kids",
            style: "modern",
            gentle: true,
            guide: with(json!({ "layout": { "rows": 5 }, "look": { "corners": "rounded", "text_size": "large" } })),
            screens: &[("up_next", "big"), ("brb", "pulse"), ("standby", "drift")],
        },
        Builtin {
            id: "night-modern",
            name: "Night, modern",
            about: "Deep navy with a cyan highlight, smooth and rounded, with clean sliding screens.",
            theme: "midnight",
            style: "modern",
            gentle: false,
            guide: with(json!({ "look": { "corners": "rounded" } })),
            screens: &[("up_next", "slide"), ("later_today", "list")],
        },
        Builtin {
            id: "paper-listing",
            name: "Printed listing",
            about: "Light and calm, like the TV page of a newspaper.",
            theme: "paper",
            style: "modern",
            gentle: false,
            guide: with(json!({ "look": { "header_style": "minimal", "cell_times": true } })),
            screens: &[("up_next", "big"), ("later_today", "list")],
        },
        Builtin {
            id: "terminal",
            name: "Terminal",
            about: "Bright green on black in chunky pixel letters, with a scrolling stand-by notice.",
            theme: "terminal",
            style: "retro",
            gentle: false,
            guide: json!({}),
            screens: &[("standby", "marquee")],
        },
        Builtin {
            id: "sunset-retro",
            name: "Sunset VHS",
            about: "Warm oranges and purples with scanlines, like a tape from the eighties.",
            theme: "sunset",
            style: "retro",
            gentle: false,
            guide: json!({ "layout": { "progress": true } }),
            screens: &[],
        },
    ]
}

/// One look as the API shows it.
fn describe(
    id: &str,
    name: &str,
    about: &str,
    builtin: bool,
    theme: &ThemeChoice,
    guide: &Value,
    screens: &serde_json::Map<String, Value>,
) -> Value {
    // A whole guide setting to draw a thumbnail from: the look's layout over the defaults.
    let mut full = json!({});
    merge(&mut full, &base_guide());
    merge(&mut full, guide);
    full["theme"] = json!(theme.theme);
    full["colors"] = json!(theme.colors);
    json!({
        "id": id, "name": name, "about": about, "builtin": builtin,
        "theme": theme, "guide": guide, "preview_guide": full,
        "screens": screens,
    })
}

fn screens_of(pairs: &[(&str, &str)]) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    for (kind, preset) in pairs {
        if let Some(d) = crate::screens::preset_design(kind, preset) {
            m.insert(kind.to_string(), json!({ "preset": preset, "design": d }));
        }
    }
    m
}

fn mine(db: &rusqlite::Connection) -> Vec<Value> {
    tvmaster_db::meta_get(db, MINE)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<Vec<Value>>(&j).ok())
        .unwrap_or_default()
}

fn all(db: &rusqlite::Connection) -> Vec<Value> {
    let mut out: Vec<Value> = builtins()
        .iter()
        .map(|b| {
            let theme = ThemeChoice {
                theme: serde_json::from_value(json!(b.theme)).unwrap_or_default(),
                style: serde_json::from_value(json!(b.style)).unwrap_or_default(),
                gentle: b.gentle,
                ..Default::default()
            };
            describe(
                b.id,
                b.name,
                b.about,
                true,
                &theme,
                &b.guide,
                &screens_of(b.screens),
            )
        })
        .collect();
    for m in mine(db) {
        let theme: ThemeChoice = serde_json::from_value(m["theme"].clone()).unwrap_or_default();
        let screens = m["screens"].as_object().cloned().unwrap_or_default();
        out.push(describe(
            m["id"].as_str().unwrap_or(""),
            m["name"].as_str().unwrap_or("My look"),
            m["about"].as_str().unwrap_or(""),
            false,
            &theme,
            &m["guide"],
            &screens,
        ));
    }
    out
}

/// `GET /api/v1/looks`: the looks that come with the station and the ones you saved.
pub async fn get(State(s): State<AppState>) -> Json<Value> {
    Json(json!({ "looks": all(&s.db()) }))
}

#[derive(Deserialize, Default)]
pub struct ApplyBody {
    /// Change the station theme (colours and style). Default yes.
    #[serde(default = "yes")]
    theme: bool,
    /// Change the stand-by, sign-off and break screens. Default yes.
    #[serde(default = "yes")]
    screens: bool,
    /// Lay out every guide channel the way the look does. Default yes.
    #[serde(default = "yes")]
    guides: bool,
}

fn yes() -> bool {
    true
}

fn bad(e: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.into())
}

/// `POST /api/v1/looks/:id/apply`: put a look on the station.
pub async fn apply(
    State(s): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<ApplyBody>>,
) -> ApiResult<Json<Value>> {
    let b = body.map(|Json(b)| b).unwrap_or(ApplyBody {
        theme: true,
        screens: true,
        guides: true,
    });
    let look = all(&s.db())
        .into_iter()
        .find(|l| l["id"] == id.as_str())
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such look"))?;
    let theme: ThemeChoice =
        serde_json::from_value(look["theme"].clone()).map_err(|e| bad(e.to_string()))?;
    theme.validate().map_err(bad)?;
    {
        let db = s.db();
        if b.theme {
            tvmaster_db::meta_set(
                &db,
                "station_theme",
                &serde_json::to_string(&theme).unwrap_or_default(),
            )?;
        }
    }
    if b.screens {
        let designs = look["screens"].as_object().cloned().unwrap_or_default();
        crate::screens::set_station_all(&s, &designs)?;
    }
    if b.guides {
        let overlay = look["guide"].clone();
        let db = s.db();
        for mut c in tvmaster_db::list_channels(&db)?
            .into_iter()
            .filter(|c| c.kind == ChannelKind::Guide)
        {
            let mut cfg = json!(c.guide);
            merge(&mut cfg, &base_guide());
            merge(&mut cfg, &overlay);
            // the guide follows the station's colours
            cfg["theme"] = json!("auto");
            cfg["colors"] = json!({ "primary": null, "accent": null, "background": null });
            c.guide = serde_json::from_value::<GuideConfig>(cfg).map_err(|e| bad(e.to_string()))?;
            tvmaster_db::upsert_channel(&db, &c)?;
        }
    }
    // everything on air picks it up
    let ids: Vec<String> = tvmaster_db::list_channels(&s.db())?
        .into_iter()
        .map(|c| c.id)
        .collect();
    crate::live::restart_running(&s, &ids);
    Ok(Json(json!({ "applied": id })))
}

#[derive(Deserialize)]
pub struct SaveBody {
    name: String,
    #[serde(default)]
    about: String,
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("my-{}", if s.is_empty() { "look".into() } else { s })
}

fn store_mine(db: &rusqlite::Connection, list: &[Value]) -> ApiResult<()> {
    tvmaster_db::meta_set(
        db,
        MINE,
        &serde_json::to_string(list).unwrap_or_else(|_| "[]".into()),
    )?;
    Ok(())
}

fn add_mine(
    db: &rusqlite::Connection,
    name: &str,
    about: &str,
    theme: Value,
    guide: Value,
    screens: Value,
) -> ApiResult<String> {
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > 40 {
        return Err(bad("Give the look a name (up to 40 letters)."));
    }
    let mut list = mine(db);
    if list.len() >= 40 {
        return Err(bad("That is a lot of looks: delete one first."));
    }
    let base = slug(&name);
    let mut id = base.clone();
    let taken = |id: &str, list: &[Value]| {
        list.iter().any(|l| l["id"] == id) || builtins().iter().any(|b| b.id == id)
    };
    let mut n = 2;
    while taken(&id, &list) {
        id = format!("{base}-{n}");
        n += 1;
    }
    list.push(json!({ "id": id, "name": name, "about": about.chars().take(200).collect::<String>(), "theme": theme, "guide": guide, "screens": screens }));
    store_mine(db, &list)?;
    Ok(id)
}

/// `POST /api/v1/looks`: save how the station looks right now (its theme, its screens and the
/// first guide's layout) as a look of your own.
pub async fn save(State(s): State<AppState>, Json(b): Json<SaveBody>) -> ApiResult<Json<Value>> {
    let id = {
        let db = s.db();
        let theme = json!(crate::theme::station(&db));
        let screens = crate::screens::station_json(&db);
        let guide = tvmaster_db::list_channels(&db)?
            .into_iter()
            .find(|c| c.kind == ChannelKind::Guide)
            .map(|c| {
                let g = json!(c.guide);
                json!({ "layout": g["layout"], "look": g["look"], "pip": { "placement": g["pip"]["placement"], "size": g["pip"]["size"], "info": g["pip"]["info"] } })
            })
            .unwrap_or_else(|| json!({}));
        add_mine(&db, &b.name, &b.about, theme, guide, screens)?
    };
    Ok(Json(json!({ "id": id, "looks": all(&s.db()) })))
}

/// `POST /api/v1/looks/import`: take in a look that was exported (the object `GET /looks` gives).
pub async fn import(State(s): State<AppState>, Json(v): Json<Value>) -> ApiResult<Json<Value>> {
    let theme: ThemeChoice = serde_json::from_value(v["theme"].clone())
        .map_err(|_| bad("That is not a look: it has no theme."))?;
    theme.validate().map_err(bad)?;
    let guide = if v["guide"].is_object() {
        v["guide"].clone()
    } else {
        json!({})
    };
    let mut screens = serde_json::Map::new();
    if let Some(m) = v["screens"].as_object() {
        for (k, e) in m {
            if let Some(d) = e.get("design") {
                crate::screens::check_design(k, d).map_err(bad)?;
                screens.insert(
                    k.clone(),
                    json!({ "preset": e["preset"].as_str().unwrap_or("custom"), "design": d }),
                );
            }
        }
    }
    // the guide part must read as a guide setting
    let mut probe = json!({});
    merge(&mut probe, &base_guide());
    merge(&mut probe, &guide);
    serde_json::from_value::<GuideConfig>(probe)
        .map_err(|e| bad(format!("The guide part is not right: {e}")))?;
    let name = v["name"].as_str().unwrap_or("Imported look").to_string();
    let about = v["about"].as_str().unwrap_or("").to_string();
    let id = add_mine(
        &s.db(),
        &name,
        &about,
        json!(theme),
        guide,
        Value::Object(screens),
    )?;
    Ok(Json(json!({ "id": id, "looks": all(&s.db()) })))
}

/// `DELETE /api/v1/looks/:id`: forget one of your own looks.
pub async fn delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    {
        let db = s.db();
        let mut list = mine(&db);
        let before = list.len();
        list.retain(|l| l["id"] != id.as_str());
        if list.len() == before {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                "that is not one of your looks",
            ));
        }
        store_mine(&db, &list)?;
    }
    Ok(Json(json!({ "looks": all(&s.db()) })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::call;
    use crate::{router, AppState};

    #[test]
    fn every_built_in_look_is_a_real_theme_with_real_screens_and_a_guide_that_reads() {
        for b in builtins() {
            let theme: ThemeChoice =
                serde_json::from_value(json!({ "theme": b.theme, "style": b.style })).unwrap();
            assert_ne!(theme.theme, Default::default(), "{} has a theme", b.id);
            for (kind, preset) in b.screens {
                assert!(
                    crate::screens::preset_design(kind, preset).is_some(),
                    "{}: {kind}/{preset}",
                    b.id
                );
            }
            let mut g = json!({});
            merge(&mut g, &base_guide());
            merge(&mut g, &b.guide);
            serde_json::from_value::<GuideConfig>(g).unwrap_or_else(|e| panic!("{}: {e}", b.id));
        }
    }

    #[tokio::test]
    async fn a_look_is_applied_saved_exported_and_brought_back() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let (st, v) = call(&app, "GET", "/api/v1/looks", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(v["looks"].as_array().unwrap().len() >= 6);
        call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"guide","number":3,"name":"Guide","kind":"guide"}"#,
        )
        .await;
        // put on the liquid glass look
        let (st, v) = call(&app, "POST", "/api/v1/looks/liquid-glass/apply", "{}").await;
        assert_eq!(st, StatusCode::OK, "{v}");
        let (_, t) = call(&app, "GET", "/api/v1/theme", "").await;
        assert_eq!(t["station"]["theme"], "glass");
        assert_eq!(t["station"]["style"], "modern");
        let (_, s) = call(&app, "GET", "/api/v1/screens", "").await;
        assert_eq!(s["kinds"][0]["current"]["preset"], "glass");
        let (_, g) = call(&app, "GET", "/api/v1/channels/guide", "").await;
        assert_eq!(g["guide"]["look"]["glass"], true);
        assert_eq!(g["guide"]["look"]["background"]["style"], "aurora");
        assert_eq!(g["guide"]["pip"]["placement"], "top_right");
        // another look leaves nothing of it behind
        call(&app, "POST", "/api/v1/looks/classic-cable/apply", "{}").await;
        let (_, g) = call(&app, "GET", "/api/v1/channels/guide", "").await;
        assert_eq!(g["guide"]["look"]["glass"], false);
        assert_eq!(g["guide"]["look"]["background"]["style"], "gradient");
        assert_eq!(g["guide"]["look"]["typeface"], "pixel");
        let (_, s) = call(&app, "GET", "/api/v1/screens", "").await;
        assert_eq!(
            s["kinds"][0]["source"], "none",
            "classic cable has no designed screens"
        );
        // only the theme, when asked
        call(
            &app,
            "POST",
            "/api/v1/looks/liquid-glass/apply",
            r#"{"screens":false,"guides":false}"#,
        )
        .await;
        let (_, g) = call(&app, "GET", "/api/v1/channels/guide", "").await;
        assert_eq!(g["guide"]["look"]["glass"], false);
        // save the current appearance, export and import it
        let (st, v) = call(
            &app,
            "POST",
            "/api/v1/looks",
            r#"{"name":"My evening","about":"for after dinner"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["id"], "my-my-evening");
        let mine = v["looks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["id"] == "my-my-evening")
            .unwrap()
            .clone();
        assert_eq!(mine["builtin"], false);
        assert_eq!(mine["theme"]["theme"], "glass");
        let body = json!({ "name": "Copy", "theme": mine["theme"], "guide": mine["guide"], "screens": mine["screens"] }).to_string();
        let (st, v) = call(&app, "POST", "/api/v1/looks/import", &body).await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["id"], "my-copy");
        let (st, _) = call(&app, "POST", "/api/v1/looks/import", r#"{"name":"Bad"}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        // delete
        let (st, _) = call(&app, "DELETE", "/api/v1/looks/liquid-glass", "").await;
        assert_eq!(st, StatusCode::NOT_FOUND, "a built-in look stays");
        let (st, v) = call(&app, "DELETE", "/api/v1/looks/my-copy", "").await;
        assert_eq!(st, StatusCode::OK);
        assert!(!v["looks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["id"] == "my-copy"));
    }
}
