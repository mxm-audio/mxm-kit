# 13. Casio phase distortion

Casio's CZ line, from 1984, is the answer to a question the rest of this reference keeps circling:
what if you got a bright, filter-like, sweepable waveform *without* ever generating a discontinuity?

Phase distortion reads a cosine table — the smoothest thing there is — but walks through it
unevenly. The output is a waveform that looks like a sawtooth, sweeps like a filter, and has no
step and no corner anywhere in it. The measurements below say that last property is worth about
20 dB.

Figures from [`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §12 at 44.1 kHz.

---

## 13.1 The mechanism

Take the ordinary phasor of [chapter 1](01-fundamentals.md#11-the-phasor) and bend it before using
it. The bend is two straight lines meeting at a breakpoint `(d, 0.5)`:

```rust
let g = if self.phase < self.d {
    0.5 * self.phase / self.d
} else {
    0.5 + 0.5 * (self.phase - self.d) / (1.0 - self.d)
};
let y = (2.0 * PI * g).cos();
```

So the first half of the cosine is traversed in the first `d` of the cycle and the second half over
the remaining `1 − d`. At `d = 0.5` the bend is straight and the output is a pure cosine. As `d`
falls, the first half-cycle is crammed into a shrinking window and the spectrum brightens.

That parameter is Casio's **DCW** — Digitally Controlled Wave — which they marketed as the CZ's
equivalent of a filter. It is not a filter, but the analogy is fair: sweeping `d` moves a spectral
corner, and the harmonics above it fall away smoothly.

The formulation above is from Kleimola, Lazzarini, Välimäki and Timoney's Vector Phaseshaping paper,
which treats classic PD as the one-breakpoint case of a general **phaseshaping** framework. Their
description of the parameter is worth having: "the closer `d` is to 0 or to 1, the brighter the
signal (and proner to audible aliasing)", with a symmetry around `d = 0.5` — below it you get a
falling shape, above it a rising one.

The real CZ hardware was more elaborate than one breakpoint: Casio's phase transforms are
"assembled from piecewise linear functions under binary logic control", with "characteristic sharp
knees (and for some transforms, even sudden jumps)", giving eight waveforms — saw, square, pulse, a
double sine, a half sine, and three resonant shapes (§13.5).

## 13.2 The trick: there is no discontinuity

Work out what is actually discontinuous in a PD sawtooth and the whole method falls into place.

At the **cycle wrap**, `g` goes from 1 back to 0, and `cos(2π·1) = cos(0) = 1`: the output is
continuous. Its slope is `−2π·g'·sin(2π·g)`, and `sin(2π·0) = 0`, so the slope is zero on both
sides — continuous too. What differs across the wrap is `g'`, which appears squared in the second
derivative. **The waveform is C¹ and breaks only in curvature.**

At the **knee**, `g = 0.5` and `sin(π) = 0`, so again the slope is zero either side and only the
curvature jumps.

That matters because the spectral decay of a waveform is set by the order of its worst
discontinuity: a step gives 6 dB/octave, a corner 12, a curvature break 18. A phase-distorted
cosine should therefore start out far quieter above the audio band than any waveform in
[chapter 2](02-antialiasing.md) — and it does, until `d` gets small enough that the fast segment's
own frequency content takes over.

Measured, 440.8 Hz, sweeping the breakpoint. `fast seg` is the instantaneous frequency of the
compressed segment, `f0/(2d)`; `tilt` is the least-squares slope over the first eight harmonics:

| `d` | fast segment | alias | bandwidth | tilt |
|---|---|---|---|---|
| 0.500 | 441 Hz | −247.6 dB | 441 Hz | — (a pure cosine) |
| 0.250 | 882 Hz | −87.9 dB | 5289 Hz | −44.89 dB/oct |
| 0.100 | 2204 Hz | −71.3 dB | 9697 Hz | −8.66 dB/oct |
| 0.050 | 4408 Hz | −59.1 dB | 18071 Hz | −7.24 dB/oct |
| 0.020 | 11019 Hz | −40.0 dB | 22038 Hz | −6.84 dB/oct |
| 0.010 | 22038 Hz | −27.9 dB | 22038 Hz | −6.78 dB/oct |
| 0.002 | 110189 Hz | −22.1 dB | 22038 Hz | −6.75 dB/oct |

Two things to read off it.

**The tilt converges on a sawtooth.** −6.84, −6.78, −6.75 dB/octave as `d` shrinks. Casio's "saw" is
a real sawtooth spectrum, approached asymptotically by squeezing a cosine rather than by drawing a
ramp. The convergence is from *above* — at `d = 0.25` the tilt is −44.9 dB/octave, which is a
gently rolled-off tone, and that is the filter-sweep behaviour the DCW envelope was selling.

**Aliasing rises smoothly, and earlier than the naive theory predicts.** I expected a cliff at the
point where the fast segment passes Nyquist, by analogy with FM
([12-fm.md §12.3](12-fm.md#123-aliasing-a-cliff-not-a-slope)). That is wrong: at `d = 0.05` the
fast segment is only 4.4 kHz, a fifth of Nyquist, and the output already measures −59.1 dB. The
compressed segment is not a steady tone at `f0/(2d)`; it is half a cosine in a short window, and
the curvature break at each end radiates a tail that decays at about 6 dB/octave — so once `d` is
small the aliasing behaves like a sawtooth's, growing about 6 dB per halving of `d`.

## 13.3 A naive PD sawtooth aliases *less* than a corrected trivial one

The comparison that makes the point. All at 440.8 Hz, 44.1 kHz:

| oscillator | alias | tilt | ns/sample |
|---|---|---|---|
| trivial sawtooth | −19.1 dB | −6.0 dB/oct | 0.54 |
| **shipped two-point PolyBLEP sawtooth** | **−35.4 dB** | −6.0 dB/oct | 1.58 |
| **phase distortion, `d = 0.02`** | **−40.0 dB** | −6.84 dB/oct | 10.37 |

A phase-distorted cosine with no antialiasing machinery of any kind measures **4.6 dB cleaner than
our band-limited sawtooth**, and 21 dB cleaner than the trivial one — because there is nothing to
band-limit. The brightness came from warping a smooth function instead of from a discontinuity, so
the usual price was never paid.

Three caveats, because this is the kind of result that gets over-quoted:

- **It is not a sawtooth.** The tilt is −6.84 dB/octave rather than −6.02, so the harmonic
  amplitudes are not `1/k` and the waveform is not the one a ramp produces. It is a *saw-like* tone.
  If you need an actual sawtooth — to match a reference, to null against another oscillator — this
  is not it.
- **It costs 6.6× more**, because every sample is a `cos()`. A table-read cosine would bring it to
  roughly 4 ns ([11-additive-resynthesis.md §11.2](11-additive-resynthesis.md#112-three-ways-to-make-a-sine-measured)),
  still more than double the PolyBLEP saw.
- **The comparison is at one `d` and one pitch.** At `d = 0.01` the PD saw is 7.5 dB *worse* than
  the PolyBLEP saw, and §13.4 shows what pitch does.

The general lesson is the one worth keeping: **the cheapest antialiasing is a waveform that never
had a discontinuity.** Casio got a filter-like sweep out of 1984 hardware by choosing a
construction whose artefacts were third-order instead of first.

## 13.4 Brightness has to be key-scaled

Everything in §13.2 scales with pitch, because the fast segment's frequency is `f0/(2d)`. Measured
at a fixed `d = 0.02`:

| `f0` | fast segment | alias | bandwidth |
|---|---|---|---|
| 55.9 Hz | 1396 Hz | −87.5 dB | 3686 Hz |
| 109.7 Hz | 2742 Hz | −73.1 dB | 7239 Hz |
| 220.0 Hz | 5501 Hz | −57.2 dB | 14523 Hz |
| 440.8 Hz | 11019 Hz | −40.0 dB | 22038 Hz |
| 880.8 Hz | 22021 Hz | −25.1 dB | 22021 Hz |

**About 15 dB per octave of pitch**, and the bandwidth pins to Nyquist from 440 Hz upward. A DCW
setting that is transparent in the bass is audibly rough two octaves up.

This is the same conclusion [12-fm.md §12.3](12-fm.md#123-aliasing-a-cliff-not-a-slope) reached for
FM's modulation index, and the same fix applies: **scale the brightness parameter with the note.**
Both techniques hand the user a knob that means "bandwidth", and on both, a fixed setting means a
different amount of aliasing at every pitch.

## 13.5 The resonant waveforms are grains

The CZ's three resonant shapes are the part people remember, and structurally they are not phase
distortion at all. A sine at `k` times the fundamental is **restarted every fundamental period** and
**windowed** so it dies away before the restart — Casio's patent figure shows a resonance counter
hard-reset by the base frequency counter.

Which makes them exactly the object of [10-granular.md §10.7](10-granular.md#107-granular-as-an-oscillator-specifically):
a synchronous grain train, grain rate locked to the pitch, the grain's carrier setting a formant
that stays put while the pitch moves. The CZ shipped granular formant synthesis in 1984 and called
it a resonant filter.

It also has a granular problem, and the measurement finds it. Casio's saw-resonance window falls
from 1 to 0 across the period, so at the restart the waveform **jumps from zero back to full
amplitude** — a step, of exactly the kind [chapter 2](02-antialiasing.md) exists to correct.

Measured at 440.8 Hz, with and without the shipped two-point PolyBLEP applied to that step:

| window | `k` | corrected | alias | A-weighted |
|---|---|---|---|---|
| falling saw | 4 | no | −22.2 dB | −26.1 dB |
| falling saw | 4 | **yes** | **−38.3 dB** | **−47.6 dB** |
| falling saw | 16 | no | −21.9 dB | −24.2 dB |
| falling saw | 16 | **yes** | **−36.2 dB** | **−43.4 dB** |
| triangle | 4 | no | −61.8 dB | −67.3 dB |
| triangle | 4 | yes | −61.8 dB | −67.3 dB |
| trapezoid | 4 | no | −26.2 dB | −30.1 dB |
| trapezoid | 4 | **yes** | **−42.3 dB** | **−51.5 dB** |

Three results, and the third is the reason to trust the first two.

**A BLEP on the reset is worth 16 dB**, and 21 dB A-weighted, on the saw window. One extra
polynomial on two samples per period.

**The triangle window is 40 dB cleaner to start with and the correction does nothing for it** —
because a triangle window starts at zero, so there is no step to correct. This is
[10-granular.md §10.2](10-granular.md#102-the-envelope-is-the-antialiasing-decision)'s finding
arriving from the opposite direction: *windows are judged by their endpoints*. A window that starts
at zero and ends at zero has no discontinuity to band-limit; one that snaps back to full amplitude
has the worst kind.

**The correction is a no-op exactly where theory says it should be**, which is the internal
consistency check that says the other rows are measuring what they claim.

Sweeping the resonance up the spectrum, the way a CZ envelope does:

| `k` | resonant peak | alias | with BLEP |
|---|---|---|---|
| 1 | 441 Hz | −22.3 dB | −38.6 dB |
| 4 | 1763 Hz | −22.2 dB | −38.3 dB |
| 16 | 7052 Hz | −21.9 dB | −36.2 dB |
| 32 | 14104 Hz | −20.6 dB | −30.0 dB |
| 48 | 21156 Hz | −14.2 dB | −16.3 dB |

The uncorrected figure barely moves with `k`, because it is dominated by the reset step rather than
by the resonant sine. The correction holds its 16 dB up to `k = 16` and then falls away as the
resonant peak itself approaches Nyquist — at `k = 48` the peak is at 21 kHz and there is nothing
left to protect. **Clamp the resonance multiple so `k·f0` stays below about 0.4·fs**; the CZ's own
resonance range does the equivalent.

## 13.6 Where PD sits in the family

**PD is phase modulation with a synchronised, angular modulator.** The Vector Phaseshaping paper
puts it as PD being "a type of complex-wave phase modulation"; the distinction against DX-style FM
is that PM uses "an oscillating modulator that can have its own period", while PD applies "an
angular modulator of straight-line segments hard-synchronised to the same period as its
corresponding carrier". Same operation, different modulator, and the hard synchronisation is what
keeps PD's output strictly harmonic where FM's ratio parameter can wander inharmonic.

**Phaseshaping is not waveshaping, and it is better behaved.** Both distort a sine, but waveshaping
bends the *amplitude* axis and phaseshaping bends the *time* axis. The consequence the DAFx-11
authors call out is worth quoting: in phaseshaping "the use of non-smooth shaping functions does not
necessarily imply the presence of audible aliasing (which is more or less inevitable in
waveshaping)". A kink in a waveshaper's transfer curve becomes a kink in the output every time the
signal crosses it; a kink in a phase-shaping function becomes, as §13.2 showed, a curvature break.
They also note phaseshaping needs no gain scaling, where a waveshaper almost always does.

**Vector phaseshaping is the generalisation.** Release the breakpoint's vertical position from 0.5
and make it a vector `(d, v)`: `v` beyond 1 sweeps multiple cosine cycles into one segment, which
produces formant peaks directly, and `v` at non-integral values of `2v − 1` produces aliasing from
the incomplete final cycle. The paper gives a suppression algorithm that renders the incomplete
segments as whole half- or full-cycle sinusoids, at the cost of some high end.

**Adaptive PD** takes the same idea to arbitrary input signals rather than a stored cosine, by
modulating the coefficient of a first-order allpass filter at audio rate — phase distortion as an
*effect* rather than an oscillator.

## 13.7 Cost

Mean of three runs, against the 0.54 ns/sample bare counter:

| variant | ns/sample | vs counter |
|---|---|---|
| PD sawtooth | 10.37 | 19× |
| PD resonant | 9.38 | 17× |
| PD resonant + BLEP on the reset | 11.26 | 21× |

All three are one `cos()` plus arithmetic, so they land where
[11-additive-resynthesis.md §11.2](11-additive-resynthesis.md#112-three-ways-to-make-a-sine-measured)
says a transcendental lands. The BLEP correction costs 1.9 ns — under 20% — for its 16 dB, which is
one of the better exchange rates in this reference.

A table-read cosine would roughly halve all three. Casio, of course, *was* a table read: the whole
method exists because reading a stored cosine at a warped index was the cheapest interesting thing
an 8-bit-era chip could do.

## 13.8 Consequences for this repository

mxm-mono-01 has no phase distortion and none is proposed. What the measurements are worth keeping for:

- **If a future instrument wants a cheap filter-like sweep, PD is a real option.** One `cos()`, one
  breakpoint, no tables, no correction functions, and measured aliasing between −88 dB and −28 dB
  depending on how far the DCW is pushed.
- **Key-scale the breakpoint.** 15 dB per octave of pitch is not something a user should have to
  manage.
- **If a resonant or formant waveform is ever built, start the window at zero.** 40 dB, free. If the
  design needs the hard reset for its character, BLEP the step: 16 dB for 1.9 ns.
- **Clamp the resonance multiple** so the formant stays below about 0.4·fs.
- **The transferable idea is the one in §13.3**: reach for a construction whose worst discontinuity
  is high-order before reaching for a correction function. Corrections are cheap, but not needing
  one is cheaper.

One thing this chapter did *not* do: `poly_blep_at` in the harness is a transcription of the private
`poly_blep` in `oscillator.rs`, because the shipped function is not exported and the resonant
experiment needs to apply it to a discontinuity that `saw` and `pulse` do not know about. That
duplication is a small hazard — if the shipped residual ever changes, this copy has to change with
it — and it is noted in [07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

---

Back to [README.md](README.md) · sources: [08-sources.md](08-sources.md)
