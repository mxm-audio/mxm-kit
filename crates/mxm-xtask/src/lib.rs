//! The build tooling every MXM repository's `cargo xtask` shares: a thin wrapper over
//! `nice_plug_xtask`, which produces the loadable `.clap`, followed by staging each plugin's control
//! map beside its bundle — an instrument owns its mapping, so the two must ship together. See
//! [`control_maps`].
//!
//! `cargo xtask fetch` builds the plugins of other MXM repositories that this repository's tests
//! load, at the tags `test-bundles.txt` pins. See [`fetch`].
//!
//! A repository's own `xtask` is a few lines that pass its workspace root to [`main`], plus any
//! command only that repository needs (MXM Player's `fixtures`).

pub mod control_maps;
pub mod fetch;

use std::path::Path;

/// Runs `fetch`, or `nice_plug_xtask` on the command line it was given. A `bundle` first checks the
/// control maps of the plugins in `workspace_root`'s `bundler.toml` against the standard, and after
/// building stages them beside the bundles.
///
/// `workspace_root` is the caller's own: the parent of its `env!("CARGO_MANIFEST_DIR")`. This crate
/// cannot know it, because its own manifest directory is wherever the dependency was fetched to.
pub fn main(workspace_root: &Path) -> nice_plug_xtask::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("fetch") {
        for path in fetch::fetch(workspace_root)? {
            println!("Placed {}", path.display());
        }
        return Ok(());
    }
    let bundling = std::env::args().nth(1).as_deref() == Some("bundle");
    if bundling {
        // Fatal, and before the build: a map every host would refuse is a broken product, as a
        // failing test is. (A missing map is not; see below.)
        control_maps::check(workspace_root)?;
    }
    nice_plug_xtask::main()?;

    if bundling {
        match control_maps::stage(workspace_root) {
            Ok(staged) => {
                for path in staged {
                    println!("Staged control map {}", path.display());
                }
            }
            // Never fatal: a bundle without its map is still a working plugin, just one whose
            // roles are unfilled until the map is put beside it.
            Err(error) => eprintln!("Control maps were not staged: {error:#}"),
        }
    }
    Ok(())
}
