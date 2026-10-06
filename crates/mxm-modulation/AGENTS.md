# AGENTS.md — crates/mxm-modulation

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The framework-free routing half of the modulation system described by
`plans/plan-modulation-routing.md` (`plans/plan-modulation-routing.md` in the private archive). Zero dependencies and
MSRV **1.87** let every DSP crate use it without inheriting the GUI floor. Its publication timing
and bounds make player-created feedback deterministic and finite.

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
  (`plans/plan-modulation-standard.md` (`plans/plan-modulation-standard.md` in the private archive); *Every
  modulation means the same*, below). Its proof, `conformance`, is behind a feature only
  `[dev-dependencies]` enable.

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

The sample counter is `u64` and never wraps in any life this instrument will have; stamps start at
`u64::MAX` so a write before the first `begin_sample` cannot be mistaken for one that already
happened, and `reset` returns them there. The source-frame change measured **146.2 ns/sample** on mxm-mono-01 with eleven sources, against a
150.9 ns/sample full-array-copy reference. Golden digests establish identical reads.

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

A module working in a wider domain **scales into this one before publishing** — `mxm-mono-00`'s
ten-volt columns already do, and `mxm-mono-pr1` publishes every source through a shared ⅛ unit. A
non-finite value publishes as zero rather than poisoning every route that reads it.

**A frame unit is sized by the widest *composed* value, not the widest raw source** — `mxm-mono-pr1`
paid for this one. A slot holds everything built *from* the sources as well as the sources: its
summing module reaches 5.9 raw units where no raw source exceeds 3, so a unit chosen from the sources
alone clamps the module on publication and takes every route that reads it with it. **Choose a power
of two**: scaling by one commutes exactly with `f32` rounding, so `Σ(sᵢ ÷ 8)` is `(Σ sᵢ) ÷ 8` and a
target scale of `8 × k` undoes it in one exact multiply — which is what let that instrument keep four
destinations bit-identical across the conversion.

The **target's** bound is separate and is the caller's: `sum` takes it, because `mxm-mono-08` clamps
its CV sum where a cutoff target sums octaves.

## A gated publication owes a `clear`

**Publishing only the sources some live route reads is the routing's main saving, and it has a
price.** An unread slot keeps whatever it held the last time something *did* read it, so the moment a
route is added, a **backward** route from that source reads a value that may be from a different
phrase entirely — for exactly one sample, and how stale it is depends on how long the source went
unread, which makes the first audible value depend on the host.

`SourceFrame::clear` is what the transition owes: call it for every source that has **just become
needed**, and the first sample is a deterministic zero instead. `mxm-mono-pr1` found this, and
**every consumer with a gated publication now clears**: `mxm-mono-00`; since 2026-09-15 the pilot
and `mxm-creative-sampler`, which carried the same gate and the same latent defect until then; and
`mxm-mono-02`, `mxm-poly-06` and `mxm-mono-03`, converted with it — each proved by its own
instrument-level test. `tests/contracts.rs`'s
`clearing_a_slot_stops_a_newly_read_source_returning_an_ancient_value` is the falsified proof —
without the clear, the first read is the ancient value.

## `product` is not scale-invariant, so a scaled frame **un-scales into the law**

`sum` is scale-invariant and `product` is not. The factor is `1 + amount × (source − 1)`, and in a ⅛
frame that literal `1` is **eight raw units**: an `n`-factor product is off by `8ⁿ⁻¹`, a partial
amount interpolates toward the wrong neutral, and removal means nothing. No *constant* repairs that
for an arbitrary live route count.

**What repairs it is the rule the frame already has, applied on the way in.** A module scales *into*
the frame's domain before publishing; a product scales *out of* it before applying its law:

```text
factor = 1 + amount × (frame.read(source) × FRAME_SCALE − 1)
```

The `1` is then one raw unit, which is what the law means, and every property returns. `mxm-mono-pr1`
is the instrument that needed it, and `mxm-para-07` carried the same law, so since 2026-09-26 it
lives here as **`product_with_tops`**, as this section said it would once a second instrument wanted
it. The *top* generalises the `1`: it is the raw value at which a factor is neutral at full amount —
one for a unipolar source, **zero for the standard Velocity**, which is `v − 1` — so a velocity factor
is the number it was before Velocity changed meaning.

**This section said the opposite until 2026-09-14**, and the cost is worth recording: reading
"`product` is not scale-invariant" as "a scaled frame cannot have a multiplier target" is what
silently dropped `plan-modulation-routing.md` decision 1.14 from `mxm-mono-pr1`, a module its owner
had specified. A law that does not hold under a transformation is a reason to undo the
transformation at the boundary, not a reason to drop the law.

## `product`'s neutral is one, not zero

The multiplier module is a target whose law is product and whose result is published as a source —
how one source scales another without a per-route modifier. Two properties, both tested:

- **Nothing present means one.** A multiplier nobody has wired changes nothing.
- **A zero amount is neutral, not annihilating.** The factor blends between neutral and the source, so
  turning a factor down does not silence the product.

## The scale is a third multiply, applied last

`sum_scaled` computes `(amount × source) × scale`, and both halves of that are load-bearing.

**Per route, not per target.** A machine's own wiring is not uniform: mxm-mono-01's filter envelope
reaches six octaves and its filter LFO four, from the same cutoff. Plan §5 takes the conservative
form — a route the instrument itself wires keeps the scale it always had — so no stored value
changes meaning and no shipped sound moves. A route a *player* adds takes the target's declared
scale, which is that table's default column.

