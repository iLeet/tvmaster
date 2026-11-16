//! Writes one PPM frame per bumper to the directory given as the first
//! argument: `cargo run -p tvmaster-render --example preview -- /tmp/out`.
use tvmaster_render::{
    render, render_guide, GuideCell, GuideData, GuideLook, GuideRow, GuideView, Palette, Placement,
    Scene, Theme, WindowLook, WindowSize,
};

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
            id: format!("ch{r}"),
            number: 4 + r as u32,
            name: ["Retro TV", "Toon Town", "Movie Vault", "Sitcom Central"][r % 4].into(),
            cells: (0..7)
                .map(|i| GuideCell {
                    start_ms: start - half + i * half,
                    end_ms: start + i * half,
                    title: shows[(r + i as usize) % shows.len()].into(),
                    range: format!("{}:{:02} PM", 9 + i / 2, (i % 2) * 30),
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
    let auto = Palette::from_theme(&theme);
    let win = |placement, size| WindowLook {
        placement,
        size,
        info: true,
    };
    let mut looks: Vec<(&str, GuideLook, GuideView)> = Vec::new();
    looks.push(("guide_a", GuideLook::new(auto), GuideView::default()));
    for (n, p) in [
        ("prevue", Palette::prevue()),
        ("amber", Palette::amber()),
        ("phosphor", Palette::phosphor()),
        ("neon", Palette::neon()),
        ("sunset", Palette::sunset()),
        ("ocean", Palette::ocean()),
        ("slate", Palette::slate()),
        ("paper", Palette::paper()),
    ] {
        looks.push((
            Box::leak(format!("guide_{n}").into_boxed_str()),
            GuideLook::new(p),
            GuideView::default(),
        ));
    }
    let mut nn = GuideLook::new(Palette::prevue());
    nn.now_next = true;
    nn.progress = true;
    nn.rows = 5;
    looks.push(("guide_nownext", nn, GuideView::default()));
    let live = GuideView {
        window_row: Some(1),
        window_live: false,
    };
    for (n, pl, sz) in [
        ("side_right", Placement::SideRight, WindowSize::Medium),
        ("side_left_large", Placement::SideLeft, WindowSize::Large),
        ("top_left", Placement::TopLeft, WindowSize::Medium),
        ("top_right_small", Placement::TopRight, WindowSize::Small),
    ] {
        let mut l = GuideLook::new(Palette::prevue());
        l.window = Some(win(pl, sz));
        looks.push((
            Box::leak(format!("guide_pip_{n}").into_boxed_str()),
            l,
            live,
        ));
    }
    // The smooth looks, drawn at 1280x720.
    for (n, pal) in [
        ("midnight", Palette::midnight()),
        ("snow", Palette::snow()),
        ("graphite", Palette::graphite()),
    ] {
        let mut l = GuideLook::new(pal);
        l.smooth_text = true;
        l.rounded = true;
        l.scanlines = false;
        l.cell_times = n == "snow";
        l.header_minimal = n == "graphite";
        looks.push((
            Box::leak(format!("modern_{n}").into_boxed_str()),
            l,
            GuideView::default(),
        ));
    }
    let mut m = GuideLook::new(Palette::midnight());
    m.smooth_text = true;
    m.rounded = true;
    m.scanlines = false;
    m.now_next = true;
    m.progress = true;
    m.rows = 5;
    m.window = Some(win(Placement::SideRight, WindowSize::Medium));
    looks.push(("modern_nownext_pip", m, live));
    for (name, look, view) in &looks {
        std::fs::write(
            format!("{dir}/{name}.ppm"),
            render_guide(&guide, look, view, 3.0).to_ppm(),
        )
        .expect("write guide frame");
    }
    std::fs::write(
        format!("{dir}/guide_b.ppm"),
        render_guide(&guide, &looks[0].1, &GuideView::default(), 12.0).to_ppm(),
    )
    .expect("write guide frame");
    for (file, scene) in scenes {
        let c = render(&scene, &theme, 2.0, 6.0);
        std::fs::write(format!("{dir}/{file}.ppm"), c.to_ppm()).expect("write frame");
    }
}
