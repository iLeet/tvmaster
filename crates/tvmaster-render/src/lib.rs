//! Station graphics: bumpers drawn frame by frame as raw RGB pixels.
//!
//! The look is deliberately retro: a chunky 8x8 pixel font, scrolling stripes
//! and scanlines, drawn at 640x360 and scaled up by FFmpeg without smoothing.
//! Everything here is original artwork and needs no font files, so the same
//! output appears on every platform. Colours come from a per-channel theme so
//! each channel looks like its own station.

pub mod alert;
pub mod guide;
pub mod modern;
pub mod overlay;
pub mod png;
pub mod text;
pub mod ticker;

pub use guide::{
    geometry, render_guide, Background, Geometry, GuideCell, GuideData, GuideLook, GuideRow,
    GuideView, Palette, Placement, Rect, WindowLook, WindowSize,
};

use font8x8::{UnicodeFonts, BASIC_FONTS, LATIN_FONTS};

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 360;
pub const FPS: u32 = 30;

type Rgb = [u8; 3];

/// A software canvas of RGB24 pixels.
#[derive(Clone)]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h * 3],
        }
    }

    pub fn fill(&mut self, c: Rgb) {
        for p in self.px.as_chunks_mut::<3>().0 {
            *p = c;
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        let (x0, x1) = (x.max(0) as usize, ((x + w).max(0) as usize).min(self.w));
        let (y0, y1) = (y.max(0) as usize, ((y + h).max(0) as usize).min(self.h));
        for yy in y0..y1 {
            for xx in x0..x1 {
                let i = (yy * self.w + xx) * 3;
                self.px[i..i + 3].copy_from_slice(&c);
            }
        }
    }

    /// Top-to-bottom gradient over the whole canvas.
    pub fn vgradient(&mut self, top: Rgb, bottom: Rgb) {
        for y in 0..self.h {
            let f = y as f32 / (self.h - 1).max(1) as f32;
            let c = mix(top, bottom, f);
            for x in 0..self.w {
                let i = (y * self.w + x) * 3;
                self.px[i..i + 3].copy_from_slice(&c);
            }
        }
    }

    fn glyph(c: char) -> [u8; 8] {
        BASIC_FONTS
            .get(c)
            .or_else(|| LATIN_FONTS.get(c))
            .or_else(|| BASIC_FONTS.get('?'))
            .unwrap_or([0; 8])
    }

    /// Draw `text` (upper-cased) with its top-left at (x, y). Each font pixel
    /// becomes a `scale` x `scale` block.
    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: i32, c: Rgb) {
        let mut cx = x;
        for ch in text.to_uppercase().chars() {
            let g = Self::glyph(ch);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..8 {
                    if bits >> col & 1 == 1 {
                        self.rect(cx + col * scale, y + row as i32 * scale, scale, scale, c);
                    }
                }
            }
            cx += 8 * scale;
        }
    }

    /// A one-pixel-offset shadow makes the chunky text readable on any background.
    pub fn text_shadow(&mut self, x: i32, y: i32, text: &str, scale: i32, c: Rgb) {
        self.text(x + scale, y + scale, text, scale, [0, 0, 0]);
        self.text(x, y, text, scale, c);
    }

    pub fn text_centered(&mut self, y: i32, text: &str, scale: i32, c: Rgb) {
        let x = (self.w as i32 - text_width(text, scale)) / 2;
        self.text_shadow(x, y, text, scale, c);
    }

    /// Copy `src` onto this canvas with its top-left corner at (x, y), clipped.
    pub fn blit(&mut self, src: &Canvas, x: i32, y: i32) {
        for sy in 0..src.h as i32 {
            let dy = y + sy;
            if dy < 0 || dy >= self.h as i32 {
                continue;
            }
            for sx in 0..src.w as i32 {
                let dx = x + sx;
                if dx < 0 || dx >= self.w as i32 {
                    continue;
                }
                let (si, di) = (
                    (sy as usize * src.w + sx as usize) * 3,
                    (dy as usize * self.w + dx as usize) * 3,
                );
                self.px[di..di + 3].copy_from_slice(&src.px[si..si + 3]);
            }
        }
    }

    /// Multiply every pixel's brightness by `f` (0 = black, 1 = unchanged).
    pub fn brightness(&mut self, f: f32) {
        if f >= 0.999 {
            return;
        }
        let f = f.clamp(0.0, 1.0);
        for v in self.px.iter_mut() {
            *v = (*v as f32 * f) as u8;
        }
    }

    /// Darken every other row, like a CRT.
    pub fn scanlines(&mut self, strength: f32) {
        let k = 1.0 - strength;
        for y in (1..self.h).step_by(2) {
            for v in &mut self.px[y * self.w * 3..(y + 1) * self.w * 3] {
                *v = (*v as f32 * k) as u8;
            }
        }
    }

    /// A binary PPM (P6), handy for previews: `ffmpeg -i frame.ppm frame.png`.
    pub fn to_ppm(&self) -> Vec<u8> {
        let mut out = format!("P6\n{} {}\n255\n", self.w, self.h).into_bytes();
        out.extend_from_slice(&self.px);
        out
    }
}

