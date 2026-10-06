# AGENTS.md — crates/mxm-xtask

# Purpose

The build tooling every MXM repository's `cargo xtask` shares: `main` runs `nice_plug_xtask`, which
produces the loadable `.clap`, and after a `bundle` stages each plugin's control map beside its
bundle (`control_maps`). `cargo xtask fetch` builds the plugins of *other* MXM repositories that
this repository's tests load, at the tags its `test-bundles.txt` pins (`fetch`). A repository's own
`xtask` passes its workspace root and adds only the commands that repository alone needs.

It was the root `xtask`'s `main.rs` and `control_maps.rs` until the collection's split into one
repository per product (`plans/plan-repo-split.md`, Phase 1): every product repository bundles, and
one copy of the tooling is what keeps them bundling the same way.

# Ownership

Owns `Cargo.toml` and `src/{lib.rs, control_maps.rs, fetch.rs}`. Reads the caller's `bundler.toml`
for display names, `plugins/<crate>/control-map.json` for the maps and `test-bundles.txt` for the
plugins to fetch; all three belong to the repository being built.

# Local Contracts

- **Stay thin.** `main` delegates to `nice_plug_xtask`. Add behaviour here only when the upstream
  tool genuinely cannot do it, and say why in the module doc.
- **The caller passes its workspace root.** This crate's own manifest directory is wherever the
  dependency was fetched to, so it never derives the root itself.
- **A control map ships with its instrument.** `bundle` copies `plugins/<crate>/control-map.json` to
  `target/bundled/<Bundle Name>.control-map.json`, which is where MXM Player looks for it. A plugin
  without one is not an error — it simply has no roles filled. Staging never fails the bundle: an
  unmapped plugin still works.
- **`bundle` builds the *outermost* workspace on the path, so a workspace nested inside another
  bundles the outer one.** `nice_plug_xtask::chdir_workspace_root` walks `CARGO_MANIFEST_DIR` from
  the root and stops at the first directory holding a workspace manifest, while the control maps go
  to the caller's own `target/bundled/`. Found 2026-09-04 building `mxm-chorus-06` from a worktree
  inside the repository. Keep worktrees, and repositories, out of any directory that holds a
  workspace manifest of its own.
- **`fetch` builds another repository the way it builds itself.** It clones the pinned tag into
  `target/fetched/<repository>-<tag>` and runs that repository's own `cargo xtask bundle` (release)
  or `cargo build` (debug, where nice-plug's allocation guard is compiled in), then copies the
  `.clap` and its control map into `target/bundled/`, or the bare library into `target/debug/`. A
  clone already there is reused, because a tag does not move. `test-bundles.txt` is
  `repository tag profile` per line, the repository name doubling as the package and bundle name.
- `bundler.toml` is parsed by hand rather than with a `toml` dependency. Six lines of flat key-value
  do not justify a pinned crate.
- `publish = false`. Dependencies pinned exactly.

# Work Guidance

A new plugin needs a row in its repository's `bundler.toml` mapping crate name to display bundle
name; no change here is required for it, and its `control-map.json` is picked up from the same row.

# Verification

```bash
cargo xtask bundle mxm-mono-01 --release    # -> target/bundled/mxm-mono-01.clap
                                            #    + mxm-mono-01.control-map.json
cargo xtask fetch                           # every line of test-bundles.txt, built and placed
cargo test -p mxm-xtask                     # test-bundles.txt parsing
cargo clippy -p mxm-xtask --all-targets
```

The check is that the artifacts appear at the paths above and that
`clap-validator validate "target/bundled/mxm-mono-01.clap"` can load the bundle.

# Child DOX Index

None.
