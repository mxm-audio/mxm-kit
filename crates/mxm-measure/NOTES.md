# NOTES.md — crates/mxm-measure

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

## Why the crate exists

It exists because nothing in this repository implemented its measurements once. "Magnitude at a
frequency" had been written **twelve times under one name for three different physical quantities**,
so a figure from one crate could not be compared with a figure from another; and two crates asserted
one-cent oscillator tuning using a ruler that quantises to **nine** cents — a defect
`mxm-mono-00-dsp` had already found and fixed, in a comment, where the fix could not travel.

The measurement toolkit in [`docs/filters/06-testing.md`](../../docs/filters/06-testing.md) §6.1 had
existed as a fenced code block nobody compiled. This is that block, compiled, with tests.

## The numeric core that came down from `dsp-lab`

**It came down from `dsp-lab` with the numeric core**, and a reference harness calls it. Not both
of them: `mod_spike` uses `N`, `fft` and `periods_for`, while `ifft`, `princarg`, `a_weight` and
`spectral_flatness_db` have `osc_spike` alone. They moved together because they *were* one core,
and splitting it to satisfy a per-function count would put a second transform back in the
repository — which is the duplication `dsp-lab` had already removed once.

## The functions the gate removed

A first draft had roughly thirty more public functions — a crest factor, a DC offset, exact-silence
and exceedance observations, `power_db`, `db_amplitude`, `hz_note`, frame/second conversions, an RMS
envelope, mid/side, `deinterleave`, `peak_if_clean`, a note-number pitch wrapper, `periodic_length`,
`component_db`, an `f32` flatness, a whole `envelope` module — and **every one had zero callers**, or
one. `interleave` is the sharpest case: its only users are this crate's own tests, and **a function
whose second consumer is its own test makes the gate vacuous**, so it is crate-private. They were written, measured
against the gate, and removed. Being already written is not a consumer.

## A ruler, never a verdict: the reasoning

There is no `assert_no_aliasing` here and there will not be one. A helper that decides whether a
number is acceptable hides the number, and hiding the number is how a threshold gets loosened without
anybody noticing. Thresholds are an argument about one instrument, and
[`docs/oscillators/06-testing.md`](../../docs/oscillators/06-testing.md) §6.4 says where they come
from: the measured value, with headroom, chosen against the failure mode. They belong beside the
assertion they justify.

**There is no "part of the published definition" exception, and a draft that claimed one was
wrong.** `spectral_flatness_db` carried an epsilon twice — absolute, then relative — each time
argued as a necessary guard on `ln(0)`, and each time it was a level somebody chose that changed the
answer. The definition needs no guard: an empty bin sends the geometric mean to zero and the result
to negative infinity, which is correct and which the result forms already admit. **If a guard seems
necessary, the metric is being asked for an answer it does not have** — report absence or an
infinity, and let the caller decide what that is worth.

The only numbers inside a function here that a caller cannot change are **definitional**, in the
strict sense that the format or the arithmetic fixes them. The clearest example used to live here:
the 16-bit WAV encoder's `32767` scale and clamp. **That encoder retired into
[`../mxm-audio-file`](../mxm-audio-file/AGENTS.md) on 2026-09-15**, with every harness that called it;
the rule is now that crate's published `quantise`, byte-identical at 16 bits, and `showcase`'s
truncation figure states it inline so this crate keeps no dependencies.

## The dev-dependency rule's one unshipped exception

Every crate but `dsp-lab` and `mxm-listening` takes this as a `[dev-dependencies]` entry, never a
normal one. Those two take it normally because they *are* measurement — `dsp-lab`'s harnesses, the
listener's interpretation — and neither has a shipped graph to protect. `mxm-listening` is itself a
dev-dependency wherever it is used, so the check below covers both names. **One exception, unshipped:**
`apps/mxm-listener-hud` takes the listener, and through it this crate, normally. It is not on the
check's list, which names shipped packages only; shipping it means adding it there, the check then
fails, and the listener's never-ships rule has to be decided first — the owner deferred that
decision (2026-09-28).

## Why `peak` returns `Option`

**A measurement over a buffer never returns NaN**, on any input including silence and at every rate
in `STRESS_RATES`, and where the buffer holds a non-finite sample it reports **absence**: a DSP
failure must not be laundered into a measurement. NaN propagates silently through a comparison; ±∞
does not. **Scalar conversions are arithmetic and behave like it** — `amplitude_db(NaN)` is NaN, as
`f64::ln` is, and an `Option` on every unit conversion would buy nothing. The invariant is about
what this crate *measures*, not what it can be handed.

**There is no exception for `peak`**, and a draft that claimed one was wrong. A maximum is
arithmetically well defined over a buffer holding a NaN — `f32::max` returns the other operand —
so the draft argued no absence was owed. True, and beside the point: every call site it serves asks
*"is it quiet?"*, and a broken render answering **yes** is the worst thing this crate could ship.
`peak` returns `Option`.

## The ruler this rule cost

A measurement crate containing a filter would eventually be used to check a filter against itself.
`stimulus::additive_saw` is the one permitted near-miss, because it is defined by a closed form rather
than by an algorithm and its role is to be what a shipped oscillator is *not*.

**This rule has already cost a working function.** An autocorrelation pitch estimator was written,
passed its complex-signal test, and was removed: it had one consumer, and
`mxm-creative-sampler-dsp` ships a YIN detector that `dsp-lab/examples/root_spike.rs` exists to
*score*. A YIN here would be a ruler built out of the thing under test. `src/pitch.rs` keeps the
reasoning where the code was.

## The extraction gate: how the count went wrong

