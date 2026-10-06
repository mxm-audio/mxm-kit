# AGENTS.md — crates/mxm-modulation

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The framework-free routing half of the modulation system described by
`plans/plan-modulation-routing.md` (in the private archive). Zero dependencies and MSRV **1.87** let
every DSP crate use it without inheriting the GUI floor. Its publication timing and bounds make
player-created feedback deterministic and finite.

# Ownership

Owns five things and **no instrument's voice**:

- **`SourceFrame`** — a sample's worth of source values with the unit delay: read this sample's value
  if this sample produced it, otherwise last sample's.
- **`Compacted`** — the live routes into one target, rebuilt when topology changes rather than per
  sample.
- **`sum`** and **`product`** — the two combination laws that are not instrument-specific — and
  their two forms for a machine that summed on a bus: **`sum_split`** (the bus's scale for the
  machine's pairs, bit-identical while nothing added is live, and each added pair's own) and
  **`product_with_tops`** (a scaled-frame multiplier, each factor neutral at its source's top).
- **The boundedness contract** (below), which is the part a unit delay does not give you.
- **The performance standard** — `standard`: what a performance source publishes, the reach an
  added route takes, the amplitude law, and which pairs are offered at all
  (`plans/plan-modulation-standard.md` in the private archive; *Every modulation means the same*,
  below). Its proof, `conformance`, is behind a feature only `[dev-dependencies]` enable.

[`SourceFrame::clear`] is the one piece of API an instrument's *topology transition* owes rather than
its per-sample loop. A consumer that publishes every source unconditionally never calls it; one that
gates publication on **is anything reading this** must, for every source that has just become read.
See *A gated publication owes a clear*.

**Everything here is proportional to live routing, not total source capacity.** `Compacted` visits
live routes only, and `SourceFrame::begin_sample` is one counter increment. Two invariants make that
possible:

- **An unwritten source's `current` slot already holds last sample's value**, because nothing has
  overwritten it. So the unit-delay read is one load with no branch, and the two cases its
  definition names are the same read.
- **`previous` is owed only to a source that is written**, so the first `write` of each sample saves
  it. Nobody publishes, nobody pays.

The sample counter is `u64`; stamps start at `u64::MAX` and `reset` returns them there
([NOTES.md § The source frame's cost](NOTES.md#the-source-frames-cost-measured)).

Does **not** own: any source's generation, any target's application, an instrument's evaluation order
or unit scale, or the detectors that are not plain CV. `mxm-mono-00`'s audio-row crossfade, its gate
threshold and its sync edge stay with it until a second instrument needs the same one — *don't
pre-generalise* applies to this crate's own growth.

**The seam, in one line:** this crate owns *when a value is readable*, *which routes are live* and
*what a performance source means*; the instrument owns its machine's sources and laws and *how a
value is applied*.

# Local Contracts

## Presence is what the DSP reads, and that is what makes removal work

A route is a **(target, source) pair** with a *presence* and an *amount*. There is no source selector,
because a target has one slot per source and a slot *is* a source.

- **An absent pair contributes nothing whatever its amount holds.** `Compacted::build` is the only
  route to an amount and admits present pairs only. This is what makes **removing a route one
  parameter write**, and what makes re-adding it restore the depth the player last set — design
  system §8.7's undo with no undo stack.
- **An amount of zero is not absence.** A present route at zero depth remains assigned and stays on
  screen. The ordinary consumer keeps it in the compacted list. A consumer with a materially large
  matrix may use a second execution compaction that omits only a *settled* zero amount, provided the
  assignment presence remains separate, the smoothing tail stays live, and execution state never
  feeds back into interface or preset topology. `mxm-drum-machine` is that consumer.
- **Nothing is ever reordered.** A route's position is its source's, so removing one never moves
  another.

## Boundedness is at publication, because a unit delay is not stability

`SourceFrame::write` bounds every value to unit magnitude. **This is load-bearing, not hygiene.** A
unit delay makes a cycle *well-defined*; it does not make it *stable*, and a chain whose loop gain
exceeds one grows geometrically. A product of unit-bounded values is unit-bounded, so no cycle can
run away.

A module working in a wider domain **scales into this one before publishing**; a non-finite value
publishes as zero. **A frame unit is sized by the widest *composed* value, not the widest raw
source**, and is **a power of two**, so scaling commutes exactly with `f32` rounding
([NOTES.md § Boundedness](NOTES.md#boundedness-the-instruments-that-taught-it)).

The **target's** bound is separate and is the caller's: `sum` takes it, because `mxm-mono-08` clamps
its CV sum where a cutoff target sums octaves.

## A gated publication owes a `clear`

An unread slot keeps whatever it held the last time something read it, so a newly added **backward**
route would read a stale, host-dependent value for one sample. **Call `SourceFrame::clear` for every
source that has just become needed**, and the first sample is a deterministic zero. Every consumer
with a gated publication clears, each proved by its own instrument-level test
([NOTES.md § Why a gated publication owes a clear](NOTES.md#why-a-gated-publication-owes-a-clear-and-who-found-it)).

## `product` is not scale-invariant, so a scaled frame **un-scales into the law**

`sum` is scale-invariant and `product` is not: in a ⅛ frame the factor's literal `1` is eight raw
units. **What repairs it is the rule the frame already has, applied on the way in.** A module scales
*into* the frame's domain before publishing; a product scales *out of* it before applying its law:

```text
factor = 1 + amount × (frame.read(source) × FRAME_SCALE − 1)
```

That law lives here as **`product_with_tops`**; the *top* is the raw value at which a factor is
neutral at full amount — one for a unipolar source, **zero for the standard Velocity** (`v − 1`).
**Never drop a multiplier target because a frame is scaled**: undo the transformation at the boundary
([NOTES.md § `product` and a scaled frame](NOTES.md#product-and-a-scaled-frame-the-reasoning-and-the-history)).

## `product`'s neutral is one, not zero

The multiplier module is a target whose law is product and whose result is published as a source —
how one source scales another without a per-route modifier. Two properties, both tested:

- **Nothing present means one.** A multiplier nobody has wired changes nothing.
- **A zero amount is neutral, not annihilating.** The factor blends between neutral and the source, so
  turning a factor down does not silence the product.

## The scale is a third multiply, applied last

`sum_scaled` computes `(amount × source) × scale`, and both halves of that are load-bearing.
**Per route, not per target**: a route the instrument itself wires keeps the scale it always had; a
route a *player* adds takes the target's declared scale. **Applied last**, never folded into the
amount: `(amount × scale) × source` rounds differently and moves every pinned digest
([NOTES.md § Why the scale is per route and applied last](NOTES.md#why-the-scale-is-per-route-and-applied-last)).

## Every modulation means the same — `standard`

**A performance source means the same thing on every instrument, and a route the machine never had
reaches the same distance on every instrument** (the owner, 2026-09-26). It is the one deliberate
exception to *don't pre-generalise* here
([NOTES.md § The modulation standard](NOTES.md#the-modulation-standard-the-ruling-and-its-conformance-proof-in-full)).

- **Publishers** (`key`, `velocity`, `wheel`, `pressure`, `bend`, `random`). Every one is **zero at
  its source's rest**, so a present route at rest does nothing: Key at middle C, **Velocity at the
  hardest note** (`v − 1`), a gesture let go, Random's centre. Key is the glided note; a Bend is the
  lever, independent of the bend range.
- **Reach** (`reach`, `key_scale`). A path **the machine itself had keeps the machine's reach**;
  every added path takes these. A Key route reads per octave of keyboard, so `key_scale` converts a
  per-octave reach through the instrument's own Key unit.
- **Amplitude** is `level × amplitude_factor(Σ)` = `1 + clamp(Σ, ±AMPLITUDE_SUM_BOUND)`: silence to
  ×2, on every instrument. A machine's own CV-summing amplifier keeps its law as a machine target
  (*VCA level*), and the standard Amplitude sits after it.
- **Offer** (`Law`, `offer`). Which (target, source) pairs exist at all, decided once: a machine pair
  is always offered; an added pair from a performance source is refused where it can never mean
  anything (an edge, an audio input, a sampler fed a per-note constant) and one-sided where the
  target discards a sign. Refused pairs are not minted; their ids are retired.
- **`conformance`** (feature `conformance`) is the proof: every converted instrument runs it from its
  own tests over a `Declaration` of its real graph, with `check_publishers` and
  `check_release_silence`; `mxm-poly-06`'s `conformance.rs` is the pattern, reused by
  `mxm_plugin_test::routing_checks`. **Enable it only from `[dev-dependencies]`**, which keeps a
  `.clap` free of test code.

# Work Guidance

- **Do not add a law here because it seems reusable.** Gate and sync detection are one instrument's
  until a second needs them; that is the rule this crate was extracted under and it applies to the
  crate itself.
- **Keep `[dependencies]` empty.** It is what earns the 1.87 override, and `cargo tree -p
  mxm-modulation` is the check.
- **A test here states which invariant or which review finding it stands for.** Several exist because
  an earlier design failed them, and a test whose reason is unrecorded is one somebody deletes while
  tidying.
- **Falsify a new test before trusting it** — see `tests/contracts.rs` and
  [NOTES.md § Why a test is falsified](NOTES.md#why-a-test-is-falsified-before-it-is-trusted).

# Verification

```bash
cargo test -p mxm-modulation
cargo clippy -p mxm-modulation --all-targets --features conformance
cargo tree -p mxm-modulation          # must show no dependencies: the 1.87 contract
```

Contract tests cover removal and reversibility, unit delay, reset, feedback bounds, multiplier
neutrality, exact silence, block-partition invariance, determinism and finite output. They do not
establish sound or instrument-specific scaling; each consumer owns those checks. Every instrument
consumes this crate (the owner, 2026-09-15):
[NOTES.md § The consumers](NOTES.md#the-consumers-and-what-each-taught-the-crate).

# Child DOX Index

No child AGENTS.md files.
