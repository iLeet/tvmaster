//! Info channels: pages of announcements, headlines, what is on tonight, the time and pictures,
//! one after another over the station's look, with a clock and a ticker along the edges. Drawn at
//! 1280x720 in smooth type, in whatever theme the station or the channel uses.

use crate::alert::{centred, draw, rect, wrap};
use crate::modern::{arrive, backdrop, block, faded, fit, header};
use crate::text::{self, Face};
use crate::{mix, Canvas, Rgb, Theme};
use std::sync::Arc;

const W: i32 = 1280;
const H: i32 = 720;
const TICKER_H: i32 = 64;
const FADE: f32 = 0.35;

/// A picture already scaled to the size it is drawn at (RGB, three bytes a pixel).
#[derive(Debug, Clone)]
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub rgb: Arc<Vec<u8>>,
}

/// The size a picture is prepared at on a page that has text beside it, and at full screen.
pub const SIDE_PICTURE: (usize, usize) = (456, 390);
pub const FULL_PICTURE: (usize, usize) = (1280, 720);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// A short tag in a coloured chip: a channel number, a position.
    pub tag: String,
    pub text: String,
    /// A smaller line under it.
    pub sub: String,
}

#[derive(Debug, Clone)]
pub enum InfoPage {
    Text {
        title: String,
        lines: Vec<String>,
        picture: Option<Picture>,
    },
    List {
        title: String,
        items: Vec<ListItem>,
    },
    Clock,
    Photo {
        picture: Picture,
        caption: String,
    },
}

#[derive(Debug, Clone)]
pub struct TimedPage {
    pub page: InfoPage,
    pub secs: f32,
}

pub struct InfoFrame<'a> {
    /// The heading at the top left.
    pub heading: &'a str,
    /// "9:04 PM" and "SUN NOV 15".
    pub clock: &'a str,
    pub date: &'a str,
    pub show_clock: bool,
    /// Text that scrolls along the bottom (nothing = no ticker).
    pub ticker: &'a str,
    pub pages: &'a [TimedPage],
}

/// Which page is up `t` seconds in, and how far into it we are.
pub fn page_at(pages: &[TimedPage], t: f32) -> Option<(usize, f32)> {
    let total: f32 = pages.iter().map(|p| p.secs.max(1.0)).sum();
    if pages.is_empty() || total <= 0.0 {
        return None;
    }
    let mut at = t.rem_euclid(total);
    for (i, p) in pages.iter().enumerate() {
        let s = p.secs.max(1.0);
        if at < s {
            return Some((i, at));
        }
        at -= s;
    }
    Some((pages.len() - 1, 0.0))
}

fn blit(c: &mut Canvas, p: &Picture, x: i32, y: i32) {
    for row in 0..p.h as i32 {
        let yy = y + row;
        if yy < 0 || yy >= c.h as i32 {
            continue;
        }
        let x0 = x.max(0);
        let x1 = (x + p.w as i32).min(c.w as i32);
        if x1 <= x0 {
            continue;
        }
        let src = (row as usize * p.w + (x0 - x) as usize) * 3;
        let dst = (yy as usize * c.w + x0 as usize) * 3;
        let n = (x1 - x0) as usize * 3;
        if src + n <= p.rgb.len() {
            c.px[dst..dst + n].copy_from_slice(&p.rgb[src..src + n]);
        }
    }
}

fn dim_toward(c: &mut Canvas, col: Rgb, k: f32) {
    if k <= 0.0 {
        return;
    }
    for p in c.px.as_chunks_mut::<3>().0 {
        for (d, s) in p.iter_mut().zip(col) {
            *d = (*d as f32 + (s as f32 - *d as f32) * k) as u8;
        }
    }
}

