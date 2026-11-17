//! The smooth "modern" look for station graphics: the same bumpers and station
//! card as the retro set, drawn at 1280x720 with a real typeface, soft shapes
//! and gentle motion, in whichever theme the station uses.

use crate::alert::{centred, disc, draw, ink_on, rect, vgrad, wrap};
use crate::overlay::Graphic;
use crate::text::{self, Face};
use crate::{mix, png, Canvas, Photo, PromoKind, PromoSpec, Rgb, Scene, Theme};

const W: i32 = 1280;
const H: i32 = 720;

fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p) * (1.0 - p) * (1.0 - p)
}

/// 0 = just arrived, 1 = settled, for an element that starts at `delay` seconds.
fn arrive(t: f32, delay: f32) -> f32 {
    ease((t - delay) / 0.6)
}

/// Where `col` is when it has faded `f` of the way in from the background.
fn faded(col: Rgb, bg: Rgb, f: f32) -> Rgb {
    mix(bg, col, f)
}

fn backdrop(c: &mut Canvas, th: &Theme, t: f32) {
    vgrad(c, th.dark, th.bottom);
    // A soft glow that drifts slowly, in the main colour.
    let cx = (W as f32 * 0.78 + (t * 0.35).sin() * 60.0) as i32;
    let cy = (H as f32 * 0.22 + (t * 0.27).cos() * 30.0) as i32;
    for i in 0..14 {
        let r = 560 - i * 38;
        glow(c, cx, cy, r, th.primary, 0.014);
    }
    // A thin accent rule along the bottom.
    rect(c, 0, H - 10, W, 10, th.primary);
    rect(c, 0, H - 10, W / 3, 10, th.accent);
}

fn glow(c: &mut Canvas, cx: i32, cy: i32, r: i32, col: Rgb, k: f32) {
    let r2 = (r * r) as i64;
    let (y0, y1) = ((cy - r).max(0), (cy + r).min(H - 1));
    for y in y0..=y1 {
        let dy = (y - cy) as i64;
        let half = ((r2 - dy * dy).max(0) as f64).sqrt() as i32;
        let (x0, x1) = ((cx - half).max(0), (cx + half).min(W - 1));
        for x in x0..=x1 {
            let i = (y as usize * c.w + x as usize) * 3;
            for (d, s) in c.px[i..i + 3].iter_mut().zip(col) {
                *d = (*d as f32 + (s as f32 - *d as f32) * k) as u8;
            }
        }
    }
}

/// The largest of `sizes` at which `s` fits `max_w` over at most `lines` lines.
fn fit(s: &str, face: Face, sizes: &[f32], max_w: f32, lines: usize) -> (f32, Vec<String>) {
    let s = if s.trim().is_empty() { "TV" } else { s.trim() };
    for &px in sizes {
        if let Some(l) = wrap(s, px, face, max_w, lines) {
            return (px, l);
        }
    }
    let px = *sizes.last().unwrap_or(&40.0);
    let mut l = wrap(s, px, face, max_w, usize::MAX).unwrap_or_default();
    l.truncate(lines);
    if let Some(last) = l.last_mut() {
        while text::width(face, &format!("{last}…"), px) > max_w && last.chars().count() > 1 {
            last.pop();
        }
        last.push('…');
    }
    (px, l)
}

fn header(c: &mut Canvas, th: &Theme, name: &str, t: f32) {
    let f = arrive(t, 0.0);
    rect(c, 64, 56, 8, 44, faded(th.accent, th.dark, f));
    let (px, l) = fit(name, Face::Bold, &[40.0, 32.0], 900.0, 1);
    draw(
        c,
        92,
        56,
        &l.join(" "),
        px,
        Face::Bold,
        faded(th.ink, th.dark, f),
    );
}

/// Lines of text, each arriving a little after the one before, centred or left.
#[allow(clippy::too_many_arguments)]
fn block(
    c: &mut Canvas,
    th: &Theme,
    x: i32,
    mut y: i32,
    lines: &[String],
    px: f32,
    face: Face,
    col: Rgb,
    centre: bool,
    t: f32,
    delay: f32,
) -> i32 {
    for (i, l) in lines.iter().enumerate() {
        let f = arrive(t, delay + i as f32 * 0.1);
        let yy = y + ((1.0 - f) * 36.0) as i32;
        let colour = faded(col, mix(th.dark, th.bottom, y as f32 / H as f32), f);
        if centre {
            centred(c, W / 2, yy, l, px, face, colour);
        } else {
            draw(c, x, yy, l, px, face, colour);
        }
        y += (px * 1.16) as i32;
    }
    y
}

