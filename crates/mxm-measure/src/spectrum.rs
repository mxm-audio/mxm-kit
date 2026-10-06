//! Spectral quantities, and the transform underneath them.
//!
//! # One name used to mean three different things
//!
//! `magnitude_at` existed twelve times in this repository and computed **three physically different
//! quantities**, so a figure from one crate could not be compared with a figure from another and
//! nobody reading either could tell without opening both:
//!
//! | Was written as | Is really | Now |
//! |---|---|---|
//! | single-bin correlation, `\|X\|/N` | a component's amplitude, **half-scaled** | [`component_amplitude`] |
//! | single-bin correlation, `2\|X\|/N` | a component's amplitude | [`component_amplitude`] |
//! | peak of the settled response to a sine, over input amplitude | a **transfer gain** | [`transfer_gain`] |
//! | output energy over input energy, in decibels | an **energy gain** | *not here* — one consumer |
//!
//! The first two are the same quantity and disagreed by a factor of two; this module keeps the
//! amplitude convention, so a figure taken with the half-scaled form is **6 dB below** what this
//! reports. The third and fourth are not the same quantity as each other or as the first two: a
//! transfer gain is a property of a system under excitation, a component amplitude is a property of
//! a buffer, and an energy gain integrates over whatever else the system put in the band.
//!
//! Energy gain has exactly one consumer in the repository, so it stays a local composition over
//! [`crate::level::rms`] and a decibel conversion rather than becoming an API here.

use std::f64::consts::{PI, TAU};

/// The analysis window length the collection's reference measurements use.
///
/// A power of two for the radix-2 transform, and long enough that a 55 Hz fundamental still has
/// hundreds of harmonics inside it.
pub const N: usize = 1 << 16;

/// The **amplitude of one sinusoidal component** of a buffer, by a single-bin correlation.
///
/// Returns a linear amplitude: a unit-amplitude sine at `hz` reads 1.0. Cost is proportional to the
/// buffer, one frequency at a time — which is why every one of the twelve copies chose this over a
/// transform, and why it is the right tool inside a `cargo test` that runs in debug.
///
/// **Domain: use a whole number of cycles**, from [`crate::stimulus::periodic_sine`], or the result
/// includes spectral leakage that will be blamed on whatever is under test. Where the window length
/// was not yours to choose, the result is still useful *comparatively* — this pitch against that one
/// — but it is no longer the component's absolute amplitude, and a call site relying on that should
/// say so. An empty buffer has no component, so the result is absent.
///
/// **DC and Nyquist are scaled differently, and getting that wrong is a factor of two.** A real
/// sinusoid strictly between them splits its energy between a positive and a negative frequency, so
/// recovering the amplitude takes the factor of two below. DC and Nyquist have no negative twin —
/// they are their own mirror — so the factor is one. Without the distinction a unit DC offset reads
/// as an amplitude of **2.0**, which is not a small error in a function whose whole job is to report
/// how much of something is present.
pub fn component_amplitude(x: &[f32], hz: f64, rate: f64) -> Option<f64> {
    // A non-positive rate makes the angle non-finite and every trig call NaN; a non-finite hz does
    // the same. Both are caller errors, and absence says so where a NaN would spread.
    if x.is_empty() || !rate.is_finite() || rate <= 0.0 || !hz.is_finite() {
        return None;
    }
    // **Reduced before it is scaled, then advanced.** Forming `TAU * hz / rate` overflows for a
    // large `hz` — `hz == rate == f64::MAX` is a frequency that aliases to DC, and the naive form
    // turns it into an infinity and then a NaN — and `w * n` overflows later in a long buffer even
    // when `w` is fine. Reducing to cycles per sample first, and advancing by it, cannot do either.
    let increment = (hz / rate).rem_euclid(1.0);
    if !increment.is_finite() {
        return None;
    }
    let (mut re, mut im) = (0.0f64, 0.0f64);
    let mut phase = 0.0f64;
    for &s in x {
        let p = TAU * phase;
        re += f64::from(s) * p.cos();
        im -= f64::from(s) * p.sin();
        phase = (phase + increment).rem_euclid(1.0);
    }
    // Two everywhere except its own mirror: DC and Nyquist have no negative-frequency twin to add
    // back, so their scaling is one. `hz` is reduced into `0..rate` first, because a caller may ask
    // for Nyquist as `rate / 2` or for DC as `rate` and mean the same bin.
    //
    // **Compared exactly, with no tolerance.** A draft used `1e-12`, which is a level somebody chose
    // deciding an answer — and it decided it by a **factor of two**, which is as large as a decision
    // gets here. A frequency is the mirror bin or it is not: ask for `0.0` or for `rate / 2.0` and
    // get the endpoint scaling, ask for anything else and get the ordinary one. A caller a hair off
    // Nyquist genuinely is measuring an ordinary component, and the discontinuity is the truth about
    // a real DFT rather than an artefact of this function.
    let mirrored = increment == 0.0 || increment == 0.5;
    let scale = if mirrored { 1.0 } else { 2.0 };
    let amplitude = scale * re.hypot(im) / x.len() as f64;
    // A non-finite sample poisons the sum; do not report it as a measurement.
    amplitude.is_finite().then_some(amplitude)
}

/// An **odd** number of periods near `hz` in a window of `n` samples, so that `p·rate/n` is exactly
/// periodic there.
///
/// This is the whole reason the collection's measurements need no window function. `p` odd and `n` a
/// power of two gives `gcd(p, n) = 1`, so harmonics land exactly on bin `k·p`, aliases land exactly
/// on `k·p mod n`, and the two sets never collide. See `docs/oscillators/01-fundamentals.md` §1.5.
///
/// **Near, not nearest, and deliberately so.** It rounds to the closest period count and then steps
/// *up* if that is even, so the result can sit up to 1.5 bins above `hz` where stepping down would
/// have been closer — at 48 kHz with `n` = 4096, 440 Hz is 37.55 periods, rounds to 38, and becomes
/// 39 rather than 37. Two things make that the right behaviour to keep rather than fix:
/// **exactness is the property that matters here, not proximity** — any odd count gives a leakage-free
/// window, and at the harness length `N` a bin is under a hertz — and this is the arithmetic every
/// figure in `docs/oscillators/` and `docs/modulation/` was measured with. Changing it would move
/// two references' worth of recorded evidence to land a probe a fraction of a bin closer to a
/// frequency that was arbitrary in the first place.
///
/// Ask [`crate::stimulus::periodic_frequency`] what frequency you actually get when it matters.
pub fn periods_for(hz: f64, rate: f64, n: usize) -> usize {
    if rate.is_nan() || rate <= 0.0 || !hz.is_finite() || n == 0 {
        return 1;
    }
    let p = (hz * n as f64 / rate).round() as i64;
    let p = if p % 2 == 0 { p + 1 } else { p };
    p.max(1) as usize
}