fn ticker_strip(c: &mut Canvas, th: &Theme, text: &str, t: f32) {
    let y = H - TICKER_H;
    rect(c, 0, y, W, TICKER_H, th.dark);
    rect(c, 0, y, W, 4, th.accent);
    let line = format!("{}        •        ", text.replace('\n', " "));
    let w = (text::width(Face::Bold, &line, 30.0) as i32).max(1);
    let mut x = -(((t * 90.0) as i32) % w);
    while x < W {
        draw(c, x, y + 14, &line, 30.0, Face::Bold, th.ink);
        x += w;
    }
}

fn text_page(
    c: &mut Canvas,
    th: &Theme,
    title: &str,
    lines: &[String],
    pic: Option<&Picture>,
    t: f32,
    bottom: i32,
) {
    let max_w = if pic.is_some() { 640.0 } else { 1120.0 };
    let (px, tl) = fit(title, Face::Bold, &[68.0, 58.0, 48.0], max_w, 2);
    let mut y = if title.trim().is_empty() { 170 } else { 164 };
    if !title.trim().is_empty() {
        y = block(c, th, 80, y, &tl, px, Face::Bold, th.accent, false, t, 0.0);
        let f = arrive(t, 0.15);
        rect(c, 80, y + 6, (120.0 * f) as i32, 6, th.accent);
        y += 40;
    }
    // The body: as large as fits the room.
    let room = bottom - y - 10;
    for size in [44.0f32, 38.0, 32.0, 28.0] {
        let step = (size * 1.28) as i32;
        let max_lines = (room / step).max(1) as usize;
        let body = lines.join("\n");
        let mut out: Vec<String> = Vec::new();
        let mut fits = true;
        for para in body.split('\n') {
            if para.trim().is_empty() {
                out.push(String::new());
                continue;
            }
            match wrap(para, size, Face::Regular, max_w, usize::MAX) {
                Some(l) => out.extend(l),
                None => fits = false,
            }
        }
        if (fits && out.len() <= max_lines) || size <= 28.0 {
            out.truncate(max_lines);
            for (i, l) in out.iter().enumerate() {
                let f = arrive(t, 0.3 + i as f32 * 0.08);
                let yy = y + i as i32 * step + ((1.0 - f) * 24.0) as i32;
                let col = faded(th.ink, mix(th.dark, th.bottom, 0.4), f);
                draw(c, 80, yy, l, size, Face::Regular, col);
            }
            break;
        }
    }
    if let Some(p) = pic {
        let f = arrive(t, 0.2);
        let (x, y) = (W - 80 - p.w as i32 + ((1.0 - f) * 40.0) as i32, 170);
        rect(
            c,
            x - 8,
            y - 8,
            p.w as i32 + 16,
            p.h as i32 + 16,
            faded(th.accent, th.dark, f),
        );
        blit(c, p, x, y);
        dim_toward(c, th.dark, 0.0);
    }
}

