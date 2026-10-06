//! Pulling one channel out of an interleaved buffer.
//!
//! Six or more verbatim copies of `left` and `right` existed across the player's behaviour tests.
//! There was nothing wrong with any of them; there was no reason for there to be six.

/// The left channel of a stereo interleaved buffer.
pub fn left(interleaved: &[f32]) -> Vec<f32> {
    channel(interleaved, 0, 2)
}

/// The right channel of a stereo interleaved buffer.
pub fn right(interleaved: &[f32]) -> Vec<f32> {
    channel(interleaved, 1, 2)
}

/// Channel `index` of a buffer interleaved across `channels`.
///
/// Crate-private: [`left`] and [`right`] are what the repository actually asks for, and a general
/// channel selector with no caller is the convenience the extraction gate exists to refuse.
///
/// An out-of-range index or a zero channel count yields an empty buffer rather than a panic: a
/// measurement helper that panics on a capture whose shape surprised it turns a diagnosis into a
/// crash.
pub(crate) fn channel(interleaved: &[f32], index: usize, channels: usize) -> Vec<f32> {
    if channels == 0 || index >= channels {
        return Vec::new();
    }
    interleaved
        .iter()
        .skip(index)
        .step_by(channels)
        .copied()
        .collect()
}

/// Interleave channel buffers into one, stopping at the shortest.
///
/// Crate-private: its only callers are this crate's own tests, and a function whose second consumer
/// is its own test would make the two-consumer gate vacuous.
///
/// The inverse of splitting a buffer into channels, and what a WAV writer is handed.
#[cfg(test)]
pub(crate) fn interleave(channels: &[Vec<f32>]) -> Vec<f32> {
    let Some(frames) = channels.iter().map(Vec::len).min() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(frames * channels.len());
    for frame in 0..frames {
        for channel in channels {
            out.push(channel[frame]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_buffer_splits_and_rejoins() {
        let interleaved = vec![1.0f32, -1.0, 2.0, -2.0, 3.0, -3.0];
        assert_eq!(left(&interleaved), vec![1.0, 2.0, 3.0]);
        assert_eq!(right(&interleaved), vec![-1.0, -2.0, -3.0]);
        assert_eq!(
            interleave(&[left(&interleaved), right(&interleaved)]),
            interleaved
        );
    }

    #[test]
    fn more_than_two_channels_work_the_same_way() {
        let interleaved = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let parts: Vec<Vec<f32>> = (0..3).map(|i| channel(&interleaved, i, 3)).collect();
        assert_eq!(parts, vec![vec![1.0, 4.0], vec![2.0, 5.0], vec![3.0, 6.0]]);
        assert_eq!(interleave(&parts), interleaved);
    }

    #[test]
    fn a_shape_that_surprises_us_is_empty_rather_than_a_panic() {
        // A capture whose channel count was not what the test assumed should produce a diagnosis,
        // not a crash inside the measurement.
        assert!(channel(&[1.0, 2.0], 2, 2).is_empty());
        assert!(channel(&[1.0, 2.0], 0, 0).is_empty());
        assert!(interleave(&[]).is_empty());
        // A ragged set stops at the shortest rather than reading past the end.
        assert_eq!(interleave(&[vec![1.0, 2.0], vec![3.0]]), vec![1.0, 3.0]);
    }
}
