//! Audio and subtitle tracks inside a video file, and how a channel chooses
//! among them. Pure: the scanner fills [`Tracks`] in, playout asks [`choose`].

use crate::options::{AudioPrefs, SubMode, SubPrefs};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AudioTrack {
    /// FFmpeg's stream index in the file (`-map 0:<index>`).
    pub index: u32,
    /// Canonical three-letter language (`eng`), empty when the file does not say.
    pub lang: String,
    pub codec: String,
    pub channels: u32,
    pub title: String,
    pub default: bool,
    /// A director's or cast commentary track, not the programme's sound.
    pub commentary: bool,
    /// Audio description for blind viewers (narrates the picture).
    pub description: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubTrack {
    pub index: u32,
    pub lang: String,
    pub codec: String,
    pub title: String,
    pub default: bool,
    /// Only the lines that are not in the audio language (signs, foreign speech).
    pub forced: bool,
    /// Subtitles for the deaf and hard of hearing (sound cues, speaker names).
    pub sdh: bool,
    /// A picture (PGS, VobSub) rather than text: it can be overlaid but not restyled.
    pub image: bool,
}

/// A subtitle file next to the video (`Show S01E02.en.srt`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExternalSub {
    pub path: String,
    pub lang: String,
    pub forced: bool,
    pub sdh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Tracks {
    #[serde(default)]
    pub audio: Vec<AudioTrack>,
    #[serde(default)]
    pub subs: Vec<SubTrack>,
    #[serde(default)]
    pub external: Vec<ExternalSub>,
    /// The picture is interlaced (old broadcast video): worth deinterlacing.
    #[serde(default)]
    pub interlaced: bool,
}

impl Tracks {
    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

/// Where a chosen subtitle comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubPick {
    /// Stream `index` inside the video file. `image` subtitles are overlaid.
    Embedded { index: u32, image: bool },
    /// A text file next to the video.
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TrackChoice {
    /// FFmpeg stream index of the audio to air; `None` = whatever FFmpeg picks.
    pub audio: Option<u32>,
    pub subtitle: Option<SubPick>,
}

/// Canonical three-letter code for a language written any common way:
/// `en`, `eng`, `English`, `fre`/`fra`/`French`. Unknown input is lower-cased
/// and returned as is; empty and `und` (undetermined) become empty.
pub fn norm_lang(s: &str) -> String {
    let t = s.trim().to_lowercase();
    if t.is_empty() || t == "und" || t == "unk" || t == "zxx" || t == "mis" {
        return String::new();
    }
    // (canonical, aliases...)
    const TABLE: &[&[&str]] = &[
        &["eng", "en", "enu", "english", "en-us", "en-gb"],
        &["fra", "fr", "fre", "french", "français", "francais"],
        &["deu", "de", "ger", "german", "deutsch"],
        &[
            "spa", "es", "español", "espanol", "spanish", "es-419", "es-es", "lat",
        ],
        &["ita", "it", "italian", "italiano"],
        &["por", "pt", "portuguese", "português", "pt-br", "pt-pt"],
        &["jpn", "ja", "jp", "japanese", "日本語"],
        &["kor", "ko", "korean", "한국어"],
        &[
            "zho", "zh", "chi", "chinese", "cmn", "yue", "zh-cn", "zh-tw", "chs", "cht",
        ],
        &["rus", "ru", "russian", "русский"],
        &["ara", "ar", "arabic"],
        &["hin", "hi", "hindi"],
        &["nld", "nl", "dut", "dutch"],
        &["swe", "sv", "swedish"],
        &["nor", "no", "nob", "nb", "norwegian"],
        &["dan", "da", "danish"],
        &["fin", "fi", "finnish"],
        &["pol", "pl", "polish"],
        &["tur", "tr", "turkish"],
        &["ell", "el", "gre", "greek"],
        &["heb", "he", "hebrew", "iw"],
        &["ces", "cs", "cze", "czech"],
        &["hun", "hu", "hungarian"],
        &["ron", "ro", "rum", "romanian"],
        &["ukr", "uk", "ukrainian"],
        &["tha", "th", "thai"],
        &["vie", "vi", "vietnamese"],
        &["ind", "id", "indonesian"],
        &["fil", "tl", "tgl", "filipino", "tagalog"],
        &["cat", "ca", "catalan"],
        &["isl", "is", "ice", "icelandic"],
        &["lat", "la", "latin"],
    ];
    for row in TABLE {
        if row.contains(&t.as_str()) {
            return row[0].to_string();
        }
    }
    // `pt-BR` style tags we did not list: use the primary part.
    if let Some((primary, _)) = t.split_once(['-', '_']) {
        if primary != t {
            return norm_lang(primary);
        }
    }
    t
}

fn is_commentary(title: &str, tags_comment: bool) -> bool {
    let t = title.to_lowercase();
    tags_comment
        || t.contains("commentary")
        || t.contains("director's")
        || t.contains("directors")
        || t.contains("behind the scenes")
}

fn is_description(title: &str, visual_impaired: bool) -> bool {
    let t = title.to_lowercase();
    visual_impaired
        || t.contains("audio description")
        || t.contains("descriptive")
        || t.contains("described video")
        || t.contains(" ad)")
        || t == "ad"
}

/// Build an audio track from what FFprobe reported (every argument is one thing it said).
#[allow(clippy::too_many_arguments)]
pub fn audio_track(
    index: u32,
    lang: &str,
    codec: &str,
    channels: u32,
    title: &str,
    default: bool,
    comment_flag: bool,
    visual_impaired: bool,
) -> AudioTrack {
    AudioTrack {
        index,
        lang: norm_lang(lang),
        codec: codec.into(),
        channels,
        title: title.into(),
        default,
        commentary: is_commentary(title, comment_flag),
        description: is_description(title, visual_impaired),
    }
}

/// Subtitle codecs that are pictures, not text.
pub fn is_image_codec(codec: &str) -> bool {
    matches!(
        codec,
        "hdmv_pgs_subtitle" | "dvd_subtitle" | "dvb_subtitle" | "xsub" | "pgssub" | "dvdsub"
    )
}

pub fn sub_track(
    index: u32,
    lang: &str,
    codec: &str,
    title: &str,
    default: bool,
    forced: bool,
    hearing_impaired: bool,
) -> SubTrack {
    let t = title.to_lowercase();
    SubTrack {
        index,
        lang: norm_lang(lang),
        codec: codec.into(),
        title: title.into(),
        default,
        forced: forced || t.contains("forced"),
        sdh: hearing_impaired
            || t.contains("sdh")
            || t.contains("hearing")
            || t.contains("closed caption")
            || t.contains("[cc]"),
        image: is_image_codec(codec),
    }
}

/// Pick the audio track to air: the first preferred language that exists, skipping
/// commentary and audio description unless nothing else is left; with no match,
/// the file's default track, then the first.
pub fn choose_audio(
    tracks: &[AudioTrack],
    prefs: &AudioPrefs,
    languages: &[String],
) -> Option<u32> {
    if tracks.is_empty() {
        return None;
    }
    let programme: Vec<&AudioTrack> = tracks
        .iter()
        .filter(|t| {
            !(prefs.avoid_commentary && t.commentary) && !(prefs.avoid_description && t.description)
        })
        .collect();
    let pool: Vec<&AudioTrack> = if programme.is_empty() {
        tracks.iter().collect()
    } else {
        programme
    };
    // Among equals: the file's own default, then the one with more channels, then the first.
    let best = |lang: &str| -> Option<&AudioTrack> {
        pool.iter()
            .filter(|t| t.lang == lang)
            .copied()
            .min_by_key(|t| (!t.default, std::cmp::Reverse(t.channels), t.index))
    };
    for l in languages {
        if let Some(t) = best(&norm_lang(l)) {
            return Some(t.index);
        }
    }
    pool.iter()
        .copied()
        .min_by_key(|t| (!t.default, t.index))
        .map(|t| t.index)
}

#[derive(Clone)]
struct SubCand {
    pick: SubPick,
    lang: String,
    forced: bool,
    sdh: bool,
    image: bool,
    default: bool,
}

fn candidates(t: &Tracks, can_burn_text: bool) -> Vec<SubCand> {
    let mut v: Vec<SubCand> = t
        .subs
        .iter()
        .filter(|s| s.image || can_burn_text)
        .map(|s| SubCand {
            pick: SubPick::Embedded {
                index: s.index,
                image: s.image,
            },
            lang: s.lang.clone(),
            forced: s.forced,
            sdh: s.sdh,
            image: s.image,
            default: s.default,
        })
        .collect();
    if can_burn_text {
        v.extend(t.external.iter().map(|e| SubCand {
            pick: SubPick::External(e.path.clone()),
            lang: norm_lang(&e.lang),
            forced: e.forced,
            sdh: e.sdh,
            image: false,
            default: false,
        }));
    }
    v
}

/// What to show, given the audio language that is playing.
pub fn choose_subtitle(
    tracks: &Tracks,
    audio_lang: &str,
    prefs: &SubPrefs,
    fallback_languages: &[String],
    can_burn_text: bool,
) -> Option<SubPick> {
    if prefs.mode == SubMode::Off {
        return None;
    }
    let cands = candidates(tracks, can_burn_text);
    if cands.is_empty() {
        return None;
    }
    let wanted: Vec<String> = if prefs.languages.is_empty() {
        fallback_languages.iter().map(|l| norm_lang(l)).collect()
    } else {
        prefs.languages.iter().map(|l| norm_lang(l)).collect()
    };
    let understood = wanted.iter().any(|l| l == audio_lang);
    // Forced-only: the forced fragments (foreign phrases, signs) in the viewer's language. With
    // no language wishes at all, the forced track in the audio language.
    let forced_pick = || {
        let best = |lang: &str| {
            cands
                .iter()
                .filter(|c| c.forced && c.lang == lang)
                .min_by_key(|c| (c.image, !c.default))
                .map(|c| c.pick.clone())
        };
        wanted.iter().find_map(|l| best(l)).or_else(|| {
            if wanted.is_empty() {
                best(audio_lang)
            } else {
                None
            }
        })
    };
    // A full subtitle in the preferred language (not the forced fragment track).
    let full_pick = |prefer_sdh: bool| {
        for l in &wanted {
            let best = cands
                .iter()
                .filter(|c| &c.lang == l && !c.forced)
                .min_by_key(|c| ((c.sdh != prefer_sdh) as u8, c.image, !c.default));
            if let Some(c) = best {
                return Some(c.pick.clone());
            }
        }
        None
    };
    match prefs.mode {
        SubMode::Off => None,
        SubMode::Forced => forced_pick(),
        SubMode::Always => full_pick(false).or_else(forced_pick),
        SubMode::Sdh => full_pick(true).or_else(forced_pick),
        // Subtitles exactly when the programme is not in a language you understand.
        SubMode::Foreign => {
            if understood {
                forced_pick()
            } else {
                full_pick(false).or_else(forced_pick)
            }
        }
    }
}

/// Audio first, then the subtitle that goes with it.
pub fn choose(
    tracks: &Tracks,
    audio: &AudioPrefs,
    subs: &SubPrefs,
    can_burn_text: bool,
) -> TrackChoice {
    let audio_index = choose_audio(&tracks.audio, audio, &audio.languages);
    let audio_lang = audio_index
        .and_then(|i| tracks.audio.iter().find(|t| t.index == i))
        .map(|t| t.lang.clone())
        .unwrap_or_default();
    TrackChoice {
        audio: audio_index,
        subtitle: choose_subtitle(tracks, &audio_lang, subs, &audio.languages, can_burn_text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{AudioPrefs, SubMode, SubPrefs};

    fn a(index: u32, lang: &str, ch: u32, title: &str, default: bool) -> AudioTrack {
        audio_track(index, lang, "aac", ch, title, default, false, false)
    }
    fn s(index: u32, lang: &str, title: &str, forced: bool, hi: bool, codec: &str) -> SubTrack {
        sub_track(index, lang, codec, title, false, forced, hi)
    }
    fn prefs(langs: &[&str]) -> AudioPrefs {
        AudioPrefs {
            languages: langs.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }
    fn subs(mode: SubMode, langs: &[&str]) -> SubPrefs {
        SubPrefs {
            mode,
            languages: langs.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn languages_are_normalised_however_they_are_written() {
        for (input, want) in [
            ("en", "eng"),
            ("ENG", "eng"),
            ("English", "eng"),
            ("fre", "fra"),
            ("fr", "fra"),
            ("ger", "deu"),
            ("ja", "jpn"),
            ("pt-BR", "por"),
            ("zh-TW", "zho"),
            ("und", ""),
            ("", ""),
            ("tlh", "tlh"),
        ] {
            assert_eq!(norm_lang(input), want, "{input}");
        }
    }

    #[test]
    fn the_first_preferred_language_that_exists_wins() {
        let t = vec![
            a(1, "eng", 2, "", true),
            a(2, "jpn", 2, "", false),
            a(3, "fra", 6, "", false),
        ];
        assert_eq!(choose_audio(&t, &prefs(&["jpn"]), &["jpn".into()]), Some(2));
        assert_eq!(
            choose_audio(&t, &prefs(&["deu", "fr"]), &["deu".into(), "fr".into()]),
            Some(3),
            "falls through the list"
        );
        assert_eq!(
            choose_audio(&t, &prefs(&["deu"]), &["deu".into()]),
            Some(1),
            "no match: the file's default"
        );
        assert_eq!(choose_audio(&t, &prefs(&[]), &[]), Some(1));
        assert_eq!(choose_audio(&[], &prefs(&["eng"]), &["eng".into()]), None);
    }

    #[test]
    fn commentary_and_audio_description_are_skipped_unless_nothing_else_exists() {
        let mut commentary = a(2, "eng", 2, "Director's Commentary", false);
        commentary.commentary = true;
        let desc = a(3, "eng", 2, "English (Audio Description)", false);
        assert!(desc.description);
        let t = vec![commentary.clone(), desc.clone(), a(1, "eng", 6, "", false)];
        assert_eq!(choose_audio(&t, &prefs(&["eng"]), &["eng".into()]), Some(1));
        // only the extras are English: better than silence in the wrong language? no: the language matches, so they are all that is left
        let only_extras = vec![commentary.clone(), a(5, "jpn", 2, "", true)];
        assert_eq!(
            choose_audio(&only_extras, &prefs(&["eng"]), &["eng".into()]),
            Some(5),
            "the programme track in another language beats commentary in the right one"
        );
        let mut p = prefs(&["eng"]);
        p.avoid_commentary = false;
        assert_eq!(choose_audio(&[commentary], &p, &["eng".into()]), Some(2));
    }

    #[test]
    fn among_tracks_in_one_language_prefer_default_then_more_channels() {
        let t = vec![
            a(1, "eng", 2, "", false),
            a(2, "eng", 6, "", false),
            a(3, "eng", 2, "", true),
        ];
        assert_eq!(choose_audio(&t, &prefs(&["eng"]), &["eng".into()]), Some(3));
        let t = vec![a(1, "eng", 2, "", false), a(2, "eng", 6, "", false)];
        assert_eq!(choose_audio(&t, &prefs(&["eng"]), &["eng".into()]), Some(2));
    }

    fn anime() -> Tracks {
        Tracks {
            audio: vec![a(1, "jpn", 2, "", true), a(2, "eng", 2, "", false)],
            subs: vec![
                s(3, "eng", "", false, false, "subrip"),
                s(4, "eng", "SDH", false, true, "subrip"),
                s(5, "eng", "Signs", true, false, "ass"),
                s(6, "fra", "", false, false, "subrip"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn subtitle_modes_pick_the_right_track() {
        let t = anime();
        let japanese = prefs(&["jpn"]);
        // Always: full English, not the SDH or forced ones
        assert_eq!(
            choose(&t, &japanese, &subs(SubMode::Always, &["eng"]), true).subtitle,
            Some(SubPick::Embedded {
                index: 3,
                image: false
            })
        );
        // SDH prefers the hearing-impaired track
        assert_eq!(
            choose(&t, &japanese, &subs(SubMode::Sdh, &["eng"]), true).subtitle,
            Some(SubPick::Embedded {
                index: 4,
                image: false
            })
        );
        // Forced: only the forced fragment track
        assert_eq!(
            choose(&t, &japanese, &subs(SubMode::Forced, &["eng"]), true).subtitle,
            Some(SubPick::Embedded {
                index: 5,
                image: false
            })
        );
        assert_eq!(
            choose(&t, &japanese, &subs(SubMode::Off, &["eng"]), true).subtitle,
            None
        );
        // preferred language order
        assert_eq!(
            choose(&t, &japanese, &subs(SubMode::Always, &["deu", "fra"]), true).subtitle,
            Some(SubPick::Embedded {
                index: 6,
                image: false
            })
        );
        assert_eq!(
            choose(&t, &japanese, &subs(SubMode::Always, &["deu"]), true).subtitle,
            None
        );
    }

    #[test]
    fn foreign_mode_shows_subtitles_only_when_the_audio_is_not_yours() {
        let t = anime();
        // Japanese audio, English wanted: full English subtitles
        let c = choose(
            &t,
            &prefs(&["jpn"]),
            &subs(SubMode::Foreign, &["eng"]),
            true,
        );
        assert_eq!(c.audio, Some(1));
        assert_eq!(
            c.subtitle,
            Some(SubPick::Embedded {
                index: 3,
                image: false
            })
        );
        // English audio: only the forced fragments (signs)
        let c = choose(
            &t,
            &prefs(&["eng"]),
            &subs(SubMode::Foreign, &["eng"]),
            true,
        );
        assert_eq!(c.audio, Some(2));
        assert_eq!(
            c.subtitle,
            Some(SubPick::Embedded {
                index: 5,
                image: false
            })
        );
        // subtitle languages default to the audio languages when not given
        let c = choose(&t, &prefs(&["eng"]), &subs(SubMode::Foreign, &[]), true);
        assert_eq!(
            c.subtitle,
            Some(SubPick::Embedded {
                index: 5,
                image: false
            })
        );
    }

    #[test]
    fn without_a_text_renderer_only_picture_subtitles_are_offered() {
        let mut t = anime();
        t.subs
            .push(s(7, "eng", "", false, false, "hdmv_pgs_subtitle"));
        t.external.push(ExternalSub {
            path: "/tv/x.en.srt".into(),
            lang: "en".into(),
            forced: false,
            sdh: false,
        });
        let c = choose(
            &t,
            &prefs(&["jpn"]),
            &subs(SubMode::Always, &["eng"]),
            false,
        );
        assert_eq!(
            c.subtitle,
            Some(SubPick::Embedded {
                index: 7,
                image: true
            })
        );
        // with a renderer, text wins over the picture track
        let c = choose(&t, &prefs(&["jpn"]), &subs(SubMode::Always, &["eng"]), true);
        assert_eq!(
            c.subtitle,
            Some(SubPick::Embedded {
                index: 3,
                image: false
            })
        );
    }

    #[test]
    fn a_sidecar_file_is_used_when_the_video_has_no_subtitles() {
        let t = Tracks {
            audio: vec![a(1, "jpn", 2, "", true)],
            external: vec![ExternalSub {
                path: "/tv/Show.en.srt".into(),
                lang: "en".into(),
                forced: false,
                sdh: false,
            }],
            ..Default::default()
        };
        let c = choose(&t, &prefs(&["jpn"]), &subs(SubMode::Always, &["eng"]), true);
        assert_eq!(
            c.subtitle,
            Some(SubPick::External("/tv/Show.en.srt".into()))
        );
    }

    #[test]
    fn titles_reveal_commentary_forced_and_sdh() {
        assert!(audio_track(1, "eng", "ac3", 2, "Cast Commentary", false, false, false).commentary);
        assert!(sub_track(1, "eng", "subrip", "English [Forced]", false, false, false).forced);
        assert!(sub_track(1, "eng", "subrip", "English SDH", false, false, false).sdh);
        assert!(sub_track(1, "eng", "hdmv_pgs_subtitle", "", false, false, false).image);
        assert!(!sub_track(1, "eng", "ass", "", false, false, false).image);
    }

    #[test]
    fn tracks_survive_json_and_tolerate_old_rows() {
        let t = anime();
        assert_eq!(Tracks::from_json(&t.to_json()), Some(t));
        assert_eq!(Tracks::from_json("{}"), Some(Tracks::default()));
        assert_eq!(Tracks::from_json("nonsense"), None);
    }
}
