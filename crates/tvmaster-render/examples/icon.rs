//! Draws the app icon: `cargo run -p tvmaster-render --example icon -- icon.ppm`.
use tvmaster_render::{Canvas, Theme};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "icon.ppm".into());
    let th = Theme::from_hue(0.62);
    let n = 512;
    let mut c = Canvas::new(n, n);
    c.vgradient(th.primary, mix(th.dark, [10, 10, 30]));
    // a stylised screen with a stripe band, like the bumpers
    c.rect(86, 120, 340, 232, [8, 8, 16]);
    c.rect(98, 132, 316, 208, th.dark);
    for y in 300..336 {
        for x in 98..414 {
            let on = ((x + (y - 300) * 2) / 18) % 2 == 0;
            c.rect(x, y, 1, 1, if on { th.primary } else { th.accent });
        }
    }
    c.text(150, 160, "TV", 18, [255, 255, 255]);
    c.rect(226, 360, 60, 22, [8, 8, 16]);
    c.rect(176, 382, 160, 14, [8, 8, 16]);
    std::fs::write(out, c.to_ppm()).expect("write icon");
}

fn mix(a: [u8; 3], b: [u8; 3]) -> [u8; 3] {
    [
        (a[0] / 2 + b[0] / 2),
        (a[1] / 2 + b[1] / 2),
        (a[2] / 2 + b[2] / 2),
    ]
}
