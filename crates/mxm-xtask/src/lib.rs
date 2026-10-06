//! The build tooling every MXM repository's `cargo xtask` shares: a thin wrapper over
//! `nice_plug_xtask`, which produces the loadable `.clap`, followed by staging each plugin's control
//! map beside its bundle — an instrument owns its mapping, so the two must ship together. See
//! [`control_maps`].
//!
//! A repository's own `xtask` is a few lines that pass its workspace root to [`main`], plus any
//! command only that repository needs (MXM Player's `fixtures`).

pub mod control_maps;

use std::path::Path;

/// Runs `nice_plug_xtask` on the command line it was given and, after a `bundle`, stages the
/// control maps of the plugins in `workspace_root`'s `bundler.toml`.
///
/// `workspace_root` is the caller's own: the parent of its `env!("CARGO_MANIFEST_DIR")`. This crate
/// cannot know it, because its own manifest directory is wherever the dependency was fetched to.
pub fn main(workspace_root: &Path) -> nice_plug_xtask::Result<()> {
    let bundling = std::env::args().nth(1).as_deref() == Some("bundle");
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