/// How much a **system** does to one frequency: the peak of its settled response to a sine at `hz`,
/// divided by the input amplitude.
///
/// A dimensionless ratio — 1.0 for a unity path, `1/sqrt(2)` at a first-order corner. This is the
/// quantity three filter test modules were computing with byte-identical private helpers, doc
/// comment included.
///
/// The excitation amplitude is the caller's, and it matters: **keep it small enough that the
/// system's saturators stay in their linear region**, or the measurement describes the drive rather
/// than the filter. `settle` is discarded before measuring, because the first samples of a response
/// are its transient and not its gain.
///
/// Absent where `hz` or `rate` is not positive, or where nothing was measured.
pub fn transfer_gain(probe: Probe, rate: f64, mut system: impl FnMut(f32) -> f32) -> Option<f64> {
    if probe.hz <= 0.0
        || rate <= 0.0
        || probe.amplitude <= 0.0
        || !probe.hz.is_finite()
        || !probe.amplitude.is_finite()
        || !rate.is_finite()
    {
        return None;
    }
    // **The excitation has to survive the cast.** The system under test takes `f32`, so an amplitude
    // below `f32`'s smallest positive value arrives as silence — and dividing the resulting zero peak
    // by the `f64` amplitude reports a gain of **zero** for a system that is working perfectly.
    // Likewise an amplitude past `f32::MAX` arrives as an infinity.
    if (probe.amplitude as f32) <= 0.0 || !(probe.amplitude as f32).is_finite() {
        return None;
    }
    // **Checked, and checked *first*.** Every field of `Probe` is public, so `Probe::new`'s refusal
    // is not the only way one arrives: a hand-built `settle: usize::MAX, measure: 1` overflows the
    // window. Validating after the settle loop would have been too late — the settle loop would
    // already be running `usize::MAX` iterations, which is the hang this ordering exists to prevent.
    // The check found exactly that when it was written the other way round.
    let until = probe.settle.checked_add(probe.measure)?;
    if probe.measure == 0 {
        return None;
    }

    // Bounded phase, for the same reason `stimulus::sine` uses one: `step * n` overflows for a large
    // finite `hz` late in a long settle, and `sin` of an infinity is NaN — which would be read as the
    // *system* misbehaving when it was the excitation all along.
    let increment = (probe.hz / rate).rem_euclid(1.0);
    if !increment.is_finite() {
        return None;
    }
    let mut phase = 0.0f64;
    let advance = |phase: &mut f64| {
        let drive = probe.amplitude * (TAU * *phase).sin();
        *phase = (*phase + increment).rem_euclid(1.0);
        drive
    };

    for _ in 0..probe.settle {
        let drive = advance(&mut phase);
        // A system that blows up during the settle is just as broken as one that blows up while
        // being measured; discarding the transient must not mean discarding the evidence.
        if !system(drive as f32).is_finite() {
            return None;
        }
    }
    let mut peak = 0.0f64;
    for _ in probe.settle..until {
        let drive = advance(&mut phase);
        let y = system(drive as f32);
        // `f64::max` returns the non-NaN operand, so a system that went NaN would otherwise be
        // reported as whatever its last healthy sample was — a broken filter measured as a working
        // one. Absence is the honest answer, and the caller can ask `observe` what happened.
        if !y.is_finite() {
            return None;
        }
        peak = peak.max(f64::from(y).abs());
    }
    Some(peak / probe.amplitude)
}

/// What [`transfer_gain`] excites a system with.
///
/// Every field is the caller's decision, on purpose: the amplitude decides whether a nonlinearity is
/// engaged, and the two lengths decide whether a slow resonance has settled. There are no defaults
/// here, because a default settle time is a claim about somebody else's filter.
#[derive(Clone, Copy, Debug)]
pub struct Probe {
    /// The excitation frequency, in hertz.
    pub hz: f64,
    /// The excitation amplitude. Small keeps saturators linear; large measures the drive too.
    pub amplitude: f64,
    /// Samples run and discarded before measuring, so the transient is not counted as gain.
    pub settle: usize,
    /// Samples measured. At least a few cycles of `hz`, or the peak is a sample of the waveform.
    pub measure: usize,
}

/// A sample count from a length in seconds or cycles, or **absence where it cannot be represented**.
///
/// `Option`, not a zero, because a zero is a real answer — *no settle was asked for* — and folding
/// the two together is how an unrepresentable settle became a probe that measured without settling.
///
/// Representability is the only test applied. An earlier draft also refused anything above a fixed
/// ceiling on the grounds that it was not *worth* measuring, which is a caller-independent
/// acceptance decision — precisely the policy this crate does not carry. How long a probe should be
/// is the caller's to decide and their filter's to justify; what this cannot do is pretend a length
/// that does not fit a `usize` is one that does, which is what `as usize` saturation quietly did.
fn samples(count: f64) -> Option<usize> {
    // **The limit is `2^usize::BITS`, exclusive — representability and nothing else.**
    //
    // Two earlier attempts were both wrong in the same direction, and both were policy dressed as
    // arithmetic. Comparing against `usize::MAX as f64` admits values that do not fit, because that
    // cast rounds *up* to `2^BITS`. Comparing against `2^(BITS-1)` fixed that by rejecting the whole
    // representable upper half of `usize` — a limit nobody could derive from the type. `2^BITS` is
    // exact in `f64` on 32- and 64-bit targets alike, and every `f64` strictly below it that is also
    // non-negative casts to a `usize` without saturating. Three platforms, always.
    let limit = 2f64.powi(usize::BITS as i32);
    (count.is_finite() && (0.0..limit).contains(&count)).then_some(count as usize)
}

