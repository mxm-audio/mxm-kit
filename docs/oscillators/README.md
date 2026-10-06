# Synthesizer oscillator design — a working reference

Research notes for building alias-free, musical, real-time oscillators for CLAP/VST plugins.
Written for this repo (Rust, `crates/<plugin>-dsp`, realtime rules from
[`../../AGENTS.md`](../../AGENTS.md)), but the theory is framework-agnostic.

The through-line: **an oscillator is a bandwidth problem wearing a waveform costume.** Every
classic synth waveform is defined by a discontinuity — in the signal for saw and pulse, in its
first derivative for triangle — and a discontinuity has infinite bandwidth. There is no sample
rate at which sampling one is correct. So every oscillator algorithm in this document is really
answering the same question: *how much of that infinite bandwidth do I fake, how much do I throw
away, and what does each choice cost?*

The companion volume for the rest of the voice is [`../filters/`](../filters/README.md).

## Read in this order

| File | What it covers |
|---|---|
| [01-fundamentals.md](01-fundamentals.md) | Phasors, the harmonic series, exactly how trivial sampling aliases, where the aliases land, audibility and masking, and the measurement method used throughout |
| [02-antialiasing.md](02-antialiasing.md) | The algorithm families — additive, wavetable/mipmap, BLIT, BLEP/minBLEP, PolyBLEP orders, DPW, PTR/EPTR, oversampling — with the measured comparison table |
| [03-waveshapes.md](03-waveshapes.md) | Per-waveform practice: sawtooth, pulse and PWM, the two-edge problem, the width clamp, triangle and BLAMP, sub-oscillators, noise, hard and soft sync |
| [04-analog-character.md](04-analog-character.md) | What separates a real VCO from a mathematical sawtooth: finite reset time, comparator delay, drift and tuning, DCO versus VCO, and which of it is worth modelling |
| `research:oscillators/05-machines.md` (research repository) | The oscillators worth studying: SH-101's CEM3340 and its flip-flop sub, the Juno DCO, the TB-303 saw core, the Moog reset, and what to steal from each |
| [06-testing.md](06-testing.md) | Turning all of the above into `cargo test`: tuning, aliasing, DC, boundedness, and the traps that make an aliasing measurement lie |
| [07-rust-recipes.md](07-rust-recipes.md) | The concrete verdict for mxm-mono-01 — what the shipped code does well, what it costs, what to change, and what is still open |
| [08-sources.md](08-sources.md) | Annotated bibliography: what each paper is actually good for |

Then the deep dives, each on a family the chapters above only sketch:

| File | What it covers |
|---|---|
| [09-wavetable.md](09-wavetable.md) | Wavetable oscillators taken apart: interpolation order against table length, how many tables per octave, why mip crossfading is a bad trade, phase-locking, and what the memory traffic really costs |
| [10-granular.md](10-granular.md) | Granular as a tone generator — pulsar, FOF and VOSIM: grain envelopes, why fractional onsets are worth 71 dB, constant-overlap-add, cloud statistics, voice pools, and what a grain costs when it reads a sample rather than a sine |
| [11-additive-resynthesis.md](11-additive-resynthesis.md) | Additive banks and resynthesis: three ways to make a sine and what each costs, why a parametric representation stretches without artefacts, and a reading of **FL Studio's Harmor** |
| [12-fm.md](12-fm.md) | FM and phase modulation: the oscillator whose bandwidth is a knob, why everyone implements PM instead, why its aliasing is a cliff rather than a slope, feedback as a sawtooth generator, and the one place oversampling is the right answer |
| [13-phase-distortion.md](13-phase-distortion.md) | Casio's PD: brightness from bending the phase rather than from a discontinuity, why a naive PD sawtooth beats a corrected trivial one, and the CZ's resonant waveforms as granular formant synthesis |
| [14-samplers.md](14-samplers.md) | Samplers: companding converters, why the vintage grit is a missing anti-alias filter, what zero-order-hold reconstruction leaves behind, the transposition problem no interpolator solves — and a reading of **TAL-Sampler** |
| [15-granular-in-the-wild.md](15-granular-in-the-wild.md) | What the field built: GR-1/GR-MEGA, Clouds, Granulator III, Pigments, Padshop 2 and the rest — the converged parameter set, the five-thousand-to-one spread in grain budget, and what our measurements say about their feature lists |
| [16-supersaw-and-unison.md](16-supersaw-and-unison.md) | Stacking detuned voices: why unison does not add aliasing, what it does instead, where detune stops helping, and the 4.6 dB that start phase is worth |
| [17-waveshaping-and-folding.md](17-waveshaping-and-folding.md) | Bending a sine into a spectrum: why folding aliases worse than a trivial sawtooth, what ADAA is actually worth, and why a sine folder is FM in disguise |

