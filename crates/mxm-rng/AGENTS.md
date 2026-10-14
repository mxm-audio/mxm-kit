# AGENTS.md — crates/mxm-rng

Parent: [`../../AGENTS.md`](../../AGENTS.md).

# Purpose

The collection's pseudo-random generators: `Xorshift32`, `Xorshift64`, `Xorshift64Star`,
`SplitMix64` and `Lcg32`, each bit-exact to the copies it replaced (the architecture audit's O3,
owner-approved 2026-10-13), and `map`, the laws that turn a drawn word into a float. Every noise
source, sample-and-hold, scatter and test stream in the collection draws from one of them.

# Ownership

Owns `src/`, `tests/` and `Cargo.toml`. Does **not** own a consumer's seeds, its seeding rule
(`seed | 1`, `.max(1)`, a multiply) or a mapping only it uses: those are decisions about its sound
and stay with it. Does not own a machine's own register — the drum machine's 909 LFSR is the
machine — nor the drum machine's GF(2) jump-ahead, whose one user builds it on `step`.

# Local Contracts

- **Bit-exact, for ever.** A golden digest or a recorded render depends on every stream here, so a
  generator, a constructor or a `map` law never changes what it returns. `tests/reference.rs` pins
  each against the first outputs of the copies it replaced, printed by those copies before they
  went; a failure there is a moved render somewhere in the collection, never a test to update.
- **A consumer never changes family.** Choosing a generator for new code is free; moving an existing
  stream to another is a change to its sound.
- **`new` replaces a zero xorshift seed with the word's golden-ratio constant; `from_state` keeps
  the state as given**, zero included, for a consumer whose own seeding rule decides. SplitMix64's
  and Lcg32's `new` is `from_state`: every state is valid.
- **The exact laws are the default** (`unit_f32`, `bipolar_f32`, `unit_f64`, `bipolar_f64`: the top
  bits a float holds). `map::unit_f32_closed` and `map::unit_f32_word` are inexact, reach 1.0, and
  exist only because shipped renders were recorded with them; new code never takes them.
- **A law with one user stays with that user** (fx-oldify's f64 word law, mxm-measure's). It joins
  `map` when a second copy appears.
- **Realtime-safe**: every draw is `#[inline]`, allocates nothing and cannot panic.
  `#![forbid(unsafe_code)]`.
- **Every family names its source** in the crate's doc table; a sixth family arrives with its second
  copy, never ahead of it.
- Zero dependencies and MSRV **1.87**, verified with the toolchain.

# Verification

```bash
cargo +1.87.0 test -p mxm-rng
cargo clippy -p mxm-rng --all-targets -- -D warnings
```

# Child DOX Index

None.
