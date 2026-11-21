//! The guide channel's picture: a listing of what is on, in many looks.
//!
//! Everything is laid out on a 640x360 grid of "points". The retro looks draw
//! one point per pixel with the chunky 8x8 font and FFmpeg scales the picture
//! up; the smooth looks draw two pixels per point (1280x720) with a real
//! typeface. The caller formats times (it knows the time zone), so this crate
//! stays free of time-zone logic. A live video window (picture-in-picture) is
//! not drawn here: the picture reserves the space and [`geometry`] says where it
//! is, so FFmpeg can place the video there.

use super::{ease_out, mix, Canvas, Rgb, Theme, HEIGHT, WHITE, WIDTH};
use crate::text::{self, Face};
use std::sync::Arc;

/// One programme on the guide.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuideCell {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
    /// "9:00-9:30 PM", for the now-and-next look, the cell times and the video window's card.
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

    /// Two colours you choose; the dark background is the first one, darkened,
    /// unless you give one.
    pub fn custom(primary: Rgb, accent: Rgb) -> Self {
        Self::custom_on(primary, accent, None)
    }

    pub fn custom_on(primary: Rgb, accent: Rgb, background: Option<Rgb>) -> Self {
        let dark = background.unwrap_or([
            (primary[0] as f32 * 0.22) as u8,
            (primary[1] as f32 * 0.22) as u8,
            (primary[2] as f32 * 0.22) as u8,
        ]);
        let mut p = Self::dark(dark, primary, accent);
        if background.is_some() {
            // The listing sits on the colour chosen, all the way down.
            p.bg_bottom = mix(dark, [0, 0, 0], 0.5);
        }
        p
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
    pub fn midnight() -> Self {
        Self::dark([8, 14, 36], [52, 96, 200], [64, 220, 255])
    }
    pub fn graphite() -> Self {
        Self::dark([30, 30, 34], [96, 98, 108], [255, 150, 40])
    }
    pub fn forest() -> Self {
        Self::dark([8, 28, 18], [46, 140, 84], [255, 196, 70])
    }
    pub fn berry() -> Self {
        Self::dark([34, 10, 36], [160, 52, 150], [255, 140, 190])
    }

    /// Black and white.
    pub fn mono() -> Self {
        let mut p = Self::dark([14, 14, 14], [120, 120, 120], [255, 255, 255]);
        p.cell = [30, 30, 30];
        p.cell_alt = [22, 22, 22];
        p.cell_now = [232, 232, 232];
        p.text_now = [8, 8, 8];
        p.label_bg = [44, 44, 44];
        p.header = [28, 28, 28];
        p.panel = [34, 34, 34];
        p
    }

    /// A light palette from a background (top to bottom), a main colour, a highlight and the ink
    /// the text is set in.
    fn bright(bg_top: Rgb, bg_bottom: Rgb, primary: Rgb, accent: Rgb, ink: Rgb) -> Self {
        Self {
            bg_top,
            bg_bottom,
            header: primary,
            header_text: WHITE,
            header_sub: mix(primary, WHITE, 0.78),
            rule: accent,
            label_bg: primary,
            label_num: WHITE,
            label_name: mix(primary, WHITE, 0.82),
            bar: mix(bg_bottom, [0, 0, 0], 0.08),
            bar_text: ink,
            tick: mix(primary, bg_top, 0.35),
            empty: mix(bg_bottom, WHITE, 0.45),
            cell: WHITE,
            cell_alt: mix(WHITE, bg_top, 0.3),
            cell_now: mix(accent, WHITE, 0.5),
            text: ink,
            text_now: mix(ink, [0, 0, 0], 0.5),
            accent,
            ticker_bg: primary,
            ticker_text: WHITE,
            panel: WHITE,
        }
    }

    /// Bright and friendly: sky blue, sunshine yellow and tangerine, with big colourful cells.
    pub fn kids() -> Self {
        let mut p = Self::bright(
            [176, 224, 255],
            [138, 200, 250],
            [255, 118, 52],
            [255, 206, 0],
            [38, 48, 96],
        );
        p.label_bg = [48, 132, 232];
        p.label_name = [255, 244, 160];
        p.ticker_bg = [56, 168, 92];
        p.header_sub = [255, 240, 150];
        p
    }
    pub fn bubblegum() -> Self {
        Self::bright(
            [255, 214, 236],
            [250, 188, 222],
            [226, 62, 148],
            [120, 78, 220],
            [74, 24, 66],
        )
    }
    pub fn lavender() -> Self {
        Self::bright(
            [232, 222, 255],
            [212, 198, 250],
            [116, 84, 210],
            [255, 150, 196],
            [44, 30, 90],
        )
    }
    pub fn mint() -> Self {
        Self::bright(
            [210, 247, 230],
            [180, 236, 210],
            [18, 150, 108],
            [255, 168, 56],
            [10, 60, 44],
        )
    }
    pub fn arctic() -> Self {
        Self::bright(
            [228, 243, 252],
            [204, 228, 244],
            [14, 112, 166],
            [255, 132, 68],
            [14, 40, 64],
        )
    }
    pub fn cherry() -> Self {
        Self::dark([38, 6, 12], [196, 30, 60], [255, 192, 120])
    }
    pub fn gold() -> Self {
        Self::dark([20, 16, 6], [186, 148, 40], [255, 226, 140])
    }
    pub fn coffee() -> Self {
        Self::dark([30, 20, 14], [140, 92, 60], [244, 210, 150])
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

    /// Clean white with a blue highlight.
    pub fn snow() -> Self {
        let ink = [30, 36, 48];
        Self {
            bg_top: [246, 248, 252],
            bg_bottom: [226, 232, 242],
            header: [28, 98, 220],
            header_text: WHITE,
            header_sub: [200, 224, 255],
            rule: WHITE,
            label_bg: [28, 98, 220],
            label_num: WHITE,
            label_name: [214, 232, 255],
            bar: [214, 222, 236],
            bar_text: ink,
            tick: [120, 136, 170],
            empty: [236, 240, 248],
            cell: WHITE,
            cell_alt: [243, 246, 252],
            cell_now: [200, 222, 255],
            text: ink,
            text_now: [8, 20, 50],
            accent: [28, 98, 220],
            ticker_bg: [28, 98, 220],
            ticker_text: WHITE,
            panel: WHITE,
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

/// What is behind the listing.
#[derive(Debug, Clone, Default)]
pub enum Background {
    /// The palette's two background colours, top to bottom.
    #[default]
    Gradient,
    Solid,
    /// Faint diagonal stripes.
    Stripes,
    /// A picture already scaled to the frame's size (RGB, 3 bytes a pixel).
    Image(Arc<Vec<u8>>),
}

impl PartialEq for Background {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Background::Image(a), Background::Image(b)) => Arc::ptr_eq(a, b),
            (Background::Gradient, Background::Gradient)
            | (Background::Solid, Background::Solid)
            | (Background::Stripes, Background::Stripes) => true,
            _ => false,
        }
    }
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
    /// A real typeface at 1280x720 instead of the pixel font at 640x360.
    pub smooth_text: bool,
    /// Rounded corners on cells, labels and cards.
    pub rounded: bool,
    /// 1.0 is the usual size; the pixel font only has whole sizes.
    pub text_scale: f32,
    pub background: Background,
    /// The heading is just words over the background.
    pub header_minimal: bool,
    pub ticker_top: bool,
    /// Each programme's times under its name, when there is room.
    pub cell_times: bool,
    pub now_marker: bool,
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
            smooth_text: false,
            rounded: false,
            text_scale: 1.0,
            background: Background::Gradient,
            header_minimal: false,
            ticker_top: false,
            cell_times: false,
            now_marker: true,
        }
    }

    /// Pixels per point: 2 for the smooth typeface, 1 for the pixel font.
    pub fn k(&self) -> usize {
        if self.smooth_text {
            2
        } else {
            1
        }
    }

    /// The picture's size in pixels.
    pub fn size(&self) -> (usize, usize) {
        (WIDTH * self.k(), HEIGHT * self.k())
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
    /// The window is playing commercials only: the card says so instead of naming a channel.
    pub window_ad: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where everything goes, in points (640x360).
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
const TICKER_H: i32 = 26;
const TICKER_Y: i32 = 326;
const MIN_ROW_H: i32 = 26;
const SCROLL_SECS: f32 = 0.9;
/// Pixels per second of a smooth roll.
const ROLL_SPEED: f32 = 14.0;
const FRAME: i32 = 2;
const PAD: i32 = 6;
/// The smooth typeface's size in points for a pixel-font scale of 1 (8 points tall).
const EM: f32 = 10.4;
/// Corner radius, in points, of the rounded looks.
const RADIUS: i32 = 5;

pub fn geometry(look: &GuideLook) -> Geometry {
    let label_w = match (look.numbers, look.names) {
        (true, true) => 104,
        (true, false) => 60,
        (false, true) => 100,
        (false, false) => 0,
    };
    let (body_top, body_bottom, ticker_y) = match (look.ticker, look.ticker_top) {
        (false, _) => (HEADER_H, HEIGHT as i32 - 4, TICKER_Y),
        (true, false) => (HEADER_H, TICKER_Y - 4, TICKER_Y),
        (true, true) => (HEADER_H + TICKER_H + 2, HEIGHT as i32 - 4, HEADER_H),
    };
    let mut bar_y = body_top;
    let (mut left, mut area_w) = (0, WIDTH as i32);
    let (mut window, mut panel) = (None, None);
    if let Some(w) = look.window {
        let pw = w.size.width();
        let ph = pw * 9 / 16;
        let y = body_top + 6 + FRAME;
        match w.placement {
            Placement::SideLeft | Placement::SideRight => {
                let col = pw + 2 * FRAME + 2 * PAD;
                let x = if w.placement == Placement::SideLeft {
                    PAD + FRAME
                } else {
                    WIDTH as i32 - col + PAD + FRAME
                };
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
        ticker_y,
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

// ------------------------------------------------------------------ drawing surface

/// A canvas addressed in points, drawing either the pixel font or the smooth
/// typeface, with optional rounded corners and a clip.
struct Gfx {
    c: Canvas,
    k: i32,
    smooth: bool,
    radius: i32,
    /// Added to every x (points), to draw a column of the layout without repeating its offset.
    ox: i32,
    /// Physical pixels: x0, y0, x1, y1.
    clip: Option<[i32; 4]>,
}

impl Gfx {
    fn new(look: &GuideLook) -> Self {
        let (w, h) = look.size();
        Self {
            c: Canvas::new(w, h),
            k: look.k() as i32,
            smooth: look.smooth_text,
            radius: if look.rounded { RADIUS } else { 0 },
            ox: 0,
            clip: None,
        }
    }

    fn set_clip(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let k = self.k;
        self.clip = Some([(x + self.ox) * k, y * k, (x + self.ox + w) * k, (y + h) * k]);
    }

    fn bounds(&self) -> [i32; 4] {
        let full = [0, 0, self.c.w as i32, self.c.h as i32];
        match self.clip {
            Some(c) => [
                c[0].max(0),
                c[1].max(0),
                c[2].min(full[2]),
                c[3].min(full[3]),
            ],
            None => full,
        }
    }

    fn fill_px(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, col: Rgb) {
        let b = self.bounds();
        let (x0, x1, y0, y1) = (x0.max(b[0]), x1.min(b[2]), y0.max(b[1]), y1.min(b[3]));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let w = self.c.w;
        for y in y0..y1 {
            let row = &mut self.c.px
                [(y as usize * w + x0 as usize) * 3..(y as usize * w + x1 as usize) * 3];
            for p in row.as_chunks_mut::<3>().0 {
                *p = col;
            }
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, col: Rgb) {
        let k = self.k;
        self.fill_px(
            (x + self.ox) * k,
            y * k,
            (x + self.ox + w) * k,
            (y + h) * k,
            col,
        );
    }

    /// A rectangle with rounded corners when the look asks for them.
    fn rrect(&mut self, x: i32, y: i32, w: i32, h: i32, col: Rgb) {
        let r = (self.radius * self.k)
            .min(w * self.k / 2)
            .min(h * self.k / 2);
        if r <= 1 {
            return self.rect(x, y, w, h, col);
        }
        let k = self.k;
        let (px, py, pw, ph) = ((x + self.ox) * k, y * k, w * k, h * k);
        for row in 0..ph {
            let d = row.min(ph - 1 - row);
            let inset = if d >= r {
                0
            } else {
                let dy = (r - d) as f32 - 0.5;
                (r as f32 - ((r * r) as f32 - dy * dy).max(0.0).sqrt()).round() as i32
            };
            self.fill_px(px + inset, py + row, px + pw - inset, py + row + 1, col);
        }
    }

    fn vgradient(&mut self, top: Rgb, bottom: Rgb) {
        let (w, h) = (self.c.w, self.c.h);
        for y in 0..h {
            let col = mix(top, bottom, y as f32 / (h - 1).max(1) as f32);
            self.fill_px(0, y as i32, w as i32, y as i32 + 1, col);
        }
    }

    fn put(&mut self, x: i32, y: i32, col: Rgb, alpha: u8) {
        let b = self.bounds();
        if x < b[0] || y < b[1] || x >= b[2] || y >= b[3] || alpha == 0 {
            return;
        }
        let i = (y as usize * self.c.w + x as usize) * 3;
        let a = u32::from(alpha);
        for (d, s) in self.c.px[i..i + 3].iter_mut().zip(col) {
            *d = ((u32::from(*d) * (255 - a) + u32::from(s) * a + 127) / 255) as u8;
        }
    }

    fn scanlines(&mut self, strength: f32) {
        // Every other row of points; at two pixels a point that is every other pair.
        let k = self.k as usize;
        let keep = 1.0 - strength;
        let w = self.c.w;
        for y in 0..self.c.h {
            if (y / k) % 2 == 1 {
                for v in &mut self.c.px[y * w * 3..(y + 1) * w * 3] {
                    *v = (*v as f32 * keep) as u8;
                }
            }
        }
    }

    // ---- text

    /// The pixel font's whole-number size, or the smooth typeface's fractional one.
    fn size(&self, scale: f32) -> f32 {
        if self.smooth {
            scale
        } else {
            scale.round().max(1.0)
        }
    }

    fn face(bold: bool) -> Face {
        if bold {
            Face::Bold
        } else {
            Face::Regular
        }
    }

    /// Width of `text` in points.
    fn tw(&self, text: &str, scale: f32, bold: bool) -> i32 {
        let s = self.size(scale);
        if self.smooth {
            (text::width(Self::face(bold), text, EM * s * self.k as f32) / self.k as f32).ceil()
                as i32
        } else {
            (text.chars().count() as f32 * 8.0 * s) as i32
        }
    }

    /// Distance between lines of text at this size, in points.
    fn lh(&self, scale: f32) -> i32 {
        (11.0 * self.size(scale)).round() as i32
    }

    /// Draw `text` with its top-left at (x, y) points. The pixel font draws capitals only.
    fn text(&mut self, x: i32, y: i32, text: &str, scale: f32, col: Rgb, bold: bool) {
        let s = self.size(scale);
        let k = self.k;
        if !self.smooth {
            let block = s as i32 * k;
            let mut cx = (x + self.ox) * k;
            for ch in text.to_uppercase().chars() {
                let g = Canvas::glyph(ch);
                for (row, bits) in g.iter().enumerate() {
                    for colm in 0..8 {
                        if bits >> colm & 1 == 1 {
                            let (px, py) = (cx + colm * block, y * k + row as i32 * block);
                            self.fill_px(px, py, px + block, py + block, col);
                        }
                    }
                }
                cx += 8 * block;
            }
            return;
        }
        let px = EM * s * k as f32;
        let baseline = ((y as f32 + 7.0 * s) * k as f32).round() as i32;
        let x0 = (x + self.ox) * k;
        for (ch, pen) in text::layout(Self::face(bold), text, px) {
            let g = text::glyph(Self::face(bold), ch, px);
            let gx = x0 + pen.round() as i32 + g.left;
            let gy = baseline - g.top;
            for row in 0..g.h {
                for colm in 0..g.w {
                    let a = g.coverage[row * g.w + colm];
                    if a > 0 {
                        self.put(gx + colm as i32, gy + row as i32, col, a);
                    }
                }
            }
        }
    }

    /// A one-point shadow makes text readable on any background.
    fn text_shadow(&mut self, x: i32, y: i32, text: &str, scale: f32, col: Rgb, bold: bool) {
        let off = if self.smooth {
            1
        } else {
            self.size(scale) as i32
        };
        self.text(x + off, y + off, text, scale, [0, 0, 0], bold);
        self.text(x, y, text, scale, col, bold);
    }

    // ---- fitting text into a width

    /// Wrap `text` into at most `max_lines` lines no wider than `max_w` points;
    /// `None` when it does not fit.
    fn wrap(
        &self,
        text: &str,
        max_w: i32,
        scale: f32,
        bold: bool,
        max_lines: usize,
    ) -> Option<Vec<String>> {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in text.split_whitespace() {
            let mut word = word.to_string();
            // A word wider than the line is cut where it runs out.
            while self.tw(&word, scale, bold) > max_w && word.chars().count() > 1 {
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                }
                let mut n = 1;
                while n < word.chars().count()
                    && self.tw(&word.chars().take(n + 1).collect::<String>(), scale, bold) <= max_w
                {
                    n += 1;
                }
                lines.push(word.chars().take(n).collect());
                word = word.chars().skip(n).collect();
            }
            let joined = if cur.is_empty() {
                word.clone()
            } else {
                format!("{cur} {word}")
            };
            if !cur.is_empty() && self.tw(&joined, scale, bold) > max_w {
                lines.push(std::mem::take(&mut cur));
                cur = word;
            } else {
                cur = joined;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        (lines.len() <= max_lines).then_some(lines)
    }

    /// `text` cut to `max_w` points, ending in "..." when it is cut.
    fn truncate(&self, text: &str, max_w: i32, scale: f32, bold: bool) -> String {
        if self.tw(text, scale, bold) <= max_w {
            return text.to_string();
        }
        let mut out: String = text.to_string();
        while !out.is_empty() && self.tw(&format!("{}...", out.trim_end()), scale, bold) > max_w {
            out.pop();
        }
        format!("{}...", out.trim_end())
    }

    /// Wrap into at most `max_lines`, cutting the last line short with "..." if it must.
    fn fit(&self, text: &str, max_w: i32, scale: f32, bold: bool, max_lines: usize) -> Vec<String> {
        self.wrap(text, max_w, scale, bold, max_lines)
            .unwrap_or_else(|| {
                let mut l = self
                    .wrap(text, max_w, scale, bold, usize::MAX)
                    .unwrap_or_default();
                l.truncate(max_lines);
                if let Some(last) = l.last_mut() {
                    let more = format!("{last} ...");
                    *last = self.truncate(&more, max_w, scale, bold);
                    if !last.ends_with("...") {
                        last.push_str("...");
                    }
                }
                l
            })
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

fn airing(cell: &GuideCell, now: i64) -> bool {
    cell.start_ms <= now && now < cell.end_ms
}

fn label(c: &mut Gfx, look: &GuideLook, g: &Geometry, row: &GuideRow, y: i32) {
    if g.label_w == 0 {
        return;
    }
    let p = &look.palette;
    let h = g.row_h - 2;
    let ts = look.text_scale;
    c.rrect(0, y, g.label_w, h, p.label_bg);
    let tall = h >= 38;
    let name_w = g.label_w - 12;
    if look.numbers {
        let scale = if tall { 3.0 } else { 2.0 };
        let ny = if tall { y + 4 } else { y + (h - 16) / 2 };
        let num = row.number.to_string();
        c.text_shadow(8, ny, &num, scale, p.label_num, true);
        if look.names && !tall {
            let used = c.tw(&num, scale, true) + 12;
            let room = g.label_w - used - 4;
            if room >= 24 {
                let name = c.truncate(&row.name, room, ts, false);
                c.text(used, y + (h - 8) / 2, &name, ts, p.label_name, false);
            }
        } else if look.names {
            let name = c.truncate(&row.name, name_w, ts, false);
            c.text(8, y + h - 12, &name, ts, p.label_name, false);
        }
    } else if look.names {
        let lines = c.fit(
            &row.name,
            name_w,
            ts,
            false,
            ((h - 4) / c.lh(ts)).max(1) as usize,
        );
        for (i, l) in lines.iter().enumerate() {
            c.text(6, y + 5 + i as i32 * c.lh(ts), l, ts, p.label_name, false);
        }
    }
}

fn progress_bar(c: &mut Gfx, look: &GuideLook, x: i32, y: i32, w: i32, cell: &GuideCell, now: i64) {
    let span = (cell.end_ms - cell.start_ms).max(1) as f64;
    let f = ((now - cell.start_ms) as f64 / span).clamp(0.0, 1.0);
    c.rect(x, y, w, 3, mix(look.palette.cell, [0, 0, 0], 0.5));
    c.rect(x, y, (w as f64 * f) as i32, 3, look.palette.accent);
}

fn grid_row(
    c: &mut Gfx,
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
    let ts = look.text_scale;
    let lh = c.lh(ts);
    c.rrect(gx, y, gw, h, p.empty);
    let max_lines = (((h - 6) / lh).max(1) as usize).min(4);
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
        c.rrect(x0 + 1, y, w, h, if now { p.cell_now } else { base });
        if now {
            c.rect(x0 + 1 + c.radius, y, w - 2 * c.radius, 2, p.accent);
            if look.progress && h >= 24 {
                progress_bar(
                    c,
                    look,
                    x0 + 1 + c.radius,
                    y + h - 3,
                    w - 2 * c.radius,
                    cell,
                    data.now_ms,
                );
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
        let col = if now { p.text_now } else { p.text };
        let show_times = look.cell_times && !cell.range.is_empty() && max_lines >= 2 && w >= 60;
        let title_lines = if show_times { max_lines - 1 } else { max_lines };
        let pad = 5 + c.radius / 2;
        let bold = now && look.smooth_text;
        let lines = c.fit(&title, w - pad - 4, ts, bold, title_lines);
        for (i, line) in lines.iter().enumerate() {
            c.text(x0 + pad, y + 6 + i as i32 * lh, line, ts, col, bold);
        }
        if show_times {
            let dim = mix(col, if now { p.cell_now } else { base }, 0.35);
            let t = c.truncate(&cell.range, w - pad - 4, ts * 0.85, false);
            c.text(
                x0 + pad,
                y + 6 + lines.len() as i32 * lh,
                &t,
                ts * 0.85,
                dim,
                false,
            );
        }
    }
}

/// One big row: what is on now, and what is next.
fn now_next_row(
    c: &mut Gfx,
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
    let ts = look.text_scale;
    let big = if h >= 40 { 2.0 * ts } else { ts };
    for (cell, x, w, hot) in [
        (now, gx, now_w, true),
        (next, gx + now_w + 2, gw - now_w - 2, false),
    ] {
        c.rrect(
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
            c.rect(x + c.radius, y, w - 2 * c.radius, 2, p.accent);
        }
        let col = if hot { p.text_now } else { p.text };
        let range_h = if cell.range.is_empty() {
            0
        } else {
            c.lh(ts) + 1
        };
        let room = h - 8 - range_h - if hot && look.progress { 5 } else { 0 };
        let pad = 6 + c.radius / 2;
        let bold = hot && look.smooth_text;
        // The biggest letters the title fits in; smaller ones before cutting it short.
        let mut pick = None;
        let mut sc = big;
        while sc >= ts - 0.01 {
            let lines = ((room / c.lh(sc)).max(1) as usize).min(3);
            if let Some(l) = c.wrap(&cell.title, w - 2 * pad, sc, bold, lines) {
                pick = Some((sc, l));
                break;
            }
            sc -= if look.smooth_text { 0.25 } else { 1.0 };
        }
        let (sc, lines) = pick.unwrap_or_else(|| {
            let fit = ((room / c.lh(ts)).max(1) as usize).min(3);
            (ts, c.fit(&cell.title, w - 2 * pad, ts, bold, fit))
        });
        let lh = c.lh(sc);
        for (i, l) in lines.iter().enumerate() {
            c.text(x + pad, y + 6 + i as i32 * lh, l, sc, col, bold);
        }
        if range_h > 0 {
            let ry = y + 6 + lines.len() as i32 * lh;
            if ry + c.lh(ts) - 2 <= y + h {
                let r = c.truncate(&cell.range, w - 2 * pad, ts, false);
                c.text(
                    x + pad,
                    ry,
                    &r,
                    ts,
                    if hot { p.accent } else { p.label_name },
                    false,
                );
            }
        }
        if hot && look.progress {
            progress_bar(
                c,
                look,
                x + c.radius,
                y + h - 3,
                w - 2 * c.radius,
                cell,
                data.now_ms,
            );
        }
    }
}

/// The card beside the video window: the channel and what is on it.
fn panel(
    c: &mut Gfx,
    look: &GuideLook,
    r: Rect,
    data: &GuideData,
    row: Option<&GuideRow>,
    ad: bool,
) {
    let p = &look.palette;
    c.rrect(r.x, r.y, r.w, r.h, p.panel);
    c.rect(r.x + c.radius, r.y, r.w - 2 * c.radius, 2, p.accent);
    if ad {
        // A window of commercials: no channel to name, so say what it is.
        let ts = look.text_scale;
        let inner = r.w - 12;
        c.text_shadow(r.x + 6, r.y + 10, "AD BREAK", 3.0, p.label_num, true);
        let line = c.truncate("Commercials, one after another", inner, ts, false);
        c.text(
            r.x + 6,
            r.y + 10 + c.lh(3.0) + 6,
            &line,
            ts,
            p.text_now,
            false,
        );
        let line = c.truncate("Back to your show in a moment", inner, ts, false);
        c.text(
            r.x + 6,
            r.y + 10 + c.lh(3.0) + 6 + c.lh(ts) + 2,
            &line,
            ts,
            p.accent,
            false,
        );
        return;
    }
    let Some(row) = row else { return };
    let wide = r.w > 300;
    let ts = look.text_scale;
    let mut y = r.y + 7;
    let inner = r.w - 12;
    let num = row.number.to_string();
    let nscale = if wide { 4.0 } else { 3.0 };
    c.text_shadow(r.x + 6, y, &num, nscale, p.label_num, true);
    let nx = r.x + 6 + c.tw(&num, nscale, true) + 8;
    let name = c.truncate(&row.name, r.x + r.w - nx - 6, ts, false);
    c.text(nx, y + 2, &name, ts, p.label_name, false);
    c.text(nx, y + nscale as i32 * 8 - 8, "ON NOW", ts, p.accent, false);
    y += nscale as i32 * 8 + 8;
    let now = row.cells.iter().find(|x| airing(x, data.now_ms));
    let next = row
        .cells
        .iter()
        .filter(|x| x.start_ms >= now.map_or(data.now_ms, |n| n.end_ms))
        .min_by_key(|x| x.start_ms);
    let bottom = r.y + r.h - 6;
    if let Some(cell) = now {
        let room = ((bottom - y - 24) / c.lh(2.0)).clamp(1, 3) as usize;
        // The biggest letters (up to 3x) in which the title fits whole.
        let cap = if wide { 3.0 } else { 2.0 };
        let mut sc = cap;
        let mut pick = None;
        while sc >= 2.0 - 0.01 || (c.smooth && sc >= 1.5) {
            if let Some(l) = c.wrap(&cell.title, inner, sc, true, room) {
                pick = Some((sc, l));
                break;
            }
            sc -= if c.smooth { 0.25 } else { 1.0 };
        }
        let (scale, lines) =
            pick.unwrap_or_else(|| (2.0, c.fit(&cell.title, inner, 2.0, true, room)));
        for l in &lines {
            if y + 8 * c.size(scale) as i32 > bottom {
                break;
            }
            c.text_shadow(r.x + 6, y, l, scale, p.text_now, true);
            y += c.lh(scale) - 2 + 3;
        }
        if !cell.range.is_empty() && y + 8 <= bottom {
            let t = c.truncate(&cell.range, inner, ts, false);
            c.text(r.x + 6, y, &t, ts, p.accent, false);
            y += c.lh(ts) + 1;
        }
    }
    if let Some(cell) = next {
        if y + 22 <= bottom {
            c.text(r.x + 6, y + 2, "NEXT", ts, p.label_name, false);
            let line = c.truncate(&cell.title, inner, ts, false);
            c.text(r.x + 6, y + 13, &line, ts, p.text, false);
        }
    }
}

fn background(c: &mut Gfx, look: &GuideLook) {
    let p = &look.palette;
    match &look.background {
        Background::Image(img) if img.len() == c.c.px.len() => c.c.px.copy_from_slice(img),
        Background::Solid => c.c.fill(p.bg_top),
        Background::Stripes => {
            c.vgradient(p.bg_top, p.bg_bottom);
            let band = 10 * c.k as usize;
            let w = c.c.w;
            for y in 0..c.c.h {
                for x in 0..w {
                    if ((x + y) / band).is_multiple_of(2) {
                        let i = (y * w + x) * 3;
                        for v in &mut c.c.px[i..i + 3] {
                            *v = v.saturating_add(5);
                        }
                    }
                }
            }
        }
        _ => c.vgradient(p.bg_top, p.bg_bottom),
    }
}

/// The colour for words that sit straight on the background.
fn ink_on_background(look: &GuideLook) -> Rgb {
    if matches!(look.background, Background::Image(_)) {
        WHITE
    } else if lum(look.palette.bg_top) > 140.0 {
        [20, 24, 34]
    } else {
        WHITE
    }
}

/// Render one frame of the guide channel, `t` seconds into the stream.
pub fn render_guide(data: &GuideData, look: &GuideLook, view: &GuideView, t: f32) -> Canvas {
    let p = &look.palette;
    let g = geometry(look);
    let mut c = Gfx::new(look);
    background(&mut c, look);
    let w = WIDTH as i32;
    let smooth = look.smooth_text;

    // heading
    let (ink, sub) = if look.header_minimal {
        (ink_on_background(look), p.accent)
    } else {
        (p.header_text, p.header_sub)
    };
    if look.header_minimal {
        c.rect(0, HEADER_H - 3, w, 2, p.rule);
    } else {
        c.rect(0, 0, w, HEADER_H, p.header);
        c.rect(0, HEADER_H - 3, w, 3, p.rule);
    }
    let mut right = w - 12;
    if look.clock && !data.clock.is_empty() {
        let cw = c.tw(&data.clock, 2.0, true);
        c.text_shadow(
            w - cw - 12,
            if look.date { 5 } else { 10 },
            &data.clock,
            2.0,
            ink,
            true,
        );
        right = w - cw - 24;
    }
    if look.date && !data.date.is_empty() {
        let dw = c.tw(&data.date, 1.0, false);
        let y = if look.clock { 24 } else { 14 };
        c.text(w - dw - 12, y, &data.date, 1.0, sub, false);
        right = right.min(w - dw - 24);
    }
    if look.title {
        let title = if data.title.trim().is_empty() {
            "TV GUIDE"
        } else {
            data.title.as_str()
        };
        let x = if look.logo { 72 } else { 12 };
        let scale = if c.tw(title, 2.0, true) > right - x {
            1.0
        } else {
            2.0
        };
        let shown = c.truncate(title, right - x, scale, true);
        c.text_shadow(
            x,
            if scale == 2.0 { 9 } else { 13 },
            &shown,
            scale,
            ink,
            true,
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
            let sc = if win.w >= 192 { 2.0 } else { 1.0 };
            let tx = win.x + (win.w - c.tw(msg, sc, true)) / 2;
            c.text(
                tx,
                win.y + (win.h - 8 * c.size(sc) as i32) / 2,
                msg,
                sc,
                [150, 150, 150],
                true,
            );
        }
    }
    if let Some(r) = g.panel {
        panel(
            &mut c,
            look,
            r,
            data,
            view.window_row.and_then(|i| data.rows.get(i)),
            view.window_ad,
        );
    }

    // time bar
    c.ox = g.left;
    c.rrect(0, g.bar_y, g.area_w, BAR_H, p.bar);
    if g.label_w > 0 {
        let cap = if g.label_w >= 100 { "CHANNEL" } else { "CH" };
        c.text(8, g.bar_y + 6, cap, 1.0, p.accent, true);
    }
    if look.now_next {
        let now_w = g.gw() * 62 / 100;
        c.text(g.gx() + 6, g.bar_y + 6, "NOW", 1.0, p.accent, true);
        c.text(
            g.gx() + now_w + 6,
            g.bar_y + 6,
            "NEXT",
            1.0,
            p.bar_text,
            true,
        );
    } else {
        // Skip marks (keeping the first) until the labels have room.
        let widest = data
            .marks
            .iter()
            .map(|(_, t)| c.tw(t, 1.0, false))
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
            if x >= g.gx() && x < g.gx() + g.gw() - c.tw(text, 1.0, false) {
                c.rect(x, g.bar_y + 2, 1, BAR_H - 2, p.tick);
                c.text(x + 4, g.bar_y + 6, text, 1.0, p.bar_text, false);
            }
        }
    }

    // channel rows (scrolling when there are more than fit), clipped to their area
    c.set_clip(0, g.rows_y, g.area_w, g.visible as i32 * g.row_h);
    let n = data.rows.len();
    if n > 0 {
        let off = scroll_offset(n, g.visible, g.row_h, look, t);
        let first = off / g.row_h;
        for v in first..first + g.visible as i32 + 2 {
            let (virt, y) = (v as usize, g.rows_y + v * g.row_h - off);
            if n <= g.visible && virt >= n {
                break;
            }
            let row = &data.rows[virt % n];
            if look.now_next {
                now_next_row(&mut c, look, &g, data, row, y, virt % 2 == 1);
            } else {
                grid_row(&mut c, look, &g, data, row, y, virt % 2 == 1);
            }
        }
    }
    c.clip = None;

    // "now" marker
    if !look.now_next && look.now_marker {
        let nx = x_of(data, &g, data.now_ms);
        if (g.gx()..g.gx() + g.gw()).contains(&nx) {
            c.rect(nx, g.bar_y, 2, BAR_H + g.visible as i32 * g.row_h, p.accent);
            c.rect(nx - 3, g.bar_y, 8, 4, p.accent);
        }
    }
    c.ox = 0;

    // ticker
    if look.ticker {
        c.rect(0, g.ticker_y, w, TICKER_H, p.ticker_bg);
        let rule_y = if look.ticker_top {
            g.ticker_y + TICKER_H - 2
        } else {
            g.ticker_y
        };
        c.rect(0, rule_y, w, 2, p.tick);
        if !data.ticker.is_empty() {
            let line = if smooth {
                data.ticker.clone()
            } else {
                data.ticker.to_uppercase()
            };
            let text = format!("   {line}   *   ");
            let tw = c.tw(&text, 2.0, false);
            let x = w - ((t * look.ticker_speed) as i32 % (tw + w));
            c.set_clip(0, g.ticker_y + 2, w, TICKER_H - 3);
            c.text(x, g.ticker_y + 6, &text, 2.0, p.ticker_text, smooth);
            c.clip = None;
        }
    }
    if look.scanlines {
        c.scanlines(0.12);
    }
    c.c
}

/// Rough brightness (0-255).
fn lum(c: Rgb) -> f32 {
    0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32
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

    fn modern() -> GuideLook {
        let mut l = GuideLook::new(Palette::midnight());
        l.smooth_text = true;
        l.rounded = true;
        l.scanlines = false;
        l
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

    fn every_look() -> Vec<GuideLook> {
        let mut looks = vec![look(), modern()];
        let mut nn = look();
        nn.now_next = true;
        looks.push(nn);
        let mut smooth = look();
        smooth.smooth = true;
        smooth.rows = 10;
        looks.push(smooth);
        let mut m = modern();
        m.now_next = true;
        m.cell_times = true;
        m.header_minimal = true;
        m.ticker_top = true;
        m.background = Background::Stripes;
        looks.push(m);
        for p in [
            Placement::TopLeft,
            Placement::TopRight,
            Placement::SideLeft,
            Placement::SideRight,
        ] {
            for s in [WindowSize::Small, WindowSize::Medium, WindowSize::Large] {
                looks.push(windowed(p, s));
                let mut m = modern();
                m.window = Some(WindowLook {
                    placement: p,
                    size: s,
                    info: true,
                });
                looks.push(m);
            }
        }
        looks
    }

    #[test]
    fn the_guide_renders_with_any_number_of_channels_in_every_look() {
        for l in &every_look() {
            let (w, h) = l.size();
            for n in [0, 1, 5, 6, 7, 20] {
                for t in [0.0, 3.0, 8.5, 9.4, 40.0] {
                    let view = GuideView {
                        window_row: (n > 0).then_some(n / 2),
                        window_live: t > 5.0,
                        window_ad: false,
                    };
                    let c = render_guide(&data(n), l, &view, t);
                    assert_eq!((c.w, c.h), (w, h));
                }
            }
        }
    }

    #[test]
    fn the_smooth_looks_draw_at_twice_the_resolution() {
        assert_eq!(look().size(), (640, 360));
        assert_eq!(modern().size(), (1280, 720));
        let c = render_guide(&data(4), &modern(), &GuideView::default(), 1.0);
        assert_eq!(c.px.len(), 1280 * 720 * 3);
    }

    #[test]
    fn the_default_look_keeps_the_original_proportions() {
        let g = geometry(&look());
        assert_eq!((g.gx(), g.gw(), g.visible_rows()), (108, 528, 6));
        assert_eq!(g.window, None);
    }

    #[test]
    fn a_video_window_never_overlaps_the_listing_or_the_ticker() {
        for top_ticker in [false, true] {
            for p in [
                Placement::TopLeft,
                Placement::TopRight,
                Placement::SideLeft,
                Placement::SideRight,
            ] {
                for s in [WindowSize::Small, WindowSize::Medium, WindowSize::Large] {
                    let mut l = windowed(p, s);
                    l.ticker_top = top_ticker;
                    let g = geometry(&l);
                    let w = g.window.expect("a window");
                    assert_eq!(w.w * 9, w.h * 16, "16:9");
                    let (tick_top, tick_bottom) = if top_ticker {
                        (HEADER_H, HEADER_H + TICKER_H)
                    } else {
                        (TICKER_Y, TICKER_Y + TICKER_H)
                    };
                    assert!(
                        w.x >= 0
                            && w.x + w.w <= WIDTH as i32
                            && w.y >= HEADER_H
                            && w.y + w.h < HEIGHT as i32,
                        "{p:?} {s:?}: {w:?}"
                    );
                    assert!(
                        w.y >= tick_bottom || w.y + w.h <= tick_top,
                        "{p:?} {s:?}: the window meets the ticker"
                    );
                    let (rx0, rx1) = (g.left, g.left + g.area_w);
                    let rows_bottom = g.rows_y + g.visible as i32 * g.row_h;
                    let across = w.x + w.w <= rx0 || w.x >= rx1;
                    let above = w.y + w.h + FRAME <= g.bar_y;
                    assert!(
                        across || above,
                        "{p:?} {s:?} window {w:?} vs rows x {rx0}..{rx1}"
                    );
                    assert!(
                        g.rows_y >= tick_bottom || rows_bottom <= tick_top,
                        "{p:?} {s:?}: rows meet the ticker"
                    );
                    if let Some(pn) = g.panel {
                        assert!(
                            pn.x >= 0
                                && pn.x + pn.w <= WIDTH as i32
                                && pn.y + pn.h <= HEIGHT as i32,
                            "{p:?} {s:?}: {pn:?}"
                        );
                    }
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
    fn a_ticker_at_the_top_moves_the_listing_down() {
        let mut l = look();
        let bottom = geometry(&l);
        l.ticker_top = true;
        let top = geometry(&l);
        assert_eq!(top.ticker_y, HEADER_H);
        assert!(top.bar_y > bottom.bar_y);
        let c = render_guide(&data(3), &l, &GuideView::default(), 1.0);
        assert_eq!(c.w, 640);
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

    fn all_palettes() -> Vec<Palette> {
        vec![
            Palette::prevue(),
            Palette::amber(),
            Palette::phosphor(),
            Palette::neon(),
            Palette::sunset(),
            Palette::ocean(),
            Palette::slate(),
            Palette::paper(),
            Palette::midnight(),
            Palette::graphite(),
            Palette::snow(),
            Palette::forest(),
            Palette::berry(),
            Palette::mono(),
            Palette::kids(),
            Palette::bubblegum(),
            Palette::lavender(),
            Palette::mint(),
            Palette::arctic(),
            Palette::cherry(),
            Palette::gold(),
            Palette::coffee(),
            Palette::custom([30, 160, 90], [250, 240, 120]),
            Palette::custom_on([30, 160, 90], [250, 240, 120], Some([20, 20, 60])),
            Palette::from_theme(&Theme::for_channel("x")),
        ]
    }

    #[test]
    fn every_palette_is_distinct_and_readable() {
        let all = all_palettes();
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
            assert!(
                (lum(p.label_name) - lum(p.label_bg)).abs() > 60.0,
                "{i}: channel name"
            );
            assert!(
                (lum(p.bar_text) - lum(p.bar)).abs() > 70.0,
                "{i}: time labels"
            );
        }
        let th = Theme::for_channel("x");
        assert_eq!(
            Palette::from_theme(&th).header,
            mix(th.dark, th.primary, 0.35)
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
        for base_look in [look(), modern()] {
            let base = render_guide(&d, &base_look, &v, 2.0).px;
            let changed = |f: &dyn Fn(&mut GuideLook)| {
                let mut l = base_look.clone();
                f(&mut l);
                render_guide(&d, &l, &v, 2.0).px != base
            };
            let smooth = base_look.smooth_text;
            assert!(changed(&|l| l.now_next = true));
            assert!(changed(&|l| l.progress = true));
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
            assert!(changed(&|l| l.cell_times = true));
            assert!(changed(&|l| l.now_marker = false));
            assert!(changed(&|l| l.header_minimal = true));
            assert!(changed(&|l| l.ticker_top = true));
            assert!(changed(&|l| l.background = Background::Solid));
            assert!(changed(&|l| l.background = Background::Stripes));
            assert!(changed(&|l| l.text_scale = 1.5));
            assert!(changed(&|l| l.smooth_text = !smooth));
            assert!(changed(&|l| l.scanlines = !l.scanlines));
            assert!(changed(&|l| l.rounded = !l.rounded));
        }
    }

    #[test]
    fn a_picture_background_shows_through_and_a_wrong_sized_one_is_ignored() {
        let d = data(3);
        let v = GuideView::default();
        let mut l = modern();
        let (w, h) = l.size();
        let img = Arc::new(vec![200u8; w * h * 3]);
        l.background = Background::Image(img);
        let c = render_guide(&d, &l, &v, 1.0);
        let at = |x: usize, y: usize| c.px[(y * w + x) * 3];
        assert_eq!(at(w - 4, 600), 200, "the picture is behind the listing");
        l.background = Background::Image(Arc::new(vec![9u8; 10]));
        let c = render_guide(&d, &l, &v, 1.0);
        assert_ne!(
            c.px[(600 * w + w - 4) * 3],
            9,
            "a picture of the wrong size is not used"
        );
    }

    #[test]
    fn rounded_corners_cut_the_corners_and_nothing_else() {
        let mut round = look();
        round.rounded = true;
        let (mut a, mut b) = (Gfx::new(&look()), Gfx::new(&round));
        a.rect(10, 10, 40, 20, [255, 255, 255]);
        b.rrect(10, 10, 40, 20, [255, 255, 255]);
        let at = |g: &Gfx, x: usize, y: usize| g.c.px[(y * 640 + x) * 3];
        assert_eq!(at(&a, 10, 10), 255);
        assert_eq!(at(&b, 10, 10), 0, "the corner is cut");
        assert_eq!(at(&b, 30, 20), 255, "the middle is not");
        assert_eq!(at(&b, 30, 10), 255, "nor is the middle of an edge");
    }

    #[test]
    fn text_is_clipped_to_its_area() {
        let mut g = Gfx::new(&modern());
        g.set_clip(0, 100, 640, 20);
        g.text(10, 90, "CLIPPED TEXT", 3.0, [255, 255, 255], true);
        for y in 0..200 {
            let lit = (0..1280).any(|x| g.c.px[(y * 1280 + x) * 3] > 0);
            assert!(!lit, "row {y} is outside the clip");
        }
        assert!(
            (200..240).any(|y| (0..1280).any(|x| g.c.px[(y * 1280 + x) * 3] > 0)),
            "something drew inside it"
        );
    }

    #[test]
    fn titles_wrap_and_truncate_by_their_real_width() {
        for l in [look(), modern()] {
            let g = Gfx::new(&l);
            let lines = g.fit(
                "The Adventures of Brisco County Jr and Friends",
                120,
                1.0,
                false,
                3,
            );
            assert!(lines.len() <= 3);
            for line in &lines {
                assert!(g.tw(line, 1.0, false) <= 120, "{line:?}");
            }
            assert!(g.tw(&g.truncate(&"W".repeat(60), 100, 1.0, false), 1.0, false) <= 100);
            assert_eq!(g.truncate("Taxi", 200, 1.0, false), "Taxi");
            assert!(
                g.wrap("Supercalifragilisticexpialidocious", 60, 1.0, false, 8)
                    .is_some(),
                "a long word is cut, not lost"
            );
        }
        // the smooth typeface fits more words on a line than the pixel font
        let (p, s) = (Gfx::new(&look()), Gfx::new(&modern()));
        assert!(
            s.tw("A Very Long Programme Title", 1.0, false)
                < p.tw("A Very Long Programme Title", 1.0, false)
        );
    }

    #[test]
    fn the_window_card_names_the_channel_it_shows() {
        for base in [look(), modern()] {
            let d = data(5);
            let mut l = base;
            l.window = Some(WindowLook {
                placement: Placement::SideRight,
                size: WindowSize::Medium,
                info: true,
            });
            let a = render_guide(
                &d,
                &l,
                &GuideView {
                    window_row: Some(1),
                    window_live: true,
                    window_ad: false,
                },
                2.0,
            );
            let b = render_guide(
                &d,
                &l,
                &GuideView {
                    window_row: Some(3),
                    window_live: true,
                    window_ad: false,
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
                    window_ad: false,
                },
                2.0,
            );
            assert_ne!(a.px, off.px, "NO SIGNAL shows while there is no video");
        }
    }

    #[test]
    fn degenerate_guide_data_does_not_panic() {
        for base in [look(), modern()] {
            let mut l = base.clone();
            l.window = Some(WindowLook {
                placement: Placement::TopLeft,
                size: WindowSize::Large,
                info: true,
            });
            let v = GuideView {
                window_row: Some(0),
                window_live: false,
                window_ad: false,
            };
            let mut d = data(3);
            d.window_ms = 0;
            render_guide(&d, &l, &v, 1.0);
            d.window_ms = 1;
            d.rows[0].cells[0].title = "😀日本語".repeat(60);
            d.rows[0].name = "N".repeat(200);
            d.ticker = "é".repeat(500);
            render_guide(&d, &l, &v, 1.0);
            render_guide(
                &GuideData::default(),
                &l,
                &GuideView {
                    window_row: Some(9),
                    window_live: true,
                    window_ad: false,
                },
                5.0,
            );
            let mut tiny = base;
            tiny.rows = 10;
            tiny.numbers = false;
            tiny.names = false;
            tiny.now_next = true;
            tiny.cell_times = true;
            render_guide(&data(30), &tiny, &GuideView::default(), 7.0);
        }
    }
}
