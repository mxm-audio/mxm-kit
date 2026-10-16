//! `stretch` as a host meets it: every case in the order its documentation gives, the lengths, the
//! pitch, the attacks, the channels, and that it's repeatable and stops when asked.

use mxm_stretch::{Error, RATES, stretch, window_frames};
use std::f64::consts::TAU;
use std::sync::atomic::AtomicBool;

const RATE: u32 = 48_000;

fn go() -> AtomicBool {
    AtomicBool::new(false)
}

/// A sine at `hz`, `frames` long, of `level`.
fn sine(hz: f64, frames: usize, level: f64) -> Vec<f32> {
    (0..frames)
        .map(|n| (level * (TAU * hz * n as f64 / f64::from(RATE)).sin()) as f32)
        .collect()
}

/// Twelve harmonics of `hz`, each at 1/k, scaled to stay under 1.
fn harmonics(hz: f64, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| {
            let t = n as f64 / f64::from(RATE);
            let sum: f64 = (1..=12)
                .map(|k| (TAU * hz * k as f64 * t).sin() / k as f64)
                .sum();
            (0.3 * sum) as f32
        })
        .collect()
}

/// The power of `signal` at `hz`, through a Hann window: one bin of its windowed DTFT.
fn power(signal: &[f32], hz: f64) -> f64 {
    let n = signal.len() as f64;
    let (step_sin, step_cos) = (TAU * hz / f64::from(RATE)).sin_cos();
    let (mut sin, mut cos) = (0.0f64, 1.0f64);
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &value) in signal.iter().enumerate() {
        let window = 0.5 - 0.5 * (TAU * i as f64 / n).cos();
        let v = window * f64::from(value);
        re += v * cos;
        im -= v * sin;
        (sin, cos) = (
            sin * step_cos + cos * step_sin,
            cos * step_cos - sin * step_sin,
        );
    }
    re * re + im * im
}

/// The frequency of `signal`'s strongest partial within 3 % of `near`, to well under a cent: a grid,
/// then a finer one around its peak.
fn peak(signal: &[f32], near: f64) -> f64 {
    let mut best = near;
    let mut best_power = f64::NEG_INFINITY;
    let scan = |from: f64, to: f64, step: f64, best: &mut f64, best_power: &mut f64| {
        let mut hz = from;
        while hz <= to {
            let p = power(signal, hz);
            if p > *best_power {
                *best_power = p;
                *best = hz;
            }
            hz += step;
        }
    };
    scan(near * 0.97, near * 1.03, 0.1, &mut best, &mut best_power);
    let coarse = best;
    scan(
        coarse - 0.1,
        coarse + 0.1,
        0.002,
        &mut best,
        &mut best_power,
    );
    best
}

fn cents(hz: f64, reference: f64) -> f64 {
    1200.0 * (hz / reference).log2()
}

/// The middle half of a channel, where a measurement isn't about the ends.
fn middle(channel: &[f32]) -> &[f32] {
    &channel[channel.len() / 4..channel.len() * 3 / 4]
}

#[test]
fn the_refusals_come_first_in_their_order() {
    let a = sine(440.0, 10_000, 0.5);
    let short = sine(440.0, 9_000, 0.5);
    let cancel = go();
    assert_eq!(
        stretch(&[], 100, RATE, &cancel),
        Err(Error::Refused("there are no channels"))
    );
    // Different lengths win over a rate of 0, and both over frames of 0.
    assert_eq!(
        stretch(&[&a, &short], 0, 0, &cancel),
        Err(Error::Refused("its channels are of different lengths"))
    );
    let rate = Err(Error::Refused("its rate isn't between 1 kHz and 768 kHz"));
    for outside in [0, 999, 768_001] {
        assert_eq!(stretch(&[&a], 0, outside, &cancel), rate, "{outside}");
    }
    let mut broken = a.clone();
    broken[5_000] = f32::NAN;
    assert_eq!(
        stretch(&[&broken], 0, RATE, &cancel),
        Err(Error::Refused("a sample isn't a finite number"))
    );
}

