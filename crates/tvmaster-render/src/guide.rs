//! The guide channel's picture: a listing of what is on, in several looks.
//!
//! Everything is drawn at 640x360 like the bumpers. The caller formats times
//! (it knows the time zone), so this crate stays free of time-zone logic. A
//! live video window (picture-in-picture) is not drawn here: the picture
//! reserves the space and [`geometry`] says where it is, so FFmpeg can place
//! the video there.

use super::{ease_out, mix, text_width, truncate, wrap, Canvas, Rgb, Theme, HEIGHT, WHITE, WIDTH};

/// One programme on the guide.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuideCell {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
    /// "9:00-9:30 PM", for the now-and-next look and the video window's card.
    pub range: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuideRow {
    /// The channel's id (not drawn; lets the video window name the channel it shows).
    pub id: String,
    pub number: u32,
    pub name: String,
    pub cells: Vec<GuideCell>,
}

/// Everything the guide channel draws.
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

// ------------------------------------------------------------------ colours

/// Every colour the guide uses. [`Palette::from_theme`] is the original
/// look, derived from the channel; the others are complete alternatives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg_top: Rgb,
    pub bg_bottom: Rgb,
    pub header: Rgb,
    pub header_text: Rgb,
    pub header_sub: Rgb,
    pub rule: Rgb,
    pub label_bg: Rgb,
    pub label_num: Rgb,
    pub label_name: Rgb,
    pub bar: Rgb,
    pub bar_text: Rgb,
    pub tick: Rgb,
    pub empty: Rgb,
    pub cell: Rgb,
    pub cell_alt: Rgb,
    pub cell_now: Rgb,
    pub text: Rgb,
    pub text_now: Rgb,
    pub accent: Rgb,
    pub ticker_bg: Rgb,
    pub ticker_text: Rgb,
    pub panel: Rgb,
}

impl Palette {
    /// A dark guide from three colours: the background, the main colour and the highlight.
    pub fn dark(dark: Rgb, primary: Rgb, accent: Rgb) -> Self {
        Self {
            bg_top: dark,
            bg_bottom: [4, 4, 8],
            header: mix(dark, primary, 0.35),
            header_text: WHITE,
            header_sub: accent,
            rule: accent,
            label_bg: mix(dark, primary, 0.25),
            label_num: WHITE,
            label_name: accent,
            bar: [6, 6, 10],
            bar_text: WHITE,
            tick: primary,
            empty: [8, 8, 12],
            cell: mix(dark, primary, 0.12),
            cell_alt: mix(dark, [0, 0, 0], 0.15),
            cell_now: mix(dark, primary, 0.55),
            text: [210, 210, 220],
            text_now: WHITE,
            accent,
            ticker_bg: [6, 6, 10],
            ticker_text: accent,
            panel: mix(dark, primary, 0.2),
        }
    }

    /// Colours derived from the channel, so each guide looks like its own station.
    pub fn from_theme(th: &Theme) -> Self {
        Self::dark(th.dark, th.primary, th.accent)
    }

    /// Two colours you choose; the dark background is the first one, darkened.
    pub fn custom(primary: Rgb, accent: Rgb) -> Self {
        let dark = [
            (primary[0] as f32 * 0.22) as u8,
            (primary[1] as f32 * 0.22) as u8,
            (primary[2] as f32 * 0.22) as u8,
        ];
        Self::dark(dark, primary, accent)
    }

    pub fn prevue() -> Self {
        Self::dark([6, 10, 92], [44, 84, 224], [255, 224, 64])
    }
    pub fn amber() -> Self {
        Self::dark([22, 11, 0], [255, 176, 0], [255, 214, 102])
    }
    pub fn phosphor() -> Self {
        Self::dark([0, 26, 8], [40, 220, 80], [170, 255, 170])
    }
    pub fn neon() -> Self {
        Self::dark([20, 0, 34], [255, 40, 200], [0, 240, 255])
    }
    pub fn sunset() -> Self {
        Self::dark([44, 8, 26], [255, 110, 60], [255, 200, 80])
    }
    pub fn ocean() -> Self {
        Self::dark([0, 26, 44], [0, 160, 200], [120, 240, 220])
    }
    pub fn slate() -> Self {
        Self::dark([24, 26, 34], [112, 124, 148], [240, 200, 60])
    }

    /// Light, like a printed listing.
    pub fn paper() -> Self {
        let ink = [40, 40, 52];
        Self {
            bg_top: [244, 238, 222],
            bg_bottom: [226, 218, 196],
            header: [32, 40, 70],
            header_text: [255, 250, 235],
            header_sub: [255, 200, 90],
            rule: [200, 60, 40],
            label_bg: [60, 70, 100],
            label_num: WHITE,
            label_name: [255, 220, 150],
            bar: [214, 206, 184],
            bar_text: ink,
            tick: [120, 110, 90],
            empty: [232, 224, 204],
            cell: [255, 252, 240],
            cell_alt: [247, 242, 226],
            cell_now: [255, 226, 150],
            text: ink,
            text_now: [20, 20, 30],
            accent: [200, 60, 40],
            ticker_bg: [32, 40, 70],
            ticker_text: [255, 240, 200],
            panel: [255, 252, 240],
        }
    }
}

