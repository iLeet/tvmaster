//! Smooth (anti-aliased) text for the modern guide looks. The retro looks keep
//! the chunky 8x8 pixel font; this draws a real typeface, bundled so it looks
//! the same on every platform (Lato, SIL Open Font License; see `fonts/OFL.txt`).

use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Regular,
    Bold,
}

static REGULAR: &[u8] = include_bytes!("../fonts/Lato-Regular.subset.ttf");
static BOLD: &[u8] = include_bytes!("../fonts/Lato-Bold.subset.ttf");

fn font(face: Face) -> &'static Font {
    static FONTS: OnceLock<(Font, Font)> = OnceLock::new();
    let (r, b) = FONTS.get_or_init(|| {
        let load = |d: &[u8]| {
            Font::from_bytes(d, FontSettings::default()).expect("the bundled font is valid")
        };
        (load(REGULAR), load(BOLD))
    });
    match face {
        Face::Regular => r,
        Face::Bold => b,
    }
}

/// One rendered character: coverage (0-255) per pixel, and where it sits.
pub struct Glyph {
    pub w: usize,
    pub h: usize,
    /// Pixels from the pen to the left edge of the bitmap.
    pub left: i32,
    /// Pixels from the baseline up to the top edge of the bitmap.
    pub top: i32,
    pub advance: f32,
    pub coverage: Vec<u8>,
}

type Key = (Face, char, u32);

/// Characters the font lacks (and everything else unusual) draw as `?`.
fn usable(face: Face, c: char) -> char {
    if c == ' ' || font(face).lookup_glyph_index(c) != 0 {
        c
    } else {
        '?'
    }
}

/// The glyph for `c` at `px` pixels tall, cached (drawing a guide frame asks for the same few hundred over and over).
pub fn glyph(face: Face, c: char, px: f32) -> Arc<Glyph> {
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<Glyph>>>> = OnceLock::new();
    let c = usable(face, c);
    let key = (face, c, (px * 8.0).round() as u32);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(g) = cache.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
        return g.clone();
    }
    let (m, coverage) = font(face).rasterize(c, key.2 as f32 / 8.0);
    let g = Arc::new(Glyph {
        w: m.width,
        h: m.height,
        left: m.xmin,
        top: m.ymin + m.height as i32,
        advance: m.advance_width,
        coverage,
    });
    let mut map = cache.lock().unwrap_or_else(|p| p.into_inner());
    // A guide has a small vocabulary, but never let a pathological one grow without bound.
    if map.len() > 20_000 {
        map.clear();
    }
    map.insert(key, g.clone());
    g
}

fn kern(face: Face, left: char, right: char, px: f32) -> f32 {
    font(face)
        .horizontal_kern(usable(face, left), usable(face, right), px)
        .unwrap_or(0.0)
}

/// How wide `text` is at `px` pixels tall.
pub fn width(face: Face, text: &str, px: f32) -> f32 {
    let mut w = 0.0;
    let mut prev: Option<char> = None;
    for c in text.chars() {
        w += glyph(face, c, px).advance;
        if let Some(p) = prev {
            w += kern(face, p, c, px);
        }
        prev = Some(c);
    }
    w
}

/// Pen positions (pixels from the start) for each character of `text`.
pub fn layout(face: Face, text: &str, px: f32) -> Vec<(char, f32)> {
    let mut out = Vec::with_capacity(text.len());
    let mut x = 0.0;
    let mut prev: Option<char> = None;
    for c in text.chars() {
        if let Some(p) = prev {
            x += kern(face, p, c, px);
        }
        out.push((c, x));
        x += glyph(face, c, px).advance;
        prev = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_has_width_that_grows_with_size_and_length() {
        let a = width(Face::Regular, "Cheers", 20.0);
        assert!(a > 40.0 && a < 120.0, "{a}");
        assert!(width(Face::Regular, "Cheers", 40.0) > a * 1.8);
        assert!(width(Face::Regular, "Cheers Cheers", 20.0) > a * 1.9);
        assert!(width(Face::Bold, "Cheers", 20.0) > a, "bold is wider");
        assert_eq!(width(Face::Regular, "", 20.0), 0.0);
    }

    #[test]
    fn a_glyph_has_coverage_and_is_cached() {
        let g = glyph(Face::Regular, 'A', 24.0);
        assert!(g.w > 8 && g.h > 10 && g.coverage.iter().any(|c| *c > 200));
        assert_eq!(g.coverage.len(), g.w * g.h);
        assert!(Arc::ptr_eq(&g, &glyph(Face::Regular, 'A', 24.0)));
    }

    #[test]
    fn what_the_font_lacks_draws_as_a_question_mark() {
        let q = glyph(Face::Regular, '?', 20.0);
        for odd in ['日', '😀', '\u{0}'] {
            let g = glyph(Face::Regular, odd, 20.0);
            assert_eq!(g.coverage, q.coverage, "{odd:?}");
        }
        for ok in ['é', 'ü', 'ñ', '—', '€'] {
            assert_ne!(
                glyph(Face::Regular, ok, 20.0).coverage,
                q.coverage,
                "{ok:?} is in the font"
            );
        }
    }

    #[test]
    fn layout_places_characters_left_to_right() {
        let l = layout(Face::Regular, "Taxi", 20.0);
        assert_eq!(l.len(), 4);
        assert!(l.windows(2).all(|w| w[1].1 > w[0].1));
        assert!(
            (l[3].1 + glyph(Face::Regular, 'i', 20.0).advance - width(Face::Regular, "Taxi", 20.0))
                .abs()
                < 0.01
        );
    }
}
