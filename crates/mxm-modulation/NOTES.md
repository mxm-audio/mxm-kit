# NOTES.md — crates/mxm-modulation

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

## The source frame's cost, measured

The sample counter is `u64` and never wraps in any life this instrument will have; stamps start at
`u64::MAX` so a write before the first `begin_sample` cannot be mistaken for one that already
happened, and `reset` returns them there. The source-frame change measured **146.2 ns/sample** on mxm-mono-01 with eleven sources, against a
150.9 ns/sample full-array-copy reference. Golden digests establish identical reads.

## Boundedness: the instruments that taught it

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

## Why a gated publication owes a clear, and who found it

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

## `product` and a scaled frame: the reasoning and the history

`sum` is scale-invariant and `product` is not. The factor is `1 + amount × (source − 1)`, and in a ⅛
frame that literal `1` is **eight raw units**: an `n`-factor product is off by `8ⁿ⁻¹`, a partial
amount interpolates toward the wrong neutral, and removal means nothing. No *constant* repairs that
for an arbitrary live route count.

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

## Why the scale is per route and applied last

**Per route, not per target.** A machine's own wiring is not uniform: mxm-mono-01's filter envelope
reaches six octaves and its filter LFO four, from the same cutoff. Plan §5 takes the conservative
form — a route the instrument itself wires keeps the scale it always had — so no stored value
changes meaning and no shipped sound moves. A route a *player* adds takes the target's declared
scale, which is that table's default column.

**Applied last**, never folded into the amount, because `(amount × scale) × source` does not round
the same way and the first is the instruction sequence these voices executed before they had
routing. Plan §6.2 says so; the pilot's params layer folded it anyway, and every pinned digest moved.

## The modulation standard: the ruling and its conformance proof in full

The owner's ruling of 2026-09-26, after *Amplitude ← Velocity* at +100 % was inaudible on
`mxm-poly-06`: **a performance source means the same thing on every instrument, and a route the
machine never had reaches the same distance on every instrument.** It is the one deliberate
exception to *don't pre-generalise* here, because a meaning written ten times is ten meanings — the
audit found Velocity → Pitch at full was 7, 12, 24 or 144 semitones depending on where it was
patched. `plans/plan-modulation-standard.md` (in the private archive) is the rollout and the rulings.

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

## The consumers, and what each taught the crate

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

## Why a test is falsified before it is trusted

**Falsify a new test before trusting it.** One in `tests/contracts.rs` says so in its own doc
comment: an earlier feedback test routed through `product`, which clamps its own output, so it
passed with the bound removed and proved nothing. It now routes through an unbounded `sum` and goes
red without the bound.
