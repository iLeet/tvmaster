//! Finding video files and parsing show/season/episode out of their paths.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::WalkDir;

const VIDEO_EXTS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "ts", "m2ts", "mpg", "mpeg", "wmv", "webm", "flv",
];

const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "aac", "flac", "wav", "ogg", "opus", "wma", "aiff",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedName {
    pub show: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub title: String,
}

fn re_sxe() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\bs(\d{1,3})\s*[ex](\d{1,3})").unwrap())
}

fn re_nxn() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\b(\d{1,2})x(\d{2,3})\b").unwrap())
}

fn re_year() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?:^|[^0-9])((?:19[2-9]|20[0-3])[0-9])(?:[^0-9]|$)").unwrap())
}

fn re_season_dir() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^(?:(?:season|series|saison|staffel|temporada)[\s._-]*\d*|specials?|s[\s._-]?\d{1,2})$").unwrap())
}

fn clean(s: &str) -> String {
    let spaced: String = s
        .chars()
        .map(|c| if c == '.' || c == '_' { ' ' } else { c })
        .collect();
    spaced
        .trim_matches(|c: char| c.is_whitespace() || c == '-')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// File extensions a library of this kind accepts: video for shows, commercials
/// and bumpers (plus subtitle files beside shows), audio for music.
pub fn library_extensions(kind: &str) -> Vec<&'static str> {
    match kind {
        "music" => AUDIO_EXTS.to_vec(),
        "shows" => VIDEO_EXTS.iter().chain(SUB_EXTS).copied().collect(),
        _ => VIDEO_EXTS.to_vec(),
    }
}

const SUB_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt"];

