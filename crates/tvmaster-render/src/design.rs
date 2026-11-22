//! Designed screens: stand-by, sign-off, "Up next", "Later today", "We'll be right back" and the
//! rest, described as data (a background and a stack of layers, each with an entrance and a loop)
//! and drawn at 1280x720 with smooth type. The same description is drawn whatever a client shows,
//! so the web admin, the Mac app and the TV all preview and air exactly one thing.
//!
//! Everything is whole numbers (positions are thousandths of the screen, times are milliseconds,
//! speeds are percent) so a design compares and saves without surprises.

use crate::alert::put;
use crate::modern::{backdrop, fit};
use crate::text::{self, Face};
use crate::{mix, Canvas, Rgb, Theme};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::f32::consts::TAU;

const W: i32 = 1280;
const H: i32 = 720;

fn d_center() -> i32 {
    500
}
fn d_ink() -> String {
    "ink".into()
}
fn d_100() -> u32 {
    100
}
fn d_600() -> u32 {
    600
}
fn yes() -> bool {
    true
}

// ------------------------------------------------------------------------------- the model

/// What is behind everything.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Backdrop {
    /// The station theme's own colours, with a soft glow that drifts.
    #[default]
    Theme,
    Solid {
        color: String,
    },
    Gradient {
        top: String,
        bottom: String,
    },
    /// Soft blobs of colour drifting about: the liquid look under glass panels.
    Aurora {
        colors: Vec<String>,
    },
    /// SMPTE colour bars.
    Bars,
    /// A quietly moving pattern.
    Pattern {
        kind: PatternKind,
        color: String,
        bg: String,
        #[serde(default = "d_size")]
        size: u32,
    },
    /// A picture of your own, filling the screen.
    Picture {
        path: String,
    },
}

fn d_size() -> u32 {
    48
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    #[default]
    Dots,
    Grid,
    Stripes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

/// How a layer arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Enter {
    None,
    #[default]
    Fade,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Zoom,
    /// Letters appear one by one (text only; others fade).
    Type,
    /// Revealed left to right.
    Wipe,
}

/// What a layer keeps doing while it is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    #[default]
    None,
    /// Breathes in and out.
    Pulse,
    /// Floats up and down.
    Float,
    /// Bounces slowly around the screen, like a screensaver.
    Drift,
    /// Fades in and out.
    Blink,
    /// Scrolls across the screen again and again (text).
    Marquee,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anim {
    #[serde(default)]
    pub enter: Enter,
    /// How long the entrance takes.
    #[serde(default = "d_600")]
    pub enter_ms: u32,
    /// Wait this long before arriving.
    #[serde(default)]
    pub delay_ms: u32,
    #[serde(default)]
    pub motion: Motion,
    /// How fast the motion goes, percent (100 = normal).
    #[serde(default = "d_100")]
    pub speed: u32,
}

