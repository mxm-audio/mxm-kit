# 3. Waveshapes in practice

The corrections of [chapter 2](02-antialiasing.md) applied to the waveforms a synthesizer actually
has, and the things that go wrong when you do.

---

## 3.1 Sawtooth

One discontinuity per period, height 2, always in the same direction. This is the easy case and the
one every method is derived for.

```rust
#[inline]
pub fn saw(p: &Phasor) -> f32 {
    2.0 * p.phase() - 1.0 - poly_blep(p.phase(), p.inc())
}
```

Two details that are easy to get wrong:

**Direction.** A rising ramp that wraps from +1 to −1 has a jump of −2, so the correction is
*subtracted* when the residual is written with a positive step. Get the sign backwards and the
aliasing gets worse than trivial, which is a useful debugging signal: if your "band-limited" saw
measures worse than the naive one, you have a sign error, not a subtle problem.

**Where `dt` comes from.** The residual is a function of `t/dt`, so `dt` must be the increment
actually used for this sample. If pitch is modulated per-sample, `dt` changes per-sample, and the
correction must use the new one. Our `Phasor` stores `inc` and `saw()` reads it back from the same
struct precisely so those cannot drift apart.

## 3.2 Pulse and PWM

A pulse of width `w` is a sawtooth minus a copy of itself shifted by `w`:

```rust
#[inline]
pub fn pulse(p: &Phasor, width: f32) -> f32 {
    let (t, dt) = (p.phase(), p.inc());
    let w = clamp_pulse_width(width, dt);
    let mut y = if t < w { 1.0 } else { -1.0 };
    y += poly_blep(t, dt);
    let second = {
        let x = t - w;
        if x < 0.0 { x + 1.0 } else { x }
    };
    y -= poly_blep(second, dt);
    y
}
```

Two edges, two corrections, opposite signs, and the second one's phase argument is the *wrapped*
`t − w`. The wrap is not a detail: without it the second correction is silently skipped for
`t < w`, which is exactly the region where the rising edge lives, and half the aliasing stays.

**Measured** (shipped code, 440 Hz, 44.1 kHz):

| width | alias | A-weighted | loudest alias |
|---|---|---|---|
| 0.05 | −29.3 dB | −38.1 dB | −26.3 dB |
| 0.10 | −32.5 dB | −40.5 dB | −34.2 dB |
| 0.25 | −36.2 dB | −42.5 dB | −40.9 dB |
| 0.50 | −36.9 dB | −42.5 dB | −42.3 dB |
| 0.75 | −36.2 dB | −42.5 dB | −40.9 dB |
| 0.95 | −29.3 dB | −38.1 dB | −26.3 dB |

Symmetric about 0.5, as it must be, and **7.6 dB worse at the extremes than at square**. That is
not a defect in the correction; it is the waveform. A 5% pulse has its energy spread over twenty
times the bandwidth of a square, so twenty times as much of it is above Nyquist waiting to fold.

The practical consequence: *PWM sweeps toward narrow are the worst case in the whole instrument*,
and if you are going to hear aliasing anywhere, it is on a bright PWM patch playing high.

## 3.3 The two-edge problem

The corrections have width. A two-point PolyBLEP touches one sample either side of an edge, so it
spans `2·dt` of phase. When the pulse is narrower than that, the two corrections overlap, each one
lands partly inside the other's edge, and the output is not a bad pulse but a wrong one.

Our clamp:

```rust
#[inline]
pub fn clamp_pulse_width(width: f32, dt: f32) -> f32 {
    let lo = 0.05f32.max(2.0 * dt);
    let hi = 0.95f32.min(1.0 - 2.0 * dt);
    if lo > hi { 0.5 } else { width.clamp(lo, hi) }
}
```

**Measured** at 44.1 kHz, showing the clamp in action:

| `f0` | `dt` | request 0.05 → | request 0.95 → |
|---|---|---|---|
| 55 Hz | 0.00125 | 0.0500 | 0.9500 |
| 440 Hz | 0.00998 | 0.0500 | 0.9500 |
| 2 kHz | 0.04535 | 0.0907 | 0.9093 |
| 8 kHz | 0.18141 | 0.3628 | 0.6372 |
| 16 kHz | 0.36281 | 0.5000 | 0.5000 |

So PWM range narrows from 2 kHz upward and has collapsed to a fixed square by 16 kHz. This is
correct behaviour — there is no width below `2·dt` that can be rendered — but it is also *audible
behaviour*: sweep the pitch of a narrow pulse upward and the timbre thins toward square on its own.
Whether that reads as a flaw or as analog-ish behaviour depends on the patch, and it is worth
knowing it is there rather than discovering it.

A table-based BLEP has the same problem, worse: its correction is `K` samples wide, so the collapse
starts at `f0 = fs/(K·w)` — with a 32-sample table and a 10% pulse, that is 138 Hz.

mxm-mono-01's pitch range makes this live: at the 2' setting the range switch adds 24 semitones, so a
high MIDI note lands well above 10 kHz before the `NYQUIST_FRACTION` clamp catches it. The
`clamp_pulse_width` unit test covers `dt` up to 0.45 for exactly this reason.

## 3.4 Sub-oscillator

The hardware (`research:oscillators/05-machines.md` §5.1) derives the sub with a
flip-flop divider clocked from the main oscillator's edge. That is the cheapest possible analog
answer and reproducing it digitally is a mistake: a divider driven by a sampled edge inherits the
edge's timing quantisation and produces a square wave whose transitions land on sample boundaries.
Sample-quantised edges are exactly what band-limiting exists to avoid.

Ours runs its own phasor at an exactly divided increment, and band-limits it like any other pulse:

```rust
// Derived from the main increment rather than from a divided frequency, so
// the sub stays exactly an octave (or two) down even as pitch moves.
self.sub.set_inc(self.main.inc() / sub_shape.divisor());
```

Dividing the *increment* rather than recomputing from a divided frequency matters: the increment is
already rounded once, and dividing it by 2 or 4 is exact in binary floating point, so the sub can
never drift against the main oscillator. The unit test
`sub_phase_is_locked_to_the_main_oscillator` asserts the ratio holds while pitch sweeps.

