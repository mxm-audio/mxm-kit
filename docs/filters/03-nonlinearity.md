# 3. Nonlinearity and character

[← topologies](02-topologies.md) · [index](README.md) · [next: efficiency →](04-efficiency.md)

Chapter 2 ended on an uncomfortable fact: **structures of the same order with the same poles have
identical linear responses.** A Minimoog ladder, a Juno-106 IR3109 and a Prophet-5 CEM3320 all
realise `1/((1+s)⁴+k)`. They sound nothing alike. Everything that separates them is here.

---

## 3.1 What "character" actually decomposes into

Character is not one thing. It is at least seven, and they are separately implementable and
separately testable:

| Ingredient | What it does | Where it comes from |
|---|---|---|
| **Passband shape under resonance** | The ladder droop; the diode ladder's early roll-off | Pole positions — §2.3, §2.5 |
| **Saturation curve** | Harmonic spectrum of the distortion; soft vs hard | The transfer curve of the nonlinearity |
| **Saturation *placement*** | Whether drive tames resonance, thickens the body, or fizzes the top | Which node clips — §3.2 |
| **Self-oscillation amplitude and purity** | Whether the filter sings a sine, a triangle, or screams | How the loop limits — §3.2, §3.3 |
| **Asymmetry / DC** | Even harmonics; the operating point moving with level | Single-ended clipping, diode pairs of unequal drop |
| **Noise and drift** | "Alive" vs sterile; polyphonic width | Thermal noise, component tolerance, temperature — §3.8 |
| **Control feel** | Whether resonance is usable across its range and cutoff tracks the keyboard | Parameter mapping — §3.9 |

If a filter sounds "digital", the cause is nearly always #3, #5 or #7, and almost never the pole
placement everyone spends their time on.

---

## 3.2 Where the nonlinearity goes

There are four candidate nodes in a resonant filter. They do completely different things.

```
              ┌──────────── × k ◄──── [D] ◄───────────────┐
              ▼ −                                         │
 x ──[A]────►(+)──► LP[C] ──► LP[C] ──► LP[C] ──► LP[C] ──┴──[B]──► y
```

### [A] Input saturation — cosmetic

Clips before the filter. Adds harmonics that the filter then removes. Does **nothing** to bound the
resonant loop: a linear loop past its oscillation threshold grows without bound no matter how
politely you feed it. If your filter's only nonlinearity is here, the resonance will still blow up.
This repo's `filter.rs` header documents exactly that mistake being made and corrected.

Useful for: a drive control that colours the source. Not a substitute for anything else.

### [B] Output saturation — a limiter, not a filter

Bounds what you hear but not what the filter *does*. The loop is still linear and still diverging;
you are just clamping the display. The internal state grows until it hits `f32` limits, and the
recovery behaviour when you back the resonance off is horrible (the state has to decay from 10⁶).

Useful for: final safety, and for a post-filter drive stage — which is a real and good-sounding
thing, just not a resonance control.

### [C] Per-stage saturation — the ladder's body

A `tanh` inside each rung. This is physically what the transistor pairs do, and it is Huovilainen's
model. Effects:

- Each stage compresses independently, so loud low frequencies get squashed *before* they reach the
  next pole — the filter thickens rather than fizzes.
- The effective cutoff moves with level, because the small-signal gain of `tanh` drops as the signal
  grows. Hard-driven ladders get *darker*, which is exactly what analog ones do.
- It bounds the loop, since each stage's output is limited.

Cost: four extra `tanh` evaluations, and if you solve the loop properly, a nonlinear system with
four unknowns rather than one.

### [D] Feedback-path saturation — the one that decides everything

The single most important node. It sets:

- **Whether the filter is stable past `k = 4`.** With a bounded saturator here, the injected
  feedback is bounded by `k` regardless of internal state, so the whole system is bounded. That is
  the argument `filter.rs` rests on.
- **The self-oscillation amplitude.** The loop settles where the saturator's average gain brings
  loop gain back to exactly 1. A softer curve settles louder and cleaner; a harder curve settles
  quieter and buzzier.
