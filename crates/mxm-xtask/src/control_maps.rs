//! Stages each instrument's control map beside its bundle.
//!
//! An instrument owns its own mapping — see `docs/MXM_CONTROL_MAP.md`. Nobody is obliged to
//! install the whole collection, so the map has to travel with the `.clap` rather than living in
//! the player, which would need a release every time an instrument was added.
//!
//! `plugins/<crate>/control-map.json` becomes `target/bundled/<Bundle Name>.control-map.json`,
//! where the bundle name comes from `bundler.toml` — the same name `cargo xtask bundle` uses, so
//! the two land side by side.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The file an instrument's map is written as, in its own crate.
const SOURCE_NAME: &str = "control-map.json";

/// Copies every instrument's control map into `target/bundled`.
///
/// Run after `bundle`. A plugin with no map is not an error: mapping is optional, and a plugin
/// without one simply has no roles filled.
pub fn stage(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    // Deliberately not canonicalised: on Windows that yields an extended-length `\?\` path, which
    // is correct but unreadable in the line this prints.
    let bundled = workspace_root.join("target").join("bundled");

    let mut staged = Vec::new();
    for (crate_dir, bundle_name) in bundle_names(workspace_root)? {
        let source = crate_dir.join(SOURCE_NAME);
        if !source.exists() {
            continue;
        }
        std::fs::create_dir_all(&bundled)
            .with_context(|| format!("Could not create {}", bundled.display()))?;

        let destination = bundled.join(format!("{bundle_name}.control-map.json"));
        std::fs::copy(&source, &destination).with_context(|| {
            format!(
                "Could not copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
        staged.push(destination);
    }
    Ok(staged)
}

/// Reads `bundler.toml` for the crate-to-bundle-name mapping.
///
/// Parsed by hand rather than with a TOML dependency: the file is a flat list of
/// `[crate]` / `name = "..."` pairs, and adding a dependency to read six lines is not a trade
/// this repo makes lightly.
fn bundle_names(workspace_root: &Path) -> Result<Vec<(PathBuf, String)>> {
    let path = workspace_root.join("bundler.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Could not read {}", path.display()))?;

    let mut out = Vec::new();
    let mut current: Option<String> = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('[')
            && let Some(name) = rest.strip_suffix(']')
        {
            current = Some(name.trim().to_owned());
            continue;
        }
        if let Some(rest) = line.strip_prefix("name")
            && let Some((_, value)) = rest.split_once('=')
            && let Some(crate_name) = current.as_ref()
        {
            let bundle_name = value.trim().trim_matches('"').to_owned();
            out.push((workspace_root.join("plugins").join(crate_name), bundle_name));
        }
    }
    Ok(out)
}
