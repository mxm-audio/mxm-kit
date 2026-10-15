# AGENTS.md — crates/mxm-keys

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The shared keyboard language's engine: physical keys in, finished gestures out, and the plain-text
keymap that puts its jobs on keys. Navigate with the arrows; then a verb (MOVE, EXTENT, VALUE,
SELECT, DUPLICATE), optionally a step size (COARSE, FINE, MICRO), and a direction; held together
like the M8, or tapped one key at a time like Blender. Zero dependencies, no egui and MSRV
**1.87**, so newDAWn, MXM Player and any instrument, open or closed, can take it.

**The language is newDAWn's keyboard design** (its `docs/keyboard.md`, in newDAWn's repository,
private until it is ready), which the owner made the law for the whole collection on 2026-10-06.
Every instrument's editor reads it through `mxm_ui::navigation`'s cursor since 2026-10-08 (piloted
on mxm-mono-08 from 2026-10-07), and design system §11 states it. The cursor over every widget,
`mxm_ui::reach`, is built on this engine.

# Ownership

- **`key`**: `Key`, a physical position named after its US legend; `Direction`; `Mods`, where
  `command` is the platform's shortcut modifier.
- **`keymap`**: `Verb`, `Step`, `Action` and `Job`; `Keymap`, read from text with line-numbered
  errors; `Settings` (`tap-arms`, `one-arrow`, `timeout`); the host's keys, `panel.<name>` and
  note entry's `note.*`; `SHIPPED`, the keymap in `keymaps/`, the default and the only one;
  `Keymap::default()` is `DEFAULT`'s.
- **`engine`**: `Engine`, the state machine; `Output`, what each input means (`Begin` when a
  verb is armed); `Arrows`, what the arrows do now, for the arrow map.
- **`examples/replay.rs`**: presses in, the engine's outputs out, one per line; newDAWn's key
  study checks its press model with it.

**Not here:** what a gesture changes, focus and spatial navigation, drawing the arrow map, reading
keys from a toolkit, and where a keymap file lives. Those are the host's; the folder newDAWn and
every MXM editor share is `mxm_ui::keymaps`'.

# Local Contracts

- **The default keymap's bindings are checked by `the_default_is_the_key_studys` alone**: every
  other test here, in `mxm-ui` and in newDAWn presses jobs, or uses a keymap of its own on
  purpose (the engine's rule tests their `KEYS`; others the default with a key moved, found from
  the keymap), so a remap of `study.keymap` changes that one test.
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
- **One keymap ships, tuned by the owner's practice**, so the keys are the same in every host
  (newDAWn's owner, 2026-10-13); a user's own keymap file still changes them. It is the default,
  `study.keymap`, from newDAWn's key study of 2026-10-06 (its `docs/key-study.md`): 26 known
  songs made again as an experienced user would, every learnable layout priced for finger travel
  and comfortable chords, held and tapped. Verbs on the top row, step sizes on the home row,
  actions on the bottom; OUT on Tab until a host reads Caps Lock, the study's pick, as a key; MENU,
  the right-click menu of what the cursor is on, on R (newDAWn's owner, 2026-10-14). A change to it
  changes the study's record first.
- **A keymap without a line for a job reads, the job unbound** (`a_keymap_without_menu_reads_with_menu_unbound`),
  so a user's copy written before a job joined keeps its keys.
- **On a value a step key is VALUE at its size** (newDAWn's owner, 2026-10-09): the host says so
  before each key (`Engine::set_on_value`), from where its cursor is once the key before is applied,
  and the step key arms VALUE as a verb's key does, held or tapped; ← → are the value's too, for
  the host to snap. An older keymap's `musical =` line reads and binds nothing.
  `on_a_value_a_step_key_is_value_at_its_size` and
  `a_keymap_written_with_musical_still_reads_and_its_key_is_free` hold it.
- **VIEW and MICRO together pick the view's edge** (newDAWn's owner, 2026-10-15: a view's size by
  the keys): `Output::ViewEdge`, held or tapped in either order, VIEW on its key or its modifier,
  on a value too, where VIEW after MICRO takes the VALUE it armed, before a step, for the edge. No
  new job, so no keymap changes; an editor has no views to size and drops it (`mxm_ui::navigation`).
  `view_and_micro_together_give_the_views_edge_held_or_tapped_in_either_order` and
  `on_a_value_view_after_micro_takes_it_for_the_edge_without_a_step` hold it.
- **The host's keys are not jobs** (newDAWn's owner, 2026-10-08): `panel.<name>` (the names are
  the host's) and note entry's `note.keys`, `note.octave-down`, `note.octave-up` sit in the same
  file, so one file binds every key. A panel key is no job's and no other panel's, so the engine
  passes it through as `Raw`; note entry's keys may be jobs' keys, being read only in note entry,
  but never an arrow, OUT's, BACK's or a panel's. The editors never read them.
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
