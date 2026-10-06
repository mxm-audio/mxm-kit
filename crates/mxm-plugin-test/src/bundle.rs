//! The bundle a plugin builds into: its section in the `bundler.toml` at the root of whatever
//! repository holds it, which `xtask` reads at bundle time and the plugin never does — so only a test
//! can catch the two names disagreeing.
//!
//! The file is found by walking up from the plugin's manifest rather than at a fixed `../..`, so the
//! check holds in the collection's workspace and in a repository of the plugin's own, where the
//! plugin sits at a different depth (`plans/plan-repo-split.md`, in the private archive).

use std::path::Path;

/// The display name the nearest `bundler.toml` at or above `manifest_dir` gives `package`'s bundle.
/// Panics when there is no such file, no `[package]` section, or no `name` in it.
pub fn display_name(manifest_dir: &str, package: &str) -> String {
    let Some(bundler) = Path::new(manifest_dir)
        .ancestors()
        .map(|dir| dir.join("bundler.toml"))
        .find(|path| path.is_file())
    else {
        panic!("no bundler.toml at or above {manifest_dir}");
    };
    let text = std::fs::read_to_string(&bundler)
        .unwrap_or_else(|error| panic!("{} could not be read: {error}", bundler.display()));
    let section = format!("[{package}]");
    let entry = text
        .split(&section)
        .nth(1)
        .unwrap_or_else(|| panic!("{} has no `{section}` section", bundler.display()));
    entry
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .find_map(|line| line.trim().strip_prefix("name"))
        .and_then(|rest| rest.split('"').nth(1))
        .unwrap_or_else(|| panic!("{}'s `{section}` sets no display name", bundler.display()))
        .to_owned()
}

/// Panics unless `package`'s bundle is displayed as `name`: the plugin calls itself what its bundle
/// is called. Pass `env!("CARGO_MANIFEST_DIR")`, `env!("CARGO_PKG_NAME")` and the plugin's `NAME`.
pub fn is_named(manifest_dir: &str, package: &str, name: &str) {
    let display = display_name(manifest_dir, package);
    assert_eq!(
        display, name,
        "bundler.toml would bundle this plugin as {display:?}, but it calls itself {name:?}"
    );
}
