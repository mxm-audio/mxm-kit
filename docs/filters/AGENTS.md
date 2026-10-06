# AGENTS.md — docs/filters

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

A working reference for building resonant, musical, real-time filters: theory, topologies,
nonlinearity, efficiency, the historical machines, measurement method, and Rust recipes. The
per-family deep-dives the chapters cite live in the research repository as
`research:filters/machines/<family>.md` (root *Research citations*; in full, [`collection-rules.md`](../collection-rules.md#research-boundary)).

*Since the split (2026-10-06):* `crates/dsp-lab/…` is in [mxm-tools](https://github.com/mxm-audio/mxm-tools) and `crates/mxm-mono-01-dsp/…` (like any `crates/<plugin>-dsp/…`) in that product's repository, [mxm-mono-01](https://github.com/mxm-audio/mxm-mono-01); the commands that build them run there.

Written for this repo but framework-agnostic. It is the source the DSP crates are expected to follow
when a filter changes.

# Ownership

| Path | Scope |
|---|---|
| `README.md` | Index, reading order, and the code's verification status |
| `01`–`09` | The reference chapters, read in order. `09` is the cross-cutting voicing model and depends on the deep-dives having established each family's threshold |

The twelve per-family deep-dives moved to `research:filters/machines/` on 2026-09-04; the chapters
cite them, and `09` depends on the thresholds they establish.

`README.md` is the index and **must** be refreshed whenever a chapter is added, moved or renamed,
and whenever a deep-dive lands over there.

# Local Contracts

## Code in these documents is compiled and run

Every Rust type published here has been compiled and exercised by the tests in `06-testing.md`
before publication. A snippet that has not been run does not go in.

When changing code in a chapter, rebuild the check crate, run it, and update any measured numbers it
changes. Numbers quoted in prose come from that run, not from expectation.

## Measured, derived, and assumed are labelled differently

- **Measured** — state the value and the conditions (sample rates, cutoffs, parameter settings).
- **Derived** — say it is a derivation from stated component values, not a quoted specification.
- **Assumed / chosen by ear** — say so plainly, and say what measurement would replace it.

`research:filters/machines/ir3109-roland.md` §10 is the reference for how measured results are presented, including
the ones that contradicted the expectation.

## Conflicting sources are recorded, not resolved silently

Where sources disagree, list the disagreement and what each claims, and say which reading the
document uses. `research:filters/machines/ir3109-roland.md` §9 is the pattern. Never smooth a conflict into a
confident sentence.

## Corrections propagate

These documents cross-reference heavily. When a fact changes, fix every place it appears — the
survey chapter, the deep-dive, and the recipes — in the same pass. A survey entry that contradicts
its own deep-dive is worse than no survey.

## Resonance is exposed normalised, never native

Any document or type that hands a user a resonance control maps it through a **normalised** value
where `1.0` is that family's own oscillation threshold — 4 for a ladder, 18.34 for a TB-303 diode
ladder, a damping floor for a CS-80. Wiring a knob to a native loop gain puts the singing point in a
different place on every family and is the one thing that stops the models being comparable.

`09-voicing.md` is the contract; each deep-dive is responsible for establishing its own family's
threshold by measurement.

## An exposed control needs a measured effect

Where a document exposes a filter parameter to a user — a voicing, a model, an advanced panel —
every control in it carries a test that measures what it does, and the document tabulates the
result. A control whose effect cannot be measured is either mis-designed or misnamed.

## Deep-dives are written in the research repository

`research:filters/machines/README.md` defines the section order and holds the candidate list. Write
one when a plugin is about to need it, not speculatively; the survey entry in `05` and the citation
from `09` are made here when it lands.

## Structure and honesty about scope

Section numbers are referenced across files (`§2.3`, `§3.2`). Renumbering means updating every
cross-reference. `README.md` also carries a **known limits** paragraph naming what is *not* verified
— behavioural rather than circuit-exact models, coefficients taken from a paper and not rechecked,
and the fact that no real hardware has been measured. Keep it accurate.

# Work Guidance

- Sources go in `08-sources.md` **annotated** — what each one is actually good for, not just a link.
- Broad rules belong in chapters `01`–`04`; instrument specifics belong in `05` or a research
  deep-dive.
- The DSP consequences for this repo belong in `07-rust-recipes.md` §7.5 and each deep-dive's own
  "consequences" section, so `crates/<plugin>-dsp` has one place to look.

# Verification

**The chapter code is not reproducibly tested inside this repository.** It currently lives in an
external scratch crate. Moving it into `crates/dsp-lab`, as the oscillator and modulation harnesses
already are, remains open work and requires recovering that scratch source first.

To re-verify after editing code in a chapter: collect the snippets into a scratch crate, add the
tests from `06-testing.md`, and run them.

**One exception is already reproducible:** `research:filters/machines/ba662-sh-2.md` publishes no code and quotes only
numbers from `crates/mxm-mono-01-dsp`'s `mono_01_filter_spike` and `resonance_gain` examples, because the diode-clamped cascade of `research:filters/machines/ir3109-roland.md` §7 ships in that
crate. When
that crate's filter changes, re-run both examples here (since the split, in mxm-mono-01) and update that page's §7 in the same pass —
a commit in the research repository.

```bash
cargo test --release        # in the scratch crate
```

The recoverable suite must cover chapter 2–3 building blocks, every exposed `Voicing` control, one
model per machine deep-dive, the chapter 9 `Voiceable` / `CoreVoicing` families, and the cost
measurements behind `09-voicing.md` §9.7. Current measured results are summarized in
`06-testing.md` and each deep-dive; update those when the suite changes.

# Child DOX Index

No child AGENTS.md files. The machine deep-dives are in the research repository, under its own
`filters/AGENTS.md`.
