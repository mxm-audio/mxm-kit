# 4. Efficiency

[← nonlinearity](03-nonlinearity.md) · [index](README.md) · [next: seminal machines →](05-seminal-machines.md)

The budget: a 16-voice synth at 48 kHz on one core, leaving room for the twenty other plugins in the
session. That is on the order of a few hundred nanoseconds per sample for *everything* — oscillators,
filters, envelopes, LFOs, mixing. The filter is usually the second most expensive thing after the
oscillators, and the first if it is oversampled.

---

## 4.1 What actually costs

Rough shape on a modern x86-64 or Apple Silicon core. Do not treat these as numbers; treat them as
an ordering.

| Operation | Relative cost | Notes |
|---|---|---|
| `+`, `−`, `*`, FMA | 1 | Pipelined, several per cycle. Effectively free in bulk. |
| Compare / select | 1–2 | Fine if it compiles to a `cmov`/`select`, not a branch. |
| `/` | ~10–20× | Poorly pipelined. Hoist out of the sample loop. |
| `sqrt` | ~10–20× | Same. |
| `tan`, `exp`, `ln`, `tanh` (libm) | ~20–50× | A real function call in some builds. This is why §4.4 exists. |
| **Unpredictable branch** | ~15–40× when mispredicted | The reason fixed-iteration Newton beats convergence testing. |
| **Denormal arithmetic** | up to ~100× | Microcode assist. §4.6. |
| L2/L3 cache miss | ~50–200× | Rare in a filter, common in a voice array laid out badly. |

Two consequences that shape every decision below:

1. **Move everything transcendental and every division out of the per-sample path** and into the
   coefficient update.
2. **Then decide how often the coefficient update runs**, because that is now the real cost knob.

---

## 4.2 Coefficient update rate

This is the single biggest performance lever in a VA filter, and it is a *quality* decision, not
just a speed one.

| Rate | Cost | When it is right |
|---|---|---|
| Per block (once per `process` call) | Negligible | Never for a synth VCF. A 512-sample block at 48 kHz is 10.7 ms of frozen cutoff — audible stepping on any envelope sweep. |
| Per control block (16–64 samples) | Cheap | The default. 32 samples = 0.67 ms; a full-range filter sweep in 20 ms crosses ~30 steps, which is smooth if you also smooth the parameter. |
| Per sample | One `tan` + one divide per sample | Required for audio-rate cutoff modulation (filter FM), and for anything where the modulator is itself an audio-rate signal. |

