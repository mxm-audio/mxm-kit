# AGENTS.md — crates/mxm-plugin-test

# Purpose

The headless checks every plugin's tests share, as one library: `keyboard_checks` (the keyboard
cursor's coverage), `paging_checks` (real-card reachability on the dynamic pager), `opening_size`
(the derived opening size and the bar at the minimum), `tree_checks` (layout-tree cards and the
review pictures), `routing_checks` (route parameters held to the DSP's
`mxm_modulation::conformance::Declaration`), `time_text_checks` (a time reading's host text round
trip), `bundle` (the plugin's display name in the nearest `bundler.toml`, found by walking up
from its manifest so the check holds at any depth) and `hover_text` (the words a player reads on
hover and in a host's plugin browser speak about the sound, never the machine or the code — design
system §7.6). What each check proves and does not prove is in its module's docs.

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
  `MICRO`, `OUT`, through `mxm_ui::navigation::default_key`), and so do the plugins' own key
  tests: a remap of the default keymap changes no test (the owner, 2026-10-08).
- **A `[dev-dependencies]` entry only, never a normal one.** Nothing here may reach a bundle; the
  check under *Verification* holds it.
- **One copy for every plugin.** A check is kept whole, not trimmed or varied for one consumer; a
  plugin runs the checks that apply to it, and one it does not call is not a defect.
- **No fixed path.** The checks take what they measure as arguments (a panel, a size, a parameter
  set, the plugin's manifest directory) and write nowhere of their own; a plugin's test chooses
  where its pictures go. Nothing assumes where the plugin sits in a repository.
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
cargo check --tests -p <each plugin>        # in each plugin's repository: every consumer still compiles against it
```

The shipped graph must not contain this crate: the leak check in
[`crates/mxm-measure/AGENTS.md`](../mxm-measure/AGENTS.md) *Verification* names it alongside the
measurement crates, over every shipped package with dev edges excluded.

# Child DOX Index

None.
