# AGENTS.md — crates/mxm-measure

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The collection's measurement rulers, for tests and harnesses. **This crate ships nothing.**

It exists because nothing in this repository implemented its measurements once. "Magnitude at a
frequency" had been written **twelve times under one name for three different physical quantities**,
so a figure from one crate could not be compared with a figure from another; and two crates asserted
one-cent oscillator tuning using a ruler that quantises to **nine** cents — a defect
`mxm-mono-00-dsp` had already found and fixed, in a comment, where the fix could not travel.

The measurement toolkit in [`docs/filters/06-testing.md`](../../docs/filters/06-testing.md) §6.1 had
existed as a fenced code block nobody compiled. This is that block, compiled, with tests.

# Ownership

| Path | Scope |
|---|---|
| `src/level.rs` | `peak`, `rms` |
| `src/convert.rs` | `amplitude_db`, `cents`, `cents_error`, `note_hz` |
| `src/spectrum.rs` | `component_amplitude`, `transfer_gain` + `Probe`, `fft`/`ifft`, `princarg`, `periods_for`, `a_weight`, `spectral_flatness_db`, `harmonic_split`, `alias_to_signal_db`, `N`. **The numeric core `dsp-lab` used to own** |
| `src/pitch.rs` | `frequency_by_crossings` — the corrected ruler |
| `src/observe.rs` | `worst_step`, `first_nonfinite` |
| `src/channels.rs` | `left`, `right`; `channel` and `interleave` are crate-private |
| `src/stimulus.rs` | `silence`, `dc`, `sine`, `periodic_sine`, `periodic_frequency`, `noise`, and the two aliasing controls |
| `examples/showcase.rs` | Prints every figure on [`docs/mxm-measure.html`](../../docs/mxm-measure.html) as JSON. Adding a figure means adding it here, re-running, and replacing the page's data block — the page may not be hand-corrected |

Does **not** own: any shipped DSP, any threshold, any verdict, or any normalisation law.

**Every public item earns its place one of three ways**, and there is no fourth:

1. **Two or more call sites outside this crate compute it today.** That is the gate below.
2. **The crate's own closed-form validation needs it.** The stimulus module, and the two aliasing
   controls with `harmonic_split`/`alias_to_signal_db`.
3. **It came down from `dsp-lab` with the numeric core**, and a reference harness calls it. Not both
   of them: `mod_spike` uses `N`, `fft` and `periods_for`, while `ifft`, `princarg`, `a_weight` and
   `spectral_flatness_db` have `osc_spike` alone. They moved together because they *were* one core,
   and splitting it to satisfy a per-function count would put a second transform back in the
   repository — which is the duplication `dsp-lab` had already removed once.

A first draft had roughly thirty more public functions — a crest factor, a DC offset, exact-silence
and exceedance observations, `power_db`, `db_amplitude`, `hz_note`, frame/second conversions, an RMS
envelope, mid/side, `deinterleave`, `peak_if_clean`, a note-number pitch wrapper, `periodic_length`,
`component_db`, an `f32` flatness, a whole `envelope` module — and **every one had zero callers**, or
one. `interleave` is the sharpest case: its only users are this crate's own tests, and **a function
whose second consumer is its own test makes the gate vacuous**, so it is crate-private. They were written, measured
against the gate, and removed. Being already written is not a consumer.

**`examples/showcase.rs` is not a call site for the gate either.** It draws the page from what the
crate already justifies, so a function it alone calls has one caller, not two.

# Local Contracts

## A ruler, never a verdict

**In — a *named computation*:** a definition that can be written down, cited, and scored against a
closed form. A transform, a weighting curve, a window length, a statistic, a unit conversion.

**Out — a *decision*:** a threshold, a tolerance, a margin, a default epsilon that changes an answer,
a function that returns a verdict, or a normalisation law.

The two are told apart by one question: **does the result change if somebody changes their mind about
what is acceptable?** A flatness figure does not. A "close enough" does.

There is no `assert_no_aliasing` here and there will not be one. A helper that decides whether a
number is acceptable hides the number, and hiding the number is how a threshold gets loosened without
anybody noticing. Thresholds are an argument about one instrument, and
[`docs/oscillators/06-testing.md`](../../docs/oscillators/06-testing.md) §6.4 says where they come
from: the measured value, with headroom, chosen against the failure mode. They belong beside the
assertion they justify.

**Watch for the default tolerance argument in review.** A helper that returns a number is a ruler;
the same helper with a tolerance parameter is a verdict wearing a parameter's clothing. That is the
most likely way this line gets breached, and it will arrive as a convenience.

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

## Dev-dependency only, and it is checked

Every crate but `dsp-lab` and `mxm-listening` takes this as a `[dev-dependencies]` entry, never a
normal one. Those two take it normally because they *are* measurement — `dsp-lab`'s harnesses, the
listener's interpretation — and neither has a shipped graph to protect. `mxm-listening` is itself a
dev-dependency wherever it is used, so the check below covers both names. **One exception, unshipped:**
`apps/mxm-listener-hud` takes the listener, and through it this crate, normally. It is not on the
check's list, which names shipped packages only; shipping it means adding it there, the check then
fails, and the listener's never-ships rule has to be decided first — the owner deferred that
decision (2026-09-28).

This is what keeps the crate from becoming shared DSP by the back door, so it is verified rather than
asserted — see *Verification*. A shipped crate that needs something from here has found shipped DSP,
and the answer is [`../mxm-modulation/AGENTS.md`](../mxm-modulation/AGENTS.md)'s rule, not this one.