The practical architecture: **split the buffer into control blocks, and cap them.** This project's
`plugins/AGENTS.md` ([`plugin-conventions.md`](../plugin-conventions.md#realtime-rules-for-process)) already mandates the cap for a different reason (an event-free buffer must not become
one arbitrarily long block); the same split gives the control rate.

```rust
const MAX_BLOCK: usize = 32;

// In process(): block boundaries are min(next_event, host_end, start + MAX_BLOCK).
// At each boundary: read smoothed params, recompute filter coefficients once,
// then run MAX_BLOCK samples through a tight loop with no coefficient work.
```

If you need per-sample coefficients, you need §4.4 — a `tan` approximation — and you should reuse
one prewarp across all stages of a ladder rather than computing four.

**Parameter smoothing vs coefficient smoothing.** Smooth the *parameter* (cutoff in Hz, or better,
in log-frequency) and derive coefficients from the smoothed value. Do not smooth `g` directly: `g`
is a tangent, so linear interpolation in `g` is not linear in frequency and the sweep will sound
uneven at the top.

---

## 4.3 Per-sample operation counts

Measured by reading the code, not the profiler — these are the arithmetic in the inner loop after
coefficients are hoisted.

| Filter | mul | add | div | transcendental | states |
|---|---|---|---|---|---|
| `OnePole` (§2.1) | 1 | 3 | 0 | 0 | 1 |
| TPT SVF (§2.2), one output | 5 | 7 | 0 | 0 | 2 |
| TPT SVF, all three outputs | 7 | 9 | 0 | 0 | 2 |
| Linear ladder + 5 taps (§2.3) | ~16 | ~16 | 0 | 0 | 4 |
| Ladder + pole mix (§2.6) | +5 | +4 | 0 | 0 | — |
| Nonlinear ladder, 3 Newton steps | ~45 | ~40 | 3 | 3 × `tanh` approx | 5 |
| Same, with per-stage `tanh` too | ~75 | ~65 | 3 | 15 × `tanh` approx | 5 |
| Any of the above, 2× oversampled | ×2 | ×2 | — | ×2 | + resampler state |

The jump from "linear ladder" to "nonlinear ladder with a proper solve" is roughly 3×. The jump to
2× oversampling on top is another 2.3–2.6× including resampling filters. **A fully-loaded ladder is
~7× a linear one**, so decide deliberately what you are buying.

---

## 4.4 Approximating `tan`

Needed once per coefficient update; if that is per-sample, it dominates.

The rational (Padé) approach is the right one because its pole can be placed *exactly* at `π/2`,
where `tan`'s pole is — polynomial approximations blow up before or after the pole and mistune the
top octave. From this repo's `crates/mxm-mono-01-dsp/src/filter.rs` (in mxm-mono-01 since the split):

```rust
/// `tan(x)` via the [5/4] Padé approximant, for `x` in `[0, PI * 0.45]`.
/// Better than 1e-4 relative across the range; pole at PI/2, exactly where tan's is.
/// Evaluated in f64 because numerator and denominator both lose significance near the top.
#[inline]
pub fn tan_approx(x: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;
    x * (945.0 - 105.0 * x2 + x4) / (945.0 - 420.0 * x2 + 15.0 * x4)
}
```

Cost: 6 multiplies, 4 adds, 1 divide — roughly 5× cheaper than libm `tan`, and it is inlinable and
branch-free, which matters more than the raw count.

**Alternatives:**

- **Table + linear interpolation** in `log2(fc)`. Fast, but the table has to be large enough that
  interpolation error stays below a cent, and it is a cache resident you did not want. Worth it only
  if you also need `exp2` for the pitch mapping and can share the table.
- **Simper's sine formulation.** Andy Simper has SVF variants parameterised by `sin`/`cos` rather
  than `tan`, which are better conditioned very near Nyquist. Worth reading if you need the last
  half-octave.
- **Incremental update.** If cutoff changes slowly, update `g` by a multiplicative delta rather than
  recomputing. Fragile — drifts, and breaks on parameter jumps. Not recommended.

**Accuracy target.** 1 cent of pitch is a relative frequency error of `2^(1/1200) − 1 ≈ 5.8e-4`. A
relative error of `1e-4` in `g` is comfortably inside that. Do not chase more.

---

## 4.5 Approximating `tanh`

Runs 1–15 times per sample. Requirements from §3.4: bounded exactly, monotonic, smooth.

```rust
/// `tanh(x)` via the [7/6] Padé approximant, with the input clamped.
/// The clamp is load-bearing, not defensive: without it the rational form diverges
/// for large x, breaking the boundedness argument the filter rests on.
/// tanh(4) = 0.9993, so the curve is flat where the clamp takes over.
#[inline]
pub fn tanh_approx(x: f32) -> f32 {
    let x = x.clamp(-4.0, 4.0);
    let x2 = x * x;
    let num = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let den = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    (num / den).clamp(-1.0, 1.0)
}
```

The outer clamp exists so `|tanh_approx(x)| ≤ 1` is exactly true in `f32`, not nearly true. Every
boundedness proof downstream depends on it.

**On cheaper forms.** A [3/2] Padé is a few multiplies cheaper and deviates by up to 2.4% around
`x = 1.5` — right in the middle of where a driven filter operates. That is audible as a different
saturation colour. Accuracy in a saturator is not about matching `tanh`; it is about the curve you
chose being the curve you get.

**Derivative for Newton.** `tanh'(x) = 1 − tanh²(x)`, so compute the value once and square it. If
you use a Padé approximant, differentiate the *approximant*, not `tanh`, or Newton converges to the
wrong root by a hair.

---

## 4.6 Denormals

A filter decaying into silence passes through the denormal range on its way to zero. On x86-64 a
denormal operand triggers a microcode assist costing tens to ~100× a normal operation, and a
sustained tail can put a whole voice into that state. It presents as a CPU spike *after* the notes
stop — the most confusing possible symptom.

Two mitigations, and this project mandates the second:

1. **FTZ/DAZ in the MXCSR register.** Set flush-to-zero and denormals-are-zero at the top of
   `process`. Effective, but: it is process-wide state you are mutating in someone else's host, some
   hosts set it and some do not, it is architecture-specific, and on ARM the equivalent (`FZ` in
   `FPCR`) has slightly different semantics.
2. **Flush explicitly in the DSP.** [`crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md)
   requires this, and it is the right call: it is
   portable, it does not touch global state, and — the real reason — it preserves *exact digital
   silence*. A filter that flushes its own state actually reaches zero and stays there.

```rust
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}
```

Apply on **every recursive state** — every integrator `s`, every feedback memory, every delay-line
write. Not on intermediate values, which are about to be multiplied by something normal anyway.

The comparison compiles to a compare and a select, not a branch, and it is well predicted regardless.
Cost: about one operation. Cost of not doing it: occasionally 100.

**Do not rely on a framework's FTZ guard.** It may be a no-op without an opt-in feature, and it does
not give you exact silence.

---

## 4.7 `f32` versus `f64`

The project rule — `f32` in the audio path, `f64` for filter coefficients and anything recursive
where precision loss compounds — is the right one. The specifics:

- **Prewarping in `f64`.** `tan` near `π/2` loses significance fast, and both the numerator and
  denominator of a Padé approximant shrink. Compute `g` in `f64`, store `G` as `f32`.
- **Integrator states in `f32` are fine** for a lowpass down to ~20 Hz. Below that, or in a very
  high-Q bandpass at low frequency, the state is a small difference of large numbers and `f64` pays.
  Symptom: quantisation noise that changes with cutoff.
- **Delay lines and comb feedback in `f32`.** The error does not compound the same way.
- **`f64` costs you SIMD width**: 2 lanes instead of 4 on SSE, 4 instead of 8 on AVX. If you are
  going to vectorise across voices, that halves your throughput. Prefer fixing the algorithm over
  widening the type.

---

## 4.8 SIMD

The right axis is **across voices**, not across samples — a recursive filter cannot be vectorised
along time, since sample `n` depends on `n−1`.

Process 4 voices (SSE/NEON) or 8 (AVX2) in lockstep: each lane holds one voice's state, coefficients
are per-lane vectors, and the whole voice runs vector-wide.

What this demands of the filter code:

- **No branches.** Every lane executes every path. This is another argument for fixed-iteration
  Newton: a convergence test would force you to run until the *worst* lane converges anyway.
- **Structure-of-arrays state.** `[f32; 4]` per lane laid out as four separate `f32x4`s, not an
  array of structs.
- **Uniform work.** Voices in different states (releasing vs sustaining) still cost full price.
  Gather/scatter to keep active voices packed is possible but often not worth the shuffling.
- **The clamps and selects must be vectorised too** — `clamp` maps to `min`/`max`, `flush` maps to a
  compare and a blend. Both are fine.

In Rust: `std::simd` (nightly), or `wide`, or hand-written `core::arch` intrinsics behind a
`cfg(target_arch)` with a scalar fallback. Do not do this speculatively. It is a 2–4× win on the
filter and a large complexity cost; earn it with a profile first.

---

## 4.9 Dispatch, generics and the inner loop

- **No `dyn` in the audio path.** A virtual call per sample per voice is a branch the predictor
  cannot help with and an inlining barrier.
- **Dispatch per block, not per sample** — but know why. The rule is about **unpredictable**
  branches. A mode enum that is constant across a buffer is predicted perfectly: measured at
  **+0.01 ns/sample (+0.0 %)** against calling the specialised path directly
  ([09 §9.7](09-voicing.md#97-what-it-costs--and-why-this-is-not-one-do-everything-filter)).
  Hoisting it is tidier and costs nothing, but it is not a performance fix. Spend the effort on
  iteration counts instead, where one Newton step is worth ~32 % of a 4-pole filter. With pole
  mixing (§2.6) you do not need the match at all — the mode is data.
- **Generics over a `Saturator` trait** monomorphise cleanly and cost nothing:

  ```rust
  pub trait Saturator: Copy {
      fn f(self, x: f32) -> f32;
      fn df(self, x: f32) -> f32;
  }
  ```

  A ladder generic over `S: Saturator` compiles to the same code as one with `tanh` hardcoded, and
  lets you A/B curves without duplicating the solver.
- **`#[inline]` the per-sample functions**, and check it happened. Across crate boundaries (which
  `crates/<plugin>-dsp` is) `#[inline]` is what makes cross-crate inlining possible at all without
  LTO. **Trait impls need it too**: measured, adding `#[inline]` to the `Voiceable` methods in
  [09 §9.7](09-voicing.md#97-what-it-costs--and-why-this-is-not-one-do-everything-filter) cut a
  wrapper's overhead from **+14.05 ns to +3.00 ns per sample**.
- **Keep measurement instrumentation out of the audio path.** A `process` that also computes
  convergence statistics is doing a third more work than it needs to, for a number nothing reads.
  Separate the two functions; this repo shipped that mistake once and §9.7 records it.
- **Enable LTO and `codegen-units = 1` in the release profile.** For DSP this is routinely 10–20%.

---

## 4.10 How to actually measure

- **Benchmark the DSP crate standalone.** It is framework-free by design (see its
  [`AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md)), so a
  criterion bench or a plain `examples/` binary rendering N seconds and timing it works, with no host
  involved. `crates/mxm-mono-01-dsp/examples/mono_01_filter_spike.rs` is already this shape.
- **Benchmark the worst case, not the average**: all voices active, resonance at maximum (Newton
  works hardest and the loop is loudest), cutoff swept at audio rate, oversampling on.
- **Check for denormals separately**: render a few seconds, stop all notes, and keep rendering
  silence for several more seconds while timing. A denormal problem shows as the *silent* section
  being slower than the loud one.
- **Then measure in a host.** Buffer sizes, thread priorities and the host's own denormal settings
  all differ from your bench. `clap-validator` first, then a real DAW at 64 samples.
- **`assert_process_allocs` only fires in debug builds.** A clean release validator run proves
  nothing about allocation. Run the debug build too.

---

[← nonlinearity](03-nonlinearity.md) · [index](README.md) · [next: seminal machines →](05-seminal-machines.md)
