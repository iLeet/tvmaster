//! MPEG-TS continuity fixing.
//!
//! Every item is encoded by its own FFmpeg process, and each one restarts the
//! per-PID continuity counters at zero. Concatenated naively, the demuxer sees a
//! counter jump at every item boundary and flags the first packets as corrupt
//! (dropping part of a keyframe). We rewrite the counters so the joined stream
//! looks like one continuous transport stream.

const PACKET: usize = 188;
const SYNC: u8 = 0x47;
const NULL_PID: u16 = 0x1fff;

pub struct ContinuityFixer {
    /// Next expected counter per PID.
    next: Vec<Option<u8>>,
    carry: Vec<u8>,
}

impl Default for ContinuityFixer {
    fn default() -> Self {
        Self::new()
    }
}

impl ContinuityFixer {
    pub fn new() -> Self {
        Self {
            next: vec![None; 8192],
            carry: Vec::new(),
        }
    }

    /// Feed arbitrary-sized chunks; returns only whole, fixed-up packets.
    pub fn process(&mut self, data: &[u8]) -> Vec<u8> {
        self.carry.extend_from_slice(data);
        let mut out = Vec::with_capacity(self.carry.len());
        let mut pos = 0;
        while self.carry.len() - pos >= PACKET {
            if self.carry[pos] != SYNC {
                pos += 1; // resync
                continue;
            }
            let mut pkt = [0u8; PACKET];
            pkt.copy_from_slice(&self.carry[pos..pos + PACKET]);
            self.fix(&mut pkt);
            out.extend_from_slice(&pkt);
            pos += PACKET;
        }
        self.carry.drain(..pos);
        out
    }

    /// Discard a trailing partial packet (call at the end of an item).
    pub fn end_item(&mut self) {
        self.carry.clear();
    }

    fn fix(&mut self, pkt: &mut [u8; PACKET]) {
        let pid = (((pkt[1] & 0x1f) as u16) << 8) | pkt[2] as u16;
        if pid == NULL_PID {
            return;
        }
        let has_payload = (pkt[3] >> 4) & 0x1 != 0;
        let slot = &mut self.next[pid as usize];
        if has_payload {
            let cc = slot.unwrap_or(pkt[3] & 0x0f);
            pkt[3] = (pkt[3] & 0xf0) | cc;
            *slot = Some((cc + 1) & 0x0f);
        } else if let Some(n) = *slot {
            // Adaptation-field-only packets repeat the previous counter.
            pkt[3] = (pkt[3] & 0xf0) | (n.wrapping_sub(1) & 0x0f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkt(pid: u16, cc: u8, afc: u8) -> Vec<u8> {
        let mut p = vec![0u8; PACKET];
        p[0] = SYNC;
        p[1] = (pid >> 8) as u8 & 0x1f;
        p[2] = pid as u8;
        p[3] = (afc << 4) | (cc & 0x0f);
        p
    }

    fn ccs(data: &[u8]) -> Vec<u8> {
        data.chunks(PACKET).map(|p| p[3] & 0x0f).collect()
    }

    #[test]
    fn counters_stay_continuous_across_items() {
        let mut f = ContinuityFixer::new();
        // Item 1: cc 0..3 on PID 0x100
        let item1: Vec<u8> = (0..4).flat_map(|c| pkt(0x100, c, 1)).collect();
        // Item 2 restarts at 0
        let item2: Vec<u8> = (0..4).flat_map(|c| pkt(0x100, c, 1)).collect();
        let mut out = f.process(&item1);
        f.end_item();
        out.extend(f.process(&item2));
        assert_eq!(ccs(&out), vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn counters_wrap_and_pids_are_independent() {
        let mut f = ContinuityFixer::new();
        let mut input = Vec::new();
        for _ in 0..20 {
            input.extend(pkt(0x100, 0, 1));
            input.extend(pkt(0x101, 7, 1));
        }
        let out = f.process(&input);
        let video: Vec<u8> = out
            .chunks(PACKET)
            .filter(|p| p[2] == 0x00 && p[1] & 0x1f == 0x01)
            .map(|p| p[3] & 0x0f)
            .collect();
        assert_eq!(video.len(), 20);
        for w in video.windows(2) {
            assert_eq!(w[1], (w[0] + 1) & 0x0f);
        }
        // PID 0x101 started at its own counter (7) and continues from there.
        let audio: Vec<u8> = out
            .chunks(PACKET)
            .filter(|p| p[2] == 0x01 && p[1] & 0x1f == 0x01)
            .map(|p| p[3] & 0x0f)
            .collect();
        assert_eq!(audio[0], 7);
        assert_eq!(audio[1], 8);
    }

    #[test]
    fn handles_split_chunks_and_reports_only_whole_packets() {
        let mut f = ContinuityFixer::new();
        let data: Vec<u8> = (0..3).flat_map(|c| pkt(0x100, c, 1)).collect();
        let a = f.process(&data[..100]);
        assert!(a.is_empty());
        let b = f.process(&data[100..400]);
        assert_eq!(b.len(), PACKET * 2);
        let c = f.process(&data[400..]);
        assert_eq!(c.len(), PACKET);
    }

    #[test]
    fn adaptation_only_packets_repeat_counter_and_null_packets_untouched() {
        let mut f = ContinuityFixer::new();
        let mut input = Vec::new();
        input.extend(pkt(0x100, 5, 1)); // payload, cc 5
        input.extend(pkt(0x100, 9, 2)); // adaptation only -> should repeat 5
        input.extend(pkt(NULL_PID, 3, 1)); // null: untouched
        let out = f.process(&input);
        assert_eq!(ccs(&out), vec![5, 5, 3]);
    }

    #[test]
    fn resyncs_after_garbage() {
        let mut f = ContinuityFixer::new();
        let mut input = vec![0u8, 1, 2];
        input.extend(pkt(0x100, 0, 1));
        let out = f.process(&input);
        assert_eq!(out.len(), PACKET);
    }
}