// ------------------------------------------------------------------ look

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// A band across the top: video on the left, what is on beside it.
    TopLeft,
    TopRight,
    SideLeft,
    #[default]
    SideRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl WindowSize {
    fn width(self) -> i32 {
        match self {
            WindowSize::Small => 144,
            WindowSize::Medium => 192,
            WindowSize::Large => 256,
        }
    }
}

/// A video window inside the guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowLook {
    pub placement: Placement,
    pub size: WindowSize,
    /// A card beside the window saying what is on.
    pub info: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuideLook {
    pub palette: Palette,
    /// One big row per channel (now and next) instead of the time grid.
    pub now_next: bool,
    /// Channels on screen at once.
    pub rows: usize,
    /// Roll continuously instead of page by page.
    pub smooth: bool,
    pub page_secs: f32,
    pub numbers: bool,
    pub names: bool,
    pub progress: bool,
    pub scanlines: bool,
    pub title: bool,
    pub clock: bool,
    pub date: bool,
    /// Leave room for a logo picture at the left of the heading.
    pub logo: bool,
    pub ticker: bool,
    /// Pixels per second.
    pub ticker_speed: f32,
    pub window: Option<WindowLook>,
}

impl GuideLook {
    pub fn new(palette: Palette) -> Self {
        Self {
            palette,
            now_next: false,
            rows: 6,
            smooth: false,
            page_secs: 9.0,
            numbers: true,
            names: true,
            progress: false,
            scanlines: true,
            title: true,
            clock: true,
            date: true,
            logo: false,
            ticker: true,
            ticker_speed: 70.0,
            window: None,
        }
    }
}

impl Default for GuideLook {
    fn default() -> Self {
        Self::new(Palette::from_theme(&Theme::for_channel("guide")))
    }
}

/// What differs from frame to frame beyond time: which channel the video
/// window is showing, and whether it has a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GuideView {
    /// Index into `GuideData::rows` of the channel in the window.
    pub window_row: Option<usize>,
    /// The video is flowing (otherwise the window says NO SIGNAL).
    pub window_live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where everything goes, in 640x360 pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// The video itself (inside its frame).
    pub window: Option<Rect>,
    /// Where a logo picture goes in the heading.
    pub logo: Option<Rect>,
    /// The card beside the video window.
    pub panel: Option<Rect>,
    left: i32,
    area_w: i32,
    label_w: i32,
    bar_y: i32,
    rows_y: i32,
    row_h: i32,
    visible: usize,
    ticker_y: i32,
}

const HEADER_H: i32 = 36;
const BAR_H: i32 = 20;
const TICKER_Y: i32 = 326;
const MIN_ROW_H: i32 = 26;
const SCROLL_SECS: f32 = 0.9;
/// Pixels per second of a smooth roll.
const ROLL_SPEED: f32 = 14.0;
const FRAME: i32 = 2;
const PAD: i32 = 6;

pub fn geometry(look: &GuideLook) -> Geometry {
    let label_w = match (look.numbers, look.names) {
        (true, true) => 104,
        (true, false) => 60,
        (false, true) => 100,
        (false, false) => 0,
    };
    let body_bottom = if look.ticker {
        TICKER_Y - 4
    } else {
        HEIGHT as i32 - 4
    };
    let mut bar_y = HEADER_H;
    let (mut left, mut area_w) = (0, WIDTH as i32);
    let (mut window, mut panel) = (None, None);
    if let Some(w) = look.window {
        let pw = w.size.width();
        let ph = pw * 9 / 16;
        match w.placement {
            Placement::SideLeft | Placement::SideRight => {
                let col = pw + 2 * FRAME + 2 * PAD;
                let x = if w.placement == Placement::SideLeft {
                    PAD + FRAME
                } else {
                    WIDTH as i32 - col + PAD + FRAME
                };
                let y = HEADER_H + 6 + FRAME;
                window = Some(Rect { x, y, w: pw, h: ph });
                if w.info {
                    let top = y + ph + FRAME + 6;
                    if body_bottom - top >= 50 {
                        panel = Some(Rect {
                            x: x - FRAME,
                            y: top,
                            w: pw + 2 * FRAME,
                            h: body_bottom - top,
                        });
                    }
                }
                area_w = WIDTH as i32 - col;
                if w.placement == Placement::SideLeft {
                    left = col;
                }
            }
            Placement::TopLeft | Placement::TopRight => {
                let y = HEADER_H + 6 + FRAME;
                let x = if w.placement == Placement::TopLeft {
                    PAD + FRAME
                } else {
                    WIDTH as i32 - PAD - FRAME - pw
                };
                window = Some(Rect { x, y, w: pw, h: ph });
                if w.info {
                    let px = if w.placement == Placement::TopLeft {
                        x + pw + FRAME + 8
                    } else {
                        PAD
                    };
                    let pwid = WIDTH as i32 - pw - 2 * FRAME - 8 - 2 * PAD;
                    panel = Some(Rect {
                        x: px,
                        y: y - FRAME,
                        w: pwid,
                        h: ph + 2 * FRAME,
                    });
                }
                bar_y = y + ph + FRAME + 6;
            }
        }
    }
    let rows_y = bar_y + BAR_H;
    let area_h = (body_bottom - rows_y).max(MIN_ROW_H);
    let visible = look
        .rows
        .clamp(1, 10)
        .min((area_h / MIN_ROW_H) as usize)
        .max(1);
    let row_h = area_h / visible as i32;
    let logo = look.logo.then_some(Rect {
        x: 8,
        y: 4,
        w: 56,
        h: HEADER_H - 11,
    });
    Geometry {
        window,
        logo,
        panel,
        left,
        area_w,
        label_w,
        bar_y,
        rows_y,
        row_h,
        visible,
        ticker_y: TICKER_Y,
    }
}