**Measured** (shipped `pulse()` at each sub's own frequency and width, main note 220 Hz, 44.1 kHz):

| Shape | `f0` | width | alias | A-weighted |
|---|---|---|---|---|
| 1 oct square | 109.7 Hz | 0.50 | −43.2 dB | −42.1 dB |
| 2 oct square | 55.9 Hz | 0.50 | −45.9 dB | −42.0 dB |
| 2 oct pulse | 55.9 Hz | 0.25 | −44.7 dB | −42.0 dB |

The sub is the *quietest* source in the instrument by alias content, simply because it is the
lowest-pitched, and the 2-octave-down narrow pulse is only 1.2 dB worse than the square despite
having four times the bandwidth.

One thing the independent phasor gives up: with a real divider, the sub's phase is rigidly tied to
the main oscillator's, and every note starts with the same phase relationship. With two free-running
phasors the relationship is whatever it happens to be. In a monosynth with a lowpass filter this is
inaudible. It would not be in a design where the sub and main are meant to null each other.

## 3.5 Noise

The one source that needs no band-limiting: white noise is already full-spectrum, and a sampled
white sequence is white up to Nyquist by construction. There is nothing above Nyquist to fold.

```rust
pub fn next_bipolar(&mut self) -> f32 {
    self.state ^= self.state << 13;
    self.state ^= self.state >> 17;
    self.state ^= self.state << 5;
    ((self.state >> 8) as f32 / 8_388_608.0) - 1.0
}
```

Three things about the shipped generator matter, and none of them is the spectrum. It must be
**deterministic** for tests — a contract in
[`../../crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) — it must be **reseeded
on `reset()`** so a rendered test is reproducible, and it maps to `[-1, 1)` through a range that is
exactly representable, hence the `>> 8` and the division by `2²³`, so the distribution has no bias
at the ends.

### Colour

"Noise" in a synthesizer is rarely white. Measured, as the least-squares slope over octave bands
from 100 Hz to 10 kHz:

| colour | slope | flatness | ns/sample |
|---|---|---|---|
| white (the shipped xorshift) | +0.08 dB/oct | −2.47 dB | 3.12 |
| pink (three staggered one-poles) | −2.91 dB/oct | −4.91 dB | 8.30 |
| brown (leaky integrator) | −5.90 dB/oct | −13.64 dB | 5.66 |
| blue (first difference) | +6.07 dB/oct | −5.89 dB | 4.35 |

The slopes land within 0.1 dB/octave of their targets, which is the check that the filters are what
they claim. The flatness column is the reminder from
[11-additive-resynthesis.md §11.5](11-additive-resynthesis.md#115-the-phase-vocoder-and-where-it-sits)
that a *periodogram* of white noise measures about −2.5 dB rather than 0 dB — the bias is in the
measurement, not the signal, which is why the white row is the baseline every other row is read
against.

Pink is the expensive one at 8.3 ns because it is three one-poles; brown is a single leaky
integrator and needs a clamp, because an unbounded integrator of a zero-mean sequence performs a
random walk and will eventually wander out of range.

### What noise is not

It is a synthesis source at mixer level, not dither and not a substitute for analog hiss. And the
SH-101's noise is not white either — it comes through the same mixer and filter as everything else,
so what reaches the ear has already been shaped. Modelling "the noise of a machine" means modelling
what is after it.

## 3.6 Hard sync

Not in mxm-mono-01 — one oscillator, nothing to sync to — but it is the case that breaks every
technique in this reference, so it belongs on the record.

In hard sync, a master oscillator resets the slave's phase mid-cycle. The reset is a discontinuity
whose height is *not* known in advance and *not* constant: it is whatever the slave's instantaneous
value was when the master wrapped. The correction has to be scaled by that height, sample by sample.

Three consequences:

1. **The step height must be computed, not assumed.** `h = y_after − y_before`, evaluated at the
   sync instant with the slave's fractional phase taken into account.
2. **Two discontinuities can now be arbitrarily close together**, because the master's period is
   unrelated to the slave's. The overlap problem of §3.3 becomes the normal case rather than an
   edge case, and the correction machinery has to accumulate overlapping residuals rather than
   assume at most one is active.
3. **Lookahead is awkward**, which is exactly why Brandt introduced minBLEP: its minimum-phase
   residual only touches samples after the transition, so no lookahead is needed and sync can be
   applied the instant it is detected.

A two-point PolyBLEP handles sync perfectly well if the height is computed properly and the
residual accumulator can hold several active corrections. The naive version — reusing the sawtooth
code path and hoping — produces the loud, buzzy, characteristically *wrong* sync that gives
digital sync a bad reputation.

**Soft sync** is a different operation and often confused with it: instead of resetting the slave,
it reverses or nudges its direction. On a triangle-core oscillator that is a slope discontinuity,
not a step, so the correct correction is a BLAMP rather than a BLEP
([02-antialiasing.md §2.8](02-antialiasing.md#28-blamp-corners-rather-than-steps)).

## 3.7 Pitch modulation and the moving `dt`

Every correction in this reference is derived for a *constant* `dt`. Under vibrato, glide,
envelope-to-pitch or audio-rate FM, `dt` changes between one sample and the next, and the
assumption behind the residual — that the waveform is a straight ramp of known slope either side of
the edge — stops being exactly true.

In practice this is a small effect for LFO-rate modulation and a real one for audio-rate FM. Two
guards are worth having regardless:

- **Clamp the increment, always.** `set_freq` clamps to `[8 Hz, 0.45·fs]` in our code because a
  modulation sum can and will ask for a frequency past Nyquist, and PolyBLEP does not make an
  invalid increment safe — it makes it produce garbage confidently.
- **Update `dt` before computing the sample, not after.** Otherwise the correction uses the
  previous sample's slope, which under fast modulation is the wrong slope by exactly the amount
  that is moving.

Aliasing under modulation is **not measured** in this reference: the exactly-periodic measurement
method needs a steady tone, and a different technique — comparing against a heavily oversampled
rendering of the same modulated signal — would be required. Recorded as an open gap in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

## 3.8 DC and asymmetry

A pulse wave has a mean of `2w − 1`. At `w = 0.05` that is −0.9 — the signal spends 95% of its time
at −1. Modulating width therefore modulates DC, and DC through a resonant filter and a VCA is an
audible thump, not a silent offset.

Hardware solves this with AC coupling. We solve it with an explicit one-pole blocker at 15 Hz,
deliberately below the fundamental of the lowest note at the 16' setting so it removes offset
without thinning the bass.

**Measured** (220 Hz, 44.1 kHz):

| width | raw mean | after the blocker |
|---|---|---|
| 0.05 | −0.900000 | −0.00000000 |
| 0.25 | −0.500000 | −0.00000002 |
| 0.50 | 0.000000 | 0.00000000 |
| 0.75 | 0.500000 | 0.00000002 |
| 0.95 | 0.900000 | 0.00000001 |

Nine orders of magnitude of offset removed, and the crate's existing test
`dc_blocker_passes_the_low_bass_it_is_supposed_to` asserts the other half of the requirement: less
than 1 dB of attenuation at 40, 60 and 100 Hz.

The blocker goes **after the mixer**, not per-source: one filter for the whole oscillator section
is cheaper and the sum is what needs correcting.

## 3.9 Hard sync, measured

§3.6 asserted that sync "breaks every technique in this reference" and left it at that. Here are the
numbers.

The setup is the one §3.6 describes: a slave sawtooth whose phase is reset by a master. The
corrected version applies the shipped two-point residual **twice** — once for the slave's own wrap,
which is an ordinary sawtooth discontinuity, and once for the reset, whose height is computed from
the slave's instantaneous value rather than assumed:

```rust
// The slave's own wrap, exactly as `saw()` corrects it.
y -= poly_blep_at(self.slave_phase, dt_s);
// The reset. Height is where the slave will land minus where it is, which
// is only knowable from the slave's current value.
let h = -2.0 * self.slave_phase;
y += 0.5 * h * poly_blep_at(self.master_phase, dt_m);
```

The reset also carries its fractional overshoot: the slave restarts at the point between samples
where the master actually wrapped, not on the sample grid. That is the same fractional-onset rule
[10-granular.md §10.3](10-granular.md#103-onset-placement-matters-more-than-everything-else)
measures at 71 dB for grains.

Measured at a 440.8 Hz master, sweeping the slave-to-master ratio. `vs ref` is deviation from an 8×
rendering — the fidelity companion, so a "correction" that works by removing treble cannot pass:

| ratio | naive | corrected | naive, vs ref | corrected, vs ref |
|---|---|---|---|---|
| 1.0 | −19.1 dB | −26.0 dB | −19.8 dB | −23.5 dB |
| 1.5 | −17.7 dB | **−29.9 dB** | −18.4 dB | −24.6 dB |
| 2.0 | −16.1 dB | −25.4 dB | −16.7 dB | −21.0 dB |
| 3.0 | −14.2 dB | −24.7 dB | −14.9 dB | −19.9 dB |
| 4.5 | −12.7 dB | −27.4 dB | −13.2 dB | −20.8 dB |
| 6.0 | −11.1 dB | −23.5 dB | −11.9 dB | −17.7 dB |
| 8.0 | −9.9 dB | −22.9 dB | −10.1 dB | −16.6 dB |

**The correction is worth 7 to 13 dB**, and it improves the fidelity column too — so it is genuinely
removing aliasing rather than trading treble for it.

**But it recovers much less than the same residual does on a plain sawtooth.** The shipped PolyBLEP
saw measures −35.4 dB at 440 Hz against the trivial waveform's −19.1: sixteen decibels. Under sync
at the same pitch the same residual manages seven. §3.6's claim survives contact with measurement —
sync really is harder — and now there is a number on it.

Two mechanisms account for the gap, and both were predicted in §3.6:

**The corrections overlap.** At ratio 1.0 the reset coincides with the slave's own wrap, so two
residuals land on the same samples and neither is correct. That row is the worst corrected figure in
the table (−26.0 dB) despite being the simplest case.

**The step height varies from cycle to cycle.** A sawtooth's discontinuity is always −2; a sync
reset's is `−2·φ_slave`, which is a different number every master period and, at non-integer ratios,
never repeats. A two-point residual is a good approximation to a *fixed* step and a worse one to a
step whose size is drifting.

The practical reading: **a two-point PolyBLEP is not enough for hard sync.** This is the case
[02-antialiasing.md §2.5](02-antialiasing.md#25-minblep) says minBLEP was invented for, and the
measured 7 dB is the argument for it — a longer, causal, minimum-phase residual has more of its
energy where the correction is needed and does not need lookahead the sync instant cannot provide.
We have not implemented minBLEP; it is in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

## 3.10 Ring modulation and AM

Multiply two oscillators together. With both inputs bipolar it is **ring modulation** and the
carrier disappears; with one input offset so it never crosses zero it is **amplitude modulation**
and the carrier survives. The SH-101 has neither, but the ARP 2600, the CS-80 and most modular
systems do.

The spectral rule is simple: multiplication in time is convolution in frequency, so every partial of
one input meets every partial of the other and produces a **sum and a difference**. Two sines at
`fa` and `fb` give `fa ± fb` and nothing else, which is why ring-modulated sines sound like bells —
the output frequencies are unrelated to either input.

And that is exactly why it aliases. Measured, with both inputs being the shipped band-limited
sawtooth, at multiples of a 109.7 Hz fundamental:

| inputs | ratio | alias | A-weighted |
|---|---|---|---|
| sine × sine | 2:3 | −247.5 dB | −247.5 dB |
| sine × sine | 4:7 | −247.8 dB | −251.5 dB |
| sine × sine | 8:13 | −248.1 dB | −253.1 dB |
| **saw × saw** | **2:3** | **−28.1 dB** | −27.2 dB |
| **saw × saw** | **4:7** | **−27.0 dB** | −32.7 dB |
| **saw × saw** | **8:13** | **−26.0 dB** | −32.7 dB |

**Two perfectly band-limited sawtooths, multiplied, produce −27 dB of aliasing** — about what a
*trivial* modulo-counter sawtooth manages on its own
([§2.11](02-antialiasing.md#211-the-comparison-table): −28.1 dB at 55 Hz). Every correction function
in this reference, applied to both inputs, buys nothing at all once they are multiplied.

The sine rows show why: two sines produce two output partials, both below Nyquist, so there is
nothing to fold. Two sawtooths with a hundred partials each produce ten thousand, and the sums run
to twice the input bandwidth — far past Nyquist by construction.

**So ring modulation is a case where band-limiting the sources is not the answer.** The output's
bandwidth is genuinely twice the inputs', and the honest fixes are the ones
[17-waveshaping-and-folding.md](17-waveshaping-and-folding.md) reaches for: oversample the
multiplication, or accept it. This is the same structural point as
[§12.8](12-fm.md#128-fm-is-not-only-an-oscillator)'s about phase-modulating a band-limited
oscillator — the corrections in chapter 2 protect a *waveform*, not an operation performed on it.

---

Next: [04-analog-character.md](04-analog-character.md) — everything above assumed the target was a
mathematical waveform. It usually is not.
