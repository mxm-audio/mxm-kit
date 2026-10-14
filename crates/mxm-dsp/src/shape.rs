//! The saturators and the prewarp the instruments' filters share.

/// `tan(x)` via the [5/4] Padé approximant, for `x` in `[0, PI * 0.45]`.
///
/// `f32::tan` is a libm call and this runs per sample. The approximant is accurate
/// to better than 1e-4 relative across the whole valid range and has its pole at
/// `PI/2`, exactly where `tan` does. Evaluated in `f64` because numerator and
/// denominator both lose significance near the top of the range.
#[inline]
pub fn tan_approx(x: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    x * (945.0 - 105.0 * x2 + x4) / (945.0 - 420.0 * x2 + 15.0 * x4)
}

/// `tanh(x)` via the [7/6] Padé approximant, with the input clamped.
///
/// Accurate to better than 1e-4 absolute over the clamped range, which matters
/// more than it might look: this saturator *is* a filter's drive character and
/// it sets the amplitude at which self-oscillation settles. A cheaper [3/2] form
/// was tried first and deviates by up to 2.4% around `x = 1.5` — audible as a
/// different saturation colour, for the sake of a few multiplies.
///
/// The input clamp is load-bearing, not defensive: without it the rational form
/// diverges for large `x`, which would break the boundedness argument a filter
/// design rests on. `tanh(4) = 0.9993`, so the curve is essentially flat where the
/// clamp takes over and the discontinuity in slope is negligible.
///
/// The output clamp guards the last ulp: the invariant `|tanh_approx(x)| <= 1` is
/// what bounds the filter, so it should be exactly true in `f32`, not nearly true.
#[inline]
pub fn tanh_approx(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    let num = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let den = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    (num / den).clamp(-1.0, 1.0)
}

