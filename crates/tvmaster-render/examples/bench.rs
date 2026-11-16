//! How long one guide frame takes to draw: `cargo run --release -p tvmaster-render --example bench`.
use std::time::Instant;
use tvmaster_render::{
    render_guide, GuideCell, GuideData, GuideLook, GuideRow, GuideView, Palette,
};

fn main() {
    let start = 1_000_000_000_000_i64;
    let half = 30 * 60_000;
    let data = GuideData {
        title: "Retro TV Guide".into(),
        now_ms: start + 12 * 60_000,
        window_start_ms: start,
        window_ms: 3 * 60 * 60_000,
        clock: "9:12 PM".into(),
        date: "SUN NOV 15".into(),
        marks: (0..6)
            .map(|i| {
                (
                    start + i * half,
                    format!("{}:{:02} PM", 9 + i / 2, (i % 2) * 30),
                )
            })
            .collect(),
        rows: (0..12)
            .map(|r| GuideRow {
                id: format!("c{r}"),
                number: r as u32 + 2,
                name: "Some Channel".into(),
                cells: (0..7)
                    .map(|i| GuideCell {
                        start_ms: start - half + i * half,
                        end_ms: start + i * half,
                        title: "Murder She Wrote".into(),
                        range: "9:00-9:30 PM".into(),
                    })
                    .collect(),
            })
            .collect(),
        ticker: "NOW ON 4: Taxi * NOW ON 5: Cheers".into(),
    };
    for (name, smooth) in [("pixel 640x360", false), ("smooth 1280x720", true)] {
        let mut look = GuideLook::new(Palette::midnight());
        look.smooth_text = smooth;
        look.rounded = smooth;
        look.cell_times = true;
        let n = 60;
        let t0 = Instant::now();
        for i in 0..n {
            let c = render_guide(&data, &look, &GuideView::default(), i as f32 / 15.0);
            std::hint::black_box(&c);
        }
        let per = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("{name}: {per:.1} ms a frame (budget 66 ms at 15 fps)");
    }
}
