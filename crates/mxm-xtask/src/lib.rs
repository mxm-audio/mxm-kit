//! The build tooling every MXM repository's `cargo xtask` shares: a thin wrapper over
//! `nice_plug_xtask`, which produces the loadable `.clap`, followed by staging each plugin's control
//! map beside its bundle — an instrument owns its mapping, so the two must ship together. See
//! [`control_maps`]. Then each plugin bundled is installed in the user's CLAP folder, so the hosts
//! there load the last one built. See [`install`].
//!
//! `cargo xtask fetch` builds the plugins of other MXM repositories that this repository's tests
//! load, at the tags `test-bundles.txt` pins. See [`fetch`].
//!
//! A repository's own `xtask` is a few lines that pass its workspace root to [`main`], plus any
//! command only that repository needs (MXM Player's `fixtures`).

pub mod control_maps;
pub mod fetch;
pub mod install;

use std::path::Path;

/// Runs `fetch`, or `nice_plug_xtask` on the command line it was given. A `bundle` (or
/// `bundle-universal`) first checks the control maps of the plugins in `workspace_root`'s
/// `bundler.toml` against the standard, and after building stages them beside the bundles and
/// installs the bundles.
///
/// `workspace_root` is the caller's own: the parent of its `env!("CARGO_MANIFEST_DIR")`. This crate
/// cannot know it, because its own manifest directory is wherever the dependency was fetched to.
pub fn main(workspace_root: &Path) -> nice_plug_xtask::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("fetch") {
        for path in fetch::fetch(workspace_root)? {
            println!("Placed {}", path.display());
        }
        return Ok(());
    }
    let bundling = install::bundled_packages(&args);
    if bundling.is_some() {
        // Fatal, and before the build: a map every host would refuse is a broken product, as a
        // failing test is. (A missing map is not; see below.)
        control_maps::check(workspace_root)?;
    }
    nice_plug_xtask::main()?;

    if let Some(packages) = bundling {
        let staged = match control_maps::stage(workspace_root) {
            Ok(staged) => {
                for path in &staged {
                    println!("Staged control map {}", path.display());
                }
                staged
            }
            // Never fatal: a bundle without its map is still a working plugin, just one whose
            // roles are unfilled until the map is put beside it.
            Err(error) => {
                eprintln!("Control maps were not staged: {error:#}");
                Vec::new()
            }
        };
        let switch = std::env::var(install::SWITCH).ok();
        if install::wanted(switch.as_deref(), std::env::var_os("CI").is_some()) {
            install_bundles(workspace_root, &packages, &staged);
        }
    }
    Ok(())
}

/// Installs what the bundle made, saying what went in and what didn't. Never fatal: a bundle a test
/// made still succeeds when a host holds the installed copy.
fn install_bundles(workspace_root: &Path, packages: &[String], staged: &[std::path::PathBuf]) {
    let Some(folder) = install::user_folder() else {
        eprintln!("Not installed: this user's CLAP folder isn't known (no LOCALAPPDATA or HOME)");
        return;
    };
    // The target folder as nice-plug's bundler finds it, so `CARGO_TARGET_DIR` moves both.
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(workspace_root.join("Cargo.toml"))
        .no_deps()
        .exec();
    let bundled = match metadata {
        Ok(metadata) => metadata.target_directory.as_std_path().join("bundled"),
        Err(error) => {
            eprintln!("Not installed: the target folder isn't known: {error}");
            return;
        }
    };
    match control_maps::bundles(workspace_root) {
        Ok(bundles) => {
            let outcomes = install::install(&bundles, packages, &bundled, staged, &folder);
            install::report(&outcomes, &folder);
        }
        Err(error) => eprintln!("Not installed: {error:#}"),
    }
}
