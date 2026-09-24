//! The byte range one connection is responsible for inside a shared partial
//! file. The range can shrink while the connection runs: when another
//! connection is idle, the untouched half of the largest remaining range is
//! handed to it.

use std::sync::Mutex;

#[derive(Debug)]
struct SlotState {
    start: u64,
    /// Inclusive; only ever lowered, and never below `next - 1`.
    end: u64,
    /// The next byte to be claimed by the connection writing this range.
    next: u64,
}

#[derive(Debug)]
pub struct RangeSlot {
    state: Mutex<SlotState>,
}

impl RangeSlot {
    /// A range `[start, end]` of which the first `downloaded` bytes are
    /// already on disk.
    pub fn new(start: u64, end: u64, downloaded: u64) -> Self {
        Self {
            state: Mutex::new(SlotState {
                start,
                end,
                next: start.saturating_add(downloaded).min(end.saturating_add(1)),
            }),
        }
    }

    fn with<T>(&self, action: impl FnOnce(&mut SlotState) -> T) -> T {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        action(&mut guard)
    }

    pub fn start(&self) -> u64 {
        self.with(|state| state.start)
    }

    pub fn end(&self) -> u64 {
        self.with(|state| state.end)
    }

    pub fn next(&self) -> u64 {
        self.with(|state| state.next)
    }

    /// Bytes not yet claimed.
    pub fn remaining(&self) -> u64 {
        self.with(|state| (state.end + 1).saturating_sub(state.next))
    }

    /// Claims up to `wanted` bytes for writing. Returns the offset they go
    /// to and how many may be written; fewer than `wanted` (possibly zero)
    /// means the range ends there.
    pub fn claim(&self, wanted: u64) -> (u64, u64) {
        self.with(|state| {
            let offset = state.next;
            let allowed = wanted.min((state.end + 1).saturating_sub(offset));
            state.next = offset + allowed;
            (offset, allowed)
        })
    }

    /// Gives away the unclaimed second half of the range when both halves
    /// would be at least `min_piece` bytes. Returns the inclusive range that
    /// was given away; this slot then ends just before it.
    pub fn split_off(&self, min_piece: u64) -> Option<(u64, u64)> {
        self.with(|state| {
            let remaining = (state.end + 1).saturating_sub(state.next);
            if min_piece == 0 || remaining < min_piece.saturating_mul(2) {
                return None;
            }
            let tail_start = state.next + remaining / 2;
            let tail_end = state.end;
            state.end = tail_start - 1;
            Some((tail_start, tail_end))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_stop_exactly_at_the_end_of_the_range() {
        let slot = RangeSlot::new(10, 19, 0);
        assert_eq!(slot.claim(6), (10, 6));
        assert_eq!(slot.claim(6), (16, 4));
        assert_eq!(slot.claim(6), (20, 0));
        assert_eq!(slot.remaining(), 0);
    }

    #[test]
    fn resumes_after_the_bytes_already_on_disk() {
        let slot = RangeSlot::new(100, 199, 40);
        assert_eq!(slot.next(), 140);
        assert_eq!(slot.remaining(), 60);
    }

    #[test]
    fn splitting_gives_away_the_unclaimed_half_only() {
        let slot = RangeSlot::new(0, 99, 0);
        slot.claim(20);

        let (tail_start, tail_end) = slot.split_off(10).unwrap();
        assert_eq!((tail_start, tail_end), (60, 99));
        assert_eq!(slot.end(), 59);

        // What was claimed before the split is untouched and the two ranges
        // meet without a gap.
        assert_eq!(slot.claim(100), (20, 40));
    }

    #[test]
    fn small_remainders_are_not_split() {
        let slot = RangeSlot::new(0, 99, 85);
        assert_eq!(slot.split_off(10), None);
        assert_eq!(slot.end(), 99);
    }
}
