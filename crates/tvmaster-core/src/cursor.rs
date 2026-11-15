use serde::{Deserialize, Serialize};

/// Tracks position in an ordered episode list.
///
/// `reserved` counts episodes already placed on the schedule; `committed`
/// counts episodes that actually aired. Re-rolling the schedule resets
/// `reserved` to `committed`, so changing settings never burns episodes.
/// Positions are absolute counts: the episode index is `count % total` and the
/// number of completed passes is `count / total`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeCursor {
    pub reserved: u64,
    pub committed: u64,
}

impl EpisodeCursor {
    /// Reserve the next episode index. With `wrap`, loops back to the first
    /// episode after the last; without it, returns `None` once exhausted so the
    /// caller can apply a run-out policy.
    pub fn reserve_next(&mut self, total: u64, wrap: bool) -> Option<u64> {
        if total == 0 {
            return None;
        }
        if !wrap && self.reserved >= total {
            return None;
        }
        let index = self.reserved % total;
        self.reserved += 1;
        Some(index)
    }

    /// Mark `n` episodes as aired (never moves backwards).
    pub fn commit(&mut self, n: u64) {
        self.committed = self.committed.max(n);
    }

    /// Drop reservations that have not aired (used when re-rolling).
    pub fn reset_reservations(&mut self) {
        self.reserved = self.committed;
    }

    /// Full passes through the list completed so far.
    pub fn passes(&self, total: u64) -> u64 {
        self.committed.checked_div(total).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_after_last_episode() {
        let mut c = EpisodeCursor::default();
        let got: Vec<_> = (0..5).filter_map(|_| c.reserve_next(3, true)).collect();
        assert_eq!(got, vec![0, 1, 2, 0, 1]);
    }

    #[test]
    fn stops_without_wrap() {
        let mut c = EpisodeCursor::default();
        assert_eq!(c.reserve_next(2, false), Some(0));
        assert_eq!(c.reserve_next(2, false), Some(1));
        assert_eq!(c.reserve_next(2, false), None);
    }

    #[test]
    fn empty_list_yields_none() {
        let mut c = EpisodeCursor::default();
        assert_eq!(c.reserve_next(0, true), None);
    }

    #[test]
    fn reroll_does_not_burn_episodes() {
        let mut c = EpisodeCursor::default();
        for _ in 0..4 {
            c.reserve_next(10, true);
        }
        c.commit(1);
        c.reset_reservations();
        assert_eq!(c.reserve_next(10, true), Some(1));
    }

    #[test]
    fn commit_never_moves_backwards() {
        let mut c = EpisodeCursor::default();
        c.commit(5);
        c.commit(2);
        assert_eq!(c.committed, 5);
        assert_eq!(c.passes(3), 1);
    }
}
