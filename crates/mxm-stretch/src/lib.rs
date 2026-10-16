//! A recording stretched to an exact length, its pitch kept: the MXM collection's WSOLA, made a
//! whole-buffer stretch.
//!
//! [`stretch`] returns every channel of a recording exactly `frames` long. It is the creative
//! sampler's Stretch reader, offline: two overlap-add heads reading [`STRETCH_WINDOW_S`] Hann
//! windows half a window apart, each laid where the material should be by then and moved by the
//! splice search to where it best continues what the other head is still playing. That is
//! **WSOLA**, after Verhelst and Roelands, *An overlap-add technique based on waveform similarity
//! (WSOLA) for high quality time-scale modification of speech*, ICASSP 1993. A head reads at the
//! recording's own speed, so the pitch stays; the place it's laid moves at the ratio, so the length
//! changes.
//!
//! **Where it comes from.** The splice search (`search`), the period track that narrows it
//! (`period`), the onset detector (`onsets`) and the sixteen-tap sinc read (`read`) are the
//! sampler's (`mxm-creative-sampler-dsp`, GPL-3.0-or-later), relicensed MIT by their author for
//! this crate (2026-10-16); each file says what it took. What a whole buffer allows, which a voice
//! reading as it plays can't, is this crate's own:
//!
//! - **An attack is read once, from its first frame, where the ratio puts it.** The input is read as
//!   pieces between its onsets: at an onset's place in the output both heads are laid on it, so the
//!   attack is the input's own from its first frame, and no head reads past the next onset before
//!   that onset's place, so nothing of an attack is heard early or twice. Onsets closer than a
//!   window to the last one kept, or to either end, are passed over.
//! - **No ramps.** Both heads start on the first frame, so the output's first frames are the input's
//!   own until the first splice; no head reads past the input's end, nor before its start; and the
//!   two windows' sum is divided out everywhere, so nothing fades at either end. Stretched longer,
//!   the heads run ahead of where the material should be and are held back at the end, so the
//!   output ends on the input's own last frames; made shorter, they trail it, and what's dropped
//!   to fit may include the very end.
//! - **The same length is a copy**, bit for bit: a buffer has no state to carry across, unlike the
//!   sampler's voice, whose unity is an invariant rather than a bypass.
//!
//! **Deterministic**: the same input gives the same bits from the same build on the same platform;
//! no threads, no randomness, correlations summed in f64. Every buffer sized by the input or the
//! output is reserved fallibly, so a host that stretches off its audio thread gets
//! [`Error::NoMemory`], never an abort.

#![forbid(unsafe_code)]

mod onsets;
mod period;
mod read;
mod search;

use std::fmt;
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, Ordering};

/// The overlap-add window, in seconds: the sampler's, whose crate measured it against windows
/// derived from the material's period and kept it.
pub const STRETCH_WINDOW_S: f64 = 0.080;

/// The rates a stretch is made at, in frames a second: the sampler's bounds. Below them the period
/// track has no lag range to search; above them the buffers sized by the rate stop being small.
pub const RATES: RangeInclusive<u32> = 1_000..=768_000;

