# AGENTS.md — crates/mxm-stretch

Parent: [`../../AGENTS.md`](../../AGENTS.md).

# Purpose

**A recording stretched to an exact length, its pitch kept**: the collection's WSOLA, made a
whole-buffer stretch, for a host that stretches audio off its audio thread. newDAWn is the first
consumer: a recording note follows the song's tempo with it (newDAWn's
`plans/plan-audio-stretch.md`). An owner-approved exception to evidence-first extraction (the
owner, 2026-10-16: "Should it not be a shared crate?"); the creative sampler can move its Stretch
reader onto it later, as a change of its own.

# Ownership

Owns `src/`, `tests/`, `examples/` and `Cargo.toml`.
- `lib.rs`: `stretch`, its cases and `Error`, `window_frames`, the two heads and where they're laid.
- `search.rs`: the splice search. `period.rs`: the period track that narrows it. `onsets.rs`: the
  onset detector. `read.rs`: the sixteen-tap sinc read.
- `examples/listen.rs`: writes a recording stretched by the ratios given, to listen to.

The four ported files came from `mxm-creative-sampler-dsp` (GPL-3.0-or-later), **relicensed MIT by
their author** for this crate (2026-10-16); each file's header says what it took and what changed.
The sampler's own copies stay with it, under its licence, and are not owned here.

# Local Contracts

- **`stretch`'s cases are decided in the order its documentation lists**, the first that applies
  winning: the refusals, no frames, no input, the same length (a copy, bit for bit), material
  shorter than two windows (copied, cut or followed by silence), then the stretch. A new case gets a
  place in that order and a test.
- **The method is the sampler's**, its constants named as they were: the window
  (`STRETCH_WINDOW_S`), the search (`ALIGN_*`), the period track (`PERIOD_*`, `PITCH_*`), the
  onsets (`TRANSIENT_*`). The measurements behind them are the sampler's `NOTES.md`; a change to
  one is measured here, by its tests and by ear, never copied back by assumption.
- **Attacks are read once, from their first frame, where the ratio puts them**: no head reads past
  the next onset before that onset's place, and both are laid on it there.
- **No ramps**: the output starts on the input's own frames, no head reads past either end, and the
  window sum is divided out. Stretched longer, the output ends on the input's own last frame.
- **Every buffer sized by the input or the output is reserved fallibly** (`Error::NoMemory`), held
  by `tests/allocation_refusal.rs`; buffers sized by the rate alone (the lag buffers) and the sinc
  kernel, built once, are the only infallible ones.
- **Deterministic** per build and platform; bit-exact pins are Windows' (the workspace's rule).
- Zero dependencies (the example's dev-dependencies are the kit's audio file crates) and MSRV
  **1.87**, verified with the toolchain. `#![forbid(unsafe_code)]`.

# Work Guidance

- A listening round: `cargo run --release -p mxm-stretch --example listen -- <recording> <folder>
  0.8 1.25`, into a folder under `target/`; the owner listens.

# Verification

```bash
cargo +1.87.0 test -p mxm-stretch
cargo clippy -p mxm-stretch --all-targets -- -D warnings
```

# Child DOX Index

None.
