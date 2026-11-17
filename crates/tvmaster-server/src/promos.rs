//! Spots made in the app: a sponsor, a programme promo, or a few slides of text, drawn in the
//! station's look (or your own), with a picture behind them, a jingle or your music, and an old-tape
//! look if you like. The result is a video file in the station's commercials, so it airs in breaks
//! with the rest.

use crate::{libraries, ApiError, ApiResult, AppState};
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path as FsPath, PathBuf};
use std::process::{Command, Stdio};
use tvmaster_core::{Look, ThemeChoice};
use tvmaster_render::{Photo, PromoKind, PromoSpec, Scene};

const FOLDER: &str = "Station promos";
const FPS: u32 = 30;

fn bad(m: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, m)
}

fn internal(m: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, m.to_string())
}

#[derive(Deserialize, Clone)]
pub struct PromoBody {
    /// `sponsor`, `coming_up` or `slides`.
    #[serde(default = "d_kind")]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    tagline: String,
    #[serde(default)]
    contact: String,
    #[serde(default)]
    slides: Vec<String>,
    /// How long the spot is, 5-120 seconds.
    #[serde(default = "d_seconds")]
    seconds: u32,
    /// A picture on the server to put behind the words.
    #[serde(default)]
    picture: Option<String>,
    /// Blank = the station's theme.
    #[serde(default)]
    theme: Option<ThemeChoice>,
    /// An old-tape treatment: `vhs`, `crt`, `film`, `black_white` or `sepia`.
    #[serde(default)]
    look: Option<String>,
    /// `none`, `jingle` or `music`.
    #[serde(default = "d_sound")]
    sound: String,
    /// For `music`: an audio file on the server.
    #[serde(default)]
    music: Option<String>,
    /// A name for the file; blank = the title.
    #[serde(default)]
    name: String,
    /// Which commercials library to put it in; blank = the station's own.
    #[serde(default)]
    library: Option<i64>,
    /// The year the spot is "from": it is filed under that year so a channel that only airs ads from
    /// certain years (an era) can use it. Blank = no year (such a spot is skipped by an era filter).
    #[serde(default)]
    year: Option<i32>,
    /// Preview only: seconds into the spot.
    #[serde(default)]
    t: f32,
}

fn d_kind() -> String {
    "sponsor".into()
}
fn d_seconds() -> u32 {
    15
}
fn d_sound() -> String {
    "jingle".into()
}

impl PromoBody {
    fn validate(&self) -> Result<(), String> {
        let long = |s: &str, n: usize| s.chars().count() > n || s.chars().any(char::is_control);
        if !["sponsor", "coming_up", "slides"].contains(&self.kind.as_str()) {
            return Err("kind must be sponsor, coming_up or slides".into());
        }
        if long(&self.title, 80) || long(&self.tagline, 120) || long(&self.contact, 100) {
            return Err(
                "the title is at most 80 characters, the tagline 120, the contact line 100".into(),
            );
        }
        if self.slides.len() > 10 || self.slides.iter().any(|s| long(s, 120)) {
            return Err("at most 10 slides of 120 characters".into());
        }
        if self.kind == "slides" && self.slides.iter().all(|s| s.trim().is_empty()) {
            return Err("write at least one slide".into());
        }
        if self.kind != "slides" && self.title.trim().is_empty() {
            return Err("a spot needs a title".into());
        }
        if !(5..=120).contains(&self.seconds) {
            return Err("a spot is 5-120 seconds".into());
        }
        if !["none", "jingle", "music"].contains(&self.sound.as_str()) {
            return Err("sound must be none, jingle or music".into());
        }
        if self.sound == "music" && self.music.as_deref().is_none_or(|m| m.trim().is_empty()) {
            return Err("pick the music file".into());
        }
        for p in [&self.picture, &self.music].into_iter().flatten() {
            if p.len() > 500 || p.chars().any(char::is_control) {
                return Err("a file path is not valid".into());
            }
        }
        if let Some(l) = self
            .look
            .as_deref()
            .filter(|l| !l.is_empty() && *l != "none")
        {
            if serde_json::from_value::<Look>(json!(l)).is_err() {
                return Err("look must be none, vhs, crt, film, black_white or sepia".into());
            }
        }
        if self.year.is_some_and(|y| !(1900..=2100).contains(&y)) {
            return Err("the year must be 1900-2100".into());
        }
        if let Some(t) = &self.theme {
            t.validate()?;
        }
        Ok(())
    }