impl Probe {
    /// A probe at `hz` with `amplitude`, settling for `settle_s` seconds and measuring `cycles`
    /// whole cycles — the shape a filter response is usually asked for in.
    ///
    /// **Exactly what was asked for, rounded up to a whole sample and no further.** A draft added a
    /// 64-sample guard on the reasoning that a truncated cycle count could stop a fraction short of a
    /// crest — which is true, and made the guard a **margin somebody chose**, silently lengthening
    /// every caller's window and able to move the measured peak. That is the policy this crate does
    /// not carry, in the one struct whose doc comment promises *"no defaults"*. Rounding up is
    /// representational: ask for eight cycles and you get at least eight cycles.
    ///
    /// A non-positive `hz` or `cycles` measures nothing, and [`transfer_gain`] then reports absence.
    pub fn new(hz: f64, amplitude: f64, settle_s: f64, cycles: f64, rate: f64) -> Self {
        // **An unrepresentable part refuses the whole probe.** Zeroing only the offending field left
        // an unrepresentable settle as `settle = 0` beside a positive `measure`, which is a
        // measurement taken without settling — a plausible number from a probe that was rejected.
        let usable =
            hz > 0.0 && cycles > 0.0 && amplitude.is_finite() && rate.is_finite() && rate > 0.0;
        let lengths = usable
            .then(|| {
                let settle = samples(settle_s * rate)?;
                // Rounded up, so a requested whole cycle is never cut short by truncation.
                let measure = samples((rate / hz * cycles).ceil())?;
                Some((settle, measure))
            })
            .flatten();
        match lengths {
            Some((settle, measure)) => Self {
                hz,
                amplitude,
                settle,
                measure,
            },
            // `measure` of zero is what `transfer_gain` reports absence on.
            None => Self {
                hz,
                amplitude,
                settle: 0,
                measure: 0,
            },
        }
    }
}

/// In-place iterative radix-2 FFT.
///
/// Written by hand because the crates this measures have no dependencies and a measurement crate is
/// not the place to start acquiring them. **One copy, used by everything** — it was two before
/// `dsp-lab` de-duplicated it, and a transform duplicated across files is a change waiting to be
/// applied to only one of them.
///
/// In-place iterative radix-2 FFT. **Absent, never a panic, on a length it cannot transform.**
///
/// Written by hand because the crates this measures have no dependencies and a measurement crate is
/// not the place to start acquiring them. **One copy, used by everything** — it was two before
/// `dsp-lab` de-duplicated it, and a transform duplicated across files is a change waiting to be
/// applied to only one of them.
///
/// Returns `None` and **leaves both buffers untouched** where the length is not a power of two or the
/// halves differ. A draft panicked here instead, on the argument that a wrong buffer length is a
/// caller's mistake in the same way slice indexing is; the argument is arguable and the behaviour was
/// not worth defending — a crate whose whole contract is *report absence rather than a bad number*
/// should not make one exception for its own convenience. An earlier version was worse still: `ifft`
/// negated the imaginary half and *then* rejected the length, corrupting a buffer behind a panic.
#[must_use = "an absent transform means the buffers were left untouched"]
pub fn fft(re: &mut [f64], im: &mut [f64]) -> Option<()> {
    let n = re.len();
    if !n.is_power_of_two() || n != im.len() {
        return None;
    }

    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let mut len = 2usize;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0usize;
        while i < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (vr, vi) = (
                    re[i + k + len / 2] * cr - im[i + k + len / 2] * ci,
                    re[i + k + len / 2] * ci + im[i + k + len / 2] * cr,
                );
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr;
                im[i + k + len / 2] = ui - vi;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
            i += len;
        }
        len <<= 1;
    }
    Some(())
}

/// Inverse FFT, by conjugation.
///
/// Same contract as [`fft`]: absent on a length it cannot transform, and **checked before the first
/// mutation** so a rejected call leaves both buffers as it found them.
#[must_use = "an absent transform means the buffers were left untouched"]
pub fn ifft(re: &mut [f64], im: &mut [f64]) -> Option<()> {
    if !re.len().is_power_of_two() || re.len() != im.len() {
        return None;
    }
    for v in im.iter_mut() {
        *v = -*v;
    }
    fft(re, im)?;
    let n = re.len() as f64;
    for v in re.iter_mut() {
        *v /= n;
    }
    for v in im.iter_mut() {
        *v = -*v / n;
    }
    Some(())
}

/// Wrap a phase difference into `-pi..pi`.
#[inline]
pub fn princarg(x: f64) -> f64 {
    (x + PI).rem_euclid(TAU) - PI
}

/// Unwanted energy over wanted energy, in decibels, for a signal that is exactly periodic with
/// `periods` cycles in the window.
///
/// The wanted set is the harmonic grid — bins at multiples of `periods`. Everything else in the
/// spectrum is unwanted. **This is the measurement whose floor must be checked with a control**:
/// [`crate::stimulus::additive_saw`] has no aliasing by construction, so what this reports for it is
/// the method's own floor rather than a property of the waveform.
///
/// Domain: the buffer's length must be a power of two and it must be exactly periodic. **Absent
/// otherwise**, because a leaky spectrum's off-grid energy is leakage rather than aliasing, and
/// reporting it as aliasing would be the wrong answer rather than an imprecise one.
///
/// Silence has no wanted energy to compare against, so it is absent too — not `-inf`, which would
/// read as "perfectly clean" and pass any aliasing assertion ever written.
pub fn alias_to_signal_db(x: &[f32], periods: usize) -> Option<f64> {
    let (wanted, unwanted) = harmonic_split(x, periods)?;
    (wanted > 0.0).then(|| 10.0 * (unwanted / wanted).log10())
}

/// The wanted and unwanted **unnormalised spectral energy** either side of the harmonic grid, or
/// absence where the buffer cannot carry the measurement.
///
/// Unnormalised on purpose: the two are only ever compared with each other, so a common factor
/// cancels, and choosing one would be picking a convention no caller needs. Bins `1..=n/2` are
/// classified — DC belongs to neither, and **Nyquist counts**.
///
/// Exposed separately because the decision about what counts as wanted *is* the experiment — a
/// detuned stack, an FM pair or a grain train each answers it differently — and this crate carries
/// the split, not the policy.
pub fn harmonic_split(x: &[f32], periods: usize) -> Option<(f64, f64)> {
    let n = x.len();
    if n == 0 || !n.is_power_of_two() || periods == 0 {
        return None;
    }
    let mut re: Vec<f64> = x.iter().map(|&s| f64::from(s)).collect();
    let mut im = vec![0.0f64; n];
    // The length is this function's own `next_power_of_two`, so the transform cannot refuse it.
    fft(&mut re, &mut im)?;

    // A non-finite sample poisons every bin; report absence rather than a split of NaNs.
    if re.iter().any(|v| !v.is_finite()) {
        return None;
    }

    let (mut wanted, mut unwanted) = (0.0f64, 0.0f64);
    // Bin 0 is DC and belongs to neither: it is not a harmonic and it is not an alias.
    //
    // **The Nyquist bin is included.** An earlier form stopped at `n/2` exclusive, so a component
    // sitting exactly on Nyquist fell out of both sides and a pure Nyquist tone measured as
    // `(0, 0)` — no wanted energy, no unwanted energy, and no ratio at all. Aliasing lands there
    // more often than anywhere else, which makes it the worst bin to drop.
    for k in 1..=n / 2 {
        let power = re[k] * re[k] + im[k] * im[k];
        if k % periods == 0 {
            wanted += power;
        } else {
            unwanted += power;
        }
    }
    Some((wanted, unwanted))
}