fn mix(a: Rgb, b: Rgb, f: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

/// The pixel-font alert screen, in the colour of its severity.
pub(crate) fn retro_alert(severity: alert::Severity, event: &str, text: &str, t: f32) -> Canvas {
    let mut c = Canvas::new(WIDTH, HEIGHT);
    alert_into(&mut c, event, text, t, severity);
    c
}

pub fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * 8 * scale
}

fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let (i, f) = (h.floor() as i32, h - h.floor());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

/// The colours and style of the station's graphics (cards, bumpers, screens).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub dark: Rgb,
    pub primary: Rgb,
    pub accent: Rgb,
    /// The bottom of the background gradient.
    pub bottom: Rgb,
    /// Main text colour.
    pub ink: Rgb,
    /// Text on an accent-coloured band.
    pub on_accent: Rgb,
    /// Smooth type at full resolution instead of the pixel font.
    pub modern: bool,
}

impl Theme {
    pub fn from_hue(h: f32) -> Self {
        Self {
            dark: hsv(h, 0.65, 0.22),
            primary: hsv(h, 0.75, 0.95),
            accent: hsv(h + 0.13, 0.85, 1.0),
            bottom: [4, 4, 8],
            ink: WHITE,
            on_accent: [10, 10, 20],
            modern: false,
        }
    }

    /// A stable theme derived from the channel id, so each channel keeps its look.
    pub fn for_channel(id: &str) -> Self {
        let mut h: u32 = 2166136261;
        for b in id.bytes() {
            h = (h ^ b as u32).wrapping_mul(16777619);
        }
        Self::from_hue((h % 360) as f32 / 360.0)
    }

    /// The graphics' look taken from a guide palette, so one theme dresses everything.
    pub fn from_palette(p: &Palette, modern: bool) -> Self {
        Self {
            dark: p.bg_top,
            primary: p.tick,
            accent: p.accent,
            bottom: p.bg_bottom,
            ink: p.text_now,
            on_accent: alert::ink_on(p.accent),
            modern,
        }
    }

    /// The size frames are drawn at.
    pub fn frame_size(&self) -> (usize, usize) {
        if self.modern {
            (WIDTH * 2, HEIGHT * 2)
        } else {
            (WIDTH, HEIGHT)
        }
    }
}

/// What kind of spot a promo is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromoKind {
    /// A sponsor or advertiser: a big name, a tagline and a contact line.
    Sponsor,
    /// A promo for a programme: "Coming up", the name, and when.
    ComingUp,
    /// A few lines of text, one after another.
    Slides,
}

/// What a made-by-you spot says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromoSpec {
    pub kind: PromoKind,
    pub title: String,
    pub tagline: String,
    pub contact: String,
    pub slides: Vec<String>,
}

/// A picture behind a promo, already fitted to a little more than the frame so it can drift slowly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Photo {
    pub w: usize,
    pub h: usize,
    pub rgb: std::sync::Arc<Vec<u8>>,
}