/// Why a stretch gave nothing back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// `cancel` was set; the work stops at the next splice.
    Cancelled,
    /// A buffer sized by the input or the output couldn't be reserved.
    NoMemory,
    /// The input can't be stretched, and why.
    Refused(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cancelled => f.write_str("the stretch was cancelled"),
            Error::NoMemory => f.write_str("there isn't the memory to stretch it"),
            Error::Refused(why) => write!(f, "it can't be stretched: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// The window at `rate`, in frames: [`STRETCH_WINDOW_S`] rounded, held between 32 and 32 768 as the
/// sampler holds it, and made even, so the two heads are exactly half a window apart.
#[must_use]
pub fn window_frames(rate: u32) -> usize {
    let frames = (STRETCH_WINDOW_S * f64::from(rate))
        .round()
        .clamp(32.0, 32_768.0) as usize;
    frames & !1
}

/// Every channel of `input` stretched to exactly `frames`, its pitch kept, at `rate` (which sets
/// the window and the search in frames). `cancel` is read at every splice.
///
/// The cases, in the order they're decided, the first that applies winning:
/// 1. refused: no channels, then channels of different lengths, then a `rate` outside [`RATES`],
///    then a sample that isn't a finite number, whatever `frames` is;
/// 2. `frames` of 0: empty channels;
/// 3. an empty input: silence, `frames` long;
/// 4. `frames` equal to the input's length: a copy, bit for bit;
/// 5. material shorter than two windows ([`window_frames`]) isn't stretched, since no splice fits
///    in it: it's copied from its start, cut at `frames` or followed by silence to it, nothing
///    ramped;
/// 6. anything else is stretched.
///
/// One channel is analysed as it is, two as their sum or, where they cancel, their difference (the
/// sampler's choice), more as their mean; every channel is read at the same places, so the image
/// holds.
pub fn stretch(
    input: &[&[f32]],
    frames: usize,
    rate: u32,
    cancel: &AtomicBool,
) -> Result<Vec<Vec<f32>>, Error> {
    let Some(first) = input.first() else {
        return Err(Error::Refused("there are no channels"));
    };
    let length = first.len();
    if input.iter().any(|channel| channel.len() != length) {
        return Err(Error::Refused("its channels are of different lengths"));
    }
    if !RATES.contains(&rate) {
        return Err(Error::Refused("its rate isn't between 1 kHz and 768 kHz"));
    }
    if input
        .iter()
        .any(|channel| channel.iter().any(|v| !v.is_finite()))
    {
        return Err(Error::Refused("a sample isn't a finite number"));
    }
    let window = window_frames(rate);
    if frames == 0 || length == 0 || frames == length || length < 2 * window {
        return each(input, |channel| copied(channel, frames));
    }
    Stretcher::new(input, frames, rate, window)?.run(cancel)
}

/// `make` of every channel, the list of them reserved fallibly too.
fn each(
    input: &[&[f32]],
    mut make: impl FnMut(&[f32]) -> Result<Vec<f32>, Error>,
) -> Result<Vec<Vec<f32>>, Error> {
    let mut out = reserved(input.len())?;
    for channel in input {
        out.push(make(channel)?);
    }
    Ok(out)
}

/// `channel` from its start, cut at `frames` or followed by silence to it: empty when `frames` is
/// 0, silence when `channel` is.
fn copied(channel: &[f32], frames: usize) -> Result<Vec<f32>, Error> {
    let mut out = reserved(frames)?;
    out.extend_from_slice(&channel[..frames.min(channel.len())]);
    out.resize(frames, 0.0);
    Ok(out)
}

/// An empty buffer with room for exactly `capacity` values, or [`Error::NoMemory`]: filling it to
/// its capacity never reallocates.
pub(crate) fn reserved<T>(capacity: usize) -> Result<Vec<T>, Error> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(capacity)
        .map_err(|_| Error::NoMemory)?;
    Ok(buffer)
}

/// One overlap-add head: the output frame its window started at, and the input frame it read
/// there. It reads on at the input's own speed, a frame of input for a frame of output.
#[derive(Clone, Copy)]
struct Head {
    anchor: i64,
    position: f64,
}

impl Head {
    /// The input frame it reads at output frame `n`.
    fn read_at(self, n: usize) -> f64 {
        self.position + (n as i64 - self.anchor) as f64
    }
}

/// A stretch under way: the input, what's known about it, and where its onsets land.
struct Stretcher<'a> {
    input: &'a [&'a [f32]],
    frames: usize,
    rate: u32,
    window: usize,
    /// The channels as one, for the search, the onsets and the period track.
    mono: Vec<f32>,
    /// Every onset found, for keeping the search blind near an attack.
    onsets: Vec<usize>,
    /// The onsets the heads are laid on, each with its place in the output, and last the input's
    /// end at the output's.
    boundaries: Vec<(usize, usize)>,
    periods: period::Track,
    /// The Hann window, a value per frame.
    hann: Vec<f32>,
}

