# 1. Fundamentals

What an oscillator has to produce, why the obvious implementation fails, exactly where the failure
lands in the spectrum, and how to measure it without lying to yourself.

---

## 1.1 The phasor

Every algorithm in this reference is built on the same object: a counter that increases by a fixed
amount each sample and wraps at 1.

```rust
phase += inc;          // inc = f0 / fs
if phase >= 1.0 { phase -= 1.0; }
```

`inc` is the *normalised frequency*, cycles per sample, written `T = f0/fs` in most of the
literature and `dt` in our code. Everything else — sawtooth, pulse, triangle, wavetable index — is
a function of `phase` and `inc`.

Three properties of this loop matter more than they look:

**The wrap is the waveform.** For a sawtooth the wrap *is* the discontinuity; for a pulse it is one
of two. All the difficulty in this document is about the two or four samples on either side of it.

**`inc` is the only thing that knows about pitch.** So the correction functions are all expressed
in terms of `inc`, and every one of them degrades as `inc` grows. An algorithm that is transparent
at 55 Hz (`inc ≈ 0.00125`) is doing something visibly crude at 7 kHz (`inc ≈ 0.16`).

**Precision is not free but is rarely the problem.** In `f32`, `phase` near 1.0 has a resolution of
about 6e-8, which at 48 kHz is a timing jitter of 3 ps — about −190 dB of phase noise, far below
anything else here. `f64` for the phasor buys nothing audible. What `f32` *does* cost is
accumulated frequency error: `inc` is rounded once, so the pitch error is fixed at up to half an
ulp, about 3e-8 relative, or 5e-5 cents. Also irrelevant. We keep the audio path in `f32`
([`../../AGENTS.md`](../../AGENTS.md)) and this is one of the places where that is uncontroversial.

**Do not reset the phase on note-on.** An analog VCO free-runs; the note-on gate opens an amplifier,
it does not restart the oscillator. Resetting mid-legato inserts a waveform discontinuity that no
band-limiting scheme knows about, which is both an audible click and, ironically, a burst of
aliasing. `oscillator.rs` resets phase only from `reset()`, and `voice.rs` never calls it on
note-on. The unit test `changing_frequency_never_resets_the_phase` exists to keep it that way.

## 1.2 What the classic waveforms actually are

For fundamental `f0`, amplitude ±1:

| Waveform | Harmonics | Amplitude of harmonic `k` | Envelope |
|---|---|---|---|
| Sawtooth | all | `2/(πk)` | −6 dB/octave |
| Square | odd only | `4/(πk)` | −6 dB/octave |
| Pulse, duty `w` | all, with nulls at `k = n/w` | `(4/(πk))·sin(πkw)` | −6 dB/octave, comb-notched |
| Triangle | odd only | `8/(π²k²)` | −12 dB/octave |

Two consequences worth internalising.

**Saw and pulse are the same problem.** Both decay at 6 dB/octave, which is the signature of a
step discontinuity. A pulse of width `w` is literally a sawtooth minus a copy of itself delayed by
`w` of a period; that is why the correct implementation is *two* sawtooth corrections with opposite
signs, and why the notches appear.

