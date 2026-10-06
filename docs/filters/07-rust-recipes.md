# 7. Rust recipes and a plan for this repo

[← testing](06-testing.md) · [index](README.md) · [sources →](08-sources.md)

---

## 7.1 API shape

> The filter trait below is the per-sample primitive. For the **user-facing voicing layer** that
> sits above it — normalised resonance, the core/family split, and the advanced panel — see
> [09-voicing.md](09-voicing.md).


Three constraints from the AGENTS.md chain drive the design: the DSP crate must contain no nice-plug types,
`process()` must not allocate or branch unpredictably, and DSP must never read editor state.

```rust
/// A per-sample filter. Coefficients are set separately from processing so the
/// caller controls the update rate (04-efficiency.md §4.2).
pub trait Filter {
    type Out;

    /// Recompute coefficients. Called at a control-block boundary, or per sample
    /// when the cutoff is being modulated at audio rate.
    fn set(&mut self, cutoff_hz: f32, resonance: f32, sample_rate: f32);

    /// One sample. Must be allocation-free, branch-light and inlinable.
    fn process(&mut self, x: f32) -> Self::Out;

    /// Clear all state. Must leave no tail — the `reset()` contract in plugins/AGENTS.md.
    fn reset(&mut self);
}
```

Deliberately **not** in the trait:

- **`sample_rate` in `process`.** Passing it per sample invites recomputation per sample. Take it in
  `set`, where the coefficient work lives. (The current `Ladder::process` in this repo takes cutoff,
  resonance *and* sample rate per sample — see §7.5.)
- **Block processing.** A `process_block` is tempting but it fixes the coefficient update at the
  block boundary. Keep the sample-level primitive and let the voice own the blocking.
- **`dyn` anything.** Monomorphise; §4.9.

### Generic over the saturator

```rust
pub trait Saturator: Copy {
    /// The curve. Must be bounded, monotonic, and exactly bounded in f32.
    fn f(self, x: f32) -> f32;
    /// Its derivative — for the Newton solve. Differentiate the *approximant*,
    /// not the ideal function, or Newton converges a hair off.
    fn df(self, x: f32) -> f32;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TanhSat;

impl Saturator for TanhSat {
    #[inline]
    fn f(self, x: f32) -> f32 {
        tanh_approx(x)
    }
    #[inline]
    fn df(self, x: f32) -> f32 {
        let t = tanh_approx(x);
        1.0 - t * t
    }
}

/// The SH-101 flavour: back-to-back diodes rather than a smooth curve.
#[derive(Debug, Clone, Copy)]
pub struct DiodeSat {
    pub knee: f32,
}
```

A `Ladder<S: Saturator>` compiles to exactly the code you would write by hand, and lets you A/B
curves — which, per chapter 3, is the highest-leverage experiment available.

---

## 7.2 Parameter mapping

The mapping is part of the sound (§3.9). Keep it in the DSP crate as plain functions so it is
testable, and have the plugin's `params.rs` call into it rather than duplicating the maths.

```rust
/// Normalised 0..=1 → cutoff in Hz, exponential over the audible range.
#[inline]
pub fn cutoff_hz(norm: f32) -> f32 {
    const LO: f32 = 20.0;
    const HI: f32 = 20_000.0;
    LO * (HI / LO).powf(norm.clamp(0.0, 1.0))
}

/// Combine base cutoff with modulation expressed in octaves, then clamp once.
/// Everything — envelope, LFO, key tracking, velocity — is an octave offset, so
/// they add before the exponential and there is a single clamp at the end.
#[inline]
pub fn modulated_cutoff(base_hz: f32, octaves: f32, sample_rate: f32) -> f32 {
    (base_hz * exp2_approx(octaves)).clamp(20.0, 0.45 * sample_rate)
}

/// Key tracking: 0 = fixed cutoff, 1 = cutoff follows pitch exactly.
#[inline]
pub fn key_track_octaves(note: u8, centre_note: u8, amount: f32) -> f32 {
    amount * (note as f32 - centre_note as f32) / 12.0
}
```

Two rules that are easy to get wrong:

- **Sum modulation in octaves, exponentiate once, clamp once.** Clamping intermediate values makes
  modulation depth behave differently depending on where the base cutoff sits.
