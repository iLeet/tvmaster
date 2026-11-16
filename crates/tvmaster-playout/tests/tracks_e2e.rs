//! Real FFmpeg, real files with several audio tracks and subtitles: the
//! channel's choices must show up in what viewers actually receive.
//! Frequencies, brightness and pixel colours are measured on the HLS output.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tvmaster_core::{ChannelOptions, Look, Quality, SignOffStyle, SubMode, Tracks};
use tvmaster_media::FfmpegInfo;
use tvmaster_playout::{
    ChannelSpec, EngineConfig, Manager, PlayItem, PlayKind, ScheduleSource, Upcoming,
};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn ffmpeg(args: &[&str]) -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .status()
        .unwrap()
        .success()
}

/// A 14-second file: eng 440 Hz, jpn 880 Hz, an English commentary at 1500 Hz,
/// and (with `subs`) an English subtitle track and an `.srt` saying SUBTITLES.
fn make_media(dir: &Path, name: &str, black: bool, subs: bool) -> PathBuf {
    let srt = dir.join("subs.srt");
    std::fs::write(
        &srt,
        "1\n00:00:00,500 --> 00:00:13,500\nHELLO SUBTITLES HELLO SUBTITLES\n",
    )
    .unwrap();
    let out = dir.join(name);
    let video = if black {
        "color=c=black:s=640x360:r=25:d=14"
    } else {
        "testsrc=size=640x360:rate=25:duration=14"
    };
    let mut a: Vec<&str> = vec![
        "-f",
        "lavfi",
        "-i",
        video,
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=14",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=880:duration=14",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=1500:duration=14",
    ];
    if subs {
        a.extend(["-i", srt.to_str().unwrap()]);
    }
    a.extend(["-map", "0:v", "-map", "1:a", "-map", "2:a", "-map", "3:a"]);
    if subs {
        a.extend([
            "-map",
            "4:s",
            "-c:s",
            "srt",
            "-metadata:s:s:0",
            "language=eng",
        ]);
    }
    a.extend([
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:1",
        "language=jpn",
        "-metadata:s:a:2",
        "language=eng",
        "-metadata:s:a:2",
        "title=Director's Commentary",
        "-disposition:a:2",
        "comment",
        "-disposition:a:0",
        "default",
    ]);
    a.push(out.to_str().unwrap());
    assert!(ffmpeg(&a), "could not make the test file");
    out
}

struct One {
    items: Vec<PlayItem>,
    unplayable: Mutex<Vec<i64>>,
    defaults: Option<tvmaster_core::ShowOptions>,
}
impl ScheduleSource for One {
    fn item_at_or_after(&self, _c: &str, at_ms: i64) -> Option<PlayItem> {
        self.items.iter().find(|i| i.end_ms > at_ms).cloned()
    }
    fn report_unplayable(&self, _c: &str, id: i64) {
        self.unplayable.lock().unwrap().push(id);
    }
    fn upcoming(&self, _c: &str, _a: i64, _l: usize) -> Upcoming {
        Upcoming::default()
    }
    fn show_defaults(&self, _c: &str, _show: &str) -> Option<tvmaster_core::ShowOptions> {
        self.defaults.clone()
    }
}

struct Rig {
    manager: Arc<Manager>,
    root: PathBuf,
    media: PathBuf,
    info: FfmpegInfo,
}

