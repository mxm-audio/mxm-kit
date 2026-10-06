//! Signals whose ideal properties are known analytically, before they are rendered.
//!
//! A measurement is only as good as what it is pointed at, and the most valuable line in an aliasing
//! test is the one that measures something known to be clean. These are the controls: an additive
//! sawtooth that has no aliasing by construction, so whatever the analysis reports for it is the
//! analysis's own floor, and a trivial one that establishes the other end of the range.
//!
//! **Nothing here reimplements shipped DSP.** [`additive_saw`] is defined by a closed form rather
//! than by an algorithm, and its whole role is to be the thing a shipped oscillator is *not*. A
//! filter or a real oscillator in this crate would eventually be used to check a filter or an
//! oscillator against itself.

use crate::spectrum::periods_for;
use std::f64::consts::TAU;

/// `n` samples of silence.
pub fn silence(n: usize) -> Vec<f32> {
    vec![0.0; n]
}

/// Direct current: `n` samples of `value`.
///
/// For DC-blocker and offset tests, where the interesting answer is what comes out rather than what
/// went in.
pub fn dc(n: usize, value: f32) -> Vec<f32> {
    vec![value; n]
}

/// A sine at exactly `hz`, whatever that does to the window's periodicity.
///
/// **Domain: a positive finite `rate`, and a finite `hz` and `amplitude`.** Anything else yields
/// silence rather than a buffer of NaN — a stimulus that poisons the measurement it was built for is
/// the worst possible failure mode here. Within that domain the phase is reduced rather than
/// accumulated, so there is no frequency at which this quietly starts returning zeros.
///
/// Use this when the frequency matters more than the analysis — a filter's response at a named
/// corner, say. When the *spectrum* is being measured, use [`periodic_sine`] instead, or the
/// measurement will include leakage that looks like the thing under test.
pub fn sine(n: usize, hz: f64, rate: f64, amplitude: f32) -> Vec<f32> {
    // A zero or non-finite rate makes every angle non-finite, and a generator that hands back NaN is
    // a generator that breaks the measurement it was made for. Silence is the one output that can be
    // nothing but an obvious failure at the call site.
    if !rate.is_finite() || rate <= 0.0 || !hz.is_finite() || !amplitude.is_finite() {
        return silence(n);
    }
    // **The phase advances, it is never computed from the sample index.** Two earlier drafts formed
    // `hz * i / rate` and guarded the result; both had the same failure, because `i` grows: a large
    // finite `hz` overflows partway through, so the buffer came back as a valid prefix followed by
    // silence that looked like a measurement. Advancing by a wrapped increment cannot overflow at any
    // `i`, and the accumulated error over a buffer is a few parts in 10^10 of a cycle.
    // `hz / rate` is itself an overflow for a huge `hz` and a tiny `rate`, and `rem_euclid` of an
    // infinity is NaN — so the guard has to be on the increment, not only on the inputs that formed
    // it. This is the third shape this overflow has taken; each earlier one guarded one step further
    // from where the number actually goes wrong.
    let increment = (hz / rate).rem_euclid(1.0);
    if !increment.is_finite() {
        return silence(n);
    }
    let mut phase = 0.0f64;
    (0..n)
        .map(|_| {
            let sample = (f64::from(amplitude) * (TAU * phase).sin()) as f32;
            phase = (phase + increment).rem_euclid(1.0);
            sample
        })
        .collect()
}

/// A sine at a frequency **near** `hz` that is **exactly periodic** in `n` samples.
///
/// This is the whole reason the collection's spectral measurements need no window function: with a
/// whole number of cycles in the window, the transform has no leakage and a single-bin probe reads
/// the component's true amplitude. The chosen frequency is [`periodic_frequency`]; ask for it when
/// the difference matters, because at short `n` it can sit well away from `hz`.
pub fn periodic_sine(n: usize, hz: f64, rate: f64, amplitude: f32) -> Vec<f32> {
    sine(n, periodic_frequency(n, hz, rate), rate, amplitude)
}

