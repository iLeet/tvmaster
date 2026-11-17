//! Emergency and information alerts: a full-screen takeover, or a bar across the
//! bottom of the programme with a crawl, in a retro pixel look or in smooth modern
//! themes. Colour says how serious it is (red emergency, amber warning, blue
//! information); the theme says what it looks like.

use super::{mix, Canvas, Rgb, HEIGHT, WIDTH};
use crate::guide::Palette;
use crate::png;
use crate::text::{self, Face};

const WHITE: Rgb = [255, 255, 255];
const KEY: Rgb = [255, 0, 255];

/// How an alert is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertTheme {
    /// The chunky pixel font on a dark screen, like an old broadcast alert.
    #[default]
    Retro,
    /// Clean smooth type on charcoal.
    Modern,
    Midnight,
    Graphite,
    Snow,
    Forest,
    Berry,
    Mono,
    /// Smooth, in the station theme's own colours.
    Station(Palette),
}

impl AlertTheme {
    pub const ALL: [(&'static str, &'static str); 8] = [
        ("retro", "Retro"),
        ("modern", "Modern"),
        ("midnight", "Midnight"),
        ("graphite", "Graphite"),
        ("snow", "Snow"),
        ("forest", "Forest"),
        ("berry", "Berry"),
        ("mono", "Mono"),
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "retro" => Self::Retro,
            "modern" => Self::Modern,
            "midnight" => Self::Midnight,
            "graphite" => Self::Graphite,
            "snow" => Self::Snow,
            "forest" => Self::Forest,
            "berry" => Self::Berry,
            "mono" => Self::Mono,
            _ => return None,
        })
    }

    fn palette(self) -> Palette {
        match self {
            Self::Retro | Self::Modern => Palette::dark([16, 18, 24], [60, 66, 80], WHITE),
            Self::Midnight => Palette::midnight(),
            Self::Graphite => Palette::graphite(),
            Self::Snow => Palette::snow(),
            Self::Forest => Palette::forest(),
            Self::Berry => Palette::berry(),
            Self::Mono => Palette::mono(),
            Self::Station(p) => p,
        }
    }
}

/// How serious: it sets the colour and the wording of the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Emergency,
    Warning,
    Info,
}

impl Severity {
    pub const ALL: [(&'static str, &'static str); 3] = [
        ("emergency", "Emergency (red)"),
        ("warning", "Warning (amber)"),
        ("info", "Information (blue)"),
    ];

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "emergency" => Self::Emergency,
            "warning" => Self::Warning,
            "info" => Self::Info,
            _ => return None,
        })
    }

    pub fn accent(self) -> Rgb {
        match self {
            Self::Emergency => [208, 20, 32],
            Self::Warning => [242, 160, 16],
            Self::Info => [24, 116, 214],
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Emergency => "EMERGENCY ALERT",
            Self::Warning => "ALERT",
            Self::Info => "INFORMATION",
        }
    }

    fn mark(self) -> &'static str {
        match self {
            Self::Info => "i",
            _ => "!",
        }
    }
}

/// Over the whole screen, or a bar across the bottom of the programme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertStyle {
    #[default]
    Full,
    Bar,
}

impl AlertStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "bar" => Some(Self::Bar),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlertLook {
    pub theme: AlertTheme,
    pub severity: Severity,
}

/// Ink that reads on `bg`.
pub(crate) fn ink_on(bg: Rgb) -> Rgb {
    let l = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if l > 150.0 {
        [18, 18, 24]
    } else {
        WHITE
    }
}

// ------------------------------------------------------------------ smooth text

pub(crate) fn put(c: &mut Canvas, x: i32, y: i32, col: Rgb, a: u8) {
    if x < 0 || y < 0 || x >= c.w as i32 || y >= c.h as i32 || a == 0 {
        return;
    }
    let i = (y as usize * c.w + x as usize) * 3;
    let a = u32::from(a);
    for (d, s) in c.px[i..i + 3].iter_mut().zip(col) {
        *d = ((u32::from(*d) * (255 - a) + u32::from(s) * a + 127) / 255) as u8;
    }
}

/// Smooth text with its line box's top at `y`.
pub(crate) fn draw(c: &mut Canvas, x: i32, y: i32, s: &str, px: f32, face: Face, col: Rgb) {
    let baseline = y + (px * 0.82) as i32;
    for (ch, off) in text::layout(face, s, px) {
        let g = text::glyph(face, ch, px);
        let gx = x + off.round() as i32 + g.left;
        let gy = baseline - g.top;
        for row in 0..g.h {
            for colm in 0..g.w {
                let cov = g.coverage[row * g.w + colm];
                put(c, gx + colm as i32, gy + row as i32, col, cov);
            }
        }
    }
}

