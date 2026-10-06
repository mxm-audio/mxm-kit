# 16. Supersaw, unison and detune

Stack several detuned copies of the same oscillator and you get the sound that has carried more
records than any filter design in this reference. The Roland JP-8000's supersaw is seven sawtooths
with a specific detune curve; every soft synth since has some version of it.

It is also the one oscillator topic where the intuitive answer is wrong in a way that measurement
settles immediately. **Stacking voices does not make aliasing worse.** It makes it *different*, and
"different" turns out to be the whole point.

Figures from [`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §17 at 44.1 kHz.

---

## 16.1 Measuring a stack needs a different ruler

The analysis of [chapter 1](01-fundamentals.md#15-the-measurement-method) marks bin `k·P` as wanted
and everything else as alias. Point it at a supersaw and it scores six of the seven voices as
aliasing — the technique counted as its own defect. Every number would have come out plausible and
wrong.

So §17 uses a generalisation: **N known harmonic series instead of one.** Each voice gets a distinct
*integer* period count `Pᵢ` in the analysis window rather than a detune in cents, so every voice is
exactly periodic, every wanted partial of every voice lands exactly on bin `k·Pᵢ`, and everything
else is alias by construction. Detune is quantised to the bin grid — about 10.6 cents per step at
220 Hz with `N = 65536`, and the steps are pairs of bins because the counts must stay odd.

That has a cost worth stating rather than hiding. An alias from voice `i` can land on a wanted bin
of voice `j` when `k·Pᵢ ≡ m·Pⱼ (mod N)`, and gets counted as signal. The harness reports how many
such collisions exist: **0 at one voice, 408 at nine.** The colliding harmonics are all high-order
and therefore low-amplitude, and the invariance measured below holds to 0.2 dB across that whole
range — which bounds the effect at 0.2 dB or less. That is the argument for trusting the numbers,
and it is available only because the collisions are counted.

## 16.2 Unison does not add aliasing

Seven voices means seven times as many aliased components. It also means seven times as much wanted
signal. Measured, at a fixed detune spread, power-preserving normalisation, 220 Hz centre:

| voices | alias | alias flatness | collisions |
|---|---|---|---|
| 1 | −38.3 dB | −103.51 dB | 0 |
| 3 | −38.2 dB | −87.76 dB | 32 |
| 5 | −38.2 dB | −79.11 dB | 110 |
| 7 | −38.1 dB | −73.39 dB | 238 |
| 9 | −38.1 dB | −68.94 dB | 408 |

**The alias-to-signal ratio does not move: 0.2 dB across a factor of nine in voice count.**

The reason is that detuned voices sit at *different frequencies*, so they add in power rather than
amplitude — and so do their aliases. Wanted power grows as N, alias power grows as N, the ratio is
constant, and no choice of output gain changes it. This is worth stating because the intuitive
argument ("more voices, more aliasing") and the opposite intuitive argument ("more voices, the
fundamental reinforces coherently") are both wrong, and they are wrong in opposite directions.

## 16.3 What unison actually changes: aliasing becomes noise

Look at the flatness column again. It rises monotonically — **−103.5 dB at one voice to −68.9 dB at
nine**, nearly 35 dB.

That column is the spectral flatness of the *alias bins only*. Near 0 dB means the alias energy is
spread evenly; strongly negative means it is concentrated in a few tall spikes. So:

**A single sawtooth's aliasing is a handful of discrete ghost tones. Nine detuned sawtooths' aliasing
is a noise floor.** Same total energy, same ratio to the signal, completely different character —
because each voice's aliases land at slightly different frequencies, and nine sparse combs
interleave into something dense.

This is why a supersaw can be run brighter than a single saw before it sounds wrong. Discrete
inharmonic ghosts beat against the harmonics and are heard as roughness and mistuning; the same
energy spread thin is heard as air. It is the same trade [10-granular.md §10.5](10-granular.md#105-asynchronous-clouds-statistics-not-waveforms)
makes with grain density, arrived at from a different direction.

## 16.4 How much detune, and when it stops helping

At seven voices, sweeping the spread:

| spread | ≈ cents | alias | alias flatness | collisions |
|---|---|---|---|---|
| 0 | 0.0 | −38.3 dB | −103.51 dB | 0 |
| 1 | 10.6 | −38.3 dB | −87.93 dB | 83 |
| 2 | 21.0 | −38.2 dB | −80.00 dB | 164 |
| 4 | 41.8 | −38.1 dB | **−73.39 dB** | 238 |
| 8 | 82.7 | −38.2 dB | −73.35 dB | 245 |
| 16 | 161.6 | −38.3 dB | −73.23 dB | 244 |

Two results.

**The ratio is invariant with detune too**, for the same reason as §16.2 — again 0.2 dB across a
sixteen-fold change.

**The dispersion saturates at about 40 cents.** From 0 to 42 cents the flatness improves 30 dB; from
42 to 162 cents it improves 0.16 dB. Once voices are separated by more than a few analysis bins their
alias combs no longer overlap, and detuning further cannot interleave them any more than they
already are.

That gives a concrete design number: **the alias-dispersing benefit of detune is essentially fully
paid out by ~40 cents.** Detune beyond that is a musical decision about beating and width, not an
antialiasing one.

## 16.5 Start phase is worth 4.6 dB of headroom

If every voice starts at phase zero, every voice's discontinuity coincides at note-on and the stack
sums coherently for one cycle. Measured over the first 50 ms, seven voices, power-preserving
normalisation:

| start phase | peak | |
|---|---|---|
| all zero | 2.6193 | 8.36 dBFS |
| randomised | 1.5400 | 3.75 dBFS |

**4.6 dB**, for free, by advancing each phasor a random fraction of a cycle at construction. The
zero-phase version also has an audible attack transient that has nothing to do with the envelope.

This interacts with the rule from [chapter 1](01-fundamentals.md#11-the-phasor) that phasors should
free-run and never reset on note-on. A free-running stack gets randomised phases automatically after
the first note; the problem is only the *first* one, and the fix is to randomise at construction
rather than to reset at note-on.

## 16.6 Cost

Measured against the 0.54 ns/sample bare counter:

| voices | ns/sample | vs counter |
|---|---|---|
| 1 | 2.12 | 3.9× |
| 3 | 5.96 | 11× |
| 7 | 11.58 | 21× |
| 9 | 15.38 | 28× |

Linear at about **1.7 ns per voice**, which is the shipped PolyBLEP sawtooth's 1.58 ns plus the
summation. A seven-voice supersaw is 11.6 ns/sample — about 0.06% of one core at 48 kHz, or under 1%
for sixteen-voice polyphony. Unison is cheap; it is one of the few things in this reference where
the obvious implementation is also the right one.

The one alternative worth knowing about is the **shared-phasor trick** — running one phasor and
deriving the others by offset — which saves the per-voice increment and wrap. It also forces every
voice to the same frequency, which makes it useless for detune. It is only applicable to unison
*without* detune, which is a phase-stacking effect rather than a supersaw.

## 16.7 Stereo, which is what unison is actually for

Everything above is mono, and so is every other measurement in this reference. That is a real
limitation here more than anywhere else, because **spreading the voices across the stereo field is
the reason unison exists** in most patches.

What the mono measurements do establish transfers directly: the per-voice cost, the phase-start
headroom, and the alias behaviour, all of which are per-voice properties. What they cannot say
anything about is the width, the mono-compatibility of a hard-panned stack, or how detune interacts
with inter-channel phase.

The one thing worth recording from practice rather than from measurement, clearly labelled as such:
a stack panned hard left/right in alternating voices collapses badly in mono, because the detuned
copies sum and produce exactly the beating the width was hiding. Spreading by *phase* rather than by
pan is the usual fix. **We have not measured this**; it is in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

## 16.8 Consequences for this repository

mxm-mono-01 is monophonic with one oscillator and no unison, and this proposes none. If an MXM
instrument grows one:

- **Seven voices, ~40 cents of spread** is the measured knee: past that, detune buys musical width
  but no further alias dispersion.
- **Randomise start phases at construction.** 4.6 dB of headroom for nothing.
- **Do not expect a unison patch to alias more than a single voice.** It does not. Budget the alias
  ceiling from the single-oscillator numbers in
  [02-antialiasing.md §2.11](02-antialiasing.md#211-the-comparison-table) and spend the effort on
  the correction function instead.
- **Budget 1.7 ns per voice**, linear.
- Normalise by `1/√N` for power or `1/N` for peak safety, and say which in the parameter's
  documentation — the choice does not change the alias ratio but changes every level in the patch.

---

Next: [17-waveshaping-and-folding.md](17-waveshaping-and-folding.md) — the other way to make a
spectrum from a sine.
