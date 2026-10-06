# AGENTS.md — crates/mxm-control-map

# Purpose

The collection's control map as code and data: `control-map.json`, the **collection standard**
(roles, fixed knobs, the bank, the pages), and the schema both it and an instrument's own
`control-map.json` are read with — `Layout`, `InstrumentMap`, their parsing and validation, and the
string-to-CLAP parameter id hash. The normative contract is
[`docs/MXM_CONTROL_MAP.md`](../../docs/MXM_CONTROL_MAP.md); this crate implements it.

It was MXM Player's `src/control_map/schema.rs` and `docs/control-map.json` until the collection's
split into one repository per product (`plans/plan-repo-split.md` in the private archive, Phase 1): the player is not the
only reader — every plugin's test holds its own map to the standard, and a DAW will read it too.

# Ownership

Owns `Cargo.toml`, `control-map.json` and `src/`. How a host *applies* a map — takeover, curves at
run time, the user's overlay file, what the player shows — stays with the host
(mxm-player's [`apps/mxm-player/AGENTS.md`](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/AGENTS.md), `src/control_map/`). An instrument's
own map stays with the instrument (`plugins/<plugin>/control-map.json`).

# Local Contracts

- **Data, not code.** Adding an instrument means adding its map file, never editing this crate or a
  host. A role is permanent once the standard reaches 1.0 (`docs/MXM_CONTROL_MAP.md`).
- **`SHIPPED` is compiled in** from `control-map.json`, so a host is never without a valid layout;
  the crate's own tests prove it parses and pin its page count.
- **A schema version mismatch is refused with a reason**, never parsed hopefully (`SCHEMA_VERSION`).
- No dependency beyond serde and serde_json. MSRV is the workspace's, inherited rather than lowered,
  because every consumer is at the GUI floor already.

# Work Guidance

- A change to `control-map.json` is a pass over every instrument's map at once
  (`docs/MXM_CONTROL_MAP.md`, *before 1.0*). Each repository's `cargo xtask bundle` checks its own
  maps against the standard (`mxm-xtask`'s `control_maps::check`), so a change here reaches every
  product the next time it bundles against the new kit tag. MXM Player's `tests/t5_control_map.rs`
  checks how the host applies a map.

# Verification

```bash
cargo test -p mxm-control-map              # the shipped layout parses; schema and validation rules
cargo clippy -p mxm-control-map --all-targets
cargo test -p mxm-xtask --lib control_maps   # a map that breaks the standard fails the bundle
```

# Child DOX Index

None.
