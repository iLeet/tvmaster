//! The ticker: a strip of scrolling text across the picture, in the station's theme. The strip, the
//! little coloured tab at its left and the text are separate pictures; FFmpeg slides the text along
//! the strip, under the tab.

use crate::alert::{blit_png, sample_picture, Graphic, Rgba};
use crate::text::{self, Face};
use crate::{png, text_width, Canvas, Rgb, Theme};

const KEY: Rgb = [255, 0, 255];

pub struct Ticker {
    pub strip: Graphic,
    /// The tab with the label, if there is one.
    pub chip: Option<Graphic>,
    /// The text, one long line (it is repeated with a separator so it never ends abruptly).
    pub crawl: Graphic,
    /// Height of the strip.
    pub height: u32,
}

fn canvas_graphic(c: &Canvas, alpha: u8) -> Graphic {
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
        rgba,
    }
}

/// The pictures for a ticker over a screen `width` pixels wide.
pub fn ticker(th: &Theme, label: &str, text: &str, width: u32) -> Ticker {
    let label: String = label.trim().chars().take(16).collect();
    let line = format!("{}        •        ", text.replace('\n', " "));
    if th.modern {
        let k = width as f32 / 1280.0;
        let h = (54.0 * k).round().max(28.0) as usize;
        let mut strip = Rgba::new(width as usize, h);
        strip.fill(0, 0, width as i32, h as i32, th.dark, 240);
        strip.fill(
            0,
            0,
            width as i32,
            ((3.0 * k) as i32).max(2),
            th.accent,
            255,
        );
        let c_px = 30.0 * k;
        let cw = (text::width(Face::Bold, &line, c_px) as usize + 8).max(16);
        let mut crawl = Rgba::new(cw, (c_px * 1.3) as usize);
        crawl.text(4, 0, &line, c_px, Face::Bold, th.ink);
        let chip = (!label.is_empty()).then(|| {
            let l_px = 24.0 * k;
            let w = text::width(Face::Bold, &label, l_px) as usize + (40.0 * k) as usize;
            let mut chip = Rgba::new(w, h);
            chip.fill(0, 0, w as i32, h as i32, th.accent, 255);
            chip.text(
                (20.0 * k) as i32,
                ((h as f32 - l_px * 1.2) / 2.0) as i32,
                &label,
                l_px,
                Face::Bold,
                th.on_accent,
            );
            chip.png()
        });
        return Ticker {
            strip: strip.png(),
            chip,
            crawl: crawl.png(),
            height: h as u32,
        };
    }
    // The pixel look: drawn in 640-wide points and scaled with the picture.
    let scale = (width / 640).max(1) as i32;
    let h = 18 * scale;
    let mut strip = Canvas::new(width as usize, h as usize);
    strip.fill(th.dark);
    strip.rect(0, 0, width as i32, scale.max(1), th.accent);
    let mut crawl = Canvas::new(
        (text_width(&line, scale * 2) + 8).max(16) as usize,
        (8 * scale * 2) as usize,
    );
    crawl.fill(KEY);
    crawl.text(4, 0, &line, scale * 2, th.ink);
    let chip = (!label.is_empty()).then(|| {
        let up = label.to_uppercase();
        let w = text_width(&up, scale) + 16 * scale;
        let mut c = Canvas::new(w as usize, h as usize);
        c.fill(th.accent);
        c.text(8 * scale, (h - 8 * scale) / 2, &up, scale, th.on_accent);
        canvas_graphic(&c, 255)
    });
    Ticker {
        strip: canvas_graphic(&strip, 255),
        chip,
        crawl: canvas_graphic(&crawl, 255),
        height: h as u32,
    }
}

/// How the ticker looks over a sample picture, 1280x720.
pub fn ticker_preview(th: &Theme, label: &str, text: &str, top: bool, t: f32) -> Canvas {
    let mut c = sample_picture();
    let k = ticker(th, label, text, 1280);
    let y = if top { 0 } else { 720 - k.height as i32 };
    blit_png(&mut c, &k.strip, 0, y);
    let cy = y + (k.height as i32 - k.crawl.h as i32) / 2;
    let cx = 1280 - ((t * 140.0) as i32 % (1280 + k.crawl.w as i32));
    blit_png(&mut c, &k.crawl, cx, cy);
    if let Some(chip) = &k.chip {
        blit_png(&mut c, chip, 0, y);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> Vec<Theme> {
        let mut m = Theme::for_channel("tick");
        m.modern = true;
        vec![Theme::for_channel("tick"), m]
    }

    #[test]
    fn the_strip_the_tab_and_the_text_are_drawn_for_every_width_and_style() {
        for th in themes() {
            for width in [640u32, 854, 1280, 1920] {
                let t = ticker(
                    &th,
                    "News",
                    "Cheers is on at 8. The bakery opens at 6. Power cut on Elm St.",
                    width,
                );
                assert_eq!(t.strip.w, width);
                assert_eq!(t.strip.h, t.height);
                assert!(
                    t.height >= 18 && t.height < width / 8,
                    "{} at {width}",
                    t.height
                );
                assert!(t.crawl.w > 100 && t.crawl.h > 8);
                let chip = t.chip.expect("a label gives a tab");
                assert_eq!(chip.h, t.height);
                assert!(
                    ticker(&th, "", "x", width).chip.is_none(),
                    "no label, no tab"
                );
            }
        }
    }

    #[test]
    fn the_preview_shows_the_strip_where_asked_and_the_text_moves() {
        let th = &themes()[1];
        let bottom = ticker_preview(th, "NEWS", "Something is happening", false, 1.0);
        let top = ticker_preview(th, "NEWS", "Something is happening", true, 1.0);
        assert_ne!(bottom.px, top.px);
        assert_ne!(
            ticker_preview(th, "NEWS", "Something is happening", false, 1.0).px,
            ticker_preview(th, "NEWS", "Something is happening", false, 3.0).px
        );
        assert_eq!((bottom.w, bottom.h), (1280, 720));
        // Very long and odd text does not panic.
        let long = "é日本語😀 ".repeat(200);
        let _ = ticker(&themes()[0], &long, &long, 1280);
        let _ = ticker(&themes()[1], &long, &long, 1280);
    }
}
