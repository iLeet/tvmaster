//! The channel's view of its library: shows with ordered episodes.

use crate::daypart::Rating;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct MediaRef {
    pub id: i64,
    pub show: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// Tie-breaker (usually the file path).
    pub sort_key: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ep {
    pub id: i64,
    pub dur_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ShowEps {
    pub name: String,
    /// In airing order.
    pub eps: Vec<Ep>,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Keyed by lower-cased show name. Files without a show share the key "".
    shows: BTreeMap<String, ShowEps>,
    /// The rating of each rated show, by lower-cased name. A show missing from here is unrated.
    ratings: BTreeMap<String, Rating>,
}

impl Catalog {
    pub fn build(media: &[MediaRef]) -> Self {
        let mut grouped: BTreeMap<String, (String, Vec<&MediaRef>)> = BTreeMap::new();
        for m in media.iter().filter(|m| m.duration_ms > 0) {
            let name = m.show.clone().unwrap_or_default();
            grouped
                .entry(name.to_lowercase())
                .or_insert_with(|| (name, Vec::new()))
                .1
                .push(m);
        }
        let shows = grouped
            .into_iter()
            .map(|(key, (name, mut v))| {
                v.sort_by(|a, b| {
                    (a.season, a.episode, &a.sort_key, a.id).cmp(&(
                        b.season,
                        b.episode,
                        &b.sort_key,
                        b.id,
                    ))
                });
                let eps = v
                    .into_iter()
                    .map(|m| Ep {
                        id: m.id,
                        dur_ms: m.duration_ms,
                    })
                    .collect();
                (key, ShowEps { name, eps })
            })
            .collect();
        Self {
            shows,
            ratings: BTreeMap::new(),
        }
    }

    /// Say how each show is rated (keys are lower-cased show names).
    pub fn with_ratings(mut self, ratings: BTreeMap<String, Rating>) -> Self {
        self.ratings = ratings;
        self
    }

    /// Whether a show may air under a limit: `cap` is the highest rating allowed and whether
    /// shows without one may air. No limit allows everything.
    pub fn allowed(&self, key: &str, cap: Option<(Rating, bool)>) -> bool {
        match cap {
            None => true,
            Some((max, unrated_ok)) => match self.ratings.get(key) {
                Some(r) => *r <= max,
                None => unrated_ok,
            },
        }
    }

    /// How many shows are rated, and how many are not.
    pub fn rated_counts(&self) -> (usize, usize) {
        let rated = self
            .shows
            .keys()
            .filter(|k| self.ratings.contains_key(*k))
            .count();
        (rated, self.shows.len() - rated)
    }

    pub fn is_empty(&self) -> bool {
        self.shows.is_empty()
    }

    pub fn show(&self, name: &str) -> Option<&ShowEps> {
        self.shows.get(&name.to_lowercase())
    }

    /// Lower-cased keys of every show, in stable order.
    pub fn show_keys(&self) -> Vec<String> {
        self.shows.keys().cloned().collect()
    }

    pub fn by_key(&self, key: &str) -> Option<&ShowEps> {
        self.shows.get(key)
    }

    pub fn episode_count(&self) -> usize {
        self.shows.values().map(|s| s.eps.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: i64, show: Option<&str>, s: u32, e: u32) -> MediaRef {
        MediaRef {
            id,
            show: show.map(str::to_string),
            season: Some(s),
            episode: Some(e),
            sort_key: id.to_string(),
            duration_ms: 1000,
        }
    }

    #[test]
    fn groups_case_insensitively_and_orders_episodes() {
        let c = Catalog::build(&[
            m(1, Some("Alpha"), 1, 2),
            m(2, Some("alpha"), 1, 1),
            m(3, Some("Alpha"), 2, 1),
            m(4, None, 1, 1),
        ]);
        let ids: Vec<i64> = c.show("ALPHA").unwrap().eps.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![2, 1, 3]);
        assert_eq!(c.show_keys(), vec!["".to_string(), "alpha".to_string()]);
        assert_eq!(c.episode_count(), 4);
    }

    #[test]
    fn ignores_zero_length_media() {
        let mut z = m(1, Some("A"), 1, 1);
        z.duration_ms = 0;
        assert!(Catalog::build(&[z]).is_empty());
    }
}