/// One frame of a bumper, `t` seconds in of `dur`.
pub fn render(scene: &Scene, th: &Theme, t: f32, dur: f32) -> Canvas {
    let mut c = Canvas::new(W as usize, H as usize);
    backdrop(&mut c, th, t);
    match scene {
        Scene::StationId { name } => {
            let f = arrive(t, 0.05);
            centred(
                &mut c,
                W / 2,
                200,
                "YOU'RE WATCHING",
                34.0,
                Face::Bold,
                faded(th.accent, th.dark, f),
            );
            let (px, ls) = fit(name, Face::Bold, &[150.0, 120.0, 96.0, 76.0], 1100.0, 2);
            let y = 250 - (ls.len() as i32 - 1) * (px as i32 / 2);
            let end = block(&mut c, th, 0, y, &ls, px, Face::Bold, th.ink, true, t, 0.15);
            let grow = arrive(t, 0.5);
            let w = (360.0 * grow) as i32;
            rect(&mut c, W / 2 - w / 2, end + 14, w, 8, th.accent);
        }
        Scene::Brb { name, show } => {
            header(&mut c, th, name, t);
            block(
                &mut c,
                th,
                96,
                200,
                &["We'll be".into(), "right back".into()],
                140.0,
                Face::Bold,
                th.ink,
                false,
                t,
                0.05,
            );
            let (px, ls) = fit(
                &format!("After this: {show}"),
                Face::Regular,
                &[44.0, 36.0, 30.0],
                1080.0,
                2,
            );
            block(
                &mut c,
                th,
                96,
                520,
                &ls,
                px,
                Face::Regular,
                th.accent,
                false,
                t,
                0.5,
            );
        }
        Scene::BackTo { name, show } => {
            header(&mut c, th, name, t);
            block(
                &mut c,
                th,
                96,
                190,
                &["Welcome back to".into()],
                54.0,
                Face::Regular,
                th.accent,
                false,
                t,
                0.05,
            );
            let (px, ls) = fit(show, Face::Bold, &[132.0, 108.0, 88.0, 70.0], 1090.0, 3);
            block(
                &mut c,
                th,
                96,
                270,
                &ls,
                px,
                Face::Bold,
                th.ink,
                false,
                t,
                0.2,
            );
        }
        Scene::UpNext { name, title, time } => {
            header(&mut c, th, name, t);
            chip(&mut c, th, 96, 190, "UP NEXT", t, 0.05);
            let (px, ls) = fit(title, Face::Bold, &[120.0, 100.0, 82.0, 66.0], 1090.0, 2);
            let end = block(
                &mut c,
                th,
                96,
                290,
                &ls,
                px,
                Face::Bold,
                th.ink,
                false,
                t,
                0.2,
            );
            if !time.is_empty() {
                block(
                    &mut c,
                    th,
                    96,
                    end + 20,
                    &[format!("at {time}")],
                    56.0,
                    Face::Regular,
                    th.accent,
                    false,
                    t,
                    0.5,
                );
            }
        }
        Scene::LaterToday {
            name,
            evening,
            items,
        } => {
            header(&mut c, th, name, t);
            chip(
                &mut c,
                th,
                96,
                170,
                if *evening {
                    "COMING UP TONIGHT"
                } else {
                    "LATER TODAY"
                },
                t,
                0.05,
            );
            let mut y = 268;
            for (i, (time, prog)) in items.iter().take(4).enumerate() {
                let f = arrive(t, 0.2 + i as f32 * 0.12);
                let bg = mix(th.dark, th.bottom, y as f32 / H as f32);
                let yy = y + ((1.0 - f) * 30.0) as i32;
                draw(
                    &mut c,
                    96,
                    yy,
                    time,
                    46.0,
                    Face::Bold,
                    faded(th.accent, bg, f),
                );
                let (px, l) = fit(prog, Face::Regular, &[46.0, 38.0], 840.0, 1);
                draw(
                    &mut c,
                    360,
                    yy,
                    &l.join(" "),
                    px,
                    Face::Regular,
                    faded(th.ink, bg, f),
                );
                y += 84;
            }
        }
        Scene::Promo { spec, photo } => promo(&mut c, th, spec, photo.as_ref(), t, dur),
        Scene::Alert { .. } => {}
    }
    // Fade in from black and out to black.
    let f = (t / 0.3).min(1.0).min(((dur - t) / 0.4).max(0.0));
    c.brightness(f);
    c
}

