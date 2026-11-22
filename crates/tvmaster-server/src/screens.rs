//! Screens: the stand-by screen, the sign-off screen and the break screens ("Up next", "Later
//! today", "We'll be right back", "Welcome back", the station ID), each of which can be a design
//! the owner chose from a gallery and changed, for the whole station or for one channel.
//!
//! A design is plain data (see `tvmaster_render::design`); the gallery below is written as data
//! too, so what a built-in does is exactly what a custom design can do.

use crate::{ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tvmaster_core::ChannelOptions;
use tvmaster_render::design::{Design, Vars};

const KEY: &str = "screens";

struct Kind {
    id: &'static str,
    name: &'static str,
    about: &'static str,
    /// The `{...}` words this screen can use.
    words: &'static [&'static str],
    /// How long it airs, for the preview's scrubber (0 = it keeps going).
    seconds: u32,
}

const KINDS: [Kind; 7] = [
    Kind {
        id: "standby",
        name: "Stand-by",
        about: "On air when nothing is: time nothing is scheduled, a finished show, a file that cannot be played. It replaces the station card, so a channel set to colour bars, snow, black or a picture of your own keeps those.",
        words: &["channel", "number", "message", "clock", "date"],
        seconds: 0,
    },
    Kind {
        id: "signoff",
        name: "Sign-off",
        about: "On air during the sign-off hours (when the sign-off screen is the station card). Without a design of its own it uses the stand-by one.",
        words: &["channel", "number", "message", "clock", "date"],
        seconds: 0,
    },
    Kind {
        id: "up_next",
        name: "Up next",
        about: "Between the last break and the next programme.",
        words: &["channel", "title", "time", "clock"],
        seconds: 6,
    },
    Kind {
        id: "later_today",
        name: "Later today",
        about: "The next few programmes, in a list.",
        words: &["channel", "title", "time", "clock"],
        seconds: 8,
    },
    Kind {
        id: "brb",
        name: "We'll be right back",
        about: "At the start of a commercial break.",
        words: &["channel", "show", "clock"],
        seconds: 5,
    },
    Kind {
        id: "back_to",
        name: "Welcome back",
        about: "At the end of a break inside a show.",
        words: &["channel", "show", "clock"],
        seconds: 5,
    },
    Kind {
        id: "station_id",
        name: "Station ID",
        about: "A short reminder of which channel this is.",
        words: &["channel", "number", "clock", "date"],
        seconds: 5,
    },
];

// ------------------------------------------------------------------------------ the gallery

fn text(t: &str, size: u32, x: i32, y: i32, extra: Value) -> Value {
    let mut v = json!({ "type": "text", "text": t, "size": size, "x": x, "y": y });
    merge(&mut v, extra);
    v
}

fn shape(w: u32, h: u32, x: i32, y: i32, extra: Value) -> Value {
    let mut v = json!({ "type": "shape", "w": w, "h": h, "x": x, "y": y });
    merge(&mut v, extra);
    v
}

fn merge(a: &mut Value, b: Value) {
    if let (Some(a), Value::Object(b)) = (a.as_object_mut(), b) {
        for (k, v) in b {
            a.insert(k, v);
        }
    }
}

fn anim(enter: &str, delay: u32, extra: Value) -> Value {
    let mut a = json!({ "enter": enter, "delay_ms": delay });
    merge(&mut a, extra);
    json!({ "anim": a })
}

fn design(name: &str, backdrop: Value, layers: Vec<Value>, fade: u32) -> Value {
    json!({ "name": name, "backdrop": backdrop, "layers": layers, "fade_out_ms": fade })
}

struct Preset {
    kind: &'static str,
    id: &'static str,
    name: &'static str,
    about: &'static str,
    design: Value,
}

fn glass_card(w: u32, h: u32) -> Value {
    shape(
        w,
        h,
        500,
        500,
        json!({ "glass": true, "radius": 48, "anim": { "enter": "zoom", "enter_ms": 800 } }),
    )
}

fn aurora() -> Value {
    json!({ "type": "aurora", "colors": [] })
}