    fn spec(&self) -> PromoSpec {
        PromoSpec {
            kind: match self.kind.as_str() {
                "coming_up" => PromoKind::ComingUp,
                "slides" => PromoKind::Slides,
                _ => PromoKind::Sponsor,
            },
            title: self.title.trim().to_string(),
            tagline: self.tagline.trim().to_string(),
            contact: self.contact.trim().to_string(),
            slides: self
                .slides
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }
}

/// The choices the maker offers, so every app shows the same ones.
pub async fn options() -> Json<Value> {
    Json(json!({
        "kinds": [
            { "id": "sponsor", "name": "Sponsor or advert", "about": "A big name, a tagline and a contact line: “Mel's Diner — best pie in town — call 555-0134”." },
            { "id": "coming_up", "name": "Programme promo", "about": "“Coming up”: the show's name and when it is on." },
            { "id": "slides", "name": "Slides", "about": "A few lines of text, one after another: a menu, a notice, a quiz." },
        ],
        "looks": [
            { "id": "none", "name": "Clean" },
            { "id": "vhs", "name": "VHS tape" },
            { "id": "crt", "name": "Tube TV" },
            { "id": "film", "name": "Old film" },
            { "id": "black_white", "name": "Black & white" },
            { "id": "sepia", "name": "Sepia" },
        ],
        "sounds": [
            { "id": "jingle", "name": "A little jingle" },
            { "id": "music", "name": "Music from my library" },
            { "id": "none", "name": "Silent" },
        ],
        "seconds": [10, 15, 20, 30, 45, 60],
    }))
}

/// `GET /api/v1/promos/music`: audio files in the music libraries, to put under a spot.
pub async fn music(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let db = s.db();
    let libs: Vec<i64> = tvmaster_db::list_libraries(&db)?
        .into_iter()
        .filter(|l| l.kind == "music" && l.enabled)
        .map(|l| l.id)
        .collect();
    let rows = tvmaster_db::list_media_of_kind(&db, &libs, "music")?;
    Ok(Json(Value::Array(
        rows.into_iter()
            .take(500)
            .map(|r| {
                let name = FsPath::new(&r.path)
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                json!({ "path": r.path, "name": name, "seconds": r.duration_ms / 1000 })
            })
            .collect(),
    )))
}

/// Theme for a spot: its own, or the station's. The title gives "automatic" its colours.
fn theme_of(s: &AppState, b: &PromoBody) -> tvmaster_render::Theme {
    let choice = b
        .theme
        .clone()
        .unwrap_or_else(|| crate::theme::station(&s.db()));
    let id = if b.title.trim().is_empty() {
        "promo"
    } else {
        b.title.trim()
    };
    tvmaster_playout::card_theme(&choice, id)
}

/// The picture behind the words, fitted to a little more than the frame so it can drift.
fn load_photo(s: &AppState, b: &PromoBody, w: usize, h: usize) -> Result<Option<Photo>, String> {
    let Some(p) = b.picture.as_deref().filter(|p| !p.trim().is_empty()) else {
        return Ok(None);
    };
    let ff = s.ffmpeg.as_ref().ok_or("ffmpeg is not available")?;
    if !FsPath::new(p).is_file() {
        return Err("that picture is not on the server".into());
    }
    let (pw, ph) = (w * 115 / 100, h * 115 / 100);
    let rgb = tvmaster_playout::background_image(&ff.ffmpeg, FsPath::new(p), pw, ph, 0)
        .ok_or("could not read that picture")?;
    Ok(Some(Photo { w: pw, h: ph, rgb }))
}

/// `POST /api/v1/promos/preview`: one frame of the spot, as a PNG.
pub async fn preview(State(s): State<AppState>, Json(b): Json<PromoBody>) -> ApiResult<Response> {
    b.validate().map_err(bad)?;
    let png = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let th = theme_of(&s, &b);
        let (w, h) = th.frame_size();
        let photo = load_photo(&s, &b, w, h)?;
        let secs = b.seconds as f32;
        let scene = Scene::Promo {
            spec: b.spec(),
            photo,
        };
        // Show it once everything has arrived, unless asked for a moment.
        let t = if b.t > 0.0 {
            b.t.min(secs)
        } else {
            (secs * 0.4).min(3.0)
        };
        let c = tvmaster_render::render(&scene, &th, t, secs);
        let mut rgba = Vec::with_capacity(c.w * c.h * 4);
        for px in c.px.as_chunks::<3>().0 {
            rgba.extend_from_slice(px);
            rgba.push(255);
        }
        Ok(tvmaster_render::png::rgba_png(
            c.w as u32, c.h as u32, &rgba,
        ))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    let mut res = png.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}

/// A bright three-note arpeggio at the start and a chime at the end, made from nothing.
fn jingle(seconds: u32) -> String {
    let d = seconds as f32;
    let note =
        "if(lt(t\\,0.25)\\,523.25\\,if(lt(t\\,0.5)\\,659.25\\,if(lt(t\\,0.75)\\,783.99\\,1046.5)))";
    let e = format!(
        "0.3*lt(t\\,1.0)*sin(2*PI*{note}*t)*exp(-5*mod(t\\,0.25))+0.22*gt(t\\,{end:.2})*sin(2*PI*1046.5*t)*exp(-4*(t-{end:.2}))",
        end = d - 1.2
    );
    format!("aevalsrc=exprs={e}|{e}:s=48000:d={d}")
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').chars().take(40).collect::<String>();
    if out.is_empty() {
        "promo".into()
    } else {
        out
    }
}

/// Draw the frames and have FFmpeg make them into a video file.
fn render_file(s: &AppState, b: &PromoBody, out: &FsPath) -> Result<(), String> {
    let ff = s.ffmpeg.as_ref().ok_or("ffmpeg is not available")?;
    let th = theme_of(s, b);
    let (w, h) = th.frame_size();
    let photo = load_photo(s, b, w, h)?;
    let scene = Scene::Promo {
        spec: b.spec(),
        photo,
    };
    let secs = b.seconds;
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
    ]
    .iter()
    .map(|a| a.to_string())
    .collect();
    args.extend([
        format!("{w}x{h}"),
        "-r".into(),
        FPS.to_string(),
        "-i".into(),
        "-".into(),
    ]);
    let mut af: Option<String> = None;
    match b.sound.as_str() {
        "jingle" => args.extend(["-f".into(), "lavfi".into(), "-i".into(), jingle(secs)]),
        "music" => {
            let m = b.music.clone().unwrap_or_default();
            if !FsPath::new(&m).is_file() {
                return Err("that music file is not on the server".into());
            }
            args.extend(["-stream_loop".into(), "-1".into(), "-i".into(), m]);
            af = Some(format!("afade=t=out:st={}:d=1.5", secs as f32 - 1.5));
        }
        _ => args.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "anullsrc=r=48000:cl=stereo".into(),
        ]),
    }
    let mut vf: Vec<String> = Vec::new();
    if !th.modern {
        vf.push("scale=1280:720:flags=neighbor".into());
    }
    if let Some(l) = b
        .look
        .as_deref()
        .filter(|l| !l.is_empty())
        .and_then(|l| serde_json::from_value::<Look>(json!(l)).ok())
    {
        if let Some(chain) = tvmaster_playout::look_chain(l) {
            vf.push(chain.to_string());
        }
    }
    vf.push("format=yuv420p".into());
    args.extend(["-vf".into(), vf.join(",")]);
    if let Some(a) = af {
        args.extend(["-af".into(), a]);
    }
    args.extend(["-t".into(), secs.to_string(), "-r".into(), FPS.to_string()]);
    if ff.has_libx264 {
        args.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf", "20"].map(String::from));
    } else {
        args.extend(["-c:v", "mpeg4", "-q:v", "3"].map(String::from));
    }
    args.extend(
        [
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-movflags",
            "+faststart",
        ]
        .map(String::from),
    );
    args.push(out.to_string_lossy().to_string());
    let mut child = Command::new(&ff.ffmpeg)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start ffmpeg: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no input to ffmpeg")?;
    let mut stderr = child.stderr.take().ok_or("no output from ffmpeg")?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        text
    });
    let frames = secs * FPS;
    let mut write_err = None;
    for i in 0..frames {
        let t = i as f32 / FPS as f32;
        let c = tvmaster_render::render(&scene, &th, t, secs as f32);
        if let Err(e) = stdin.write_all(&c.px) {
            write_err = Some(e);
            break;
        }
    }
    drop(stdin);
    let status = child.wait().map_err(|e| e.to_string())?;
    let log = reader.join().unwrap_or_default();
    if !status.success() || write_err.is_some() {
        let _ = std::fs::remove_file(out);
        return Err(format!(
            "ffmpeg could not make the spot: {}",
            log.lines().last().unwrap_or("it stopped early")
        ));
    }
    Ok(())
}

