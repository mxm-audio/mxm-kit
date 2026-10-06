# AGENTS.md — docs/oscillators

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

A working reference for building alias-free, musical, real-time oscillators: fundamentals, the
antialiasing algorithm families, per-waveform practice, analog character, measurement method,
per-technique deep dives, and the concrete verdict for mxm-mono-01. The machine survey (chapter 5)
and the exceptional 208 build-from appendix live in the research repository, cited as
`research:oscillators/…` (root *Research boundary*).

Written for this repo but framework-agnostic. It is the source `crates/<plugin>-dsp` is expected to
follow when an oscillator changes, and it is the companion volume to
[`../filters/`](../filters/README.md).

# Ownership

| Path | Scope |
|---|---|
| `README.md` | Index, reading order, measurement conditions, and the ten things worth knowing |
| `01`–`04` | The general chapters: fundamentals, the algorithm families, waveshapes, analog character |
| `06-testing.md` | Measurement method as `cargo test`, and what is not yet covered |
| `07-rust-recipes.md` | The verdict for mxm-mono-01, the changes worth making, and §7.6's open gaps |
| `08-sources.md` | Annotated bibliography with per-source anchors |
| `MEASUREMENTS.md` | Index of every in-repo DSP measurement, the metrics' limits, externally reported hardware-measurement boundary, and what is unmeasured here |
| `measurements-run.txt` | Verbatim output of the run the documents quote, so a fresh run can be diffed. The **Threadripper** run; everything except the granular cost tables |
| `measurements-run-13900k.txt` | The same, on the **i9-13900K** development machine. §9e and §9f's cost figures only — see the machine contract below |
| `09`–`14`, `16`–`17` | Deep dives on one synthesis family each: wavetable, granular, additive/resynthesis, FM/PM, phase distortion, sampler playback, supersaw/unison, waveshaping |
| `15` | Survey of shipping granular instruments, read against chapter 10's measurements |

`05-machines.md` (the machine survey) and `buchla-208-oscillators.md` (the one per-machine
appendix) moved to `research:oscillators/` on 2026-09-04. Chapter 5's number stays vacant here so
the citations remain stable.

`README.md` is the index and **must** be refreshed whenever a chapter is added, moved or renamed.

Per-*machine* deep-dives are exceptional and live in the research repository, where the case for
one is stated; an ordinary machine entry stays in `research:oscillators/05-machines.md`.

The numbered deep dives are per-*technique* (`09`–`17`), because that is normally where the design
space is. A technique earns one when the general chapters can only sketch it and the decisions inside
it are measurable.

# Local Contracts

## Every in-repo DSP measurement comes from one program, and it is not in a plugin's crate