- **The oscillation waveform.** `tanh` gives a near-sine that gets triangular as you push past
  threshold. Back-to-back diodes (a much harder knee) give something closer to a squashed square.

**This is where the SH-101 differs from the Juno-60.** Same IR3109 chip, same four poles — but the
SH-101 puts back-to-back diodes to ground in the resonance feedback loop. They limit the
self-oscillation level and produce the raunchier top end the 101 is known for. One pair of diodes,
in one place, is the difference between two famous filters.

### The practical recipe

For a ladder: **[D] always**, **[C] if you can afford it**, **[A] as a user-facing drive**, **[B]
never as your only bound**. For an SVF standing in for a Sallen–Key: saturate the damping term
(`k·v1`) — that is the same node as [D].

```rust
/// Ladder feedback with a diode-clamp flavour instead of a smooth tanh.
/// `knee` sets where it turns over; smaller = harder, buzzier, quieter oscillation.
#[inline]
pub fn diode_clamp(x: f32, knee: f32) -> f32 {
    // Two back-to-back diodes: nearly linear below the knee, hard above it.
    let a = x / knee;
    knee * a / (1.0 + a.abs().powf(3.0)).powf(1.0 / 3.0)
}
```

Swap that for `tanh` in a ladder's feedback path and A/B them. The difference is far larger than any
change you can make to the pole positions.

---

## 3.3 Solving a nonlinear delay-free loop

Once a saturator sits inside the loop, `y = f(...)` is implicit and nonlinear. Four options, in
increasing cost and quality.

### (a) Delay the feedback one sample

```rust
let u = x - k * tanh(self.y_prev);
```

Cheap, always stable, and **wrong in a way you can hear**. The extra `z⁻¹` adds phase to the loop
that scales with `fc/fs`, so both the resonant peak frequency and the oscillation threshold become
functions of cutoff. Measured in this repo: threshold `k` wandering between 2.66 and 3.96 depending
on cutoff, and 11% tuning error at 2 kHz. At high cutoff the top of the resonance control is already
self-oscillating while at low cutoff it never gets there.

Acceptable only if you oversample heavily (which shrinks `fc/fs`), which is a strange way to spend
CPU.

### (b) One-step linearisation

Replace `f` with a straight line for this sample and solve the resulting linear system.

- **Tangent (Newton, one step):** line through the previous solution with slope `f'`. Accurate when
  the signal changes slowly; degrades badly on transients.
- **Fixed-pivot secant ("mystran's method"):** line through the origin and the previous estimate,
  i.e. use the *secant* gain `f(y₋₁)/y₋₁` rather than the derivative. Less accurate in the smooth
  case, far more tolerant when the assumption breaks.

Cost: one extra multiply-ish, plus recomputing the loop denominator per sample. This repo measured
the secant version: tuning and threshold both correct, but peak output overshooting to **54×** on
fast growth, because the gain estimate lags. Fine for gentle settings, dangerous at extremes.

### (c) Newton–Raphson to convergence-ish

Substitute the cascade into the feedback and you get one scalar equation in the output. For a
ladder with the nonlinearity only in the feedback path, that equation is strictly monotonic, so it
has exactly one root and Newton cannot wander. A **fixed** iteration count (three is enough for the
residual to drop below the noise floor) keeps the cost constant and the audio thread branch-free —
which also keeps it SIMD-able across voices.

This is what `crates/mxm-mono-01-dsp/src/filter.rs` does, and its measured numbers are threshold
`k = 3.97–4.00` across 44.1/48/96/192 kHz, tuning within 1%, peak output 1.59.

```rust
// Sketch. Solve  y = C(x - k·f(y))  for y, where C is the linear cascade gain
// including its state contribution, and f is the saturator.
let mut y = self.y_prev;                    // warm start: last sample's answer
for _ in 0..3 {
    let (f, df) = tanh_and_derivative(y);
    let residual = y - (c_gain * (x - self.k * f) + c_state);
    let slope = 1.0 + c_gain * self.k * df;  // d(residual)/dy, always ≥ 1
    y -= residual / slope;
}
self.y_prev = y;
```