- **Map resonance around the *measured* threshold.** `crates/mxm-mono-01-dsp/src/filter.rs` sets
  `K_MAX = 4.5` against a measured threshold of ~4.00, so the top ~11% of the control is past
  oscillation. That is a deliberate, documented choice; make it deliberately.

---

## 7.3 Voice-level blocking

```rust
const MAX_BLOCK: usize = 32;

pub fn render(&mut self, out: &mut [f32], sample_rate: f32) {
    let mut start = 0;
    while start < out.len() {
        let end = (start + MAX_BLOCK).min(out.len());

        // Control-rate work, once per block.
        let cutoff = modulated_cutoff(self.base_cutoff, self.mod_octaves(), sample_rate);
        self.filter.set(cutoff, self.resonance, sample_rate);

        // Audio-rate work: no coefficient computation, no branches.
        for sample in &mut out[start..end] {
            *sample = self.filter.process(self.osc.next());
        }
        start = end;
    }
}
```

Host events split blocks too, so in the real `process()` the boundary is
`min(next_event, host_end, start + MAX_BLOCK)` — which [`plugins/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/AGENTS.md) already
requires for a different reason.

**When you need per-sample coefficients** (audio-rate filter FM), keep this structure but move
`set` inside the inner loop, and make sure `set` uses `tan_approx` (§4.4) rather than libm.

---

## 7.4 An honest cost menu

Everything in chapters 2–3 has a price. Pick from this deliberately rather than by accretion.

| Feature | Cost | Musical return |
|---|---|---|
| TPT one-pole HPF in front | ~5 flops | High — the Juno/Jupiter architecture, and it fixes muddy patches. Not an SH-101 feature |
| Pole mixing on an existing ladder | ~9 flops | **Very high** — 15 modes, all resonant |
| Diode clamp instead of `tanh` in feedback | ~0 | **Very high** — this is the SH-101's identity |
| Per-stage saturation | 4 × saturator + a 4-unknown solve | **High** — measured at −40 semitones of level-dependent cutoff against −10 for feedback-only (Moog `research:filters/machines/moog-transistor-ladder.md` §10). Was ranked medium here; that was wrong |
| Newton solve instead of delayed feedback | ~3× filter cost | **Essential** — tuning and threshold |
| Drift and per-voice tolerance | negligible | High on polyphonic, medium on mono |
| 2× oversampling | ~2.4× | Low, *unless* the preset is bright + driven |
| ADAA on input/output drive stages | ~2× that stage | Medium — cheap alias reduction where it is safe |
| Full 5-unknown implicit solve | ~2× the Newton solve | Low — spend it on the feedback node instead |

---

## 7.5 Where mxm-mono-01 stands, and what is next

The existing `crates/mxm-mono-01-dsp/src/filter.rs` is in good shape: TPT one-poles, a **diode
clamp** in the feedback path (since 2026-09-03; a plain `tanh` before that), a three-step Newton
solve with a written-down monotonicity argument, a measured oscillation threshold of 4.00 across
four sample rates, explicit denormal flushing, deterministic excitation for self-oscillation from
silence, and a documented record of two cheaper designs that were measured and rejected. That is a
better foundation than most shipping VA filters.

Gaps worth closing, roughly in value order:

1. **Feedback saturator — closed, 2026-09-03.** It was a plain `tanh`; the SH-101's distinguishing
   feature is *back-to-back diodes in the resonance feedback loop*, and the crate now carries the
   diode-clamp curve from `research:filters/machines/ir3109-roland.md` §7 with the measured
   before-and-after in that document's §11 and in the crate's AGENTS.md. The saturator was **not**
   made generic (§7.1): one curve, one constant, the knee recorded as chosen. What remains from this
   item is fitting the knee to a bench measurement.
2. **Q compensation.** There is none, so the filter droops like a Moog (`1/(1+k)`, −14 dB at
   `k = 4`) where every Roland IR3109 design boosts level back as resonance rises. One multiply.
   Choosing *input*-side versus *output*-side compensation also decides whether turning up resonance
   turns up distortion — see `research:filters/machines/ir3109-roland.md` §4.
   **Note:** a non-resonant highpass in front would be a Juno/Jupiter feature, not an SH-101 one —
   the SH-101 has no HPF. Add it if you want it, but call it an MXM addition in the brief.
3. **Divisions per sample.** `process` currently does `1.0/(1.0+g)` plus three Newton divisions
   every sample — four divisions, the most expensive scalar op in the loop. `1/(1+g)` can be folded
   into the coefficient update if the API moves cutoff into a `set` (§7.1); the Newton divisions are
   load-bearing and should stay.
4. **Pole mixing.** The ladder already computes all four stage outputs in its final loop and throws
   three of them away. Returning `[f32; 5]` and adding a mix vector is nine flops for fourteen extra
   filter modes. Whether mxm-mono-01 *exposes* them is a design-brief question — the SH-101 has one
   filter mode and the design system forbids copying the panel, not the DSP — but the capability
   should exist in the crate.
5. **Aliasing measurement.** There is no test for it (§6.5). Measure before deciding whether
   oversampling is needed; the answer for a 24 dB lowpass with feedback-only saturation may well be
   "no".
6. **Doc drift.** `measure_oscillation_threshold`'s comment still explains itself in terms of "the
   unit-delay feedback", which the Newton solve replaced. Worth a pass.
7. **Drift.** Even on a monosynth, a slow sub-Hz wander of a few cents on the cutoff is audible as
   life, especially at self-oscillation.

---

## 7.6 Where the code should live

The root [`AGENTS.md`](../../AGENTS.md) is explicit and the reasoning holds here: `crates/ui` is shared from day one,
`crates/<plugin>-dsp` is **per-plugin and stays that way** until a second instrument demonstrates a
genuinely shared API.

So: **do not create `crates/filters`.** Build the pole-mixing ladder, the SVF and the one-pole inside
`mxm-mono-01-dsp`. When a second instrument needs an SVF — an MXM-80 (CS-80) would, immediately, since
it is two 2-pole state-variable sections — that is the moment to look at what the two actually share
and extract it with knowledge rather than guesswork. A filter extracted for one consumer will have
the wrong API for the second one.

What a few plausible future instruments would need, as evidence about what the shared API would have
to support:

| Instrument | Filter requirement |
|---|---|
| MXM-5 / MXM-1 (Prophet-5 / Pro-One) | The gm-C core: saturation on **both sides** of every integrator, an arrowhead Newton solve, and the user-editable voicing layer of `research:filters/machines/ssm2040-cem3320-prophet.md` §12 |
| MXM-80 (Yamaha CS-80) | Two 2-pole SVFs, HP + LP, **damping clamped away from zero** so it cannot self-oscillate, Q as a function of cutoff (§5.7) |
| MXM-20 (Korg MS-20) | 2-pole HP + 2-pole LP, hard asymmetric saturation on the damping path (§2.4, §5.4) |
| MXM-303-alike | Diode-ladder coupling, high oscillation threshold, and — more importantly — the envelope/accent circuit (§5.3) |
| Any polysynth | Per-voice tolerance and drift; SIMD across voices, which means branch-free filter code (§4.8) |

Three of those four want a *different* nonlinearity in a *different* place on a *similar* linear
core. That is the shape of the eventual shared API: a small set of linear cores, generic over a
saturator, with the placement chosen by the plugin — not a `Filter` enum with fifteen variants.

---

## 7.7 A note on licences and provenance

The root [`AGENTS.md`](../../AGENTS.md) requires recording source and licence at the top of any
file whose algorithm was
borrowed, and citing techniques even when the implementation is original. For filters specifically:

- **Techniques to cite even when writing from scratch:** TPT/ZDF (Zavalishin), the trapezoidal SVF
  (Simper), the nonlinear ladder (Huovilainen), ADAA (Parker/Zavalishin/Le Bivic; Bilbao et al.),
  pole mixing (Oberheim, prior art in hardware).
- **Do not port from VCV Rack.** It is GPL-3.0 and copying from it forces the plugin to GPL. Decide
  before copying, not after.
- Published *equations* in papers are not copyrightable; a specific *implementation* is. Read the
  paper, write your own code.
- Naming: these are homages. Per the root [`AGENTS.md`](../../AGENTS.md), never use the original manufacturer's name or model
  designation as a product name, and that applies to parameter labels and preset names too. A filter
  mode called "Ladder" is fine; one called "Moog" is not.

---

[← testing](06-testing.md) · [index](README.md) · [sources →](08-sources.md)
