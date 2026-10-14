# AGENTS.md — crates/mxm-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md).

# Purpose

The collection's shared DSP primitives, each moved verbatim from copies the instruments carried
byte for byte (the architecture audit's O1, owner-approved 2026-10-13): `guard::flush`,
`shape::{tanh_approx, tan_approx, diode_clamp}`, `blep::{poly_blep, clamp_pulse_width}`, and
`envelope::{Adsr, Stage, ZERO_THRESHOLD}`. O2 adds the effects' primitives here.

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own a copy that differs from these — mono-00's ADSR (its
machine's 0.4 ms floor), mono-01's (without the decay stall fix), mono-pr1's envelope, mono-03's
`tan_approx` (libm's `tan` under that name), para-07's and mono-08's BLEPs and pulse clamp: those stay
with their instruments, and each instrument's NOTES says which of its items moved and which stayed.

# Local Contracts

- **An item moves here when its copies are byte-identical, or on the owner's ruling for one that
  differs.** A parametrised version of differing copies is a new decision, never a refactor: it
  changes what some instrument does, or needs a parameter only one instrument sets.
- **Bit-exact, for ever**: every instrument's golden digests were recorded through these bodies.
  Changing an expression here changes several instruments' sound at once; it needs a plan and the
  goldens of every consumer.
- **Plain values in and out**: no framework types, no allocation; `#[inline]` as the copies had it
  (`#[inline(always)]` for `flush`). `#![forbid(unsafe_code)]`.
- **The knee, the time constants and the thresholds are the copies'** and stay named where they
  are defined; a consumer's own constants stay in its crate.
- Zero dependencies and MSRV **1.87**, verified with the toolchain.

# Verification

```bash
cargo +1.87.0 test -p mxm-dsp
cargo clippy -p mxm-dsp --all-targets -- -D warnings
```

# Child DOX Index

None.
