# AGENTS.md — crates/mxm-lfo

Parent: [`../../AGENTS.md`](../../AGENTS.md).

# Purpose

The collection's standard LFO: a free-running phase, seven shapes (Sine, Triangle, Saw up, Saw
down, Square, Sample and hold, Smooth random), a random glide from a step to Smooth random, and
random draws from `mxm-rng`'s `Xorshift32`. **For every LFO that copies no machine** (the owner,
2026-11-25: "Some of the LFOs are based on specific machins and so should not be in a library.; but
yes make a standard one in the library"). First user: mxm-fx-bucket-verb.

# Ownership

Owns `src/` and `Cargo.toml`. The shape's stored ids and names are `mxm-modulation-params`'
`lfo::LfoShapeChoice`, its parameter half. Does **not** own:

- **a machine's LFO**: MXM Poly-06's delayed triangle is the Juno-106's, and stays with it, as every
  instrument's LFO that models its machine does;
- **a consumer's rate range, sync, retrigger or fade-in**: those are its parameters and its voice's;
- **MXM Modal Synth's three LFOs**, the source of this one: Modal Synth keeps its copy until it
  moves onto this crate, a change of its own, proved bit-identical (its draws already are:
  `the_draws_are_modal_synths`).

# Local Contracts

- **Relicensed MIT by its copyright holder** (the owner, 2026-11-25: "Yes, make the lfo MIT"),
  lifted from Modal Synth's GPL-3.0-or-later `mxm-modal-synth-dsp/src/lfo.rs`; the crate's header
  names the source. Code lifted from a product into the kit needs that word each time.
- **The call's contract is the caller's**, not an engine's block rate: a rate that is not finite or
  is below zero holds still, a `dt` that is not finite or not above zero moves nothing, and one call
  moves the phase under `MAX_CYCLE_PER_CALL` of a cycle, so a cycle draws one random value however
  the caller steps it (`the_call_contract`).
- **The shapes' order and meaning are stored**: `LfoShape::ALL` is `LfoShapeChoice`'s order, and a
  change to either is a change to every project that saved one.
- **Draws are bit-exact to Modal Synth's** (`the_draws_are_modal_synths`): its zero seed is
  `Xorshift32`'s golden-ratio constant and its bipolar law `mxm_rng::map::bipolar_f32`.
- Allocation-free and panic-free on every path, `Copy`, for the audio thread.
- No dependency but the zero-dependency `mxm-rng`; MSRV **1.87**, verified with the toolchain.

# Work Guidance

- A new shape arrives with a plugin that needs it, appended after `SmoothRandom` with its id in
  `LfoShapeChoice`; never inserted, never reordered.

# Verification

```bash
cargo +1.87.0 test -p mxm-lfo
cargo test -p mxm-lfo -p mxm-modulation-params
cargo clippy -p mxm-lfo -p mxm-modulation-params --all-targets -- -D warnings
```

# Child DOX Index

None.