fn presets() -> Vec<Preset> {
    let p = |kind, id, name, about, design| Preset {
        kind,
        id,
        name,
        about,
        design,
    };
    let theme = json!({ "type": "theme" });
    let mut out = vec![
        // ---- stand-by
        p(
            "standby",
            "classic",
            "Station card",
            "The channel number and name, with a gentle pulse.",
            design(
                "Station card",
                theme.clone(),
                vec![
                    text(
                        "{number}",
                        190,
                        500,
                        330,
                        anim("zoom", 0, json!({ "enter_ms": 800 })),
                    ),
                    text(
                        "{channel}",
                        64,
                        500,
                        540,
                        json!({ "anim": { "enter": "slide_up", "delay_ms": 250 } }),
                    ),
                    shape(
                        260,
                        10,
                        500,
                        640,
                        json!({ "anim": { "enter": "wipe", "delay_ms": 600 } }),
                    ),
                    text(
                        "{message}",
                        34,
                        500,
                        760,
                        json!({ "bold": false, "anim": { "enter": "fade", "delay_ms": 900, "motion": "blink", "speed": 60 } }),
                    ),
                ],
                0,
            ),
        ),
        p(
            "standby",
            "drift",
            "Drifting name",
            "The channel name drifts around the screen, like a screensaver.",
            design(
                "Drifting name",
                json!({ "type": "solid", "color": "dark" }),
                vec![
                    text(
                        "{number}  {channel}",
                        60,
                        500,
                        500,
                        json!({ "color": "accent_text", "anim": { "enter": "fade", "motion": "drift", "speed": 80 } }),
                    ),
                    text(
                        "{message}",
                        28,
                        500,
                        940,
                        json!({ "bold": false, "opacity": 70, "anim": { "enter": "fade", "delay_ms": 600 } }),
                    ),
                ],
                0,
            ),
        ),
        p(
            "standby",
            "glass",
            "Liquid glass",
            "Frosted glass over soft, moving colour, with the time.",
            design(
                "Liquid glass",
                aurora(),
                vec![
                    glass_card(560, 520),
                    text("{number}", 150, 500, 380, anim("zoom", 300, json!({}))),
                    text("{channel}", 56, 500, 540, anim("slide_up", 450, json!({}))),
                    text(
                        "{message}",
                        30,
                        500,
                        650,
                        json!({ "bold": false, "anim": { "enter": "type", "delay_ms": 900, "enter_ms": 1400 } }),
                    ),
                    shape(
                        190,
                        100,
                        880,
                        100,
                        json!({ "glass": true, "radius": 50, "anim": { "enter": "slide_down", "delay_ms": 400 } }),
                    ),
                    json!({ "type": "clock", "size": 40, "x": 880, "y": 100, "anim": { "enter": "fade", "delay_ms": 700 } }),
                ],
                0,
            ),
        ),
        p(
            "standby",
            "clock",
            "Big clock",
            "The time, large, with the date and the channel.",
            design(
                "Big clock",
                theme.clone(),
                vec![
                    json!({ "type": "clock", "size": 230, "x": 500, "y": 430, "anim": { "enter": "zoom", "enter_ms": 900 } }),
                    text(
                        "{date}",
                        46,
                        500,
                        640,
                        json!({ "bold": false, "anim": { "enter": "fade", "delay_ms": 500 } }),
                    ),
                    text(
                        "{number}  {channel}",
                        40,
                        500,
                        150,
                        json!({ "color": "accent_text", "anim": { "enter": "slide_down", "delay_ms": 300 } }),
                    ),
                    text(
                        "{message}",
                        28,
                        500,
                        900,
                        json!({ "bold": false, "anim": { "enter": "fade", "delay_ms": 900, "motion": "blink", "speed": 60 } }),
                    ),
                ],
                0,
            ),
        ),
        p(
            "standby",
            "dots",
            "Moving dots",
            "A quiet pattern behind a card.",
            design(
                "Moving dots",
                json!({ "type": "pattern", "kind": "dots", "color": "primary", "bg": "dark", "size": 56 }),
                vec![
                    shape(
                        620,
                        380,
                        500,
                        500,
                        json!({ "color": "dark", "radius": 28, "opacity": 92, "anim": { "enter": "zoom" } }),
                    ),
                    text("{channel}", 64, 500, 440, anim("slide_up", 300, json!({}))),
                    text(
                        "{message}",
                        32,
                        500,
                        570,
                        json!({ "bold": false, "color": "accent_text", "anim": { "enter": "fade", "delay_ms": 700, "motion": "blink", "speed": 60 } }),
                    ),
                ],
                0,
            ),
        ),
        p(
            "standby",
            "marquee",
            "Scrolling notice",
            "The channel name, with the message scrolling underneath in chunky letters.",
            design(
                "Scrolling notice",
                json!({ "type": "solid", "color": "dark" }),
                vec![
                    text("{channel}", 96, 500, 400, anim("zoom", 0, json!({}))),
                    text(
                        "{number}",
                        40,
                        500,
                        250,
                        json!({ "color": "accent_text", "anim": { "enter": "slide_down", "delay_ms": 300 } }),
                    ),
                    text(
                        "{message}   *   {message}   *   {message}",
                        40,
                        500,
                        840,
                        json!({ "pixel": true, "color": "accent_text", "anim": { "enter": "none", "motion": "marquee" } }),
                    ),
                    json!({ "type": "clock", "size": 30, "x": 930, "y": 80, "bold": false, "anim": { "enter": "fade", "delay_ms": 600 } }),
                ],
                0,
            ),
        ),
        p(
            "standby",
            "testcard",
            "Test card",
            "Colour bars with a label, like a station's test pattern.",
            design(
                "Test card",
                json!({ "type": "bars" }),
                vec![
                    shape(
                        520,
                        150,
                        500,
                        420,
                        json!({ "color": "black", "opacity": 85, "radius": 12, "anim": { "enter": "fade" } }),
                    ),
                    text(
                        "{channel}",
                        56,
                        500,
                        395,
                        json!({ "color": "white", "anim": { "enter": "fade", "delay_ms": 200 } }),
                    ),
                    text(
                        "{message}",
                        28,
                        500,
                        465,
                        json!({ "bold": false, "color": "white", "anim": { "enter": "fade", "delay_ms": 400, "motion": "blink", "speed": 60 } }),
                    ),
                ],
                0,
            ),
        ),
        // ---- sign-off
        p(
            "signoff",
            "goodnight",
            "Good night",
            "A quiet goodnight with the time.",
            design(
                "Good night",
                json!({ "type": "gradient", "top": "dark", "bottom": "black" }),
                vec![
                    text(
                        "Good night",
                        120,
                        500,
                        420,
                        anim("slide_up", 0, json!({ "enter_ms": 1400 })),
                    ),
                    text(
                        "{channel} will be back soon",
                        40,
                        500,
                        580,
                        json!({ "bold": false, "anim": { "enter": "fade", "delay_ms": 1200 } }),
                    ),
                    json!({ "type": "clock", "size": 34, "x": 500, "y": 880, "bold": false, "opacity": 70, "anim": { "enter": "fade", "delay_ms": 1800, "motion": "blink", "speed": 40 } }),
                ],
                0,
            ),
        ),
        p(
            "signoff",
            "glass",
            "Liquid glass night",
            "Frosted glass over slow colour.",
            design(
                "Liquid glass night",
                json!({ "type": "aurora", "colors": ["primary", "dark", "accent"] }),
                vec![
                    glass_card(620, 400),
                    text(
                        "Good night",
                        96,
                        500,
                        430,
                        anim("fade", 400, json!({ "enter_ms": 1600 })),
                    ),
                    json!({ "type": "clock", "size": 52, "x": 500, "y": 590, "bold": false, "anim": { "enter": "fade", "delay_ms": 1400 } }),
                ],
                0,
            ),
        ),
        p(
            "signoff",
            "bars",
            "Signed off",
            "Colour bars with a small label.",
            design(
                "Signed off",
                json!({ "type": "bars" }),
                vec![
                    shape(
                        600,
                        130,
                        500,
                        420,
                        json!({ "color": "black", "opacity": 85, "radius": 12 }),
                    ),
                    text(
                        "{channel} has signed off",
                        46,
                        500,
                        420,
                        json!({ "color": "white" }),
                    ),
                ],
                0,
            ),
        ),
        // ---- up next
        p(
            "up_next",
            "slide",
            "Slide-in bar",
            "A bar slides in along the bottom with the title and time.",
            design(
                "Slide-in bar",
                theme.clone(),
                vec![
                    text(
                        "{channel}",
                        36,
                        90,
                        120,
                        json!({ "align": "left", "color": "accent_text", "anim": { "enter": "slide_down" } }),
                    ),
                    shape(
                        1000,
                        230,
                        0,
                        800,
                        json!({ "align": "left", "color": "accent", "color2": "primary", "anim": { "enter": "slide_right", "enter_ms": 700 } }),
                    ),
                    text(
                        "UP NEXT",
                        30,
                        60,
                        740,
                        json!({ "align": "left", "color": "on_accent", "anim": { "enter": "fade", "delay_ms": 500 } }),
                    ),
                    text(
                        "{title}",
                        74,
                        60,
                        820,
                        json!({ "align": "left", "color": "on_accent", "width": 880, "lines": 1, "anim": { "enter": "slide_left", "delay_ms": 650 } }),
                    ),
                    text(
                        "{time}",
                        56,
                        960,
                        820,
                        json!({ "align": "right", "color": "ink", "anim": { "enter": "fade", "delay_ms": 1100 } }),
                    ),
                ],
                400,
            ),
        ),
        p(
            "up_next",
            "big",
            "Big title",
            "The title large and typed in, with the time.",
            design(
                "Big title",
                theme.clone(),
                vec![
                    text(
                        "UP NEXT",
                        42,
                        500,
                        270,
                        json!({ "color": "accent_text", "anim": { "enter": "fade" } }),
                    ),
                    text(
                        "{title}",
                        118,
                        500,
                        480,
                        json!({ "lines": 2, "anim": { "enter": "type", "delay_ms": 400, "enter_ms": 1200 } }),
                    ),
                    text(
                        "at {time}",
                        56,
                        500,
                        760,
                        json!({ "bold": false, "anim": { "enter": "slide_up", "delay_ms": 1300 } }),
                    ),
                ],
                400,
            ),
        ),
        p(
            "up_next",
            "glass",
            "Liquid glass",
            "A frosted card over soft colour.",
            design(
                "Liquid glass",
                aurora(),
                vec![
                    glass_card(720, 420),
                    text(
                        "UP NEXT",
                        34,
                        500,
                        360,
                        json!({ "color": "accent_text", "anim": { "enter": "fade", "delay_ms": 400 } }),
                    ),
                    text(
                        "{title}",
                        76,
                        500,
                        500,
                        json!({ "lines": 2, "width": 640, "anim": { "enter": "slide_up", "delay_ms": 600 } }),
                    ),
                    shape(
                        230,
                        96,
                        500,
                        660,
                        json!({ "glass": true, "radius": 48, "anim": { "enter": "zoom", "delay_ms": 1000 } }),
                    ),
                    text("{time}", 46, 500, 660, anim("fade", 1200, json!({}))),
                ],
                400,
            ),
        ),
        // ---- later today
        p(
            "later_today",
            "list",
            "The list",
            "The next few programmes, one after another.",
            design(
                "The list",
                theme.clone(),
                vec![
                    text(
                        "{channel}",
                        36,
                        90,
                        110,
                        json!({ "align": "left", "color": "accent_text", "anim": { "enter": "slide_down" } }),
                    ),
                    text(
                        "LATER TODAY",
                        80,
                        90,
                        230,
                        json!({ "align": "left", "anim": { "enter": "slide_right", "delay_ms": 200 } }),
                    ),
                    json!({ "type": "list", "x": 90, "y": 640, "align": "left", "size": 52, "rows": 4, "width": 820, "anim": { "enter": "slide_up", "delay_ms": 600, "enter_ms": 500 } }),
                ],
                400,
            ),
        ),
        p(
            "later_today",
            "glass",
            "Liquid glass",
            "The list on frosted glass.",
            design(
                "Liquid glass",
                aurora(),
                vec![
                    glass_card(820, 640),
                    text(
                        "LATER TODAY",
                        40,
                        500,
                        270,
                        json!({ "color": "accent_text", "anim": { "enter": "fade", "delay_ms": 400 } }),
                    ),
                    json!({ "type": "list", "x": 500, "y": 560, "size": 46, "rows": 4, "width": 700, "anim": { "enter": "slide_up", "delay_ms": 700 } }),
                ],
                400,
            ),
        ),
        // ---- we'll be right back
        p(
            "brb",
            "classic",
            "Right back",
            "Big words, and what comes after the break.",
            design(
                "Right back",
                theme.clone(),
                vec![
                    text(
                        "{channel}",
                        36,
                        90,
                        110,
                        json!({ "align": "left", "color": "accent_text", "anim": { "enter": "slide_down" } }),
                    ),
                    text(
                        "We'll be",
                        130,
                        90,
                        330,
                        json!({ "align": "left", "anim": { "enter": "slide_up", "delay_ms": 100 } }),
                    ),
                    text(
                        "right back",
                        130,
                        90,
                        480,
                        json!({ "align": "left", "anim": { "enter": "slide_up", "delay_ms": 250 } }),
                    ),
                    text(
                        "After this: {show}",
                        46,
                        90,
                        760,
                        json!({ "align": "left", "bold": false, "color": "accent_text", "width": 820, "anim": { "enter": "fade", "delay_ms": 900 } }),
                    ),
                ],
                300,
            ),
        ),
        p(
            "brb",
            "pulse",
            "Pulsing",
            "The words breathe gently while you wait.",
            design(
                "Pulsing",
                theme.clone(),
                vec![
                    text(
                        "We'll be right back",
                        110,
                        500,
                        430,
                        json!({ "lines": 2, "width": 800, "anim": { "enter": "zoom", "motion": "pulse", "speed": 70 } }),
                    ),
                    text(
                        "{show} returns shortly",
                        40,
                        500,
                        750,
                        json!({ "bold": false, "color": "accent_text", "anim": { "enter": "fade", "delay_ms": 800 } }),
                    ),
                ],
                300,
            ),
        ),
        p(
            "brb",
            "glass",
            "Liquid glass",
            "A frosted card over soft colour.",
            design(
                "Liquid glass",
                aurora(),
                vec![
                    glass_card(740, 380),
                    text(
                        "We'll be right back",
                        84,
                        500,
                        440,
                        json!({ "lines": 2, "width": 640, "anim": { "enter": "slide_up", "delay_ms": 400 } }),
                    ),
                    text(
                        "After this: {show}",
                        34,
                        500,
                        640,
                        json!({ "bold": false, "width": 640, "anim": { "enter": "fade", "delay_ms": 1000 } }),
                    ),
                ],
                300,
            ),
        ),
        // ---- welcome back
        p(
            "back_to",
            "welcome",
            "Welcome back",
            "The show's name, typed in.",
            design(
                "Welcome back",
                theme.clone(),
                vec![
                    text(
                        "{channel}",
                        36,
                        90,
                        110,
                        json!({ "align": "left", "color": "accent_text", "anim": { "enter": "slide_down" } }),
                    ),
                    text(
                        "Welcome back to",
                        56,
                        500,
                        330,
                        json!({ "bold": false, "color": "accent_text", "anim": { "enter": "fade" } }),
                    ),
                    text(
                        "{show}",
                        120,
                        500,
                        520,
                        json!({ "lines": 2, "anim": { "enter": "type", "delay_ms": 400, "enter_ms": 1100 } }),
                    ),
                ],
                300,
            ),
        ),
        p(
            "back_to",
            "glass",
            "Liquid glass",
            "A frosted card over soft colour.",
            design(
                "Liquid glass",
                aurora(),
                vec![
                    glass_card(740, 360),
                    text(
                        "Welcome back to",
                        36,
                        500,
                        400,
                        json!({ "bold": false, "color": "accent_text", "anim": { "enter": "fade", "delay_ms": 400 } }),
                    ),
                    text(
                        "{show}",
                        84,
                        500,
                        540,
                        json!({ "lines": 2, "width": 640, "anim": { "enter": "slide_up", "delay_ms": 700 } }),
                    ),
                ],
                300,
            ),
        ),
        // ---- station id
        p(
            "station_id",
            "logo",
            "You're watching",
            "The channel's name, zooming in.",
            design(
                "You're watching",
                theme.clone(),
                vec![
                    text(
                        "YOU'RE WATCHING",
                        36,
                        500,
                        270,
                        json!({ "color": "accent_text", "anim": { "enter": "fade" } }),
                    ),
                    text(
                        "{channel}",
                        140,
                        500,
                        480,
                        json!({ "lines": 2, "anim": { "enter": "zoom", "delay_ms": 300, "enter_ms": 900 } }),
                    ),
                    shape(
                        360,
                        12,
                        500,
                        700,
                        json!({ "anim": { "enter": "wipe", "delay_ms": 1000 } }),
                    ),
                    text(
                        "{number}",
                        48,
                        500,
                        780,
                        json!({ "color": "accent_text", "anim": { "enter": "fade", "delay_ms": 1300 } }),
                    ),
                ],
                300,
            ),
        ),
        p(
            "station_id",
            "glass",
            "Liquid glass",
            "A frosted card over soft colour.",
            design(
                "Liquid glass",
                aurora(),
                vec![
                    glass_card(640, 440),
                    text(
                        "{number}",
                        120,
                        500,
                        410,
                        json!({ "color": "accent_text", "anim": { "enter": "zoom", "delay_ms": 300 } }),
                    ),
                    text("{channel}", 64, 500, 580, anim("slide_up", 600, json!({}))),
                ],
                300,
            ),
        ),
    ];
    // Written out in full, so a client never has to know the defaults.
    for p in &mut out {
        if let Ok(d) = serde_json::from_value::<Design>(p.design.clone()) {
            if let Ok(v) = serde_json::to_value(d) {
                p.design = v;
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------- storage

/// The station's chosen screens: kind -> `{ "preset": id, "design": {...} }`.
fn station_screens(db: &rusqlite::Connection) -> Map<String, Value> {
    tvmaster_db::meta_get(db, KEY)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<Value>(&j).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// The designs in force on a channel: its own, else the station's, kind -> design.
pub(crate) fn effective(
    db: &rusqlite::Connection,
    options: &ChannelOptions,
) -> std::collections::BTreeMap<String, Value> {
    let mut out = std::collections::BTreeMap::new();
    let station = station_screens(db);
    for k in KINDS {
        let pick = options
            .screens
            .get(k.id)
            .or_else(|| station.get(k.id))
            .and_then(|v| v.get("design"))
            .filter(|d| !d.is_null());
        if let Some(d) = pick {
            out.insert(k.id.to_string(), d.clone());
        }
    }
    out
}

/// A gallery design, written out in full.
pub(crate) fn preset_design(kind: &str, id: &str) -> Option<Value> {
    presets()
        .into_iter()
        .find(|p| p.kind == kind && p.id == id)
        .map(|p| p.design)
}

/// The station's chosen screens as stored: kind -> `{ preset, design }`.
pub(crate) fn station_json(db: &rusqlite::Connection) -> Value {
    Value::Object(station_screens(db))
}

/// Whether `design` is a good design for the screen `kind`.
pub(crate) fn check_design(kind: &str, design: &Value) -> Result<(), String> {
    kind_of(kind).map_err(|_| format!("There is no screen called {kind}."))?;
    parse_design(design)
        .map(|_| ())
        .map_err(|e| e.message().to_string())
}

/// Make the station's screens exactly these (a look): those named are set, the rest go back to classic.
/// The caller restarts what is on air.
pub(crate) fn set_station_all(s: &AppState, designs: &Map<String, Value>) -> ApiResult<()> {
    let mut m = Map::new();
    for k in KINDS {
        if let Some(e) = designs.get(k.id) {
            if let Some(d) = e.get("design") {
                parse_design(d)?;
                m.insert(
                    k.id.to_string(),
                    json!({ "preset": e["preset"].as_str().unwrap_or("custom"), "design": d }),
                );
            }
        }
    }
    tvmaster_db::meta_set(&s.db(), KEY, &Value::Object(m).to_string())?;
    Ok(())
}

fn kind_of(id: &str) -> ApiResult<&'static Kind> {
    KINDS
        .iter()
        .find(|k| k.id == id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such screen"))
}

fn bad(e: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, e.into())
}

fn parse_design(v: &Value) -> ApiResult<Design> {
    let d: Design = serde_json::from_value(v.clone())
        .map_err(|e| bad(format!("That is not a screen design: {e}")))?;
    if let Some(p) = d.problem() {
        return Err(bad(p));
    }
    Ok(d)
}

#[derive(Deserialize, Default)]
pub struct ChannelQ {
    channel: Option<String>,
}

/// `GET /api/v1/screens?channel=`: the screens, the gallery for each, and what is in force (for
/// one channel, whether it has its own design or follows the station's).
pub async fn get(State(s): State<AppState>, Query(q): Query<ChannelQ>) -> ApiResult<Json<Value>> {
    let db = s.db();
    let station = station_screens(&db);
    let own = match q.channel.as_deref() {
        Some(id) => {
            tvmaster_db::get_channel(&db, id)?
                .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such channel"))?
                .options
                .screens
        }
        None => Default::default(),
    };
    let all = presets();
    let kinds: Vec<Value> = KINDS
        .iter()
        .map(|k| {
            let current = if q.channel.is_some() {
                own.get(k.id).or_else(|| station.get(k.id))
            } else {
                station.get(k.id)
            };
            let source = if q.channel.is_some() && own.contains_key(k.id) {
                "channel"
            } else if station.contains_key(k.id) {
                "station"
            } else {
                "none"
            };
            json!({
                "id": k.id, "name": k.name, "about": k.about, "words": k.words, "seconds": k.seconds,
                "presets": all.iter().filter(|p| p.kind == k.id).map(|p| json!({
                    "id": p.id, "name": p.name, "about": p.about, "design": p.design,
                })).collect::<Vec<_>>(),
                "current": current,
                "source": source,
            })
        })
        .collect();
    Ok(Json(json!({
        "kinds": kinds,
        "colors": ["primary", "accent", "ink", "dark", "bottom", "on_accent", "white", "black"],
    })))
}

#[derive(Deserialize)]
pub struct SetBody {
    /// A gallery design by id, or `custom` with a `design`.
    preset: Option<String>,
    design: Option<Value>,
}

/// `PUT /api/v1/screens/:kind?channel=`: choose a design for a screen, for the station or one channel.
pub async fn put(
    State(s): State<AppState>,
    Path(kind): Path<String>,
    Query(q): Query<ChannelQ>,
    Json(b): Json<SetBody>,
) -> ApiResult<Json<Value>> {
    let k = kind_of(&kind)?;
    let (preset, design) = match (&b.design, b.preset.as_deref()) {
        (Some(d), p) => {
            parse_design(d)?;
            (p.unwrap_or("custom").to_string(), d.clone())
        }
        (None, Some(id)) => {
            let p = presets()
                .into_iter()
                .find(|p| p.kind == k.id && p.id == id)
                .ok_or_else(|| bad("No such design for this screen."))?;
            (p.id.to_string(), p.design)
        }
        (None, None) => return Err(bad("Choose a design.")),
    };
    let entry = json!({ "preset": preset, "design": design });
    store(&s, k.id, q.channel.as_deref(), Some(entry))?;
    get(State(s), Query(q)).await
}

/// `DELETE /api/v1/screens/:kind?channel=`: back to the classic screen (or, for a channel, to the station's).
pub async fn delete(
    State(s): State<AppState>,
    Path(kind): Path<String>,
    Query(q): Query<ChannelQ>,
) -> ApiResult<Json<Value>> {
    let k = kind_of(&kind)?;
    store(&s, k.id, q.channel.as_deref(), None)?;
    get(State(s), Query(q)).await
}

fn store(s: &AppState, kind: &str, channel: Option<&str>, entry: Option<Value>) -> ApiResult<()> {
    let affected: Vec<String> = {
        let db = s.db();
        match channel {
            Some(id) => {
                let mut c = tvmaster_db::get_channel(&db, id)?
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such channel"))?;
                match entry {
                    Some(e) => c.options.screens.insert(kind.to_string(), e),
                    None => c.options.screens.remove(kind),
                };
                tvmaster_db::upsert_channel(&db, &c)?;
                vec![c.id]
            }
            None => {
                let mut m = station_screens(&db);
                match entry {
                    Some(e) => m.insert(kind.to_string(), e),
                    None => m.remove(kind),
                };
                tvmaster_db::meta_set(&db, KEY, &Value::Object(m).to_string())?;
                tvmaster_db::list_channels(&db)?
                    .into_iter()
                    .filter(|c| !c.options.screens.contains_key(kind))
                    .map(|c| c.id)
                    .collect()
            }
        }
    };
    // what is on air picks the new screen up now
    crate::live::restart_running(s, &affected);
    Ok(())
}

#[derive(Deserialize)]
pub struct PreviewBody {
    kind: String,
    design: Value,
    #[serde(default)]
    t: f32,
    #[serde(default)]
    channel: Option<String>,
}

/// Sample words for a preview.
fn sample(kind: &str, name: &str, number: u32) -> Vars {
    let mut v = Vars {
        channel: name.to_string(),
        number,
        message: "PLEASE STAND BY".into(),
        clock0_secs: 20 * 3600 + 29 * 60 + 50,
        date: "SAT NOV 21".into(),
        ..Vars::default()
    };
    match kind {
        "up_next" | "later_today" | "brb" | "back_to" => {
            v.show = "The Late Show".into();
            v.title = "The Late Show".into();
            v.time = "9:00 PM".into();
            v.items = vec![
                ("9:00 PM".into(), "The Late Show".into()),
                ("9:30 PM".into(), "Cheers".into()),
                ("10:00 PM".into(), "Taxi".into()),
                ("10:30 PM".into(), "Wings".into()),
            ];
        }
        _ => {}
    }
    v
}

/// `POST /api/v1/screens/preview`: one frame of a design (at second `t`) as a PNG. Nothing goes on air.
pub async fn preview(State(s): State<AppState>, Json(b): Json<PreviewBody>) -> ApiResult<Response> {
    let k = kind_of(&b.kind)?;
    let design = parse_design(&b.design)?;
    let (name, number, theme) = {
        let db = s.db();
        let ch = b
            .channel
            .as_deref()
            .and_then(|id| tvmaster_db::get_channel(&db, id).ok().flatten());
        match ch {
            Some(c) => {
                let choice = crate::theme::effective(&db, &c.options);
                (
                    c.name.clone(),
                    c.number,
                    tvmaster_playout::card_theme(&choice, &c.id),
                )
            }
            None => (
                "Retro TV".to_string(),
                4,
                tvmaster_playout::card_theme(&crate::theme::station(&db), "preview"),
            ),
        }
    };
    let ffmpeg = s.ffmpeg.as_ref().map(|f| f.ffmpeg.clone());
    let t = b.t.clamp(0.0, 600.0);
    let dur = k.seconds as f32;
    let kind = b.kind.clone();
    let png = tokio::task::spawn_blocking(move || {
        let mut th = theme;
        th.modern = true;
        let images = ffmpeg
            .map(|f| tvmaster_playout::load_pictures(&f, &design))
            .unwrap_or_default();
        let frame = tvmaster_render::design::render(
            &design,
            &sample(&kind, &name, number),
            &images,
            &th,
            t,
            dur,
        );
        let mut rgba = Vec::with_capacity(frame.w * frame.h * 4);
        for px in frame.px.as_chunks::<3>().0 {
            rgba.extend_from_slice(px);
            rgba.push(255);
        }
        tvmaster_render::png::rgba_png(frame.w as u32, frame.h as u32, &rgba)
    })
    .await
    .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "could not draw that"))?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        png,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{call, call_raw};
    use crate::{router, AppState};

    #[test]
    fn every_built_in_design_reads_properly_and_draws() {
        let th = tvmaster_render::Theme::from_hue(0.58);
        for p in presets() {
            let d: Design = serde_json::from_value(p.design.clone())
                .unwrap_or_else(|e| panic!("{} {}: {e}", p.kind, p.id));
            assert_eq!(d.problem(), None, "{} {}", p.kind, p.id);
            let k = kind_of(p.kind).unwrap();
            let f = tvmaster_render::design::render(
                &d,
                &sample(p.kind, "Treehouse", 6),
                &Default::default(),
                &th,
                2.5,
                k.seconds as f32,
            );
            assert_eq!(f.w, 1280, "{} {}", p.kind, p.id);
        }
        // every kind has a few to choose from
        for k in KINDS {
            assert!(
                presets().iter().filter(|p| p.kind == k.id).count() >= 2,
                "{}",
                k.id
            );
        }
    }

    #[tokio::test]
    async fn screens_are_chosen_for_the_station_or_a_channel_and_previewed() {
        let state = AppState::in_memory();
        let app = router(state.clone());
        let (st, v) = call(&app, "GET", "/api/v1/screens", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["kinds"].as_array().unwrap().len(), 7);
        assert_eq!(v["kinds"][0]["source"], "none");
        let glass = v["kinds"][0]["presets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "glass")
            .unwrap()
            .clone();
        // choose one for the station
        let (st, v) = call(
            &app,
            "PUT",
            "/api/v1/screens/standby",
            r#"{"preset":"glass"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["kinds"][0]["source"], "station");
        assert_eq!(v["kinds"][0]["current"]["preset"], "glass");
        // a channel follows it until it has its own
        call(
            &app,
            "POST",
            "/api/v1/channels",
            r#"{"id":"c","number":3,"name":"Kids","kind":"programming"}"#,
        )
        .await;
        let (_, v) = call(&app, "GET", "/api/v1/screens?channel=c", "").await;
        assert_eq!(v["kinds"][0]["source"], "station");
        let spec_designs = {
            let db = state.db();
            let c = tvmaster_db::get_channel(&db, "c").unwrap().unwrap();
            effective(&db, &c.options)
        };
        assert!(spec_designs.contains_key("standby"));
        // its own design, changed from the gallery's
        let mut mine = glass["design"].clone();
        mine["layers"][1]["text"] = json!("{channel}!");
        let body = json!({ "preset": "glass", "design": mine }).to_string();
        let (st, v) = call(&app, "PUT", "/api/v1/screens/standby?channel=c", &body).await;
        assert_eq!(st, StatusCode::OK, "{v}");
        assert_eq!(v["kinds"][0]["source"], "channel");
        {
            let db = state.db();
            let c = tvmaster_db::get_channel(&db, "c").unwrap().unwrap();
            assert_eq!(
                effective(&db, &c.options)["standby"]["layers"][1]["text"],
                "{channel}!"
            );
        }
        // nonsense is refused in words
        let bad_body =
            json!({ "design": { "layers": [{ "type": "text", "text": "x", "size": 3 }] } })
                .to_string();
        let (st, v) = call(&app, "PUT", "/api/v1/screens/standby", &bad_body).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(v["error"].as_str().unwrap().contains("size"), "{v}");
        let (st, _) = call(
            &app,
            "PUT",
            "/api/v1/screens/nonsense",
            r#"{"preset":"glass"}"#,
        )
        .await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        // back to the station's, then to the classic screen
        let (_, v) = call(&app, "DELETE", "/api/v1/screens/standby?channel=c", "").await;
        assert_eq!(v["kinds"][0]["source"], "station");
        let (_, v) = call(&app, "DELETE", "/api/v1/screens/standby", "").await;
        assert_eq!(v["kinds"][0]["source"], "none");
        // a preview is a picture
        let body = json!({ "kind": "up_next", "design": presets().into_iter().find(|p| p.kind == "up_next").unwrap().design, "t": 2.0 }).to_string();
        let (st, png) = call_raw(&app, "POST", "/api/v1/screens/preview", &body).await;
        assert_eq!(st, StatusCode::OK);
        assert!(png.len() > 1000, "a picture came back");
    }
}