**Applied last**, never folded into the amount, because `(amount × scale) × source` does not round
the same way and the first is the instruction sequence these voices executed before they had
routing. Plan §6.2 says so; the pilot's params layer folded it anyway, and every pinned digest moved.

## Every modulation means the same — `standard`

The owner's ruling of 2026-09-26, after *Amplitude ← Velocity* at +100 % was inaudible on
`mxm-poly-06`: **a performance source means the same thing on every instrument, and a route the
machine never had reaches the same distance on every instrument.** It is the one deliberate
exception to *don't pre-generalise* here, because a meaning written ten times is ten meanings — the
audit found Velocity → Pitch at full was 7, 12, 24 or 144 semitones depending on where it was
patched. `plans/plan-modulation-standard.md` (`plans/plan-modulation-standard.md` in the private archive) is the
rollout and the rulings.

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

**`conformance`** (feature `conformance`) is the proof, and every converted instrument runs it
from its own tests over a `Declaration` of its real graph: offers equal `offer`, **every pair
delivers exactly nothing at its source's rest**, every offered half moves its target by at least a
per-kind minimum, added pairs deliver the standard reach; `check_publishers` has a real voice
publish each performance source at fixed inputs and compares the standard's publisher, and
`check_release_silence` plays every performance pair through a voice to exact silence after
release (`mxm-mono-08`, whose optical gates take seconds to reach zero, runs its cases at its
lowest sample rate so each can still wait for exact silence). **Refinements came with the
first one-sided targets** (`mxm-mono-08`, `mxm-mono-00`): a bipolar source's move and its half-travel
reach are its larger at either extreme, since into a target that discards a sign a negative amount
moves it only with the lever pushed the other way; and **a machine pair is held to a move only on
the halves an added pair would be offered** — it keeps both, but into a one-sided target the other
subtracts from another route and does nothing alone, and a machine law with no added pairs (an edge's
reset depth) owes no move beyond its rest. Each can only accept more, so no earlier instrument's
result changed. `Kind::NarrowingWidth` is the standard table's one-sided width (`0.5 − Σ`, 45 %,
9 %/oct). `mxm-poly-06` is the first consumer and the pattern: a `conformance.rs` in its DSP crate
behind a feature of the same name, which the plugin's tests reuse (`mxm_plugin_test::routing_checks`)
to hold each route's reading to what the graph delivers. **Enable it only from
`[dev-dependencies]`** — dev-dependency features do not reach a shipped graph under the workspace
resolver, which is what keeps a `.clap` free of test code.

# Work Guidance

- **Do not add a law here because it seems reusable.** Gate and sync detection are one instrument's
  until a second needs them; that is the rule this crate was extracted under and it applies to the
  crate itself.
- **Keep `[dependencies]` empty.** It is what earns the 1.87 override, and `cargo tree -p
  mxm-modulation` is the check.
- **A test here states which invariant or which review finding it stands for.** Several exist because
  an earlier design failed them, and a test whose reason is unrecorded is one somebody deletes while
  tidying.
- **Falsify a new test before trusting it.** One in `tests/contracts.rs` says so in its own doc
  comment: an earlier feedback test routed through `product`, which clamps its own output, so it
  passed with the bound removed and proved nothing. It now routes through an unbounded `sum` and goes
  red without the bound.

# Verification

```bash
cargo test -p mxm-modulation
cargo clippy -p mxm-modulation --all-targets --features conformance
cargo tree -p mxm-modulation          # must show no dependencies: the 1.87 contract
```

Contract tests cover removal and reversibility, unit delay, reset, feedback bounds, multiplier
neutrality, exact silence, block-partition invariance, determinism and finite output. They do not
establish sound or instrument-specific scaling; each consumer owns those checks.

**Seven instruments consume this crate**: `mxm-mono-01`, the pilot; `mxm-creative-sampler`, the
first **polyphonic** one and therefore the first to exercise the plan's per-scope frames for real;
`mxm-mono-pr1`, the first whose sources are **not unit-bounded**; `mxm-mono-00`, whose switching
jacks became routes; `mxm-mono-02`, the first of the five conversions M5 still owed; and
`mxm-poly-06`, the second polyphonic one, with a frame in each of its six voices; and `mxm-mono-03`,
whose Env Mod floor showed that **a floor is circuit, added by the instrument beside a
neutral-at-zero route**, so no amount type here needs to permit one — which retires the governing
plan's §7.4 framing. **Every instrument will** — the owner's ruling of 2026-09-15, with the
conversions still owed in `plan-modulation-routing.md` M5. The first two needed no change here; the
third added exactly one method — `SourceFrame::clear`, for a latent defect the pilot and the sampler
shared until they took it too (*A gated publication owes a clear*).

The sampler publishes each global source into every voice's own frame, which
`plan-modulation-routing.md` §4.1 sanctions as *the same value in every voice*, so the summing laws —
which have pinned digests behind them on the pilot — were left alone. `mxm-mono-pr1` needed the two
rules above written down rather than any new API, and it also **retired a predicted one**: plan §7.3
expected the frame to need a read-without-commit mode for that instrument's `process_frozen`. It does
not. Not calling `begin_sample` and publishing nothing leaves every `read` returning the last
committed value and advances no state, which is exactly what that mode did.

# Child DOX Index

No child AGENTS.md files.
