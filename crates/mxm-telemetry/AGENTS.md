# AGENTS.md — crates/mxm-telemetry

Parent: [`../../AGENTS.md`](../../AGENTS.md). The detail behind the rules: [NOTES.md](NOTES.md).

# Purpose

What every plugin's DSP→editor channel shares: `PeakMeter`, the level and its clip latch, and
`DevRequests`, the developer channel's four request slots. Each plugin's `telemetry.rs` holds them
as fields beside what is its own (an envelope, a modulator, stage levels).

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own a plugin's own telemetry, nor the drawing of a meter
(`crates/ui`), nor the developer channel's CC numbers and gate (`docs/plugin-conventions.md`).

# Local Contracts

- **Atomics only, no locks, no allocation**: the audio thread writes, the editor reads, and the
  editor may drop frames. `#![forbid(unsafe_code)]`.
- **One meter policy for every plugin.** A finite peak is max-combined by magnitude and reset on
  read; a peak at or above full scale latches the clip; **a non-finite peak latches the clip and is
  never stored as the level**; only the user clears the clip
  ([NOTES.md § The policy](NOTES.md#the-policy-and-the-five-it-replaced)).
- **A meter is composed, never forked.** A plugin with several levels holds one `PeakMeter` per
  level, and its light reads the meter whose samples it judges. A DSP that reports clipping itself
  calls `latch_clip`, and the meter's own rule still applies to every published peak.
- **Callers go through the field**: `telemetry.meter.publish(peak)`, `telemetry.dev.take_view()`.
  No plugin keeps delegating methods, and each act has one name.
- **Each request is taken once.** A view or theme is clamped below the no-request sentinel.
- Zero dependencies and MSRV **1.87**, verified with the toolchain.

# Verification

```bash
cargo +1.87.0 test -p mxm-telemetry
cargo clippy -p mxm-telemetry --all-targets -- -D warnings
```

# Child DOX Index

None.