impl Default for Anim {
    fn default() -> Self {
        Self {
            enter: Enter::Fade,
            enter_ms: 600,
            delay_ms: 0,
            motion: Motion::None,
            speed: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerKind {
    /// Words. `{channel}`, `{number}`, `{show}`, `{title}`, `{time}`, `{message}`, `{clock}` and
    /// `{date}` are filled in when the screen airs.
    Text {
        text: String,
        #[serde(default = "d_48")]
        size: u32,
        #[serde(default = "yes")]
        bold: bool,
        /// The chunky pixel font instead of the smooth one.
        #[serde(default)]
        pixel: bool,
        #[serde(default = "d_ink")]
        color: String,
        #[serde(default)]
        shadow: bool,
        /// Widest the text may be, thousandths of the screen width.
        #[serde(default = "d_900")]
        width: u32,
        #[serde(default = "d_2")]
        lines: u32,
    },
    /// The time of day, ticking.
    Clock {
        #[serde(default = "d_64")]
        size: u32,
        #[serde(default = "yes")]
        bold: bool,
        #[serde(default = "d_ink")]
        color: String,
        #[serde(default = "yes")]
        hour12: bool,
        #[serde(default)]
        seconds: bool,
        /// Add the date under the time.
        #[serde(default)]
        date: bool,
    },
    /// A block of colour: a bar, a card, a pill, a circle, or frosted glass.
    Shape {
        /// Width, thousandths of the screen width.
        w: u32,
        /// Height, thousandths of the screen height.
        h: u32,
        #[serde(default = "d_accent")]
        color: String,
        /// A second colour for a top-to-bottom gradient.
        #[serde(default)]
        color2: Option<String>,
        /// Corner radius in pixels.
        #[serde(default)]
        radius: u32,
        /// An ellipse (a circle when as wide as it is tall).
        #[serde(default)]
        circle: bool,
        /// Frosted glass: blurs what is behind it.
        #[serde(default)]
        glass: bool,
    },
    /// The next few programmes: a time and a title on each line.
    List {
        #[serde(default = "d_40")]
        size: u32,
        #[serde(default = "d_ink")]
        color: String,
        /// Colour of the times.
        #[serde(default = "d_accent_text")]
        time_color: String,
        #[serde(default = "d_4")]
        rows: u32,
        /// Widest a line may be, thousandths of the screen width.
        #[serde(default = "d_900")]
        width: u32,
    },
    /// A picture of your own (a logo, a photo).
    Image { path: String, w: u32, h: u32 },
}

fn d_48() -> u32 {
    48
}
fn d_64() -> u32 {
    64
}
fn d_40() -> u32 {
    40
}
fn d_900() -> u32 {
    900
}
fn d_2() -> u32 {
    2
}
fn d_4() -> u32 {
    4
}
fn d_accent() -> String {
    "accent".into()
}
fn d_accent_text() -> String {
    "accent_text".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    #[serde(flatten)]
    pub kind: LayerKind,
    /// Where it sits, thousandths of the screen from the left and the top.
    #[serde(default = "d_center")]
    pub x: i32,
    #[serde(default = "d_center")]
    pub y: i32,
    /// Which edge `x` refers to.
    #[serde(default)]
    pub align: Align,
    /// 0-100.
    #[serde(default = "d_100")]
    pub opacity: u32,
    #[serde(default)]
    pub anim: Anim,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Design {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub backdrop: Backdrop,
    #[serde(default)]
    pub layers: Vec<Layer>,
    /// Everything fades out over this long at the end of a screen that has an end (0 = cuts).
    #[serde(default)]
    pub fade_out_ms: u32,
}

/// What fills the `{...}` words, and the time the clock starts from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vars {
    pub channel: String,
    pub number: u32,
    pub show: String,
    pub title: String,
    pub time: String,
    pub message: String,
    /// The next few programmes: (time, title).
    pub items: Vec<(String, String)>,
    /// Local seconds after midnight when the screen starts.
    pub clock0_secs: u32,
    pub date: String,
}

/// A picture the design uses, as RGBA (a logo keeps its see-through parts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub rgba: std::sync::Arc<Vec<u8>>,
}

/// Pictures the design uses, loaded by the caller and keyed by the path the design names.
pub type Images = BTreeMap<String, Picture>;

impl Design {
    /// Every picture path the design wants loaded.
    pub fn pictures(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Backdrop::Picture { path } = &self.backdrop {
            out.push(path.clone());
        }
        for l in &self.layers {
            if let LayerKind::Image { path, .. } = &l.kind {
                out.push(path.clone());
            }
        }
        out.retain(|p| !p.is_empty());
        out.sort();
        out.dedup();
        out
    }

    /// The first thing wrong with a design, in words.
    pub fn problem(&self) -> Option<String> {
        if self.layers.len() > 40 {
            return Some("A screen can have at most 40 layers.".into());
        }
        for (i, l) in self.layers.iter().enumerate() {
            let n = i + 1;
            if !(-500..=1500).contains(&l.x) || !(-500..=1500).contains(&l.y) {
                return Some(format!("Layer {n} is far off the screen."));
            }
            if l.opacity > 100 {
                return Some(format!("Layer {n}: opacity is 0 to 100."));
            }
            if l.anim.enter_ms > 20_000 || l.anim.delay_ms > 60_000 {
                return Some(format!(
                    "Layer {n}: an entrance is at most 20 seconds, a delay 60."
                ));
            }
            if !(10..=500).contains(&l.anim.speed) {
                return Some(format!("Layer {n}: speed is 10 to 500 percent."));
            }
            match &l.kind {
                LayerKind::Text {
                    size, lines, text, ..
                } => {
                    if !(8..=400).contains(size) {
                        return Some(format!("Layer {n}: text size is 8 to 400."));
                    }
                    if !(1..=6).contains(lines) {
                        return Some(format!("Layer {n}: 1 to 6 lines."));
                    }
                    if text.chars().count() > 400 {
                        return Some(format!("Layer {n}: that is a lot of text."));
                    }
                }
                LayerKind::Clock { size, .. } | LayerKind::List { size, .. }
                    if !(8..=400).contains(size) =>
                {
                    return Some(format!("Layer {n}: size is 8 to 400."));
                }
                LayerKind::Shape { w, h, .. } | LayerKind::Image { w, h, .. }
                    if *w > 2000 || *h > 2000 =>
                {
                    return Some(format!("Layer {n}: too big."));
                }
                _ => {}
            }
        }
        None
    }
}

/// Fill the `{...}` words in `s`.
pub fn fill(s: &str, v: &Vars, t: f32, hour12: bool, seconds: bool) -> String {
    let secs = v.clock0_secs + t.max(0.0) as u32;
    let (h, m, sec) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    let clock = clock_text(h, m, sec, hour12, seconds);
    s.replace("{channel}", &v.channel)
        .replace("{number}", &v.number.to_string())
        .replace("{show}", &v.show)
        .replace("{title}", &v.title)
        .replace("{next_title}", &v.title)
        .replace("{time}", &v.time)
        .replace("{next_time}", &v.time)
        .replace("{message}", &v.message)
        .replace("{clock}", &clock)
        .replace("{date}", &v.date)
}

fn clock_text(h: u32, m: u32, s: u32, hour12: bool, seconds: bool) -> String {
    let tail = if seconds {
        format!(":{s:02}")
    } else {
        String::new()
    };
    if hour12 {
        let h12 = match h % 12 {
            0 => 12,
            x => x,
        };
        format!("{h12}:{m:02}{tail} {}", if h < 12 { "AM" } else { "PM" })
    } else {
        format!("{h:02}:{m:02}{tail}")
    }
}

// ------------------------------------------------------------------------------ colours

fn colour(spec: &str, th: &Theme) -> Rgb {
    match spec.trim() {
        "primary" => th.primary,
        "accent" => th.accent,
        // the accent, lifted when it would be hard to read as text on a dark screen
        "accent_text" => legible(th.accent),
        "primary_text" => legible(th.primary),
        "ink" => th.ink,
        "dark" => th.dark,
        "bottom" => th.bottom,
        "on_accent" => th.on_accent,
        "white" => [255, 255, 255],
        "black" => [0, 0, 0],
        s => hex(s).unwrap_or(th.ink),
    }
}

fn legible(c: Rgb) -> Rgb {
    let lum = 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
    if lum >= 150.0 {
        c
    } else {
        mix(c, [255, 255, 255], ((150.0 - lum) / 150.0 * 0.75).min(0.7))
    }
}

/// `#rgb` or `#rrggbb`.
pub fn hex(s: &str) -> Option<Rgb> {
    let s = s.strip_prefix('#')?;
    let p = |a: usize, n: usize| u8::from_str_radix(s.get(a..a + n)?, 16).ok();
    match s.len() {
        3 => Some([p(0, 1)? * 17, p(1, 1)? * 17, p(2, 1)? * 17]),
        6 => Some([p(0, 2)?, p(2, 2)?, p(4, 2)?]),
        _ => None,
    }
}

// ---------------------------------------------------------------------------- drawing

fn alpha8(a: f32) -> u8 {
    (a.clamp(0.0, 1.0) * 255.0) as u8
}

/// Coverage (0-1) of a rounded rectangle at pixel (px, py) relative to its top-left, with the
/// corner radius `r`: anti-aliased at the corners.
fn rr_cover(px: i32, py: i32, w: i32, h: i32, r: f32) -> f32 {
    if r <= 0.5 {
        return 1.0;
    }
    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
    let (cx, cy) = (fx.clamp(r, w as f32 - r), fy.clamp(r, h as f32 - r));
    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
    (r - d + 0.5).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn fill_rr(
    c: &mut Canvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: f32,
    top: Rgb,
    bottom: Rgb,
    alpha: f32,
    clip_w: i32,
) {
    for row in 0..h {
        let col = mix(top, bottom, row as f32 / (h - 1).max(1) as f32);
        for cx in 0..w.min(clip_w) {
            let cov = rr_cover(cx, row, w, h, r) * alpha;
            if cov > 0.003 {
                put(c, x + cx, y + row, col, alpha8(cov));
            }
        }
    }
}

/// A box blur of the rectangle, as RGB.
fn blur_rect(c: &Canvas, x: i32, y: i32, w: i32, h: i32, r: i32) -> Vec<u8> {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let mut a = vec![0u8; w * h * 3];
    for row in 0..h {
        for col in 0..w {
            let sx = (x + col as i32).clamp(0, c.w as i32 - 1) as usize;
            let sy = (y + row as i32).clamp(0, c.h as i32 - 1) as usize;
            let i = (sy * c.w + sx) * 3;
            a[(row * w + col) * 3..(row * w + col) * 3 + 3].copy_from_slice(&c.px[i..i + 3]);
        }
    }
    let r = r.max(1) as usize;
    let mut b = a.clone();
    for _ in 0..2 {
        // horizontal
        for row in 0..h {
            for ch in 0..3 {
                let mut sum = 0u32;
                let mut cnt = 0u32;
                let at = |i: usize| a[(row * w + i) * 3 + ch] as u32;
                for i in 0..r.min(w) {
                    sum += at(i);
                    cnt += 1;
                }
                for col in 0..w {
                    if col + r < w {
                        sum += at(col + r);
                        cnt += 1;
                    }
                    if col > r {
                        sum -= at(col - r - 1);
                        cnt -= 1;
                    }
                    b[(row * w + col) * 3 + ch] = (sum / cnt.max(1)) as u8;
                }
            }
        }
        // vertical
        for col in 0..w {
            for ch in 0..3 {
                let mut sum = 0u32;
                let mut cnt = 0u32;
                let at = |j: usize| b[(j * w + col) * 3 + ch] as u32;
                for j in 0..r.min(h) {
                    sum += at(j);
                    cnt += 1;
                }
                for row in 0..h {
                    if row + r < h {
                        sum += at(row + r);
                        cnt += 1;
                    }
                    if row > r {
                        sum -= at(row - r - 1);
                        cnt -= 1;
                    }
                    a[(row * w + col) * 3 + ch] = (sum / cnt.max(1)) as u8;
                }
            }
        }
    }
    a
}

/// Frosted glass: the blurred picture behind, a light tint, a bright edge and a soft sheen.
#[allow(clippy::too_many_arguments)]
fn glass(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, r: f32, tint: Rgb, alpha: f32) {
    if w < 4 || h < 4 {
        return;
    }
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = ((x + w).min(c.w as i32), (y + h).min(c.h as i32));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let blurred = blur_rect(c, x, y, w, h, 14);
    let bw = w as usize;
    for row in y0..y1 {
        for col in x0..x1 {
            let (lx, ly) = (col - x, row - y);
            let cov = rr_cover(lx, ly, w, h, r) * alpha;
            if cov < 0.003 {
                continue;
            }
            let bi = (ly as usize * bw + lx as usize) * 3;
            let di = (row as usize * c.w + col as usize) * 3;
            let sheen = (1.0 - ly as f32 / h as f32).powi(2) * 0.16;
            for k in 0..3 {
                let behind = blurred[bi + k] as f32;
                let lit = behind + (tint[k] as f32 - behind) * 0.2 + 255.0 * (0.07 + sheen);
                let dst = c.px[di + k] as f32;
                c.px[di + k] = (dst + (lit.min(255.0) - dst) * cov) as u8;
            }
        }
    }
    // a bright 2px rim, brighter at the top
    for row in y0..y1 {
        for col in x0..x1 {
            let (lx, ly) = (col - x, row - y);
            let outer = rr_cover(lx, ly, w, h, r);
            let inner = rr_cover(lx - 2, ly - 2, w - 4, h - 4, (r - 2.0).max(0.0));
            let ring = (outer - inner).clamp(0.0, 1.0) * alpha;
            if ring > 0.01 {
                let k = 0.35 + 0.4 * (1.0 - ly as f32 / h as f32);
                put(c, col, row, [255, 255, 255], alpha8(ring * k));
            }
        }
    }
}

fn face(bold: bool) -> Face {
    if bold {
        Face::Bold
    } else {
        Face::Regular
    }
}

/// Smooth text with alpha, its line box's top at `y`.
#[allow(clippy::too_many_arguments)]
fn draw_text(c: &mut Canvas, x: i32, y: i32, s: &str, px: f32, f: Face, col: Rgb, alpha: f32) {
    let baseline = y + (px * 0.82) as i32;
    for (ch, off) in text::layout(f, s, px) {
        let g = text::glyph(f, ch, px);
        let gx = x + off.round() as i32 + g.left;
        let gy = baseline - g.top;
        for row in 0..g.h {
            for k in 0..g.w {
                let cov = g.coverage[row * g.w + k] as f32 / 255.0;
                put(c, gx + k as i32, gy + row as i32, col, alpha8(cov * alpha));
            }
        }
    }
}

/// The chunky pixel font with alpha.
fn draw_pixel(c: &mut Canvas, x: i32, y: i32, s: &str, scale: i32, col: Rgb, alpha: f32) {
    let w = crate::text_width(s, scale).max(1) as usize;
    let mut tmp = Canvas::new(w, (8 * scale) as usize);
    tmp.text(0, 0, s, scale, [255, 255, 255]);
    for ty in 0..tmp.h {
        for tx in 0..tmp.w {
            if tmp.px[(ty * tmp.w + tx) * 3] > 0 {
                put(c, x + tx as i32, y + ty as i32, col, alpha8(alpha));
            }
        }
    }
}

fn pixel_width(s: &str, scale: i32) -> i32 {
    crate::text_width(s, scale)
}

// --------------------------------------------------------------------------- backdrops

#[allow(clippy::too_many_arguments)]
fn draw_photo(
    c: &mut Canvas,
    p: &Picture,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    cover: bool,
    alpha: f32,
) {
    if w <= 0 || h <= 0 || p.w == 0 || p.h == 0 {
        return;
    }
    let (sw, sh) = (p.w as f32, p.h as f32);
    let k = if cover {
        (w as f32 / sw).max(h as f32 / sh)
    } else {
        (w as f32 / sw).min(h as f32 / sh)
    };
    let (dw, dh) = ((sw * k) as i32, (sh * k) as i32);
    let (ox, oy) = (x + (w - dw) / 2, y + (h - dh) / 2);
    for dy in 0..dh {
        let py = oy + dy;
        if py < y.max(0) || py >= (y + h).min(c.h as i32) {
            continue;
        }
        let sy = ((dy as f32 / k) as usize).min(p.h - 1);
        for dx in 0..dw {
            let px = ox + dx;
            if px < x.max(0) || px >= (x + w).min(c.w as i32) {
                continue;
            }
            let sx = ((dx as f32 / k) as usize).min(p.w - 1);
            let i = (sy * p.w + sx) * 4;
            put(
                c,
                px,
                py,
                [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2]],
                alpha8(alpha * p.rgba[i + 3] as f32 / 255.0),
            );
        }
    }
}

fn bars(c: &mut Canvas) {
    let cols: [Rgb; 7] = [
        [192, 192, 192],
        [192, 192, 0],
        [0, 192, 192],
        [0, 192, 0],
        [192, 0, 192],
        [192, 0, 0],
        [0, 0, 192],
    ];
    let (w, h) = (c.w as i32, c.h as i32);
    for (i, col) in cols.iter().enumerate() {
        let x0 = w * i as i32 / 7;
        let x1 = w * (i as i32 + 1) / 7;
        c.rect(x0, 0, x1 - x0, h * 67 / 100, *col);
        let rev: Rgb = [cols[6 - i][0], cols[6 - i][1], cols[6 - i][2]];
        c.rect(
            x0,
            h * 67 / 100,
            x1 - x0,
            h * 8 / 100,
            if i % 2 == 0 { rev } else { [19, 19, 19] },
        );
    }
    c.rect(0, h * 75 / 100, w, h - h * 75 / 100, [16, 16, 16]);
    c.rect(0, h * 75 / 100, w / 6, h * 25 / 100, [0, 33, 76]);
    c.rect(w / 6, h * 75 / 100, w / 6, h * 25 / 100, [255, 255, 255]);
    c.rect(w / 3, h * 75 / 100, w / 6, h * 25 / 100, [50, 0, 106]);
}

fn aurora(c: &mut Canvas, th: &Theme, colors: &[String], t: f32) {
    let cols: Vec<Rgb> = if colors.is_empty() {
        vec![th.primary, th.accent, mix(th.primary, th.accent, 0.5)]
    } else {
        colors.iter().map(|s| colour(s, th)).collect()
    };
    aurora_rgb(c, th.dark, th.bottom, &cols, t);
}

/// Soft blobs of colour drifting over a gradient, filling the canvas (any size).
pub(crate) fn aurora_rgb(c: &mut Canvas, dark: Rgb, bottom: Rgb, cols: &[Rgb], t: f32) {
    let (lw, lh) = ((c.w / 4).max(2), (c.h / 4).max(2));
    let mut field = vec![[0f32; 3]; lw * lh];
    let n = cols.len().max(1) as f32;
    for (y, row) in field.chunks_mut(lw).enumerate() {
        let base = mix(dark, bottom, y as f32 / (lh - 1) as f32);
        for px in row.iter_mut() {
            *px = [base[0] as f32, base[1] as f32, base[2] as f32];
        }
    }
    for (i, col) in cols.iter().enumerate() {
        let f = i as f32 / n;
        let cx = (0.18 + 0.64 * ((f * 2.3 + 0.2).fract())) * lw as f32
            + (t * 0.21 + i as f32 * 2.1).sin() * lw as f32 * 0.14;
        let cy = (0.25 + 0.5 * ((f * 1.7 + 0.35).fract())) * lh as f32
            + (t * 0.17 + i as f32 * 1.3).cos() * lh as f32 * 0.16;
        let r = lw as f32 * 0.42;
        for (y, row) in field.chunks_mut(lw).enumerate() {
            let dy = y as f32 - cy;
            for (x, px) in row.iter_mut().enumerate() {
                let dx = x as f32 - cx;
                let wgt = (-(dx * dx + dy * dy) / (r * r)).exp() * 0.75;
                for k in 0..3 {
                    px[k] += (col[k] as f32 - px[k]) * wgt;
                }
            }
        }
    }
    // upscale by 4 with bilinear smoothing
    for y in 0..c.h {
        let fy = (y as f32 / 4.0 - 0.5).max(0.0);
        let (y0, ty) = ((fy as usize).min(lh - 1), fy.fract());
        let y1 = (y0 + 1).min(lh - 1);
        for x in 0..c.w {
            let fx = (x as f32 / 4.0 - 0.5).max(0.0);
            let (x0, tx) = ((fx as usize).min(lw - 1), fx.fract());
            let x1 = (x0 + 1).min(lw - 1);
            let i = (y * c.w + x) * 3;
            for k in 0..3 {
                let a = field[y0 * lw + x0][k] * (1.0 - tx) + field[y0 * lw + x1][k] * tx;
                let b = field[y1 * lw + x0][k] * (1.0 - tx) + field[y1 * lw + x1][k] * tx;
                c.px[i + k] = (a * (1.0 - ty) + b * ty) as u8;
            }
        }
    }
}

fn pattern(c: &mut Canvas, kind: PatternKind, fg: Rgb, bg: Rgb, size: i32, t: f32) {
    c.fill(bg);
    let size = size.clamp(8, 200);
    let shift = (t * 14.0) as i32;
    match kind {
        PatternKind::Dots => {
            let mut y = -size + shift % size;
            while y < H + size {
                let mut x = -size + shift % size;
                while x < W + size {
                    crate::alert::disc(c, x + size / 2, y + size / 2, (size / 6).max(2), fg);
                    x += size;
                }
                y += size;
            }
        }
        PatternKind::Grid => {
            let mut x = shift % size;
            while x < W {
                c.rect(x, 0, 2, H, fg);
                x += size;
            }
            let mut y = shift % size;
            while y < H {
                c.rect(0, y, W, 2, fg);
                y += size;
            }
        }
        PatternKind::Stripes => {
            for y in 0..H {
                for x in (0..W).step_by(1) {
                    if ((x + y + shift * 2) / size) % 2 == 0 {
                        let i = (y as usize * c.w + x as usize) * 3;
                        c.px[i..i + 3].copy_from_slice(&fg);
                    }
                }
            }
        }
    }
}

fn draw_backdrop(c: &mut Canvas, d: &Design, th: &Theme, images: &Images, t: f32) {
    match &d.backdrop {
        Backdrop::Theme => backdrop(c, th, t),
        Backdrop::Solid { color } => c.fill(colour(color, th)),
        Backdrop::Gradient { top, bottom } => {
            crate::alert::vgrad(c, colour(top, th), colour(bottom, th));
        }
        Backdrop::Aurora { colors } => aurora(c, th, colors, t),
        Backdrop::Bars => bars(c),
        Backdrop::Pattern {
            kind,
            color,
            bg,
            size,
        } => {
            pattern(c, *kind, colour(color, th), colour(bg, th), *size as i32, t);
        }
        Backdrop::Picture { path } => {
            c.fill([0, 0, 0]);
            if let Some(p) = images.get(path) {
                draw_photo(c, p, 0, 0, W, H, true, 1.0);
            } else {
                crate::alert::vgrad(c, th.dark, th.bottom);
            }
        }
    }
}

// ------------------------------------------------------------------------------ layers

fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p).powi(3)
}