/// A spot made in the app, in the smooth style.
fn promo(c: &mut Canvas, th: &Theme, spec: &PromoSpec, photo: Option<&Photo>, t: f32, dur: f32) {
    if let Some(p) = photo {
        crate::draw_photo(c, p, t, dur, 0.5);
    }
    match spec.kind {
        PromoKind::Sponsor => {
            let (px, ls) = fit(
                &spec.title,
                Face::Bold,
                &[150.0, 120.0, 96.0, 76.0],
                1100.0,
                2,
            );
            let y = 210 - (ls.len() as i32 - 1) * (px as i32 / 2);
            let end = block(c, th, 0, y, &ls, px, Face::Bold, th.ink, true, t, 0.1);
            let grow = arrive(t, 0.5);
            let w = (360.0 * grow) as i32;
            rect(c, W / 2 - w / 2, end + 8, w, 8, th.accent);
            if !spec.tagline.trim().is_empty() {
                let (tp, tl) = fit(&spec.tagline, Face::Regular, &[56.0, 46.0, 38.0], 1080.0, 2);
                block(
                    c,
                    th,
                    0,
                    end + 40,
                    &tl,
                    tp,
                    Face::Regular,
                    th.accent,
                    true,
                    t,
                    0.7,
                );
            }
            if !spec.contact.trim().is_empty() {
                let f = arrive(t, 1.4);
                let rise = ((1.0 - f) * 90.0) as i32;
                rect(c, 0, H - 118 + rise, W, 118, th.accent);
                let (cp, cl) = fit(&spec.contact, Face::Bold, &[44.0, 36.0, 30.0], 1160.0, 1);
                centred(
                    c,
                    W / 2,
                    H - 82 + rise,
                    &cl.join(" "),
                    cp,
                    Face::Bold,
                    th.on_accent,
                );
            }
        }
        PromoKind::ComingUp => {
            chip(c, th, 96, 170, "COMING UP", t, 0.05);
            let (px, ls) = fit(
                &spec.title,
                Face::Bold,
                &[130.0, 108.0, 88.0, 70.0],
                1090.0,
                3,
            );
            let end = block(c, th, 96, 270, &ls, px, Face::Bold, th.ink, false, t, 0.25);
            if !spec.tagline.trim().is_empty() {
                let (tp, tl) = fit(&spec.tagline, Face::Regular, &[58.0, 48.0, 40.0], 1080.0, 2);
                block(
                    c,
                    th,
                    96,
                    end + 18,
                    &tl,
                    tp,
                    Face::Regular,
                    th.accent,
                    false,
                    t,
                    0.6,
                );
            }
            if !spec.contact.trim().is_empty() {
                let (cp, cl) = fit(&spec.contact, Face::Regular, &[36.0, 30.0], 1090.0, 1);
                block(
                    c,
                    th,
                    96,
                    H - 110,
                    &cl,
                    cp,
                    Face::Regular,
                    th.ink,
                    false,
                    t,
                    1.1,
                );
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
                let (px, ls) = fit(text, Face::Bold, &[110.0, 92.0, 76.0, 60.0], 1100.0, 4);
                let y = 300 - (ls.len() as i32) * (px as i32 / 2);
                // Fade each slide out at its end so the next arrives cleanly.
                let out = ((each - local) / 0.35).clamp(0.0, 1.0);
                let fo = ease(out);
                block(
                    c,
                    th,
                    0,
                    y,
                    &ls,
                    px,
                    Face::Bold,
                    faded(th.ink, mix(th.dark, th.bottom, 0.5), fo),
                    true,
                    local,
                    0.05,
                );
            }
            for k in 0..n {
                let col = if k == i {
                    th.accent
                } else {
                    mix(th.dark, th.ink, 0.35)
                };
                disc(
                    c,
                    W / 2 + (k as i32 * 28) - (n as i32 - 1) * 14,
                    H - 56,
                    7,
                    col,
                );
            }
        }
    }
}

fn chip(c: &mut Canvas, th: &Theme, x: i32, y: i32, label: &str, t: f32, delay: f32) {
    let f = arrive(t, delay);
    let px = 40.0;
    let w = text::width(Face::Bold, label, px) as i32 + 56;
    let off = ((1.0 - f) * 60.0) as i32;
    let bg = mix(th.dark, th.bottom, y as f32 / H as f32);
    let col = faded(th.accent, bg, f);
    rect(c, x - off, y, w, 64, col);
    draw(
        c,
        x - off + 28,
        y + 8,
        label,
        px,
        Face::Bold,
        faded(th.on_accent, bg, f),
    );
}

