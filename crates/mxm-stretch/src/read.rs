//! A channel read between its frames: the sixteen-tap Blackman-windowed sinc.
//!
//! Taken from the creative sampler's unity-rate kernel (`UNITY_KERNEL`, `sinc_read`'s tabled path;
//! `mxm-creative-sampler-dsp`, GPL-3.0-or-later), relicensed MIT by their author for this crate
//! (2026-10-16). A head always reads at the input's own speed, so the unity kernel is the only one
//! needed. What changed: a position is f64; a read on a frame is that frame, exactly; and frames
//! past either end are silence.

use std::sync::LazyLock;

/// Taps in the kernel.
const SINC_TAPS: usize = 16;
/// Rows of the kernel between two frames, interpolated pairwise: rounding to the nearest row would be
/// a fractional delay quantised to ±1/1024 of a frame, audible above the read's own error.
const SINC_PHASES: usize = 512;

/// The kernel at each of [`SINC_PHASES`] fractions, each row normalised to sum to one, with a
/// duplicate last row so interpolating to the next never needs a bound. Built once.
static KERNEL: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let mut table = vec![0.0f32; (SINC_PHASES + 1) * SINC_TAPS];
    let first_tap = -((SINC_TAPS / 2 - 1) as f64);
    let half_width = SINC_TAPS as f64 / 2.0;
    for phase in 0..=SINC_PHASES {
        let fraction = phase as f64 / SINC_PHASES as f64;
        let row = &mut table[phase * SINC_TAPS..(phase + 1) * SINC_TAPS];
        let mut weights = [0.0f64; SINC_TAPS];
        let mut sum = 0.0f64;
        for (tap, weight) in weights.iter_mut().enumerate() {
            let distance = fraction - first_tap - tap as f64;
            let sinc = if distance.abs() < 1.0e-12 {
                1.0
            } else {
                (std::f64::consts::PI * distance).sin() / (std::f64::consts::PI * distance)
            };
            let cosine = (std::f64::consts::PI * distance / half_width).cos();
            let window = 0.42 + 0.5 * cosine + 0.08 * (2.0 * cosine * cosine - 1.0);
            *weight = sinc * window;
            sum += *weight;
        }
        for (slot, weight) in row.iter_mut().zip(weights) {
            *slot = if sum.abs() > 1.0e-12 {
                (weight / sum) as f32
            } else {
                weight as f32
            };
        }
    }
    table
});

/// `channel` at `position`, in frames: the frame itself when `position` falls on one, silence past
/// either end.
pub(crate) fn at(channel: &[f32], position: f64) -> f32 {
    let floor = position.floor();
    let centre = floor as i64;
    let frame = |index: i64| {
        usize::try_from(index)
            .ok()
            .and_then(|index| channel.get(index))
            .copied()
            .unwrap_or(0.0)
    };
    let fraction = position - floor;
    if fraction == 0.0 {
        return frame(centre);
    }
    let scaled = fraction * SINC_PHASES as f64;
    let phase = (scaled as usize).min(SINC_PHASES - 1);
    let blend = (scaled - phase as f64) as f32;
    let table = &*KERNEL;
    let row = &table[phase * SINC_TAPS..(phase + 1) * SINC_TAPS];
    let next = &table[(phase + 1) * SINC_TAPS..(phase + 2) * SINC_TAPS];
    let first = centre - (SINC_TAPS as i64 / 2 - 1);
    let mut sum = 0.0f32;
    for (tap, (low, high)) in row.iter().zip(next).enumerate() {
        let weight = low + (high - low) * blend;
        sum += frame(first + tap as i64) * weight;
    }
    sum
}