/// What a layer's entrance and motion do to it at time `t`.
struct Pose {
    alpha: f32,
    dx: f32,
    dy: f32,
    scale: f32,
    /// 0-1: how much is shown (typewriter, wipe).
    reveal: f32,
    /// Where a drifting layer is (replaces the position), as a 0-1 fraction of the free space.
    drift: Option<(f32, f32)>,
    /// A marquee's progress, 0-1.
    scroll: Option<f32>,
}

fn tri(x: f32) -> f32 {
    let f = x.rem_euclid(2.0);
    if f > 1.0 {
        2.0 - f
    } else {
        f
    }
}

fn pose(a: &Anim, t: f32) -> Pose {
    let speed = a.speed as f32 / 100.0;
    let dur = (a.enter_ms as f32 / 1000.0).max(0.001);
    let p = ease((t - a.delay_ms as f32 / 1000.0) / dur);
    let mut o = Pose {
        alpha: 1.0,
        dx: 0.0,
        dy: 0.0,
        scale: 1.0,
        reveal: 1.0,
        drift: None,
        scroll: None,
    };
    let away = 1.0 - p;
    match a.enter {
        Enter::None => {
            if t < a.delay_ms as f32 / 1000.0 {
                o.alpha = 0.0;
            }
        }
        Enter::Fade => o.alpha = p,
        Enter::SlideUp => {
            o.alpha = p;
            o.dy = away * 120.0;
        }
        Enter::SlideDown => {
            o.alpha = p;
            o.dy = -away * 120.0;
        }
        Enter::SlideLeft => {
            o.alpha = p;
            o.dx = away * 240.0;
        }
        Enter::SlideRight => {
            o.alpha = p;
            o.dx = -away * 240.0;
        }
        Enter::Zoom => {
            o.alpha = p;
            o.scale = 0.55 + 0.45 * p;
        }
        Enter::Type => {
            o.reveal = p;
            o.alpha = if p > 0.0 { 1.0 } else { 0.0 };
        }
        Enter::Wipe => {
            o.reveal = p;
            o.alpha = if p > 0.0 { 1.0 } else { 0.0 };
        }
    }
    let tt = t * speed;
    match a.motion {
        Motion::None => {}
        Motion::Pulse => o.scale *= 1.0 + 0.045 * (tt * TAU / 3.0).sin(),
        Motion::Float => o.dy += (tt * TAU / 4.0).sin() * 12.0,
        Motion::Blink => o.alpha *= 0.55 + 0.45 * (tt * TAU / 2.5).sin().abs().max(0.0),
        Motion::Drift => o.drift = Some((tri(tt * 0.11 + 0.15), tri(tt * 0.083 + 0.4))),
        Motion::Marquee => o.scroll = Some((tt * 0.09).fract()),
    }
    o
}

