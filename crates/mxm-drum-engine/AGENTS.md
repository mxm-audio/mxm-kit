# AGENTS.md — crates/mxm-drum-engine

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

What the collection's sixteen-slot drum engines share, moved verbatim from the copies
`mxm-drum-machine` and `mxm-model-drums` carried (the architecture audit's O4, owner-approved
2026-10-13): `SLOT_COUNT`, the kit `Lfo` and its `LfoShape`, `ModulationPatch`, `OutputFrame`,
`TriggerGroup`, `hash_bipolar`, `CHOKE_GROUP_MAX`, `OUTPUT_TRANSFER_SECONDS`, `RANDOM_SEED` and
`LFO_SEEDS`.

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own an engine, a slot, a voice, a model or anything one
instrument alone uses: those stay with the instrument. Each engine module re-exports the public items
as its own facade, so an instrument's paths are unchanged.

# Local Contracts

- **Bit-exact, for ever**: both instruments' recorded renders and golden digests were made through
  these bodies, and the seeds are load-bearing. Changing one changes two instruments at once.
- An item moves here when both engines' copies are identical (the `Lfo` was split into `value` and
  `advance`, the arithmetic and its order unchanged); a parametrised version of differing copies is a
  new decision.
- Plain values in and out; no allocation, no framework types. MSRV **1.87**, its dependencies
  (`mxm-dsp`, `mxm-modulation`, `mxm-rng`) dependency-free at that floor, verified with the toolchain.

# Verification

```bash
cargo +1.87.0 test -p mxm-drum-engine
cargo clippy -p mxm-drum-engine --all-targets -- -D warnings
```

# Child DOX Index

None.
