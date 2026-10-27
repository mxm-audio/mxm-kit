# AGENTS.md — crates/mxm-xtask

# Purpose

The build tooling every MXM repository's `cargo xtask` shares: `main` runs `nice_plug_xtask`, which
produces the loadable `.clap` (and the `.vst3` of a plugin that exports one), and after a `bundle`
writes each VST3's `moduleinfo.json` (`module_info`), stages each plugin's control map beside its
bundle (`control_maps`) and installs the CLAP, its map and the VST3 in the standard plugin folders
(`install`). `cargo xtask
fetch` builds the plugins of *other* MXM repositories that this repository's tests load, at the tags
its `test-bundles.txt` pins (`fetch`). A repository's own `xtask` passes its workspace root and adds
only the commands that repository alone needs.

It was the root `xtask`'s `main.rs` and `control_maps.rs` until the collection's split into one
repository per product (`plans/plan-repo-split.md` in the private archive, Phase 1): every product repository bundles, and
one copy of the tooling is what keeps them bundling the same way.

# Ownership

Owns `Cargo.toml` and
`src/{lib.rs, control_maps.rs, fetch.rs, install.rs, install/tests.rs, module_info.rs, presets.rs}`.
Reads the caller's `bundler.toml` for display names, `plugins/<crate>/control-map.json` for the maps
and `test-bundles.txt` for the plugins to fetch; all three belong to the repository being built.

# Local Contracts

- **Stay thin.** `main` delegates to `nice_plug_xtask`. Add behaviour here only when the upstream
  tool genuinely cannot do it, and say why in the module doc.
- **The caller passes its workspace root.** This crate's own manifest directory is wherever the
  dependency was fetched to, so it never derives the root itself.
- **A control map ships with its instrument.** `bundle` copies `plugins/<crate>/control-map.json` to
  `target/bundled/<Bundle Name>.control-map.json`, which is where MXM Player looks for it. A plugin
  without one is not an error — it simply has no roles filled. Staging never fails the bundle: an
  unmapped plugin still works.
- **A plugin's factory presets ship with it** (`presets`; the owner, 2026-10-27, in
  `docs/plugin-conventions.md`): `bundle` checks `plugins/<crate>/presets/` before the build
  (fatal: every `.json` a preset of one plugin, `order.txt` naming only files there), then stages
  its `.json`s and order as `<Bundle Name>.presets/` in the repository's `target/bundled`, where
  the map is staged whichever folder Cargo builds in, and inside the `.vst3` Cargo built (in the
  folder `CARGO_TARGET_DIR` moves) as `Contents/Resources/presets/`, each replacing what an
  earlier bundle left; staging is never fatal. The install is handed the staged folder with the
  map and puts it beside the `.clap` as it does the map, set aside with it before the plugin goes
  in and retired with it, so an older set never stays beside a newer plugin.