#[test]
fn it_stretches_at_either_end_of_its_rates() {
    for rate in [*RATES.start(), *RATES.end()] {
        let frames = 4 * window_frames(rate);
        let input: Vec<f32> = (0..frames)
            .map(|n| (0.5 * (TAU * 50.0 * n as f64 / f64::from(rate)).sin()) as f32)
            .collect();
        for ratio in [0.8, 1.25] {
            let length = (frames as f64 * ratio).round() as usize;
            let out = stretch(&[&input], length, rate, &go()).unwrap();
            assert_eq!(out[0].len(), length, "at {rate} Hz and {ratio}");
        }
    }
}

#[test]
fn no_frames_are_empty_channels_and_no_input_is_silence() {
    let a = sine(440.0, 10_000, 0.5);
    let cancel = go();
    assert_eq!(
        stretch(&[&a, &a], 0, RATE, &cancel).unwrap(),
        vec![Vec::<f32>::new(), Vec::new()]
    );
    let empty: [f32; 0] = [];
    assert_eq!(
        stretch(&[&empty], 7, RATE, &cancel).unwrap(),
        vec![vec![0.0f32; 7]]
    );
}

#[test]
fn the_same_length_is_a_copy_bit_for_bit() {
    let a = harmonics(220.0, 30_000);
    let b = sine(330.0, 30_000, 0.4);
    let out = stretch(&[&a, &b], 30_000, RATE, &go()).unwrap();
    assert_eq!(out[0], a);
    assert_eq!(out[1], b);
}

#[test]
fn material_shorter_than_two_windows_is_copied_cut_or_followed_by_silence() {
    let window = window_frames(RATE);
    let a = sine(440.0, 2 * window - 1, 0.5);
    let longer = stretch(&[&a], a.len() + 100, RATE, &go()).unwrap();
    assert_eq!(&longer[0][..a.len()], &a[..]);
    assert!(longer[0][a.len()..].iter().all(|&v| v == 0.0));
    let shorter = stretch(&[&a], a.len() - 100, RATE, &go()).unwrap();
    assert_eq!(shorter[0], a[..a.len() - 100]);
}

#[test]
fn the_length_is_exact_and_neither_end_is_ramped() {
    let input = sine(440.0, 12_000, 0.5);
    let half = window_frames(RATE) / 2;
    for ratio in [0.25, 0.5, 0.8, 1.25, 2.0, 4.0, 50.0] {
        let frames = (input.len() as f64 * ratio).round() as usize;
        let out = stretch(&[&input], frames, RATE, &go()).unwrap();
        assert_eq!(out[0].len(), frames, "at {ratio}");
        // The input's own frames until the first splice.
        assert_eq!(&out[0][..half], &input[..half], "the start at {ratio}");
        // Nothing fades at the end: the last half window still reaches the sine's level.
        let end = &out[0][frames - half..];
        let level = end.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(level > 0.49, "the end at {ratio} reaches only {level}");
    }
}

#[test]
fn stretched_longer_it_ends_on_the_inputs_own_last_frames() {
    let input = harmonics(220.0, 24_000);
    for ratio in [1.25, 2.0] {
        let frames = (input.len() as f64 * ratio).round() as usize;
        let out = stretch(&[&input], frames, RATE, &go()).unwrap();
        assert_eq!(out[0].last(), input.last(), "at {ratio}");
    }
}

#[test]
fn a_sine_and_a_twelve_harmonic_tone_keep_their_pitch() {
    let tone = sine(440.0, 48_000, 0.5);
    let rich = harmonics(220.0, 48_000);
    for ratio in [0.5, 0.8, 1.25, 2.0] {
        let frames = (48_000.0 * ratio) as usize;
        let out = stretch(&[&tone], frames, RATE, &go()).unwrap();
        let hz = peak(middle(&out[0]), 440.0);
        assert!(cents(hz, 440.0).abs() < 1.0, "the sine at {ratio}: {hz} Hz");
        let out = stretch(&[&rich], frames, RATE, &go()).unwrap();
        let hz = peak(middle(&out[0]), 220.0);
        assert!(cents(hz, 220.0).abs() < 1.0, "the tone at {ratio}: {hz} Hz");
    }
}