/// `POST /api/v1/promos`: make the spot and add it to the commercials.
pub async fn create(State(s): State<AppState>, Json(b): Json<PromoBody>) -> ApiResult<Json<Value>> {
    b.validate().map_err(bad)?;
    if s.ffmpeg.is_none() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ffmpeg is not available",
        ));
    }
    let lib = match b.library {
        Some(id) => tvmaster_db::get_library(&s.db(), id)?
            .filter(|l| l.kind == "commercials")
            .ok_or_else(|| bad("that is not a commercials library"))?,
        None => libraries::ensure_station_folder(&s)?
            .1
            .into_iter()
            .find(|l| l.kind == "commercials")
            .ok_or_else(|| internal("no commercials library"))?,
    };
    let mut dir = PathBuf::from(&lib.path).join(FOLDER);
    if let Some(y) = b.year {
        dir = dir.join(y.to_string());
    }
    std::fs::create_dir_all(&dir).map_err(|e| internal(format!("cannot make the folder: {e}")))?;
    let label = if b.name.trim().is_empty() {
        &b.title
    } else {
        &b.name
    };
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let dest = dir.join(format!("{}-{stamp}.mp4", slug(label)));
    let part = dir.join(format!(".{}.part.mp4", slug(label)));
    let _ = std::fs::remove_file(&part);
    let (st, bb, p) = (s.clone(), b.clone(), part.clone());
    tokio::task::spawn_blocking(move || render_file(&st, &bb, &p))
        .await
        .map_err(internal)?
        .map_err(|m| ApiError::new(StatusCode::BAD_GATEWAY, m))?;
    std::fs::rename(&part, &dest).map_err(internal)?;
    let note = crate::monitor::rescan_with(&s, &lib, std::time::Duration::ZERO).await;
    let id = tvmaster_db::library_media_paths(&s.db(), lib.id)?
        .into_iter()
        .find(|(_, p, missing)| !missing && FsPath::new(p) == dest)
        .map(|(id, _, _)| id);
    Ok(Json(json!({
        "id": id,
        "path": dest.to_string_lossy(),
        "seconds": b.seconds,
        "library": lib.id,
        "added": note.map_or(0, |n| n.added),
    })))
}