- **Every bundle installs itself**, the CLAP with its map and, when the bundle made one, the VST3,
  where newDAWn, Bitwig and the other hosts look, so a host always loads the last build (newDAWn's
  owner, 2026-10-13, choosing every build over every commit: "So the latest ones are always in the
  daw and bitwig"). Here because nice-plug's bundler writes to `target/bundled` alone. `bundle` and
  `bundle-universal` alike, only the packages the command line bundled that `bundler.toml` names,
  each with the map staged in the same run, or none: the installed map is set aside before the
  plugin goes in, so an older map never stays beside a newer plugin. The map goes beside the
  `.clap` only: MXM Player keys a map by the CLAP id.
  - **The folders are the standard ones, and the user's to change** (the owner, 2026-10-16: "Lets
    keep it as standard as possible", "It should also be user changable"): `install::folder` is
    what `MXM_CLAP_DIR` or `MXM_VST3_DIR` names, else `install::standard_folder`, on Windows the
    system's (`%COMMONPROGRAMFILES%\CLAP`, `…\VST3`), which every host reads unconfigured and the
    developer's account is given write access to once, and on macOS and Linux this user's.
    `python ws.py install` in the workspace keeps the same rules (`plugin_folder`).
  - **The older per-user CLAP goes only once the new one is in** (`install::retire`): on Windows,
    after a plugin's install into another folder succeeds, its own files in
    `%LOCALAPPDATA%\Programs\Common\CLAP` (the `.clap`, its map, their leftovers, by exact name) are
    removed under that folder's lock, or hosts reading both list it twice. A failed install
    removes nothing.
  - **A VST3 is installed only when this build made it**: `install::clear_stale_vst3s` takes each
    bundled package's `.vst3` out of `target/bundled` before the build, because nice-plug's bundler
    leaves an older one where it is when the library no longer exports one. One it can't remove
    fails the bundle, as one nice-plug can't replace does. The VST3 folder is needed only then.
  - **A destination that is where the bundle is made or its map staged is refused**, touching
    nothing (`MXM_CLAP_DIR` naming a `target/bundled`, however spelled): installing there would set
    aside the map it copies from.
  - **On by default** (`install::wanted`): set, `MXM_INSTALL_CLAP` decides for every format, `1`
    installing and any other value (`0`) not; unset, a bundle installs unless `CI` is set.
  - **Never fatal**: a failed install says why and the bundle succeeds, so a host holding the
    installed plugin, or a folder not yet writable, never fails a test's bundle.
  - **A plugin a host holds is replaced all the same.** Windows won't overwrite or delete a loaded
    library but renames it (tried 2026-10-13 on a loaded `mxm-mono-01.clap`), and renames a `.vst3`
    bundle folder with its module loaded (tried 2026-10-16), so the held copy goes aside as
    `<name>.<extension>.<stamp>.old` and the new one is renamed in; a `.old` goes at a later
    install, once nothing holds it, as does a `.tmp` or `.new` an install cut off left. Installs
    take turns on `.mxm-install.lock` in each folder. Hosts list only `*.clap` and `*.vst3`.
  - A bundle of the same name in a folder newDAWn scans before the one installed into
    (`install::scan_order`: `CLAP_PATH` or `VST3_PATH` first) is warned about, as is a folder
    newDAWn doesn't scan at all.
  - The tests install into temporary folders, never the real ones; the held cases load a copy of
    a system library with `LoadLibraryW`, as a host loads a plugin.
- **Each VST3 carries its `moduleinfo.json`** (`Contents/Resources/`), which nice-plug's bundler
  doesn't make: after the build, before the install copies the bundle, `module_info::write` loads
  the module just built through `mxm_vst3_host::module_info` and writes what its factory says, as
  the SDK's own build does with `moduleinfotool`. In CI and under `ws install` too, since it is part
  of the bundle, not of installing. Never fatal: a VST3 without the file is listed by loading it,
  and one that can't be made (a module built for another architecture) has any file there removed,
  since a file that isn't this build's is worse than none.
- **A map that does not hold to the standard fails the bundle**, before anything is built
  (`control_maps::check`, the check a host makes on loading it). That is what keeps a broken map from
  shipping now that no collection-wide test reads every product's map; it replaced MXM Player's
  `every_shipped_product_map_loads_against_the_standard` at the split.
- **`bundle` builds the *outermost* workspace on the path, so a workspace nested inside another
  bundles the outer one.** `nice_plug_xtask::chdir_workspace_root` walks `CARGO_MANIFEST_DIR` from
  the root and stops at the first directory holding a workspace manifest, while the control maps go
  to the caller's own `target/bundled/`. Found 2026-09-04 building `mxm-chorus-06` from a worktree
  inside the repository. Keep worktrees, and repositories, out of any directory that holds a
  workspace manifest of its own.
- **`fetch` builds another repository the way it builds itself.** It clones the pinned tag into
  `fetch::fetch_dir()` — `MXM_FETCH_DIR`, else `<CARGO_HOME>/mxm-fetch` — **outside every workspace**,
  for the reason above: its first version cloned into this repository's `target/fetched/`, and the
  bundler then looked for the fetched plugin in this repository (0.3.1). It runs that repository's
  own `cargo xtask bundle` (release)
  or `cargo build` (debug, where nice-plug's allocation guard is compiled in), then copies the
  `.clap` and its control map into `target/bundled/`, or the bare library into `target/debug/`. A
  clone already there is reused, because a tag does not move. `test-bundles.txt` is
  `repository tag profile` per line, the repository name doubling as the package and bundle name.
  **`fetch` never installs** (`MXM_INSTALL_CLAP=0` for its builds): a plugin at an old tag must not
  replace the last one built.
- `bundler.toml` is parsed by hand rather than with a `toml` dependency. Six lines of flat key-value
  do not justify a pinned crate.
- `publish = false`. Dependencies pinned exactly.

# Work Guidance

A new plugin needs a row in its repository's `bundler.toml` mapping crate name to display bundle
name; no change here is required for it, and its `control-map.json` is picked up from the same row.

# Verification

```bash
cargo xtask bundle mxm-mono-00 --release    # -> target/bundled/mxm-mono-00.clap, its
                                            #    control map and .vst3 (with its
                                            #    moduleinfo.json), all installed
cargo xtask fetch                           # every line of test-bundles.txt, built and placed
cargo test -p mxm-xtask                     # test-bundles.txt parsing, installing
cargo clippy -p mxm-xtask --all-targets
```

The check is that the artifacts appear at the paths above, `Installed <path>` is printed for each
format, and that `clap-validator validate "target/bundled/mxm-mono-00.clap"` can load the bundle.

# Child DOX Index

None.
