//! What frequency a signal is.
//!
//! # The bug this module exists to delete
//!
//! Two crates estimated oscillator frequency by **counting** positive-going zero crossings over a
//! fixed window, and both asserted tuning accurate to one cent. A third crate did not, and said why
//! in a comment:
//!
//! > counting whole crossings over a fixed window quantises to ±1 cycle, which at 55 Hz over two
//! > seconds is ±9 cents, too coarse to tell a tuning error from rounding.
//!
//! So that crate interpolated the crossings and measured between the first and the last. **The
//! correction was made once and stayed where it was made**, and the two crates with the coarse ruler
//! kept passing an assertion they were not measuring. That is the whole argument for this crate in
//! one example: a duplicated helper is tedious, and a duplicated helper one author improved is a test
//! suite reporting green about a property it never checked.
//!
//! [`frequency_by_crossings`] is the corrected method, once.

/// Frequency from the time between the **first and last** rising zero crossings, with the crossings
/// interpolated to sub-sample resolution.
///
/// Accurate to far better than a cent over a couple of seconds at any musical pitch, because the
/// measurement is a duration between two interpolated instants rather than a count of whole cycles:
/// the ±1 cycle quantisation that makes counting useless at low pitch does not arise.
///
/// **Domain: one steady periodic signal that crosses zero once per cycle.** A waveform with more
/// than one rising crossing per cycle — a pulse train with ringing, a signal with a strong upper
/// partial crossing zero — reads high by the number of extra crossings. Absent for silence, for a
/// signal with fewer than two rising crossings, or where the two crossings coincide.
pub fn frequency_by_crossings(x: &[f32], rate: f64) -> Option<f64> {
    if rate <= 0.0 || !rate.is_finite() {
        return None;
    }
    // Comparisons against NaN are false, so a non-finite run would simply produce no crossings and
    // the frequency would be measured over whatever finite stretches remain — a plausible number
    // from a broken render. Absence is the honest answer.
    if crate::observe::first_nonfinite(x).is_some() {
        return None;
    }
    let (mut first, mut last, mut count) = (None::<f64>, 0.0f64, 0usize);
    for i in 1..x.len() {
        let (previous, current) = (f64::from(x[i - 1]), f64::from(x[i]));
        if previous <= 0.0 && current > 0.0 {
            // Linear interpolation to where the line between the two samples crosses zero.
            let fraction = -previous / (current - previous);
            let at = (i as f64 - 1.0 + fraction) / rate;
            match first {
                None => first = Some(at),
                Some(_) => {
                    last = at;
                    count += 1;
                }
            }
        }
    }
    let first = first?;
    let span = last - first;
    (count > 0 && span > 0.0).then(|| count as f64 / span)
}