pub(crate) fn centred(c: &mut Canvas, cx: i32, y: i32, s: &str, px: f32, face: Face, col: Rgb) {
    let w = text::width(face, s, px) as i32;
    draw(c, cx - w / 2, y, s, px, face, col);
}

/// Words wrapped to `max_w` pixels; `None` when more than `max_lines` are needed.
pub(crate) fn wrap(
    s: &str,
    px: f32,
    face: Face,
    max_w: f32,
    max_lines: usize,
) -> Option<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let trial = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if text::width(face, &trial, px) <= max_w || cur.is_empty() {
            cur = trial;
        } else {
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    (lines.len() <= max_lines).then_some(lines)
}

pub(crate) fn disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, col: Rgb) {
    for y in -r..=r {
        for x in -r..=r {
            let d = ((x * x + y * y) as f32).sqrt();
            if d <= r as f32 - 0.5 {
                put(c, cx + x, cy + y, col, 255);
            } else if d < r as f32 + 0.5 {
                put(c, cx + x, cy + y, col, ((r as f32 + 0.5 - d) * 255.0) as u8);
            }
        }
    }
}

pub(crate) fn rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Rgb) {
    c.rect(x, y, w, h, col);
}

pub(crate) fn vgrad(c: &mut Canvas, top: Rgb, bottom: Rgb) {
    for y in 0..c.h as i32 {
        let col = mix(top, bottom, y as f32 / (c.h as f32 - 1.0).max(1.0));
        c.rect(0, y, c.w as i32, 1, col);
    }
}

// ------------------------------------------------------------------ the full screen

/// The size an alert screen is drawn at.
pub fn screen_size(look: &AlertLook) -> (usize, usize) {
    if look.theme == AlertTheme::Retro {
        (WIDTH, HEIGHT)
    } else {
        (WIDTH * 2, HEIGHT * 2)
    }
}

/// One frame of the full-screen alert, `t` seconds in (the header flashes and the crawl moves).
pub fn alert_screen(look: &AlertLook, event: &str, message: &str, t: f32) -> Canvas {
    if look.theme == AlertTheme::Retro {
        return crate::retro_alert(look.severity, event, message, t);
    }
    let pal = look.theme.palette();
    let acc = look.severity.accent();
    let acc_ink = ink_on(acc);
    let mut c = Canvas::new(WIDTH * 2, HEIGHT * 2);
    vgrad(&mut c, pal.bg_top, pal.bg_bottom);
    // The header band.
    let flash = (t * 1.6) as i64 % 2 == 0;
    rect(
        &mut c,
        0,
        0,
        1280,
        120,
        if flash {
            acc
        } else {
            mix(acc, pal.bg_top, 0.25)
        },
    );
    disc(&mut c, 88, 60, 34, acc_ink);
    centred(&mut c, 88, 28, look.severity.mark(), 54.0, Face::Bold, acc);
    draw(
        &mut c,
        150,
        30,
        look.severity.label(),
        52.0,
        Face::Bold,
        acc_ink,
    );
    // The event, as large as fits.
    let ink = pal.text_now;
    let body_ink = pal.text;
    let (mut size, mut lines) = (96.0, Vec::new());
    for s in [96.0, 80.0, 66.0, 54.0] {
        size = s;
        if let Some(l) = wrap(event, s, Face::Bold, 1120.0, 2) {
            lines = l;
            break;
        }
    }
    if lines.is_empty() {
        lines = vec![event.chars().take(30).collect()];
    }
    let mut y = 156;
    for l in &lines {
        draw(&mut c, 80, y, l, size, Face::Bold, ink);
        y += (size * 1.12) as i32;
    }
    rect(&mut c, 80, y + 8, 120, 6, acc);
    // The message; the crawl carries all of it when it does not fit.
    let room = (((600 - y - 34) / 58).max(1)) as usize;
    let mut body = wrap(message, 40.0, Face::Regular, 1120.0, room).unwrap_or_else(|| {
        let mut b = wrap(message, 40.0, Face::Regular, 1120.0, usize::MAX).unwrap_or_default();
        b.truncate(room);
        if let Some(last) = b.last_mut() {
            last.push('…');
        }
        b
    });
    body.retain(|l| !l.is_empty());
    let mut my = y + 38;
    for l in &body {
        draw(&mut c, 80, my, l, 40.0, Face::Regular, body_ink);
        my += 58;
    }
    // The crawl.
    rect(&mut c, 0, 636, 1280, 84, acc);
    let crawl = format!("{}        •        ", message.replace('\n', " "));
    let w = (text::width(Face::Bold, &crawl, 38.0) as i32).max(1);
    let x0 = -(((t * 200.0) as i32) % w);
    let mut x = x0;
    while x < 1280 {
        draw(&mut c, x, 660, &crawl, 38.0, Face::Bold, acc_ink);
        x += w;
    }
    c
}