`slope ≥ 1` because `c_gain`, `k` and `df` are all non-negative — hence no division blow-up and no
convergence check needed. Establishing that property for *your* topology is the work; once it holds,
the solver is three lines.

### (d) Full implicit solve of a multi-nonlinearity system

With saturators in all four stages plus the feedback ([C] + [D]), you have a 5-unknown nonlinear
system per sample. Newton on a 5×5 (which is banded, so cheap) or a Gauss–Seidel sweep. Huovilainen
sidesteps it by using a one-sample delay only in the *stage* nonlinearities while oversampling 2×;
D'Angelo & Välimäki and the "Generalized Moog Ladder" papers do it properly.

**Judgment call:** the audible return on (d) over (c)-plus-[C]-with-delayed-stage-nonlinearity is
small compared to the return on getting [D] right. Spend the effort on [D] first.

---

## 3.4 Choosing the saturator

Requirements, in priority order:

1. **Bounded.** `|f(x)| ≤ M` for all `x`, including `f32::MAX`. The boundedness proof of the whole
   filter usually rests on this, so it must be *exactly* true in floating point, not nearly true —
   clamp the output.
2. **Monotonic.** A non-monotonic curve gives the loop equation multiple roots, and Newton will
   flip between them, which sounds like tearing.
3. **Smooth enough.** A discontinuity in slope is a source of high-order harmonics and therefore
   aliasing. `tanh` is `C∞`; a hard clip is `C⁰` and needs several times more oversampling.
4. **Right shape.** Odd symmetry → odd harmonics only (hollow, "tube-ish"). Asymmetry → even
   harmonics (fuller, "warm") plus a DC offset you must deal with.
5. **Cheap.** It runs at least once, often five times, per sample per voice, possibly inside a
   3-iteration Newton loop.

### Practical curves

| Curve | Formula | Notes |
|---|---|---|
| `tanh` | `tanh(x)` | The default. Soft, odd, `C∞`. libm call — approximate it (§4.5). |
| Padé [7/6] `tanh` | rational | <1e-4 error to \|x\|≤4, ~8 flops. Used in this repo. |
| Algebraic soft clip | `x/(1+\|x\|)` | Very cheap, but flattens too slowly — sounds vague. |
| Cubic soft clip | `x − x³/3`, clamped at ±1 | Cheap, hard knee at the clamp. `C¹` only. |
| Diode clamp | §3.2 | Harder knee, "raunchy" — the SH-101 flavour. |
| Asymmetric | `tanh(x + b)` − `tanh(b)` | Adds even harmonics. Watch DC. |

### Rate limiting is a different axis

Every curve in the table above limits **amplitude**. A **rate** limiter — clamping `dy/dt` rather
than `y` — is a genuinely different nonlinearity and costs the same:

- Whether it distorts depends on **amplitude × frequency**, so the same setting is clean on a low
  note and dirty on a high one, with no key tracking involved.
- It produces **even and odd harmonics** in a dense, slowly-falling series, where a symmetric
  saturator produces odd harmonics only. Measured: even harmonics ~50 dB stronger, and the series
  falling at 2 dB per harmonic instead of 6.

It is how the Polivoks filters — see `research:filters/machines/polivoks.md` §2 — and it is
close to unexplored in software.

### Asymmetry and DC

Asymmetric saturation shifts the mean of its output. Inside a feedback loop, that offset *biases the
operating point*, which changes the effective gain — a slow, level-dependent wobble. That is a real
part of the TB-303's and MS-20's behaviour, and it is why those circuits feel alive and slightly
unstable.

If you want it: keep it, and block DC at the **output** only, with a ~5 Hz one-pole highpass. If you
block DC inside the loop, you remove the effect you were trying to model. If you leave it entirely,
you hand the DAW an offset and every downstream plugin a headache.

---

## 3.5 Aliasing — and why filters alias less than you fear

