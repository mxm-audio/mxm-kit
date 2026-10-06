//! How loud, how peaky, how far off zero.
//!
//! The most duplicated group in the repository: 11 named `peak` functions plus about 65 open-coded
//! `fold` chains, and 6 named `rms` functions. None of them disagreed — which is why these are
//! *equivalence* migrations, and why a call site that starts reporting a different number after
//! adopting one of these has found a defect rather than a rounding change.

/// The largest absolute value in a buffer, or **absence if any sample is not finite**.
///
/// Returns `f32` rather than `f64` deliberately: selecting the largest of a set of `f32`s loses
/// nothing, so widening the result would only suggest a precision the answer does not have. An
/// empty buffer has a peak of zero — there is nothing above zero in it — which is a measurement,
/// unlike the non-finite case.
///
/// **Why this is `Option` when a maximum is arithmetically well defined.** `f32::max` returns the
/// non-NaN operand, so the obvious implementation reports the largest *number* present and a render
/// that is half NaN measures as perfectly healthy. A draft of this crate kept that behaviour and
/// argued it was a selection rather than an accumulation, so no absence was owed. The argument is
/// true and beside the point: the eleven call sites this replaces all ask **"is it quiet?"**, and a
/// broken render answering *yes* is the worst failure this crate could ship. The contract has no
/// exceptions now, and every call site states what it assumes.
pub fn peak(x: &[f32]) -> Option<f32> {
    let mut peak = 0.0f32;
    for &s in x {
        if !s.is_finite() {
            return None;
        }
        peak = peak.max(s.abs());
    }
    Some(peak)
}

/// Root mean square, accumulated in `f64`.
///
/// An empty buffer has no mean square, so the result is absent rather than zero: zero RMS is a
/// measurement of silence, and *"there was nothing to measure"* is not.
pub fn rms(x: &[f32]) -> Option<f64> {
    if x.is_empty() {
        return None;
    }
    let sum: f64 = x.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    let rms = (sum / x.len() as f64).sqrt();
    // A non-finite sample makes the sum non-finite; reporting a number for it would launder a DSP
    // failure into a measurement. `observe::first_nonfinite` is how a caller asks what went wrong.
    rms.is_finite().then_some(rms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RATES, STRESS_RATES, stimulus};

    #[test]
    fn a_sines_rms_is_its_amplitude_over_root_two() {
        // The closed form this module is scored against. Exactly-periodic so the window holds whole
        // cycles and the answer is the analytic one rather than a fraction of a cycle's worth of bias.
        for rate in RATES {
            let x = stimulus::periodic_sine(4096, 440.0, rate, 0.5);
            let rms = rms(&x).expect("a non-empty buffer has an RMS");
            let want = 0.5 / 2f64.sqrt();
            assert!(
                (rms - want).abs() < 1e-4,
                "at {rate} Hz: RMS {rms:.6}, want {want:.6}"
            );
        }
    }

    #[test]
    fn peak_is_the_amplitude() {
        let x = stimulus::periodic_sine(4096, 440.0, 48_000.0, 0.5);
        let measured = peak(&x).expect("a rendered sine is finite");
        assert!((f64::from(measured) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn nothing_to_measure_is_absent_and_silence_is_zero() {
        // The distinction the result-form contract exists for: an empty buffer and a silent one are
        // different answers, and a test asserting "below the threshold" must not pass on both.
        assert_eq!(rms(&[]), None);

        let silence = stimulus::silence(1024);
        assert_eq!(rms(&silence), Some(0.0));
        assert_eq!(peak(&silence), Some(0.0));
        assert_eq!(
            peak(&[]),
            Some(0.0),
            "an empty buffer's largest value is zero, which is a measurement"
        );
    }

    #[test]
    fn a_non_finite_buffer_has_no_peak() {
        // The failure this `Option` exists for. A naive `fold` with `f32::max` reports 0.75 here,
        // because `f32::max` returns the non-NaN operand — so a render that is half NaN measures as
        // perfectly healthy and then passes every "is it quiet?" assertion in the repository.
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(peak(&[0.25, bad, -0.75, 0.5]), None, "peak laundered {bad}");
        }
        assert_eq!(peak(&[0.25, -0.75, 0.5]), Some(0.75));
    }

    #[test]
    fn a_non_finite_sample_is_absence_rather_than_a_laundered_number() {
        // The contract's sharpest case: a DSP that produced an infinity must not come back as a
        // measurement. Reporting absence sends the caller to `observe` to find out what happened.
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let x = [0.5f32, bad, -0.5];
            assert_eq!(rms(&x), None, "rms laundered {bad}");
            assert_eq!(peak(&x), None, "peak laundered {bad}");
        }
    }

    #[test]
    fn never_nan_at_every_stress_rate() {
        // The universal invariant. Accuracy is not claimed here — a kilohertz rate cannot carry a
        // 440 Hz sine faithfully — only that no function in this module returns NaN.
        for rate in STRESS_RATES {
            let x = stimulus::periodic_sine(512, 440.0, rate, 0.5);
            assert!(
                !peak(&x).expect("a rendered sine is finite").is_nan(),
                "peak went NaN at {rate}"
            );
            assert!(
                !rms(&x).expect("non-empty").is_nan(),
                "rms went NaN at {rate}"
            );
            // Adversarial, not only ordinary: the invariant is about every input, and a stress test
            // over well-behaved sines proves nothing about the case that actually breaks things.
            let hostile = [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                1e-45,
                -0.0,
                3.4e38,
            ];
            assert_eq!(
                peak(&hostile),
                None,
                "peak laundered a hostile buffer at {rate}"
            );
            assert_eq!(rms(&hostile), None);
            // The subnormal and the extremes on their own are finite, and measurable.
            let extreme = [1e-45f32, -0.0, 3.4e38];
            assert!(peak(&extreme).is_some_and(|p| !p.is_nan()));
        }
    }
}