fn rig(test: &str) -> Option<Rig> {
    let info = FfmpegInfo::detect("ffmpeg", "ffprobe").ok()?;
    if !info.has_libx264 {
        return None;
    }
    let root = std::env::temp_dir().join(format!("tvmaster-tracks-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let media = root.join("media");
    std::fs::create_dir_all(&media).unwrap();
    let mut cfg = EngineConfig::new(&info.ffmpeg, tvmaster_media::Encoder::Software, &root);
    cfg.readrate_burst = info.has_readrate_burst;
    if info.has_drawtext {
        cfg.slate_font = tvmaster_media::find_system_font();
    }
    Some(Rig {
        manager: Manager::new(cfg),
        root,
        media,
        info,
    })
}

fn show_item(path: &Path, start_in_ms: i64, secs: i64) -> PlayItem {
    // What the library scan stores: the file's tracks plus subtitle files beside it.
    let mut tracks = tvmaster_media::probe_with(Path::new("ffprobe"), path)
        .unwrap()
        .tracks;
    tracks.external = tvmaster_media::find_sidecar_subs(path);
    let start = now_ms() + start_in_ms;
    PlayItem {
        kind: PlayKind::Media,
        media_id: Some(1),
        path: Some(path.to_path_buf()),
        has_audio: true,
        in_ms: 0,
        subject: Some("Cowboy Bebop".into()),
        start_ms: start,
        end_ms: start + secs * 1000,
        is_show: true,
        show: Some("Cowboy Bebop".into()),
        subtitle: Some("S01E02 - Honky Tonk Women".into()),
        tracks: Some(tracks),
    }
}

impl Rig {
    /// Put `items` on air with `options` and wait until about `secs` seconds of stream exist.
    async fn air(
        &self,
        items: Vec<PlayItem>,
        options: ChannelOptions,
        signoff: SignOffStyle,
        secs: u64,
    ) -> (Arc<One>, PathBuf) {
        self.air_with(items, options, signoff, secs, None).await
    }

    /// Like `air`, with settings that follow the show onto every channel.
    async fn air_with(
        &self,
        items: Vec<PlayItem>,
        options: ChannelOptions,
        signoff: SignOffStyle,
        secs: u64,
        defaults: Option<tvmaster_core::ShowOptions>,
    ) -> (Arc<One>, PathBuf) {
        let source = Arc::new(One {
            items,
            unplayable: Mutex::new(Vec::new()),
            defaults,
        });
        let spec = ChannelSpec {
            id: "t".into(),
            name: "Test TV".into(),
            always_on: true,
            source: source.clone(),
            guide: None,
            options,
            number: 7,
            can_burn_text: self.info.has_subtitles,
            signoff,
        };
        assert!(self.manager.start_if_absent(spec));
        self.manager.touch("t");
        let playlist = self.manager.hls_dir("t").join("index.m3u8");
        let t = Instant::now();
        tokio::time::sleep(Duration::from_secs(secs)).await;
        while !std::fs::read_to_string(&playlist)
            .unwrap_or_default()
            .contains("#EXTINF")
        {
            assert!(t.elapsed() < Duration::from_secs(60), "no stream");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        (source, self.frozen(&playlist))
    }

    /// A finished copy of the live playlist, so FFmpeg reads what exists and stops.
    fn frozen(&self, playlist: &Path) -> PathBuf {
        let mut text = std::fs::read_to_string(playlist).unwrap();
        text.push_str("#EXT-X-ENDLIST\n");
        let f = playlist.with_file_name("frozen.m3u8");
        std::fs::write(&f, text).unwrap();
        f
    }

    fn finish(self) {
        self.manager.stop("t");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Dominant frequency of the audio between `from` and `to` seconds, by zero crossings.
fn pitch(playlist: &Path, from: f64, to: f64) -> f64 {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(playlist)
        .args(["-vn", "-ac", "1", "-ar", "48000", "-f", "s16le", "-"])
        .output()
        .unwrap();
    let pcm: Vec<i16> = out
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    let (a, b) = (
        (from * 48000.0) as usize,
        ((to * 48000.0) as usize).min(pcm.len()),
    );
    assert!(b > a + 4800, "not enough audio: {} samples", pcm.len());
    let seg = &pcm[a..b];
    let crossings = seg.windows(2).filter(|w| (w[0] < 0) != (w[1] < 0)).count();
    crossings as f64 / 2.0 / ((b - a) as f64 / 48000.0)
}

/// Mean brightness (0-255) of a region of the picture, once a second.
fn brightness_series(playlist: &Path, crop: &str) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(playlist)
        .args([
            "-an",
            "-vf",
            &format!("fps=1,{crop},scale=1:1:flags=area,format=gray"),
            "-f",
            "rawvideo",
            "-",
        ])
        .output()
        .unwrap();
    out.stdout
}

fn assert_near(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() < want * 0.06,
        "{what}: expected about {want} Hz, measured {got:.0} Hz"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_channels_language_decides_which_audio_track_airs() {
    let Some(rig) = rig("audio") else { return };
    let file = make_media(&rig.media, "multi.mkv", false, false);
    let tracks: Tracks = tvmaster_media::probe_with(Path::new("ffprobe"), &file)
        .unwrap()
        .tracks;
    assert_eq!(tracks.audio.len(), 3, "probing sees all three tracks");
    assert!(tracks.audio[2].commentary);

    // Japanese first.
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["jpn".into(), "eng".into()];
    let (_, playlist) = rig
        .air(vec![show_item(&file, 0, 14)], opts, SignOffStyle::Bars, 9)
        .await;
    assert_near(pitch(&playlist, 2.0, 6.0), 880.0, "Japanese channel");
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // English, and the commentary (also English, 1500 Hz) must not be chosen.
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["English".into()];
    let (_, playlist) = rig
        .air(vec![show_item(&file, 0, 14)], opts, SignOffStyle::Bars, 9)
        .await;
    assert_near(
        pitch(&playlist, 2.0, 6.0),
        440.0,
        "English channel skips the commentary",
    );
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // A per-show override beats the channel's language.
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["eng".into()];
    opts.shows.insert(
        "cowboy bebop".into(),
        tvmaster_core::ShowOptions {
            audio_languages: Some(vec!["jpn".into()]),
            ..Default::default()
        },
    );
    let (_, playlist) = rig
        .air(vec![show_item(&file, 0, 14)], opts, SignOffStyle::Bars, 9)
        .await;
    assert_near(pitch(&playlist, 2.0, 6.0), 880.0, "show override");
    rig.finish();
}

#[tokio::test(flavor = "multi_thread")]
async fn subtitles_are_burned_in_only_when_the_mode_says_so() {
    let Some(rig) = rig("subs") else { return };
    if !rig.info.has_subtitles {
        eprintln!("FFmpeg has no libass here; the fall-back test covers this machine");
        return;
    }
    let file = make_media(&rig.media, "subbed.mkv", true, true);
    let bottom = "crop=1000:140:140:520";

    // Always: bright text on black.
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["jpn".into()];
    opts.subtitles.mode = SubMode::Always;
    opts.subtitles.languages = vec!["eng".into()];
    let (_, playlist) = rig
        .air(vec![show_item(&file, 0, 14)], opts, SignOffStyle::Bars, 9)
        .await;
    let with = brightness_series(&playlist, bottom);
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Off: the same picture stays black.
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["jpn".into()];
    let (_, playlist) = rig
        .air(vec![show_item(&file, 0, 14)], opts, SignOffStyle::Bars, 9)
        .await;
    let without = brightness_series(&playlist, bottom);

    let peak = |v: &[u8]| v.iter().copied().max().unwrap_or(0);
    assert!(
        peak(&with) >= 6,
        "subtitles should light the bottom of a black picture: {with:?}"
    );
    assert!(peak(&without) <= 2, "no subtitles, no text: {without:?}");
    rig.finish();
}

/// Where FFmpeg lacks libass the channel must still air the programme: the item
/// fails with its subtitle filter, is retried without, and the file is not blamed.
#[tokio::test(flavor = "multi_thread")]
async fn a_failing_extra_never_takes_the_programme_off_the_air() {
    let Some(rig) = rig("fallback") else { return };
    if rig.info.has_subtitles {
        return; // the real filter works here; the other test covers it
    }
    let file = make_media(&rig.media, "subbed.mkv", true, true);
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["jpn".into()];
    opts.subtitles.mode = SubMode::Always;
    opts.subtitles.languages = vec!["eng".into()];
    // Claim text subtitles work, as a machine with a half-working libass would.
    let source = Arc::new(One {
        items: vec![show_item(&file, 0, 14)],
        unplayable: Mutex::new(Vec::new()),
        defaults: None,
    });
    let spec = ChannelSpec {
        id: "t".into(),
        name: "Test TV".into(),
        always_on: true,
        source: source.clone(),
        guide: None,
        options: opts,
        number: 7,
        can_burn_text: true,
        signoff: SignOffStyle::Bars,
    };
    assert!(rig.manager.start_if_absent(spec));
    rig.manager.touch("t");
    let playlist = rig.manager.hls_dir("t").join("index.m3u8");
    tokio::time::sleep(Duration::from_secs(9)).await;
    assert!(
        std::fs::read_to_string(&playlist)
            .unwrap_or_default()
            .contains("#EXTINF"),
        "the stream is on air"
    );
    assert_near(
        pitch(&rig.frozen(&playlist), 2.0, 6.0),
        880.0,
        "the programme aired, with the right audio",
    );
    assert!(
        source.unplayable.lock().unwrap().is_empty(),
        "the file was not reported unplayable"
    );
    rig.finish();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_channel_bug_the_programme_card_and_the_look_are_on_the_picture() {
    let Some(rig) = rig("graphics") else { return };
    let black = make_media(&rig.media, "black.mkv", true, false);

    // Bug in the top-right corner for the whole programme; a title card for the first 4 seconds.
    let mut opts = ChannelOptions::default();
    opts.overlay.bug.enabled = true;
    opts.overlay.bug.text = "ANIME 7".into();
    opts.overlay.show_title.enabled = true;
    opts.overlay.show_title.seconds = 4;
    // 1.5 s from now: the runner covers the gap with the slate, so the item starts exactly on time and gets its title card.
    let (_, playlist) = rig
        .air(
            vec![show_item(&black, 1500, 14)],
            opts,
            SignOffStyle::Bars,
            14,
        )
        .await;
    let bug = brightness_series(&playlist, "crop=124:28:1132:24");
    let card = brightness_series(&playlist, "crop=560:56:44:612");
    assert!(
        bug.iter().filter(|b| **b >= 6).count() >= 8,
        "the logo should be on most seconds: {bug:?}"
    );
    let lit: Vec<usize> = card
        .iter()
        .enumerate()
        .filter(|(_, b)| **b >= 10)
        .map(|(i, _)| i)
        .collect();
    assert!(
        !lit.is_empty() && lit.len() <= 7,
        "the title card shows for a few seconds only: {card:?}"
    );
    assert!(
        card.iter().rev().take(3).all(|b| *b < 6),
        "and is gone by the end: {card:?}"
    );
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Black and white: every pixel is grey, even on a colourful picture.
    let colourful = make_media(&rig.media, "colour.mkv", false, false);
    let mut opts = ChannelOptions::default();
    opts.video.look = Look::BlackWhite;
    let (_, playlist) = rig
        .air(
            vec![show_item(&colourful, 0, 14)],
            opts,
            SignOffStyle::Bars,
            9,
        )
        .await;
    let rgb = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&playlist)
        .args([
            "-an",
            "-frames:v",
            "1",
            "-ss",
            "3",
            "-vf",
            "scale=64:36,format=rgb24",
            "-f",
            "rawvideo",
            "-",
        ])
        .output()
        .unwrap()
        .stdout;
    assert_eq!(rgb.len(), 64 * 36 * 3);
    let max_spread = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| p.iter().max().unwrap() - p.iter().min().unwrap())
        .max()
        .unwrap();
    assert!(
        max_spread <= 12,
        "a black-and-white picture has no colour (max channel spread {max_spread})"
    );
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Without the look the same file really is colourful (so the check above means something).
    let (_, playlist) = rig
        .air(
            vec![show_item(&colourful, 0, 14)],
            ChannelOptions::default(),
            SignOffStyle::Bars,
            9,
        )
        .await;
    let rgb = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&playlist)
        .args([
            "-an",
            "-frames:v",
            "1",
            "-ss",
            "3",
            "-vf",
            "scale=64:36,format=rgb24",
            "-f",
            "rawvideo",
            "-",
        ])
        .output()
        .unwrap()
        .stdout;
    let colour_spread = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| p.iter().max().unwrap() - p.iter().min().unwrap())
        .max()
        .unwrap();
    assert!(colour_spread > 60, "{colour_spread}");
    rig.finish();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_channels_quality_sets_the_size_of_what_it_streams() {
    let Some(rig) = rig("quality") else { return };
    let file = make_media(&rig.media, "q.mkv", false, false);
    let mut opts = ChannelOptions::default();
    opts.video.quality = Quality::P480;
    opts.video.fps = Some(24);
    let (_, playlist) = rig
        .air(vec![show_item(&file, 0, 14)], opts, SignOffStyle::Bars, 8)
        .await;
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(&playlist)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    assert!(text.starts_with("854,480,24/1"), "got {text}");
    rig.finish();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_station_signs_off_with_bars_and_a_tone() {
    let Some(rig) = rig("signoff") else { return };
    let start = now_ms();
    let item = PlayItem {
        kind: PlayKind::SignOff,
        media_id: None,
        path: None,
        has_audio: false,
        in_ms: 0,
        subject: None,
        start_ms: start,
        end_ms: start + 30_000,
        is_show: false,
        show: None,
        subtitle: None,
        tracks: None,
    };
    let (_, playlist) = rig
        .air(vec![item], ChannelOptions::default(), SignOffStyle::Bars, 8)
        .await;
    assert_near(pitch(&playlist, 1.5, 5.0), 1000.0, "sign-off tone");
    let rgb = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&playlist)
        .args([
            "-an",
            "-frames:v",
            "1",
            "-ss",
            "3",
            "-vf",
            "scale=64:36,format=rgb24",
            "-f",
            "rawvideo",
            "-",
        ])
        .output()
        .unwrap()
        .stdout;
    // SMPTE bars: a white-ish bar and a saturated bar both appear
    let spread = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| p.iter().max().unwrap() - p.iter().min().unwrap())
        .max()
        .unwrap();
    assert!(spread > 150, "colour bars are very colourful ({spread})");
    rig.finish();
}

/// A subtitle file beside a video that has none inside it is found by the scan and used.
#[tokio::test(flavor = "multi_thread")]
async fn a_subtitle_file_beside_the_video_is_burned_in() {
    let Some(rig) = rig("sidecar") else { return };
    if !rig.info.has_subtitles {
        return;
    }
    let file = make_media(&rig.media, "plain.mkv", true, false);
    std::fs::write(
        rig.media.join("plain.en.srt"),
        "1\n00:00:00,500 --> 00:00:13,500\nSIDECAR SUBTITLES SIDECAR\n",
    )
    .unwrap();
    let item = show_item(&file, 0, 14);
    let t = item.tracks.as_ref().unwrap();
    assert!(t.subs.is_empty());
    assert_eq!(t.external.len(), 1);
    assert_eq!(t.external[0].lang, "eng");

    let mut opts = ChannelOptions::default();
    opts.subtitles.mode = SubMode::Always;
    opts.subtitles.languages = vec!["eng".into()];
    let (_, playlist) = rig.air(vec![item], opts, SignOffStyle::Bars, 9).await;
    let lit = brightness_series(&playlist, "crop=1000:140:140:520");
    assert!(lit.iter().copied().max().unwrap_or(0) >= 6, "{lit:?}");
    rig.finish();
}

/// Settings made once for a show reach every channel that airs it, and the
/// channel's own override for that show still wins.
#[tokio::test(flavor = "multi_thread")]
async fn a_shows_own_settings_follow_it_onto_the_channel() {
    let Some(rig) = rig("showdefaults") else {
        return;
    };
    let file = make_media(&rig.media, "multi.mkv", false, false);
    let defaults = tvmaster_core::ShowOptions {
        audio_languages: Some(vec!["jpn".into()]),
        ..Default::default()
    };
    // The channel says English; the show's default says Japanese: Japanese airs.
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["eng".into()];
    let (_, playlist) = rig
        .air_with(
            vec![show_item(&file, 0, 14)],
            opts,
            SignOffStyle::Bars,
            9,
            Some(defaults.clone()),
        )
        .await;
    assert_near(pitch(&playlist, 2.0, 6.0), 880.0, "the show's default");
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // This channel's own override for the show beats the show's default.
    let mut opts = ChannelOptions::default();
    opts.audio.languages = vec!["jpn".into()];
    opts.shows.insert(
        "Cowboy Bebop".into(),
        tvmaster_core::ShowOptions {
            audio_languages: Some(vec!["eng".into()]),
            ..Default::default()
        },
    );
    let (_, playlist) = rig
        .air_with(
            vec![show_item(&file, 0, 14)],
            opts,
            SignOffStyle::Bars,
            9,
            Some(defaults),
        )
        .await;
    assert_near(pitch(&playlist, 2.0, 6.0), 440.0, "the channel's override");
    rig.finish();
}

fn first_frame_rgb(playlist: &Path, at: &str) -> Vec<u8> {
    Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(playlist)
        .args([
            "-an",
            "-ss",
            at,
            "-frames:v",
            "1",
            "-vf",
            "scale=64:36,format=rgb24",
            "-f",
            "rawvideo",
            "-",
        ])
        .output()
        .unwrap()
        .stdout
}

fn spread(rgb: &[u8]) -> u8 {
    rgb.as_chunks::<3>()
        .0
        .iter()
        .map(|p| p.iter().max().unwrap() - p.iter().min().unwrap())
        .max()
        .unwrap_or(0)
}

fn mean(rgb: &[u8]) -> f64 {
    rgb.iter().map(|b| *b as f64).sum::<f64>() / rgb.len().max(1) as f64
}

/// With nothing scheduled, the channel shows what its owner chose.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_channel_shows_the_chosen_off_air_screen() {
    use tvmaster_core::Screen;
    let Some(rig) = rig("offair") else { return };

    // Colour bars with a tone: very colourful, and a 1 kHz tone.
    let mut opts = ChannelOptions::default();
    opts.off_air.screen = Screen::Bars;
    opts.off_air.tone = true;
    let (_, playlist) = rig.air(vec![], opts, SignOffStyle::Bars, 9).await;
    assert!(
        spread(&first_frame_rgb(&playlist, "3")) > 150,
        "colour bars"
    );
    assert_near(pitch(&playlist, 2.0, 6.0), 1000.0, "the test tone");
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Black: nothing but black and silence.
    let mut opts = ChannelOptions::default();
    opts.off_air.screen = Screen::Black;
    let (_, playlist) = rig.air(vec![], opts, SignOffStyle::Bars, 9).await;
    assert!(mean(&first_frame_rgb(&playlist, "3")) < 6.0, "black");
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The station card (the default): the channel's colours and its words, no tone needed, no drawtext needed.
    let mut opts = ChannelOptions::default();
    opts.off_air.message = "BACK AT NINE".into();
    let (_, playlist) = rig.air(vec![], opts, SignOffStyle::Bars, 9).await;
    let card = first_frame_rgb(&playlist, "3");
    assert!(
        mean(&card) > 12.0 && spread(&card) > 40,
        "a coloured card with text, not a blank slate"
    );
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // A picture of the owner's own: a solid red PNG from the same encoder the graphics use.
    let mut rgba = Vec::new();
    for _ in 0..(64 * 36) {
        rgba.extend([220u8, 20, 20, 255]);
    }
    let pic = rig.root.join("mine.png");
    std::fs::write(&pic, tvmaster_render::png::rgba_png(64, 36, &rgba)).unwrap();
    let mut opts = ChannelOptions::default();
    opts.off_air.screen = Screen::Image;
    opts.off_air.image = Some(pic.to_string_lossy().to_string());
    let (_, playlist) = rig.air(vec![], opts, SignOffStyle::Bars, 9).await;
    let frame = first_frame_rgb(&playlist, "3");
    let centre = &frame[(18 * 64 + 32) * 3..(18 * 64 + 32) * 3 + 3];
    assert!(
        centre[0] > 150 && centre[1] < 90 && centre[2] < 90,
        "the picture fills the screen: {centre:?}"
    );
    rig.manager.stop("t");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // A picture that has gone missing: the card, not a dead channel.
    let mut opts = ChannelOptions::default();
    opts.off_air.screen = Screen::Image;
    opts.off_air.image = Some("/definitely/not/here.png".into());
    let (_, playlist) = rig.air(vec![], opts, SignOffStyle::Bars, 9).await;
    assert!(
        mean(&first_frame_rgb(&playlist, "3")) > 12.0,
        "falls back to the card"
    );
    rig.finish();
}