Any nonlinearity generates harmonics. Harmonics above Nyquist fold back as inharmonic tones that do
not move correctly with pitch, which is the "digital" sound people complain about.

But a **resonant lowpass attenuates its own generated harmonics**, and the ones generated in the
feedback path go through the whole 24 dB/oct cascade before they reach the output. This is a real,
large effect and it is why VA filters get away with far less oversampling than VA distortion units.

The danger zone is narrow and specific:

- **Cutoff high** (the filter is not attenuating anything), **and**
- **drive high** (the nonlinearity is generating a lot), **and**
- **input already bright** (a saw at 2 kHz, not a sine at 100 Hz).

That is a real preset, so it has to be handled — but "oversample the whole plugin 4× always" is the
wrong response. Options, cheapest first:

1. Use a smooth saturator (`C∞`), which puts most of the energy in low-order harmonics.
2. ADAA on the memoryless nonlinearities (§3.7) — roughly the alias reduction of 2–4× oversampling,
   at a fraction of the cost.
3. Oversample only the filter, 2×, and only when drive exceeds a threshold — but see the note on
   switching artefacts below.
4. Oversample 2× always. Honest, simple, ~2.5× the filter cost including the resampling filters.

**Do not switch oversampling factors on a parameter value at runtime.** The state discontinuity
clicks and the latency changes. Either switch on a user-facing "HQ" control that is documented as a
non-realtime setting, or don't switch.

---

## 3.6 Oversampling, done properly

### The shape of it

```
 x[n] ──► upsample ×L ──► [ nonlinear filter at L·fs ] ──► downsample ÷L ──► y[n]
              │                                                 │
        interpolation                                       decimation
        lowpass at fs/2                                    lowpass at fs/2
```

Both lowpasses must stop everything above the *original* Nyquist. For `L = 2` this is a **halfband**
filter — passband to 0.5·fs/2, stopband from 1.5·fs/2 — and halfbands are special: every other tap
of the FIR is exactly zero, so they cost half of what their length suggests. That is the real reason
oversampling factors are powers of two.

### FIR vs IIR

| | FIR halfband | Polyphase IIR (allpass) halfband |
|---|---|---|
| Phase | Linear | Nonlinear |
| Latency | (N−1)/2 samples, must be reported to the host | ~1 sample, effectively none |
| Cost for −80 dB | ~30–60 taps, half of them zero | ~4–6 first-order allpass sections total |
| Where it wins | When phase matters, or when you already have latency | Synths, where nobody can hear the phase and latency is annoying |

For a synth VCF, polyphase IIR is usually right: nobody is phase-matching a filtered sawtooth
against a dry copy, and reporting latency for an instrument is a nuisance. For a mastering-adjacent
effect, use FIR.

### Generating your own FIR halfband

Do not copy coefficients from a forum post. Generate them at `activate()` (allocation is fine there,
never in `process`) and check the response in a test.

```rust
/// Kaiser-free Blackman–Harris windowed-sinc halfband, `2*half + 1` taps.
/// Cutoff at fs/4 by construction, so every even-indexed tap except the centre is zero.
pub fn halfband_taps(half: usize) -> Vec<f32> {
    let n = 2 * half + 1;
    let mut h = vec![0.0f32; n];
    for (i, tap) in h.iter_mut().enumerate() {
        let m = i as f64 - half as f64;
        let sinc = if m == 0.0 {
            0.5
        } else {
            (std::f64::consts::PI * m * 0.5).sin() / (std::f64::consts::PI * m)
        };
        // Blackman–Harris window
        let t = 2.0 * std::f64::consts::PI * (i as f64) / ((n - 1) as f64);
        let w = 0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos()
            - 0.01168 * (3.0 * t).cos();
        *tap = (sinc * w) as f32;
    }
    // Normalise DC gain to 1 (×2 on the interpolation side).
    let sum: f32 = h.iter().sum();
    for tap in h.iter_mut() {
        *tap /= sum;
    }
    h
}
```

