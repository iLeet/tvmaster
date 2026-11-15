//! Writes one PPM frame per bumper to the directory given as the first
//! argument: `cargo run -p tvmaster-render --example preview -- /tmp/out`.
use tvmaster_render::{render, Scene, Theme};

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
    for (file, scene) in scenes {
        let c = render(&scene, &theme, 2.0, 6.0);
        std::fs::write(format!("{dir}/{file}.ppm"), c.to_ppm()).expect("write frame");
    }
}