**Triangle is a different problem and an easier one.** Its 12 dB/octave decay comes from a
discontinuity in the *first derivative*, not the signal. It aliases far less to begin with, and
correcting it needs the integral of a step correction — the BLAMP of
[02-antialiasing.md §2.8](02-antialiasing.md#28-blamp-corners-rather-than-steps) — rather than a
step correction itself.

The SH-101 and therefore mxm-mono-01 have no triangle. Its sources are sawtooth, pulse with PWM, a
pulse sub-oscillator and noise, so this reference weights saw and pulse accordingly.

## 1.3 Why trivial sampling fails, precisely

A continuous sawtooth has harmonics up to infinity. Sampling it at `fs` folds every harmonic above
Nyquist back into the baseband. Harmonic `k` at frequency `k·f0` appears at

```
f_alias = | k·f0 − round(k·f0 / fs)·fs |
```

with the amplitude it had before folding, `2/(πk)`. Three properties follow, and they are what make
aliasing sound like aliasing rather than like noise:

1. **The folded partials are inharmonic.** `f_alias` is not generally a multiple of `f0`, so they
   beat against the real harmonics instead of reinforcing them.

2. **They move the wrong way.** Raise `f0` and a folded partial's frequency *falls*. This is the
   single most recognisable artefact in a badly written soft synth: play a rising line and hear a
   descending ghost line underneath it.

3. **They do not get quieter with pitch — they get louder.** Higher `f0` means fewer harmonics fit
   below Nyquist, so more of them fold, and the ones that fold have lower `k` and therefore larger
   amplitude.

Measured, on the trivial sawtooth
([`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §1, 44.1 kHz):

| `f0` | alias/signal | loudest alias vs fundamental |
|---|---|---|
| 55.9 Hz | −28.1 dB | −51.9 dB |
| 220.0 Hz | −22.2 dB | −40.1 dB |
| 880.8 Hz | −16.1 dB | −28.3 dB |
| 3520.0 Hz | −9.9 dB | −16.9 dB |
| 7040.7 Hz | −6.8 dB | −12.0 dB |

At the top of the range the aliased energy is a quarter of the total signal energy. That is not a
subtle defect; it is a different instrument.

## 1.4 Audibility: why the energy number is the wrong number

Total aliased energy is easy to measure and easy to argue about, and it is not what you hear. Two
distributions with identical total energy sound completely different depending on where the energy
sits relative to the masking curve of the harmonics next to it.

The literature's answer is the **noise-to-mask ratio** (NMR): compute the error signal against an
ideally band-limited reference, split both into critical bands, apply an interband spreading
function and the absolute hearing threshold, and report the ratio of error energy to masking
threshold. Below about −10 dB NMR, the artefacts are considered inaudible.
Välimäki, Pekonen and Nam ([08-sources.md](08-sources.md#jasa2012)) use exactly this and report the
highest fundamental at which each algorithm stays clean:

| Correction | Highest perceptually alias-free `f0` at 44.1 kHz |
|---|---|
| Look-up-table BLEP, 4 samples | 358 Hz |
| Look-up-table BLEP, 32 samples | 2036 Hz |
| **PolyBLEP, 2-point (what we ship)** | **2135 Hz** |
| PolyBLEP, 3rd-order Lagrange | 3236 Hz |
| PolyBLEP, 3rd-order B-spline | 4591 Hz |
| Look-up-table BLEP, 64 samples | 4595 Hz |
| PolyBLEP, 4th-order Lagrange | 5134 Hz |
| PolyBLEP, 4th-order B-spline | 7845 Hz |

The piano range tops out at 4186 Hz, which is why that table is usually summarised as "a two-point
PolyBLEP is fine for almost everything, and a four-point one is fine for everything".

The gap between the two metrics is real and worth stating plainly: **by total energy our shipped
PolyBLEP beats DPW2 by about 6 dB; by NMR it is clean an octave and a bit further up the keyboard.**
Energy-based metrics systematically understate methods whose residual aliasing is spread thin and
high, which is exactly what the BLEP family produces.

We measure energy anyway, for three reasons: it is exactly separable rather than model-dependent,
it needs no psychoacoustic model we would then have to defend, and it is monotonic in the same
direction as NMR for every algorithm compared here. Where it misleads, this reference says so.

The A-weighted column in our tables is a cheap partial correction — it discounts alias energy above
~12 kHz and below ~100 Hz roughly as the ear does — and it is *not* an NMR. Do not quote it as one.

## 1.5 The measurement method

Naive spectrum measurement of an oscillator is easy to get wrong in ways that manufacture or hide
30 dB. The method used throughout this reference removes the two big error sources by construction.

**Choose frequencies that are exactly periodic in the analysis window.** With `N = 65536` and

```
f0 = P · fs / N,   P odd
```

the waveform repeats exactly every `N` samples. No window function is needed, so there is no
spectral leakage and no window sidelobe floor to hide behind — a 4-term Blackman-Harris window,
the usual choice, has −92 dB sidelobes and would have made every result below −92 dB unmeasurable.

**Harmonics and aliases then land on disjoint, exactly known bins.** Harmonic `k` lands on bin
`k·P`; its alias lands on bin `k·P mod N`. Because `P` is odd and `N` is a power of two,
`gcd(P, N) = 1`, so an aliased component can only land on a wanted-harmonic bin if `k` and `j`
differ by a multiple of 65536 — far past where any partial has meaningful energy. So:

- wanted energy = the bins `k·P` for `k·P < N/2`
- aliased energy = every other non-DC bin

with no thresholding, no peak-picking and no estimation anywhere in the chain.

**Discard a settling prefix.** Every algorithm has state, and startup transients are broadband.
This is not a small effect: a differentiated-parabolic oscillator started with a zeroed memory
emits a single sample of `1/(4T)`, which at 220 Hz is +26 dB relative to the waveform and buries
every result below it. Our harness discards 4096 samples. Periodicity survives, because the signal
repeats every `N` samples regardless of starting phase.

**Validate the ruler before using it.** The harness measures an additively synthesised sawtooth
first. It reports −246 to −230 dB of "aliasing" — the numerical floor of the FFT — and zero
harmonic error. Any measurement chain that cannot do this is measuring its own artefacts.

The four numbers reported per waveform:

| Column | Meaning |
|---|---|
| `alias` | total aliased energy over total wanted-harmonic energy, dB |
| `A-wtd` | the same, both sides A-weighted |
| `worst` | loudest single aliased component, relative to the fundamental, dB |
| `h.err` | largest harmonic amplitude error against an ideal `1/k` saw, below 0.45·fs, dB |

`h.err`, and the `gone` count beside it in the harness output, exist because alias suppression can
always be faked by throwing away treble, and a table with only an alias column rewards exactly that
cheat. See [02-antialiasing.md §2.11](02-antialiasing.md#211-the-comparison-table).

## 1.6 What "correct" would even mean

There are two defensible targets and they are not the same.

**Target A: the band-limited ideal.** Reproduce the mathematical sawtooth's harmonics `2/(πk)` up
to Nyquist and nothing above. This is what additive synthesis and a well-built wavetable do exactly,
and what every BLEP-family method approximates. It is the right target when the oscillator is a
component in a larger design and you want no surprises.

**Target B: the analog original.** Reproduce a *specific circuit's* output, which is not a
mathematical sawtooth at all — its ramp is curved, its reset takes time, its duty cycle drifts with
pitch. See [04-analog-character.md](04-analog-character.md). This is the right target when the
character of one machine is the product.

mxm-mono-01 takes target A for the oscillator and puts the character in the filter — a deliberate
choice recorded in `oscillator.rs`'s own module docs ("quiet enough for an instrument whose
character comes from the filter rather than the oscillator") and revisited with numbers in
[07-rust-recipes.md](07-rust-recipes.md). The two targets need different measurements: against
target A, `h.err` should be zero; against target B, `h.err` should match the circuit's own
deviation, and being "wrong" in the same way as the hardware is the goal.

---

Next: [02-antialiasing.md](02-antialiasing.md) — the algorithm families, and what each one costs.
