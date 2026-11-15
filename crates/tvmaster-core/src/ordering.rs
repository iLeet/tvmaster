//! Builds the play order for sequential channels: episodes in order within a
//! show, shows interleaved round-robin so a multi-show channel feels like TV.

use crate::timeline::Entry;
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

pub fn order_sequential(media: &[MediaRef]) -> Vec<Entry> {
    let mut groups: BTreeMap<String, Vec<&MediaRef>> = BTreeMap::new();
    for m in media {
        groups
            .entry(m.show.clone().unwrap_or_default().to_lowercase())
            .or_default()
            .push(m);
    }
    for g in groups.values_mut() {
        g.sort_by(|a, b| {
            (a.season, a.episode, &a.sort_key).cmp(&(b.season, b.episode, &b.sort_key))
        });
    }
    let mut out = Vec::with_capacity(media.len());
    let mut i = 0;
    loop {
        let mut any = false;
        for g in groups.values() {
            if let Some(m) = g.get(i) {
                out.push(Entry {
                    media_id: m.id,
                    duration_ms: m.duration_ms,
                });
                any = true;
            }
        }
        if !any {
            break;
        }
        i += 1;
    }
    out
}

pub fn order_unsorted(media: &[MediaRef]) -> Vec<Entry> {
    let mut v: Vec<&MediaRef> = media.iter().collect();
    v.sort_by_key(|m| m.id);
    v.into_iter()
        .map(|m| Entry {
            media_id: m.id,
            duration_ms: m.duration_ms,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: i64, show: &str, s: u32, e: u32) -> MediaRef {
        MediaRef {
            id,
            show: Some(show.into()),
            season: Some(s),
            episode: Some(e),
            sort_key: format!("{id}"),
            duration_ms: 1000,
        }
    }

    #[test]
    fn episodes_in_order_shows_interleaved() {
        let media = vec![
            m(1, "B Show", 1, 2),
            m(2, "A Show", 1, 2),
            m(3, "A Show", 1, 1),
            m(4, "B Show", 1, 1),
            m(5, "A Show", 2, 1),
        ];
        let ids: Vec<i64> = order_sequential(&media)
            .into_iter()
            .map(|e| e.media_id)
            .collect();
        // A1, B1, A2, B2, then A's season 2
        assert_eq!(ids, vec![3, 4, 2, 1, 5]);
    }
}