/// What a bumper says. Text is filled in by the caller when the bumper airs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scene {
    StationId {
        name: String,
    },
    /// "We'll be right back", at the start of a break.
    Brb {
        name: String,
        show: String,
    },
    /// "Back to <show>", at the end of a mid-show break.
    BackTo {
        name: String,
        show: String,
    },
    UpNext {
        name: String,
        title: String,
        time: String,
    },
    /// The next few programmes: (time, title).
    LaterToday {
        name: String,
        evening: bool,
        items: Vec<(String, String)>,
    },
    /// A spot made in the app: a sponsor, a programme promo or a few slides of text.
    Promo {
        spec: PromoSpec,
        photo: Option<Photo>,
    },
    /// An emergency-alert screen that takes over the channel: a flashing red
    /// header, the event, the message, and the message again as a crawl.
    Alert {
        event: String,
        text: String,
    },
}

const WHITE: Rgb = [255, 255, 255];

/// Split `text` into lines of at most `chars` characters at word boundaries
/// (long words are cut). Returns `None` if more than `max_lines` are needed.
fn wrap(text: &str, chars: usize, max_lines: usize) -> Option<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while word.chars().count() > chars {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            lines.push(word.chars().take(chars).collect());
            word = word.chars().skip(chars).collect();
        }
        let need = cur.chars().count() + usize::from(!cur.is_empty()) + word.chars().count();
        if need > chars && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    (lines.len() <= max_lines).then_some(lines)
}

/// Choose the biggest pixel scale at which `text` fits in `max_w` pixels over
/// at most `max_lines` lines; at the smallest scale, truncate with "...".
pub fn layout_title(text: &str, max_w: i32, max_lines: usize) -> (i32, Vec<String>) {
    let text = if text.trim().is_empty() {
        "TV"
    } else {
        text.trim()
    };
    for scale in [6, 5, 4, 3, 2] {
        let chars = (max_w / (8 * scale)).max(1) as usize;
        if let Some(lines) = wrap(text, chars, max_lines) {
            return (scale, lines);
        }
    }
    let chars = (max_w / 16).max(4) as usize;
    let mut lines = wrap(text, chars, usize::MAX).unwrap_or_default();
    lines.truncate(max_lines);
    if let Some(last) = lines.last_mut() {
        let keep = chars.saturating_sub(3);
        *last = format!(
            "{}...",
            last.chars().take(keep).collect::<String>().trim_end()
        );
    }
    (2, lines)
}

fn truncate(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        text.to_string()
    } else {
        format!(
            "{}...",
            text.chars()
                .take(chars.saturating_sub(3))
                .collect::<String>()
                .trim_end()
        )
    }
}

fn ease_out(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p) * (1.0 - p) * (1.0 - p)
}

/// Horizontal slide-in offset in pixels for an element that starts at `delay`.
fn slide(t: f32, delay: f32) -> i32 {
    ((1.0 - ease_out((t - delay) / 0.45)) * 90.0) as i32
}

fn background(c: &mut Canvas, th: &Theme, t: f32) {
    c.vgradient(th.dark, th.bottom);
    // Scrolling stripe band at the bottom.
    let off = (t * 48.0) as i32;
    for y in 318..344 {
        for x in 0..WIDTH as i32 {
            if ((x + (y - 318) * 2 + off) / 14) % 2 == 0 {
                c.rect(x, y, 1, 1, th.primary);
            } else {
                c.rect(x, y, 1, 1, th.accent);
            }
        }
    }
    c.rect(0, 316, WIDTH as i32, 2, th.ink);
    c.rect(0, 344, WIDTH as i32, 2, th.ink);
}