// **There is deliberately no autocorrelation estimator here, and it was written and then removed.**
//
// A draft of this module carried one, for the case `frequency_by_crossings` genuinely cannot handle:
// a waveform whose loudest partial is not its fundamental, which crosses zero more than once a cycle.
// It worked. It came out anyway, for two reasons that are both this crate's own rules:
//
// - **One consumer.** Nothing in the repository's tests measures the pitch of a complex signal this
//   way; they measure oscillators, which cross zero once a cycle. A shared function with one caller
//   is the pre-generalisation the root contract forbids (*Don't pre-generalise*, now in
//   `docs/collection-rules.md`), and the gate does not get relaxed for code that happens to be
//   written already.
// - **It would have been a second detector.** `mxm-creative-sampler-dsp` ships a YIN detector and
//   `dsp-lab/examples/root_spike.rs` exists to score it against rendered ground truth. A YIN in the
//   measurement crate is a ruler built out of the thing under test, which is the one shape this crate
//   may never take.
//
// It also failed its own closed-form test on the way out, in the textbook way — a pure 110 Hz sine
// read as 55, because autocorrelation scores a lag of two periods as highly as one — and the fix for
// that is a published decision parameter. Worth knowing if this is ever reconsidered: the second
// consumer that promotes it arrives with a real signal, and the octave decision comes with it.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RATES, STRESS_RATES, convert::cents_error, stimulus};

    #[test]
    fn the_corrected_ruler_resolves_far_better_than_a_cent_where_counting_cannot() {
        // The assertion the two defective crates believed they were making. 55 Hz over two seconds
        // is precisely the case the coarse method quantises to about +/-9 cents.
        for rate in RATES {
            for hz in [55.0f64, 110.0, 440.0, 1_000.0, 4_000.0] {
                let x = stimulus::sine((2.0 * rate) as usize, hz, rate, 0.8);
                let measured = frequency_by_crossings(&x, rate).expect("a tone has a frequency");
                let error = cents_error(measured, hz);
                assert!(
                    error.abs() < 0.05,
                    "{hz} Hz at {rate}: measured {measured:.4}, off by {error:.4} cents"
                );
            }
        }
    }

    #[test]
    fn counting_whole_cycles_would_have_failed_the_same_assertion() {
        // The defect, reproduced, so the fix cannot be quietly reverted: the old method's error at
        // this pitch over two seconds is an order of magnitude past the one-cent claim it was making.
        //
        // **55.3 Hz, not 55.** At exactly 55 Hz two seconds holds exactly 110 cycles, so counting
        // happens to be exact and the defect hides — which is worth knowing, because it is why the
        // coarse ruler survived in two crates whose test frequencies all divided evenly into their
        // window. A detuned oscillator is the case that matters and it does not divide evenly.
        let rate = 48_000.0;
        let hz = 55.3;
        let seconds = 2.0;
        let x = stimulus::sine((seconds * rate) as usize, hz, rate, 0.8);

        let counted = {
            let mut crossings = 0usize;
            let mut previous = 0.0f32;
            for &s in &x {
                if previous <= 0.0 && s > 0.0 {
                    crossings += 1;
                }
                previous = s;
            }
            crossings as f64 / seconds
        };
        let coarse = cents_error(counted, hz).abs();
        let fine = cents_error(frequency_by_crossings(&x, rate).expect("a frequency"), hz).abs();

        assert!(
            coarse > 1.0,
            "the coarse method measured {coarse:.2} cents of error, so this test no longer \
             demonstrates the defect it was written for"
        );
        assert!(
            fine < 0.05,
            "the corrected method is off by {fine:.4} cents"
        );
    }

    #[test]
    fn no_pitch_is_absent_rather_than_zero() {
        // Silence, a signal with no rising crossing, and a buffer too short to hold two periods.
        let rate = 48_000.0;
        assert_eq!(
            frequency_by_crossings(&stimulus::silence(4_800), rate),
            None
        );
        assert_eq!(
            frequency_by_crossings(&stimulus::dc(4_800, -0.5), rate),
            None
        );
        assert_eq!(frequency_by_crossings(&[0.0, 1.0], rate), None);
        // Degenerate arguments, rather than a panic or a division by zero.
        assert_eq!(
            frequency_by_crossings(&stimulus::sine(1024, 440.0, rate, 0.5), 0.0),
            None
        );
    }

    #[test]
    fn a_tone_at_a_note_reads_as_that_note() {
        let rate = 48_000.0;
        let x = stimulus::sine(96_000, crate::convert::note_hz(69.0), rate, 0.8);
        let hz = frequency_by_crossings(&x, rate).expect("a frequency");
        assert!(
            cents_error(hz, 440.0).abs() < 1.0,
            "A440 read as {hz:.4} Hz"
        );
    }

    #[test]
    fn never_nan_at_every_stress_rate() {
        for rate in STRESS_RATES {
            let x = stimulus::sine(4_096, 440.0, rate, 0.8);
            if let Some(hz) = frequency_by_crossings(&x, rate) {
                assert!(!hz.is_nan(), "crossings went NaN at {rate}");
            }
        }
    }
}
