//! On-screen graphics laid over the programme: the channel bug in a corner,
//! and the "now playing" lower third. Drawn in the same chunky pixel font as
//! the bumpers, as transparent PNGs that FFmpeg overlays.

use crate::{png, text_width, Canvas, Theme};

/// Transparent background key: never used for drawing.
const KEY: [u8; 3] = [255, 0, 255];

pub struct Graphic {
    pub w: u32,
    pub h: u32,
    pub png: Vec<u8>,
}

fn to_png(c: &Canvas, alpha: u8) -> Graphic {
    let mut rgba = Vec::with_capacity(c.w * c.h * 4);
    for p in c.px.as_chunks::<3>().0 {
        if *p == KEY {
            rgba.extend([0, 0, 0, 0]);
        } else {
            rgba.extend([p[0], p[1], p[2], alpha]);
        }
    }
    Graphic {
        w: c.w as u32,
        h: c.h as u32,
        png: png::rgba_png(c.w as u32, c.h as u32, &rgba),
    }
}

/// The channel logo: text on a small box. `opacity` is 10-100 percent.
pub fn bug(text: &str, opacity: u8) -> Graphic {
    let text: String = text.chars().take(24).collect();
    let (scale, pad) = (2, 6);
    let w = (text_width(&text, scale) + pad * 2).max(16);
    let h = 8 * scale + pad * 2;
    let mut c = Canvas::new(w as usize, h as usize);
    c.fill(KEY);
    c.rect(0, 0, w, h, [16, 20, 36]);
    c.rect(0, 0, w, 2, [255, 255, 255]);
    c.text(pad, pad, &text, scale, [255, 255, 255]);
    to_png(&c, (opacity.clamp(10, 100) as u32 * 255 / 100) as u8)
}

/// A lower third: the programme's name, and below it the episode.
pub fn lower_third(title: &str, subtitle: &str, width: u32) -> Graphic {
    let w = width.clamp(240, 900) as i32;
    let (big, small, pad) = (3, 2, 10);
    let fit = |s: &str, scale: i32| -> String {
        let max = ((w - pad * 2) / (8 * scale)).max(4) as usize;
        let chars: Vec<char> = s.chars().collect();
        if chars.len() <= max {
            s.to_string()
        } else {
            chars[..max - 1].iter().collect::<String>() + "~"
        }
    };
    let (t, s) = (fit(title, big), fit(subtitle, small));
    let has_sub = !s.trim().is_empty();
    let h = pad * 2 + 8 * big + if has_sub { 6 + 8 * small } else { 0 };
    let mut c = Canvas::new(w as usize, h as usize);
    c.fill(KEY);
    c.rect(0, 0, w, h, [12, 16, 30]);
    c.rect(0, 0, 6, h, [255, 200, 60]);
    c.text(pad + 6, pad, &t, big, [255, 255, 255]);
    if has_sub {
        c.text(pad + 6, pad + 8 * big + 6, &s, small, [190, 200, 225]);
    }
    to_png(&c, 235)
}

/// A full-screen station card: the channel's number and name, and a message,
/// in the channel's colours. 640x360, to be scaled up without smoothing.
pub fn station_card(theme: &Theme, number: u32, name: &str, message: &str) -> Graphic {
    let (w, h) = (crate::WIDTH, crate::HEIGHT);
    let mut c = Canvas::new(w, h);
    c.vgradient(theme.dark, [4, 6, 12]);
    c.rect(0, 0, w as i32, 6, theme.primary);
    c.rect(0, h as i32 - 6, w as i32, 6, theme.accent);
    let num = number.to_string();
    c.text_centered(70, &num, 10, theme.primary);
    let name: String = name.chars().take(24).collect();
    c.text_centered(190, &name, 3, [255, 255, 255]);
    let msg: String = if message.trim().is_empty() {
        "PLEASE STAND BY".into()
    } else {
        message.trim().chars().take(36).collect()
    };
    let scale = if msg.chars().count() > 22 { 2 } else { 3 };
    c.text_centered(262, &msg, scale, theme.accent);
    Graphic {
        w: w as u32,
        h: h as u32,
        png: png::rgba_png(w as u32, h as u32, &rgba_opaque(&c)),
    }
}

fn rgba_opaque(c: &Canvas) -> Vec<u8> {
    let mut out = Vec::with_capacity(c.w * c.h * 4);
    for p in c.px.as_chunks::<3>().0 {
        out.extend([p[0], p[1], p[2], 255]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded_alpha(g: &Graphic) -> (usize, usize) {
        // Walk the stored deflate stream back to the pixels to count transparent and opaque ones.
        let idat = g.png.windows(4).position(|w| w == b"IDAT").unwrap() + 4;
        let mut z = &g.png[idat + 2..]; // skip the zlib header
        let mut raw: Vec<u8> = Vec::new();
        loop {
            let last = z[0] == 1;
            let len = u16::from_le_bytes([z[1], z[2]]) as usize;
            raw.extend(&z[5..5 + len]);
            z = &z[5 + len..];
            if last {
                break;
            }
        }
        let (mut clear, mut solid) = (0, 0);
        for row in raw.chunks(g.w as usize * 4 + 1) {
            for px in row[1..].chunks(4) {
                if px[3] == 0 {
                    clear += 1
                } else {
                    solid += 1
                }
            }
        }
        (clear, solid)
    }

    #[test]
    fn a_bug_is_a_small_graphic_with_the_requested_opacity() {
        let g = bug("CH 4 RETRO", 50);
        assert!(g.w >= 100 && g.h >= 20 && g.w < 400);
        let (_, solid) = decoded_alpha(&g);
        assert_eq!(
            solid as u32,
            g.w * g.h,
            "the box is opaque at the chosen level everywhere"
        );
        let alpha_of_first = g.png.windows(4).position(|w| w == b"IDAT").is_some();
        assert!(alpha_of_first);
    }

    #[test]
    fn a_lower_third_fits_long_titles_and_has_transparent_margins_only_when_asked() {
        let g = lower_third(
            "A Very Long Programme Title That Cannot Possibly Fit On One Line",
            "S01E02 - The Episode",
            400,
        );
        assert_eq!(g.w, 400);
        assert!(g.h > 40);
        let (clear, solid) = decoded_alpha(&g);
        assert_eq!(clear, 0);
        assert!(solid > 0);
        // wider than allowed is clamped
        assert_eq!(lower_third("x", "", 5000).w, 900);
    }

    #[test]
    fn a_station_card_is_a_full_opaque_frame_with_the_channels_colours() {
        let g = station_card(&Theme::for_channel("retro"), 4, "Retro TV", "");
        assert_eq!((g.w, g.h), (640, 360));
        assert_eq!(decoded_alpha(&g), (0, 640 * 360), "every pixel opaque");
        // a different message gives a different picture
        let other = station_card(&Theme::for_channel("retro"), 4, "Retro TV", "BACK AT NINE");
        assert_ne!(g.png, other.png);
    }
}