This is the root contract's own bar, not a new one. `mxm-preset` waited for five copies and
`mxm-modulation` for three; a measurement crate does not get a lower bar because its subject is tests.

**Count second, compare first.** The hard half is *identical quantity*, and this crate's own plan got
it wrong four times in three review rounds — always the same way, **a group whose total count is large
hiding a member whose count is one.** Twelve `magnitude_at` sites were three quantities with counts of
five, three and one. Splitting a group by quantity re-runs the gate on each piece.

## The declined register

Everything named here is a concept somebody may want. **None of it is an API**, and the counts say
why.

Named here so a second consumer is recognisable when it arrives, and so nobody re-implements one of
these without knowing the others exist. **A name in this list is not an API**; it is a concept, with
the crate that owns its local composition.

| Concept | Consumers | Where it lives |
|---|---|---|
| Energy gain in a band (output energy over input energy, in dB) | 1 | `mxm-mono-pr1-dsp/examples/pro_one_measure.rs` |
| Time until the last sample above an audibility floor | 1 | `mxm-player/plugins/mxm-mono-01/host-tests/tests/behaviour.rs` |
| RT60 from an energy-decay slope | 1 | `mxm-shimmer/examples/shimmer_preset_audit.rs` |
| Residual level in a fixed late window | 1 | `mxm-shimmer-dsp/src/reverb.rs` |
| Ratio between two separated RMS windows | 1 | `mxm-bucket-delay-dsp/examples/feedback_spike.rs` |
| Time until the DSP parks (an activity property, not an acoustic one) | 1 | `mxm-shimmer-dsp/src/lib.rs` |
| Echo density | 1 | `mxm-shimmer-dsp/src/reverb.rs` |
| Stereo width as a dB ratio | 1 | `mxm-shimmer/examples/shimmer_preset_audit.rs` |
| Render digest | 1 | each instrument's `plugins/<plugin>/host-tests/tests/golden_audio.rs` |
| Preset descriptor vectors and their distance law | 2, with **different members and different laws** | `mxm-shimmer`, `mxm-bucket-delay` |
| Block-partition invariance | 3, each over a **different engine's** API | `mxm-shimmer-dsp`, `mxm-grain-fx-dsp`, `mxm-modulation` |
| K-weighted loudness (ITU-R BS.1770) | 1 implementation, two callers: `mxm-listening` and the drum A/B page, which calls the listener's | `mxm-listening/src/prep.rs`; not moved, because no crate that must not depend on the listener needs it yet. The body gating around it is a normalisation law and would stay there anyway |
| Autocorrelation pitch | 0 in tests | removed; `src/pitch.rs` says why |
| Onset detection, **and the windowed peak envelope under it** | 1 | both removed. The envelope looked like a shared primitive until the second candidate was examined: `mxm-shimmer`'s preset audit windows a *stereo* buffer for **energy**, not a mono one for peaks — a different quantity, which is the §5.1 trap again. `apps/mxm-player/tests/t6_sequencer.rs` keeps envelope and detector, three lines over `level::peak` |
| Exact silence, exceedance either way, a NaN finder | 0 | removed; the repository's silence and NaN checks assert **per sample inside the render loop**, where they name the offending sample, and a buffer-level scan would be a worse test rather than a shared one |
| DC offset, crest factor, RMS envelope, mid/side, power decibels | 0 | removed; add one back with its second consumer |
| An impulse and a tone-burst stimulus | 0 | removed. Both looked obviously useful for an impulse response or a reverb tail, and neither had a caller — *obviously useful* is what the gate is for |

Each of the five tail concepts is a composition over `level`, `observe` and `convert` — a windowed
envelope is three lines of `chunks(...).map(peak)` at the call site, which is why the `envelope`
module that briefly existed here is gone too. That is the correct shape, not a consolation: the
±9-cent bug lived in the arithmetic, never in the composition.

## Why the ruler validates itself

Unlike `dsp-lab`, **this crate has unit tests and they are not optional.** A broken shared ruler moves
every number in the repository at once, silently, in the same direction — the one failure a relative
assertion cannot see.

Every function is scored against a signal whose answer is known by construction: a sine's RMS is its
amplitude over √2, a first-order lowpass is −3.01 dB at its corner, an impulse transforms to a flat
spectrum, an additive saw measures clean and a trivial one filthy. **A metric that cannot be
scored against a closed form does not belong here yet.**

## Migrations so far, and the re-derived thresholds

- **A migration onto these rulers is an equivalence or a correction, and the commit says which.** An
  equivalence must not move the number; a correction moves it, says by how much and why, and chases
  every quoted instance. Both kinds have happened: the three filter transfer-gain migrations were
  equivalences, and the WAV encoder and the two frequency estimators were corrections.
- **A threshold derived from a defective ruler is re-derived, not rescaled.** `mxm-mono-01-dsp` and
  `mxm-poly-06-dsp` asserted one cent on a ±9-cent ruler; measured with the corrected one the worst
  case is 0.009 cents, so both now assert a **tenth of a cent** — ninety times tighter, with an order
  of magnitude of headroom.

## The MSRV runs

**MSRV is verified, not asserted** (2026-09-13):

```bash
cargo +1.87.0 build -p mxm-measure     # the library
cargo +1.87.0 test  -p mxm-measure     # and its tests, which are the harder floor
```

Both pass — 67 tests then; **54 now** (re-run on stable and 1.87, 2026-09-15), because the 13 that
proved the WAV encoder left with it for `crates/mxm-audio-file`. The tests matter more than the library here: they are where a newer language
feature slips in unnoticed, because `cargo test` on `stable` accepts it silently. Running the check
found that **two other crates in the collection declare 1.87 and cannot build on it**
(`docs/known-issues.md`), so this is not a formality.