`half = 15` (31 taps) gives roughly −70 dB stopband with a transition band suitable for 2×. Assert
the stopband in a test rather than trusting that sentence.

### The cost, honestly

2× oversampling a filter costs: 2× the filter, plus the interpolator, plus the decimator. With a
cheap polyphase IIR pair that is around 2.3–2.6× the original filter cost. With a 31-tap FIR pair
run naively it is more like 4×. Budget accordingly, and see [04-efficiency.md](04-efficiency.md).

---

## 3.7 Antiderivative antialiasing (ADAA)

The best cost/benefit trick available for memoryless nonlinearities, from Parker, Zavalishin & Le
Bivic (DAFx-16), formalised by Bilbao, Esqueda, Parker & Välimäki (2017).

**Idea.** Instead of point-sampling `f(x)`, integrate `f` over the segment between the last input
and this one — i.e. convolve the continuous-time nonlinearity with a box, which is a lowpass, which
suppresses the high harmonics *before* they can fold. If `F' = f`, the first-order form is:

```
    y[n] = ( F(x[n]) − F(x[n−1]) ) / ( x[n] − x[n−1] )
```

with a fallback to `f((x[n]+x[n−1])/2)` when the denominator is small (it is `0/0` in the limit and
catastrophically ill-conditioned before that).

For `tanh`, `F(x) = ln(cosh x)`, which needs a numerically stable form or it overflows around
`x ≈ 89`:

```rust
/// ln(cosh x), stable for large |x|.
#[inline]
fn ln_cosh(x: f32) -> f32 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f32::consts::LN_2
}

/// First-order antiderivative-antialiased tanh.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdaaTanh {
    x1: f32,
    f1: f32,
}

impl AdaaTanh {
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let f = ln_cosh(x);
        let dx = x - self.x1;
        // 1e-3, not 1e-5 — see the note below. This threshold is the whole ballgame.
        let y = if dx.abs() < 1.0e-3 {
            (0.5 * (x + self.x1)).tanh()
        } else {
            (f - self.f1) / dx
        };
        self.x1 = x;
        self.f1 = f;
        y
    }
}
```

**The fallback threshold is the part that bites, and everyone gets it wrong.** `(F(x) − F(x₁))/Δx`
is a difference of two similar `f32` values divided by a small number — textbook catastrophic
cancellation. With `F ≈ 2.3` and `f32`'s ~1e-7 relative precision, the absolute error in the
numerator is ~1.4e-7, so the error in `y` is about `1.4e-7/Δx`. At a threshold of `1e-5` that is a
**1.4% error** — and it happens exactly at signal peaks, where `Δx → 0`.

Measured: a slow 3-amplitude sine through the version above with a `1e-5` threshold deviates from
true `tanh` by **0.0129**; with `1e-3` it drops below `0.001`. The fallback's own error is
`≈ f''·Δx²/24 ≈ 4e-8`, i.e. nothing. So raise the threshold: `1e-3` for `f32`, and only go lower if
you compute `F` in `f64`.

**What you get.** Roughly the alias suppression of 2× oversampling for first order, 4× for second
order, at a small fraction of the cost.

**What it costs you, and the caveats nobody mentions first:**

- **Half a sample of delay.** The box average is centred between samples. Outside a loop this is
  irrelevant; *inside a resonant feedback loop it is extra phase*, and it will shift your resonance
  and threshold exactly like the delayed-feedback mistake in §3.3(a). Prefer ADAA on nonlinearities
  that are **not** inside the ZDF loop — an input drive stage, an output stage, a waveshaper.
- **It is memoryless-only.** Inside a stateful ZDF loop it does not apply directly. There is a
  literature on ADAA for stateful systems, but it is not a drop-in.
- **The ill-conditioned branch is a real branch** in the audio path. Keep it cheap; the fallback
  path is taken often on quiet signals.
- **Under-shoots on sustained loud signals.** The averaging slightly softens the curve, so a
  hard-driven ADAA `tanh` is a touch politer than the real one.

**Where to use it in a synth:** oscillator waveshaping, the input drive stage, the output stage,
wavefolders. **Where not to:** inside the resonance loop you carefully arranged to have no extra
delay.

