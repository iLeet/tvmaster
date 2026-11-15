//! Writes one PPM frame per bumper to the directory given as the first
//! argument: `cargo run -p tvmaster-render --example preview -- /tmp/out`.
use tvmaster_render::{render, render_guide, GuideCell, GuideData, GuideRow, Scene, Theme};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let name = "Retro TV".to_string();
    let theme = Theme::for_channel("retro");
    let scenes = [
        ("station_id", Scene::StationId { name: name.clone() }),
        (
            "brb",
            Scene::Brb {
                name: name.clone(),
                show: "Cheers".into(),
            },
        ),
        (
            "back_to",
            Scene::BackTo {
                name: name.clone(),
                show: "Cheers".into(),
            },
        ),
        (
            "up_next",
            Scene::UpNext {
                name: name.clone(),
                title: "The Adventures of Brisco County Jr".into(),
                time: "8:30 PM".into(),
            },
        ),
        (
            "later_today",
            Scene::LaterToday {
                name,
                evening: true,
                items: vec![
                    ("8:30 PM".into(), "Taxi".into()),
                    ("9:00 PM".into(), "Cheers".into()),
                    ("9:30 PM".into(), "Night Court".into()),
                    ("10:00 PM".into(), "The Late Late Show With Somebody".into()),
                ],
            },
        ),
    ];
    let start = 1_000_000_000_000_i64;
    let half = 30 * 60_000;
    let shows = [
        "Taxi",
        "Cheers",
        "Night Court",
        "The Simpsons",
        "Looney Tunes",
        "Knight Rider",
        "Magnum PI",
        "Murder She Wrote",
    ];
    let rows = (0..8)
        .map(|r| GuideRow {
            number: 4 + r as u32,
            name: ["Retro TV", "Toon Town", "Movie Vault", "Sitcom Central"][r % 4].into(),
            cells: (0..7)
                .map(|i| GuideCell {
                    start_ms: start - half + i * half,
                    end_ms: start + i * half,
                    title: shows[(r + i as usize) % shows.len()].into(),
                })
                .collect(),
        })
        .collect();
    let guide = GuideData {
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
        rows,
        ticker: "Now on channel 4: Taxi * Up next: Cheers * Stay tuned".into(),
    };
    for (name, t) in [("guide_a", 3.0), ("guide_b", 12.0)] {
        std::fs::write(
            format!("{dir}/{name}.ppm"),
            render_guide(&guide, &theme, t).to_ppm(),
        )
        .expect("write guide frame");
    }
    for (file, scene) in scenes {
        let c = render(&scene, &theme, 2.0, 6.0);
        std::fs::write(format!("{dir}/{file}.ppm"), c.to_ppm()).expect("write frame");
    }
}
