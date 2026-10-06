# AGENTS.md — docs/modulation

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

A working reference for the third leg of the voice: LFOs, envelopes, parameter smoothing, and
machine-specific control systems whose coupled state cannot be reduced to one generic modulator.
What moves the sound, as against what makes it ([`../oscillators/`](../oscillators/README.md)) and
what shapes it ([`../filters/`](../filters/README.md)).

It exists because `crates/mxm-mono-01-dsp/src/lfo.rs` and `envelope.rs` are shipped, tested code with no
durable documentation, and because smoothing is folklore in an AGENTS.md with no numbers behind it.

# Ownership

| Path | Scope |
|---|---|
| `README.md` | Index, reproduction, the evidence-scope warning, and the three things worth knowing |
| `01-lfos.md` | Rates, shapes, band-limiting, free-run against retrigger, sample and hold |
| `02-envelopes.md` | Curves, the overshoot correction, the click clamp, the state machine |
| `03-smoothing-and-events.md` | Zipper noise, control rate, block splitting |
| `04-glide-and-portamento.md` | Fixed time against fixed rate, the one-pole pitch lag, tempo dependence, slide semantics. **Unmeasured** — see below |
| `measurements-run.txt` | Verbatim output of the run the documents quote |

The 208/218 control-source appendix moved to `research:modulation/buchla-208-control-sources.md`
on 2026-09-04 (root *Research citations*; in full, [`collection-rules.md`](../collection-rules.md#research-boundary)).

`README.md` is the index and **must** be refreshed whenever a chapter is added, moved or renamed.

# Local Contracts

## Two evidence sources, and the boundary between them is the ownership line

Measured figures come from `crates/dsp-lab/examples/mod_spike.rs` — a crate that ships nothing,
so that the evidence behind this reference is reproducible from a clone:

```bash
cargo run -p dsp-lab --release --example mod_spike    # in mxm-tools, since the split
```

*Since the split (2026-10-06):* `crates/dsp-lab/…` is in [mxm-tools](https://github.com/mxm-audio/mxm-tools) and `crates/mxm-mono-01-dsp/…` (like any `crates/<plugin>-dsp/…`) in that product's repository, [mxm-mono-01](https://github.com/mxm-audio/mxm-mono-01); the commands that build them run there.

**Sections 1–3 measure the shipped code** — `lfo.rs` and `envelope.rs` — through their public API,
never a copy.

**Section 4 does not, and cannot.** Smoothing belongs to the plugin: `../../plugins/AGENTS.md` (since
the split, [`plugin-conventions.md`](../plugin-conventions.md#smooth-signals-not-coefficients)) owns
parameters, ranges and smoothing, and `../../crates/mxm-mono-01-dsp/AGENTS.md` disclaims them. The DSP
crate has no dependencies and the plugin depends on it, so an example inside the DSP crate cannot
reach `plugins/mxm-mono-01/src/params.rs`. §4 measures a **generic one-pole model**.

So the rule this directory runs on: **every measured claim names the program that produced it, and
any claim measuring a generic model rather than the shipped path says so in the sentence that makes
it.** Not in a footnote, not in a preamble — in the sentence. `03-smoothing-and-events.md` opens
with the warning and repeats it as a numbered gap in §3.4.

Closing that properly means measuring through `apps/mxm-player`, which hosts the CLAP and captures
audio in its tests (since the split, from mxm-mono-01's `plugins/mxm-mono-01/host-tests`, which load
the bundle through MXM Player). Until someone does, no number from §4 may be quoted as mxm-mono-01's behaviour.

## Chapter 4 is derived and published, not measured, and says so in every claim

`mod_spike` does not reach glide, so `04-glide-and-portamento.md` carries **no measured numbers at
all**: the TB-303's ≈60 ms time constant is published and unverified here, and every arrival
percentage and residual is arithmetic on the exponential. The chapter labels each one at the point
it is used and records the gap as its own §4.7.6.

`Voice`'s lag is reachable through the DSP crate's public API, so this gap is closable in the same
program and at the same standard as §§1–3 — unlike §4 of `03-smoothing-and-events.md`, which
**cannot** be closed there because smoothing belongs to the plugin. Do not conflate the two open
items: one is undone, the other is out of reach.

## Durable rules promote along the ownership line too

When something here becomes a convention rather than a finding:

- LFO and envelope *generator* behaviour → `../../crates/mxm-mono-01-dsp/AGENTS.md`
- Smoothing, parameter ranges, event and block handling → `../../plugins/AGENTS.md` (since the
  split, [`plugin-conventions.md`](../plugin-conventions.md))
- What a per-step slide flag means for the step model → `../../apps/mxm-player/AGENTS.md` (in
  mxm-player since the split), where
  it now lives: **slide is built.** A tied step carrying notes is a slide — chapter 4's §4.7.1
  lead held up, both halves were already there, and no third step flag was added. The chapter
  records the decision; the player's doc carries the contract.

Putting a smoothing rule in the DSP crate's doc would contradict that crate's own ownership
statement, which is the mistake this directory was designed around.

## Measured, published and asserted are labelled differently

Same standard as `../oscillators/AGENTS.md`. Additionally, where a chapter reports a property of the
shipped code that is enforced by a constant — `MIN_TIME_S`, the block cap — it says **which constant
and where**, because those are the lines most likely to be removed by someone tidying up and the
documentation is the only thing that will stop them.

## A claim about the source is checked against the source

`01-lfos.md` §1.2 quotes `lfo.rs`'s own comment about band limiting and then tests it. That is the
pattern: where shipped code asserts something about its own behaviour, this reference either
confirms it with a measurement or records that it could not.

# Work Guidance

- Broad technique first, then what the shipped code does, then what it means for the collection.
  Each numbered chapter ends with the consequences section, and those are the parts that promote
  out. A named machine appendix — in the research repository — instead preserves a coupled control
  graph that would become misleading if split across generic chapters; it stays source-labelled and
  does not claim in-repo measurements.
- Do not restate oscillator or filter material. Cross-link.
- An LFO is an oscillator; when a question is really about waveform generation, it belongs in
  `../oscillators/` and this directory links to it.

# Verification

```bash
cargo run -p dsp-lab --release --example mod_spike    # in mxm-tools, since the split
cargo test -p mxm-mono-01-dsp                         # in mxm-mono-01
```

Internal links, after moving or renaming a document:

```bash
cd docs && grep -roh "](\([0-9A-Za-z._/-]*\.md\)" . | sed 's/](//' | sort -u
```

Last full run: the spike's four sections, recorded verbatim in `measurements-run.txt`. Update this
line when the set changes.

# Child DOX Index

No child AGENTS.md files. Every document in this directory is covered by this doc.