[`../../crates/dsp-lab/examples/osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs)
produces every **measurement made by this repository** and quoted in this directory. It lives in
`dsp-lab` — a crate that ships nothing — rather than in `mxm-mono-01-dsp`, because this is a
collection-level reference and most of what the harness measures is not in mxm-mono-01 at all.
Quoting an in-repo result that program does not produce is not allowed; if a new result is needed,
add the measurement to the spike, run it, and quote the run.

A source may report its own hardware measurement — Dave Brown's one-unit 208 ranges are the first
case. That is **measured by the source**, never “measured here”: attribute the person/unit and
conditions at the claim, retain revision and calibration limits, and keep it outside
`MEASUREMENTS.md`, which indexes reproducible in-repo DSP runs.

The spike's numeric core — the FFT, the exactly-periodic frequency choice, the weighting curves —
moved out of `dsp-lab` into
[`../../crates/mxm-measure/src/spectrum.rs`](../../crates/mxm-measure/src/spectrum.rs), where
`mod_spike` and the collection's tests share it. The move was gated on the numbers: both harnesses
were re-run and diffed, and **every spectral figure in this reference is bit-identical across it** —
only cost columns moved, by the few percent they already drift. What is *not* shared is which bins
any measurement counts as wanted; that stays in the spike, because it is the experiment.

The spike measures the **shipped** oscillator through its real public API — `Phasor`, `saw`,
`pulse`, `clamp_pulse_width`, `DcBlocker` — never a copy of it. A candidate algorithm that is not
shipped is implemented in the spike itself, behind the same `Osc` trait, so quality and cost figures
describe the same code.

When `oscillator.rs` changes, re-run the spike and update every number it moves. `README.md`'s
measurement conditions block records the machine and toolchain; keep it current.

## A cost figure carries its machine, and two machines are now in the tree

`measurements-run.txt` is the Threadripper; `measurements-run-13900k.txt` is the current development
machine, and it runs the cost rows about **2x faster**. Spectra are bit-identical between them and
nanoseconds are not.

- **Never compare a nanosecond figure across the two files.** `10-granular.md` §10.6 measures the
  sine-carrier grain on the first machine and the sample-reading grain on the second, and re-ran §9e
  on the second machine rather than compare 16 ns against 8.87 ns across a hardware change.
- **Every cost figure quoted from the first machine is stale in absolute terms.** The ratios still
  hold and the ratios are the result, per the rule below. A full re-measurement pass on one machine
  is outstanding work, recorded in `07-rust-recipes.md` §7.6 rather than done piecemeal.
- **A new cost figure goes on the current machine and says so at the claim.**

**Cost figures are five-run means and drift a few percent between sessions.** They are quoted to two
decimals because that is what the harness prints, not because the last digit is meaningful. Do not
re-edit a chapter to chase a 2% change; do re-edit when an ordering changes or a figure moves by
more than its stated spread. The wavetable rows are the exception that proves it — they are
cache-bound and have been measured at 4.2 and 7.9 ns for identical code, which is a fact about the
technique and is documented as one.

## Measured here, measured by a source, published, and secondary are labelled differently

- **Measured here** — by the in-repo spike, with the conditions stated. Everything in the tables of
  chapters 2–3.
- **Measured by a source** — a named person's capture of a named hardware unit, with that source's
  conditions and revision limits. It is evidence about that unit and is not reproducible here.
- **Published** — a figure from a paper, attributed to it, with the paper's own metric named. The
  NMR table in `01-fundamentals.md` §1.4 is the pattern: it is not our measurement and does not
  pretend to be.
- **Secondary** — someone's account of a circuit we have not seen. `research:oscillators/05-machines.md` ends every
  section with where its claims came from and says "secondary" where that is what it is.

**No hardware has been measured for this repository.** `README.md` and `research:oscillators/05-machines.md` both say so.
That stays true in writing until someone records an instrument, at which point both change.

## Some things cannot be measured against an ideal, and then the reference is oversampling

Where an algorithm has no closed-form ideal — a grain train, a resynthesis engine, an FM operator —
the alias column can say how much energy is off the harmonic grid but not whether the wanted
harmonics came out at the right level. The comparison is then against a heavily oversampled
rendering of the *same* algorithm, and the chapter says so, states the oversampling factor, and
states the metric's floor: **about −115 dB here**, set by the decimation filter and not by the
oscillator. Every table using it carries a known-clean control row so the floor is visible.
`06-testing.md` §6.6 is the method; §10 and §11 of the spike are the implementations.

## The alias number is never quoted alone

Alias suppression can always be bought by removing treble, so a table with only an alias column
rewards exactly that. Every quality comparison in this directory carries the high-frequency cost
beside it — `h.err`, the count of harmonics more than 3 dB low, or the frequency where the −3 dB
point falls.

## Conflicting sources are recorded, not resolved silently

Where sources disagree — the CEM3340's core topology is the live example — state both readings, say
what each claims, and say what the disagreement does and does not change. Never smooth it into a
confident sentence.

## Our own measurements may contradict a paper, and then both are reported

`02-antialiasing.md` §2.7 measures EPTR slower than DPW2 where the published operation count says
it should be faster. The paper is not wrong about operation counts; the measurement is not wrong
about this machine. Record both and explain the gap. Do not quietly drop either.

## Corrections propagate

These documents cross-reference by section number (`§2.7`, `§3.3`). Renumbering means updating every
reference. When a measured number changes, fix it in the chapter that owns it, in `README.md`'s
ten-things list, and in `07-rust-recipes.md`'s verdict, in the same pass.

# Work Guidance

- Broad technique belongs in `01`–`04`; ordinary instrument specifics in
  `research:oscillators/05-machines.md`, and an exceptional named appendix there too; consequences
  for this repo belong in `07`, so `crates/mxm-mono-01-dsp` has one place to look.
- Sources go in `08-sources.md` **annotated** — what each is actually good for — with an `<a id>`
  anchor, because the chapters link to individual entries.
- A recommendation in `07` states what would change it. A verdict with no falsifier is an opinion.
- `07-rust-recipes.md` §7.6 is the open-gaps list. Add to it rather than quietly leaving a gap
  undocumented.

# Verification

The code in these documents is quoted from the crate and from the spike, both of which build and
run in-tree:

```bash
cargo test -p mxm-mono-01-dsp
cargo run -p dsp-lab --release --example osc_spike
```

Internal links, after moving or renaming a document:

```bash
cd docs && grep -roh "](https://github.com/mxm-audio/newdawn-workspace/blob/main/../../../../../../([0-9A-Za-z._/-]*/.md)" . | sed 's/](//' | sort -u
```

and confirm each path exists relative to its referring file.

Last full run: the spike's twenty sections. `measurements-run.txt` holds the Threadripper run every
chapter but the granular cost tables quotes; `measurements-run-13900k.txt` holds the current
machine's, which is what §9e and §9f are quoted from. Plus **12 oscillator tests passing**
in `crates/mxm-mono-01-dsp`, including `sawtooth_aliasing_stays_far_below_the_trivial_waveform` and
`pulse_aliasing_holds_up_at_narrow_widths`, which are the two that guard the band limiting itself.
The spike takes about 20 seconds in release; §8b's 16384-point bank and §11's 8x references are most
of it. Update this line when the set changes.

# Child DOX Index

No child AGENTS.md files. Every document in this directory is covered by this doc.
