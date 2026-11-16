//! FFmpeg command construction: the per-item encoder and the HLS segmenter.
//! Kept free of process spawning so the argument lists are unit-testable.

use crate::looks::{
    audio_chain, clock_filter, look_chain, scale_chain, subtitle_filter, Burn, ItemPlan,
};
use std::path::Path;
use tvmaster_core::VideoOptions;
use tvmaster_media::Encoder;

/// The normalized output every item is converted to, so mixed sources
/// (4:3 retro ads, 1080p shows, no-audio clips) all join seamlessly.
#[derive(Debug, Clone)]
pub struct OutputProfile {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_kbps: u32,
    pub audio_kbps: u32,
    /// Keyframe interval in seconds (also the HLS segment length).
    pub gop_secs: u32,
}

impl Default for OutputProfile {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps: 30,
            video_kbps: 3000,
            audio_kbps: 128,
            gop_secs: 2,
        }
    }
}

impl OutputProfile {
    /// This channel's picture size, frame rate and bitrate.
    pub fn for_channel(&self, v: &VideoOptions) -> OutputProfile {
        let (width, height, kbps) = v.quality.size();
        OutputProfile {
            width,
            height,
            fps: v.fps.unwrap_or(self.fps),
            video_kbps: v.kbps.unwrap_or(kbps),
            audio_kbps: self.audio_kbps,
            gop_secs: self.gop_secs,
        }
    }
}

/// One item (or the remaining part of one) to encode.
#[derive(Debug, Clone)]
pub struct ItemJob<'a> {
    pub file: &'a Path,
    /// Start this far into the file.
    pub offset_ms: u64,
    /// Play this much of it.
    pub duration_ms: u64,
    /// Output timestamp offset so consecutive items form one continuous stream.
    pub ts_offset_ms: u64,
    pub has_audio: bool,
    /// Which tracks, subtitles, look and graphics this item gets.
    pub plan: ItemPlan,
}

fn secs(ms: u64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

fn video_args(enc: Encoder, p: &OutputProfile) -> Vec<String> {
    let gop = (p.fps * p.gop_secs).to_string();
    let rate = format!("{}k", p.video_kbps);
    let maxrate = format!("{}k", p.video_kbps * 12 / 10);
    let bufsize = format!("{}k", p.video_kbps * 2);
    let mut a: Vec<String> = Vec::new();
    let mut push = |xs: &[&str]| a.extend(xs.iter().map(|s| s.to_string()));
    match enc {
        Encoder::Software => {
            push(&[
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-profile:v",
                "high",
            ]);
            push(&["-b:v", &rate, "-maxrate", &maxrate, "-bufsize", &bufsize]);
            push(&["-g", &gop, "-keyint_min", &gop, "-sc_threshold", "0"]);
        }
        Encoder::VideoToolbox => {
            push(&["-c:v", "h264_videotoolbox", "-profile:v", "high"]);
            push(&["-b:v", &rate, "-maxrate", &maxrate]);
            push(&["-g", &gop, "-allow_sw", "1"]);
        }
        Encoder::Nvenc => {
            push(&["-c:v", "h264_nvenc", "-preset", "p4", "-profile:v", "high"]);
            push(&["-b:v", &rate, "-maxrate", &maxrate, "-bufsize", &bufsize]);
            push(&["-g", &gop]);
        }
        Encoder::Qsv => {
            push(&[
                "-c:v",
                "h264_qsv",
                "-preset",
                "faster",
                "-profile:v",
                "high",
            ]);
            push(&["-b:v", &rate, "-maxrate", &maxrate, "-bufsize", &bufsize]);
            push(&["-g", &gop]);
        }
        Encoder::Vaapi => {
            push(&["-c:v", "h264_vaapi", "-profile:v", "high"]);
            push(&["-b:v", &rate, "-maxrate", &maxrate, "-bufsize", &bufsize]);
            push(&["-g", &gop]);
        }
        Encoder::Amf => {
            push(&[
                "-c:v",
                "h264_amf",
                "-quality",
                "speed",
                "-profile:v",
                "high",
            ]);
            push(&["-b:v", &rate, "-maxrate", &maxrate, "-bufsize", &bufsize]);
            push(&["-g", &gop]);
        }
    }
    a
}

/// Options a hardware encoder needs before the first input.
fn hw_init_args(enc: Encoder) -> Vec<String> {
    match enc {
        Encoder::Vaapi => vec!["-vaapi_device".into(), vaapi_device()],
        _ => Vec::new(),
    }
}

/// The render node VA-API uses (`TVMASTER_VAAPI_DEVICE` overrides it).
pub fn vaapi_device() -> String {
    std::env::var("TVMASTER_VAAPI_DEVICE").unwrap_or_else(|_| "/dev/dri/renderD128".into())
}

/// The last filters of every chain: the pixel format the encoder takes, and
/// for VA-API the upload to the GPU.
fn pix_tail(enc: Encoder) -> &'static str {
    match enc {
        Encoder::Vaapi => "format=nv12,hwupload",
        Encoder::Qsv => "format=nv12",
        _ => "format=yuv420p",
    }
}