fn is_promo_path(p: &str) -> bool {
    p.replace('\\', "/").contains(&format!("/{FOLDER}/"))
}

/// `GET /api/v1/promos`: the spots made so far.
pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let db = s.db();
    let libs: Vec<i64> = tvmaster_db::list_libraries(&db)?
        .into_iter()
        .filter(|l| l.kind == "commercials" && l.enabled)
        .map(|l| l.id)
        .collect();
    let mut rows = tvmaster_db::list_media_of_kind(&db, &libs, "commercials")?;
    rows.retain(|r| is_promo_path(&r.path));
    rows.sort_by_key(|r| std::cmp::Reverse(r.id));
    Ok(Json(Value::Array(
        rows.into_iter()
            .map(|r| {
                let name = FsPath::new(&r.path).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                json!({ "id": r.id, "name": name, "seconds": r.duration_ms / 1000, "path": r.path, "library": r.library_id })
            })
            .collect(),
    )))
}

/// `DELETE /api/v1/promos/:id`: remove a spot made here (its file and its place in the schedule).
pub async fn remove(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let (lib, rel) = {
        let db = s.db();
        let m = tvmaster_db::get_media(&db, id)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such spot"))?;
        if !is_promo_path(&m.path) {
            return Err(bad("only spots made here can be removed here"));
        }
        let lib = tvmaster_db::get_library(&db, m.library_id)?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such library"))?;
        let rel = FsPath::new(&m.path)
            .strip_prefix(&lib.path)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .map_err(|_| bad("that file is not in its library"))?;
        (lib, rel)
    };
    libraries::delete_file(
        State(s),
        Path(lib.id),
        axum::extract::Query(libraries::FileQuery {
            path: rel,
            overwrite: false,
        }),
    )
    .await
}