/// The emergency-alert screen. Steady (no fades), because it stays up for as long as the alert does.
fn alert_into(c: &mut Canvas, event: &str, text: &str, t: f32, severity: alert::Severity) {
    let red: Rgb = severity.accent();
    let dark_red: Rgb = mix(red, [0, 0, 0], 0.45);
    let label = match severity {
        alert::Severity::Emergency => "EMERGENCY ALERT SYSTEM",
        alert::Severity::Warning => "ALERT",
        alert::Severity::Info => "INFORMATION",
    };
    const YELLOW: Rgb = [255, 214, 0];
    c.vgradient([26, 0, 4], [6, 0, 2]);
    // Flashing header band.
    let flash = (t * 2.0) as i64 % 2 == 0;
    c.rect(0, 0, WIDTH as i32, 56, if flash { red } else { dark_red });
    c.rect(0, 56, WIDTH as i32, 3, WHITE);
    c.text_centered(18, label, 3, WHITE);
    // The event, as large as fits.
    let (scale, ls) = layout_title(event, 600, 2);
    let mut y = 74;
    for l in &ls {
        c.text_centered(y, l, scale.min(5), YELLOW);
        y += scale.min(5) * 10;
    }
    // The message; the crawl below carries all of it when it does not fit.
    let room = (((312 - y - 6) / 20).max(1)) as usize;
    let mut body = match wrap(text, 37, room) {
        Some(b) => b,
        None => {
            let mut b = wrap(text, 37, usize::MAX).unwrap_or_default();
            b.truncate(room);
            if let Some(last) = b.last_mut() {
                *last = truncate(&format!("{last}..."), 37);
            }
            b
        }
    };
    body.retain(|l| !l.is_empty());
    let mut y = y + 6;
    for l in &body {
        c.text_centered(y, l, 2, WHITE);
        y += 20;
    }
    // The crawl.
    c.rect(0, 322, WIDTH as i32, 38, red);
    c.rect(0, 320, WIDTH as i32, 2, WHITE);
    let crawl = format!("{}      ***      ", text.replace('\n', " "));
    let w = text_width(&crawl, 2).max(1);
    let x = -(((t * 110.0) as i32) % w);
    for k in 0..=(WIDTH as i32 / w + 1) {
        c.text(x + k * w, 331, &crawl, 2, WHITE);
    }
}

fn header(c: &mut Canvas, th: &Theme, name: &str, t: f32) {
    let x = 24 - slide(t, 0.0);
    c.rect(x - 8, 18, 6, 22, th.accent);
    c.text_shadow(x + 4, 20, &truncate(name, 34), 2, th.ink);
}

fn lines(
    c: &mut Canvas,
    y: i32,
    lines: &[String],
    scale: i32,
    col: Rgb,
    t: f32,
    delay: f32,
) -> i32 {
    let mut y = y;
    for (i, l) in lines.iter().enumerate() {
        let off = slide(t, delay + i as f32 * 0.08);
        let x = (c.w as i32 - text_width(l, scale)) / 2 - off;
        c.text_shadow(x, y, l, scale, col);
        y += scale * 8 + scale * 2;
    }
    y
}

/// Copy the part of `photo` that is showing at `t` (it drifts across over `dur` seconds), darkened by `keep`.
pub(crate) fn draw_photo(c: &mut Canvas, photo: &Photo, t: f32, dur: f32, keep: f32) {
    let (w, h) = (c.w, c.h);
    if photo.w < w || photo.h < h || photo.rgb.len() < photo.w * photo.h * 3 {
        return;
    }
    let f = (t / dur.max(0.1)).clamp(0.0, 1.0);
    let (x0, y0) = (
        ((photo.w - w) as f32 * f) as usize,
        ((photo.h - h) as f32 * (1.0 - f)) as usize,
    );
    for y in 0..h {
        let src = ((y0 + y) * photo.w + x0) * 3;
        let dst = y * w * 3;
        for (d, s) in c.px[dst..dst + w * 3]
            .iter_mut()
            .zip(&photo.rgb[src..src + w * 3])
        {
            *d = (*s as f32 * keep) as u8;
        }
    }
}

