# AGENTS.md — crates/mxm-measure

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The collection's measurement rulers, for tests and harnesses. **This crate ships nothing.**

It is the measurement toolkit of [`docs/filters/06-testing.md`](../../docs/filters/06-testing.md)
§6.1, compiled, with tests, so each measurement is implemented once and figures from different
crates compare ([NOTES.md § Why the crate exists](NOTES.md#why-the-crate-exists)).

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
3. **It came down from `dsp-lab` with the numeric core**, and a reference harness calls it — the core
   moved whole, so a second transform never comes back
   ([NOTES.md § The numeric core](NOTES.md#the-numeric-core-that-came-down-from-dsp-lab)).

- **Being already written is not a consumer**, and **a function whose second consumer is its own
  test makes the gate vacuous** (so `interleave` is crate-private;
  [NOTES.md § The functions the gate removed](NOTES.md#the-functions-the-gate-removed)).
- **`examples/showcase.rs` is not a call site for the gate either.** It draws the page from what the
  crate already justifies, so a function it alone calls has one caller, not two.

# Local Contracts

## A ruler, never a verdict

**In — a *named computation*:** a definition that can be written down, cited, and scored against a
closed form. A transform, a weighting curve, a window length, a statistic, a unit conversion.

**Out — a *decision*:** a threshold, a tolerance, a margin, a default epsilon that changes an answer,
a function that returns a verdict, or a normalisation law.

The two are told apart by one question: **does the result change if somebody changes their mind about
what is acceptable?** A flatness figure does not. A "close enough" does.

- **No `assert_no_aliasing`, ever.** Thresholds belong beside the assertion they justify
  ([`docs/oscillators/06-testing.md`](../../docs/oscillators/06-testing.md) §6.4).
- **Watch for the default tolerance argument in review.** A helper that returns a number is a ruler;
  the same helper with a tolerance parameter is a verdict wearing a parameter's clothing. That is the
  most likely way this line gets breached, and it will arrive as a convenience.
- **No "part of the published definition" exception, and no guard epsilon.** If a guard seems
  necessary, the metric is being asked for an answer it does not have — report absence or an
  infinity, and let the caller decide.
- The only numbers inside a function that a caller cannot change are **definitional**: the format or
  the arithmetic fixes them. Reasoning and the retired WAV encoder:
  [NOTES.md § A ruler, never a verdict](NOTES.md#a-ruler-never-a-verdict-the-reasoning).

## Dev-dependency only, and it is checked

- Every crate but `dsp-lab` and `mxm-listening` takes this as a `[dev-dependencies]` entry, never a
  normal one; those two are measurement themselves and have no shipped graph. `mxm-listening` is
  itself a dev-dependency wherever it is used, so the check covers both names.
- **One exception, unshipped:** mxm-tools' `apps/mxm-listener-hud` takes the listener, and through it this
  crate, normally. Shipping it means adding it to the check's list, which then fails — the owner
  deferred that decision (2026-09-28;
  [NOTES.md § The one unshipped exception](NOTES.md#the-dev-dependency-rules-one-unshipped-exception)).

This is what keeps the crate from becoming shared DSP by the back door, so it is verified rather than
asserted — see *Verification*. A shipped crate that needs something from here has found shipped DSP,
and the answer is [`../mxm-modulation/AGENTS.md`](../mxm-modulation/AGENTS.md)'s rule, not this one.

The DSP crates' manifests say **no runtime dependencies** rather than *no dependencies at all*, which
is what that claim was always about: the shipped graph, and therefore the MSRV override.

## Result forms

- **A measurement over a buffer never returns NaN**, on any input including silence and at every rate
  in `STRESS_RATES`; where the buffer holds a non-finite sample it reports **absence**. **Scalar
  conversions are arithmetic and behave like it** — `amplitude_db(NaN)` is NaN. **No exception for
  `peak`: it returns `Option`** ([NOTES.md § Why `peak` returns `Option`](NOTES.md#why-peak-returns-option)).
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

No filter, oscillator or detector a shipped crate implements — a ruler built from the thing under test
would check it against itself. `stimulus::additive_saw` is the one permitted near-miss: it is defined
by a closed form, not an algorithm. An autocorrelation pitch estimator was removed for this
([NOTES.md § The ruler this rule cost](NOTES.md#the-ruler-this-rule-cost)).

## The extraction gate — two identical implementations, or it stays out

> A computation moves in here when at least **two call sites compute the same quantity** and it can be
> scored against a closed form. Otherwise it stays where it is, written as a local composition over
> primitives that did move.

**Count second, compare first**: a group whose total count is large can hide a member whose count is
one, and splitting a group by quantity re-runs the gate on each piece
([NOTES.md § The extraction gate](NOTES.md#the-extraction-gate-how-the-count-went-wrong)).

**The declined register** — every concept somebody may want, its consumer count and the crate that
owns its local composition — is [NOTES.md § The declined register](NOTES.md#the-declined-register).
**A name there is not an API.** Read it before adding a ruler, so a second consumer is recognised and
nothing is re-implemented.

## The ruler validates itself

- **This crate has unit tests and they are not optional**: a broken shared ruler moves every number
  in the repository at once, in the same direction.
- **Every function is scored against a signal whose answer is known by construction**; a metric that
  cannot be scored against a closed form does not belong here yet
  ([NOTES.md § Why the ruler validates itself](NOTES.md#why-the-ruler-validates-itself)).

Two rate sets, both taken from existing suites rather than invented:

- `RATES` — the musical four, spelled identically in twelve files. **Accuracy** is scored here.
- `STRESS_RATES` — the endpoints and inflections the validator sweeps already assert at. **Never
  NaN** is scored here; accuracy is not claimed.

# Work Guidance

- **A migration onto these rulers is an equivalence or a correction, and the commit says which.** An
  equivalence must not move the number; a correction moves it, says by how much and why, and chases
  every quoted instance.
- **A threshold derived from a defective ruler is re-derived, not rescaled**
  ([NOTES.md § Migrations so far](NOTES.md#migrations-so-far-and-the-re-derived-thresholds)).
- Changing `src/spectrum.rs` changes both `docs/` references' evidence. Re-run both harnesses and
  diff against the recorded runs before recording anything.

# Verification

```bash
cargo test -p mxm-measure                       # the closed-form controls, on both rate sets
cargo tree -e normal,build -p mxm-measure       # zero dependencies
cargo clippy -p mxm-measure --all-targets
cargo +1.87.0 build -p mxm-measure              # MSRV: the library
cargo +1.87.0 test  -p mxm-measure              # and its tests, which are the harder floor
```

MSRV is verified, not asserted: `cargo test` on stable accepts a newer language feature silently
([NOTES.md § The MSRV runs](NOTES.md#the-msrv-runs)).

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

# Child DOX Index

No child AGENTS.md files.