#[cfg(test)]
mod tests {
    use crate::test_util::{call, call_raw};
    use crate::{router, AppState};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn the_maker_offers_choices_and_refuses_what_makes_no_sense() {
        let app = router(AppState::in_memory());
        let (st, v) = call(&app, "GET", "/api/v1/promos/options", "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["kinds"].as_array().unwrap().len(), 3);
        for bad in [
            r#"{"kind":"nope","title":"x"}"#,
            r#"{"kind":"sponsor"}"#,
            r#"{"kind":"slides","slides":[]}"#,
            r#"{"title":"x","seconds":2}"#,
            r#"{"title":"x","sound":"music"}"#,
            r#"{"title":"x","look":"neon"}"#,
            r#"{"title":"x","year":1850}"#,
        ] {
            let (st, _) = call_raw(&app, "POST", "/api/v1/promos/preview", bad).await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}");
        }
        let (st, png) = call_raw(&app, "POST", "/api/v1/promos/preview", r#"{"kind":"sponsor","title":"Mel's Diner","tagline":"Best pie in town","contact":"Call 555-0134"}"#).await;
        assert_eq!(st, StatusCode::OK);
        assert!(png.starts_with("\u{fffd}PNG"));
        // Without FFmpeg a spot cannot be made.
        let (st, _) = call(&app, "POST", "/api/v1/promos", r#"{"title":"x"}"#).await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn names_become_file_names_and_the_jingle_is_one_expression() {
        assert_eq!(super::slug("Mel's Diner!!"), "mel-s-diner");
        assert_eq!(super::slug("???"), "promo");
        let j = super::jingle(15);
        assert!(j.starts_with("aevalsrc=exprs=") && j.contains(":s=48000:d=15"));
        assert!(j.contains("\\,"), "commas are escaped for the filter graph");
    }

    #[tokio::test]
    async fn a_spot_is_made_into_a_video_that_is_on_the_schedule_and_can_be_removed() {
        let Ok(info) = tvmaster_media::FfmpegInfo::detect("ffmpeg", "ffprobe") else {
            eprintln!("ffmpeg not installed; skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("tvm-promo-{}", crate::generate_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = AppState::new(tvmaster_db::open_in_memory().unwrap(), Some(info), None)
            .with_data_dir(&dir);
        let app = router(state.clone());
        for (kind, look, sound) in [("sponsor", "vhs", "jingle"), ("slides", "", "none")] {
            let body = format!(
                r#"{{"kind":"{kind}","title":"Mel's Diner","tagline":"Best pie in town","contact":"Call 555-0134","slides":["Pie","Coffee"],"seconds":5,"look":"{look}","sound":"{sound}","theme":{{"theme":"neon","style":"modern"}}}}"#
            );
            let (st, v) = call(&app, "POST", "/api/v1/promos", &body).await;
            assert_eq!(st, StatusCode::OK, "{v}");
            assert!(v["id"].is_number(), "indexed at once: {v}");
        }
        let (_, list) = call(&app, "GET", "/api/v1/promos", "").await;
        let list = list.as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert!(
            list.iter()
                .all(|p| (4..=6).contains(&p["seconds"].as_i64().unwrap())),
            "{list:?}"
        );
        // It really is a playable video with sound.
        let path = list[0]["path"].as_str().unwrap();
        let probe = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
                path,
            ])
            .output()
            .unwrap();
        let kinds = String::from_utf8_lossy(&probe.stdout).to_string();
        assert!(
            kinds.contains("video") && kinds.contains("audio"),
            "{kinds}"
        );
        let id = list[0]["id"].as_i64().unwrap();
        let (st, _) = call(&app, "DELETE", &format!("/api/v1/promos/{id}"), "").await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        let (_, after) = call(&app, "GET", "/api/v1/promos", "").await;
        assert_eq!(after.as_array().unwrap().len(), 1);
        assert!(!std::path::Path::new(path).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
