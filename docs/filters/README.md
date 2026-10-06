# Synthesizer filter design — a working reference

Research notes for building resonant, musical, real-time filters for CLAP/VST plugins. Written for
this repo (Rust, `crates/<plugin>-dsp`, realtime rules from [`../../AGENTS.md`](../../AGENTS.md)),
but the theory is framework-agnostic.

The through-line: **a musical filter is not a frequency-response spec.** An EQ is judged by its
magnitude response. A synth filter is judged by what happens when you sweep it two octaves in
50 ms with the resonance at 90%, drive 12 dB of sawtooth into it, and it has to stay stable, stay
in tune, and sound like it *wants* to do that. Almost everything below follows from that.

## Read in this order

| File | What it covers |
|---|---|
| [01-fundamentals.md](01-fundamentals.md) | Analog prototypes, poles, discretisation, bilinear transform and prewarping, why naive digital structures fail, TPT/ZDF and the delay-free loop |
| [02-topologies.md](02-topologies.md) | One-pole, SVF, ladder, Sallen–Key, diode ladder, pole mixing, comb/allpass/formant/lowpass-gate — with Rust |
| [03-nonlinearity.md](03-nonlinearity.md) | Where saturation goes and why, solving nonlinear feedback, aliasing, oversampling, ADAA, what "character" decomposes into |
| [04-efficiency.md](04-efficiency.md) | Cost model, coefficient update rate, function approximations, denormals, SIMD, `f32` vs `f64`, dispatch |
| [05-seminal-machines.md](05-seminal-machines.md) | Minimoog, SH-101, TB-303, MS-20, SEM, Xpander, Prophet-5, CS-80, VCS3, Buchla LPG, Polivoks — and what is worth stealing from each |
| [06-testing.md](06-testing.md) | Measuring tuning, self-oscillation threshold, THD, aliasing, time-varying stability — as `cargo test` |
| [07-rust-recipes.md](07-rust-recipes.md) | Trait design, parameter mapping, and a concrete plan for mxm-mono-01 and successors |
| [08-sources.md](08-sources.md) | Annotated bibliography — what each source is actually good for |
| [09-voicing.md](09-voicing.md) | **One editable voicing model for every family** — normalised resonance, the core/family split, and how an "advanced" panel is put together |
| `research:filters/machines/` | **Deep-dives**, one per filter family, researched to the level you could build and argue from — twelve of them, in the private research repository (root *Research citations*; in full, [`collection-rules.md`](../collection-rules.md#research-boundary)), indexed by `research:filters/machines/README.md`: Moog ladder, Roland IR3109, the SH-2's discrete BA662 cascade, SSM2040 / CEM3320, diode ladder / TB-303, Korg-35 / MS-20, Oberheim SEM & Xpander, Yamaha CS-80, the Buchla low-pass-gate family with the 208's Cards 10/11 kept distinct, Steiner-Parker, Polivoks, ARP 4012/4072 |

## Status of the code in here

Every Rust type in these documents was compiled and run before publication — **94 tests**, all
passing, across the core filters, the voicing layer of [chapter 9](09-voicing.md), and a model for
each of the eleven machine deep-dives that carry code (now in the research repository). What that verified
is listed at the top of [chapter 6](06-testing.md) and in each deep-dive's measured-results section.

**One model now ships and is measured in the repository itself:** the diode-clamped Roland cascade
of `research:filters/machines/ir3109-roland.md` §7 is `crates/mxm-mono-01-dsp/src/filter.rs`
(since 2026-09-03), and every number the SH-2 deep-dive (`research:filters/machines/ba662-sh-2.md`) quotes comes from
that crate's `mono_01_filter_spike` and `resonance_gain` examples, which anyone who clones this can re-run.
*Since the split (2026-10-06):* that crate, and every `crates/mxm-mono-01-dsp/…` path in these
chapters, is in [mxm-mono-01](https://github.com/mxm-audio/mxm-mono-01); clone that repository to re-run them. `crates/dsp-lab` is in
[mxm-tools](https://github.com/mxm-audio/mxm-tools).
The twelfth deep-dive publishes no code of its own for that reason.

Known limits, stated rather than hidden: the `DiodeLadder` in §2.5 is a *behavioural* model, not
circuit-exact; the SVF bell/shelf mixing coefficients in §2.2 are from Simper's paper and were not
independently checked; and **no real hardware was measured by this repository** — each deep-dive's
conflicts-and-sources sections list exactly which claims come from datasheets or attributed outside
measurements, which come from conflicting secondary sources, and which are chosen rather than fitted.

## The ten things worth knowing up front

1. **Prewarp with `g = tan(π·fc/fs)`.** Any structure whose cutoff coefficient is not a tangent is
   mistuned near Nyquist, and audibly so above ~5 kHz.
2. **Resolve the feedback loop, don't delay it.** A unit delay in a resonant loop makes the
   resonance frequency and the self-oscillation threshold both cutoff-dependent. Measured numbers
   for exactly this failure are in `crates/mxm-mono-01-dsp/src/filter.rs`.
3. **The linear response is not the character.** An ideal transistor ladder, a Prophet-5 SSM2040 and
   a Juno IR3109 share the *same* idealised 4-pole transfer function. What separates them is
   nonlinearity, gain staging and control feel.
4. **Put the nonlinearity in the feedback path.** That is where it is in the circuit and it is the
   only place that bounds a loop past its oscillation threshold.
5. **Bass droop at high resonance is a feature.** `H(s) = 1/((1+s)⁴+k)` loses ~14 dB at DC when
   `k = 4`. Every ladder-derived filter does this. Removing it removes the sound.
6. **Filters alias less than waveshapers** — the lowpass attenuates its own generated harmonics.
   Measured at **−98 dBc at 1×** on a hard-driven 4-pole, with no improvement from 8×
   (deep-dive `research:filters/machines/ssm2040-cem3320-prophet.md` §10). Oversampling a
   nonlinear filter buys **harmonic accuracy, not alias reduction** — at 1× the same filter
   generates only a third of the distortion it should.
7. **Modulate `g` per sample, compute `tan` cheaply.** Audio-rate cutoff modulation is a synthesis
   technique, not an edge case; it demands a per-sample coefficient path.
8. **Denormals cost up to ~100× on a filter tail.** Flush explicitly in the DSP, on every recursive
   state.
9. **Pole mixing turns one 4-pole ladder into 15 filter modes** for the price of five multiply-adds.
   The Oberheim Xpander did it in 1984; almost no software synth does.
10. **Measure everything.** Tuning error, oscillation threshold, THD and aliasing are all cheap to
    assert in a unit test, and all four regress silently otherwise.
