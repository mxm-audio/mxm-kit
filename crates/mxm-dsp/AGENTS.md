# AGENTS.md — crates/mxm-dsp

Parent: [`../../AGENTS.md`](../../AGENTS.md).

# Purpose

The collection's shared DSP primitives, each moved verbatim from copies the instruments carried
byte for byte (the architecture audit's O1 and O2, owner-approved 2026-10-13): `guard::{flush,
finite_or}`, `shape::{tanh_approx, tan_approx, diode_clamp}`, `blep::{poly_blep, clamp_pulse_width}`,
`envelope::{Adsr, Stage, ZERO_THRESHOLD}` and `delay::AgeLine`, the line read by age that the
effects' and synths' delays store their samples in.

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own a copy that differs from these — mono-00's ADSR (its
machine's 0.4 ms floor), mono-01's (without the decay stall fix), mono-pr1's envelope, mono-03's
`tan_approx` (libm's `tan` under that name), para-07's and mono-08's BLEPs and pulse clamp: those stay
with their instruments, and each instrument's NOTES says which of its items moved and which stayed.
Nor the effects' other flushes (`MIN_POSITIVE`, `1e-30`, NaN-zeroing) or their SVFs and biquads: unifying
those changes bits and goldens, and the owner has been asked. fx-delay's generation-tagged line is its
own (its crate publishes the memory it costs).

# Local Contracts

- **An item moves here when its copies are byte-identical, or on the owner's ruling for one that
  differs.** A parametrised version of differing copies is a new decision, never a refactor: it
  changes what some instrument does, or needs a parameter only one instrument sets.
- **Bit-exact, for ever**: every instrument's golden digests were recorded through these bodies.
  Changing an expression here changes several instruments' sound at once; it needs a plan and the
  goldens of every consumer.
- **Plain values in and out**: no framework types, no allocation after construction (`AgeLine::new`
  and `try_new` alone allocate); `#[inline]` as the copies had it (`#[inline(always)]` for `flush`).
  `#![forbid(unsafe_code)]`.
- **The line keeps each consumer's exact length**, so its memory and history are what they were; it
  never flushes or clamps (a consumer's write flush, its clamp of a requested age and its interpolation
  where it is not `read_linear`'s stay at its call site), and age 1 is the newest pushed sample — a
  consumer that called its unadvanced slot age 0 reads `age + 1`.
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