---

## 3.8 The small stuff that makes it alive

None of this shows up on a frequency plot and all of it is audible.

### Noise floor and seeding self-oscillation

A digital filter at rest is *exactly* zero. It will never start oscillating, because `0 × anything`
is `0`, whereas a real filter has thermal noise to grow from. Inject a tiny amount of noise — this
repo uses `1e-6` (about −120 dB) above a resonance threshold of 0.9, which is inaudible against any
real signal and enough to seed oscillation. Use a deterministic RNG so tests are reproducible.

### Component tolerance, per voice

Real hardware has ±1–5% resistors and capacitors, so no two voices of a Prophet-5 have quite the
same filter. Give each voice a fixed random offset — `±1%` on cutoff, `±2%` on resonance, seeded per
voice at note-on or (better) fixed per voice slot so a held chord is stable. This is the single
cheapest thing that makes a polyphonic VA patch stop sounding like one voice played five times.

### Drift

Slow, correlated random walk on cutoff (a few cents, sub-Hz bandwidth) models thermal drift. Cheap:
a one-pole lowpass at 0.1 Hz on white noise. Do not overdo it — audible pitch drift on a
self-oscillating filter reads as a bug.

### Brightness that follows amplitude

Not a nonlinearity, but it belongs with the rest of this section because it does the same job for
almost no cost. Drive the filter cutoff partly from the amplitude envelope, so quiet means dark.
Every struck or plucked physical object does this, and coupling the two is what makes a synthesized
note read as an object rather than a tone with a fade on it.

Measured on a low-pass gate: brightness retained at −20 dB is **9 %** with the coupling and **96 %**
without — `research:filters/machines/buchla-lowpass-gate.md` §3. One multiply.

### Saturation of the *control* path

Real VCAs and OTAs have limited control range and their exponential converters are imperfect at the
extremes. A gentle compression of the cutoff modulation at very high envelope amounts is more
faithful than a perfectly linear-in-octaves response, and it stops envelope-amount extremes from
sounding brittle.

---

## 3.9 Control feel is part of the character

Half the complaints about software filters are actually complaints about mapping.

- **Cutoff must be exponential.** Map the parameter to `Hz` with a skewed range (this project's
  `FloatRange::Skewed`), not linearly. 20 Hz–20 kHz linear is unplayable: the bottom seven octaves
  live in the first 5% of the knob.
- **Resonance must be usable across its whole range.** If the top 30% is all self-oscillation and
  the bottom 40% does nothing audible, the control is broken. Map resonance to `k` so that the
  interesting region — the last 6 dB before threshold — occupies real knob travel. Measure where
  your threshold actually is (§6.4) and map around the measurement, not the theory.
- **Key tracking.** Cutoff should optionally follow note pitch, 0–100%, in octaves. Without it, a
  filtered patch changes timbre completely across the keyboard.
- **Envelope amount in octaves, and bipolar.** Negative envelope amounts are a real sound.
- **Drive and level interact.** If input drive changes output level dramatically, players will not
  use it. Compensate roughly (`out /= 1 + 0.5·drive`) so the control changes tone more than volume.

---

## 3.10 Checklist for a filter that has character

- [ ] Nonlinearity in the feedback path, and the boundedness argument written down
- [ ] Loop solved, not delayed — self-oscillation threshold measured and cutoff-independent
- [ ] Passband droop present (not "fixed") or compensated deliberately and documented
- [ ] Saturator bounded in floating point, monotonic, and smooth
- [ ] Asymmetry (if any) with DC blocked at the output only
- [ ] Aliasing measured at the worst-case preset, not assumed
- [ ] Noise seeding so it can self-oscillate from silence
- [ ] Per-voice component variation
- [ ] Cutoff exponential, resonance mapped around the measured threshold, key tracking present
- [ ] Denormals flushed on every recursive state

---

[← topologies](02-topologies.md) · [index](README.md) · [next: efficiency →](04-efficiency.md)
