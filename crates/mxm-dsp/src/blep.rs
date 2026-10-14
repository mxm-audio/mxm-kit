//! The PolyBLEP residual and the pulse-width clamp it needs.

/// PolyBLEP residual for a step discontinuity.
///
/// `t` is the phase in `0..1` and `dt` the per-sample phase increment. The residual
/// corrects the two samples either side of a discontinuity, which removes most of
/// the aliasing energy for the cost of a few operations (Välimäki and Huovilainen,
/// *Antialiasing Oscillators in Subtractive Synthesis*, IEEE Signal Processing
/// Magazine 24(2), 2007).
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Keep the two PolyBLEP corrections of a pulse from overlapping.
///
/// Each correction spans `dt` either side of an edge, so if the pulse is narrower
/// than `2*dt` the two would overlap and produce nonsense. At extreme frequencies
/// where no valid width exists, fall back to a square.
#[inline]
pub fn clamp_pulse_width(width: f32, dt: f32) -> f32 {
    let lo = 0.05f32.max(2.0 * dt);
    let hi = 0.95f32.min(1.0 - 2.0 * dt);
    if lo > hi { 0.5 } else { width.clamp(lo, hi) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_width_clamp_keeps_blep_corrections_apart() {
        for dt in [0.0001f32, 0.01, 0.1, 0.2, 0.3, 0.45] {
            for w in [0.0f32, 0.05, 0.5, 0.95, 1.0] {
                let c = clamp_pulse_width(w, dt);
                assert!((0.0..=1.0).contains(&c), "width {c} out of range");
                if 2.0 * dt < 0.5 {
                    assert!(c >= 2.0 * dt - 1e-6, "width {c} too narrow for dt {dt}");
                    assert!(c <= 1.0 - 2.0 * dt + 1e-6, "width {c} too wide for dt {dt}");
                }
            }
        }
    }

    #[test]
    fn the_residual_is_zero_away_from_an_edge_and_bounded_beside_one() {
        let dt = 0.01;
        assert_eq!(poly_blep(0.5, dt), 0.0);
        assert_eq!(poly_blep(0.0, dt), -1.0);
        for i in 0..=100 {
            let t = i as f32 / 100.0 * dt;
            assert!(poly_blep(t, dt).abs() <= 1.0);
            assert!(poly_blep(1.0 - t, dt).abs() <= 1.0);
        }
    }
}
