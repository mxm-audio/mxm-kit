//! A drawn word to a float: the laws every generator's draws use, over bits, for a stateless hash
//! or a word already taken.
//!
//! The four exact laws take the top bits a float can hold — 24 for `f32`, 53 for `f64` — so every
//! value is representable and none repeats. **New code takes these.**
//!
//! The two inexact laws ship in four crates each and are kept so those crates' recorded renders
//! stay bit-exact. Both reach 1.0, and [`unit_f32_word`] rounds.

/// The top 24 bits in `[0, 1)`: `(bits >> 8) / 2²⁴`, every value exact.
#[inline]
#[must_use]
pub fn unit_f32(bits: u32) -> f32 {
    (bits >> 8) as f32 / 16_777_216.0
}

/// The top 24 bits in `[-1, 1)`: `(bits >> 8) / 2²³ − 1`, every value exact.
#[inline]
#[must_use]
pub fn bipolar_f32(bits: u32) -> f32 {
    (bits >> 8) as f32 / 8_388_608.0 - 1.0
}

/// The top 53 bits in `[0, 1)`: `(bits >> 11) / 2⁵³`, every value exact.
#[inline]
#[must_use]
pub fn unit_f64(bits: u64) -> f64 {
    (bits >> 11) as f64 / 9_007_199_254_740_992.0
}

/// The top 53 bits in `[-1, 1)`: `(bits >> 11) / 2⁵² − 1`, every value exact.
#[inline]
#[must_use]
pub fn bipolar_f64(bits: u64) -> f64 {
    (bits >> 11) as f64 / 4_503_599_627_370_496.0 - 1.0
}

/// **Inexact, kept for bit-exactness**: the top 24 bits over `2²⁴ − 1`, in `[0, 1]`. The steps are
/// not powers of two, and the top value is exactly 1.0. The drum machine's and model drums' per-hit
/// and sample-and-hold draws, and the delay's random modulation, were recorded with it.
#[inline]
#[must_use]
pub fn unit_f32_closed(bits: u32) -> f32 {
    (bits >> 8) as f32 / 16_777_215.0
}

/// **Inexact, kept for bit-exactness**: the whole word over `u32::MAX`, in f32, in `[0, 1]`. The
/// word rounds to 24 bits on the way in and `u32::MAX` rounds to 2³², so a word near the top reads
/// exactly 1.0. The drum machine's 55 and metal noise, the convolution's starter room and the
/// shimmer's grain jitter were recorded with it.
#[inline]
#[must_use]
pub fn unit_f32_word(bits: u32) -> f32 {
    bits as f32 / u32::MAX as f32
}
