# AGENTS.md — crates/mxm-editor

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The plugin editor's window, the same for every MXM plugin (the architecture audit's O7, the owner,
2026-10-14: *"make the mxm-editor"*): `create` (the title, the reference size at zoom 1, resizable down
to a minimum), `EditorApp` (the egui app: `mxm_ui::shell::install` when the window opens, the host's
context held while it is open and dropped when it closes) and `Panel`, the trait each plugin's editor
implements.

# Ownership

Owns `src/` and `Cargo.toml`: the window and its lifecycle. Does **not** own what an editor draws or
the state it keeps (the plugin's `Panel`), nor a plugin's own work when the window opens or closes —
the `opened` and `closed` hooks are where that stays (committing a prepared response, settling staged
assets, an editor-open flag), each given the host's context.

# Local Contracts

- **The one crate that knows nice-plug's editor.** `mxm-ui` stays framework-free (the player draws it
  as a pane); anything here that does not need nice-plug belongs in `mxm-ui` instead.
- `install` runs before `opened`; `closed` runs once a window has opened, with the context it opened
  with; nothing is drawn while no window is open.
- Every plugin's window is resizable down to its minimum, the cards reflowing; scaling is the app
  bar's zoom. A plugin that needs another policy is a new decision here, not a local override.
- MSRV is the GUI floor (1.95). The kit patches `egui-baseview` to the fork every plugin repository
  uses, so this crate is built and tested on the window stack that ships.

# Verification

```bash
cargo test -p mxm-editor
cargo clippy -p mxm-editor --all-targets -- -D warnings
```

# Child DOX Index

None.