fn promo_retro(
    c: &mut Canvas,
    th: &Theme,
    spec: &PromoSpec,
    photo: Option<&Photo>,
    t: f32,
    dur: f32,
) {
    if let Some(p) = photo {
        draw_photo(c, p, t, dur, 0.55);
    }
    match spec.kind {
        PromoKind::Sponsor => {
            let (scale, ls) = layout_title(&spec.title, 580, 2);
            let end = lines(c, 70, &ls, scale.min(6), th.ink, t, 0.1);
            if !spec.tagline.trim().is_empty() {
                let (sc, tl) = layout_title(&spec.tagline, 560, 2);
                lines(c, end + 12, &tl, sc.min(3), th.accent, t, 0.6);
            }
            if !spec.contact.trim().is_empty() {
                let rise = ((1.0 - ease_out((t - 1.3) / 0.5)) * 50.0) as i32;
                c.rect(0, 306 + rise, 640, 40, th.accent);
                let (sc, cl) = layout_title(&spec.contact, 600, 1);
                if let Some(l) = cl.first() {
                    c.text_centered(318 + rise, l, sc.min(2), th.on_accent);
                }
            }
        }
        PromoKind::ComingUp => {
            let bar = 60 - slide(t, 0.0) / 2;
            c.rect(0, bar, 640, 40, th.accent);
            c.text(24 - slide(t, 0.05), bar + 8, "COMING UP", 3, th.on_accent);
            let (scale, ls) = layout_title(&spec.title, 580, 3);
            let end = lines(c, 130, &ls, scale.min(5), th.ink, t, 0.25);
            if !spec.tagline.trim().is_empty() {
                let (sc, tl) = layout_title(&spec.tagline, 560, 2);
                lines(c, end + 14, &tl, sc.min(3), th.primary, t, 0.6);
            }
            if !spec.contact.trim().is_empty() {
                c.text_shadow(24, 332, &truncate(&spec.contact, 60), 2, th.ink);
            }
        }
        PromoKind::Slides => {
            if !spec.title.trim().is_empty() {
                header(c, th, &spec.title, t);
            }
            let n = spec.slides.len().max(1);
            let each = dur / n as f32;
            let i = ((t / each) as usize).min(n - 1);
            let local = t - i as f32 * each;
            if let Some(text) = spec.slides.get(i) {
                let (scale, ls) = layout_title(text, 580, 4);
                let y = 180 - (ls.len() as i32 - 1) * scale * 5;
                lines(c, y, &ls, scale.min(5), th.ink, local, 0.05);
            }
            for k in 0..n {
                c.rect(
                    300 + k as i32 * 16 - n as i32 * 8,
                    340,
                    8,
                    8,
                    if k == i { th.accent } else { th.primary },
                );
            }
        }
    }
}

