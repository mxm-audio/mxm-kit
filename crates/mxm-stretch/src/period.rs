//! The material's period, all the way through: what narrows the splice search to one cycle.
//!
//! **YIN** (de Cheveigné and Kawahara, *YIN, a fundamental frequency estimator for speech and music*,
//! JASA 111(4), 2002) on a decimated copy, every [`PERIOD_TRACK_HOP_S`]. Taken from the creative
//! sampler's `track_periods` and `yin` and their constants (`mxm-creative-sampler-dsp`,
//! GPL-3.0-or-later), relicensed MIT by their author for this crate (2026-10-16). What changed: it
//! reads one signal, already mixed, and `yin` answers a period in frames where the sampler's answered
//! a MIDI note that was turned back into one.
//!
//! **It declines, and the declining is what the fallback is built on**: a hop whose lowest
//! normalised difference never reaches [`PITCH_CONFIDENCE`] stores zero, and the search reads a zero
//! as no period and stays blind. Silence, noise, a drum and a chord therefore change nothing.

use crate::{Error, reserved};

/// The hop between readings: chosen for how fast material changes, ten readings a second following
/// a melody.
const PERIOD_TRACK_HOP_S: f64 = 0.100;
/// The track runs on a copy decimated by this: pitch needs no top octave, and YIN's cost is
/// quadratic in the lag range, so this is a sixteen-fold saving.
const PERIOD_DECIMATION: usize = 4;
/// The band the track commits to a period in. Narrower than a root detector's on purpose: this
/// narrows a search, and above the ceiling a period is already shorter than the smallest offset worth
/// resolving.
const PERIOD_TRACK_FLOOR_HZ: f64 = 40.0;
const PERIOD_TRACK_CEILING_HZ: f64 = 1_000.0;
/// Below this the cumulative-mean difference is a confident period rather than a coincidence: YIN's
/// own figure.
const PITCH_CONFIDENCE: f32 = 0.20;
/// Above this the best lag in a window isn't a period at all, and the window says nothing.
const PITCH_REJECT: f32 = 0.35;
/// The pitches YIN's answer must fall in, refined, to be believed.
const PITCH_FLOOR_HZ: f32 = 30.0;
const PITCH_CEILING_HZ: f32 = 5_000.0;

/// The period at each hop of the input, in its frames; zero where the track declined.
pub(crate) struct Track {
    periods: Vec<f32>,
    hop: f64,
}

impl Track {
    /// The period at input frame `position`, or `None` where the track declined.
    pub(crate) fn at(&self, position: f64) -> Option<f64> {
        if self.periods.is_empty() || !position.is_finite() {
            return None;
        }
        let slot = (position.max(0.0) / self.hop) as usize;
        let period = self.periods[slot.min(self.periods.len() - 1)];
        (period > 0.0).then_some(f64::from(period))
    }
}

/// The track of `mono` at `rate`.
pub(crate) fn track(mono: &[f32], rate: u32) -> Result<Track, Error> {
    let rate = f64::from(rate);
    let decimated_rate = rate / PERIOD_DECIMATION as f64;
    let tau_max = (decimated_rate / PERIOD_TRACK_FLOOR_HZ).ceil() as usize;
    let tau_min = (decimated_rate / PERIOD_TRACK_CEILING_HZ).floor().max(2.0) as usize;
    let window = tau_max * 2;
    let need = window + tau_max;
    let hop = (PERIOD_TRACK_HOP_S * rate).max(1.0) as usize;
    let hops = mono.len() / hop + 1;
    let mut periods = reserved(hops)?;
    if mono.len() < need * PERIOD_DECIMATION {
        periods.resize(hops, 0.0);
        return Ok(Track {
            periods,
            hop: hop as f64,
        });
    }

    // A box average before dropping samples: a crude anti-alias filter, but YIN looks for a repeat
    // rather than a spectrum, and the alternative is the top octave aliased onto the band the period
    // lives in.
    let chunks = mono.chunks(PERIOD_DECIMATION);
    let mut decimated = reserved(chunks.len())?;
    decimated.extend(chunks.map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32));

    // Sized by the rate's lag range, not by the input, so the rate bounds them.
    let mut d = vec![0.0f32; tau_max + 1];
    let mut normalised = vec![1.0f32; tau_max + 1];
    for slot in 0..hops {
        let begin = slot * hop / PERIOD_DECIMATION;
        let period = if begin + need <= decimated.len() {
            yin(
                &decimated[begin..begin + need],
                decimated_rate as f32,
                tau_min,
                tau_max,
                window,
                &mut d,
                &mut normalised,
            )
            .map_or(0.0, |lag| lag * PERIOD_DECIMATION as f32)
        } else {
            0.0
        };
        periods.push(period);
    }
    Ok(Track {
        periods,
        hop: hop as f64,
    })
}

/// YIN over one window: the period in `x`'s frames, between frames, or `None`.
fn yin(
    x: &[f32],
    rate: f32,
    tau_min: usize,
    tau_max: usize,
    window: usize,
    d: &mut [f32],
    normalised: &mut [f32],
) -> Option<f32> {
    // Step 1: the squared difference at each lag.
    d[..=tau_max].fill(0.0);
    for (tau, slot) in d.iter_mut().enumerate().take(tau_max + 1).skip(tau_min) {
        let mut sum = 0.0f32;
        for i in 0..window {
            let delta = x[i] - x[i + tau];
            sum += delta * delta;
        }
        *slot = sum;
    }

    // Step 2: the cumulative mean normalisation, which removes the bias toward lag zero.
    normalised[..=tau_max].fill(1.0);
    let mut running = 0.0f32;
    for tau in tau_min..=tau_max {
        running += d[tau];
        normalised[tau] = if running > 0.0 {
            d[tau] * (tau - tau_min + 1) as f32 / running
        } else {
            1.0
        };
    }

    // Step 3: **the first lag under the threshold, not the smallest**, followed down to its local
    // minimum. The smallest is usually an octave down; this step is why YIN beats plain
    // autocorrelation on harmonically rich material.
    let mut chosen = None;
    for at in tau_min..=tau_max {
        if normalised[at] < PITCH_CONFIDENCE {
            let mut descend = at;
            while descend < tau_max && normalised[descend + 1] < normalised[descend] {
                descend += 1;
            }
            chosen = Some(descend);
            break;
        }
    }
    let tau = match chosen {
        Some(tau) => tau,
        // Nothing was confident: the global minimum is still worth taking if it's close.
        None => {
            let mut best = tau_min;
            for at in tau_min..=tau_max {
                if normalised[at] < normalised[best] {
                    best = at;
                }
            }
            if normalised[best] >= PITCH_REJECT {
                return None;
            }
            best
        }
    };

    // Step 4: a parabola, so the answer isn't a whole number of frames.
    let refined = if tau > tau_min && tau < tau_max {
        let (a, b, c) = (normalised[tau - 1], normalised[tau], normalised[tau + 1]);
        let divisor = 2.0 * (2.0 * b - a - c);
        if divisor.abs() > f32::EPSILON {
            tau as f32 + (c - a) / divisor
        } else {
            tau as f32
        }
    } else {
        tau as f32
    };
    let hz = rate / refined;
    (PITCH_FLOOR_HZ..=PITCH_CEILING_HZ)
        .contains(&hz)
        .then_some(refined)
}