/// The station card (number, name, message) as a full-screen picture.
pub fn station_card(th: &Theme, number: u32, name: &str, message: &str) -> Graphic {
    let mut c = Canvas::new(W as usize, H as usize);
    backdrop(&mut c, th, 0.0);
    // The channel number in a disc.
    disc(&mut c, W / 2, 218, 118, th.primary);
    let n = number.to_string();
    let ink = ink_on(th.primary);
    let size = if n.len() > 3 { 84.0 } else { 120.0 };
    centred(
        &mut c,
        W / 2,
        218 - (size * 0.5) as i32,
        &n,
        size,
        Face::Bold,
        ink,
    );
    let (px, l) = fit(name, Face::Bold, &[76.0, 62.0, 50.0], 1100.0, 1);
    centred(&mut c, W / 2, 372, &l.join(" "), px, Face::Bold, th.ink);
    let msg = if message.trim().is_empty() {
        "PLEASE STAND BY".to_string()
    } else {
        message.trim().to_string()
    };
    let (mp, ml) = fit(&msg, Face::Bold, &[52.0, 44.0, 36.0], 1100.0, 2);
    let mut y = 490;
    for line in &ml {
        centred(&mut c, W / 2, y, line, mp, Face::Bold, th.accent);
        y += (mp * 1.2) as i32;
    }
    let mut rgba = Vec::with_capacity(c.w * c.h * 4);
    for p in c.px.as_chunks::<3>().0 {
        rgba.extend([p[0], p[1], p[2], 255]);
    }
    Graphic {
        w: W as u32,
        h: H as u32,
        png: png::rgba_png(W as u32, H as u32, &rgba),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide::Palette;

    fn themes() -> Vec<Theme> {
        [
            Palette::midnight(),
            Palette::snow(),
            Palette::paper(),
            Palette::amber(),
            Palette::mono(),
        ]
        .iter()
        .map(|p| Theme::from_palette(p, true))
        .collect()
    }

    fn scenes() -> Vec<Scene> {
        vec![
            Scene::StationId {
                name: "Retro TV".into(),
            },
            Scene::Brb {
                name: "Retro TV".into(),
                show: "Cheers".into(),
            },
            Scene::BackTo {
                name: "Retro TV".into(),
                show: "Cheers".into(),
            },
            Scene::UpNext {
                name: "Retro TV".into(),
                title: "Taxi".into(),
                time: "8:30 PM".into(),
            },
            Scene::LaterToday {
                name: "Retro TV".into(),
                evening: true,
                items: vec![
                    ("8:30 PM".into(), "Taxi".into()),
                    ("9:00 PM".into(), "Cheers".into()),
                ],
            },
        ]
    }

    #[test]
    fn every_scene_draws_in_every_theme_at_full_size() {
        for th in themes() {
            for s in scenes() {
                let c = crate::render(&s, &th, 2.0, 5.0);
                assert_eq!((c.w, c.h), th.frame_size());
                let distinct: std::collections::HashSet<&[u8]> =
                    c.px.chunks(3).step_by(997).collect();
                assert!(distinct.len() > 6, "{s:?} looks blank");
            }
        }
    }

    #[test]
    fn scenes_fade_move_and_stay_deterministic() {
        let th = &themes()[0];
        let s = &scenes()[3];
        let mean = |c: &Canvas| c.px.iter().map(|v| *v as f64).sum::<f64>() / c.px.len() as f64;
        assert_ne!(
            crate::render(s, th, 1.0, 5.0).px,
            crate::render(s, th, 2.0, 5.0).px
        );
        assert_eq!(
            crate::render(s, th, 2.0, 5.0).px,
            crate::render(s, th, 2.0, 5.0).px
        );
        assert!(
            mean(&crate::render(s, th, 0.0, 5.0)) < mean(&crate::render(s, th, 2.5, 5.0)) * 0.2
        );
    }

    #[test]
    fn long_text_does_not_panic() {
        let long = "THE INCREDIBLY LONG ADVENTURES OF A SHOW WITH AN ABSURDLY VERBOSE NAME THAT KEEPS GOING AND GOING";
        let th = &themes()[1];
        for s in [
            Scene::UpNext {
                name: long.into(),
                title: long.into(),
                time: long.into(),
            },
            Scene::StationId {
                name: String::new(),
            },
            Scene::LaterToday {
                name: "N".into(),
                evening: false,
                items: (0..9)
                    .map(|i| (format!("{i}:00"), long.to_string()))
                    .collect(),
            },
        ] {
            for t in [0.0, 1.0, 2.9] {
                assert_eq!(crate::render(&s, th, t, 3.0).px.len(), 1280 * 720 * 3);
            }
        }
    }

    #[test]
    fn the_modern_station_card_is_a_full_picture() {
        let th = &themes()[0];
        let g = station_card(th, 4, "Retro TV", "");
        assert_eq!((g.w, g.h), (1280, 720));
        let other = station_card(th, 4, "Retro TV", "BACK AT NINE");
        assert_ne!(g.png, other.png);
    }
}