// ------------------------------------------------------------------ the bar

/// A graphic to lay over the picture.
pub struct Graphic {
    pub w: u32,
    pub h: u32,
    pub png: Vec<u8>,
    /// The same picture as raw RGBA, for compositing a preview.
    pub rgba: Vec<u8>,
}

/// The pieces of the bottom bar: the bar itself (with the label and the event) and the crawl text
/// that FFmpeg slides along its lower band.
pub struct AlertBar {
    pub bar: Graphic,
    pub crawl: Graphic,
    /// Height of the band the crawl runs in (the bottom of the bar).
    pub band_h: u32,
}

struct Rgba {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Rgba {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![0; w * h * 4],
        }
    }
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, col: Rgb, a: u8) {
        for yy in y.max(0)..(y + h).min(self.h as i32) {
            for xx in x.max(0)..(x + w).min(self.w as i32) {
                let i = (yy as usize * self.w + xx as usize) * 4;
                self.px[i..i + 3].copy_from_slice(&col);
                self.px[i + 3] = a;
            }
        }
    }
    fn blend(&mut self, x: i32, y: i32, col: Rgb, a: u8) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || a == 0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let da = u32::from(self.px[i + 3]);
        let sa = u32::from(a);
        let oa = sa + da * (255 - sa) / 255;
        if oa == 0 {
            return;
        }
        for k in 0..3 {
            let d = u32::from(self.px[i + k]);
            let s = u32::from(col[k]);
            self.px[i + k] = ((s * sa * 255 + d * da * (255 - sa)) / (oa * 255)) as u8;
        }
        self.px[i + 3] = oa as u8;
    }
    fn text(&mut self, x: i32, y: i32, s: &str, px: f32, face: Face, col: Rgb) {
        let baseline = y + (px * 0.82) as i32;
        for (ch, off) in text::layout(face, s, px) {
            let g = text::glyph(face, ch, px);
            let gx = x + off.round() as i32 + g.left;
            let gy = baseline - g.top;
            for row in 0..g.h {
                for colm in 0..g.w {
                    self.blend(
                        gx + colm as i32,
                        gy + row as i32,
                        col,
                        g.coverage[row * g.w + colm],
                    );
                }
            }
        }
    }
    fn png(&self) -> Graphic {
        Graphic {
            w: self.w as u32,
            h: self.h as u32,
            png: png::rgba_png(self.w as u32, self.h as u32, &self.px),
            rgba: self.px.clone(),
        }
    }
}

/// The bar for a picture `width` pixels wide.
pub fn alert_bar(look: &AlertLook, event: &str, message: &str, width: u32) -> AlertBar {
    let k = width as f32 / 1280.0;
    let acc = look.severity.accent();
    let acc_ink = ink_on(acc);
    let crawl_text = format!("{}        •        ", message.replace('\n', " "));
    if look.theme == AlertTheme::Retro {
        return retro_bar(look, event, &crawl_text, width);
    }
    let pal = look.theme.palette();
    let bar_h = (172.0 * k).round().max(70.0) as usize;
    let band_h = (58.0 * k).round().max(24.0) as usize;
    let mut r = Rgba::new(width as usize, bar_h);
    // The panel above the band, then the band.
    let panel = pal.bg_top;
    r.fill(0, 0, width as i32, (bar_h - band_h) as i32, panel, 236);
    r.fill(
        0,
        (bar_h - band_h) as i32,
        width as i32,
        band_h as i32,
        acc,
        244,
    );
    r.fill(0, 0, (14.0 * k) as i32, bar_h as i32, acc, 255);
    // The label chip, then the event.
    let chip_px = 26.0 * k;
    let label = look.severity.label();
    let chip_w = text::width(Face::Bold, label, chip_px) as i32 + (36.0 * k) as i32;
    let (cx, cy, ch) = ((36.0 * k) as i32, (12.0 * k) as i32, (40.0 * k) as i32);
    r.fill(cx, cy, chip_w, ch, acc, 255);
    r.text(
        cx + (18.0 * k) as i32,
        cy + (6.0 * k) as i32,
        label,
        chip_px,
        Face::Bold,
        acc_ink,
    );
    let ev_px = 38.0 * k;
    let mut ev = event.to_string();
    let max_w = width as f32 - cx as f32 - 40.0 * k;
    while ev.chars().count() > 4 && text::width(Face::Bold, &ev, ev_px) > max_w {
        ev.pop();
    }
    r.text(
        cx,
        cy + ch + (6.0 * k) as i32,
        &ev,
        ev_px,
        Face::Bold,
        pal.text_now,
    );

    // The crawl, as text only: FFmpeg slides it along the band.
    let c_px = 34.0 * k;
    let cw = (text::width(Face::Bold, &crawl_text, c_px) as usize + 8).max(16);
    let chh = (c_px * 1.3) as usize;
    let mut s = Rgba::new(cw, chh);
    s.text(4, 0, &crawl_text, c_px, Face::Bold, acc_ink);
    AlertBar {
        bar: r.png(),
        crawl: s.png(),
        band_h: band_h as u32,
    }
}