fn list_page(c: &mut Canvas, th: &Theme, title: &str, items: &[ListItem], t: f32, bottom: i32) {
    let (px, tl) = fit(title, Face::Bold, &[60.0, 50.0], 1100.0, 1);
    let mut y = block(
        c,
        th,
        80,
        156,
        &tl,
        px,
        Face::Bold,
        th.accent,
        false,
        t,
        0.0,
    );
    y += 10;
    let rows = items.len().clamp(1, 5) as i32;
    let step = ((bottom - y - 8) / rows).min(96);
    for (i, it) in items.iter().take(5).enumerate() {
        let f = arrive(t, 0.15 + i as f32 * 0.1);
        let yy = y + i as i32 * step + ((1.0 - f) * 30.0) as i32;
        let bg = mix(th.dark, th.bottom, 0.3);
        if !it.tag.is_empty() {
            let w = (text::width(Face::Bold, &it.tag, 34.0) as i32 + 36).max(72);
            rect(c, 80, yy, w, step - 14, faded(th.primary, bg, f));
            centred(
                c,
                80 + w / 2,
                yy + (step - 14 - 34) / 2 - 2,
                &it.tag,
                34.0,
                Face::Bold,
                faded(th.on_accent, bg, f),
            );
            let tx = 80 + w + 28;
            row_text(c, th, tx, yy, step - 14, it, f, bg, 1280 - tx - 70);
        } else {
            rect(c, 80, yy, 10, step - 14, faded(th.accent, bg, f));
            row_text(c, th, 114, yy, step - 14, it, f, bg, 1280 - 114 - 70);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn row_text(
    c: &mut Canvas,
    th: &Theme,
    x: i32,
    y: i32,
    h: i32,
    it: &ListItem,
    f: f32,
    bg: Rgb,
    max_w: i32,
) {
    let two = !it.sub.trim().is_empty();
    let (px, lines) = fit(&it.text, Face::Bold, &[38.0, 32.0, 28.0], max_w as f32, 1);
    let ty = if two {
        y + (h - 38 - 30) / 2 - 2
    } else {
        y + (h - px as i32) / 2 - 2
    };
    draw(
        c,
        x,
        ty,
        &lines.join(" "),
        px,
        Face::Bold,
        faded(th.ink, bg, f),
    );
    if two {
        let (spx, sl) = fit(&it.sub, Face::Regular, &[28.0, 24.0], max_w as f32, 1);
        draw(
            c,
            x,
            ty + 44,
            &sl.join(" "),
            spx,
            Face::Regular,
            faded(mix(th.ink, bg, 0.3), bg, f),
        );
    }
}

fn clock_page(c: &mut Canvas, th: &Theme, clock: &str, date: &str, t: f32) {
    let f = arrive(t, 0.0);
    let bg = mix(th.dark, th.bottom, 0.4);
    let (px, l) = fit(clock, Face::Bold, &[260.0, 200.0, 150.0], 1100.0, 1);
    centred(
        c,
        W / 2,
        210 + ((1.0 - f) * 30.0) as i32,
        &l.join(" "),
        px,
        Face::Bold,
        faded(th.ink, bg, f),
    );
    let g = arrive(t, 0.2);
    rect(
        c,
        W / 2 - 80,
        210 + px as i32 + 10,
        (160.0 * g) as i32,
        6,
        th.accent,
    );
    centred(
        c,
        W / 2,
        210 + px as i32 + 40,
        date,
        60.0,
        Face::Bold,
        faded(th.accent, bg, g),
    );
}

/// One frame, `t` seconds into the channel's loop of pages.
pub fn render_info(f: &InfoFrame, th: &Theme, t: f32) -> Canvas {
    let mut c = Canvas::new(W as usize, H as usize);
    let (idx, into) = page_at(f.pages, t).unwrap_or((0, 0.0));
    let page = f.pages.get(idx);
    let secs = page.map_or(1.0, |p| p.secs.max(1.0));
    // A photo page is the whole screen.
    if let Some(TimedPage {
        page: InfoPage::Photo { picture, caption },
        ..
    }) = page
    {
        blit(&mut c, picture, 0, 0);
        if !caption.trim().is_empty() {
            let f2 = arrive(into, 0.2);
            let (px, l) = fit(caption, Face::Bold, &[44.0, 36.0], 1100.0, 1);
            let h = 96;
            let y = H - h - ((1.0 - f2) * 100.0) as i32;
            // A soft band behind the caption.
            for row in 0..h {
                let k = 0.35 + 0.5 * (row as f32 / h as f32);
                for x in 0..W {
                    let i = ((y + row).clamp(0, H - 1) as usize * c.w + x as usize) * 3;
                    for (d, s) in c.px[i..i + 3].iter_mut().zip(th.dark) {
                        *d = (*d as f32 + (s as f32 - *d as f32) * k) as u8;
                    }
                }
            }
            draw(&mut c, 64, y + 24, &l.join(" "), px, Face::Bold, th.ink);
        }
    } else {
        backdrop(&mut c, th, t);
        header(&mut c, th, f.heading, into.min(0.6) + 0.0);
        if f.show_clock && !f.clock.is_empty() {
            let w = text::width(Face::Bold, f.clock, 44.0) as i32;
            draw(&mut c, W - 64 - w, 46, f.clock, 44.0, Face::Bold, th.ink);
            let dw = text::width(Face::Regular, f.date, 24.0) as i32;
            draw(
                &mut c,
                W - 64 - dw,
                98,
                f.date,
                24.0,
                Face::Regular,
                th.accent,
            );
        }
        let bottom = if f.ticker.is_empty() {
            H - 40
        } else {
            H - TICKER_H - 16
        };
        match page.map(|p| &p.page) {
            Some(InfoPage::Text {
                title,
                lines,
                picture,
            }) => text_page(&mut c, th, title, lines, picture.as_ref(), into, bottom),
            Some(InfoPage::List { title, items }) => {
                list_page(&mut c, th, title, items, into, bottom)
            }
            Some(InfoPage::Clock) | None => clock_page(&mut c, th, f.clock, f.date, into),
            Some(InfoPage::Photo { .. }) => {}
        }
    }
    if !f.ticker.is_empty() {
        ticker_strip(&mut c, th, f.ticker, t);
    }
    // Pages fade through the background at their edges.
    let edge = into.min(secs - into);
    if page.is_some() && edge < FADE && f.pages.len() > 1 {
        dim_toward(&mut c, th.dark, 1.0 - edge / FADE);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    fn th() -> Theme {
        Theme::from_hue(0.6)
    }

    fn lines(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("Line {i} of the notice")).collect()
    }

    fn solid(w: usize, h: usize, rgb: Rgb) -> Picture {
        Picture {
            w,
            h,
            rgb: Arc::new(rgb.iter().copied().cycle().take(w * h * 3).collect()),
        }
    }

    fn pages() -> Vec<TimedPage> {
        vec![
            TimedPage {
                page: InfoPage::Text {
                    title: "Garage sale".into(),
                    lines: lines(3),
                    picture: Some(solid(456, 390, [200, 40, 40])),
                },
                secs: 10.0,
            },
            TimedPage {
                page: InfoPage::List {
                    title: "Headlines".into(),
                    items: (1..=5)
                        .map(|i| ListItem {
                            tag: i.to_string(),
                            text: format!("Headline number {i}"),
                            sub: "then something".into(),
                        })
                        .collect(),
                },
                secs: 10.0,
            },
            TimedPage {
                page: InfoPage::Clock,
                secs: 10.0,
            },
            TimedPage {
                page: InfoPage::Photo {
                    picture: solid(1280, 720, [10, 200, 10]),
                    caption: "The lake".into(),
                },
                secs: 10.0,
            },
        ]
    }

    fn frame<'a>(p: &'a [TimedPage], ticker: &'a str) -> InfoFrame<'a> {
        InfoFrame {
            heading: "Community Channel",
            clock: "9:04 PM",
            date: "SUN NOV 15",
            show_clock: true,
            ticker,
            pages: p,
        }
    }

    #[test]
    fn pages_follow_one_another_and_loop() {
        let p = pages();
        assert_eq!(page_at(&p, 0.0), Some((0, 0.0)));
        assert_eq!(page_at(&p, 12.5).map(|x| x.0), Some(1));
        assert_eq!(page_at(&p, 39.9).map(|x| x.0), Some(3));
        assert_eq!(page_at(&p, 40.0).map(|x| x.0), Some(0), "round again");
        assert_eq!(page_at(&[], 3.0), None);
    }

    #[test]
    fn every_kind_of_page_draws_something_different_in_every_theme() {
        let p = pages();
        for hue in [0.1f32, 0.6, 0.9] {
            let th = Theme::from_hue(hue);
            let mut seen = Vec::new();
            for (i, _) in p.iter().enumerate() {
                let c = render_info(&frame(&p, "News: all is well"), &th, i as f32 * 10.0 + 5.0);
                assert_eq!((c.w, c.h), (1280, 720));
                assert!(c.px.iter().any(|v| *v != c.px[0]), "page {i} is blank");
                seen.push(c.px);
            }
            for i in 0..seen.len() {
                for j in i + 1..seen.len() {
                    assert_ne!(seen[i], seen[j], "pages {i} and {j} look alike");
                }
            }
        }
    }

    #[test]
    fn the_ticker_and_the_clock_are_optional_and_the_ticker_moves() {
        let p = pages();
        let th = th();
        let with = render_info(&frame(&p, "Breaking: nothing"), &th, 5.0);
        let without = render_info(&frame(&p, ""), &th, 5.0);
        assert_ne!(with.px, without.px);
        let later = render_info(&frame(&p, "Breaking: nothing"), &th, 5.5);
        assert_ne!(with.px, later.px);
        let mut f = frame(&p, "");
        f.show_clock = false;
        assert_ne!(render_info(&f, &th, 5.0).px, without.px);
    }

    #[test]
    fn a_long_notice_still_fits_and_an_empty_channel_still_shows_the_time() {
        let long: Vec<String> = (0..40)
            .map(|i| format!("This notice is long enough to need a good deal of room, line {i}."))
            .collect();
        let p = vec![TimedPage {
            page: InfoPage::Text {
                title: "A very very very very very long title for a notice board page".into(),
                lines: long,
                picture: None,
            },
            secs: 8.0,
        }];
        let c = render_info(&frame(&p, "x"), &th(), 4.0);
        assert_eq!(c.px.len(), 1280 * 720 * 3);
        let empty = render_info(&frame(&[], ""), &th(), 1.0);
        assert!(empty.px.iter().any(|v| *v != empty.px[0]));
    }

    #[test]
    fn it_reads_on_a_light_theme_too() {
        let p = crate::guide::Palette::kids();
        let th = Theme::from_palette(&p, true);
        let c = render_info(&frame(&pages(), "Hello"), &th, 5.0);
        assert!(c.px.iter().any(|v| *v != c.px[0]));
    }
}

#[cfg(test)]
mod dump {
    use super::*;
    #[test]
    fn dump_pages() {
        let Ok(dir) = std::env::var("TVM_DUMP") else {
            return;
        };
        let pages = vec![
            TimedPage {
                page: InfoPage::Text {
                    title: "Garage sale this Saturday".into(),
                    lines: vec![
                        "Come by 12 Maple Street from 8 to 2.".into(),
                        "Furniture, books, toys and bikes.".into(),
                        "All proceeds go to the library.".into(),
                    ],
                    picture: None,
                },
                secs: 10.0,
            },
            TimedPage {
                page: InfoPage::List {
                    title: "Tonight on TVMaster".into(),
                    items: (1..=5)
                        .map(|i| ListItem {
                            tag: i.to_string(),
                            text: format!("Cheers {i}"),
                            sub: "then Frasier at 9:00".into(),
                        })
                        .collect(),
                },
                secs: 10.0,
            },
            TimedPage {
                page: InfoPage::Clock,
                secs: 10.0,
            },
        ];
        for (name, th) in [
            (
                "kids",
                Theme::from_palette(&crate::guide::Palette::kids(), true),
            ),
            ("dark", Theme::from_hue(0.6)),
        ] {
            for (i, _) in pages.iter().enumerate() {
                let f = InfoFrame {
                    heading: "Community Channel",
                    clock: "9:04 PM",
                    date: "SUN NOV 15",
                    show_clock: true,
                    ticker: "Welcome to TVMaster   *   Bake sale Friday",
                    pages: &pages,
                };
                let c = render_info(&f, &th, i as f32 * 10.0 + 3.0);
                let mut rgba = Vec::new();
                for p in c.px.as_chunks::<3>().0 {
                    rgba.extend_from_slice(p);
                    rgba.push(255);
                }
                std::fs::write(
                    format!("{dir}/{name}-{i}.png"),
                    crate::png::rgba_png(1280, 720, &rgba),
                )
                .unwrap();
            }
        }
    }
}
