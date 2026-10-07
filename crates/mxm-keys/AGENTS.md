# AGENTS.md — crates/mxm-keys

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The shared keyboard language's engine: physical keys in, finished gestures out, and the plain-text
keymap that puts its jobs on keys. Navigate with the arrows; then a verb (MOVE, EXTENT, VALUE,
SELECT), optionally a step size (COARSE, FINE, MICRO, MUSICAL), and a direction; held together
like the M8, or tapped one key at a time like Blender. Zero dependencies, no egui and MSRV
**1.87**, so newDAWn, MXM Player and any instrument, open or closed, can take it.

**The language is newDAWn's keyboard design** (its `docs/keyboard.md`, in newDAWn's repository,
private until it is ready), which the owner made the law for the whole collection on 2026-10-06.
The instruments' editors still run `mxm_ui::navigation`'s cursor, design system §11, until they
convert, as the owner decided on 2026-10-07; until then §11 binds them. The cursor over every
widget, `mxm_ui::reach`, is built on this engine.

# Ownership

- **`key`**: `Key`, a physical position named after its US legend; `Direction`; `Mods`, where
  `command` is the platform's shortcut modifier.
- **`keymap`**: `Verb`, `Step`, `Action` and `Job`; `Keymap`, read from text with line-numbered
  errors; `Settings` (`tap-arms`, `one-arrow`, `timeout`); `SHIPPED`, the keymaps in `keymaps/`,
  the default first; `Keymap::default()` is `DEFAULT`'s.
- **`engine`**: `Engine`, the state machine; `Output`, what each input means; `Arrows`, what the
  arrows do now, for the arrow map.
- **`examples/replay.rs`**: presses in, the engine's outputs out, one per line; newDAWn's key
  study checks its press model with it.

**Not here:** what a gesture changes, focus and spatial navigation, drawing the arrow map, reading
keys from a toolkit, and where a keymap file lives. Those are the host's.

# Local Contracts

- **The rules are the engine module's documentation and each has a test named after it.** A change
  to a rule changes newDAWn's `docs/keyboard.md` first, by the owner's decision, then the test.
- **No keymap can take a standard shortcut away.** The format can't write a chord with `Command`,
  and the engine passes every `Command` press through as `Raw`; only `Command+Z` during a gesture
  that has stepped is taken, to cancel it. `standard_shortcuts_reach_the_panel_in_every_shipped_keymap`
  holds this.
- **The keys must work in newDAWn and the collection's own editors; other hosts are secondary**
  (the owner, 2026-10-07: "Other daws are secondary. They can use the mouse if the host does not
  send the keys"). Nothing works around a host that keeps a key from a plugin's window.
- **A gesture's steps end in exactly one `Finish` or `Cancel`**, and a gesture with no steps ends
  silently, so a host can preview steps and commit them as one undo step.
- **The shipped keymaps are starting points, tuned by the owner's practice.** The default,
  `study.keymap`, comes from newDAWn's key study of 2026-10-06 (its `docs/key-study.md`): 26 known
  songs made again as an experienced user would, every learnable layout priced for finger travel
  and comfortable chords, held and tapped. Verbs on the top row, step sizes on the home row,
  actions on the bottom; OUT on Tab until a host reads Caps Lock, the study's pick, as a key.
  `notes` and `home-row` stay for comparison. A change to the default changes the study's
  record first.
- **Zero dependencies and no `let` chains**: MSRV 1.87 is verified, not assumed.

# Work Guidance

None yet.

# Verification

```bash
cargo test -p mxm-keys
cargo +1.87.0 test -p mxm-keys
cargo clippy -p mxm-keys --all-targets -- -D warnings
```

# Child DOX Index

None.
