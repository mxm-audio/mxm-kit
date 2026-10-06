# In-repo DSP measurement index

Every figure measured by this repository in this reference, what produced it, and where it is quoted.
The point is auditability: a reader who doubts a number should be able to find the code that made it
and the conditions it was made under without reading the whole reference.

This index does not absorb measurements reported by outside hardware sources. For example, the
Buchla 208 appendix attributes one restored unit's oscillator ranges to Dave Brown and keeps his
conditions and revision caveat beside them; those are evidence from that source, not reproducible
results from `osc_spike`.

## How to reproduce all of it

```bash
cargo run -p dsp-lab --release --example osc_spike    # in mxm-tools, since the split
```

About 25 seconds. The output of the run these documents quote is recorded verbatim in
[`measurements-run.txt`](measurements-run.txt), so a fresh run can be diffed against it rather than
compared by eye. **§9e's and §9f's cost figures come from a second run on a second machine**,
recorded in [`measurements-run-13900k.txt`](measurements-run-13900k.txt); the granular cost tables
quote that file and nothing else does.

**Conditions, common to everything unless a chapter says otherwise:** 44.1 kHz; 65536-point FFT;
fundamentals chosen so the waveform is exactly periodic in the analysis window, so no window
function is used and no leakage occurs; 4096 samples discarded before analysis; AMD Ryzen
Threadripper 3970X, Windows 11, `rustc` 1.98.0, release profile.

**The granular cost sections are the exception, and it is a machine, not a setting.** §9e and §9f
were measured on an **Intel i9-13900K**, which runs the cost rows about 2x faster than the
Threadripper. Spectra are bit-identical between the two; nanoseconds are not. Do not compare a
figure from one file against a figure from the other — [10-granular.md](10-granular.md) keeps every
granular comparison inside the second run for exactly this reason.

**Cost figures are means of five runs** and drift a few percent between sessions. They are quoted to
two decimals because that is what the harness prints, not because the last digit means anything. The
wavetable rows are the exception and are cache-bound — the same code has measured 4.2 and 7.9 ns.

## The metrics, and what each can and cannot say

| Metric | What it measures | Where it is invalid |
|---|---|---|
| `alias` | Energy off the wanted harmonic grid over energy on it | Needs a *known* wanted set. Invalid for a detuned stack (§17 uses `analyse_multi` instead) and uninformative where the output is not periodic |
| `A-wtd` | The same, both sides A-weighted | A crude audibility proxy, **not** the noise-to-mask ratio the literature reports |
| `worst` | Loudest single aliased component against the fundamental | Misleading where the fundamental is not the loudest partial — FM and grain trains both do this |
| `h.err` / `gone` | Harmonic amplitude error against an ideal `1/k` saw | Only meaningful where `1/k` is the intended spectrum |
| `vs ref` | Deviation from an oversampled rendering of the same algorithm | **Floor of about −115 dB**, set by the decimation filter. Aggregates aliasing, interpolation error and level error — cannot isolate any one of them |
| `flatness` | Geometric over arithmetic mean of the spectrum | Structurally biased toward "fine" for anything that copies analysis magnitudes, e.g. a phase vocoder |
| `collisions` | Aliases landing on a wanted bin in a multi-series analysis | Counts opportunities, not energy; the colliding harmonics are high-order and quiet |

## Where each figure lives