#[test]
fn a_stereo_pair_keeps_each_channels_pitch_and_its_image() {
    let left = sine(440.0, 48_000, 0.5);
    let right = sine(660.0, 48_000, 0.3);
    for ratio in [0.8, 1.25] {
        let frames = (48_000.0 * ratio) as usize;
        let out = stretch(&[&left, &right], frames, RATE, &go()).unwrap();
        let (l, r) = (peak(middle(&out[0]), 440.0), peak(middle(&out[1]), 660.0));
        assert!(cents(l, 440.0).abs() < 1.0, "left at {ratio}: {l} Hz");
        assert!(cents(r, 660.0).abs() < 1.0, "right at {ratio}: {r} Hz");
        // Swapped channels come out swapped, exactly: both are read at the same places.
        let swapped = stretch(&[&right, &left], frames, RATE, &go()).unwrap();
        assert_eq!(swapped[0], out[1]);
        assert_eq!(swapped[1], out[0]);
    }
    // Identical channels stay identical.
    let out = stretch(&[&left, &left], 60_000, RATE, &go()).unwrap();
    assert_eq!(out[0], out[1]);
}

/// Bursts of a decaying 1 kHz tone, 20 ms each, on silence, starting loud at `starts`.
fn clicks(starts: &[usize], frames: usize) -> Vec<f32> {
    let mut signal = vec![0.0f32; frames];
    let burst = (0.020 * f64::from(RATE)) as usize;
    for &start in starts {
        for i in 0..burst {
            let t = i as f64 / f64::from(RATE);
            signal[start + i] = (0.8 * (-t * 150.0).exp() * (TAU * 1_000.0 * t).cos()) as f32;
        }
    }
    signal
}

/// Where sound starts after silence: the first frame of each run louder than a threshold that
/// follows at least 10 ms of quiet.
fn starts_of(signal: &[f32]) -> Vec<usize> {
    let quiet = (0.010 * f64::from(RATE)) as usize;
    let mut starts = Vec::new();
    let mut silent = quiet;
    for (n, &v) in signal.iter().enumerate() {
        if v.abs() > 0.01 {
            if silent >= quiet {
                starts.push(n);
            }
            silent = 0;
        } else {
            silent += 1;
        }
    }
    starts
}

#[test]
fn each_attack_is_heard_once_where_the_ratio_puts_it() {
    let rate = f64::from(RATE);
    let starts: Vec<usize> = (0..5)
        .map(|k| ((0.150 + 0.300 * f64::from(k)) * rate) as usize)
        .collect();
    let input = clicks(&starts, (1.6 * rate) as usize);
    assert_eq!(starts_of(&input), starts);
    let tolerance = (0.001 * rate) as i64;
    for ratio in [0.8, 1.25] {
        let frames = (input.len() as f64 * ratio).round() as usize;
        let out = stretch(&[&input], frames, RATE, &go()).unwrap();
        let heard = starts_of(&out[0]);
        assert_eq!(heard.len(), starts.len(), "at {ratio}: {heard:?}");
        for (&at, &start) in heard.iter().zip(&starts) {
            let expected = (start as f64 * ratio).round() as i64;
            assert!(
                (at as i64 - expected).abs() <= tolerance,
                "at {ratio}, an attack at {at} for {expected}"
            );
        }
    }
}

#[test]
fn the_same_input_gives_the_same_bits() {
    let input = harmonics(110.0, 40_000);
    let first = stretch(&[&input], 52_345, RATE, &go()).unwrap();
    let second = stretch(&[&input], 52_345, RATE, &go()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn a_cancel_stops_it_at_the_first_splice() {
    let input = sine(440.0, 48_000, 0.5);
    let cancel = AtomicBool::new(true);
    assert_eq!(
        stretch(&[&input], 60_000, RATE, &cancel),
        Err(Error::Cancelled)
    );
}
