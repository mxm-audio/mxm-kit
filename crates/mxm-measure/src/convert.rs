//! Unit conversions, one definition each.
//!
//! These existed as roughly 73 open-coded `20.0 * x.log10()` sites with three different zero guards
//! and sometimes none, about 22 hand-written note-to-hertz conversions and about 25 ratio-to-cents
//! conversions. The arithmetic was never the interesting part; having two spellings of it in one
//! repository was the problem, because a reader of a figure could not tell which they had.
//!
//! **Zero converts to −∞, and the floor belongs to the caller.** An audibility floor is a judgement
//! about what counts as silence and it differs between a filter test and a reverb tail, so it is not
//! made here. A caller that wants a printable number clamps; a caller asserting exact silence wants
//! the −∞ and must not be handed a −120 dB that looks like a measurement.

/// Amplitude ratio to decibels: `20·log₁₀(ratio)`.
///
/// **For a ratio of amplitudes.** A ratio of *squared* quantities — energies, power spectra, mean
/// squares — is `10·log₁₀`, and this repository used to mix the two so a reader could not tell which
/// a figure had. There is no `power_db` here because nothing needs one yet; write the factor of ten
/// at the call site, where the reader can see which quantity it is.
///
/// Zero returns −∞ and a negative ratio returns NaN's honest alternative — it is a domain error, so
/// the magnitude is taken first and the caller that passed a signed value gets the magnitude's
/// answer rather than a silent NaN.
#[inline]
pub fn amplitude_db(ratio: f64) -> f64 {
    20.0 * ratio.abs().log10()
}

/// A frequency ratio in cents: `1200·log₂(ratio)`.
///
/// Zero returns −∞ by the same rule as [`amplitude_db`]; a ratio is expected positive.
#[inline]
pub fn cents(ratio: f64) -> f64 {
    1200.0 * ratio.abs().log2()
}

/// How far `measured` is from `wanted`, in cents. Positive means sharp.
///
/// This is the form every oscillator tuning test actually wants, and writing it once removes the
/// question of which way round the ratio went.
#[inline]
pub fn cents_error(measured: f64, wanted: f64) -> f64 {
    cents(measured / wanted)
}

/// MIDI note number to hertz, twelve-tone equal temperament, A440 at note 69.
///
/// Fractional notes are meaningful and used: a pitch detector reports 33.4 rather than 33.
#[inline]
pub fn note_hz(note: f64) -> f64 {
    440.0 * ((note - 69.0) / 12.0).exp2()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels_are_exact_at_the_decade_and_the_octave() {
        // Closed form: a tenth of the amplitude is exactly -20 dB, a tenth of the power exactly -10.
        assert!((amplitude_db(0.1) + 20.0).abs() < 1e-12);
        assert!((amplitude_db(1.0)).abs() < 1e-12);
        // Half the amplitude is the textbook -6.0206 dB.
        assert!((amplitude_db(0.5) + 6.020_599_913_279_624).abs() < 1e-12);
    }

    #[test]
    fn zero_is_negative_infinity_and_not_a_floor_somebody_chose() {
        // The contract the ~73 open-coded conversions disagreed about. A caller wanting a printable
        // number clamps at the call site; this function does not decide what silence is worth.
        assert_eq!(amplitude_db(0.0), f64::NEG_INFINITY);
        assert_eq!(cents(0.0), f64::NEG_INFINITY);
        assert!(!amplitude_db(0.0).is_nan(), "silence must not be NaN");
    }

    #[test]
    fn an_octave_is_twelve_hundred_cents_and_a_semitone_is_a_hundred() {
        assert!((cents(2.0) - 1200.0).abs() < 1e-9);
        assert!((cents(2f64.powf(1.0 / 12.0)) - 100.0).abs() < 1e-9);
        // Sharp reads positive, which is the convention every tuning test in the repository assumes.
        assert!(cents_error(441.0, 440.0) > 0.0);
    }

    #[test]
    fn notes_land_on_the_frequencies_they_name() {
        // The anchors, by definition, and an octave either side of each.
        assert!((note_hz(69.0) - 440.0).abs() < 1e-12);
        assert!((note_hz(57.0) - 220.0).abs() < 1e-12);
        assert!((note_hz(81.0) - 880.0).abs() < 1e-12);
        // A semitone is a hundred cents, which ties this to `cents` rather than leaving both loose.
        assert!((cents_error(note_hz(61.0), note_hz(60.0)) - 100.0).abs() < 1e-9);
        // Fractional notes are meaningful: a detector reports 68.5, not 68 or 69.
        assert!(note_hz(68.5) > note_hz(68.0) && note_hz(68.5) < note_hz(69.0));
    }
}