/// Subtitle files that sit next to `video` and belong to it: `Name.srt`,
/// `Name.en.srt`, `Name.eng.forced.srt`, `Name.English.sdh.srt`. A file that
/// starts with the video's name (without extension) qualifies; the words after
/// it say the language and whether it is forced or for the hard of hearing.
pub fn find_sidecar_subs(video: &Path) -> Vec<tvmaster_core::ExternalSub> {
    let (Some(dir), Some(stem)) = (video.parent(), video.file_stem().and_then(|s| s.to_str()))
    else {
        return Vec::new();
    };
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<tvmaster_core::ExternalSub> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| SUB_EXTS.contains(&e.to_lowercase().as_str()))
        })
        .filter_map(|p| {
            let name = p.file_stem()?.to_str()?.to_string();
            let rest = name.strip_prefix(stem)?;
            // "Name.en" must not match "Name 2.en": the next character has to be a separator or the end.
            if !rest.is_empty() && !rest.starts_with(['.', '_', '-', ' ']) {
                return None;
            }
            let (mut lang, mut forced, mut sdh) = (String::new(), false, false);
            for word in rest.split(['.', '_', '-', ' ']).filter(|w| !w.is_empty()) {
                match word.to_lowercase().as_str() {
                    "forced" => forced = true,
                    "sdh" | "cc" | "hi" => sdh = true,
                    w => {
                        let n = tvmaster_core::norm_lang(w);
                        // Only words that normalise to a known three-letter language count.
                        if lang.is_empty() && n.len() == 3 && (n != w || w.len() == 3) {
                            lang = n;
                        }
                    }
                }
            }
            Some(tvmaster_core::ExternalSub {
                path: p.to_string_lossy().to_string(),
                lang,
                forced,
                sdh,
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn find_with_exts(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            !e.file_name().to_string_lossy().starts_with('.')
                && e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| exts.contains(&x.to_ascii_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .map(|e| e.into_path())
        .collect();
    v.sort();
    v
}

/// All video files under `root`, sorted for stable results.
pub fn find_videos(root: &Path) -> Vec<PathBuf> {
    find_with_exts(root, VIDEO_EXTS)
}

/// All audio files under `root` (for music libraries), sorted.
pub fn find_audio(root: &Path) -> Vec<PathBuf> {
    find_with_exts(root, AUDIO_EXTS)
}

/// A release year (1920-2039) in the file's name or any folder under `root`,
/// e.g. `Commercials/1987/Coke.mp4` or `Coke (1987).mp4`. Used to match
/// commercials to a channel's era.
pub fn parse_year(root: &Path, file: &Path) -> Option<i32> {
    let rel = file.strip_prefix(root).unwrap_or(file);
    // The file name wins over folders; deeper folders over shallower ones.
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if let Some(last) = parts.last_mut() {
        *last = Path::new(last.as_str())
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    parts
        .iter()
        .rev()
        .find_map(|p| re_year().captures(p).and_then(|c| c[1].parse().ok()))
}

/// Best-effort parse of show/season/episode/title from a file's path.
pub fn parse_name(root: &Path, file: &Path) -> ParsedName {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Nearest ancestor folder under the library root that is not a season folder ("Season 2", "S02", "Specials").
    let folder_show = file
        .parent()
        .into_iter()
        .flat_map(|p| p.ancestors())
        .take_while(|a| *a != root && a.starts_with(root))
        .filter_map(|a| a.file_name().map(|n| n.to_string_lossy().to_string()))
        .find(|name| !re_season_dir().is_match(name))
        .map(|n| clean(&n))
        .filter(|n| !n.is_empty());

    let caps = re_sxe()
        .captures(&stem)
        .or_else(|| re_nxn().captures(&stem));

    match caps {
        Some(c) => {
            let m = c.get(0).unwrap();
            let prefix = clean(&stem[..m.start()]);
            let suffix = clean(&stem[m.end()..]);
            ParsedName {
                show: folder_show.or(if prefix.is_empty() {
                    None
                } else {
                    Some(prefix)
                }),
                season: c[1].parse().ok(),
                episode: c[2].parse().ok(),
                title: if !suffix.is_empty() {
                    suffix
                } else if let (Some(sn), Some(ep)) =
                    (c[1].parse::<u32>().ok(), c[2].parse::<u32>().ok())
                {
                    format!("S{sn:02}E{ep:02}")
                } else {
                    clean(&stem)
                },
            }
        }
        None => ParsedName {
            show: folder_show,
            season: None,
            episode: None,
            title: clean(&stem),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(root: &str, file: &str) -> ParsedName {
        parse_name(Path::new(root), Path::new(file))
    }

    #[test]
    fn standard_layout() {
        let n = p(
            "/tv",
            "/tv/Cool Show/Season 2/Cool Show - S02E05 - The Fifth.mkv",
        );
        assert_eq!(n.show.as_deref(), Some("Cool Show"));
        assert_eq!((n.season, n.episode), (Some(2), Some(5)));
        assert_eq!(n.title, "The Fifth");
    }

    #[test]
    fn flat_layout_uses_filename_prefix() {
        let n = p("/tv", "/tv/Another.Show.S01E03.720p.mkv");
        assert_eq!(n.show.as_deref(), Some("Another Show"));
        assert_eq!((n.season, n.episode), (Some(1), Some(3)));
        assert_eq!(n.title, "720p");
    }

    #[test]
    fn missing_episode_title_falls_back_to_code() {
        let n = p("/tv", "/tv/Show Beta/Show Beta S01E04.mkv");
        assert_eq!(n.title, "S01E04");
    }

    #[test]
    fn audio_and_video_are_found_separately() {
        let dir = std::env::temp_dir().join(format!("tvmaster-scan-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in [
            "a.mp4",
            "sub/b.MKV",
            "c.mp3",
            "sub/d.WAV",
            ".hidden.mp4",
            "notes.txt",
        ] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let names = |v: Vec<PathBuf>| -> Vec<String> {
            v.iter()
                .map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().to_string())
                .collect()
        };
        assert_eq!(names(find_videos(&dir)), vec!["a.mp4", "sub/b.MKV"]);
        assert_eq!(names(find_audio(&dir)), vec!["c.mp3", "sub/d.WAV"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn years_come_from_folders_or_names() {
        let y = |f: &str| parse_year(Path::new("/ads"), Path::new(f));
        assert_eq!(y("/ads/1987/Coke Classic.mp4"), Some(1987));
        assert_eq!(y("/ads/Coke (1991).mp4"), Some(1991));
        assert_eq!(y("/ads/1980s/1984/Pepsi.mkv"), Some(1984));
        assert_eq!(y("/ads/Pepsi 1984 Super Bowl/Pepsi.mkv"), Some(1984));
        assert_eq!(y("/ads/Pepsi_Challenge.mp4"), None);
        assert_eq!(
            y("/ads/Spot 12345.mp4"),
            None,
            "not a year inside a longer number"
        );
        assert_eq!(y("/ads/1080p/Clip.mp4"), None);
    }

    #[test]
    fn season_folders_written_s01_are_not_the_show() {
        for folder in ["S01", "s1", "S 02", "Season 3", "season03", "Specials"] {
            let n = p(
                "/tv",
                &format!(
                    "/tv/Max and Ruby/{folder}/Max and Ruby_S01E13+S01E14_Max's Halloween.mp4"
                ),
            );
            assert_eq!(n.show.as_deref(), Some("Max and Ruby"), "{folder}");
        }
        // A show that merely starts with an S is still a show.
        let n = p("/tv", "/tv/Sesame St/S01/Sesame St - S01E01.mp4");
        assert_eq!(n.show.as_deref(), Some("Sesame St"));
        let n = p("/tv", "/tv/S Club 7/Episode - S01E01.mp4");
        assert_eq!(n.show.as_deref(), Some("S Club 7"));
        let n = p("/tv", "/tv/SG1/SG1 - S01E01.mp4");
        assert_eq!(n.show.as_deref(), Some("SG1"));
    }

    #[test]
    fn nxn_style() {
        let n = p("/tv", "/tv/Old Show/Old Show 3x07 Title.avi");
        assert_eq!(n.show.as_deref(), Some("Old Show"));
        assert_eq!((n.season, n.episode), (Some(3), Some(7)));
        assert_eq!(n.title, "Title");
    }

    #[test]
    fn no_marker_falls_back_to_folder_and_stem() {
        let n = p("/tv", "/tv/Cartoons/Bugs_Bunny_Special.mp4");
        assert_eq!(n.show.as_deref(), Some("Cartoons"));
        assert_eq!(n.season, None);
        assert_eq!(n.title, "Bugs Bunny Special");
    }

    #[test]
    fn file_directly_in_root_without_marker() {
        let n = p("/tv", "/tv/Movie.mp4");
        assert_eq!(n.show, None);
        assert_eq!(n.title, "Movie");
    }

    #[test]
    fn sidecar_subtitles_are_found_by_name_and_their_words_are_read() {
        let d = std::env::temp_dir().join(format!("tvm-sidecar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for f in [
            "Show S01E01.mkv",
            "Show S01E01.en.srt",
            "Show S01E01.eng.forced.srt",
            "Show S01E01.English.sdh.srt",
            "Show S01E01.srt",
            "Show S01E011.en.srt",
            "Show S01E02.en.srt",
            "Show S01E01.nfo",
            "Show S01E01.fr.ass",
        ] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let subs = find_sidecar_subs(&d.join("Show S01E01.mkv"));
        let names: Vec<(String, String, bool, bool)> = subs
            .iter()
            .map(|s| {
                (
                    Path::new(&s.path)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    s.lang.clone(),
                    s.forced,
                    s.sdh,
                )
            })
            .collect();
        assert_eq!(
            names,
            vec![
                (
                    "Show S01E01.English.sdh.srt".to_string(),
                    "eng".to_string(),
                    false,
                    true
                ),
                (
                    "Show S01E01.en.srt".to_string(),
                    "eng".to_string(),
                    false,
                    false
                ),
                (
                    "Show S01E01.eng.forced.srt".to_string(),
                    "eng".to_string(),
                    true,
                    false
                ),
                (
                    "Show S01E01.fr.ass".to_string(),
                    "fra".to_string(),
                    false,
                    false
                ),
                ("Show S01E01.srt".to_string(), String::new(), false, false),
            ],
            "other episodes (including E011) and non-subtitle files are ignored"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
