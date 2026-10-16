//! Where the attacks are, in the input's frames.
//!
//! **Energy-based onset detection with an adaptive threshold**, the standard construction surveyed
//! by Bello, Daudet, Abdallah, Duxbury, Davies and Sandler, *A Tutorial on Onset Detection in Music
//! Signals*, IEEE TSALP 13(5), 2005: a detection function, half-wave rectification, a threshold from
//! the signal's own local statistics, and peak picking with a refractory gap. Taken from the creative
//! sampler's `detect_transients`, `accepted_hops` and `refine_onset` and their constants
//! (`mxm-creative-sampler-dsp`, GPL-3.0-or-later), relicensed MIT by their author for this crate
//! (2026-10-16). What changed: it reads one signal, already mixed, and answers frames as `usize`.
//!
//! - **The detection function is the log-energy rise of a first-differenced signal.** The first
//!   difference is a one-tap high-pass: an attack is broadband while the sustain it interrupts
//!   usually isn't.
//! - **Log energy, so the function measures a ratio**, and one threshold serves any level.
//! - **The threshold is local**, with a floor under it, so a passage that gets louder still reports
//!   onsets and a near-silent one can't clear its own noise.
//! - **It declines rather than guesses**: a wrong marker lays the heads on material with no attack
//!   in it.

use crate::{Error, reserved};

/// The hop between detector frames: short enough that refinement has little left to do.
const TRANSIENT_HOP_S: f64 = 0.005;
/// The shortest gap between two accepted onsets: below it an attack's own decay re-triggers the
/// detector.
const TRANSIENT_MIN_SPACING_S: f64 = 0.030;
/// Hops per analysis window: long enough to span several periods of a bass note, so its own cycle
/// isn't read as a string of attacks (at two hops a steady 55 Hz tone reported 55 onsets).
const TRANSIENT_WINDOW_HOPS: usize = 8;
/// Hops either side forming the local mean the threshold is taken from.
const TRANSIENT_LOCAL_HOPS: usize = 12;
/// How far above the local mean a peak must stand to be an onset.
const TRANSIENT_THRESHOLD: f32 = 2.2;
/// The floor under the adaptive threshold.
const TRANSIENT_FLOOR: f32 = 0.35;

/// The onsets of `mono` at `rate`, ascending and each once.
pub(crate) fn detect(mono: &[f32], rate: u32) -> Result<Vec<usize>, Error> {
    let rate = rate as f32;
    let hop = (TRANSIENT_HOP_S as f32 * rate).round().max(1.0) as usize;
    let window = hop * TRANSIENT_WINDOW_HOPS;
    if mono.len() < window * 3 {
        // Too short to have a neighbourhood to compare against.
        return Ok(Vec::new());
    }
    let hops = (mono.len() - window) / hop + 1;
    let mut flux = reserved(hops)?;
    flux.resize(hops, 0.0f32);
    let mut previous_log = f32::NAN;
    for (k, value) in flux.iter_mut().enumerate() {
        let base = k * hop;
        let mut energy = 0.0f64;
        for n in base..base + window {
            let before = if n == 0 { 0.0 } else { mono[n - 1] };
            let emphasised = f64::from(mono[n] - before);
            energy += emphasised * emphasised;
        }
        let rms = (energy / window as f64).sqrt() as f32;
        let log_energy = (rms + 1.0e-9).ln();
        *value = if previous_log.is_nan() {
            0.0
        } else {
            (log_energy - previous_log).max(0.0)
        };
        previous_log = log_energy;
    }

    let spacing_hops = ((TRANSIENT_MIN_SPACING_S as f32 * rate) / hop as f32)
        .ceil()
        .max(1.0) as usize;
    // Counted before the list is reserved, so it's reserved at its length.
    let mut onsets = reserved(accepted_hops(&flux, spacing_hops).count())?;
    onsets.extend(
        accepted_hops(&flux, spacing_hops).map(|k| refine_onset(mono, k * hop, hop, window)),
    );
    // **Sorted before de-duplicating, because refinement can reorder**: it moves a marker forward by
    // up to a window, more than the gap between two coarse detections.
    onsets.sort_unstable();
    onsets.dedup();
    Ok(onsets)
}

/// The hops the detector commits to: a local peak of the detection function clearing the local mean
/// by [`TRANSIENT_THRESHOLD`] and the [`TRANSIENT_FLOOR`], at least `spacing_hops` after the last one
/// accepted.
fn accepted_hops(flux: &[f32], spacing_hops: usize) -> impl Iterator<Item = usize> + '_ {
    let hops = flux.len();
    let mut last_accepted: Option<usize> = None;
    (1..hops.saturating_sub(1)).filter(move |&k| {
        let value = flux[k];
        if value <= 0.0 || value < flux[k - 1] || value < flux[k + 1] {
            return false;
        }
        let low = k.saturating_sub(TRANSIENT_LOCAL_HOPS);
        let high = (k + TRANSIENT_LOCAL_HOPS + 1).min(hops);
        let neighbourhood = &flux[low..high];
        let mean = neighbourhood.iter().sum::<f32>() / neighbourhood.len().max(1) as f32;
        if value < mean * TRANSIENT_THRESHOLD || value < TRANSIENT_FLOOR {
            return false;
        }
        if last_accepted.is_some_and(|previous| k - previous < spacing_hops) {
            return false;
        }
        last_accepted = Some(k);
        true
    })
}

/// A hop-resolution onset moved onto the frame where the energy starts rising: the steepest rise of
/// a short rectified envelope, searched **forward across the window**, since the flux at hop `k`
/// compares the window starting at `k × hop` with the one before it, so the attack lies near the
/// window's far end, not behind its start (searching backwards left markers 32 ms early).
fn refine_onset(mono: &[f32], reported: usize, hop: usize, window: usize) -> usize {
    let begin = reported;
    let end = (reported + window + hop).min(mono.len().saturating_sub(1));
    if end <= begin {
        return reported.min(mono.len().saturating_sub(1));
    }
    let span = (hop / 4).max(1);
    let level =
        |from: usize| mono[from..from + span].iter().map(|v| v.abs()).sum::<f32>() / span as f32;
    let mut best = reported.min(end);
    let mut best_rise = f32::NEG_INFINITY;
    let mut n = begin;
    while n + span * 2 <= end {
        let rise = level(n + span) - level(n);
        if rise > best_rise {
            best_rise = rise;
            best = n + span;
        }
        n += span;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::detect;
    use std::f64::consts::TAU;

    const RATE: u32 = 48_000;

    #[test]
    fn bursts_on_silence_are_found_on_their_first_frame() {
        let starts = [7_200, 21_600, 36_000];
        let mut signal = vec![0.0f32; 48_000];
        for &start in &starts {
            for i in 0..960 {
                let t = i as f64 / f64::from(RATE);
                signal[start + i] = (0.8 * (-t * 150.0).exp() * (TAU * 1_000.0 * t).cos()) as f32;
            }
        }
        assert_eq!(detect(&signal, RATE).unwrap(), starts);
    }

    #[test]
    fn a_steady_bass_tone_has_none() {
        let signal: Vec<f32> = (0..48_000)
            .map(|n| (0.5 * (TAU * 55.0 * f64::from(n) / f64::from(RATE)).sin()) as f32)
            .collect();
        assert!(detect(&signal, RATE).unwrap().is_empty());
    }
}