/// Render one frame of a bumper `dur` seconds long, `t` seconds in.
pub fn render(scene: &Scene, th: &Theme, t: f32, dur: f32) -> Canvas {
    if th.modern && !matches!(scene, Scene::Alert { .. }) {
        return modern::render(scene, th, t, dur);
    }
    let mut c = Canvas::new(WIDTH, HEIGHT);
    if let Scene::Alert { event, text } = scene {
        alert_into(&mut c, event, text, t, alert::Severity::Emergency);
        return c;
    }
    background(&mut c, th, t);
    match scene {
        Scene::StationId { name } => {
            c.text_centered(96, "YOU'RE WATCHING", 2, th.accent);
            let (scale, ls) = layout_title(name, 580, 2);
            let y = 130 - (ls.len() as i32 - 1) * scale * 5;
            lines(&mut c, y, &ls, scale, th.ink, t, 0.1);
            // Sweeping highlight under the name.
            let x = ((t * 260.0) as i32 % 900) - 150;
            c.rect(x, 262, 120, 4, th.accent);
            c.rect(60, 266, 520, 2, th.primary);
        }
        Scene::Brb { name, show } => {
            header(&mut c, th, name, t);
            lines(
                &mut c,
                92,
                &["WE'LL BE".into(), "RIGHT BACK".into()],
                5,
                th.ink,
                t,
                0.05,
            );
            let (scale, ls) = layout_title(&format!("AFTER THIS: {show}"), 560, 2);
            lines(&mut c, 240, &ls, scale.min(2), th.accent, t, 0.5);
        }
        Scene::BackTo { name, show } => {
            header(&mut c, th, name, t);
            lines(
                &mut c,
                80,
                &["WELCOME BACK TO".into()],
                3,
                th.accent,
                t,
                0.05,
            );
            let (scale, ls) = layout_title(show, 580, 3);
            let y = 150;
            lines(&mut c, y, &ls, scale.min(5), th.ink, t, 0.2);
        }
        Scene::UpNext { name, title, time } => {
            header(&mut c, th, name, t);
            let bar = 70 - slide(t, 0.0) / 2;
            c.rect(0, bar, 640, 40, th.accent);
            c.text(24 - slide(t, 0.05), bar + 8, "UP NEXT", 3, th.on_accent);
            let (scale, ls) = layout_title(title, 580, 2);
            let end = lines(&mut c, 134, &ls, scale.min(5), th.ink, t, 0.25);
            if !time.is_empty() {
                let txt = format!("AT {time}");
                lines(&mut c, end + 14, &[txt], 3, th.primary, t, 0.5);
            }
        }
        Scene::LaterToday {
            name,
            evening,
            items,
        } => {
            header(&mut c, th, name, t);
            let title = if *evening {
                "COMING UP TONIGHT"
            } else {
                "LATER TODAY"
            };
            let bar = 62 - slide(t, 0.0) / 2;
            c.rect(0, bar, 640, 40, th.accent);
            c.text(24 - slide(t, 0.05), bar + 8, title, 3, th.on_accent);
            let mut y = 124;
            for (i, (time, prog)) in items.iter().take(4).enumerate() {
                let off = slide(t, 0.2 + i as f32 * 0.12);
                c.text_shadow(32 - off, y, time, 2, th.primary);
                c.text_shadow(32 + 10 * 16 - off, y, &truncate(prog, 26), 2, th.ink);
                y += 44;
            }
        }
        Scene::Promo { spec, photo } => promo_retro(&mut c, th, spec, photo.as_ref(), t, dur),
        Scene::Alert { .. } => {}
    }
    c.scanlines(0.16);
    // Fade in from black and out to black.
    let f = (t / 0.3).min(1.0).min(((dur - t) / 0.4).max(0.0));
    c.brightness(f);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alert_screen_flashes_and_crawls_without_fading() {
        let sc = Scene::Alert {
            event: "Tornado warning".into(),
            text: "SAMPLE ONLY. Take shelter now. This is only a test.".into(),
        };
        let th = Theme::from_hue(0.5);
        let a = render(&sc, &th, 0.1, 5.0);
        let b = render(&sc, &th, 0.6, 5.0);
        let c = render(&sc, &th, 4.9, 5.0);
        assert_ne!(a.px, b.px, "the header flashes and the crawl moves");
        assert!(
            c.px.iter().map(|v| *v as u32).sum::<u32>() > 1_000_000,
            "no fade to black at the end"
        );
        assert_eq!(a.px.len(), WIDTH * HEIGHT * 3);
    }

    fn scenes() -> Vec<Scene> {
        let name = "Retro TV".to_string();
        vec![
            Scene::StationId { name: name.clone() },
            Scene::Brb {
                name: name.clone(),
                show: "Cheers".into(),
            },
            Scene::BackTo {
                name: name.clone(),
                show: "Cheers".into(),
            },
            Scene::UpNext {
                name: name.clone(),
                title: "Taxi".into(),
                time: "8:30 PM".into(),
            },
            Scene::LaterToday {
                name,
                evening: false,
                items: vec![
                    ("8:30 PM".into(), "Taxi".into()),
                    ("9:00 PM".into(), "Cheers".into()),
                ],
            },
        ]
    }

    fn mean(c: &Canvas) -> f64 {
        c.px.iter().map(|v| *v as f64).sum::<f64>() / c.px.len() as f64
    }

    #[test]
    fn frames_have_the_right_size_and_are_not_blank() {
        let th = Theme::for_channel("retro");
        for s in scenes() {
            let c = render(&s, &th, 1.5, 4.0);
            assert_eq!((c.w, c.h, c.px.len()), (WIDTH, HEIGHT, WIDTH * HEIGHT * 3));
            let distinct: std::collections::HashSet<&[u8]> = c.px.chunks(3).step_by(37).collect();
            assert!(distinct.len() > 8, "{s:?} looks blank");
        }
    }

    #[test]
    fn animation_moves_and_fades_in_and_out() {
        let th = Theme::for_channel("retro");
        let s = &scenes()[3];
        assert_ne!(render(s, &th, 1.0, 5.0).px, render(s, &th, 2.0, 5.0).px);
        // deterministic
        assert_eq!(render(s, &th, 2.0, 5.0).px, render(s, &th, 2.0, 5.0).px);
        let (first, mid, last) = (
            mean(&render(s, &th, 0.0, 5.0)),
            mean(&render(s, &th, 2.5, 5.0)),
            mean(&render(s, &th, 4.95, 5.0)),
        );
        assert!(first < mid * 0.2, "fades in from black: {first} vs {mid}");
        assert!(last < mid * 0.2, "fades out to black: {last} vs {mid}");
    }

    #[test]
    fn long_and_odd_text_never_panics_or_overflows() {
        let th = Theme::for_channel("x");
        let long = "THE INCREDIBLY LONG ADVENTURES OF A SHOW WITH AN ABSURDLY VERBOSE NAME THAT KEEPS GOING AND GOING";
        for s in [
            Scene::UpNext {
                name: long.into(),
                title: long.into(),
                time: long.into(),
            },
            Scene::BackTo {
                name: "N".into(),
                show: "Émile et les détectives 😀 日本語".into(),
            },
            Scene::LaterToday {
                name: "N".into(),
                evening: true,
                items: (0..9)
                    .map(|i| (format!("{i}:00"), long.to_string()))
                    .collect(),
            },
            Scene::StationId {
                name: String::new(),
            },
        ] {
            for t in [0.0, 1.0, 2.9, 3.0] {
                let c = render(&s, &th, t, 3.0);
                assert_eq!(c.px.len(), WIDTH * HEIGHT * 3);
            }
        }
    }

    #[test]
    fn every_kind_of_promo_draws_in_both_styles_with_and_without_a_picture() {
        let long = "AN ABSURDLY LONG HEADLINE THAT KEEPS GOING AND GOING UNTIL IT MUST BE FITTED";
        let specs = [
            PromoSpec {
                kind: PromoKind::Sponsor,
                title: "Mel's Diner".into(),
                tagline: "Best pie in town".into(),
                contact: "Call 555-0134 - open daily".into(),
                slides: vec![],
            },
            PromoSpec {
                kind: PromoKind::Sponsor,
                title: long.into(),
                tagline: long.into(),
                contact: long.into(),
                slides: vec![],
            },
            PromoSpec {
                kind: PromoKind::ComingUp,
                title: "Cheers".into(),
                tagline: "Thursdays at 8".into(),
                contact: String::new(),
                slides: vec![],
            },
            PromoSpec {
                kind: PromoKind::Slides,
                title: "Today".into(),
                tagline: String::new(),
                contact: String::new(),
                slides: vec!["Pie".into(), "Coffee".into(), long.into()],
            },
            PromoSpec {
                kind: PromoKind::Slides,
                title: String::new(),
                tagline: String::new(),
                contact: String::new(),
                slides: vec![],
            },
        ];
        for modern in [false, true] {
            let mut th = Theme::for_channel("promo");
            th.modern = modern;
            let (w, h) = th.frame_size();
            let photo = Photo {
                w: w * 115 / 100,
                h: h * 115 / 100,
                rgb: std::sync::Arc::new(vec![180u8; (w * 115 / 100) * (h * 115 / 100) * 3]),
            };
            for spec in &specs {
                for p in [None, Some(photo.clone())] {
                    let sc = Scene::Promo {
                        spec: spec.clone(),
                        photo: p,
                    };
                    for t in [0.0, 1.0, 4.9] {
                        let c = render(&sc, &th, t, 5.0);
                        assert_eq!((c.w, c.h), (w, h));
                    }
                    if matches!(&sc, Scene::Promo { photo: None, .. })
                        && !(spec.title.is_empty() && spec.slides.is_empty())
                    {
                        let mid = render(&sc, &th, 2.5, 5.0);
                        let distinct: std::collections::HashSet<&[u8]> =
                            mid.px.chunks(3).step_by(37).collect();
                        assert!(distinct.len() > 4, "{spec:?} looks blank");
                    }
                }
            }
        }
        // The picture drifts, the text moves in, and the same frame is always the same.
        let th = Theme::for_channel("p");
        let spec = specs[0].clone();
        let photo = Photo {
            w: 736,
            h: 414,
            rgb: std::sync::Arc::new((0..736 * 414 * 3).map(|i| (i % 251) as u8).collect()),
        };
        let sc = Scene::Promo {
            spec,
            photo: Some(photo),
        };
        assert_ne!(render(&sc, &th, 1.0, 5.0).px, render(&sc, &th, 3.0, 5.0).px);
        assert_eq!(render(&sc, &th, 2.0, 5.0).px, render(&sc, &th, 2.0, 5.0).px);
    }

    #[test]
    fn titles_fit_the_width_by_scaling_wrapping_or_truncating() {
        let (scale, ls) = layout_title("Cheers", 580, 3);
        assert_eq!((scale, ls.len()), (6, 1));
        let (scale, ls) = layout_title("The Adventures of Brisco County Jr", 580, 3);
        assert!(scale <= 5 && ls.len() <= 3);
        for l in &ls {
            assert!(text_width(l, scale) <= 580, "{l:?} at {scale}");
        }
        let (scale, ls) = layout_title(&"WORD ".repeat(60), 580, 2);
        assert_eq!(scale, 2);
        assert!(ls.len() <= 2 && ls.last().unwrap().ends_with("..."));
        assert!(text_width(ls.last().unwrap(), scale) <= 580);
        assert_eq!(layout_title("", 580, 2).1, vec!["TV".to_string()]);
    }

    #[test]
    fn text_draws_pixels_and_unknown_glyphs_fall_back() {
        let mut c = Canvas::new(100, 20);
        c.text(0, 0, "A", 1, [255, 255, 255]);
        assert!(c.px.contains(&255));
        let mut d = Canvas::new(100, 20);
        d.text(0, 0, "😀", 1, [255, 255, 255]);
        let mut q = Canvas::new(100, 20);
        q.text(0, 0, "?", 1, [255, 255, 255]);
        assert_eq!(d.px, q.px, "unknown characters draw as '?'");
        // drawing off-canvas is clipped, not a panic
        c.text(-50, -50, "CLIPPED", 3, [1, 2, 3]);
        c.text(95, 15, "EDGE", 3, [1, 2, 3]);
    }

    #[test]
    fn blit_clips_to_the_canvas() {
        let mut dst = Canvas::new(4, 4);
        let mut src = Canvas::new(3, 3);
        src.fill([9, 9, 9]);
        dst.blit(&src, 2, 2);
        dst.blit(&src, -2, -2);
        dst.blit(&src, 100, 100);
        assert_eq!(&dst.px[(3 * 4 + 3) * 3..(3 * 4 + 3) * 3 + 3], &[9, 9, 9]);
        assert_eq!(&dst.px[0..3], &[9, 9, 9]);
    }

    #[test]
    fn themes_are_stable_per_channel_and_differ_between_channels() {
        assert_eq!(Theme::for_channel("retro"), Theme::for_channel("retro"));
        assert_ne!(Theme::for_channel("retro"), Theme::for_channel("toons"));
    }

    #[test]
    fn ppm_has_a_valid_header_and_payload() {
        let c = Canvas::new(4, 2);
        let ppm = c.to_ppm();
        assert!(ppm.starts_with(b"P6\n4 2\n255\n"));
        assert_eq!(ppm.len(), "P6\n4 2\n255\n".len() + 4 * 2 * 3);
    }
}