impl Geometry {
    /// How many channels fit on screen at once.
    pub fn visible_rows(&self) -> usize {
        self.visible
    }
    /// Left edge of the programme area, relative to the area.
    fn gx(&self) -> i32 {
        if self.label_w == 0 {
            4
        } else {
            self.label_w + 4
        }
    }
    fn gw(&self) -> i32 {
        (self.area_w - self.gx() - 4).max(40)
    }
}

// ------------------------------------------------------------------ drawing

fn scroll_offset(rows: usize, visible: usize, row_h: i32, look: &GuideLook, t: f32) -> i32 {
    if rows <= visible {
        return 0;
    }
    if look.smooth {
        let total = rows as i32 * row_h;
        return (t * ROLL_SPEED) as i32 % total.max(1);
    }
    let pages = rows.div_ceil(visible);
    let page = (t / look.page_secs).floor();
    let into = t - page * look.page_secs;
    let page = page as usize % pages;
    let base = (page * visible) as i32 * row_h;
    let step = visible as i32 * row_h;
    let k = (into - (look.page_secs - SCROLL_SECS)) / SCROLL_SECS;
    base + (ease_out(k.max(0.0)) * step as f32) as i32
}

fn x_of(data: &GuideData, g: &Geometry, ms: i64) -> i32 {
    let f = (ms - data.window_start_ms) as f64 / data.window_ms.max(1) as f64;
    // Clamp so absurd times (or a tiny window) cannot overflow.
    g.gx() + (f * g.gw() as f64).round().clamp(-100_000.0, 100_000.0) as i32
}

/// Wrap `title` into at most `max_lines` lines of `chars` characters, ending
/// the last in "..." when it is cut.
fn fit_lines(title: &str, chars: usize, max_lines: usize) -> Vec<String> {
    wrap(title, chars, max_lines).unwrap_or_else(|| {
        let mut l = wrap(title, chars, usize::MAX).unwrap_or_default();
        l.truncate(max_lines);
        if let Some(last) = l.last_mut() {
            *last = truncate(last, chars);
            if !last.ends_with("...") {
                last.push_str("...");
                *last = last.chars().take(chars).collect();
            }
        }
        l
    })
}

fn airing(cell: &GuideCell, now: i64) -> bool {
    cell.start_ms <= now && now < cell.end_ms
}

fn label(c: &mut Canvas, look: &GuideLook, g: &Geometry, row: &GuideRow, y: i32) {
    if g.label_w == 0 {
        return;
    }
    let p = &look.palette;
    let h = g.row_h - 2;
    c.rect(0, y, g.label_w, h, p.label_bg);
    let tall = h >= 38;
    if look.numbers {
        let scale = if tall { 3 } else { 2 };
        let ny = if tall { y + 4 } else { y + (h - 16) / 2 };
        c.text_shadow(8, ny, &row.number.to_string(), scale, p.label_num);
        if look.names && !tall {
            let used = text_width(&row.number.to_string(), scale) + 12;
            let chars = ((g.label_w - used) / 8).max(0) as usize;
            if chars >= 3 {
                c.text(
                    used,
                    y + (h - 8) / 2,
                    &truncate(&row.name, chars),
                    1,
                    p.label_name,
                );
            }
        } else if look.names {
            c.text(
                8,
                y + h - 12,
                &truncate(&row.name, ((g.label_w - 12) / 8) as usize),
                1,
                p.label_name,
            );
        }
    } else if look.names {
        let lines = fit_lines(
            &row.name,
            ((g.label_w - 12) / 8).max(3) as usize,
            ((h - 4) / 11).max(1) as usize,
        );
        for (i, l) in lines.iter().enumerate() {
            c.text(6, y + 5 + i as i32 * 11, l, 1, p.label_name);
        }
    }
}

fn progress_bar(
    c: &mut Canvas,
    look: &GuideLook,
    x: i32,
    y: i32,
    w: i32,
    cell: &GuideCell,
    now: i64,
) {
    let span = (cell.end_ms - cell.start_ms).max(1) as f64;
    let f = ((now - cell.start_ms) as f64 / span).clamp(0.0, 1.0);
    c.rect(x, y, w, 3, mix(look.palette.cell, [0, 0, 0], 0.5));
    c.rect(x, y, (w as f64 * f) as i32, 3, look.palette.accent);
}

