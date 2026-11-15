//! A deterministic, endless timeline: the channel plays its entries back to
//! back from `epoch_ms`, repeating forever. In shuffle mode each pass uses a
//! different order derived from `seed`, so the same (seed, time) always gives
//! the same programme. This is the M2 engine; block grids, cursors and run-out
//! policies (M3) build on the same idea.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub media_id: i64,
    pub duration_ms: u64,
}

/// One item on the timeline with absolute start/end times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub media_id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
}

impl Slot {
    /// How far into this item `at_ms` falls (0 if before the start).
    pub fn offset_at(&self, at_ms: i64) -> u64 {
        (at_ms - self.start_ms).max(0) as u64
    }

    pub fn duration_ms(&self) -> u64 {
        (self.end_ms - self.start_ms).max(0) as u64
    }
}

#[derive(Debug, Clone)]
pub struct Timeline {
    entries: Vec<Entry>,
    epoch_ms: i64,
    seed: u64,
    shuffle: bool,
    cycle_ms: u64,
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Timeline {
    /// Returns `None` if there is nothing playable. Zero-length entries are dropped.
    pub fn new(entries: Vec<Entry>, epoch_ms: i64, seed: u64, shuffle: bool) -> Option<Self> {
        let entries: Vec<Entry> = entries.into_iter().filter(|e| e.duration_ms > 0).collect();
        if entries.is_empty() {
            return None;
        }
        let cycle_ms = entries.iter().map(|e| e.duration_ms).sum();
        Some(Self {
            entries,
            epoch_ms,
            seed,
            shuffle,
            cycle_ms,
        })
    }

    pub fn cycle_ms(&self) -> u64 {
        self.cycle_ms
    }

    fn order(&self, cycle: u64) -> Vec<usize> {
        let n = self.entries.len();
        let mut v: Vec<usize> = (0..n).collect();
        if self.shuffle {
            let mut s = self.seed ^ cycle.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            for i in (1..n).rev() {
                let j = (splitmix64(&mut s) % (i as u64 + 1)) as usize;
                v.swap(i, j);
            }
        }
        v
    }

    /// Iterate slots starting with the one that contains `at_ms`. Never ends.
    pub fn iter_from(&self, at_ms: i64) -> SlotIter<'_> {
        let elapsed = (at_ms - self.epoch_ms).max(0) as u64;
        let cycle = elapsed / self.cycle_ms;
        let mut within = elapsed % self.cycle_ms;
        let order = self.order(cycle);
        let mut start_ms = self.epoch_ms + (cycle * self.cycle_ms) as i64;
        let mut pos = 0;
        while within >= self.entries[order[pos]].duration_ms {
            let d = self.entries[order[pos]].duration_ms;
            within -= d;
            start_ms += d as i64;
            pos += 1;
        }
        SlotIter {
            tl: self,
            cycle,
            order,
            pos,
            start_ms,
        }
    }

    /// The slot on air at `at_ms`.
    pub fn locate(&self, at_ms: i64) -> Slot {
        self.iter_from(at_ms)
            .next()
            .expect("timeline is never empty")
    }
}

pub struct SlotIter<'a> {
    tl: &'a Timeline,
    cycle: u64,
    order: Vec<usize>,
    pos: usize,
    start_ms: i64,
}

impl Iterator for SlotIter<'_> {
    type Item = Slot;

    fn next(&mut self) -> Option<Slot> {
        let e = &self.tl.entries[self.order[self.pos]];
        let slot = Slot {
            media_id: e.media_id,
            start_ms: self.start_ms,
            end_ms: self.start_ms + e.duration_ms as i64,
        };
        self.start_ms = slot.end_ms;
        self.pos += 1;
        if self.pos == self.order.len() {
            self.cycle += 1;
            self.order = self.tl.order(self.cycle);
            self.pos = 0;
        }
        Some(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        vec![
            Entry {
                media_id: 1,
                duration_ms: 1000,
            },
            Entry {
                media_id: 2,
                duration_ms: 2000,
            },
            Entry {
                media_id: 3,
                duration_ms: 3000,
            },
        ]
    }

    #[test]
    fn empty_or_zero_length_is_none() {
        assert!(Timeline::new(vec![], 0, 0, false).is_none());
        let z = vec![Entry {
            media_id: 1,
            duration_ms: 0,
        }];
        assert!(Timeline::new(z, 0, 0, false).is_none());
    }

    #[test]
    fn sequential_locates_and_wraps() {
        let t = Timeline::new(entries(), 10_000, 0, false).unwrap();
        let s = t.locate(10_500);
        assert_eq!((s.media_id, s.start_ms, s.end_ms), (1, 10_000, 11_000));
        assert_eq!(s.offset_at(10_500), 500);
        assert_eq!(t.locate(11_000).media_id, 2);
        assert_eq!(t.locate(15_999).media_id, 3);
        // second pass starts at epoch + 6000
        let s = t.locate(16_000);
        assert_eq!((s.media_id, s.start_ms), (1, 16_000));
    }

    #[test]
    fn slots_are_contiguous_and_endless() {
        let t = Timeline::new(entries(), 0, 7, true).unwrap();
        let slots: Vec<Slot> = t.iter_from(2_500).take(20).collect();
        for w in slots.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms);
        }
        assert!(slots[0].start_ms <= 2_500 && 2_500 < slots[0].end_ms);
    }

    #[test]
    fn shuffle_is_deterministic_and_covers_each_entry_per_pass() {
        let a = Timeline::new(entries(), 0, 42, true).unwrap();
        let b = Timeline::new(entries(), 0, 42, true).unwrap();
        let sa: Vec<_> = a.iter_from(0).take(9).map(|s| s.media_id).collect();
        let sb: Vec<_> = b.iter_from(0).take(9).map(|s| s.media_id).collect();
        assert_eq!(sa, sb);
        for pass in sa.chunks(3) {
            let mut p = pass.to_vec();
            p.sort();
            assert_eq!(p, vec![1, 2, 3]);
        }
    }

    #[test]
    fn locating_midway_matches_iterating_from_start() {
        let t = Timeline::new(entries(), 0, 99, true).unwrap();
        let from_start: Vec<Slot> = t.iter_from(0).take(30).collect();
        for s in &from_start {
            assert_eq!(t.locate(s.start_ms + 1), *s);
        }
    }

    #[test]
    fn before_epoch_starts_at_first_item() {
        let t = Timeline::new(entries(), 10_000, 0, false).unwrap();
        let s = t.locate(0);
        assert_eq!((s.media_id, s.start_ms), (1, 10_000));
        assert_eq!(s.offset_at(0), 0);
    }
}
