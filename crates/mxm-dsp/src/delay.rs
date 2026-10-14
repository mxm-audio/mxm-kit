//! The line read by age.
//!
//! Seven effects and four synths kept the same ring buffer: samples pushed in, read back by how many
//! pushes ago they arrived, zero past what was written, history forgotten in constant time. Two of
//! them document the same fix — a fractional age split into whole and fraction **before** wrapping,
//! because wrapping the whole position in `f32` could round a point just below the buffer's end onto
//! it. This is that line once (the architecture audit's O2); each consumer keeps its own interpolation
//! where it is not the linear read here, and its own clamps and flushes at its call site.

use std::collections::TryReserveError;

/// A circular line of samples read by **age**: age 1 is the sample most recently pushed, age `n` the
/// one pushed `n` pushes ago.
///
/// A read at age 0, or older than what has been pushed since the last [`clear`](Self::clear), is exact
/// zero, which is what lets clearing cost nothing however long the line. The line holds exactly the
/// length it was made with, so a consumer keeps its memory and its history; the index wraps by one
/// compare and subtract, which selects the same sample a power-of-two mask would.
#[derive(Debug, Clone)]
pub struct AgeLine {
    buf: Vec<f32>,
    write: usize,
    written: usize,
}

impl AgeLine {
    /// A line of exactly `len` samples (one, if `len` is zero), holding ages 1 to `len`. Allocates:
    /// call outside the audio thread.
    pub fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            write: 0,
            written: 0,
        }
    }

    /// [`new`](Self::new), reporting a length whose memory cannot be had (or whose size overflows)
    /// instead of aborting, for a consumer whose construction is fallible.
    pub fn try_new(len: usize) -> Result<Self, TryReserveError> {
        let len = len.max(1);
        let mut buf = Vec::new();
        buf.try_reserve_exact(len)?;
        buf.resize(len, 0.0);
        Ok(Self {
            buf,
            write: 0,
            written: 0,
        })
    }

    /// The oldest age the line can hold: its length.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// How many samples have been pushed since the last [`clear`](Self::clear), at most
    /// [`capacity`](Self::capacity): the oldest age that reads a written sample.
    #[inline]
    pub fn written(&self) -> usize {
        self.written
    }

    /// Pushes `x` as the new age 1. Nothing is flushed or clamped: a consumer's write policy stays at
    /// its call site.
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.write] = x;
        self.write += 1;
        if self.write == self.buf.len() {
            self.write = 0;
        }
        if self.written < self.buf.len() {
            self.written += 1;
        }
    }

    /// The sample pushed `age` pushes ago; zero at age 0 and past what has been written.
    #[inline]
    pub fn read(&self, age: usize) -> f32 {
        if age == 0 || age > self.written {
            return 0.0;
        }
        // `write < len` and `age <= len`, so the sum is inside two laps and one subtract wraps it.
        let len = self.buf.len();
        let index = self.write + len - age;
        self.buf[if index >= len { index - len } else { index }]
    }

    /// A fractional age, linearly interpolated: `a + (b − a) · f` between ages `⌊age⌋` and `⌊age⌋ + 1`,
    /// the whole and the fraction split before any index is formed. No clamp: an age under one reads
    /// toward the zero at age 0, and a non-finite age reads NaN; every consumer clamps first.
    #[inline]
    pub fn read_linear(&self, age: f32) -> f32 {
        let whole = age.floor();
        let frac = age - whole;
        let whole = whole as usize;
        let a = self.read(whole);
        let b = self.read(whole.saturating_add(1));
        a + (b - a) * frac
    }

    /// Forgets every sample, in constant time: nothing is zeroed, the history is invalidated.
    #[inline]
    pub fn clear(&mut self) {
        self.write = 0;
        self.written = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The masked line the copies kept, as the reference a power-of-two `AgeLine` must equal.
    struct Masked {
        buf: Vec<f32>,
        write: usize,
        valid: usize,
    }

    impl Masked {
        fn push(&mut self, x: f32) {
            let mask = self.buf.len() - 1;
            self.buf[self.write] = x;
            self.write = (self.write + 1) & mask;
            if self.valid < self.buf.len() {
                self.valid += 1;
            }
        }

        fn read(&self, age: usize) -> f32 {
            if age == 0 || age > self.valid {
                return 0.0;
            }
            self.buf[(self.write + self.buf.len() - age) & (self.buf.len() - 1)]
        }
    }

    #[test]
    fn ages_count_back_from_the_newest_and_read_zero_past_the_history() {
        let mut line = AgeLine::new(5);
        for x in 1..=3 {
            line.push(x as f32);
        }
        assert_eq!(line.read(0), 0.0);
        assert_eq!(line.read(1), 3.0);
        assert_eq!(line.read(3), 1.0);
        assert_eq!(line.read(4), 0.0, "never written");
        assert_eq!(line.written(), 3);
        for x in 4..=9 {
            line.push(x as f32);
        }
        assert_eq!(line.written(), 5, "saturates at the length");
        assert_eq!(line.read(1), 9.0);
        assert_eq!(line.read(5), 5.0, "the oldest held, across the wrap");
        assert_eq!(line.read(6), 0.0);
        assert_eq!(line.read(usize::MAX), 0.0);
    }

    #[test]
    fn clearing_forgets_in_constant_time() {
        let mut line = AgeLine::new(4);
        for x in 1..=6 {
            line.push(x as f32);
        }
        line.clear();
        assert_eq!(line.written(), 0);
        assert!((0..=5).all(|age| line.read(age) == 0.0));
        line.push(7.0);
        assert_eq!(line.read(1), 7.0);
        assert_eq!(line.read(2), 0.0);
    }

    #[test]
    fn a_power_of_two_line_reads_what_the_masked_copies_read() {
        for len in [1usize, 2, 8, 64] {
            let mut line = AgeLine::new(len);
            let mut reference = Masked {
                buf: vec![0.0; len],
                write: 0,
                valid: 0,
            };
            for n in 0..(3 * len + 5) {
                let x = (n as f32 * 0.37).sin();
                line.push(x);
                reference.push(x);
                for age in 0..=len + 1 {
                    assert_eq!(line.read(age).to_bits(), reference.read(age).to_bits());
                }
            }
        }
    }

    #[test]
    fn the_linear_read_splits_before_it_wraps() {
        // A position just under a long line's end: wrapped whole in f32 it rounds onto the end.
        let len = 1 << 20;
        let mut line = AgeLine::new(len);
        for n in 0..len {
            line.push(n as f32);
        }
        let age = (len - 1) as f32 + 0.75;
        let expected =
            line.read(len - 1) + (line.read(len) - line.read(len - 1)) * (age - age.floor());
        assert_eq!(line.read_linear(age).to_bits(), expected.to_bits());
        assert_eq!(line.read_linear(1.0), (len - 1) as f32);
        assert_eq!(line.read_linear(1.5), (len - 1) as f32 - 0.5);
        assert_eq!(
            line.read_linear(f32::MAX),
            0.0,
            "far past the history, no overflow"
        );
    }

    #[test]
    fn construction_is_exact_and_fallible_on_request() {
        assert_eq!(AgeLine::new(0).capacity(), 1);
        assert_eq!(AgeLine::new(1_001).capacity(), 1_001);
        assert_eq!(AgeLine::try_new(48_003).unwrap().capacity(), 48_003);
        assert!(AgeLine::try_new(usize::MAX / 2).is_err());
    }
}
