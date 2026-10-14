//! Keeping recursive state out of the denormal range, and non-finite values out of everything.

/// Flush a recursive state toward zero before it can become denormal.
///
/// Denormal arithmetic can cost orders of magnitude more than normal arithmetic,
/// which in a feedback filter shows up as a CPU spike exactly when a note decays
/// into silence. We do this in the DSP rather than relying on a framework FTZ
/// guard: the guard may be a no-op unless an opt-in feature is enabled, and
/// flushing here is also what keeps digital silence *exactly* zero.
///
/// `1e-20` is far above the f32 denormal threshold (~1.18e-38) and about -400 dB,
/// so nothing audible is lost.
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// `x`, or `fallback` when `x` is NaN or infinite.
///
/// The guard every rate, control and sample sanitiser in the collection opens with: a sample-rate
/// guard is this followed by its own bound (`finite_or(rate, 48_000.0).max(minimum)`), a sample guard
/// this with a fallback of zero.
#[inline]
pub fn finite_or(x: f32, fallback: f32) -> f32 {
    if x.is_finite() { x } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_or_passes_finite_values_and_replaces_the_rest() {
        assert_eq!(finite_or(0.25, 1.0), 0.25);
        assert_eq!(finite_or(-0.0, 1.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(finite_or(f32::MAX, 1.0), f32::MAX);
        assert_eq!(finite_or(f32::NAN, 1.0), 1.0);
        assert_eq!(finite_or(f32::INFINITY, 0.0), 0.0);
        assert_eq!(finite_or(f32::NEG_INFINITY, 48_000.0), 48_000.0);
    }

    #[test]
    fn flush_preserves_audible_values_and_kills_tiny_ones() {
        assert_eq!(flush(0.0), 0.0);
        assert_eq!(flush(1e-30), 0.0);
        assert_eq!(flush(-1e-30), 0.0);
        assert_eq!(flush(0.5), 0.5);
        assert_eq!(flush(-1e-6), -1e-6);
    }
}