fn retro_bar(look: &AlertLook, event: &str, crawl_text: &str, width: u32) -> AlertBar {
    // Drawn in 640-wide points and scaled with the picture, like the other pixel graphics.
    let acc = look.severity.accent();
    let scale = (width / 640).max(1) as i32;
    let (w, bar_h, band_h) = (width as i32, 56 * scale, 16 * scale);
    let mut c = Canvas::new(w as usize, bar_h as usize);
    c.fill(KEY);
    c.rect(0, 0, w, bar_h - band_h, [10, 10, 14]);
    c.rect(0, bar_h - band_h, w, band_h, acc);
    c.rect(0, 0, w, 2 * scale, WHITE);
    c.rect(0, 0, 6 * scale, bar_h, acc);
    let label = look.severity.label();
    c.text(12 * scale, 5 * scale, label, scale, WHITE);
    let mut ev: String = event.to_uppercase();
    let max = ((w - 24 * scale) / (8 * 2 * scale)).max(4) as usize;
    if ev.chars().count() > max {
        ev = ev.chars().take(max - 1).collect::<String>() + "~";
    }
    c.text(12 * scale, 16 * scale, &ev, 2 * scale, [255, 214, 0]);
    let to = |c: &Canvas| {
        let mut rgba = Vec::with_capacity(c.w * c.h * 4);
        for p in c.px.as_chunks::<3>().0 {
            if *p == KEY {
                rgba.extend([0, 0, 0, 0]);
            } else {
                rgba.extend([p[0], p[1], p[2], 255]);
            }
        }
        Graphic {
            w: c.w as u32,
            h: c.h as u32,
            png: png::rgba_png(c.w as u32, c.h as u32, &rgba),
            rgba,
        }
    };
    let bar = to(&c);
    let cw = crate::text_width(crawl_text, scale).max(16);
    let mut s = Canvas::new(cw as usize, (8 * scale) as usize);
    s.fill(KEY);
    s.text(0, 0, crawl_text, scale, WHITE);
    let crawl = to(&s);
    AlertBar {
        bar,
        crawl,
        band_h: band_h as u32,
    }
}

/// A frame as it would look on air: the full screen, or the bar and crawl over a sample picture.
pub fn alert_preview(
    look: &AlertLook,
    style: AlertStyle,
    event: &str,
    message: &str,
    t: f32,
) -> Canvas {
    if style == AlertStyle::Full {
        return alert_screen(look, event, message, t);
    }
    let (w, h) = (1280usize, 720usize);
    let mut c = Canvas::new(w, h);
    // A stand-in for whatever is on air: a sky and a hill.
    vgrad(&mut c, [92, 150, 214], [214, 226, 238]);
    c.rect(0, 470, w as i32, 250, [52, 110, 70]);
    disc(&mut c, 1010, 160, 70, [255, 224, 120]);
    draw(
        &mut c,
        64,
        40,
        "SAMPLE PICTURE",
        34.0,
        Face::Bold,
        [255, 255, 255],
    );
    let bar = alert_bar(look, event, message, w as u32);
    let (bar_png, crawl_png) = (&bar.bar, &bar.crawl);
    let bar_y = h as i32 - bar_png.h as i32 - 36;
    blit_png(&mut c, bar_png, 0, bar_y);
    let cy =
        bar_y + bar_png.h as i32 - bar.band_h as i32 + (bar.band_h as i32 - crawl_png.h as i32) / 2;
    let cx = w as i32 - ((t * 170.0) as i32 % (w as i32 + crawl_png.w as i32));
    blit_png(&mut c, crawl_png, cx, cy);
    c
}

fn blit_png(c: &mut Canvas, g: &Graphic, x: i32, y: i32) {
    let px = &g.rgba;
    for row in 0..g.h as i32 {
        for col in 0..g.w as i32 {
            let i = ((row as u32 * g.w + col as u32) * 4) as usize;
            put(
                c,
                x + col,
                y + row,
                [px[i], px[i + 1], px[i + 2]],
                px[i + 3],
            );
        }
    }
}