/// Back-to-back diodes to ground, as a curve: `f(x) = x / (1 + |x/knee|³)^(1/3)`,
/// from `research:filters/machines/ir3109-roland.md` §7.
///
/// Returns `(f(x), f'(x))`. With `d = (1 + |x/knee|³)^(1/3)` the value is `x/d` and
/// the derivative is `1/d⁴`, so one cube root serves both — which matters because
/// a ladder's Newton solve wants the derivative every step.
///
/// Properties a filter's boundedness argument rests on, each pinned by a test:
/// `|f(x)| <= knee` for every finite `x` (the output clamp makes that exactly true
/// in `f32`, not nearly true); `f` is strictly increasing, so the solve has one
/// root; `f` is odd, so it adds no even harmonics and no DC; and `f'(0) = 1`, so
/// the small-signal loop is the ideal ladder and the oscillation threshold is
/// unchanged from the linear theory.
///
/// `knee` must be positive and finite, as every caller's constant is; the expression is the copies'
/// own and checks nothing.
#[inline]
pub fn diode_clamp(x: f32, knee: f32) -> (f32, f32) {
    let a = x / knee;
    let a3 = (a * a * a).abs();
    // Past about 1e12 knees the cube overflows and `x / inf` would return 0 —
    // the one input for which the plain formula is *not* bounded by the knee.
    // The solve never gets there; the bound is promised for every finite input.
    if !a3.is_finite() {
        return (knee.copysign(x), 0.0);
    }
    let inv = 1.0 / (1.0 + a3).cbrt();
    let inv2 = inv * inv;
    ((x * inv).clamp(-knee, knee), inv2 * inv2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tan_approx_matches_libm_across_the_valid_range() {
        let max_x = std::f64::consts::PI * 0.45;
        let mut worst = 0.0f64;
        for i in 0..=10_000 {
            let x = max_x * i as f64 / 10_000.0;
            let (approx, exact) = (tan_approx(x), x.tan());
            if exact > 1e-9 {
                worst = worst.max(((approx - exact) / exact).abs());
            }
        }
        assert!(worst < 1e-4, "worst relative tan error {worst:e}");
    }

    #[test]
    fn tanh_approx_is_bounded_and_monotonic() {
        // The boundedness argument for every filter that uses it depends on this.
        let mut prev = f32::NEG_INFINITY;
        for i in -20_000..=20_000 {
            let x = i as f32 / 1000.0; // -20..20, well past the clamp
            let y = tanh_approx(x);
            assert!(y.abs() <= 1.0, "tanh_approx({x}) = {y} exceeds 1");
            assert!(y >= prev - 1e-6, "not monotonic at x={x}");
            prev = y;
        }
        assert!(tanh_approx(0.0).abs() < 1e-9);
        // Bounded by exactly 1, and essentially flat where the input clamp takes over.
        assert!(tanh_approx(f32::MAX) <= 1.0 && tanh_approx(f32::MAX) > 0.999);
        assert!(tanh_approx(f32::MIN) >= -1.0 && tanh_approx(f32::MIN) < -0.999);

        // Odd symmetry: a saturator that is not odd adds even harmonics and DC.
        for i in 0..=300 {
            let x = i as f32 / 100.0;
            assert!(
                (tanh_approx(x) + tanh_approx(-x)).abs() < 1e-6,
                "not odd at x={x}"
            );
        }

        // Close to real tanh across the whole clamped range. This curve is a
        // filter's drive character and sets the self-oscillation amplitude, so it
        // is held to a real tolerance rather than "close enough for a shaper".
        let mut worst = 0.0f32;
        for i in 0..=400 {
            let x = i as f32 / 100.0; // 0..4
            worst = worst.max((tanh_approx(x) - x.tanh()).abs());
        }
        assert!(worst < 1e-4, "worst deviation from tanh: {worst}");

        for i in 0..=20 {
            let x = i as f32 / 100.0; // small signal: 0..0.2
            assert!(
                (tanh_approx(x) - x.tanh()).abs() < 1e-6,
                "small-signal error at x={x}"
            );
        }
    }

    #[test]
    fn diode_clamp_is_bounded_monotonic_odd_and_its_derivative_is_exact() {
        // The boundedness argument for every ladder that uses it depends on the
        // first property; the Newton solve's single root depends on the second.
        // The knee every current user passes.
        let knee = 1.0f32;
        let mut prev = f32::NEG_INFINITY;
        for i in -40_000..=40_000 {
            let x = i as f32 / 1000.0; // -40..40, far past the knee
            let (y, dy) = diode_clamp(x, knee);
            assert!(
                y.abs() <= knee,
                "diode_clamp({x}) = {y} exceeds the knee {knee}"
            );
            // Strictly increasing where the solve can land (the deep-dive tests
            // +-4 knees); beyond that the curve is within an ulp of the bound and
            // "non-decreasing" is all f32 can promise.
            if x.abs() <= 4.0 * knee {
                assert!(y > prev, "not strictly increasing at x={x}");
            } else {
                // Rounding noise of `x * inv` at the flat top is an ulp or so
                // either way; the solve's single root rests on the analytic
                // derivative below, which is positive everywhere.
                assert!(y >= prev - 1e-6, "not monotonic at x={x}");
            }
            assert!(
                dy > 0.0 && dy <= 1.0,
                "derivative {dy} out of (0, 1] at x={x}"
            );
            prev = y;
        }
        assert_eq!(
            diode_clamp(f32::MAX, knee).0,
            knee,
            "bounded up to f32::MAX"
        );
        assert_eq!(diode_clamp(f32::MIN, knee).0, -knee);
        assert_eq!(diode_clamp(0.0, knee).0, 0.0);

        // Odd: a saturator that is not odd adds even harmonics and DC.
        for i in 0..=300 {
            let x = i as f32 / 100.0;
            let (p, n) = (diode_clamp(x, knee).0, diode_clamp(-x, knee).0);
            assert!((p + n).abs() < 1e-6, "not odd at x={x}");
        }

        // The closed-form derivative matches a central difference. Evaluated in
        // f64 so the difference quotient is not swamped by f32 rounding.
        let f = |x: f64| x / (1.0 + (x / knee as f64).abs().powi(3)).cbrt();
        let mut worst = 0.0f32;
        for i in 0..=400 {
            let x = i as f32 / 100.0; // 0..4
            let h = 1e-5f64;
            let numeric = ((f(x as f64 + h) - f(x as f64 - h)) / (2.0 * h)) as f32;
            worst = worst.max((diode_clamp(x, knee).1 - numeric).abs());
        }
        assert!(worst < 1e-4, "worst derivative error {worst}");

        // Unity slope at zero: the small-signal loop is the ideal ladder, which
        // is what keeps the measured threshold at k = 4.
        assert!((diode_clamp(0.0, knee).1 - 1.0).abs() < 1e-6);
        assert!((diode_clamp(1e-3, knee).0 - 1e-3).abs() < 1e-9);
    }
}
