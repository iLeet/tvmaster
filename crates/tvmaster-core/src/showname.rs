//! Show names: what a folder is called is not always what the station should
//! call the show. "Bleach season 01" is the show "Bleach", season 1.

use regex::Regex;
use std::sync::OnceLock;

fn re_suffix() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)^(.*?)[\s._\-–,(\[]*\b(?:season|series|saison|staffel|temporada|book)[\s._\-]*0*(\d{1,2})[\s._\-)\]]*$")
            .unwrap()
    })
}

fn re_short() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^(.*\S)[\s._\-–]+s0*(\d{1,2})$").unwrap())
}

/// Split a trailing season off a folder name: "Bleach season 01" → ("Bleach", 1),
/// "Bleach - S2" → ("Bleach", 2). Names without one come back unchanged.
pub fn split_season_suffix(name: &str) -> (String, Option<i64>) {
    let name = name.trim();
    for re in [re_suffix(), re_short()] {
        if let Some(c) = re.captures(name) {
            let base = c[1]
                .trim_matches(|ch: char| ch.is_whitespace() || "-–_.,([".contains(ch))
                .to_string();
            if !base.is_empty() {
                if let Ok(n) = c[2].parse::<i64>() {
                    return (base, Some(n));
                }
            }
        }
    }
    (name.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trailing_season_is_split_off_the_folder_name() {
        let t = |s: &str| split_season_suffix(s);
        assert_eq!(t("Bleach season 01"), ("Bleach".into(), Some(1)));
        assert_eq!(t("Bleach Season 2"), ("Bleach".into(), Some(2)));
        assert_eq!(t("Bleach - Season 03"), ("Bleach".into(), Some(3)));
        assert_eq!(t("Bleach (Season 4)"), ("Bleach".into(), Some(4)));
        assert_eq!(t("Bleach.Series.5"), ("Bleach".into(), Some(5)));
        assert_eq!(t("Cowboy Bebop S1"), ("Cowboy Bebop".into(), Some(1)));
        assert_eq!(t("Cowboy Bebop - s02"), ("Cowboy Bebop".into(), Some(2)));
    }

    #[test]
    fn other_names_are_left_alone() {
        for n in ["Bleach", "Season 1", "S Club 7", "Seasons of Love", "Show S", "Mr. Bean", "24", "The Office (US)"] {
            assert_eq!(split_season_suffix(n), (n.to_string(), None), "{n}");
        }
    }
}