/// A-weighting gain (IEC 61672), applied to a magnitude rather than a power.
///
/// A crude audibility proxy, **not** the noise-to-mask ratio the literature reports. Zero and
/// negative frequencies have no weight.
pub fn a_weight(f: f64) -> f64 {
    if !f.is_finite() || f <= 0.0 {
        return 0.0;
    }
    let f2 = f * f;
    let num = 12194.0f64.powi(2) * f2 * f2;
    let den = (f2 + 20.6f64.powi(2))
        * ((f2 + 107.7f64.powi(2)) * (f2 + 737.9f64.powi(2))).sqrt()
        * (f2 + 12194.0f64.powi(2));
    // **Both sides overflow together far above the audio band** — around 1e100 Hz the numerator and
    // the denominator are each infinite and the ratio is NaN. The curve falls monotonically up
    // there, so zero is the limit and the only answer that cannot be mistaken for a weighting.
    if !num.is_finite() || !den.is_finite() || den <= 0.0 {
        return 0.0;
    }
    // +2 dB normalisation, so the curve passes through 0 dB at 1 kHz.
    (num / den) * 10f64.powf(2.0 / 20.0)
}

/// Spectral flatness: the geometric mean of the power spectrum over its arithmetic mean, in decibels,
/// across a frequency range.
///
/// Near 0 dB for noise, strongly negative for a spectrum concentrated into peaks. Note that a
/// *periodogram* of white noise measures about −2.5 dB rather than 0 — **the bias is in the
/// measurement**, so always measure a known-flat control alongside.
///
/// **No epsilon guards the logarithm, and two drafts that had one were wrong.** A single empty bin
/// makes the geometric mean exactly zero and the result negative infinity — the correct reading of
/// *one frequency is entirely absent*, and an intentional infinity rather than an error. An absolute
/// floor was a level somebody chose; a relative one still changed the answer. Silence, where every
/// bin is zero, is absence: the ratio is 0/0.
///
/// **Takes `f64`, unlike the rest of this crate**, because its callers are analysis harnesses that
/// already work in `f64` and this function's results are quoted verbatim in `docs/oscillators/`.
/// Narrowing to `f32` at the boundary would move numbers that two references cite. Use
/// Widen an `f32` buffer at the call site when the audio domain is what you have.
pub fn spectral_flatness_db(x: &[f64], rate: f64, lo_hz: f64, hi_hz: f64) -> Option<f64> {
    let n = x.len().next_power_of_two().min(N);
    if x.is_empty() || n == 0 || rate.is_nan() || rate <= 0.0 {
        return None;
    }
    // **Normalised before the transform, not after it.** A draft scaled the *powers*, which was too
    // late: squaring a sample near `1e200` overflows and one near `1e-200` underflows to zero, so a
    // scaled copy of a perfectly measurable spectrum came back as absence. Flatness is a ratio of
    // means and is invariant to a common factor on the input, so dividing by the largest magnitude
    // first costs nothing and removes both ends.
    // **The whole buffer is checked, not the analysed prefix.** Only the first `n` samples are
    // transformed, so a NaN past that point cannot change the arithmetic — and reporting a number for
    // a buffer that contains one anyway is precisely the laundering the crate's result forms exist to
    // prevent. A caller holding a part-broken render should hear about it, not get a figure for the
    // half that happened to be clean.
    if x.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let largest_sample = x.iter().take(n).fold(0.0f64, |m, v| m.max(v.abs()));
    if largest_sample <= 0.0 {
        return None;
    }
    let mut re = vec![0.0f64; n];
    let mut im = vec![0.0f64; n];
    for i in 0..n.min(x.len()) {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
        re[i] = (x[i] / largest_sample) * w;
    }
    fft(&mut re, &mut im)?;
    let lo = (lo_hz * n as f64 / rate) as usize;
    let hi = ((hi_hz * n as f64 / rate) as usize).min(n / 2);
    // **No floor at all.** Two earlier drafts guarded `ln` with an epsilon, first absolute and then
    // relative, and both were thresholds: a level somebody chose, changing the answer, in a crate
    // that carries none. The definition needs no guard. A single empty bin makes the geometric mean
    // exactly zero and the result negative infinity, which is the correct reading of *one frequency
    // is entirely absent* and which the result forms already admit as an intentional infinity.
    let mut powers = Vec::with_capacity(hi.saturating_sub(lo));
    for k in lo..hi {
        let raw = re[k] * re[k] + im[k] * im[k];
        if !raw.is_finite() {
            return None;
        }
        powers.push(raw);
    }
    // An empty band has no flatness, and **neither does silence**: with every bin at zero the ratio
    // is 0/0, and any floor would make it read exactly 0.0 dB — *perfectly flat*, the most
    // favourable answer there is, which would pass any assertion written against it.
    if powers.is_empty() {
        return None;
    }
    // **Scaled by the largest bin before anything is summed.** Flatness is a ratio of means and is
    // therefore invariant to a common factor — which is exactly what makes this safe, and what makes
    // it necessary. Individually finite powers can overflow their sum for a loud spectrum, and a very
    // quiet one can underflow the logarithm; either way the answer moves for a reason that is about
    // `f64` rather than about the signal. Dividing by the maximum puts every power in `0..=1` and
    // removes both failures without changing the ratio.
    let largest = powers.iter().copied().fold(0.0f64, f64::max);
    // Silence has no flatness: the ratio is 0/0. Absence, not a number.
    if largest <= 0.0 {
        return None;
    }
    let arith = powers.iter().map(|p| p / largest).sum::<f64>() / powers.len() as f64;
    if arith <= 0.0 {
        return None;
    }
    // `ln(0)` is negative infinity, so one empty bin carries the geometric mean to zero and the
    // result with it. That is the definition working rather than an error.
    let log_sum: f64 = powers.iter().map(|p| (p / largest).ln()).sum();
    let geo = (log_sum / powers.len() as f64).exp();
    let flatness = 10.0 * (geo / arith).log10();
    (!flatness.is_nan()).then_some(flatness)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::amplitude_db;
    use crate::{RATES, STRESS_RATES, stimulus};

    #[test]
    fn a_components_amplitude_is_the_amplitude() {
        // The closed form. A unit sine reads 1.0, which is the convention the documented form used
        // and the four player tests' half-scaled form did not.
        for rate in RATES {
            for amplitude in [0.1f32, 0.5, 1.0] {
                let n = 4096;
                let hz = stimulus::periodic_frequency(n, 1_000.0, rate);
                let x = stimulus::periodic_sine(n, hz, rate, amplitude);
                let measured = component_amplitude(&x, hz, rate).expect("a component");
                assert!(
                    (measured - f64::from(amplitude)).abs() < 1e-3,
                    "at {rate} Hz, amplitude {amplitude}: read {measured:.6}"
                );
            }
        }
    }

    #[test]
    fn dc_and_nyquist_read_their_own_amplitude_and_not_twice_it() {
        // The defect this scaling exists for: a real sinusoid between DC and Nyquist splits its
        // energy across a positive and a negative frequency, so recovering the amplitude takes a
        // factor of two — but DC and Nyquist are their own mirror and take one. With the factor
        // applied uniformly, a **unit DC offset reads 2.0**.
        let rate = 48_000.0;
        let n = 4096;

        let dc = stimulus::dc(n, 0.7);
        let measured = component_amplitude(&dc, 0.0, rate).expect("DC is a component");
        assert!(
            (measured - 0.7).abs() < 1e-6,
            "a DC offset of 0.7 measured {measured}"
        );

        // Nyquist: alternating +a, -a, which is a cosine at exactly rate/2.
        let nyquist: Vec<f32> = (0..n)
            .map(|i| if i % 2 == 0 { 0.4 } else { -0.4 })
            .collect();
        let measured = component_amplitude(&nyquist, rate / 2.0, rate).expect("Nyquist");
        assert!(
            (measured - 0.4).abs() < 1e-6,
            "a Nyquist component of 0.4 measured {measured}"
        );

        // And an ordinary tone still takes the factor of two.
        let tone = stimulus::periodic_sine(n, 1_000.0, rate, 0.4);
        let hz = stimulus::periodic_frequency(n, 1_000.0, rate);
        let measured = component_amplitude(&tone, hz, rate).expect("a tone");
        assert!(
            (measured - 0.4).abs() < 1e-3,
            "an ordinary tone of 0.4 measured {measured}"
        );
    }

    #[test]
    fn an_excitation_that_cannot_survive_the_cast_is_refused() {
        // The system under test takes `f32`. An amplitude below `f32`'s smallest positive value
        // arrives as silence, so a **working** system produced a peak of zero and the gain came back
        // as 0.0 — a perfect filter measured as a dead one.
        let rate = 48_000.0;
        for amplitude in [f64::MIN_POSITIVE, 1e-60, 1e40] {
            let probe = Probe {
                hz: 440.0,
                amplitude,
                settle: 16,
                measure: 256,
            };
            assert_eq!(
                transfer_gain(probe, rate, |x| x),
                None,
                "an amplitude of {amplitude:e} does not survive the f32 cast and must be refused"
            );
        }
        // One that does survive still measures unity through a wire.
        let ok = Probe::new(440.0, 1e-3, 0.01, 8.0, rate);
        let gain = transfer_gain(ok, rate, |x| x).expect("a representable probe");
        assert!((gain - 1.0).abs() < 1e-3, "a wire measured {gain}");
    }

    #[test]
    fn the_endpoint_scaling_is_decided_exactly_and_not_by_a_tolerance() {
        // A draft used `1e-12` to decide whether a frequency counted as DC or Nyquist, which is a
        // chosen level deciding an answer — and deciding it by a factor of two.
        let rate = 48_000.0;
        let n = 4096;
        let dc = stimulus::dc(n, 0.5);

        // Exactly DC takes the endpoint scaling. 0.5 is exact in both formats, so this one can be
        // compared against the literal.
        let at = component_amplitude(&dc, 0.0, rate).expect("DC");
        assert!((at - 0.5).abs() < 1e-9, "exactly DC measured {at}");

        // A frequency one bin away is an ordinary component and takes the ordinary scaling. It is a
        // different answer, and that discontinuity is a property of a real DFT rather than of us.
        let one_bin = rate / n as f64;
        let near = component_amplitude(&dc, one_bin, rate).expect("a neighbouring bin");
        assert!(
            near < at,
            "a neighbouring bin should not read as loud as DC itself"
        );

        // Exactly Nyquist, likewise — compared against the `f32` value **widened**, not against the
        // literal. `0.3f32` is 0.30000001192… in `f64`, and a tolerance tighter than `f32` can
        // represent would be measuring the storage format rather than the function.
        let amplitude = 0.3f32;
        let nyquist: Vec<f32> = (0..n)
            .map(|i| if i % 2 == 0 { amplitude } else { -amplitude })
            .collect();
        let at = component_amplitude(&nyquist, rate / 2.0, rate).expect("Nyquist");
        assert!(
            (at - f64::from(amplitude)).abs() < 1e-9,
            "exactly Nyquist measured {at}"
        );
    }

    #[test]
    fn flatness_survives_input_amplitudes_that_overflow_or_underflow_when_squared() {
        // Scaling the *powers* was too late: squaring a sample near 1e200 overflows and one near
        // 1e-200 underflows to zero, so a scaled copy of a measurable spectrum came back absent.
        let rate = 48_000.0;
        let base: Vec<f64> = stimulus::noise(1 << 12, 31, 1.0)
            .iter()
            .map(|&s| f64::from(s))
            .collect();
        let reference = spectral_flatness_db(&base, rate, 100.0, 18_000.0).expect("flatness");

        for scale in [1e-200f64, 1e-100, 1e100, 1e200] {
            let scaled: Vec<f64> = base.iter().map(|v| v * scale).collect();
            let measured = spectral_flatness_db(&scaled, rate, 100.0, 18_000.0)
                .unwrap_or_else(|| panic!("scaling by {scale:e} lost the measurement"));
            assert!(
                (measured - reference).abs() < 1e-6,
                "scaling by {scale:e} moved flatness from {reference:.6} to {measured:.6}"
            );
        }
    }

    #[test]
    fn a_pure_nyquist_component_is_classified_rather_than_dropped() {
        // The Nyquist bin used to fall outside both halves of the split, so a tone sitting exactly
        // on it produced `(0, 0)` — no wanted energy, no unwanted, no ratio. Aliasing lands there
        // more often than anywhere else.
        let n = 1024;
        let periods = 3;

        // On its own: the energy must land somewhere. It has no fundamental to be a ratio against,
        // so `alias_to_signal_db` is rightly absent — but the split must still see it.
        let nyquist: Vec<f32> = (0..n)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let (wanted, unwanted) =
            harmonic_split(&nyquist, periods).expect("a power-of-two buffer splits");
        assert!(
            wanted + unwanted > 0.0,
            "a pure Nyquist tone was classified as neither wanted nor unwanted"
        );
        assert_eq!(
            wanted, 0.0,
            "Nyquist is not a multiple of three, so none of it is wanted"
        );

        // Beside a fundamental it is what it always is — **aliasing** — and must raise the unwanted
        // side. Under the old exclusive range it raised nothing at all.
        let clean = stimulus::additive_saw(n, periods);
        let dirty: Vec<f32> = clean
            .iter()
            .enumerate()
            .map(|(i, &s)| s + if i % 2 == 0 { 0.2 } else { -0.2 })
            .collect();
        let clean_db = alias_to_signal_db(&clean, periods).expect("a clean control reads");
        let dirty_db =
            alias_to_signal_db(&dirty, periods).expect("with Nyquist added it still reads");
        assert!(
            dirty_db > clean_db + 20.0,
            "adding a Nyquist component did not raise the alias measure:              {clean_db:.1} dB to {dirty_db:.1} dB"
        );
    }

    #[test]
    fn flatness_survives_amplitudes_that_would_overflow_or_underflow_the_sum() {
        // Individually finite powers can overflow their sum, and a very quiet spectrum can underflow
        // the logarithm; either way the answer would move for a reason about `f64` rather than about
        // the signal. Scaling by the largest bin removes both, and the figure must not move.
        let rate = 48_000.0;
        let base: Vec<f64> = stimulus::noise(1 << 12, 23, 1.0)
            .iter()
            .map(|&s| f64::from(s))
            .collect();
        let reference = spectral_flatness_db(&base, rate, 100.0, 18_000.0).expect("flatness");

        for scale in [1e-150f64, 1e-30, 1e30, 1e150] {
            let scaled: Vec<f64> = base.iter().map(|v| v * scale).collect();
            let measured = spectral_flatness_db(&scaled, rate, 100.0, 18_000.0)
                .unwrap_or_else(|| panic!("scaling by {scale:e} lost the measurement entirely"));
            assert!(
                (measured - reference).abs() < 1e-6,
                "scaling by {scale:e} moved flatness from {reference:.6} to {measured:.6}"
            );
        }
    }

    #[test]
    fn a_transform_it_cannot_do_is_absent_and_leaves_both_buffers_untouched() {
        // Two properties at once. **Absence, not a panic** — a crate whose contract is *report
        // absence rather than a bad number* should not make one exception for its own transform.
        // **And untouched** — an earlier `ifft` negated the imaginary half and *then* rejected the
        // length, corrupting the caller's buffer behind a failure they might have caught.
        for (re_len, im_len) in [(3usize, 3usize), (8, 7), (6, 6)] {
            let mut re: Vec<f64> = (0..re_len).map(|i| i as f64 + 1.0).collect();
            let mut im: Vec<f64> = (0..im_len).map(|i| i as f64 + 10.0).collect();
            let (re_before, im_before) = (re.clone(), im.clone());

            assert_eq!(
                fft(&mut re, &mut im),
                None,
                "a {re_len}/{im_len} transform should be absent"
            );
            assert_eq!(re, re_before, "fft modified the real half before refusing");
            assert_eq!(
                im, im_before,
                "fft modified the imaginary half before refusing"
            );

            assert_eq!(ifft(&mut re, &mut im), None);
            assert_eq!(re, re_before, "ifft modified the real half before refusing");
            assert_eq!(
                im, im_before,
                "ifft modified the imaginary half before refusing"
            );
        }

        // A length it *can* do still works, and reports that it did: an impulse transforms to a flat
        // unit spectrum.
        let mut re = vec![1.0f64, 0.0, 0.0, 0.0];
        let mut im = vec![0.0f64; 4];
        assert_eq!(fft(&mut re, &mut im), Some(()));
        assert!(re.iter().all(|v| (v - 1.0).abs() < 1e-12));
    }

    #[test]
    fn a_component_that_is_not_there_reads_far_below_one_that_is() {
        let rate = 48_000.0;
        let n = 4096;
        let hz = stimulus::periodic_frequency(n, 1_000.0, rate);
        let x = stimulus::periodic_sine(n, hz, rate, 0.5);

        let present = amplitude_db(component_amplitude(&x, hz, rate).expect("present"));
        // A different grid frequency, so the probe is not reading leakage from the one that is there.
        let absent_hz = stimulus::periodic_frequency(n, 3_000.0, rate);
        let absent = amplitude_db(component_amplitude(&x, absent_hz, rate).expect("absent"));
        assert!(
            absent < present - 60.0,
            "absent {absent:.1} dB against present {present:.1} dB"
        );
    }

    #[test]
    fn the_half_scaled_convention_is_six_decibels_below_this_one() {
        // Written down because figures measured with the old form exist in this repository and a
        // reader converting one needs the factor stated rather than rediscovered.
        let rate = 48_000.0;
        let n = 4096;
        let hz = stimulus::periodic_frequency(n, 1_000.0, rate);
        let x = stimulus::periodic_sine(n, hz, rate, 1.0);

        let ours = component_amplitude(&x, hz, rate).expect("a component");
        let old_form = ours / 2.0;
        assert!((amplitude_db(ours) - amplitude_db(old_form) - 6.020_6).abs() < 1e-3);
    }

    #[test]
    fn a_first_order_lowpass_measures_minus_three_decibels_at_its_corner() {
        // The closed form `transfer_gain` is scored against: a one-pole at its own cutoff passes
        // 1/sqrt(2). Not a shipped filter — a textbook one written here, because a measurement scored
        // against the code under test measures nothing.
        for rate in RATES {
            let cutoff = 1_000.0;
            let mut state = 0.0f64;
            let coefficient = 1.0 - (-TAU * cutoff / rate).exp();
            let probe = Probe::new(cutoff, 1e-3, 0.5, 8.0, rate);
            let gain = transfer_gain(probe, rate, |x| {
                state += coefficient * (f64::from(x) - state);
                state as f32
            })
            .expect("a gain");

            let want = 1.0 / 2f64.sqrt();
            assert!(
                (amplitude_db(gain) - amplitude_db(want)).abs() < 0.35,
                "at {rate} Hz: {:.3} dB against {:.3} dB",
                amplitude_db(gain),
                amplitude_db(want)
            );
        }
    }

    #[test]
    fn a_unity_path_has_unity_transfer_gain() {
        let probe = Probe::new(440.0, 0.1, 0.05, 8.0, 48_000.0);
        let gain = transfer_gain(probe, 48_000.0, |x| x).expect("a gain");
        assert!((gain - 1.0).abs() < 1e-3, "a wire measured {gain}");
    }

    #[test]
    fn an_impossible_probe_is_absent_rather_than_zero() {
        let rate = 48_000.0;
        let zero_hz = Probe::new(0.0, 0.1, 0.01, 8.0, rate);
        assert_eq!(transfer_gain(zero_hz, rate, |x| x), None);

        let silent_drive = Probe {
            hz: 440.0,
            amplitude: 0.0,
            settle: 16,
            measure: 16,
        };
        assert_eq!(
            transfer_gain(silent_drive, rate, |x| x),
            None,
            "dividing by a silent excitation is not a gain"
        );

        let nothing_measured = Probe {
            hz: 440.0,
            amplitude: 0.1,
            settle: 16,
            measure: 0,
        };
        assert_eq!(transfer_gain(nothing_measured, rate, |x| x), None);
    }

    #[test]
    fn the_transform_round_trips() {
        let x = stimulus::noise(1024, 99, 0.5);
        let mut re: Vec<f64> = x.iter().map(|&s| f64::from(s)).collect();
        let mut im = vec![0.0f64; re.len()];
        let original = re.clone();
        fft(&mut re, &mut im).expect("a power-of-two length transforms");
        ifft(&mut re, &mut im).expect("a power-of-two length transforms");
        for (a, b) in original.iter().zip(re.iter()) {
            assert!((a - b).abs() < 1e-10, "{a} came back as {b}");
        }
    }

    #[test]
    fn each_transform_matches_a_closed_form_on_its_own() {
        // A round trip alone proves only that the pair are inverses — two mutually compatible
        // defects pass it. Each direction is checked here against an answer written down in advance.
        let n = 64;

        // Forward: an impulse at sample zero has every bin equal to 1.
        let mut re = vec![0.0f64; n];
        let mut im = vec![0.0f64; n];
        re[0] = 1.0;
        fft(&mut re, &mut im).expect("a power-of-two length transforms");
        for k in 0..n {
            assert!(
                (re[k] - 1.0).abs() < 1e-12 && im[k].abs() < 1e-12,
                "an impulse's bin {k} is {} + {}i, not 1 + 0i",
                re[k],
                im[k]
            );
        }

        // Forward: a cosine at bin 3 puts n/2 in bins 3 and n-3 and nothing anywhere else.
        let mut re: Vec<f64> = (0..n)
            .map(|i| (TAU * 3.0 * i as f64 / n as f64).cos())
            .collect();
        let mut im = vec![0.0f64; n];
        fft(&mut re, &mut im).expect("a power-of-two length transforms");
        for k in 0..n {
            let want = if k == 3 || k == n - 3 {
                n as f64 / 2.0
            } else {
                0.0
            };
            assert!(
                (re[k].hypot(im[k]) - want).abs() < 1e-9,
                "bin {k} has magnitude {}, want {want}",
                re[k].hypot(im[k])
            );
        }

        // Inverse: a flat unit spectrum is an impulse of height 1 at sample zero.
        let mut re = vec![1.0f64; n];
        let mut im = vec![0.0f64; n];
        ifft(&mut re, &mut im).expect("a power-of-two length transforms");
        assert!((re[0] - 1.0).abs() < 1e-12, "ifft gave {} at zero", re[0]);
        for (i, value) in re.iter().enumerate().skip(1) {
            assert!(value.abs() < 1e-12, "ifft left {value} at sample {i}");
        }
    }

    #[test]
    fn periods_for_is_odd_and_never_zero() {
        for rate in STRESS_RATES {
            for hz in [1.0f64, 55.0, 440.0, 19_000.0] {
                let p = periods_for(hz, rate, 4096);
                assert!(p >= 1, "{hz} at {rate} gave {p} periods");
                assert_eq!(p % 2, 1, "{hz} at {rate} gave an even {p}");
            }
        }
        // A frequency that rounds below one period still yields a usable window.
        assert_eq!(periods_for(0.0, 48_000.0, 4096), 1);
    }

    #[test]
    fn princarg_wraps_to_the_right_value_and_not_merely_into_range() {
        // A range check alone passes an implementation that returns zero for everything, which is
        // why the values are asserted rather than the interval.
        for (x, want) in [
            (0.0f64, 0.0f64),
            (0.5, 0.5),
            (-0.5, -0.5),
            (TAU, 0.0),
            (TAU + 1.0, 1.0),
            (-TAU - 1.0, -1.0),
            (3.0 * TAU + 2.0, 2.0),
        ] {
            let w = princarg(x);
            assert!((w - want).abs() < 1e-9, "princarg({x}) = {w}, want {want}");
        }
        // And the range, which is the other half of the contract.
        for x in [-100.0f64, -PI, PI, 100.0] {
            let w = princarg(x);
            assert!(w > -PI - 1e-9 && w <= PI + 1e-9, "{x} wrapped to {w}");
        }
        // A wrapped angle differs from the original by a whole number of turns.
        for x in [7.3f64, -19.1, 41.0] {
            let turns = (x - princarg(x)) / TAU;
            assert!(
                (turns - turns.round()).abs() < 1e-9,
                "{x} moved by {turns} turns"
            );
        }
    }

    #[test]
    fn a_weighting_passes_through_zero_at_a_kilohertz() {
        assert!(amplitude_db(a_weight(1_000.0)).abs() < 0.05);
        // And rolls off hard at the bottom, which is the whole point of the curve.
        assert!(amplitude_db(a_weight(50.0)) < -25.0);
        assert_eq!(a_weight(0.0), 0.0);
        assert_eq!(a_weight(-1.0), 0.0);
    }

    #[test]
    fn flatness_separates_noise_from_a_tone() {
        let rate = 48_000.0;
        let widen = |x: Vec<f32>| -> Vec<f64> { x.iter().map(|&s| f64::from(s)).collect() };
        let noise = widen(stimulus::noise(1 << 13, 4, 0.5));
        let tone = widen(stimulus::periodic_sine(1 << 13, 1_000.0, rate, 0.5));
        let flat = spectral_flatness_db(&noise, rate, 100.0, 18_000.0).expect("noise has flatness");
        let peaky =
            spectral_flatness_db(&tone, rate, 100.0, 18_000.0).expect("a tone has flatness");
        assert!(
            flat > peaky + 20.0,
            "noise {flat:.1} dB against a tone {peaky:.1} dB"
        );
        // The documented periodogram bias: noise measures near -2.5 dB, not 0.
        assert!(flat < 0.0 && flat > -8.0, "noise measured {flat:.1} dB");
    }

    #[test]
    fn a_weight_survives_the_overflow_boundary_it_used_to_return_nan_at() {
        // Both sides of the ratio overflow together far above the audio band, and the naive form
        // returns inf/inf = NaN. Guarding only the squared frequency was not enough, which is how
        // this survived one repair.
        for f in [1e30f64, 1e77, 1e100, 1e200, f64::MAX] {
            let w = a_weight(f);
            assert!(!w.is_nan(), "a_weight({f:e}) is NaN");
            assert!(w >= 0.0, "a_weight({f:e}) is negative");
        }
        // And the curve is still the curve where it matters.
        assert!(amplitude_db(a_weight(1_000.0)).abs() < 0.05);
    }

    #[test]
    fn an_unrepresentable_probe_is_refused_whole_rather_than_in_part() {
        // Saturating a length to `usize::MAX` trades an overflow for a loop nobody interrupts, and
        // zeroing only the offending field is worse still: an unrepresentable *settle* beside a
        // positive *measure* is a measurement taken without settling, which looks like a result.
        //
        // **Every case asserts the whole probe is unusable.** A first version accepted
        // `measure == 0 || settle == 0`, which passes trivially whenever the case asked for no
        // settle — an assertion that could not fail for the reason it was written.
        let rate = 48_000.0;
        let unusable: [(&str, Probe); 10] = [
            (
                "a cycle longer than any buffer",
                Probe::new(f64::MIN_POSITIVE, 0.1, 0.5, 1.0, rate),
            ),
            (
                "boundless cycles",
                Probe::new(1.0, 0.1, 0.5, f64::MAX, rate),
            ),
            (
                "a subnormal frequency",
                Probe::new(1e-300, 0.1, 0.5, 8.0, rate),
            ),
            (
                "a settle past any buffer",
                Probe::new(440.0, 0.1, f64::MAX, 8.0, rate),
            ),
            (
                "an infinite settle",
                Probe::new(440.0, 0.1, f64::INFINITY, 8.0, rate),
            ),
            ("a NaN frequency", Probe::new(f64::NAN, 0.1, 0.5, 8.0, rate)),
            ("a NaN settle", Probe::new(440.0, 0.1, f64::NAN, 8.0, rate)),
            ("NaN cycles", Probe::new(440.0, 0.1, 0.5, f64::NAN, rate)),
            ("a NaN rate", Probe::new(440.0, 0.1, 0.5, 8.0, f64::NAN)),
            (
                "a NaN amplitude",
                Probe::new(440.0, f64::NAN, 0.5, 8.0, rate),
            ),
        ];
        for (name, probe) in unusable {
            assert_eq!(
                probe.measure, 0,
                "{name}: the probe still measures {probe:?}"
            );
            assert_eq!(probe.settle, 0, "{name}: the probe still settles {probe:?}");
            assert_eq!(
                transfer_gain(probe, rate, |x| x),
                None,
                "{name}: an unusable probe returned a gain"
            );
        }

        // **A probe built by hand, which `Probe::new` never sees.** Every field is public, so the
        // constructor's refusal is not the only way one arrives.
        let overflowing = Probe {
            hz: 440.0,
            amplitude: 0.1,
            settle: usize::MAX,
            measure: 1,
        };
        assert_eq!(
            transfer_gain(overflowing, rate, |x| x),
            None,
            "a probe whose window overflows must report absence, not a gain over an empty range"
        );

        // An ordinary probe is untouched by any of that, and its two lengths are exact.
        let ordinary = Probe::new(440.0, 0.1, 0.5, 8.0, rate);
        assert_eq!(ordinary.settle, 24_000);
        // Exactly the eight cycles asked for, rounded up to a whole sample and no further: no
        // margin is added, because a margin is a decision and this struct promises none.
        assert_eq!(ordinary.measure, (rate / 440.0 * 8.0).ceil() as usize);
        assert!(transfer_gain(ordinary, rate, |x| x).is_some());
    }

    #[test]
    fn flatness_is_scale_invariant_which_an_absolute_floor_would_break() {
        // The property that makes the guard a definition rather than a threshold: halving a signal
        // must not change how flat it measures. An absolute floor fails this — a quiet spectrum sits
        // on it and reads flatter than a loud copy of itself.
        let rate = 48_000.0;
        let loud: Vec<f64> = stimulus::noise(1 << 12, 11, 1.0)
            .iter()
            .map(|&s| f64::from(s))
            .collect();
        let reference = spectral_flatness_db(&loud, rate, 100.0, 18_000.0).expect("flatness");
        for scale in [1e-3f64, 1e-6, 1e-9, 1e3] {
            let scaled: Vec<f64> = loud.iter().map(|v| v * scale).collect();
            let measured = spectral_flatness_db(&scaled, rate, 100.0, 18_000.0).expect("flatness");
            assert!(
                (measured - reference).abs() < 1e-9,
                "scaling by {scale} moved flatness from {reference:.6} to {measured:.6}"
            );
        }
    }

    #[test]
    fn silence_has_no_flatness_rather_than_a_perfect_one() {
        // 0.0 dB reads as *perfectly flat* and would pass any assertion written against it.
        let rate = 48_000.0;
        assert_eq!(
            spectral_flatness_db(&vec![0.0f64; 4096], rate, 100.0, 18_000.0),
            None
        );
        assert_eq!(spectral_flatness_db(&[], rate, 100.0, 18_000.0), None);
        // And a band with no bins in it.
        let tone: Vec<f64> = stimulus::periodic_sine(4096, 1_000.0, rate, 0.5)
            .iter()
            .map(|&s| f64::from(s))
            .collect();
        assert_eq!(spectral_flatness_db(&tone, rate, 5_000.0, 5_000.1), None);
    }

    #[test]
    fn a_buffer_that_cannot_carry_the_split_is_absent() {
        // Not zero, and not a number: an off-grid or non-power-of-two buffer's off-harmonic energy is
        // leakage rather than aliasing, and reporting it as aliasing is the wrong answer.
        assert_eq!(harmonic_split(&[], 3), None);
        assert_eq!(harmonic_split(&vec![0.0; 1000], 3), None);
        assert_eq!(harmonic_split(&vec![0.0; 1024], 0), None);
    }

    #[test]
    fn silence_has_no_aliasing_ratio_and_must_not_report_one() {
        // -inf here would read as "perfectly clean" and pass every aliasing assertion ever written,
        // which is the exact shape of the result-form defect this contract exists to prevent.
        assert_eq!(alias_to_signal_db(&stimulus::silence(1024), 5), None);
    }

    #[test]
    fn never_nan_at_every_stress_rate() {
        for rate in STRESS_RATES {
            let x = stimulus::periodic_sine(1024, 440.0, rate, 0.5);
            let a = component_amplitude(&x, 440.0, rate).expect("a component");
            assert!(!a.is_nan(), "component went NaN at {rate}");
            let widened: Vec<f64> = x.iter().map(|&s| f64::from(s)).collect();
            assert!(
                spectral_flatness_db(&widened, rate, 20.0, rate / 2.0).is_none_or(|v| !v.is_nan()),
                "flatness went NaN at {rate}"
            );
            let gain = transfer_gain(Probe::new(440.0, 0.1, 0.001, 4.0, rate), rate, |s| s);
            if let Some(g) = gain {
                assert!(!g.is_nan(), "transfer gain went NaN at {rate}");
            }
        }
    }
}