impl<'a> Stretcher<'a> {
    fn new(input: &'a [&'a [f32]], frames: usize, rate: u32, window: usize) -> Result<Self, Error> {
        let length = input[0].len();
        let mono = mono(input)?;
        let onsets = onsets::detect(&mono, rate)?;
        let periods = period::track(&mono, rate)?;
        let mut boundaries = reserved(onsets.len() + 1)?;
        let (mut last, mut last_at) = (0, 0);
        for &onset in &onsets {
            if onset < last + window || length - onset < window {
                continue;
            }
            let at = (onset as f64 * frames as f64 / length as f64).round() as usize;
            if at <= last_at || at >= frames {
                continue;
            }
            boundaries.push((onset, at));
            (last, last_at) = (onset, at);
        }
        boundaries.push((length, frames));
        let mut hann = reserved(window)?;
        hann.extend((0..window).map(|k| {
            let phase = k as f64 / window as f64;
            (0.5 - 0.5 * (std::f64::consts::TAU * phase).cos()) as f32
        }));
        Ok(Self {
            input,
            frames,
            rate,
            window,
            mono,
            onsets,
            boundaries,
            periods,
            hann,
        })
    }

    fn run(&self, cancel: &AtomicBool) -> Result<Vec<Vec<f32>>, Error> {
        let length = self.mono.len();
        let half = self.window / 2;
        let mut out = each(self.input, |_| reserved(self.frames))?;
        // Input frames per output frame: where the material should be at each output frame.
        let step = length as f64 / self.frames as f64;
        // Both heads start on the first frame, half a window apart in their windows: the output is
        // the input's own until the first splice.
        let mut heads = [
            Head {
                anchor: -(half as i64),
                position: -(half as f64),
            },
            Head {
                anchor: 0,
                position: 0.0,
            },
        ];
        let mut next = 0;
        for n in 0..self.frames {
            // An onset's place: both heads laid on it, a head whose window ends here starting anew
            // there, so the attack is read from its first frame by both.
            if self.boundaries[next].1 == n {
                let onset = self.boundaries[next].0 as f64;
                for head in &mut heads {
                    if n as i64 - head.anchor == self.window as i64 {
                        head.anchor = n as i64;
                    }
                    head.position = onset - (n as i64 - head.anchor) as f64;
                }
                next += 1;
            }
            for index in 0..2 {
                if n as i64 - heads[index].anchor == self.window as i64 {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(Error::Cancelled);
                    }
                    let reference = heads[1 - index].read_at(n);
                    heads[index] = self.splice(n, n as f64 * step, reference, next);
                }
            }
            let (a, b) = (heads[0].read_at(n), heads[1].read_at(n));
            if a == b {
                for (channel, out) in self.input.iter().zip(out.iter_mut()) {
                    out.push(read::at(channel, a));
                }
            } else {
                let wa = f64::from(self.hann[(n as i64 - heads[0].anchor) as usize]);
                let wb = f64::from(self.hann[(n as i64 - heads[1].anchor) as usize]);
                for (channel, out) in self.input.iter().zip(out.iter_mut()) {
                    let sum =
                        wa * f64::from(read::at(channel, a)) + wb * f64::from(read::at(channel, b));
                    out.push((sum / (wa + wb)) as f32);
                }
            }
        }
        Ok(out)
    }

    /// A head laid anew at output frame `n`: where the material should be (`travel`), moved by the
    /// splice search to best continue what the other head reads (`reference`), and held back so it
    /// reads nothing of the next onset (`self.boundaries[next]`, the input's end last) before that
    /// onset's place.
    fn splice(&self, n: usize, travel: f64, reference: f64, next: usize) -> Head {
        let blind = (search::ALIGN_SEARCH_S * f64::from(self.rate)).round() as i32;
        // **The period narrows the search, but not near an attack** (the sampler's rule): across
        // an onset the material isn't periodic, and narrowing onto the sustain's period there leaves
        // the search unable to reach the offset the attack needs.
        let range = match self.periods.at(travel) {
            Some(period) if !self.near_attack(travel) => (period * 0.5 + 1.0).max(0.0) as i32,
            _ => blind,
        };
        let span = (search::ALIGN_SPAN_S * f64::from(self.rate)).min(self.window as f64 * 0.5);
        let offset = search::align_offset(&self.mono, reference, travel, span, range);
        let (onset, at) = self.boundaries[next];
        // Its last read before the onset's place must come before the onset.
        let reach = (at - n).min(self.window);
        let position = (travel + offset).min(onset as f64 - reach as f64).max(0.0);
        Head {
            anchor: n as i64,
            position,
        }
    }

    /// Whether an onset lies within a window of `travel`, either side.
    fn near_attack(&self, travel: f64) -> bool {
        let reach = self.window as f64;
        let next = self
            .onsets
            .partition_point(|&onset| (onset as f64) < travel);
        let after = self
            .onsets
            .get(next)
            .is_some_and(|&onset| onset as f64 - travel < reach);
        let before = next
            .checked_sub(1)
            .and_then(|previous| self.onsets.get(previous))
            .is_some_and(|&onset| travel - (onset as f64) < reach);
        after || before
    }
}

/// The channels as one signal for analysis: one as it is; two as their sum, or their difference
/// where they cancel when summed (the sampler's `choose_mono_mix`: an anti-phase pair sums to
/// silence, and every analysis would decline on it); more as their mean.
fn mono(input: &[&[f32]]) -> Result<Vec<f32>, Error> {
    let length = input[0].len();
    let mut mono = reserved(length)?;
    match input {
        [only] => mono.extend_from_slice(only),
        [left, right] => {
            let mix = choose_mono_mix(left, right);
            mono.extend(
                left.iter()
                    .zip(right.iter())
                    .map(|(l, r)| l * mix[0] + r * mix[1]),
            );
        }
        _ => {
            let scale = 1.0 / input.len() as f32;
            mono.extend((0..length).map(|n| input.iter().map(|c| c[n]).sum::<f32>() * scale));
        }
    }
    Ok(mono)
}

/// Whether a pair's channels reinforce or cancel when summed: `[0.5, 0.5]`, or `[0.5, -0.5]` where
/// their difference carries more energy than their sum. The sampler's, for two channels.
fn choose_mono_mix(left: &[f32], right: &[f32]) -> [f32; 2] {
    let mut sum = 0.0f64;
    let mut difference = 0.0f64;
    for (&l, &r) in left.iter().zip(right) {
        let s = f64::from(l + r);
        let d = f64::from(l - r);
        sum += s * s;
        difference += d * d;
    }
    if difference > sum {
        [0.5, -0.5]
    } else {
        [0.5, 0.5]
    }
}
