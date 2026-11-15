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
    R.get_or_init(|| Regex::new(r"(?i)^(season|series|specials?)\b[\s._-]*\d*$").unwrap())
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

    // Nearest ancestor folder under the library root that is not "Season N".
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
}