fn draw_layer(
    c: &mut Canvas,
    l: &Layer,
    v: &Vars,
    th: &Theme,
    images: &Images,
    t: f32,
    end_fade: f32,
) {
    let o = pose(&l.anim, t);
    let alpha = o.alpha * (l.opacity as f32 / 100.0) * end_fade;
    if alpha <= 0.003 {
        return;
    }
    let ax = (l.x as f32 * W as f32 / 1000.0) as i32;
    let ay = (l.y as f32 * H as f32 / 1000.0) as i32;
    match &l.kind {
        LayerKind::Text {
            text,
            size,
            bold,
            pixel,
            color,
            shadow,
            width,
            lines,
        } => {
            let s = fill(text, v, t, true, false);
            text_layer(
                c,
                th,
                &s,
                *size,
                *bold,
                *pixel,
                colour(color, th),
                *shadow,
                *width,
                *lines,
                l,
                ax,
                ay,
                &o,
                alpha,
            );
        }
        LayerKind::Clock {
            size,
            bold,
            color,
            hour12,
            seconds,
            date,
        } => {
            let s = fill("{clock}", v, t, *hour12, *seconds);
            text_layer(
                c,
                th,
                &s,
                *size,
                *bold,
                false,
                colour(color, th),
                false,
                900,
                1,
                l,
                ax,
                ay,
                &o,
                alpha,
            );
            if *date && !v.date.is_empty() {
                let sub = (*size as f32 * 0.42) as u32;
                text_layer(
                    c,
                    th,
                    &v.date,
                    sub,
                    false,
                    false,
                    colour(color, th),
                    false,
                    900,
                    1,
                    l,
                    ax,
                    ay + (*size as f32 * 0.85) as i32,
                    &o,
                    alpha * 0.8,
                );
            }
        }
        LayerKind::Shape {
            w,
            h,
            color,
            color2,
            radius,
            circle,
            glass: is_glass,
        } => {
            let k = o.scale;
            let (sw, sh) = (
                (*w as f32 * W as f32 / 1000.0 * k) as i32,
                (*h as f32 * H as f32 / 1000.0 * k) as i32,
            );
            if sw < 1 || sh < 1 {
                return;
            }
            let mut x = match l.align {
                Align::Left => ax,
                Align::Center => ax - sw / 2,
                Align::Right => ax - sw,
            };
            let mut y = ay - sh / 2;
            if let Some((fx, fy)) = o.drift {
                x = (fx * (W - sw).max(0) as f32) as i32;
                y = (fy * (H - sh).max(0) as f32) as i32;
            }
            x += o.dx as i32;
            y += o.dy as i32;
            let r = if *circle {
                (sw.min(sh) / 2) as f32
            } else {
                (*radius as f32 * k).min((sw.min(sh) / 2) as f32)
            };
            let top = colour(color, th);
            if *is_glass {
                glass(c, x, y, sw, sh, r, top, alpha);
            } else {
                let bottom = color2.as_deref().map(|s| colour(s, th)).unwrap_or(top);
                fill_rr(
                    c,
                    x,
                    y,
                    sw,
                    sh,
                    r,
                    top,
                    bottom,
                    alpha,
                    (sw as f32 * o.reveal) as i32,
                );
            }
        }
        LayerKind::List {
            size,
            color,
            time_color,
            rows,
            width,
        } => {
            let px = *size as f32 * o.scale;
            let f = Face::Bold;
            let gap = (px * 1.45) as i32;
            let n = v.items.len().min(*rows as usize);
            let total = gap * n as i32;
            let maxw = *width as f32 * W as f32 / 1000.0;
            let time_w = v
                .items
                .iter()
                .take(n)
                .map(|(tm, _)| text::width(f, tm, px))
                .fold(0.0, f32::max);
            for (i, (tm, title)) in v.items.iter().take(n).enumerate() {
                // each row arrives a little after the one before
                let rp = ease(
                    (t - l.anim.delay_ms as f32 / 1000.0 - i as f32 * 0.18)
                        / (l.anim.enter_ms as f32 / 1000.0).max(0.001),
                );
                let ra = alpha
                    * if matches!(l.anim.enter, Enter::None) {
                        1.0
                    } else {
                        rp
                    };
                if ra <= 0.003 {
                    continue;
                }
                let y = ay - total / 2
                    + i as i32 * gap
                    + o.dy as i32
                    + ((1.0 - rp) * 24.0) as i32 * i32::from(!matches!(l.anim.enter, Enter::None));
                let x0 = match l.align {
                    Align::Left => ax,
                    Align::Center => ax - (maxw / 2.0) as i32,
                    Align::Right => ax - maxw as i32,
                } + o.dx as i32;
                draw_text(c, x0, y, tm, px, f, colour(time_color, th), ra);
                let avail = (maxw - time_w - px * 0.8).max(40.0);
                let (p2, ls) = fit(title, Face::Regular, &[px, px * 0.85, px * 0.7], avail, 1);
                draw_text(
                    c,
                    x0 + (time_w + px * 0.8) as i32,
                    y + ((px - p2) * 0.3) as i32,
                    &ls.join(" "),
                    p2,
                    Face::Regular,
                    colour(color, th),
                    ra,
                );
            }
        }
        LayerKind::Image { path, w, h } => {
            let Some(p) = images.get(path) else { return };
            let k = o.scale;
            let (sw, sh) = (
                (*w as f32 * W as f32 / 1000.0 * k) as i32,
                (*h as f32 * H as f32 / 1000.0 * k) as i32,
            );
            let mut x = match l.align {
                Align::Left => ax,
                Align::Center => ax - sw / 2,
                Align::Right => ax - sw,
            };
            let mut y = ay - sh / 2;
            if let Some((fx, fy)) = o.drift {
                x = (fx * (W - sw).max(0) as f32) as i32;
                y = (fy * (H - sh).max(0) as f32) as i32;
            }
            draw_photo(c, p, x + o.dx as i32, y + o.dy as i32, sw, sh, false, alpha);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn text_layer(
    c: &mut Canvas,
    th: &Theme,
    s: &str,
    size: u32,
    bold: bool,
    pixel: bool,
    col: Rgb,
    shadow: bool,
    width: u32,
    lines: u32,
    l: &Layer,
    ax: i32,
    ay: i32,
    o: &Pose,
    alpha: f32,
) {
    let _ = th;
    if s.trim().is_empty() {
        return;
    }
    let max_w = width as f32 * W as f32 / 1000.0;
    let px = size as f32 * o.scale;
    let shown = |full: &str| -> String {
        if matches!(l.anim.enter, Enter::Type) && o.reveal < 1.0 {
            let n = (full.chars().count() as f32 * o.reveal).ceil() as usize;
            full.chars().take(n).collect()
        } else {
            full.to_string()
        }
    };
    if pixel {
        let scale = ((px / 8.0).round() as i32).max(1);
        let w = pixel_width(s, scale);
        let mut x = match l.align {
            Align::Left => ax,
            Align::Center => ax - w / 2,
            Align::Right => ax - w,
        };
        let mut y = ay - 4 * scale;
        if let Some((fx, fy)) = o.drift {
            x = (fx * (W - w).max(0) as f32) as i32;
            y = (fy * (H - 8 * scale).max(0) as f32) as i32;
        }
        if let Some(sc) = o.scroll {
            x = W - (sc * (W + w) as f32) as i32;
        }
        let txt = shown(s);
        if shadow {
            draw_pixel(
                c,
                x + o.dx as i32 + scale,
                y + o.dy as i32 + scale,
                &txt,
                scale,
                [0, 0, 0],
                alpha * 0.6,
            );
        }
        draw_pixel(c, x + o.dx as i32, y + o.dy as i32, &txt, scale, col, alpha);
        return;
    }
    let f = face(bold);
    let (px2, ls) = fit(
        s,
        f,
        &[px, px * 0.85, px * 0.72, px * 0.6],
        max_w,
        lines as usize,
    );
    let line_h = (px2 * 1.16) as i32;
    let block_h = line_h * ls.len() as i32;
    let widest = ls
        .iter()
        .map(|x| text::width(f, x, px2) as i32)
        .max()
        .unwrap_or(0);
    let mut y0 = ay - block_h / 2;
    let mut anchor_x = ax;
    if let Some((fx, fy)) = o.drift {
        anchor_x = (fx * (W - widest).max(0) as f32) as i32 + widest / 2;
        y0 = (fy * (H - block_h).max(0) as f32) as i32;
    }
    if let Some(sc) = o.scroll {
        anchor_x = W - (sc * (W + widest) as f32) as i32 + widest / 2;
    }
    let mut left_chars = if matches!(l.anim.enter, Enter::Type) && o.reveal < 1.0 {
        (ls.iter().map(|x| x.chars().count()).sum::<usize>() as f32 * o.reveal).ceil() as usize
    } else {
        usize::MAX
    };
    for (i, line) in ls.iter().enumerate() {
        let txt: String = line.chars().take(left_chars).collect();
        left_chars = left_chars.saturating_sub(line.chars().count());
        let w = text::width(f, line, px2) as i32;
        let x = match (l.align, o.scroll.is_some() || o.drift.is_some()) {
            (_, true) => anchor_x - w / 2,
            (Align::Left, _) => ax,
            (Align::Center, _) => ax - w / 2,
            (Align::Right, _) => ax - w,
        } + o.dx as i32;
        let y = y0 + i as i32 * line_h + o.dy as i32;
        if shadow {
            draw_text(c, x + 2, y + 3, &txt, px2, f, [0, 0, 0], alpha * 0.5);
        }
        draw_text(c, x, y, &txt, px2, f, col, alpha);
    }
}

// ------------------------------------------------------------------------------- render

/// One frame of a designed screen, `t` seconds in. `dur` is how long the screen lasts (0 = it
/// does not end), used for the fade-out at the end.
pub fn render(d: &Design, v: &Vars, images: &Images, th: &Theme, t: f32, dur: f32) -> Canvas {
    let mut c = Canvas::new(W as usize, H as usize);
    draw_backdrop(&mut c, d, th, images, t);
    let end_fade = if d.fade_out_ms > 0 && dur > 0.0 {
        ((dur - t) / (d.fade_out_ms as f32 / 1000.0)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    for l in &d.layers {
        draw_layer(&mut c, l, v, th, images, t, end_fade);
    }
    if end_fade < 1.0 {
        // fade the whole frame to the backdrop's darkest colour at the very end
        c.brightness(0.35 + 0.65 * end_fade);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn th() -> Theme {
        Theme::from_hue(0.6)
    }

    fn vars() -> Vars {
        Vars {
            channel: "Treehouse".into(),
            number: 6,
            show: "Blues Clues".into(),
            title: "Snack Time".into(),
            time: "1:30 PM".into(),
            message: "PLEASE STAND BY".into(),
            items: vec![
                ("1:30 PM".into(), "Blues Clues".into()),
                ("2:00 PM".into(), "Franklin".into()),
            ],
            clock0_secs: 13 * 3600 + 59 * 60 + 58,
            date: "SAT NOV 21".into(),
        }
    }

    fn sample() -> Design {
        serde_json::from_str(
            r##"{"name":"t","backdrop":{"type":"aurora","colors":["primary","#ff8800"]},
            "layers":[
              {"type":"shape","w":700,"h":300,"radius":40,"glass":true,"x":500,"y":500,"anim":{"enter":"zoom"}},
              {"type":"text","text":"{channel} {number}","size":72,"x":500,"y":440,"anim":{"enter":"slide_up","delay_ms":200}},
              {"type":"text","text":"{message}","size":36,"bold":false,"x":500,"y":560,"anim":{"enter":"type","motion":"blink"}},
              {"type":"clock","x":900,"y":100,"seconds":true,"date":true},
              {"type":"list","x":500,"y":800,"rows":3}
            ],"fade_out_ms":500}"##,
        )
        .unwrap()
    }

    #[test]
    fn a_design_survives_json_and_fills_in_its_words() {
        let d = sample();
        let back: Design = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(d, back);
        assert_eq!(
            fill("{channel} {number} · {time}", &vars(), 0.0, true, false),
            "Treehouse 6 · 1:30 PM"
        );
        assert_eq!(d.problem(), None);
    }

    #[test]
    fn the_clock_ticks_with_the_screen() {
        let v = vars();
        assert_eq!(fill("{clock}", &v, 0.0, true, true), "1:59:58 PM");
        assert_eq!(fill("{clock}", &v, 3.0, true, true), "2:00:01 PM");
        assert_eq!(fill("{clock}", &v, 3.0, false, false), "14:00");
    }

    #[test]
    fn frames_are_full_size_and_move() {
        let (d, v) = (sample(), vars());
        let a = render(&d, &v, &Images::new(), &th(), 0.0, 10.0);
        let b = render(&d, &v, &Images::new(), &th(), 2.0, 10.0);
        let c = render(&d, &v, &Images::new(), &th(), 2.0, 10.0);
        assert_eq!((a.w, a.h), (1280, 720));
        assert_ne!(a.px, b.px, "it animates");
        assert_eq!(b.px, c.px, "the same moment draws the same");
    }

    #[test]
    fn layers_arrive_when_asked() {
        let d: Design = serde_json::from_str(
            r##"{"backdrop":{"type":"solid","color":"#000000"},"layers":[
              {"type":"shape","w":400,"h":200,"color":"#ffffff","x":500,"y":500,"anim":{"enter":"fade","delay_ms":1000,"enter_ms":500}}]}"##,
        )
        .unwrap();
        let lit = |t: f32| {
            render(&d, &Vars::default(), &Images::new(), &th(), t, 0.0)
                .px
                .iter()
                .any(|b| *b > 0)
        };
        assert!(!lit(0.5), "not there before its delay");
        assert!(lit(2.0), "there afterwards");
    }

    #[test]
    fn it_fades_out_at_the_end_when_asked() {
        let d = sample();
        let sum = |t: f32| {
            render(&d, &vars(), &Images::new(), &th(), t, 10.0)
                .px
                .iter()
                .map(|b| *b as u64)
                .sum::<u64>()
        };
        assert!(sum(9.9) < sum(5.0));
    }

    #[test]
    fn colours_come_from_the_theme_or_hex() {
        let t = th();
        assert_eq!(colour("accent", &t), t.accent);
        assert_eq!(colour("#ff0000", &t), [255, 0, 0]);
        assert_eq!(colour("#f00", &t), [255, 0, 0]);
        assert_eq!(colour("nonsense", &t), t.ink);
    }

    #[test]
    fn nonsense_is_refused_in_words() {
        let mut d = sample();
        d.layers[1].anim.speed = 5;
        assert!(d.problem().unwrap().contains("speed"));
        let mut d = sample();
        d.layers[0].opacity = 400;
        assert!(d.problem().unwrap().contains("opacity"));
    }

    #[test]
    fn every_kind_of_backdrop_and_motion_draws() {
        let t = th();
        let backs = [
            r##"{"type":"theme"}"##,
            r##"{"type":"solid","color":"#123456"}"##,
            r##"{"type":"gradient","top":"primary","bottom":"dark"}"##,
            r##"{"type":"aurora","colors":[]}"##,
            r##"{"type":"bars"}"##,
            r##"{"type":"pattern","kind":"dots","color":"accent","bg":"dark"}"##,
            r##"{"type":"pattern","kind":"grid","color":"accent","bg":"dark"}"##,
            r##"{"type":"pattern","kind":"stripes","color":"accent","bg":"dark","size":30}"##,
            r##"{"type":"picture","path":"missing.png"}"##,
        ];
        for b in backs {
            for motion in ["none", "pulse", "float", "drift", "blink", "marquee"] {
                let j = format!(
                    r##"{{"backdrop":{b},"layers":[{{"type":"text","text":"Hi {{clock}}","pixel":true,"anim":{{"motion":"{motion}"}}}},{{"type":"text","text":"Hello","anim":{{"motion":"{motion}","enter":"wipe"}}}}]}}"##
                );
                let d: Design = serde_json::from_str(&j).unwrap();
                let f = render(&d, &vars(), &Images::new(), &t, 1.3, 0.0);
                assert_eq!(f.px.len(), 1280 * 720 * 3, "{b} {motion}");
            }
        }
    }

    #[test]
    fn a_frame_is_quick_enough_to_air() {
        let (d, v) = (sample(), vars());
        let start = std::time::Instant::now();
        for i in 0..6 {
            render(&d, &v, &Images::new(), &th(), i as f32 * 0.2, 0.0);
        }
        // a debug build on a slow machine: generous, but it would catch an accidental O(n^2)
        assert!(
            start.elapsed().as_millis() < 6 * 900,
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn pictures_are_listed_for_loading() {
        let d: Design = serde_json::from_str(
            r##"{"backdrop":{"type":"picture","path":"/a.png"},"layers":[{"type":"image","path":"/b.png","w":100,"h":100},{"type":"image","path":"/a.png","w":100,"h":100}]}"##,
        )
        .unwrap();
        assert_eq!(
            d.pictures(),
            vec!["/a.png".to_string(), "/b.png".to_string()]
        );
    }
}
