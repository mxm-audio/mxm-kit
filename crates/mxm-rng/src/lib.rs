//! The collection's pseudo-random generators: five families, each bit-exact to the copies it
//! replaced, so every golden digest and recorded render that depends on a stream still holds.
//!
//! | Generator | Source |
//! |---|---|
//! | [`Xorshift32`] | Marsaglia, *Xorshift RNGs*, Journal of Statistical Software 8(14), 2003: the (13, 17, 5) triple |
//! | [`Xorshift64`] | the same paper: the (13, 7, 17) triple |
//! | [`Xorshift64Star`] | Vigna, *An experimental exploration of Marsaglia's xorshift generators, scrambled*, ACM TOMS 42(4), 2016: xorshift64\* with (12, 25, 27) and the multiplier `0x2545_F491_4F6C_DD1D` |
//! | [`SplitMix64`] | Steele, Lea & Flood, *Fast splittable pseudorandom number generators*, OOPSLA 2014, with Stafford's *Mix13* output function |
//! | [`Lcg32`] | Press et al., *Numerical Recipes in C*, 2nd ed., 1992, §7.1: the "quick and dirty" `ranqd1`, × 1 664 525 + 1 013 904 223 |
//!
//! A consumer keeps its seeds, its seeding rule and any mapping of its own, and never changes
//! family: a recorded render depends on the exact stream. Every method is `#[inline]`, allocates
//! nothing and cannot panic, so a noise source may call it once per sample on the audio thread.

#![forbid(unsafe_code)]

pub mod map;

/// The golden-ratio constant of a 32-bit word, `⌊2³² / φ⌋`: what a zero xorshift seed becomes.
const GOLDEN_32: u32 = 0x9E37_79B9;
/// The golden-ratio constant of a 64-bit word, `⌊2⁶⁴ / φ⌋`: a zero xorshift seed's replacement, and
/// SplitMix64's increment.
const GOLDEN_64: u64 = 0x9E37_79B9_7F4A_7C15;

/// Marsaglia's 32-bit xorshift, shifts (13, 17, 5). Period 2³² − 1 over the nonzero states; zero is
/// a fixed point, which [`Xorshift32::new`] never starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xorshift32 {
    state: u32,
}

impl Xorshift32 {
    /// A generator from `seed`, a zero seed replaced by the golden-ratio constant.
    #[inline]
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self::from_state(if seed == 0 { GOLDEN_32 } else { seed })
    }

    /// A generator in exactly this state, zero included: for a consumer with a seeding rule of its
    /// own. A zero state draws zero for ever.
    #[inline]
    #[must_use]
    pub const fn from_state(state: u32) -> Self {
        Self { state }
    }

    /// The current state: the last word drawn, or the seed before the first draw.
    #[inline]
    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// The recurrence alone: the state after `state`. Linear over GF(2), which is what lets a jump
    /// table be built from it.
    #[inline]
    #[must_use]
    pub const fn step(mut state: u32) -> u32 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    }

    /// The next word: the new state.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        self.state = Self::step(self.state);
        self.state
    }

    /// The next draw in `[0, 1)`, from the top 24 bits: [`map::unit_f32`].
    #[inline]
    pub fn unit_f32(&mut self) -> f32 {
        map::unit_f32(self.next_u32())
    }

    /// The next draw in `[-1, 1)`, from the top 24 bits: [`map::bipolar_f32`].
    #[inline]
    pub fn bipolar_f32(&mut self) -> f32 {
        map::bipolar_f32(self.next_u32())
    }
}

/// The Numerical Recipes "quick and dirty" linear congruential generator, `ranqd1`: period 2³², and
/// low bits that cycle fast — which is why every draw takes the top bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lcg32 {
    state: u32,
}

impl Lcg32 {
    /// A generator from `seed`. Every state is valid, so this is [`Lcg32::from_state`].
    #[inline]
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self::from_state(seed)
    }

    /// A generator in exactly this state.
    #[inline]
    #[must_use]
    pub const fn from_state(state: u32) -> Self {
        Self { state }
    }

    /// The current state: the last word drawn, or the seed before the first draw.
    #[inline]
    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// The recurrence alone: `state × 1 664 525 + 1 013 904 223`, wrapping.
    #[inline]
    #[must_use]
    pub const fn step(state: u32) -> u32 {
        state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
    }

    /// The next word: the new state.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        self.state = Self::step(self.state);
        self.state
    }

    /// The next draw in `[0, 1)`, from the top 24 bits: [`map::unit_f32`].
    #[inline]
    pub fn unit_f32(&mut self) -> f32 {
        map::unit_f32(self.next_u32())
    }

    /// The next draw in `[-1, 1)`, from the top 24 bits: [`map::bipolar_f32`].
    #[inline]
    pub fn bipolar_f32(&mut self) -> f32 {
        map::bipolar_f32(self.next_u32())
    }
}