/// The frequency [`periodic_sine`] actually renders: one near `hz` that fits a whole — and odd —
/// number of cycles into `n` samples.
///
/// Odd, because with `n` a power of two it makes `gcd(periods, n) = 1`, so harmonics land on
/// distinct bins and aliases never collide with them.
///
/// **Near, not nearest**, and [`crate::spectrum::periods_for`] explains why that behaviour is kept:
/// the result can sit up to 1.5 bins above `hz`. Ask this function rather than assuming.
pub fn periodic_frequency(n: usize, hz: f64, rate: f64) -> f64 {
    if n == 0 {
        return hz;
    }
    periods_for(hz, rate, n) as f64 * rate / n as f64
}

/// A **band-limited sawtooth by additive synthesis** — the known-clean control.
///
/// Every harmonic that fits below Nyquist at amplitude `1/k`, and not one above it, so the waveform
/// has no aliasing by construction. Whatever an aliasing measurement reports for this is the
/// measurement's own floor: if somebody later changes the window, the frequency selection or the bin
/// partitioning in a way that breaks the method, a control row built on this fails first and says
/// so, instead of the real assertion failing mysteriously or passing for the wrong reason.
///
/// `periods` whole cycles in `n` samples, so the result is exactly periodic in the analysis window.
pub fn additive_saw(n: usize, periods: usize) -> Vec<f32> {
    let mut x = vec![0.0f32; n];
    if n == 0 || periods == 0 {
        return x;
    }
    // Nyquist is bin n/2, so harmonic k of a signal with `periods` cycles per window lands at
    // k*periods and must stay strictly below it.
    let highest = (n / 2).saturating_sub(1) / periods;
    for (i, s) in x.iter_mut().enumerate() {
        let phase = TAU * periods as f64 * i as f64 / n as f64;
        let mut sum = 0.0f64;
        for k in 1..=highest {
            sum += (k as f64 * phase).sin() / k as f64;
        }
        // 2/pi normalises the series to a unit-amplitude ramp.
        *s = (sum * 2.0 / std::f64::consts::PI) as f32;
    }
    x
}

/// A **trivially sampled sawtooth** — the known-dirty control.
///
/// The naive ramp, aliasing and all. It establishes the other end of an aliasing measurement's
/// range, and it is what catches the failure mode that matters most: band limiting silently not
/// happening. An absolute threshold can be passed by an oscillator that is quietly generating this;
/// a comparison against it cannot.
pub fn trivial_saw(n: usize, periods: usize) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|i| {
            let phase = (periods as f64 * i as f64 / n as f64).fract();
            (2.0 * phase - 1.0) as f32
        })
        .collect()
}