/// Arguments to encode one item to MPEG-TS on stdout.
pub fn item_args(job: &ItemJob, p: &OutputProfile, enc: Encoder) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(hw_init_args(enc));
    if job.offset_ms > 0 {
        a.extend(["-ss".into(), secs(job.offset_ms)]);
    }
    a.extend(["-i".into(), job.file.to_string_lossy().to_string()]);
    if !job.has_audio {
        // Silent track so every item has the same stream layout.
        a.extend([
            "-f".into(),
            "lavfi".into(),
            "-t".into(),
            secs(job.duration_ms),
            "-i".into(),
            "anullsrc=r=48000:cl=stereo".into(),
        ]);
    }
    // Extra inputs after the media (and the silent track, when there is one).
    let first_overlay_input = if job.has_audio { 1 } else { 2 };
    let mut overlay_inputs: Vec<(usize, &crate::looks::OverlayInput)> = Vec::new();
    for (next_input, o) in (first_overlay_input..).zip(job.plan.overlays.iter()) {
        a.extend([
            "-loop".into(),
            "1".into(),
            "-framerate".into(),
            p.fps.to_string(),
            "-i".into(),
            o.png.to_string_lossy().to_string(),
        ]);
        overlay_inputs.push((next_input, o));
    }
    a.extend(["-t".into(), secs(job.duration_ms)]);

    // The picture: image subtitles go on at the source's own size, then the
    // frame is shaped, then text subtitles, the look and the clock, then the
    // overlays (logo, programme title).
    let plan = &job.plan;
    let mut chain: Vec<String> = Vec::new();
    if plan.deinterlace {
        chain.push("yadif=0:-1:1".into());
    }
    chain.push(scale_chain(plan.aspect, p.width, p.height));
    chain.push(format!("fps={}", p.fps));
    if let Some(Burn::Text { path, size, style }) = &plan.burn {
        chain.push(subtitle_filter(path, *size, *style, p.height));
    }
    if let Some(l) = look_chain(plan.look) {
        chain.push(l.to_string());
    }
    if let Some(c) = &plan.clock {
        chain.push(clock_filter(c, p.height));
    }
    let image_sub = match &plan.burn {
        Some(Burn::Image { index }) => Some(*index),
        _ => None,
    };
    if image_sub.is_none() && overlay_inputs.is_empty() {
        chain.push(pix_tail(enc).into());
        a.extend(["-map".into(), "0:v:0".into()]);
        a.extend(["-vf".into(), chain.join(",")]);
    } else {
        let mut g = String::new();
        let mut label = "[0:v]".to_string();
        if let Some(i) = image_sub {
            g.push_str(&format!(
                "[0:v][0:{i}]overlay=eof_action=pass:repeatlast=0[sv];"
            ));
            label = "[sv]".into();
        }
        g.push_str(&format!("{label}{}[b0]", chain.join(",")));
        let mut last = "b0".to_string();
        for (n, (idx, o)) in overlay_inputs.iter().enumerate() {
            let next = format!("b{}", n + 1);
            let enable = o
                .enable
                .as_ref()
                .map(|e| format!(":enable='{e}'"))
                .unwrap_or_default();
            g.push_str(&format!(
                ";[{last}][{idx}:v]overlay={}:{}:format=auto{enable}[{next}]",
                o.x, o.y
            ));
            last = next;
        }
        g.push_str(&format!(";[{last}]{}[vout]", pix_tail(enc)));
        a.extend(["-filter_complex".into(), g, "-map".into(), "[vout]".into()]);
    }
    a.extend([
        "-map".into(),
        if !job.has_audio {
            "1:a:0".to_string()
        } else if let Some(i) = plan.audio_index {
            format!("0:{i}")
        } else {
            "0:a:0".to_string()
        },
    ]);
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
    ]);
    if job.has_audio {
        a.extend(["-af".into(), audio_chain(&plan.audio)]);
    }
    a.extend([
        "-output_ts_offset".into(),
        secs(job.ts_offset_ms),
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

fn slate_text(s: &str) -> String {
    // drawtext treats : ' \\ % specially; keep the slate text to safe characters.
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " .,-!&".contains(*c))
        .take(60)
        .collect()
}

/// Arguments for the station slate: a plain card with silence, used for gaps,
/// scheduled filler and files that cannot be played. Text is drawn only when a
/// font is supplied (FFmpeg must have `drawtext`).
pub fn slate_args(
    text: &str,
    duration_ms: u64,
    ts_offset_ms: u64,
    p: &OutputProfile,
    enc: Encoder,
    font: Option<&Path>,
) -> Vec<String> {
    let dur = secs(duration_ms);
    let mut vf = String::new();
    if let Some(font) = font {
        let font = font
            .to_string_lossy()
            .replace('\\', "/")
            .replace(':', "\\:");
        vf.push_str(&format!(
            "drawtext=fontfile='{font}':text='{}':fontcolor=white:fontsize={}:x=(w-text_w)/2:y=(h-text_h)/2,",
            slate_text(text),
            p.height / 14
        ));
    }
    vf.push_str(pix_tail(enc));
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(hw_init_args(enc));
    a.extend([
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        dur.clone(),
        "-i".into(),
        format!("color=c=0x0b1220:s={}x{}:r={}", p.width, p.height, p.fps),
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        dur,
        "-i".into(),
        "anullsrc=r=48000:cl=stereo".into(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-vf".into(),
        vf,
    ]);
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
        "-output_ts_offset".into(),
        secs(ts_offset_ms),
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

/// What is drawn behind an off-air screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenInput<'a> {
    Bars,
    Static,
    Black,
    /// A still picture. `pixel_art` scales the station card without smoothing.
    Picture {
        path: &'a Path,
        pixel_art: bool,
    },
}

/// Arguments for a screen that holds the air when there is nothing to show:
/// colour bars, snow, black, or a still picture, with a steady tone (hiss for
/// snow) or silence.
pub fn screen_args(
    input: ScreenInput,
    tone: bool,
    duration_ms: u64,
    ts_offset_ms: u64,
    p: &OutputProfile,
    enc: Encoder,
) -> Vec<String> {
    let dur = secs(duration_ms);
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(hw_init_args(enc));
    let mut vf = String::new();
    match input {
        ScreenInput::Bars => a.extend([
            "-f".into(),
            "lavfi".into(),
            "-t".into(),
            dur.clone(),
            "-i".into(),
            format!("smptebars=size={}x{}:rate={}", p.width, p.height, p.fps),
        ]),
        ScreenInput::Black => a.extend([
            "-f".into(),
            "lavfi".into(),
            "-t".into(),
            dur.clone(),
            "-i".into(),
            format!("color=c=black:s={}x{}:r={}", p.width, p.height, p.fps),
        ]),
        ScreenInput::Static => a.extend([
            "-f".into(),
            "lavfi".into(),
            "-t".into(),
            dur.clone(),
            "-i".into(),
            format!(
                "color=c=gray:s=320x180:r={},noise=alls=100:allf=t+u,scale={}:{}:flags=neighbor",
                p.fps, p.width, p.height
            ),
        ]),
        ScreenInput::Picture { path, pixel_art } => {
            a.extend([
                "-loop".into(),
                "1".into(),
                "-framerate".into(),
                p.fps.to_string(),
                "-t".into(),
                dur.clone(),
                "-i".into(),
                path.to_string_lossy().to_string(),
            ]);
            vf = if pixel_art {
                format!("scale={}:{}:flags=neighbor,setsar=1,", p.width, p.height)
            } else {
                format!(
                    "{},",
                    crate::looks::scale_chain(tvmaster_core::Aspect::Fit, p.width, p.height)
                )
            };
        }
    }
    let audio = match (input, tone) {
        (ScreenInput::Static, true) => {
            "anoisesrc=color=white:sample_rate=48000:amplitude=0.12".to_string()
        }
        (_, true) => "sine=frequency=1000:sample_rate=48000,volume=0.25".to_string(),
        (_, false) => "anullsrc=r=48000:cl=stereo".to_string(),
    };
    a.extend([
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        dur,
        "-i".into(),
        audio,
    ]);
    a.extend([
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-vf".into(),
        format!("{vf}{}", pix_tail(enc)),
    ]);
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
        "-output_ts_offset".into(),
        secs(ts_offset_ms),
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

/// Arguments for a rendered bumper: raw RGB frames of `src_w` x `src_h` arrive
/// on stdin, are scaled up without smoothing (crisp pixel art) and encoded
/// with silence.
pub fn rendered_args(
    src_w: usize,
    src_h: usize,
    duration_ms: u64,
    ts_offset_ms: u64,
    p: &OutputProfile,
    enc: Encoder,
) -> Vec<String> {
    rendered_args_with_audio(
        src_w,
        src_h,
        duration_ms,
        ts_offset_ms,
        p,
        enc,
        "anullsrc=r=48000:cl=stereo",
    )
}

/// The emergency-alert attention signal: the two tones (853 Hz and 960 Hz) every
/// alert starts with, for `tone_ms`, then quiet.
pub fn attention_tone_source(tone_ms: u64) -> String {
    let t = format!("{:.3}", tone_ms as f64 / 1000.0);
    let e = format!("if(lt(t\\,{t})\\,0.30*sin(2*PI*853*t)+0.30*sin(2*PI*960*t)\\,0)");
    format!("aevalsrc={e}|{e}:s=48000")
}

/// [`rendered_args`] with a chosen lavfi audio source.
pub fn rendered_args_with_audio(
    src_w: usize,
    src_h: usize,
    duration_ms: u64,
    ts_offset_ms: u64,
    p: &OutputProfile,
    enc: Encoder,
    audio_source: &str,
) -> Vec<String> {
    let dur = secs(duration_ms);
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(hw_init_args(enc));
    a.extend([
        "-f".into(),
        "rawvideo".into(),
        "-pixel_format".into(),
        "rgb24".into(),
        "-video_size".into(),
        format!("{src_w}x{src_h}"),
        "-framerate".into(),
        p.fps.to_string(),
        "-i".into(),
        "pipe:0".into(),
        "-f".into(),
        "lavfi".into(),
        "-t".into(),
        dur.clone(),
        "-i".into(),
        audio_source.to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "1:a:0".into(),
        "-vf".into(),
        format!(
            "scale={}:{}:flags=neighbor,setsar=1,fps={},{}",
            p.width,
            p.height,
            p.fps,
            pix_tail(enc)
        ),
        "-t".into(),
        dur,
    ]);
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
        "-output_ts_offset".into(),
        secs(ts_offset_ms),
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

/// Frame rate the guide is drawn at; FFmpeg duplicates frames up to the output
/// rate. Drawing fewer frames keeps the renderer comfortably ahead of real time.
pub const GUIDE_FPS: u32 = 15;

/// A live video window inside the guide: another channel's stream, scaled to
/// fit (with bars) at this place in the output picture.
#[derive(Debug, Clone)]
pub struct PipInput<'a> {
    /// The channel's HLS playlist on disk.
    pub playlist: &'a Path,
    /// Output pixels.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Use the window's sound instead of the music.
    pub audio: bool,
}

/// A logo picture (already sized, with its transparency) placed over the guide.
#[derive(Debug, Clone)]
pub struct LogoInput<'a> {
    pub png: &'a Path,
    pub x: i32,
    pub y: i32,
}

/// One run of the guide's encoder.
#[derive(Debug, Clone)]
pub struct GuideRun<'a> {
    pub src_w: usize,
    pub src_h: usize,
    /// Scale the listing up smoothly (the modern looks) instead of in hard pixels (the retro looks).
    pub smooth_scale: bool,
    /// Music for FFmpeg's concat demuxer; silence when `None`.
    pub music_list: Option<&'a Path>,
    pub pip: Option<PipInput<'a>>,
    pub logo: Option<LogoInput<'a>>,
    /// Stop after this long (`None` = until stdin ends).
    pub duration_ms: Option<u64>,
    /// Where this run's timestamps start, so runs join into one stream.
    pub ts_offset_ms: u64,
}

/// Arguments for the guide channel: raw RGB frames on stdin plus looping music
/// (or silence), optionally with a live video window and a logo laid over it.
pub fn guide_run_args(run: &GuideRun, p: &OutputProfile, enc: Encoder) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(hw_init_args(enc));
    a.extend([
        "-f".into(),
        "rawvideo".into(),
        "-pixel_format".into(),
        "rgb24".into(),
        "-video_size".into(),
        format!("{}x{}", run.src_w, run.src_h),
        "-framerate".into(),
        GUIDE_FPS.to_string(),
        "-i".into(),
        "pipe:0".into(),
    ]);
    match run.music_list {
        Some(list) => a.extend([
            "-stream_loop".into(),
            "-1".into(),
            "-f".into(),
            "concat".into(),
            "-safe".into(),
            "0".into(),
            "-i".into(),
            list.to_string_lossy().to_string(),
        ]),
        None => a.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "anullsrc=r=48000:cl=stereo".into(),
        ]),
    }
    let mut next_input = 2;
    let mut pip_input = None;
    if let Some(pip) = &run.pip {
        a.extend([
            "-live_start_index".into(),
            "-2".into(),
            "-i".into(),
            pip.playlist.to_string_lossy().to_string(),
        ]);
        pip_input = Some((next_input, pip));
        next_input += 1;
    }
    let mut logo_input = None;
    if let Some(logo) = &run.logo {
        a.extend([
            "-loop".into(),
            "1".into(),
            "-framerate".into(),
            p.fps.to_string(),
            "-i".into(),
            logo.png.to_string_lossy().to_string(),
        ]);
        logo_input = Some((next_input, logo));
    }

    let base = format!(
        "scale={}:{}:flags={},setsar=1,fps={}",
        p.width,
        p.height,
        if run.smooth_scale {
            "bicubic"
        } else {
            "neighbor"
        },
        p.fps
    );
    if pip_input.is_none() && logo_input.is_none() {
        a.extend([
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "1:a:0".into(),
            "-vf".into(),
            format!("{base},{}", pix_tail(enc)),
        ]);
    } else {
        let mut g = format!("[0:v]{base}[g0]");
        let mut last = "g0".to_string();
        if let Some((i, pip)) = pip_input {
            // Timestamps restart at zero so the window lines up with the guide's own frames.
            g.push_str(&format!(
                ";[{i}:v]setpts=PTS-STARTPTS,scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps={fps}[pip];[{last}][pip]overlay={x}:{y}:eof_action=pass:format=auto[g1]",
                w = pip.w,
                h = pip.h,
                x = pip.x,
                y = pip.y,
                fps = p.fps,
            ));
            last = "g1".into();
        }
        if let Some((i, logo)) = logo_input {
            g.push_str(&format!(
                ";[{last}][{i}:v]overlay={}:{}:format=auto[g2]",
                logo.x, logo.y
            ));
            last = "g2".into();
        }
        g.push_str(&format!(";[{last}]{}[vout]", pix_tail(enc)));
        a.extend(["-filter_complex".into(), g, "-map".into(), "[vout]".into()]);
        match pip_input {
            Some((i, pip)) if pip.audio => {
                a.extend([
                    "-map".into(),
                    format!("{i}:a:0?"),
                    "-af".into(),
                    "asetpts=PTS-STARTPTS,aresample=48000:async=1".into(),
                ]);
            }
            _ => a.extend(["-map".into(), "1:a:0".into()]),
        }
    }
    if let Some(ms) = run.duration_ms {
        a.extend(["-t".into(), secs(ms)]);
    }
    a.extend(video_args(enc, p));
    a.extend([
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{}k", p.audio_kbps),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
    ]);
    if run.ts_offset_ms > 0 {
        a.extend(["-output_ts_offset".into(), secs(run.ts_offset_ms)]);
    }
    a.extend([
        "-muxdelay".into(),
        "0".into(),
        "-muxpreload".into(),
        "0".into(),
        "-f".into(),
        "mpegts".into(),
        "pipe:1".into(),
    ]);
    a
}