/// The draws every 64-bit generator offers over its `next_u64`.
macro_rules! draws_64 {
    () => {
        /// The next word's high half, the better half of every generator here.
        #[inline]
        pub fn next_u32(&mut self) -> u32 {
            (self.next_u64() >> 32) as u32
        }

        /// The next draw in `[0, 1)` as `f32`, from the top 24 bits: [`map::unit_f32`].
        #[inline]
        pub fn unit_f32(&mut self) -> f32 {
            map::unit_f32(self.next_u32())
        }

        /// The next draw in `[-1, 1)` as `f32`, from the top 24 bits: [`map::bipolar_f32`].
        #[inline]
        pub fn bipolar_f32(&mut self) -> f32 {
            map::bipolar_f32(self.next_u32())
        }

        /// The next draw in `[0, 1)` as `f64`, from the top 53 bits: [`map::unit_f64`].
        #[inline]
        pub fn unit_f64(&mut self) -> f64 {
            map::unit_f64(self.next_u64())
        }

        /// The next draw in `[-1, 1)` as `f64`, from the top 53 bits: [`map::bipolar_f64`].
        #[inline]
        pub fn bipolar_f64(&mut self) -> f64 {
            map::bipolar_f64(self.next_u64())
        }
    };
}

/// Marsaglia's 64-bit xorshift, shifts (13, 7, 17). Period 2⁶⁴ − 1 over the nonzero states; zero
/// is a fixed point, which [`Xorshift64::new`] never starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// A generator from `seed`, a zero seed replaced by the golden-ratio constant.
    #[inline]
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self::from_state(if seed == 0 { GOLDEN_64 } else { seed })
    }

    /// A generator in exactly this state, zero included: for a consumer with a seeding rule of its
    /// own. A zero state draws zero for ever.
    #[inline]
    #[must_use]
    pub const fn from_state(state: u64) -> Self {
        Self { state }
    }

    /// The current state: the last word drawn, or the seed before the first draw.
    #[inline]
    #[must_use]
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// The recurrence alone: the state after `state`. Linear over GF(2).
    #[inline]
    #[must_use]
    pub const fn step(mut state: u64) -> u64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    }

    /// The next word: the new state.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = Self::step(self.state);
        self.state
    }

    draws_64!();
}

/// Vigna's xorshift64\*: a 64-bit xorshift with shifts (12, 25, 27) whose output is the state times
/// `0x2545_F491_4F6C_DD1D`, which scrambles the linear low bits. Zero is a fixed point, which
/// [`Xorshift64Star::new`] never starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xorshift64Star {
    state: u64,
}

impl Xorshift64Star {
    /// The output multiplier, Vigna's `M32`.
    const MULTIPLIER: u64 = 0x2545_F491_4F6C_DD1D;

    /// A generator from `seed`, a zero seed replaced by the golden-ratio constant.
    #[inline]
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self::from_state(if seed == 0 { GOLDEN_64 } else { seed })
    }

    /// A generator in exactly this state, zero included: for a consumer with a seeding rule of its
    /// own. A zero state draws zero for ever.
    #[inline]
    #[must_use]
    pub const fn from_state(state: u64) -> Self {
        Self { state }
    }

    /// The current state, before the output multiply.
    #[inline]
    #[must_use]
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// The recurrence alone, without the output multiply. Linear over GF(2).
    #[inline]
    #[must_use]
    pub const fn step(mut state: u64) -> u64 {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state
    }

    /// The next word: the new state times the multiplier.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = Self::step(self.state);
        self.state.wrapping_mul(Self::MULTIPLIER)
    }

    draws_64!();
}

