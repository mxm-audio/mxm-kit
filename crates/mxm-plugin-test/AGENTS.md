# AGENTS.md — crates/mxm-plugin-test

# Purpose

The headless checks every plugin's tests share, as one library: `keyboard_checks` (the keyboard
cursor's coverage), `paging_checks` (real-card reachability on the dynamic pager), `opening_size`
(the derived opening size and the bar at the minimum), `tree_checks` (layout-tree cards and the
review pictures), `routing_checks` (route parameters held to the DSP's
`mxm_modulation::conformance::Declaration`), `text_checks` (every parameter's text through the
host's round trip, in every plugin, and the census a text refactor is diffed with),
`time_text_checks` (a time reading's host text round
trip), `bundle` (the plugin's display name in the nearest `bundler.toml`, found by walking up
from its manifest so the check holds at any depth) and `hover_text` (the words a player reads on
hover and in a host's plugin browser speak about the sound, never the machine or the code — design
system §7.6), and `formats` (the VST3 identity held to the CLAP one: the class ID the CLAP id's
FNV-1a hash, the subcategories the CLAP features' by one table; `docs/plugin-conventions.md`
*Every plugin is a CLAP and a VST3*). What each check proves and does not prove is in its module's
docs. Beside them,
`NoHost`: the GUI context a headless editor test hands its `ParamSetter` when no edit it makes
matters, once rather than in each plugin's test module (eighteen identical copies until
`plans/plan-architecture-audit.md` B2). A test that asserts on edits takes `keyboard_checks::Recorder`
or a host of its own. `mxm-preset`'s tests take it too: they kept a copy while this crate depended
on the preset system through `mxm-modulation-params`, which `plans/plan-mxm-params.md` P1 ended.

They were `plugins/*_test_support.rs` files that each plugin `include!`d, until the collection's split
into one repository per product (`plans/plan-repo-split.md` in the private archive, Phase 1): a file outside a plugin's own
folder cannot travel with it, and a crate can.

# Ownership

Owns `Cargo.toml` and `src/`. The rules the checks enforce belong to their owners — the editor
contract to each plugin repository's `plugins/AGENTS.md` and
[`docs/plugin-conventions.md`](../../docs/plugin-conventions.md#editor-contract), the layout to
[`crates/ui/AGENTS.md`](../ui/AGENTS.md) and the design system, routing to
[`crates/mxm-modulation/AGENTS.md`](../mxm-modulation/AGENTS.md). This crate holds the measuring code
only.

# Local Contracts

- **The checks press jobs, not keys** (`keyboard_checks::key_of` and its `VALUE`, `COARSE`,
  `MICRO`, `VIEW`, `OUT`, through `mxm_ui::navigation::default_key`), and so do the plugins' own key
  tests: a remap of the default keymap changes no test (the owner, 2026-10-08). `operates` holds
  that VALUE + ↑ and COARSE + ↑ (VALUE implied on a value, 2026-10-09) each edit as one
  balanced host gesture, and that VIEW + an arrow selects a card without editing.
- **A `[dev-dependencies]` entry only, never a normal one.** Nothing here may reach a bundle; the
  check under *Verification* holds it.
- **One copy for every plugin.** A check is kept whole, not trimmed or varied for one consumer; a
  plugin runs the checks that apply to it, and one it does not call is not a defect.
- **No fixed path.** The checks take what they measure as arguments (a panel, a size, a parameter
  set, the plugin's manifest directory) and write nowhere of their own; a plugin's test chooses
  where its pictures go. Nothing assumes where the plugin sits in a repository.
- **`text_checks` is the union of the walks it replaced, and every plugin calls it** (`plans/plan-mxm-params.md`
  P2): ten whole-parameter walks, each probing what its author had last been bitten by, and five
  plugins with none. A plugin passes only the switch points the collection's list does not name.
  A plugin check that probes more — route readings, a model selector's unavailable ids — stays in
  the plugin beside it. **The census is the one thing here that writes**, and only into the
  directory `MXM_TEXT_CENSUS` names: a refactor tool, diffed before and after, never a golden,
  because text through a skewed range goes through the platform's `powf`.
- **`hover_text`'s word lists are the contract**: extend them when a new leak is found, never loosen
  them to pass. They were one collection-wide test in `xtask`; each plugin now runs them on its own
  sources, so a plugin carries the rule with it.
- MSRV is the GUI floor (1.95): every check paints a real panel through egui.

# Work Guidance

- A change here reaches every plugin's tests: run them all, in each plugin's own repository; no
  bundle is needed, because a dev-dependency cannot change one. (Until the split the merge gate,
  `scripts/merge_gate.py` in the private archive, ran them that way; there is no gate now.)

# Verification

```bash
cargo clippy -p mxm-plugin-test --all-targets -- -D warnings
cargo test -p mxm-plugin-test --lib         # the formats table and hash; the other checks are tested by their consumers
cargo check --tests -p <each plugin>        # in each plugin's repository: every consumer still compiles against it
```

The shipped graph must not contain this crate: the leak check in
[`crates/mxm-measure/AGENTS.md`](../mxm-measure/AGENTS.md) *Verification* names it alongside the
measurement crates, over every shipped package with dev edges excluded.

# Child DOX Index

None.
