//! Numeric observations — facts about a buffer, never judgements about it.
//!
//! This is the module most likely to breach the crate's own line, so the line is drawn here
//! explicitly. An earlier draft called it *numeric contracts* and listed *"inside a stated bound"*
//! among its jobs, which is exactly the `assert_within_bound` this crate forbids: a bound is the
//! instrument's own `pub const`, with the argument that establishes it, and a shared helper taking
//! that bound as an argument is a threshold wearing a parameter's clothing.
//!
//! **So these report and do not decide.** `OUTPUT_BOUND` stays where it is and the comparison
//! against it stays in the test that cares; what moves here is the scan over the buffer, which had
//! been written about 226 times.
//!
//! NaN is the one exception, and it is not a threshold: *never NaN* is an invariant of arithmetic
//! rather than a view about what is acceptable, so a fact about a number is not a judgement.
//!
//! **This module is deliberately two functions.** A first draft had six — exceedance either way,
//! exact silence, a NaN finder — and every one of them had *zero* callers: the repository's own
//! silence and NaN checks assert per sample, inside the render loop, where they report the exact
//! sample immediately and a buffer-level scan would be a worse test rather than a shared one. They
//! were written, found to have no consumer, and removed. The gate does not relax for code that is
//! already written.

/// The index and value of the first sample that is not finite — a NaN or an infinity.
///
/// Crate-private: it is how the measurements here decide to report absence, and no test outside
/// this crate asks the question directly. The gate does not except a function for being useful.
///
/// Reports *what* was found as well as where, because the two failures have different causes: a NaN
/// usually comes from `0/0` or `inf - inf` in a recursive path, and an infinity from unbounded
/// growth.
pub(crate) fn first_nonfinite(x: &[f32]) -> Option<(usize, f32)> {
    x.iter()
        .enumerate()
        .find(|(_, s)| !s.is_finite())
        .map(|(i, &s)| (i, s))
}

/// The largest step between neighbouring samples, and where it starts.
///
/// **A discontinuity is what a click is**, so this is how a transition claim is judged rather than
/// by ear. Two crates had this under two names — `worst_jump` and `worst_step` — computing the same
/// thing.
///
/// Absent for a buffer with fewer than two samples, which has no steps rather than a zero-sized one,
/// and absent for a buffer holding a non-finite sample.
pub fn worst_step(x: &[f32]) -> Option<(usize, f64)> {
    // A non-finite sample makes every step around it non-finite, and "the worst step was NaN" is not
    // a click measurement. `first_nonfinite` is how a caller asks what actually happened.
    if first_nonfinite(x).is_some() {
        return None;
    }
    let mut worst: Option<(usize, f64)> = None;
    for (i, w) in x.windows(2).enumerate() {
        // Widened before subtracting: `f32::MAX - (-f32::MAX)` is an infinity from a buffer that was
        // entirely finite, which would be this crate producing the non-finite value it exists to
        // report. The crate's `f32` in, `f64` out rule covers exactly this.
        let step = (f64::from(w[1]) - f64::from(w[0])).abs();
        if worst.is_none_or(|(_, b)| step > b) {
            worst = Some((i, step));
        }
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stimulus;

    #[test]
    fn infinities_and_nans_are_told_apart() {
        assert_eq!(first_nonfinite(&[0.0, 1.0, 0.5]), None);
        let (at, value) = first_nonfinite(&[0.0, f32::INFINITY, 0.5]).expect("an infinity");
        assert_eq!(at, 1);
        assert!(value.is_infinite());
        // A NaN is non-finite too, and reported as itself rather than as an infinity.
        let (_, value) = first_nonfinite(&[f32::NAN]).expect("a NaN");
        assert!(value.is_nan());
    }

    #[test]
    fn the_worst_step_is_the_discontinuity() {
        // A closed form: a single jump of known size in an otherwise flat signal.
        let mut x = vec![0.25f32; 32];
        for s in x.iter_mut().skip(16) {
            *s = -0.35;
        }
        let (at, step) = worst_step(&x).expect("a step");
        assert_eq!(at, 15, "the step is between 15 and 16");
        assert!((step - 0.6).abs() < 1e-6, "the step measured {step}");

        // And a smooth signal's worst step is small — a sine's is its per-sample slope.
        let smooth = stimulus::periodic_sine(4096, 100.0, 48_000.0, 1.0);
        let (_, gentle) = worst_step(&smooth).expect("a step");
        assert!(gentle < 0.05, "a 100 Hz sine stepped {gentle}");
    }

    #[test]
    fn a_buffer_too_short_to_have_steps_says_so() {
        assert_eq!(worst_step(&[]), None);
        assert_eq!(worst_step(&[0.5]), None);
        let (_, step) = worst_step(&[0.0, 1.0]).expect("one step");
        assert!((step - 1.0).abs() < 1e-9);
    }
}