And one deliberately per-machine appendix, in the research repository:

| File | What it covers |
|---|---|
| `research:oscillators/buchla-208-oscillators.md` | The original 208's three-card, two-core complex oscillator, its optical AM/FM subsystem, low-rate modulation-oscillator wart, revision conflict, and an evidence-ranked implementation route |

It is the exception to the usual one-screen entries in chapter 5 because the dated circuit is not
one ordinary VCO plus shaping: its two tracking cores and Card 5/7 optical interactions need
build-from depth and cannot be separated cleanly into the generic FM and waveshaping chapters.

## Status of the numbers in here

Every **in-repo DSP measurement** in these documents comes from one program,
[`crates/dsp-lab/examples/osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs),
which measures the **shipped** mxm-mono-01 oscillator code through its real public API alongside eight
alternative algorithms implemented in the same file. A hardware figure measured by an external
source is instead labelled with that person, unit and conditions; it is not an in-repo result:

```bash
cargo run -p dsp-lab --release --example osc_spike    # in mxm-tools, since the split
```

*Since the split (2026-10-06):* `crates/dsp-lab/…` is in [mxm-tools](https://github.com/mxm-audio/mxm-tools) and the shipped
`crates/mxm-mono-01-dsp/…` in [mxm-mono-01](https://github.com/mxm-audio/mxm-mono-01).

Measurement conditions, so the numbers can be reproduced or disputed:

- **Machine, and there are now two.** Everything except the granular cost tables is the **AMD Ryzen
  Threadripper 3970X**, 3.7 GHz base, Windows 11, `rustc` 1.98.0, release profile, recorded verbatim
  in `measurements-run.txt`. [10-granular.md §10.6.1 and §10.6.2](10-granular.md#106-what-granular-costs)
  are the **Intel i9-13900K**, recorded in `measurements-run-13900k.txt`, because they were measured
  later on the current development machine. The second machine runs the cost rows roughly **2x
  faster** — `trivial + 8x OS` is 549 ns against 1745, the wavetable 4.07 against 8.46 — so the two
  files' absolute nanoseconds are not comparable and **every cost figure quoted from the first
  machine is stale in absolute terms until a full re-measurement pass**. Nothing quoted in this
  directory mixes them within one comparison. Timing figures are single-machine and
  single-compiler; treat the *ratios* as the result and the absolute nanoseconds as incidental.
- **Timing figures are the mean of five runs.** The spectra are bit-identical between runs; the cost
  rows are not. Run-to-run spread is under 3% for every per-sample algorithm and under 1% for the
  oversampled ones, but **52% for the wavetable**, whose 176 kB of tables meet the cache differently
  each time. One run cannot tell 1.58 ns from 1.70 ns apart, and this reference does not pretend
  otherwise. Cost figures also drift a few percent between sessions; they are quoted to two
  decimals because that is what the harness prints, not because the last digit means anything.
- **Spectra:** 65536-point FFT, fundamentals chosen so the waveform is exactly periodic in the
  window, so no window function is needed and alias energy separates *exactly* rather than by
  estimate. The floor of the method is about −230 dB, verified by measuring an additively
  synthesised sawtooth, which the analysis correctly reports as having no aliasing.
- **What was not measured here:** this repository recorded no hardware. Claims about a real
  synthesizer's circuit come from a service manual, datasheet, or attributed specialist source, and
  `research:oscillators/05-machines.md` says which. The 208 appendix includes Dave Brown's measurements
  from one restored original and labels them as his, not ours. Aliasing under *modulation* — a fast
  pitch sweep or audio-rate PWM — is not measured either; the exactly-periodic method requires a
  steady tone. That gap is recorded in [07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

**[MEASUREMENTS.md](MEASUREMENTS.md) indexes every in-repo DSP measurement** — which section of the
harness produced it, which chapter quotes it, what each metric can and cannot say, and what has never been
measured at all. The verbatim output of the run these documents quote is in
[`measurements-run.txt`](measurements-run.txt), so a fresh run can be diffed rather than eyeballed.

## What this reference deliberately excludes

Sixteen chapters in, absence and exclusion have become impossible to tell apart from the outside.
So, explicitly:

- **Physical modelling** — Karplus-Strong, digital waveguides, modal synthesis, mass-spring
  networks. These are *excited resonators*: the sound comes from a structure responding to an
  input, not from a function of phase. Almost nothing in this reference applies to them — there is
  no waveform to band-limit and no discontinuity to correct — and covering them properly would be a
  different volume, not a longer chapter here.
- **Effects processing** — reverb, delay, chorus, distortion as an insert. The one exception is
  [chapter 17](17-waveshaping-and-folding.md), which covers waveshaping because it is also a
  *synthesis* technique and because its antialiasing story is the counter-example to chapter 2's.
- **Filters**, which have their own volume: [`../filters/`](../filters/README.md).
- **Voice architecture** — allocation, stealing, glide, note priority. It is real DSP and it is not
  oscillator design.

Anything else missing is missing by oversight, and worth raising.

## The things worth knowing up front

1. **The trivial sawtooth is not a little bit aliased, it is catastrophically aliased.** Measured:
   −28 dB alias-to-signal at 55 Hz, −6.8 dB at 7 kHz. At the top of a keyboard there is more
   aliased energy than signal.

2. **Correcting two samples per discontinuity buys ~16 dB, and that is the whole trick.** The
   shipped two-point PolyBLEP measures −44.2 dB at 55 Hz against the trivial waveform's −28.1 dB,
   for a measured cost of 1.58 ns/sample against 0.54 ns.

3. **Every alias-reduction method pays in high-frequency droop, and nobody advertises it.** The
   shipped PolyBLEP is more than 3 dB down above ~16 kHz; the four-point version, which suppresses
   10 dB more aliasing, is 3 dB down from ~12 kHz. That trade is the actual design decision, not
   the alias number alone.

4. **Oversampling is the worst deal on the table for an oscillator.** Measured: 8× oversampling
   with a 513-tap decimator reaches −47.6 dB — 6 dB *worse* than a four-point PolyBLEP — at 3170×
   the cost. Aliasing you never generate is always cheaper than aliasing you filter away.

5. **Wavetables win on aliasing by 40 dB and lose on everything else.** Measured −62.7 to −123 dB,
   at 176 kB of tables, 42 ms of startup, and — with naive one-table-per-octave selection — up to
   an octave of missing top harmonics.

6. **Both edges of a pulse need correcting, and they collide at high pitch.** A pulse is two
   sawtooth discontinuities; when the pulse gets narrower than the correction is wide, the two
   corrections overlap and produce nonsense. Our `clamp_pulse_width` handles it, and the measured
   consequence is that PWM range collapses to a square by 16 kHz.

7. **Run the sub-oscillator from its own phasor, not a flip-flop.** A divider reproduces the
   hardware's topology and its aliasing. An independent band-limited phasor at exactly half the
   increment costs the same and measures −43 dB instead.

8. **The perceptual literature and an energy metric disagree, and the literature is right.** By
   total energy, a two-point PolyBLEP beats DPW2 by 6 dB. By noise-to-mask ratio, it stays
   inaudible to a fundamental of 2135 Hz against DPW2's ~900 Hz. Aliasing is judged by where it
   lands, not how much of it there is.

9. **A real VCO's sawtooth is not a straight line.** Measurements of a Moog oscillator show the
   rising ramp closer to a quarter sine than to a line, and the reset taking measurable time.
   That, not the filter, is a large part of why hardware sounds different from a textbook ramp.

10. **Free-run the phase. Never reset it on note-on.** An analog VCO does not restart, resetting is
    an audible click in legato playing, and the only thing phase reset buys is a repeatability
    nobody asked for.

## And from the deep dives

11. **A wavetable has two quality knobs and they are independent.** Interpolation order sets the
    alias floor — 12, 24 or 36 dB per quadrupling of table length for drop-sample, linear and
    cubic. Bank resolution sets how much treble is missing. Two tables per octave is the whole
    answer; four and twelve measure identically. Crossfading between mip levels costs 40 dB of
    aliasing to recover 6 dB of treble, which is the wrong way round.

12. **In granular synthesis, rounding grain onsets to the sample grid costs 71 dB** — more than
    any choice of grain envelope, and more than the difference between a rectangular and a Hann
    window. Keep onsets fractional and compute both envelope position and carrier phase from a
    fractional `t`. And **a grain that reads a sample costs what a grain that synthesises a sine
    costs — 8.87 ns against 9.42 — provided its interpolation kernel comes out of a table.**
    Computing the kernel per read instead is 6.8x, which is the difference between 2 000 grains on
    a core and 300.

13. **Additive resynthesis stretches without artefacts because it never stitches anything.**
    Measured at 8× stretch: under half a cent of pitch error against naive overlap-add's 167 cents.
    The cost is that noise and transients are not sums of sinusoids, which is where every
    resynthesis engine — Harmor included — actually struggles.

14. **FM aliasing is a cliff, and oversampling is the right tool for it — the only place in this
    reference where that is true.** Measured: transparent up to a 1:1 index of 16, destroyed by 64;
    and one doubling of the sample rate buys **124 dB**, against 6 dB for a sawtooth. The predictor
    is the −60 dB bandwidth, about 1.5× Carson's rule, against Nyquist.

15. **The cheapest antialiasing is a waveform that never had a discontinuity.** A Casio-style
    phase-distorted cosine, with no correction machinery at all, measured **−40.0 dB** against the
    shipped PolyBLEP sawtooth's −35.4 dB at the same pitch and a comparable spectral tilt. Its
    brightness comes from warping a smooth function, so the usual price was never paid.

16. **Grain budget spans five thousand to one across shipping granular instruments** — two grains
    in Granulator III's Classic mode, 10 000 in a Tasty Chips GR-MEGA — and it is the spec that
    decides what each thinks granular is for. **The split is software against hardware and it is not
    the processor**: every published software figure sits between 2 and 100 (Padshop 2 caps at
    eight), every hardware figure between 1 000 and 10 000, while a measured ~10 ns per live stereo
    grain puts about 2 000 on one desktop core — twice the GR-1, a fifth of the GR-MEGA, and Padshop's
    eight two hundred and fifty times over. The GR-1 reaches 1 000+ of its nominal 2 048, a processor
    running out; the GR-MEGA reaches essentially all of its 10 240. The parameter *set* has converged
    completely; the numbers behind it have not.

17. **A vintage sampler's character is aliasing, and its converter's character is companding.**
    Measured: sampling a full-bandwidth source at 27.7 kHz without an input filter folds −25 dB of
    inharmonic material into the band, while the same rate *with* a filter is clean. And 8-bit
    µ-law companding beats 8-bit linear by 21 dB at −36 dBFS — where music actually lives — while
    being 13 dB worse at full scale.