| Spike § | Measures | Quoted in |
|---|---|---|
| 1 | Sawtooth aliasing against fundamental, nine algorithms | [02 §2.11](02-antialiasing.md#211-the-comparison-table), [01 §1.3](01-fundamentals.md#13-why-trivial-sampling-fails-precisely) |
| 2 | The shipped sawtooth across four sample rates | [07 §7.1](07-rust-recipes.md#71-what-mxm-mono-01-ships) |
| 3 | Pulse aliasing against width | [03 §3.2](03-waveshapes.md#32-pulse-and-pwm) |
| 4 | Sub-oscillator shapes | [03 §3.4](03-waveshapes.md#34-sub-oscillator) |
| 5 | Cost of the nine sawtooth algorithms | [02 §2.11](02-antialiasing.md#211-the-comparison-table) |
| 6 | The pulse-width clamp against pitch | [03 §3.3](03-waveshapes.md#33-the-two-edge-problem) |
| 7 | DC through the shipped blocker | [03 §3.8](03-waveshapes.md#38-dc-and-asymmetry) |
| 8 | Wavetable: interpolation, table length, bank resolution, crossfade, cost | [09](09-wavetable.md) throughout |
| 9 | Granular: envelope, onset placement, overlap-add, cloud statistics, cost of a sine-carrier grain | [10](10-granular.md) throughout |
| 9f | Granular: cost of a **sample-reading** grain — kernel, bounds, window, pool walk, render shape, occupancy and playback rate | [10 §10.6.1](10-granular.md#1061-a-sample-reading-grain-is-a-different-animal-and-it-need-not-be) |
| 10 | Additive: partial purity, resonator drift, cost against partial count, time-stretch | [11 §11.2](11-additive-resynthesis.md#112-three-ways-to-make-a-sine-measured), [11 §11.4](11-additive-resynthesis.md#114-why-it-stretches-so-cleanly) |
| 11 | FM and PM: index, ratio, oversampling, PM against true FM, feedback, cost | [12](12-fm.md) throughout |
| 12 | Phase distortion: the breakpoint, pitch scaling, resonant waveforms, cost | [13](13-phase-distortion.md) throughout |
| 13 | Sampler: converter and level, input rate, reconstruction, transposition | [14](14-samplers.md) throughout |
| 14 | Grain pitch against interpolation | [15 §15.9](15-granular-in-the-wild.md#159-what-our-measurements-say-about-their-feature-lists) |
| 15 | Phase vocoder: round trip, tone stretch, noise stretch | [11 §11.5](11-additive-resynthesis.md#115-the-phase-vocoder-and-where-it-sits) |
| 16 | Hard sync, naive against corrected | [03 §3.9](03-waveshapes.md#39-hard-sync-measured) |
| 17 | Supersaw: voice count, detune spread, start phase, cost | [16](16-supersaw-and-unison.md) throughout |
| 18 | Discrete summation formulae | [02 §2.12](02-antialiasing.md#212-discrete-summation-formulae) |
| 19 | Waveshaping, folding and ADAA | [17](17-waveshaping-and-folding.md) throughout |
| 20 | Ring modulation, and noise colours | [03 §3.10](03-waveshapes.md#310-ring-modulation-and-am), [03 §3.5](03-waveshapes.md#35-noise) |

## What has never been measured

Stated here as well as in [07 §7.6](07-rust-recipes.md#76-open-gaps), because an index that only
lists successes is misleading:

- **No hardware measured by this repository.** Claims about real instruments in
  05 (`research:oscillators/05-machines.md`), [14](14-samplers.md), and the
  208 appendix (`research:oscillators/buchla-208-oscillators.md`) come from service manuals, datasheets, or attributed
  specialist sources. A source-reported hardware measurement remains that source's result.
- **No listening test.** Every quality claim is an energy measurement or a figure quoted from a
  paper.
- **Aliasing under modulation.** All of it is steady tones; the exactly-periodic method requires
  one.
- **Stereo.** The whole reference is mono, which matters most in [16](16-supersaw-and-unison.md).
- **minBLEP, phase locking, inverse-FFT synthesis, inter-bin coherence.** Cited, not implemented.

---

**Naming note.** The instrument referred to as MXM-101 in this document was renamed `mxm-mono-01`
on 2026-08-31. The old name is left in place deliberately: this file records what was measured, on
which build, at the time — renaming the subject retroactively would make it claim a measurement that
never happened. See *Naming* in the root [`AGENTS.md`](../../AGENTS.md).