The DSP crates' manifests say **no runtime dependencies** rather than *no dependencies at all*, which
is what that claim was always about: the shipped graph, and therefore the MSRV override.

## Result forms

- **A measurement over a buffer never returns NaN**, on any input including silence and at every rate
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
- **An intentional infinity is a result.** Zero amplitude in decibels is −∞ and a function returning
  it is working. Any floor is applied at the call site, where the argument for it lives.
- **An unavailable measurement is absent, not zero.** `Option`, not a sentinel. A function that
  returns `0.0` for *"I could not tell"* is indistinguishable from one that measured silence, and a
  test asserting *"below the threshold"* passes on both. `detect_root` in
  [`../mxm-creative-sampler-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-creative-sampler/blob/main/crates/mxm-creative-sampler-dsp/AGENTS.md) already declines
  rather than guessing; that is the standard.
- **Each function declares its valid domain** — band, minimum length, periodicity assumption. Outside
  it a function must still not return NaN, and is not required to be accurate.

## No reimplementation of the thing under test

A measurement crate containing a filter would eventually be used to check a filter against itself.
`stimulus::additive_saw` is the one permitted near-miss, because it is defined by a closed form rather
than by an algorithm and its role is to be what a shipped oscillator is *not*.

**This rule has already cost a working function.** An autocorrelation pitch estimator was written,
passed its complex-signal test, and was removed: it had one consumer, and
`mxm-creative-sampler-dsp` ships a YIN detector that `dsp-lab/examples/root_spike.rs` exists to
*score*. A YIN here would be a ruler built out of the thing under test. `src/pitch.rs` keeps the
reasoning where the code was.

## The extraction gate — two identical implementations, or it stays out

> A computation moves in here when at least **two call sites compute the same quantity** and it can be
> scored against a closed form. Otherwise it stays where it is, written as a local composition over
> primitives that did move.

This is the root contract's own bar, not a new one. `mxm-preset` waited for five copies and
`mxm-modulation` for three; a measurement crate does not get a lower bar because its subject is tests.

**Count second, compare first.** The hard half is *identical quantity*, and this crate's own plan got
it wrong four times in three review rounds — always the same way, **a group whose total count is large
hiding a member whose count is one.** Twelve `magnitude_at` sites were three quantities with counts of
five, three and one. Splitting a group by quantity re-runs the gate on each piece.

### The declined register

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

## The ruler validates itself

Unlike `dsp-lab`, **this crate has unit tests and they are not optional.** A broken shared ruler moves
every number in the repository at once, silently, in the same direction — the one failure a relative
assertion cannot see.

Every function is scored against a signal whose answer is known by construction: a sine's RMS is its
amplitude over √2, a first-order lowpass is −3.01 dB at its corner, an impulse transforms to a flat
spectrum, an additive saw measures clean and a trivial one filthy. **A metric that cannot be
scored against a closed form does not belong here yet.**

Two rate sets, both taken from existing suites rather than invented:

- `RATES` — the musical four, spelled identically in twelve files. **Accuracy** is scored here.
- `STRESS_RATES` — the endpoints and inflections the validator sweeps already assert at. **Never
  NaN** is scored here; accuracy is not claimed.

# Work Guidance

- **A migration onto these rulers is an equivalence or a correction, and the commit says which.** An
  equivalence must not move the number; a correction moves it, says by how much and why, and chases
  every quoted instance. Both kinds have happened: the three filter transfer-gain migrations were
  equivalences, and the WAV encoder and the two frequency estimators were corrections.
- **A threshold derived from a defective ruler is re-derived, not rescaled.** `mxm-mono-01-dsp` and
  `mxm-poly-06-dsp` asserted one cent on a ±9-cent ruler; measured with the corrected one the worst
  case is 0.009 cents, so both now assert a **tenth of a cent** — ninety times tighter, with an order
  of magnitude of headroom.
- Changing `src/spectrum.rs` changes both `docs/` references' evidence. Re-run both harnesses and
  diff against the recorded runs before recording anything.

# Verification

```bash
cargo test -p mxm-measure                       # the closed-form controls, on both rate sets
cargo tree -e normal,build -p mxm-measure       # zero dependencies
cargo clippy -p mxm-measure --all-targets
```

The shipped graph must not contain this crate. The shipped packages are the products, so **each
product repository runs the check over its own**, with dev edges excluded — a plain `cargo tree`
prints the permitted dev edge and would report a leak that is not one:

```powershell
$leaked = @()
foreach ($p in '<each shipped package of the repository>') {
  $tree = cargo tree -e normal,build -p $p
  if ($LASTEXITCODE -ne 0) { Write-Error "cargo tree failed for $p"; exit 2 }
  if ($tree -match 'mxm-measure|mxm-listening|mxm-plugin-test') { $leaked += $p }
}
if ($leaked) { Write-Error "a test-only crate leaked into: $($leaked -join ', ')"; exit 1 }
```

The pattern also names [`mxm-plugin-test`](../mxm-plugin-test/AGENTS.md), the plugins' shared
checks, and `mxm-listening`, the listener in `mxm-tools`: dev-dependencies on the same terms, one
check. The collection-wide run over every product — the twenty plugins and MXM Player — is the
workspace repository's, which enumerates them rather than globbing, so a product that forgets the
check shows up as a missing name.

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

# Child DOX Index

No child AGENTS.md files.