fn grid_row(
    c: &mut Canvas,
    look: &GuideLook,
    g: &Geometry,
    data: &GuideData,
    row: &GuideRow,
    y: i32,
    alt: bool,
) {
    let p = &look.palette;
    label(c, look, g, row, y);
    let h = g.row_h - 2;
    let base = if alt { p.cell_alt } else { p.cell };
    let (gx, gw) = (g.gx(), g.gw());
    c.rect(gx, y, gw, h, p.empty);
    let max_lines = (((h - 6) / 11).max(1) as usize).min(3);
    for cell in &row.cells {
        let (s, e) = (
            cell.start_ms.max(data.window_start_ms),
            cell.end_ms.min(data.window_start_ms + data.window_ms),
        );
        if e <= s {
            continue;
        }
        let (x0, x1) = (x_of(data, g, s), x_of(data, g, e));
        let w = x1 - x0 - 2;
        if w < 10 {
            continue;
        }
        let now = airing(cell, data.now_ms);
        c.rect(x0 + 1, y, w, h, if now { p.cell_now } else { base });
        if now {
            c.rect(x0 + 1, y, w, 2, p.accent);
            if look.progress && h >= 24 {
                progress_bar(c, look, x0 + 1, y + h - 3, w, cell, data.now_ms);
            }
        }
        // Too narrow for readable text (very short programmes): just a block.
        if w < 36 {
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
        let col = if now { p.text_now } else { p.text };
        for (i, line) in fit_lines(&title, chars, max_lines).iter().enumerate() {
            c.text(x0 + 5, y + 6 + i as i32 * 11, line, 1, col);
        }
    }
}

/// One big row: what is on now, and what is next.
fn now_next_row(
    c: &mut Canvas,
    look: &GuideLook,
    g: &Geometry,
    data: &GuideData,
    row: &GuideRow,
    y: i32,
    alt: bool,
) {
    let p = &look.palette;
    label(c, look, g, row, y);
    let h = g.row_h - 2;
    let (gx, gw) = (g.gx(), g.gw());
    let now_w = gw * 62 / 100 - 2;
    let now = row.cells.iter().find(|x| airing(x, data.now_ms));
    let next = row
        .cells
        .iter()
        .filter(|x| x.start_ms >= now.map_or(data.now_ms, |n| n.end_ms))
        .min_by_key(|x| x.start_ms);
    let base = if alt { p.cell_alt } else { p.cell };
    let scale = if h >= 40 { 2 } else { 1 };
    for (cell, x, w, hot) in [
        (now, gx, now_w, true),
        (next, gx + now_w + 2, gw - now_w - 2, false),
    ] {
        c.rect(
            x,
            y,
            w,
            h,
            if hot && cell.is_some() {
                p.cell_now
            } else if cell.is_some() {
                base
            } else {
                p.empty
            },
        );
        let Some(cell) = cell else { continue };
        if hot {
            c.rect(x, y, w, 2, p.accent);
        }
        let col = if hot { p.text_now } else { p.text };
        let range_h = if cell.range.is_empty() { 0 } else { 12 };
        let room = h - 8 - range_h - if hot && look.progress { 5 } else { 0 };
        // The biggest letters the title fits in; smaller ones before cutting it short.
        let mut pick = None;
        for sc in (1..=scale).rev() {
            let (chars, lh) = (((w - 12) / (8 * sc)).max(1) as usize, sc * 10 + 3);
            let lines = ((room / lh).max(1) as usize).min(3);
            if let Some(l) = wrap(&cell.title, chars, lines) {
                pick = Some((sc, lh, l));
                break;
            }
        }
        let (scale, line_h, lines) = pick.unwrap_or_else(|| {
            let chars = ((w - 12) / 8).max(1) as usize;
            let fit = ((room / 13).max(1) as usize).min(3);
            (1, 13, fit_lines(&cell.title, chars, fit))
        });
        for (i, l) in lines.iter().enumerate() {
            c.text(x + 6, y + 6 + i as i32 * line_h, l, scale, col);
        }
        if range_h > 0 {
            let ry = y + 6 + lines.len() as i32 * line_h;
            if ry + 8 <= y + h {
                c.text(
                    x + 6,
                    ry,
                    &truncate(&cell.range, ((w - 12) / 8).max(1) as usize),
                    1,
                    if hot { p.accent } else { p.label_name },
                );
            }
        }
        if hot && look.progress {
            progress_bar(c, look, x, y + h - 3, w, cell, data.now_ms);
        }
    }
}

/// The card beside the video window: the channel and what is on it.
fn panel(c: &mut Canvas, look: &GuideLook, r: Rect, data: &GuideData, row: Option<&GuideRow>) {
    let p = &look.palette;
    c.rect(r.x, r.y, r.w, r.h, p.panel);
    c.rect(r.x, r.y, r.w, 2, p.accent);
    let Some(row) = row else { return };
    let wide = r.w > 300;
    let mut y = r.y + 7;
    let inner = r.w - 12;
    let num = row.number.to_string();
    let nscale = if wide { 4 } else { 3 };
    c.text_shadow(r.x + 6, y, &num, nscale, p.label_num);
    let nx = r.x + 6 + text_width(&num, nscale) + 8;
    let name_chars = ((r.x + r.w - nx - 6) / 8).max(1) as usize;
    c.text(nx, y + 2, &truncate(&row.name, name_chars), 1, p.label_name);
    c.text(nx, y + nscale * 8 - 8, "ON NOW", 1, p.accent);
    y += nscale * 8 + 8;
    let now = row.cells.iter().find(|x| airing(x, data.now_ms));
    let next = row
        .cells
        .iter()
        .filter(|x| x.start_ms >= now.map_or(data.now_ms, |n| n.end_ms))
        .min_by_key(|x| x.start_ms);
    let bottom = r.y + r.h - 6;
    if let Some(cell) = now {
        let room = ((bottom - y - 24) / 11).clamp(1, 3) as usize;
        // The biggest letters (up to 3x) in which the title fits whole.
        let cap = if wide { 3 } else { 2 };
        let (scale, lines) = (2..=cap)
            .rev()
            .find_map(|sc| {
                wrap(&cell.title, (inner / (8 * sc)).max(1) as usize, room).map(|l| (sc, l))
            })
            .unwrap_or_else(|| {
                (
                    2,
                    fit_lines(&cell.title, (inner / 16).max(1) as usize, room),
                )
            });
        for l in &lines {
            if y + scale * 8 > bottom {
                break;
            }
            c.text_shadow(r.x + 6, y, l, scale, p.text_now);
            y += scale * 8 + 3;
        }
        if !cell.range.is_empty() && y + 8 <= bottom {
            c.text(
                r.x + 6,
                y,
                &truncate(&cell.range, (inner / 8) as usize),
                1,
                p.accent,
            );
            y += 12;
        }
    }
    if let Some(cell) = next {
        if y + 22 <= bottom {
            c.text(r.x + 6, y + 2, "NEXT", 1, p.label_name);
            let line = truncate(&cell.title, (inner / 8) as usize);
            c.text(r.x + 6, y + 13, &line, 1, p.text);
        }
    }
}

/// Render one frame of the guide channel, `t` seconds into the stream.
pub fn render_guide(data: &GuideData, look: &GuideLook, view: &GuideView, t: f32) -> Canvas {
    let p = &look.palette;
    let g = geometry(look);
    let mut c = Canvas::new(WIDTH, HEIGHT);
    c.vgradient(p.bg_top, p.bg_bottom);
    let w = WIDTH as i32;

    // heading
    c.rect(0, 0, w, HEADER_H, p.header);
    c.rect(0, HEADER_H - 3, w, 3, p.rule);
    let mut right = w - 12;
    if look.clock && !data.clock.is_empty() {
        let cw = text_width(&data.clock, 2);
        c.text_shadow(
            w - cw - 12,
            if look.date { 5 } else { 10 },
            &data.clock,
            2,
            p.header_text,
        );
        right = w - cw - 24;
    }
    if look.date && !data.date.is_empty() {
        let dw = text_width(&data.date, 1);
        let y = if look.clock { 24 } else { 14 };
        c.text(w - dw - 12, y, &data.date, 1, p.header_sub);
        right = right.min(w - dw - 24);
    }
    if look.title {
        let title = if data.title.trim().is_empty() {
            "TV GUIDE"
        } else {
            data.title.as_str()
        };
        let x = if look.logo { 72 } else { 12 };
        let scale = if text_width(title, 2) > right - x {
            1
        } else {
            2
        };
        let chars = ((right - x) / (8 * scale)).max(1) as usize;
        c.text_shadow(
            x,
            if scale == 2 { 9 } else { 13 },
            &truncate(title, chars.min(40)),
            scale,
            p.header_text,
        );
    }

    // the video window's frame and card
    if let Some(win) = g.window {
        c.rect(
            win.x - FRAME,
            win.y - FRAME,
            win.w + 2 * FRAME,
            win.h + 2 * FRAME,
            p.accent,
        );
        c.rect(win.x, win.y, win.w, win.h, [0, 0, 0]);
        if !view.window_live {
            let msg = "NO SIGNAL";
            let sc = if win.w >= 192 { 2 } else { 1 };
            let tx = win.x + (win.w - text_width(msg, sc)) / 2;
            c.text(tx, win.y + (win.h - 8 * sc) / 2, msg, sc, [150, 150, 150]);
        }
    }
    if let Some(r) = g.panel {
        panel(
            &mut c,
            look,
            r,
            data,
            view.window_row.and_then(|i| data.rows.get(i)),
        );
    }

    // time bar
    c.rect(g.left, g.bar_y, g.area_w, BAR_H, p.bar);
    if g.label_w > 0 {
        let cap = "CHANNEL";
        c.text(
            g.left + 8,
            g.bar_y + 6,
            if g.label_w >= 100 { cap } else { "CH" },
            1,
            p.accent,
        );
    }
    if look.now_next {
        let now_w = g.gw() * 62 / 100;
        c.text(g.left + g.gx() + 6, g.bar_y + 6, "NOW", 1, p.accent);
        c.text(
            g.left + g.gx() + now_w + 6,
            g.bar_y + 6,
            "NEXT",
            1,
            p.bar_text,
        );
    } else {
        // Skip marks (keeping the first) until the labels have room.
        let widest = data
            .marks
            .iter()
            .map(|(_, t)| 8 * t.chars().count() as i32)
            .max()
            .unwrap_or(0);
        let spacing = match data.marks.as_slice() {
            [(a, _), (b, _), ..] => (x_of(data, &g, *b) - x_of(data, &g, *a)).max(1),
            _ => i32::MAX,
        };
        let every = [1usize, 2, 3, 4, 6, 12]
            .into_iter()
            .find(|k| spacing.saturating_mul(*k as i32) >= widest + 10)
            .unwrap_or(12);
        for (i, (ms, text)) in data.marks.iter().enumerate() {
            if i % every != 0 {
                continue;
            }
            let x = x_of(data, &g, *ms);
            if x >= g.gx() && x < g.gx() + g.gw() - 8 * text.chars().count() as i32 {
                c.rect(g.left + x, g.bar_y + 2, 1, BAR_H - 2, p.tick);
                c.text(g.left + x + 4, g.bar_y + 6, text, 1, p.bar_text);
            }
        }
    }

    // channel rows (scrolling when there are more than fit)
    let mut grid = Canvas::new(g.area_w as usize, g.visible * g.row_h as usize);
    grid.fill(p.bg_bottom);
    let n = data.rows.len();
    if n > 0 {
        let off = scroll_offset(n, g.visible, g.row_h, look, t);
        let first = off / g.row_h;
        for v in first..first + g.visible as i32 + 2 {
            let (virt, y) = (v as usize, v * g.row_h - off);
            if n <= g.visible && virt >= n {
                break;
            }
            let row = &data.rows[virt % n];
            if look.now_next {
                now_next_row(&mut grid, look, &g, data, row, y, virt % 2 == 1);
            } else {
                grid_row(&mut grid, look, &g, data, row, y, virt % 2 == 1);
            }
        }
    }
    c.blit(&grid, g.left, g.rows_y);

    // "now" marker
    if !look.now_next {
        let nx = x_of(data, &g, data.now_ms);
        if (g.gx()..g.gx() + g.gw()).contains(&nx) {
            c.rect(
                g.left + nx,
                g.bar_y,
                2,
                BAR_H + g.visible as i32 * g.row_h,
                p.accent,
            );
            c.rect(g.left + nx - 3, g.bar_y, 8, 4, p.accent);
        }
    }

    // ticker
    if look.ticker {
        c.rect(0, g.ticker_y, w, 26, p.ticker_bg);
        c.rect(0, g.ticker_y, w, 2, p.tick);
        if !data.ticker.is_empty() {
            let text = format!("   {}   *   ", data.ticker.to_uppercase());
            let tw = text_width(&text, 2);
            let x = w - ((t * look.ticker_speed) as i32 % (tw + w));
            c.text(x, g.ticker_y + 6, &text, 2, p.ticker_text);
        }
    }
    if look.scanlines {
        c.scanlines(0.12);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(rows: usize) -> GuideData {
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
                    id: format!("ch{r}"),
                    number: r as u32 + 2,
                    name: format!("Channel {r}"),
                    cells: vec![
                        GuideCell {
                            start_ms: start - half,
                            end_ms: start + half,
                            title: format!("Show {r} Pilot"),
                            range: "8:30-9:30 PM".into(),
                        },
                        GuideCell {
                            start_ms: start + half,
                            end_ms: start + 2 * half,
                            title: "A Very Long Programme Title That Needs Wrapping".into(),
                            range: "9:30-10:00 PM".into(),
                        },
                        GuideCell {
                            start_ms: start + 2 * half,
                            end_ms: start + 6 * half,
                            title: "Marathon".into(),
                            range: "10:00 PM-12:00 AM".into(),
                        },
                    ],
                })
                .collect(),
            ticker: "Now on channel 4: Taxi".into(),
        }
    }

    fn look() -> GuideLook {
        GuideLook::new(Palette::from_theme(&Theme::for_channel("guide")))
    }

    fn windowed(placement: Placement, size: WindowSize) -> GuideLook {
        let mut l = look();
        l.window = Some(WindowLook {
            placement,
            size,
            info: true,
        });
        l
    }

    #[test]
    fn the_guide_renders_with_any_number_of_channels_in_every_look() {
        let mut looks = vec![look()];
        let mut nn = look();
        nn.now_next = true;
        looks.push(nn);
        let mut smooth = look();
        smooth.smooth = true;
        smooth.rows = 10;
        looks.push(smooth);
        for p in [
            Placement::TopLeft,
            Placement::TopRight,
            Placement::SideLeft,
            Placement::SideRight,
        ] {
            for s in [WindowSize::Small, WindowSize::Medium, WindowSize::Large] {
                looks.push(windowed(p, s));
            }
        }
        for l in &looks {
            for n in [0, 1, 5, 6, 7, 20] {
                for t in [0.0, 3.0, 8.5, 9.4, 40.0] {
                    let view = GuideView {
                        window_row: (n > 0).then_some(n / 2),
                        window_live: t > 5.0,
                    };
                    let c = render_guide(&data(n), l, &view, t);
                    assert_eq!((c.w, c.h), (WIDTH, HEIGHT));
                }
            }
        }
    }

    #[test]
    fn the_default_look_keeps_the_original_proportions() {
        let g = geometry(&look());
        assert_eq!((g.gx(), g.gw(), g.visible_rows()), (108, 528, 6));
        assert_eq!(g.window, None);
    }

    #[test]
    fn a_video_window_never_overlaps_the_listing_or_the_ticker() {
        for p in [
            Placement::TopLeft,
            Placement::TopRight,
            Placement::SideLeft,
            Placement::SideRight,
        ] {
            for s in [WindowSize::Small, WindowSize::Medium, WindowSize::Large] {
                let l = windowed(p, s);
                let g = geometry(&l);
                let w = g.window.expect("a window");
                assert_eq!(w.w * 9, w.h * 16, "16:9");
                assert!(
                    w.x >= 0
                        && w.y >= HEADER_H
                        && w.x + w.w <= WIDTH as i32
                        && w.y + w.h < TICKER_Y,
                    "{p:?} {s:?}: {w:?}"
                );
                // The rows and the window share no pixels.
                let (rx0, rx1) = (g.left, g.left + g.area_w);
                let (rows_top, rows_bottom) = (g.rows_y, g.rows_y + g.visible as i32 * g.row_h);
                let across = w.x + w.w <= rx0 || w.x >= rx1;
                let above = w.y + w.h + FRAME <= g.bar_y;
                assert!(
                    across || above,
                    "{p:?} {s:?} window {w:?} vs rows x {rx0}..{rx1} y {rows_top}..{rows_bottom}"
                );
                assert!(
                    rows_bottom <= TICKER_Y,
                    "{p:?} {s:?}: rows run into the ticker"
                );
                if let Some(pn) = g.panel {
                    assert!(
                        pn.y + pn.h <= TICKER_Y && pn.x >= 0 && pn.x + pn.w <= WIDTH as i32,
                        "{p:?} {s:?}: {pn:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_window_shrinks_the_listing_in_the_direction_it_sits() {
        let plain = geometry(&look());
        let side = geometry(&windowed(Placement::SideRight, WindowSize::Large));
        assert!(side.gw() < plain.gw() && side.visible_rows() == plain.visible_rows());
        let top = geometry(&windowed(Placement::TopLeft, WindowSize::Large));
        assert!(
            top.visible_rows() < plain.visible_rows() && top.gw() == plain.gw(),
            "a top band takes rows, not width"
        );
    }

    #[test]
    fn turning_the_ticker_off_gives_its_room_to_the_rows() {
        let mut l = look();
        let a = geometry(&l);
        l.ticker = false;
        l.rows = 10;
        let b = geometry(&l);
        assert!(b.visible_rows() > a.visible_rows());
        assert!(b.rows_y + b.visible as i32 * b.row_h > a.rows_y + a.visible as i32 * a.row_h);
    }

    #[test]
    fn few_channels_stay_put_and_many_channels_scroll_in_pages() {
        let l = look();
        let view = GuideView::default();
        let grid_area = |c: &Canvas| c.px[(60 * WIDTH * 3)..(300 * WIDTH * 3)].to_vec();
        let few = data(4);
        assert_eq!(
            grid_area(&render_guide(&few, &l, &view, 0.0)),
            grid_area(&render_guide(&few, &l, &view, 30.0)),
            "no scrolling needed"
        );
        let many = data(14);
        let first = grid_area(&render_guide(&many, &l, &view, 1.0));
        let later = grid_area(&render_guide(&many, &l, &view, 12.0));
        assert_ne!(first, later, "moved on to the next page");
        let mid = grid_area(&render_guide(&many, &l, &view, 9.0 - 0.45));
        assert_ne!(mid, first);
        assert_ne!(mid, later);
        // it wraps: three pages of 6 -> back at the start
        assert_eq!(
            first,
            grid_area(&render_guide(&many, &l, &view, 1.0 + 3.0 * 9.0))
        );
    }

    #[test]
    fn page_length_and_row_count_are_respected() {
        let mut l = look();
        l.page_secs = 5.0;
        l.rows = 4;
        let g = geometry(&l);
        let (v, rh) = (g.visible_rows(), g.row_h);
        assert_eq!(v, 4);
        assert_eq!(scroll_offset(12, v, rh, &l, 1.0), 0);
        assert_eq!(
            scroll_offset(12, v, rh, &l, 5.0 + 1.0),
            v as i32 * rh,
            "second page after 5 s"
        );
        assert_eq!(
            scroll_offset(12, v, rh, &l, 15.0 + 1.0),
            0,
            "three pages, then round again"
        );
        assert_eq!(scroll_offset(4, v, rh, &l, 100.0), 0);
    }

    #[test]
    fn smooth_scrolling_moves_a_little_every_frame_and_wraps() {
        let mut l = look();
        l.smooth = true;
        let g = geometry(&l);
        let (v, rh) = (g.visible_rows(), g.row_h);
        let a = scroll_offset(12, v, rh, &l, 1.0);
        let b = scroll_offset(12, v, rh, &l, 1.5);
        assert!(b > a, "keeps moving");
        let lap = (12 * rh) as f32 / ROLL_SPEED;
        assert!(
            scroll_offset(12, v, rh, &l, lap + 0.1) < 3,
            "wraps round to the top"
        );
        assert_eq!(
            scroll_offset(3, v, rh, &l, 50.0),
            0,
            "a short list holds still"
        );
    }

    #[test]
    fn every_palette_is_distinct_and_readable() {
        let all = [
            Palette::prevue(),
            Palette::amber(),
            Palette::phosphor(),
            Palette::neon(),
            Palette::sunset(),
            Palette::ocean(),
            Palette::slate(),
            Palette::paper(),
            Palette::custom([30, 160, 90], [250, 240, 120]),
            Palette::from_theme(&Theme::for_channel("x")),
        ];
        let lum = |c: Rgb| 0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32;
        for (i, p) in all.iter().enumerate() {
            for q in &all[i + 1..] {
                assert_ne!(p, q);
            }
            // Text must stand out from the cell it is drawn on, now and later.
            assert!(
                (lum(p.text) - lum(p.cell)).abs() > 70.0,
                "{i}: text on cell"
            );
            assert!(
                (lum(p.text) - lum(p.cell_alt)).abs() > 70.0,
                "{i}: text on alternate cell"
            );
            assert!(
                (lum(p.text_now) - lum(p.cell_now)).abs() > 70.0,
                "{i}: text on the airing cell"
            );
            assert!(
                (lum(p.header_text) - lum(p.header)).abs() > 70.0,
                "{i}: heading"
            );
            assert!(
                (lum(p.ticker_text) - lum(p.ticker_bg)).abs() > 70.0,
                "{i}: ticker"
            );
            assert!(
                (lum(p.label_num) - lum(p.label_bg)).abs() > 70.0,
                "{i}: channel number"
            );
        }
        assert_eq!(
            Palette::from_theme(&Theme::for_channel("x")).header,
            mix(
                Theme::for_channel("x").dark,
                Theme::for_channel("x").primary,
                0.35
            )
        );
    }

    #[test]
    fn the_clock_and_airing_highlight_change_the_picture() {
        let l = look();
        let v = GuideView::default();
        let a = data(3);
        let mut b = data(3);
        b.clock = "9:11 PM".into();
        assert_ne!(
            render_guide(&a, &l, &v, 1.0).px,
            render_guide(&b, &l, &v, 1.0).px
        );
        let mut c = data(3);
        c.now_ms += 40 * 60_000; // the second programme is now airing
        assert_ne!(
            render_guide(&a, &l, &v, 1.0).px,
            render_guide(&c, &l, &v, 1.0).px
        );
    }

    #[test]
    fn each_option_changes_the_picture() {
        let d = data(8);
        let v = GuideView::default();
        let base = render_guide(&d, &look(), &v, 2.0).px;
        let changed = |f: &dyn Fn(&mut GuideLook)| {
            let mut l = look();
            f(&mut l);
            render_guide(&d, &l, &v, 2.0).px != base
        };
        assert!(changed(&|l| l.now_next = true));
        assert!(changed(&|l| l.progress = true));
        assert!(changed(&|l| l.scanlines = false));
        assert!(changed(&|l| l.numbers = false));
        assert!(changed(&|l| l.names = false));
        assert!(changed(&|l| l.title = false));
        assert!(changed(&|l| l.clock = false));
        assert!(changed(&|l| l.date = false));
        assert!(changed(&|l| l.ticker = false));
        assert!(changed(&|l| l.rows = 4));
        assert!(changed(&|l| l.ticker_speed = 140.0));
        assert!(changed(&|l| l.palette = Palette::amber()));
        assert!(changed(&|l| l.window = Some(WindowLook {
            placement: Placement::SideRight,
            size: WindowSize::Medium,
            info: true
        })));
    }

    #[test]
    fn the_window_card_names_the_channel_it_shows() {
        let d = data(5);
        let l = windowed(Placement::SideRight, WindowSize::Medium);
        let a = render_guide(
            &d,
            &l,
            &GuideView {
                window_row: Some(1),
                window_live: true,
            },
            2.0,
        );
        let b = render_guide(
            &d,
            &l,
            &GuideView {
                window_row: Some(3),
                window_live: true,
            },
            2.0,
        );
        assert_ne!(a.px, b.px);
        let off = render_guide(
            &d,
            &l,
            &GuideView {
                window_row: Some(1),
                window_live: false,
            },
            2.0,
        );
        assert_ne!(a.px, off.px, "NO SIGNAL shows while there is no video");
    }

    #[test]
    fn degenerate_guide_data_does_not_panic() {
        let l = windowed(Placement::TopLeft, WindowSize::Large);
        let v = GuideView {
            window_row: Some(0),
            window_live: false,
        };
        let mut d = data(3);
        d.window_ms = 0;
        render_guide(&d, &l, &v, 1.0);
        d.window_ms = 1;
        d.rows[0].cells[0].title = "😀".repeat(200);
        d.rows[0].name = "N".repeat(200);
        render_guide(&d, &l, &v, 1.0);
        render_guide(
            &GuideData::default(),
            &l,
            &GuideView {
                window_row: Some(9),
                window_live: true,
            },
            5.0,
        );
        let mut tiny = look();
        tiny.rows = 10;
        tiny.numbers = false;
        tiny.names = false;
        tiny.now_next = true;
        render_guide(&data(30), &tiny, &GuideView::default(), 7.0);
    }
}