/// SplitMix64: a Weyl sequence (the state advances by the golden-ratio constant) through a
/// bijective output function. Every state is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator from `seed`. Every state is valid, so this is [`SplitMix64::from_state`].
    #[inline]
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self::from_state(seed)
    }

    /// A generator in exactly this state.
    #[inline]
    #[must_use]
    pub const fn from_state(state: u64) -> Self {
        Self { state }
    }

    /// The current state: the Weyl sequence's position, before the output function.
    #[inline]
    #[must_use]
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// The recurrence alone: the state plus the golden-ratio constant.
    #[inline]
    #[must_use]
    pub const fn step(state: u64) -> u64 {
        state.wrapping_add(GOLDEN_64)
    }

    /// The output function, Stafford's *Mix13*: a bijection that spreads every input bit over
    /// every output bit. Stateless, so it doubles as a hash of an index.
    #[inline]
    #[must_use]
    pub const fn mix(mut z: u64) -> u64 {
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The next word: the advanced state through [`SplitMix64::mix`].
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = Self::step(self.state);
        Self::mix(self.state)
    }

    draws_64!();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A zero seed never sticks**: `new` replaces it, `from_state` keeps it and draws zero for
    /// ever, which is what a consumer's own seeding rule is there to prevent.
    #[test]
    fn a_zero_seed_is_replaced_by_new_and_kept_by_from_state() {
        assert_eq!(Xorshift32::new(0).state(), GOLDEN_32);
        assert_eq!(Xorshift64::new(0).state(), GOLDEN_64);
        assert_eq!(Xorshift64Star::new(0).state(), GOLDEN_64);
        assert_eq!(Xorshift32::new(7).state(), 7, "a nonzero seed is the state");
        let mut stuck = Xorshift32::from_state(0);
        assert_eq!([stuck.next_u32(), stuck.next_u32()], [0, 0]);
        let mut stuck = Xorshift64::from_state(0);
        assert_eq!(stuck.next_u64(), 0);
        let mut stuck = Xorshift64Star::from_state(0);
        assert_eq!(stuck.next_u64(), 0);
    }

    /// **`step` is the recurrence the draws run**, so a jump table built from it lands where
    /// drawing would have.
    #[test]
    fn step_is_the_recurrence_the_draws_run() {
        let mut a = Xorshift32::new(0x1234_5678);
        let mut b = Xorshift64::new(0x1234_5678);
        let mut c = Xorshift64Star::new(0x1234_5678);
        let mut d = SplitMix64::new(0x1234_5678);
        let mut e = Lcg32::new(0x1234_5678);
        for _ in 0..64 {
            let (sa, sb, sc, sd, se) = (a.state(), b.state(), c.state(), d.state(), e.state());
            assert_eq!(a.next_u32(), Xorshift32::step(sa));
            assert_eq!(b.next_u64(), Xorshift64::step(sb));
            assert_eq!(
                c.next_u64(),
                Xorshift64Star::step(sc).wrapping_mul(0x2545_F491_4F6C_DD1D)
            );
            assert_eq!(d.next_u64(), SplitMix64::mix(SplitMix64::step(sd)));
            assert_eq!(e.next_u32(), Lcg32::step(se));
        }
    }

    /// **A 64-bit generator's 32-bit draw is the high half** of the word it would have drawn.
    #[test]
    fn a_64_bit_generators_next_u32_is_the_high_half() {
        let (mut a, mut b) = (Xorshift64Star::new(9), Xorshift64Star::new(9));
        for _ in 0..16 {
            assert_eq!(u64::from(a.next_u32()), b.next_u64() >> 32);
        }
    }

    /// **The exact laws stay inside their half-open ranges** at both ends of the word, and the two
    /// inexact laws reach 1.0, which is why they are not the default.
    #[test]
    fn the_laws_hold_their_ranges_at_both_ends() {
        assert_eq!(map::unit_f32(0), 0.0);
        assert!(map::unit_f32(u32::MAX) < 1.0);
        assert_eq!(map::bipolar_f32(0), -1.0);
        assert!(map::bipolar_f32(u32::MAX) < 1.0);
        assert_eq!(map::unit_f64(0), 0.0);
        assert!(map::unit_f64(u64::MAX) < 1.0);
        assert_eq!(map::bipolar_f64(0), -1.0);
        assert!(map::bipolar_f64(u64::MAX) < 1.0);
        assert_eq!(map::unit_f32_closed(u32::MAX), 1.0);
        assert_eq!(map::unit_f32_word(u32::MAX), 1.0);
        assert_eq!(map::unit_f32_word(0), 0.0);
    }

    /// **Every value of an exact law is distinct**: the 24 bits a step changes move the draw.
    #[test]
    fn neighbouring_top_bits_give_distinct_draws() {
        assert!(map::unit_f32(1 << 8) > map::unit_f32(0));
        assert!(map::bipolar_f32(u32::MAX) > map::bipolar_f32(u32::MAX - (1 << 8)));
        assert!(map::unit_f64(1 << 11) > map::unit_f64(0));
    }
}