/// Arguments for the long-running segmenter that turns the continuous
/// MPEG-TS on stdin into a rolling HLS playlist in `dir`.
pub fn segmenter_args(dir: &Path, p: &OutputProfile, readrate_burst: bool) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Pace input at real time; burst the first few seconds so tune-in is fast.
    if readrate_burst {
        a.extend([
            "-readrate".into(),
            "1".into(),
            "-readrate_initial_burst".into(),
            (p.gop_secs * 3).to_string(),
        ]);
    } else {
        a.push("-re".into());
    }
    a.extend(["-f", "mpegts", "-i", "pipe:0", "-c", "copy", "-f", "hls"].map(String::from));
    a.extend(["-hls_time".into(), p.gop_secs.to_string()]);
    a.extend(["-hls_list_size".into(), "10".into()]);
    a.extend([
        "-hls_flags".into(),
        "delete_segments+independent_segments+omit_endlist+program_date_time+temp_file".into(),
        "-hls_start_number_source".into(),
        "epoch".into(),
        "-hls_segment_filename".into(),
        dir.join("seg_%d.ts").to_string_lossy().to_string(),
        dir.join("index.m3u8").to_string_lossy().to_string(),
    ]);
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn job(has_audio: bool, offset: u64) -> (PathBuf, u64, u64, bool) {
        (PathBuf::from("/tv/a.mkv"), offset, 20_000, has_audio)
    }

    fn build(has_audio: bool, offset: u64) -> Vec<String> {
        let (f, off, dur, audio) = job(has_audio, offset);
        item_args(
            &ItemJob {
                file: &f,
                offset_ms: off,
                duration_ms: dur,
                ts_offset_ms: 5_000,
                has_audio: audio,
                plan: Default::default(),
            },
            &OutputProfile::default(),
            Encoder::Software,
        )
    }

    fn has_pair(a: &[String], k: &str, v: &str) -> bool {
        a.windows(2).any(|w| w[0] == k && w[1] == v)
    }

    #[test]
    fn seeks_only_when_offset_nonzero() {
        assert!(!build(true, 0).contains(&"-ss".to_string()));
        assert!(has_pair(&build(true, 12_500), "-ss", "12.500"));
    }

    #[test]
    fn silent_items_get_generated_audio() {
        let a = build(false, 0);
        assert!(a.iter().any(|x| x.starts_with("anullsrc")));
        assert!(has_pair(&a, "-map", "1:a:0"));
        let b = build(true, 0);
        assert!(!b.iter().any(|x| x.starts_with("anullsrc")));
        assert!(has_pair(&b, "-map", "0:a:0"));
    }

    #[test]
    fn timestamps_continue_across_items() {
        let a = build(true, 0);
        assert!(has_pair(&a, "-output_ts_offset", "5.000"));
        assert!(has_pair(&a, "-f", "mpegts"));
        assert_eq!(a.last().unwrap(), "pipe:1");
    }

    #[test]
    fn normalizes_to_profile() {
        let a = build(true, 0);
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(vf.contains("scale=1280:720"));
        assert!(vf.contains("pad=1280:720"));
        assert!(vf.contains("fps=30"));
    }

    #[test]
    fn rendered_bumpers_scale_up_without_smoothing() {
        let a = rendered_args(
            640,
            360,
            4_000,
            2_000,
            &OutputProfile::default(),
            Encoder::Software,
        );
        assert!(has_pair(&a, "-video_size", "640x360"));
        assert!(has_pair(&a, "-f", "rawvideo"));
        assert!(has_pair(&a, "-t", "4.000"));
        assert!(has_pair(&a, "-output_ts_offset", "2.000"));
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(vf.contains("scale=1280:720:flags=neighbor"));
        assert!(a.iter().any(|x| x.starts_with("anullsrc")));
    }

    #[test]
    fn guide_args_loop_music_or_fall_back_to_silence() {
        let p = OutputProfile::default();
        let run = |music| GuideRun {
            src_w: 640,
            src_h: 360,
            smooth_scale: false,
            music_list: music,
            pip: None,
            logo: None,
            duration_ms: None,
            ts_offset_ms: 0,
        };
        let with = guide_run_args(&run(Some(Path::new("/c/list.txt"))), &p, Encoder::Software);
        assert!(has_pair(&with, "-stream_loop", "-1"));
        assert!(has_pair(&with, "-f", "concat"));
        assert!(has_pair(&with, "-framerate", "15"));
        assert!(!with.contains(&"-t".to_string()), "endless");
        assert!(!with.contains(&"-output_ts_offset".to_string()));
        assert!(
            !with.contains(&"-filter_complex".to_string()),
            "no window, no graph"
        );
        let without = guide_run_args(&run(None), &p, Encoder::Software);
        assert!(without.iter().any(|x| x.starts_with("anullsrc")));
        assert_eq!(without.last().unwrap(), "pipe:1");
    }

    #[test]
    fn a_video_window_is_laid_over_the_guide_at_its_place() {
        let p = OutputProfile::default();
        let list = Path::new("/c/list.txt");
        let hls = Path::new("/hls/retro/index.m3u8");
        let run = GuideRun {
            src_w: 640,
            src_h: 360,
            smooth_scale: false,
            music_list: Some(list),
            pip: Some(PipInput {
                playlist: hls,
                x: 836,
                y: 90,
                w: 384,
                h: 216,
                audio: false,
            }),
            logo: None,
            duration_ms: Some(30_000),
            ts_offset_ms: 90_000,
        };
        let a = guide_run_args(&run, &p, Encoder::Software);
        assert!(has_pair(&a, "-i", "/hls/retro/index.m3u8"));
        assert!(
            has_pair(&a, "-live_start_index", "-2"),
            "start near the live edge"
        );
        let graph = &a[a.iter().position(|x| x == "-filter_complex").unwrap() + 1];
        assert!(
            graph.contains("setpts=PTS-STARTPTS"),
            "the window's clock starts at zero: {graph}"
        );
        assert!(
            graph.contains("scale=384:216:force_original_aspect_ratio=decrease,pad=384:216"),
            "{graph}"
        );
        assert!(graph.contains("overlay=836:90"), "{graph}");
        assert!(graph.ends_with("[vout]"));
        assert!(
            has_pair(&a, "-map", "[vout]") && has_pair(&a, "-map", "1:a:0"),
            "music stays"
        );
        assert!(has_pair(&a, "-t", "30.000"));
        assert!(has_pair(&a, "-output_ts_offset", "90.000"));

        // The window's own sound, when asked for.
        let mut loud = run.clone();
        loud.pip.as_mut().unwrap().audio = true;
        let a = guide_run_args(&loud, &p, Encoder::Software);
        assert!(has_pair(&a, "-map", "2:a:0?") && !has_pair(&a, "-map", "1:a:0"));

        // A logo goes on last, as the next input.
        let mut both = run.clone();
        both.logo = Some(LogoInput {
            png: Path::new("/c/logo.png"),
            x: 16,
            y: 8,
        });
        let a = guide_run_args(&both, &p, Encoder::Software);
        let graph = &a[a.iter().position(|x| x == "-filter_complex").unwrap() + 1];
        assert!(graph.contains("[g1][3:v]overlay=16:8"), "{graph}");
        let mut only_logo = run;
        only_logo.pip = None;
        only_logo.logo = Some(LogoInput {
            png: Path::new("/c/logo.png"),
            x: 16,
            y: 8,
        });
        let a = guide_run_args(&only_logo, &p, Encoder::Software);
        let graph = &a[a.iter().position(|x| x == "-filter_complex").unwrap() + 1];
        assert!(graph.contains("[g0][2:v]overlay=16:8"), "{graph}");
    }

    #[test]
    fn slate_is_silent_video_with_matching_layout() {
        let a = slate_args(
            "Retro TV",
            5_000,
            1_000,
            &OutputProfile::default(),
            Encoder::Software,
            None,
        );
        assert!(a.iter().any(|x| x.starts_with("color=c=")));
        assert!(a.iter().any(|x| x.starts_with("anullsrc")));
        assert!(has_pair(&a, "-output_ts_offset", "1.000"));
        assert!(has_pair(&a, "-t", "5.000"));
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert_eq!(vf, "format=yuv420p", "no text without a font");
    }

    #[test]
    fn slate_text_is_sanitized_when_a_font_is_given() {
        let font = Path::new("/fonts/a.ttf");
        let a = slate_args(
            "Bob's: 100% \\ TV",
            1_000,
            0,
            &OutputProfile::default(),
            Encoder::Software,
            Some(font),
        );
        let vf = a
            .windows(2)
            .find(|w| w[0] == "-vf")
            .map(|w| w[1].clone())
            .unwrap();
        assert!(vf.contains("text='Bobs 100  TV'"), "{vf}");
        assert!(vf.contains("fontfile='/fonts/a.ttf'"));
    }

    #[test]
    fn segmenter_uses_burst_when_available() {
        let d = Path::new("/tmp/x");
        let a = segmenter_args(d, &OutputProfile::default(), true);
        assert!(has_pair(&a, "-readrate_initial_burst", "6"));
        let b = segmenter_args(d, &OutputProfile::default(), false);
        assert!(b.contains(&"-re".to_string()));
        assert!(b.last().unwrap().ends_with("index.m3u8"));
    }

    #[test]
    fn hardware_encoders_get_their_pixel_format_and_device() {
        let p = OutputProfile::default();
        let job = ItemJob {
            file: Path::new("/x.mp4"),
            offset_ms: 0,
            duration_ms: 1000,
            ts_offset_ms: 0,
            has_audio: true,
            plan: Default::default(),
        };
        let va = item_args(&job, &p, Encoder::Vaapi);
        let dev = va.iter().position(|a| a == "-vaapi_device").unwrap();
        let input = va.iter().position(|a| a == "-i").unwrap();
        assert!(dev < input, "the device is set before the input");
        assert!(va.iter().any(|a| a.ends_with("format=nv12,hwupload")));
        assert!(va.contains(&"h264_vaapi".to_string()));

        let qsv = item_args(&job, &p, Encoder::Qsv);
        assert!(qsv.iter().any(|a| a.ends_with("format=nv12")));
        assert!(qsv.contains(&"h264_qsv".to_string()));
        assert!(!qsv.contains(&"-vaapi_device".to_string()));

        let amf = slate_args("x", 1000, 0, &p, Encoder::Amf, None);
        assert!(amf.contains(&"h264_amf".to_string()));
        assert!(amf.contains(&"format=yuv420p".to_string()));
    }

    fn job_with(plan: ItemPlan, has_audio: bool) -> Vec<String> {
        let job = ItemJob {
            file: Path::new("/media/Show S01E01.mkv"),
            offset_ms: 0,
            duration_ms: 60_000,
            ts_offset_ms: 0,
            has_audio,
            plan,
        };
        item_args(&job, &OutputProfile::default(), Encoder::Software)
    }

    fn arg_after<'a>(a: &'a [String], flag: &str) -> &'a str {
        let i = a
            .iter()
            .position(|x| x == flag)
            .unwrap_or_else(|| panic!("{flag} missing in {a:?}"));
        &a[i + 1]
    }

    #[test]
    fn the_chosen_audio_track_is_mapped_by_its_stream_index() {
        let plain = job_with(ItemPlan::default(), true);
        assert!(plain.windows(2).any(|w| w[0] == "-map" && w[1] == "0:a:0"));
        let chosen = job_with(
            ItemPlan {
                audio_index: Some(3),
                ..Default::default()
            },
            true,
        );
        assert!(chosen.windows(2).any(|w| w[0] == "-map" && w[1] == "0:3"));
        assert!(!chosen.iter().any(|a| a == "0:a:0"));
        // a silent file still gets the generated silent track, whatever was chosen
        let silent = job_with(
            ItemPlan {
                audio_index: Some(3),
                ..Default::default()
            },
            false,
        );
        assert!(silent.windows(2).any(|w| w[0] == "-map" && w[1] == "1:a:0"));
    }

    #[test]
    fn the_picture_chain_runs_in_a_sensible_order() {
        use crate::looks::Burn;
        use tvmaster_core::{Aspect, Look, SubSize, SubStyle};
        let plan = ItemPlan {
            deinterlace: true,
            aspect: Aspect::Fill,
            look: Look::Crt,
            burn: Some(Burn::Text {
                path: "/c/subs/1.srt".into(),
                size: SubSize::Medium,
                style: SubStyle::Outline,
            }),
            ..Default::default()
        };
        let a = job_with(plan, true);
        let vf = arg_after(&a, "-vf");
        let at = |k: &str| vf.find(k).unwrap_or_else(|| panic!("{k} not in {vf}"));
        let order = [
            at("yadif"),
            at("crop="),
            at("fps=30"),
            at("subtitles=filename="),
            at("drawgrid"),
            at("format=yuv420p"),
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{vf}");
        assert!(
            !a.iter().any(|x| x == "-filter_complex"),
            "no extra inputs: a plain -vf chain"
        );
    }

    #[test]
    fn overlays_become_looped_inputs_with_the_right_stream_numbers() {
        use crate::looks::OverlayInput;
        let overlays = vec![
            OverlayInput {
                png: "/c/bug.png".into(),
                x: "main_w-overlay_w-24".into(),
                y: "24".into(),
                enable: None,
            },
            OverlayInput {
                png: "/c/title.png".into(),
                x: "40".into(),
                y: "main_h-overlay_h-48".into(),
                enable: Some("between(t,0.5,8)".into()),
            },
        ];
        // with audio in the file: media is input 0, overlays 1 and 2
        let a = job_with(
            ItemPlan {
                overlays: overlays.clone(),
                ..Default::default()
            },
            true,
        );
        let g = arg_after(&a, "-filter_complex");
        assert!(g.contains("[b0][1:v]overlay=main_w-overlay_w-24:24"), "{g}");
        assert!(
            g.contains(
                "[b1][2:v]overlay=40:main_h-overlay_h-48:format=auto:enable='between(t,0.5,8)'[b2]"
            ),
            "{g}"
        );
        assert!(g.ends_with("[b2]format=yuv420p[vout]"), "{g}");
        assert_eq!(arg_after(&a, "-map"), "[vout]");
        assert_eq!(a.iter().filter(|x| *x == "-loop").count(), 2);
        // the looped inputs come before -t, so -t limits the output
        let (loop_at, t_at) = (
            a.iter().position(|x| x == "-loop").unwrap(),
            a.iter().position(|x| x == "-t").unwrap(),
        );
        assert!(loop_at < t_at);
        // without audio the silent generator is input 1, so overlays start at 2
        let silent = job_with(
            ItemPlan {
                overlays,
                ..Default::default()
            },
            false,
        );
        let g = arg_after(&silent, "-filter_complex");
        assert!(
            g.contains("[b0][2:v]overlay") && g.contains("[b1][3:v]overlay"),
            "{g}"
        );
    }

    #[test]
    fn picture_subtitles_are_overlaid_at_the_sources_size_before_scaling() {
        use crate::looks::Burn;
        let a = job_with(
            ItemPlan {
                burn: Some(Burn::Image { index: 5 }),
                ..Default::default()
            },
            true,
        );
        let g = arg_after(&a, "-filter_complex");
        assert!(g.starts_with("[0:v][0:5]overlay="), "{g}");
        let overlay_at = g.find("overlay=").unwrap();
        let scale_at = g.find("scale=").unwrap();
        assert!(
            overlay_at < scale_at,
            "the bitmap matches the original frame, so it goes on first: {g}"
        );
        assert!(g.ends_with("[vout]"));
    }

    #[test]
    fn audio_effects_reach_the_audio_filter() {
        use crate::looks::AudioFx;
        use tvmaster_core::Loudness;
        let plan = ItemPlan {
            audio: AudioFx {
                loudness: Loudness::Broadcast,
                night: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let a = job_with(plan, true);
        let af = arg_after(&a, "-af");
        assert!(
            af.starts_with("aresample=async=1")
                && af.contains("loudnorm=I=-23")
                && af.contains("acompressor")
        );
        // no audio filter at all for the generated silent track
        assert!(!job_with(ItemPlan::default(), false)
            .iter()
            .any(|x| x == "-af"));
    }

    #[test]
    fn a_channels_quality_and_rate_set_its_output_profile() {
        use tvmaster_core::{Quality, VideoOptions};
        let base = OutputProfile::default();
        let p = base.for_channel(&VideoOptions {
            quality: Quality::P1080,
            fps: Some(24),
            kbps: None,
            ..Default::default()
        });
        assert_eq!(
            (p.width, p.height, p.fps, p.video_kbps),
            (1920, 1080, 24, 6000)
        );
        let p = base.for_channel(&VideoOptions {
            quality: Quality::P480,
            kbps: Some(900),
            ..Default::default()
        });
        assert_eq!(
            (p.width, p.height, p.fps, p.video_kbps),
            (854, 480, 30, 900)
        );
        assert_eq!(base.for_channel(&VideoOptions::default()).width, 1280);
    }

    #[test]
    fn off_air_screens_are_bars_snow_black_or_a_picture_with_a_tone_or_silence() {
        let p = OutputProfile::default();
        let join = |a: Vec<String>| a.join(" ");
        let bars = join(screen_args(
            ScreenInput::Bars,
            true,
            30_000,
            0,
            &p,
            Encoder::Software,
        ));
        assert!(bars.contains("smptebars=size=1280x720") && bars.contains("sine=frequency=1000"));
        let snow = join(screen_args(
            ScreenInput::Static,
            true,
            30_000,
            0,
            &p,
            Encoder::Software,
        ));
        assert!(snow.contains("noise=alls=100") && snow.contains("anoisesrc"));
        let black = join(screen_args(
            ScreenInput::Black,
            false,
            30_000,
            0,
            &p,
            Encoder::Software,
        ));
        assert!(
            black.contains("color=c=black:s=1280x720") && black.contains("anullsrc"),
            "silence when no tone"
        );
        let card = join(screen_args(
            ScreenInput::Picture {
                path: Path::new("/c/card.png"),
                pixel_art: true,
            },
            false,
            30_000,
            0,
            &p,
            Encoder::Software,
        ));
        assert!(
            card.contains("-loop 1")
                && card.contains("/c/card.png")
                && card.contains("flags=neighbor")
        );
        let photo = join(screen_args(
            ScreenInput::Picture {
                path: Path::new("/pics/a.jpg"),
                pixel_art: false,
            },
            false,
            30_000,
            0,
            &p,
            Encoder::Software,
        ));
        assert!(
            photo.contains("pad=1280:720") && !photo.contains("flags=neighbor"),
            "a user's picture keeps its shape: {photo}"
        );
    }
}
