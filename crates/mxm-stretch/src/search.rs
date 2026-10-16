//! The splice search: where a head about to be laid best continues what the other head is playing.
//!
//! **WSOLA**, after Verhelst and Roelands, *An overlap-add technique based on waveform similarity
//! (WSOLA) for high quality time-scale modification of speech*, ICASSP 1993. Taken from the creative
//! sampler's `align_offset` and its constants (`mxm-creative-sampler-dsp`, GPL-3.0-or-later),
//! relicensed MIT by their author for this crate (2026-10-16). What changed: positions are f64,
//! since a recording can be longer than f32 counts frames exactly; the correlation is summed in
//! f64; a head here always reads at the input's own speed; and a read past either end is silence
//! where the sampler's wrapped its loop. The sampler's crate `NOTES.md`, *The splice is aligned,
//! and that is the whole reader*, holds the measurements behind each rule.

/// How far either side of where the material should be a head may look for a better splice. It has
/// to reach a period of the lowest note anyone plays through it, or it can't find the alignment that
/// exists; reaching further buys nothing and costs linearly.
pub(crate) const ALIGN_SEARCH_S: f64 = 0.010;
/// Points compared per candidate offset: the correlation is a similarity over a span, not audio, so
/// it's sampled rather than walked.
const ALIGN_POINTS: usize = 64;
/// The span the comparison covers, held to half a window by the caller.
pub(crate) const ALIGN_SPAN_S: f64 = 0.020;
/// The coarse pass steps by at least this many frames; finer passes then narrow on the winner.
const ALIGN_COARSE_STRIDE: i32 = 16;
/// How many offsets either side the coarse pass probes, whatever the rate: the stride widens to hold
/// it, so a search costs the same at 768 kHz as at 48 kHz.
const ALIGN_COARSE_PROBES: i32 = 30;
/// How much better a shifted candidate must correlate before it displaces a smaller shift. **A
/// strict `>` isn't a tie-break, because a tie is never exactly a tie in floating point**: on an
/// exactly periodic input the offsets zero and one period are the same alignment, and rounding would
/// decide between them. The margin sits far above that noise and far below a real difference.
const ALIGN_TIE_MARGIN: f64 = 1.0e-3;
/// How close to a perfect correlation counts as the same material, for the early return that keeps a
/// head reading where it should when it already continues the other exactly. Much tighter than the
/// margin, which asks a different question: whether one offset beats another.
const ALIGN_IDENTITY_EPS: f64 = 1.0e-5;

/// The offset, in frames and between frames, to add to `candidate` (where the material should be)
/// so the head laid there best continues the head reading at `reference`, within `search` frames
/// either way, comparing `span` frames of `mono`.
///
/// **Ties go to zero offset, and that is load-bearing**: a perfectly periodic input correlates as
/// well a period away, and a search that took that offset would be right in spectrum and wrong in
/// time. Evaluating zero first and improving only by [`ALIGN_TIE_MARGIN`] is the whole mechanism.
pub(crate) fn align_offset(
    mono: &[f32],
    reference: f64,
    candidate: f64,
    span: f64,
    search: i32,
) -> f64 {
    if search <= 0 {
        return 0.0;
    }
    let dt = (span / ALIGN_POINTS as f64).max(1.0e-3);
    let sample = |position: f64| {
        let index = position.round();
        if index < 0.0 || index >= mono.len() as f64 {
            0.0
        } else {
            f64::from(mono[index as usize])
        }
    };
    let score = |offset: i32| {
        let shift = f64::from(offset);
        let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
        for point in 0..ALIGN_POINTS {
            let walk = point as f64 * dt;
            let a = sample(reference + walk);
            let b = sample(candidate + shift + walk);
            ab += a * b;
            aa += a * a;
            bb += b * b;
        }
        let denominator = (aa * bb).sqrt();
        if denominator > 1.0e-12 {
            ab / denominator
        } else {
            0.0
        }
    };
    let mut best = 0i32;
    let mut best_score = f64::NEG_INFINITY;
    let evaluate = |offset: i32, best: &mut i32, best_score: &mut f64| {
        let candidate_score = score(offset);
        if candidate_score > *best_score + ALIGN_TIE_MARGIN {
            *best_score = candidate_score;
            *best = offset;
        }
    };

    // Coarse: zero first, then outward in steps. The stride scales with the range, so the number of
    // probes doesn't.
    evaluate(0, &mut best, &mut best_score);
    let stride = (search / ALIGN_COARSE_PROBES).max(ALIGN_COARSE_STRIDE);
    let mut offset = stride;
    while offset <= search {
        evaluate(offset, &mut best, &mut best_score);
        evaluate(-offset, &mut best, &mut best_score);
        offset += stride;
    }
    // Fine, in levels, each narrowing by the coarse pass's factor around the winner and probing the
    // same bounded number of candidates, from the middle out, so a tie still goes to the smaller
    // shift.
    let mut level = stride;
    while level > 1 {
        let step = (level / ALIGN_COARSE_PROBES).max(1);
        let centre = best;
        let mut offset = step;
        while offset <= level {
            for candidate_offset in [centre + offset, centre - offset] {
                if candidate_offset.abs() <= search {
                    evaluate(candidate_offset, &mut best, &mut best_score);
                }
            }
            offset += step;
        }
        level = step;
    }

    // **Between frames, because half a frame isn't a small error at the top of the spectrum**: at a
    // 220 Hz fundamental's twelfth harmonic it's ten degrees of phase, heard as roughness. A
    // parabola through the three scores around the winner places it; the read interpolates.
    let scores = [score(best - 1), score(best), score(best + 1)];
    // An exact continuation has nothing to refine, and saying so keeps the head where it is.
    if best == 0 && scores[1] >= 1.0 - ALIGN_IDENTITY_EPS {
        return 0.0;
    }
    let curvature = scores[0] - 2.0 * scores[1] + scores[2];
    let fraction = if curvature.abs() > 1.0e-9 {
        (0.5 * (scores[0] - scores[2]) / curvature).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    // Held to the search: at its edge the parabola could reach a frame past it.
    (f64::from(best) + fraction).clamp(-f64::from(search), f64::from(search))
}

#[cfg(test)]
mod tests {
    use super::align_offset;
    use std::f64::consts::TAU;

    /// A sine whose best continuation lies past the search: the answer is held to its edge.
    #[test]
    fn an_answer_at_the_searchs_edge_stays_inside_it() {
        let mono: Vec<f32> = (0..20_000)
            .map(|n| (TAU * f64::from(n) / 400.0).sin() as f32)
            .collect();
        for (shift, search) in [(60.0, 50), (-60.0, 50), (35.0, 32)] {
            let offset = align_offset(&mono, 8_000.0, 8_000.0 - shift, 960.0, search);
            assert!(
                offset.abs() <= f64::from(search) && offset.abs() >= f64::from(search) - 1.0,
                "{offset} for a best of {shift} within {search}"
            );
        }
    }
}
