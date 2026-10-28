//! The build tooling every MXM repository's `cargo xtask` shares: a thin wrapper over
//! `nice_plug_xtask`, which produces the loadable `.clap`, followed by staging each plugin's control
//! map beside its bundle — an instrument owns its mapping, so the two must ship together. See
//! [`control_maps`]. Each VST3 gets its `moduleinfo.json` ([`module_info`]). Each plugin's factory
//! presets are staged too ([`presets`]). Then each plugin bundled is installed, CLAP and VST3, in
//! the standard plugin folders, so the hosts load the last one built, and its presets in their
//! folder for its version under the local data directory. See [`install`].
//!
//! `cargo xtask fetch` builds the plugins of other MXM repositories that this repository's tests
//! load, at the tags `test-bundles.txt` pins. See [`fetch`].
//!
//! A repository's own `xtask` is a few lines that pass its workspace root to [`main`], plus any
//! command only that repository needs (MXM Player's `fixtures`).

pub mod control_maps;
pub mod fetch;
pub mod install;
pub mod module_info;
pub mod presets;

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
    if let Some(packages) = &bundling {
        // Fatal, and before the build: a map every host would refuse is a broken product, as a
        // failing test is. (A missing map is not; see below.)
        control_maps::check(workspace_root)?;
        // Fatal too: a preset that doesn't parse is a broken product, and is shipped as a file.
        presets::check(workspace_root)?;
        // Fatal too: an earlier VST3 left in `target/bundled` would be installed as this build's.
        if let (Ok(bundled), Ok(bundles)) = (
            bundled_folder(workspace_root),
            control_maps::bundles(workspace_root),
        ) {
            install::clear_stale_vst3s(&bundles, packages, &bundled)?;
        }
    }
    nice_plug_xtask::main()?;

    if let Some(packages) = bundling {
        // Before the install, which copies each bundle as it stands.
        if let (Ok(bundled), Ok(bundles)) = (
            bundled_folder(workspace_root),
            control_maps::bundles(workspace_root),
        ) {
            module_info::write(&bundles, &packages, &bundled);
        }
        // Before the install too, which takes each set from where it is staged. Never fatal, as
        // a map isn't: a plugin without its presets still plays.
        let staged_presets = match presets::stage(workspace_root) {
            Ok(staged) => {
                for path in &staged {
                    println!("Staged presets {}", path.display());
                }
                staged
            }
            Err(error) => {
                eprintln!("Presets were not staged: {error:#}");
                Vec::new()
            }
        };
        let mut staged = match control_maps::stage(workspace_root) {
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
        staged.extend(staged_presets);
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
    let folders = match install::Folders::chosen() {
        Ok(folders) => folders,
        Err(error) => {
            eprintln!("Not installed: {error:#}");
            return;
        }
    };
    let bundled = match bundled_folder(workspace_root) {
        Ok(bundled) => bundled,
        Err(error) => {
            eprintln!("Not installed: the target folder isn't known: {error}");
            return;
        }
    };
    // Each package's version, the folder its presets go in. Unknown, a plugin still installs, and
    // says it went in without its presets.
    let versions = package_versions(workspace_root).unwrap_or_default();
    match control_maps::bundles(workspace_root) {
        Ok(bundles) => {
            let outcomes =
                install::install(&bundles, packages, &bundled, staged, &versions, &folders);
            install::report(&outcomes, &folders);
        }
        Err(error) => eprintln!("Not installed: {error:#}"),
    }
}

fn metadata(workspace_root: &Path) -> Result<cargo_metadata::Metadata, cargo_metadata::Error> {
    cargo_metadata::MetadataCommand::new()
        .manifest_path(workspace_root.join("Cargo.toml"))
        .no_deps()
        .exec()
}

/// `target/bundled` as nice-plug's bundler finds it, so `CARGO_TARGET_DIR` moves both.
fn bundled_folder(workspace_root: &Path) -> Result<std::path::PathBuf, cargo_metadata::Error> {
    Ok(metadata(workspace_root)?
        .target_directory
        .as_std_path()
        .join("bundled"))
}

/// The version of each package in the workspace, by name.
fn package_versions(
    workspace_root: &Path,
) -> Result<std::collections::HashMap<String, String>, cargo_metadata::Error> {
    Ok(metadata(workspace_root)?
        .packages
        .into_iter()
        .map(|package| (package.name.to_string(), package.version.to_string()))
        .collect())
}
