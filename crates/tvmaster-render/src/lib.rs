//! Station graphics: bumpers drawn frame by frame as raw RGB pixels.
//!
//! The look is deliberately retro: a chunky 8x8 pixel font, scrolling stripes
//! and scanlines, drawn at 640x360 and scaled up by FFmpeg without smoothing.
//! Everything here is original artwork and needs no font files, so the same
//! output appears on every platform. Colours come from a per-channel theme so
//! each channel looks like its own station.

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

/// A channel's colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub dark: Rgb,
    pub primary: Rgb,
    pub accent: Rgb,
}

impl Theme {
    pub fn from_hue(h: f32) -> Self {
        Self {
            dark: hsv(h, 0.65, 0.22),
            primary: hsv(h, 0.75, 0.95),
            accent: hsv(h + 0.13, 0.85, 1.0),
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
    c.vgradient(th.dark, [4, 4, 8]);
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
    c.rect(0, 316, WIDTH as i32, 2, WHITE);
    c.rect(0, 344, WIDTH as i32, 2, WHITE);
}

fn header(c: &mut Canvas, th: &Theme, name: &str, t: f32) {
    let x = 24 - slide(t, 0.0);
    c.rect(x - 8, 18, 6, 22, th.accent);
    c.text_shadow(x + 4, 20, &truncate(name, 34), 2, WHITE);
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

/// Render one frame of a bumper `dur` seconds long, `t` seconds in.
pub fn render(scene: &Scene, th: &Theme, t: f32, dur: f32) -> Canvas {
    let mut c = Canvas::new(WIDTH, HEIGHT);
    background(&mut c, th, t);
    match scene {
        Scene::StationId { name } => {
            c.text_centered(96, "YOU'RE WATCHING", 2, th.accent);
            let (scale, ls) = layout_title(name, 580, 2);
            let y = 130 - (ls.len() as i32 - 1) * scale * 5;
            lines(&mut c, y, &ls, scale, WHITE, t, 0.1);
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
                WHITE,
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
            lines(&mut c, y, &ls, scale.min(5), WHITE, t, 0.2);
        }
        Scene::UpNext { name, title, time } => {
            header(&mut c, th, name, t);
            let bar = 70 - slide(t, 0.0) / 2;
            c.rect(0, bar, 640, 40, th.accent);
            c.text(24 - slide(t, 0.05), bar + 8, "UP NEXT", 3, [10, 10, 20]);
            let (scale, ls) = layout_title(title, 580, 2);
            let end = lines(&mut c, 134, &ls, scale.min(5), WHITE, t, 0.25);
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
            c.text(24 - slide(t, 0.05), bar + 8, title, 3, [10, 10, 20]);
            let mut y = 124;
            for (i, (time, prog)) in items.iter().take(4).enumerate() {
                let off = slide(t, 0.2 + i as f32 * 0.12);
                c.text_shadow(32 - off, y, time, 2, th.primary);
                c.text_shadow(32 + 10 * 16 - off, y, &truncate(prog, 26), 2, WHITE);
                y += 44;
            }
        }
    }
    c.scanlines(0.16);
    // Fade in from black and out to black.
    let f = (t / 0.3).min(1.0).min(((dur - t) / 0.4).max(0.0));
    c.brightness(f);
    c
}

// ------------------------------------------------------------------ guide

/// One programme on the guide grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideCell {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideRow {
    pub number: u32,
    pub name: String,
    pub cells: Vec<GuideCell>,
}

/// Everything the guide channel draws. The caller formats times (it knows the
/// time zone), so this crate stays free of time-zone logic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuideData {
    pub title: String,
    pub now_ms: i64,
    pub window_start_ms: i64,
    pub window_ms: i64,
    /// "9:04 PM"
    pub clock: String,
    /// "SUN NOV 15"
    pub date: String,
    /// Time header labels: (instant, text), e.g. every half hour.
    pub marks: Vec<(i64, String)>,
    pub rows: Vec<GuideRow>,
    /// Text that scrolls along the bottom.
    pub ticker: String,
}

const GUIDE_HEADER_H: i32 = 36;
const GUIDE_TIMEBAR_H: i32 = 20;
const GUIDE_ROW_H: i32 = 43;
const GUIDE_ROWS: usize = 6;
const GUIDE_LABEL_W: i32 = 104;
const GUIDE_GRID_X: i32 = 108;
const GUIDE_GRID_W: i32 = 528;
const GUIDE_TICKER_Y: i32 = 326;
/// Seconds each page of channels stays up before scrolling to the next.
const GUIDE_PAGE_SECS: f32 = 9.0;
const GUIDE_SCROLL_SECS: f32 = 0.9;

fn guide_scroll_offset(rows: usize, t: f32) -> i32 {
    if rows <= GUIDE_ROWS {
        return 0;
    }
    let pages = rows.div_ceil(GUIDE_ROWS);
    let page = (t / GUIDE_PAGE_SECS).floor();
    let into = t - page * GUIDE_PAGE_SECS;
    let page = page as usize % pages;
    let base = (page * GUIDE_ROWS) as i32 * GUIDE_ROW_H;
    let step = GUIDE_ROWS as i32 * GUIDE_ROW_H;
    let k = (into - (GUIDE_PAGE_SECS - GUIDE_SCROLL_SECS)) / GUIDE_SCROLL_SECS;
    base + (ease_out(k.max(0.0)) * step as f32) as i32
}

fn guide_x(data: &GuideData, ms: i64) -> i32 {
    let f = (ms - data.window_start_ms) as f64 / data.window_ms.max(1) as f64;
    // Clamp so absurd times (or a tiny window) cannot overflow.
    GUIDE_GRID_X
        + (f * GUIDE_GRID_W as f64)
            .round()
            .clamp(-100_000.0, 100_000.0) as i32
}

fn guide_row(c: &mut Canvas, th: &Theme, data: &GuideData, row: &GuideRow, y: i32, alt: bool) {
    // channel label
    c.rect(
        0,
        y,
        GUIDE_LABEL_W,
        GUIDE_ROW_H - 2,
        mix(th.dark, th.primary, 0.25),
    );
    c.text_shadow(8, y + 4, &row.number.to_string(), 3, WHITE);
    c.text(8, y + 31, &truncate(&row.name, 12), 1, th.accent);
    // programme cells
    let base = if alt {
        mix(th.dark, [0, 0, 0], 0.15)
    } else {
        mix(th.dark, th.primary, 0.12)
    };
    c.rect(GUIDE_GRID_X, y, GUIDE_GRID_W, GUIDE_ROW_H - 2, [8, 8, 12]);
    for cell in &row.cells {
        let (s, e) = (
            cell.start_ms.max(data.window_start_ms),
            cell.end_ms.min(data.window_start_ms + data.window_ms),
        );
        if e <= s {
            continue;
        }
        let (x0, x1) = (guide_x(data, s), guide_x(data, e));
        let w = x1 - x0 - 2;
        if w < 10 {
            continue;
        }
        let airing = cell.start_ms <= data.now_ms && data.now_ms < cell.end_ms;
        let fill = if airing {
            mix(th.dark, th.primary, 0.55)
        } else {
            base
        };
        c.rect(x0 + 1, y, w, GUIDE_ROW_H - 2, fill);
        if airing {
            c.rect(x0 + 1, y, w, 2, th.accent);
        }
        // Too narrow for readable text (very short programmes): just a block.
        if w < 56 {
            continue;
        }
        let mut title = cell.title.clone();
        if cell.start_ms < data.window_start_ms {
            title = format!("< {title}");
        }
        if cell.end_ms > data.window_start_ms + data.window_ms {
            title.push_str(" >");
        }
        let chars = ((w - 8) / 8).max(1) as usize;
        let lines = wrap(&title, chars, 3).unwrap_or_else(|| {
            let mut l = wrap(&title, chars, usize::MAX).unwrap_or_default();
            l.truncate(3);
            if let Some(last) = l.last_mut() {
                *last = truncate(last, chars);
                if !last.ends_with("...") {
                    last.push_str("...");
                    *last = last.chars().take(chars).collect();
                }
            }
            l
        });
        for (i, line) in lines.iter().enumerate() {
            let col = if airing { WHITE } else { [210, 210, 220] };
            c.text(x0 + 5, y + 6 + i as i32 * 11, line, 1, col);
        }
    }
}

/// Render one frame of the guide channel, `t` seconds into the stream.
pub fn render_guide(data: &GuideData, th: &Theme, t: f32) -> Canvas {
    let mut c = Canvas::new(WIDTH, HEIGHT);
    c.vgradient(th.dark, [4, 4, 8]);

    // header
    c.rect(
        0,
        0,
        WIDTH as i32,
        GUIDE_HEADER_H,
        mix(th.dark, th.primary, 0.35),
    );
    c.rect(0, GUIDE_HEADER_H - 3, WIDTH as i32, 3, th.accent);
    let title = if data.title.trim().is_empty() {
        "TV GUIDE"
    } else {
        data.title.as_str()
    };
    c.text_shadow(12, 9, &truncate(title, 28), 2, WHITE);
    let clock_w = text_width(&data.clock, 2);
    c.text_shadow(WIDTH as i32 - clock_w - 12, 5, &data.clock, 2, WHITE);
    let date_w = text_width(&data.date, 1);
    c.text(WIDTH as i32 - date_w - 12, 24, &data.date, 1, th.accent);

    // time bar
    let tb = GUIDE_HEADER_H;
    c.rect(0, tb, WIDTH as i32, GUIDE_TIMEBAR_H, [6, 6, 10]);
    c.text(8, tb + 6, "CHANNEL", 1, th.accent);
    for (ms, label) in &data.marks {
        let x = guide_x(data, *ms);
        if x >= GUIDE_GRID_X && x < GUIDE_GRID_X + GUIDE_GRID_W - 8 * label.chars().count() as i32 {
            c.rect(x, tb + 2, 1, GUIDE_TIMEBAR_H - 2, th.primary);
            c.text(x + 4, tb + 6, label, 1, WHITE);
        }
    }

    // channel rows (scrolling pages when there are more than fit)
    let top = GUIDE_HEADER_H + GUIDE_TIMEBAR_H;
    let mut grid = Canvas::new(WIDTH, GUIDE_ROWS * GUIDE_ROW_H as usize);
    grid.fill([4, 4, 8]);
    let n = data.rows.len();
    if n > 0 {
        let off = guide_scroll_offset(n, t);
        let first = off / GUIDE_ROW_H;
        for v in first..first + GUIDE_ROWS as i32 + 2 {
            let (virt, y) = (v as usize, v * GUIDE_ROW_H - off);
            if n <= GUIDE_ROWS && virt >= n {
                break;
            }
            guide_row(&mut grid, th, data, &data.rows[virt % n], y, virt % 2 == 1);
        }
    }
    c.blit(&grid, 0, top);

    // "now" marker
    let nx = guide_x(data, data.now_ms);
    if (GUIDE_GRID_X..GUIDE_GRID_X + GUIDE_GRID_W).contains(&nx) {
        c.rect(
            nx,
            tb,
            2,
            GUIDE_TIMEBAR_H + (GUIDE_ROWS as i32 * GUIDE_ROW_H),
            th.accent,
        );
        c.rect(nx - 3, tb, 8, 4, th.accent);
    }

    // ticker
    c.rect(0, GUIDE_TICKER_Y, WIDTH as i32, 26, [6, 6, 10]);
    c.rect(0, GUIDE_TICKER_Y, WIDTH as i32, 2, th.primary);
    if !data.ticker.is_empty() {
        let text = format!("   {}   *   ", data.ticker.to_uppercase());
        let tw = text_width(&text, 2);
        let x = WIDTH as i32 - ((t * 70.0) as i32 % (tw + WIDTH as i32));
        c.text(x, GUIDE_TICKER_Y + 6, &text, 2, th.accent);
    }
    c.scanlines(0.12);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn guide(rows: usize) -> GuideData {
        let start = 1_000_000_000_000;
        let half = 30 * 60_000;
        GuideData {
            title: "Retro TV Guide".into(),
            now_ms: start + 10 * 60_000,
            window_start_ms: start,
            window_ms: 3 * 60 * 60_000,
            clock: "9:10 PM".into(),
            date: "SUN NOV 15".into(),
            marks: (0..6)
                .map(|i| {
                    (
                        start + i * half,
                        format!("{}:{:02}", 9 + i / 2, (i % 2) * 30),
                    )
                })
                .collect(),
            rows: (0..rows)
                .map(|r| GuideRow {
                    number: r as u32 + 2,
                    name: format!("Channel {r}"),
                    cells: vec![
                        GuideCell {
                            start_ms: start - half,
                            end_ms: start + half,
                            title: format!("Show {r} Pilot"),
                        },
                        GuideCell {
                            start_ms: start + half,
                            end_ms: start + 2 * half,
                            title: "A Very Long Programme Title That Needs Wrapping".into(),
                        },
                        GuideCell {
                            start_ms: start + 2 * half,
                            end_ms: start + 6 * half,
                            title: "Marathon".into(),
                        },
                    ],
                })
                .collect(),
            ticker: "Now on channel 4: Taxi".into(),
        }
    }

    #[test]
    fn the_guide_renders_with_any_number_of_channels() {
        let th = Theme::for_channel("guide");
        for n in [0, 1, 5, 6, 7, 20] {
            for t in [0.0, 3.0, 8.5, 9.4, 40.0] {
                let c = render_guide(&guide(n), &th, t);
                assert_eq!((c.w, c.h), (WIDTH, HEIGHT));
            }
        }
        let c = render_guide(&guide(5), &th, 1.0);
        let distinct: std::collections::HashSet<&[u8]> = c.px.chunks(3).step_by(29).collect();
        assert!(distinct.len() > 10);
    }

    #[test]
    fn few_channels_stay_put_and_many_channels_scroll_in_pages() {
        let th = Theme::for_channel("guide");
        let grid_area = |c: &Canvas| c.px[(60 * WIDTH * 3)..(300 * WIDTH * 3)].to_vec();
        let few = guide(4);
        assert_eq!(
            grid_area(&render_guide(&few, &th, 0.0)),
            grid_area(&render_guide(&few, &th, 30.0)),
            "no scrolling needed"
        );
        let many = guide(14);
        let first = grid_area(&render_guide(&many, &th, 1.0));
        let later = grid_area(&render_guide(&many, &th, 12.0));
        assert_ne!(first, later, "moved on to the next page");
        // mid-scroll differs from both ends
        let mid = grid_area(&render_guide(&many, &th, 9.0 - 0.45));
        assert_ne!(mid, first);
        assert_ne!(mid, later);
        // it wraps: three pages of 6 -> back at the start
        assert_eq!(first, grid_area(&render_guide(&many, &th, 1.0 + 3.0 * 9.0)));
    }

    #[test]
    fn scroll_offsets_move_by_whole_pages() {
        assert_eq!(guide_scroll_offset(6, 100.0), 0);
        assert_eq!(guide_scroll_offset(12, 1.0), 0);
        let page = GUIDE_ROWS as i32 * GUIDE_ROW_H;
        assert_eq!(guide_scroll_offset(12, 9.0 + 1.0), page);
        assert_eq!(
            guide_scroll_offset(12, 18.0 + 1.0),
            0,
            "wraps after two pages"
        );
    }

    #[test]
    fn the_clock_and_airing_highlight_change_the_picture() {
        let th = Theme::for_channel("guide");
        let a = guide(3);
        let mut b = guide(3);
        b.clock = "9:11 PM".into();
        assert_ne!(render_guide(&a, &th, 1.0).px, render_guide(&b, &th, 1.0).px);
        let mut c = guide(3);
        c.now_ms += 40 * 60_000; // the second programme is now airing
        assert_ne!(render_guide(&a, &th, 1.0).px, render_guide(&c, &th, 1.0).px);
    }

    #[test]
    fn degenerate_guide_data_does_not_panic() {
        let th = Theme::for_channel("g");
        let mut d = guide(3);
        d.window_ms = 0;
        render_guide(&d, &th, 1.0);
        d.window_ms = 1;
        d.rows[0].cells[0].title = "😀".repeat(200);
        render_guide(&d, &th, 1.0);
        let d = GuideData::default();
        render_guide(&d, &th, 5.0);
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