/// Deterministic white noise from an explicitly seeded generator.
///
/// **Seeded, never from the clock.** The collection's randomness is bit-repeatable by contract, and
/// a measurement excited by an unseeded source is not reproducible from a clone. The generator is an
/// xorshift64\*, which is adequate for excitation and is not claimed to be adequate for anything
/// else.
pub fn noise(n: usize, seed: u64, amplitude: f32) -> Vec<f32> {
    let mut state = if seed == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        seed
    };
    (0..n)
        .map(|_| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let value = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
            // The top 24 bits into -1..1: enough entropy for excitation, and exactly reproducible.
            let unit = ((value >> 40) as f64 / (1u64 << 24) as f64) * 2.0 - 1.0;
            (unit * f64::from(amplitude)) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RATES, level, spectrum};

    #[test]
    fn a_periodic_sine_holds_a_whole_number_of_cycles() {
        // The property the whole no-window method rests on: the last sample joins the first, so the
        // buffer repeats seamlessly and the transform sees no discontinuity.
        for rate in RATES {
            let n = 4096;
            let hz = periodic_frequency(n, 440.0, rate);
            let cycles = hz * n as f64 / rate;
            assert!(
                (cycles - cycles.round()).abs() < 1e-9,
                "at {rate} Hz: {cycles} cycles is not whole"
            );
            assert_eq!(
                cycles.round() as usize % 2,
                1,
                "the period count must be odd so harmonics and aliases cannot collide"
            );
            // And it is near what was asked for, or the control is measuring the wrong note.
            // Within **two** bins, not one: rounding to nearest lands inside half a bin, and then
            // forcing the count odd can push it a whole bin further. At 48 kHz and n = 4096, 440 Hz
            // is 37.55 bins, rounds to 38, and becomes 39 — 457 Hz, a bin and a half away.
            let bin = rate / n as f64;
            assert!(
                (hz - 440.0).abs() < 2.0 * bin,
                "at {rate} Hz: asked for 440, got {hz:.2}, which is more than two {bin:.2} Hz bins"
            );
        }
    }

    #[test]
    fn the_additive_saw_is_clean_and_the_trivial_one_is_not() {
        // The two controls, against each other. This is the assertion that catches a broken analysis
        // as well as a broken oscillator: if the clean control ever measures dirty, the ruler is
        // wrong and no other row in the table means anything.
        let n = 1 << 14;
        let periods = 129;
        let clean = additive_saw(n, periods);
        let dirty = trivial_saw(n, periods);

        let clean_db =
            spectrum::alias_to_signal_db(&clean, periods).expect("a clean control reads");
        let dirty_db =
            spectrum::alias_to_signal_db(&dirty, periods).expect("a dirty control reads");

        assert!(
            clean_db < -100.0,
            "the analysis floor is only {clean_db:.1} dB — the ruler is not trustworthy"
        );
        assert!(
            dirty_db > -30.0,
            "the trivial saw measured {dirty_db:.1} dB, which is too clean to be the dirty control"
        );
        assert!(clean_db < dirty_db - 60.0);
    }

    #[test]
    fn the_additive_saw_is_a_unit_ramp() {
        let x = additive_saw(1 << 12, 1);
        let peak = level::peak(&x).expect("an additive saw is finite");
        // Gibbs overshoot at the discontinuity puts the peak a little over one; the series is
        // normalised so the ramp itself spans -1..1.
        assert!(
            (0.95..1.25).contains(&peak),
            "an additive saw peaked at {peak}"
        );
    }

    #[test]
    fn noise_is_reproducible_from_its_seed_and_differs_between_seeds() {
        // The contract that makes an excited measurement reproducible from a clone.
        assert_eq!(noise(256, 12345, 0.5), noise(256, 12345, 0.5));
        assert_ne!(noise(256, 12345, 0.5), noise(256, 54321, 0.5));
        // Zero is remapped rather than producing a stuck generator.
        assert!(level::peak(&noise(256, 0, 0.5)).expect("finite noise") > 0.0);
        assert!(level::peak(&noise(4096, 7, 0.5)).expect("finite noise") <= 0.5);
    }

    #[test]
    fn the_simple_generators_are_what_they_say() {
        assert_eq!(silence(8), vec![0.0; 8]);
        assert_eq!(dc(3, -0.25), vec![-0.25; 3]);
    }

    #[test]
    fn no_finite_frequency_produces_a_nan_or_a_silent_tail() {
        // Three drafts overflowed here, each guarding one step further from where the number
        // actually goes wrong: the whole angle, then the cycle count, then the increment itself.
        // `f64::MAX / f64::MIN_POSITIVE` is an infinity, and `rem_euclid` of one is NaN.
        for (hz, rate) in [
            (f64::MAX, f64::MIN_POSITIVE),
            (f64::MAX, 1e-300),
            (1e300, 1e-300),
            (1e30, 48_000.0),
            (440.0, 48_000.0),
        ] {
            let x = sine(4096, hz, rate, 0.5);
            assert!(
                x.iter().all(|s| s.is_finite()),
                "hz {hz:e} at rate {rate:e} produced a non-finite sample"
            );
        }
        // And an ordinary tone is unaffected by any of those guards.
        let ordinary = sine(4096, 440.0, 48_000.0, 0.5);
        assert!(level::peak(&ordinary).expect("finite") > 0.49);
    }

    #[test]
    fn zero_length_requests_are_empty_rather_than_a_panic() {
        assert!(additive_saw(0, 4).is_empty());
        assert!(trivial_saw(0, 4).is_empty());
        assert!(sine(0, 440.0, 48_000.0, 1.0).is_empty());
        assert!(noise(0, 1, 1.0).is_empty());
        // A zero period count has no waveform, and returning silence beats dividing by it.
        assert!(additive_saw(16, 0).iter().all(|&s| s == 0.0));
    }
}
