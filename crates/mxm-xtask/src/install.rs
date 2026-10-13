//! Installs each plugin a bundle made into the user's CLAP folder, where newDAWn and the other
//! hosts look, so the plugin a host loads is always the last one built (newDAWn's owner,
//! 2026-10-13: "the plugins by default are compiled into the default clap folder. So the latest ones
//! are always in the daw and bitwig"). nice-plug's bundler can't: it writes to `target/bundled`
//! alone.
//!
//! On by default, off with `MXM_INSTALL_CLAP=0` and under CI ([`wanted`]). Never fatal: a bundle a
//! test made still succeeds when its install doesn't, and says why.
//!
//! **A plugin a host holds is replaced all the same.** Windows refuses to overwrite or delete a
//! loaded library but lets it be renamed, so the installed copy is set aside as
//! `<name>.clap.<stamp>.old` and the new one renamed in. A host already running keeps the old code
//! until it loads the plugin again, and the `.old` goes at a later install, once nothing holds it.
//! Hosts list only `*.clap`, so no leftover, nor the lock file, is ever offered as a plugin.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// The variable that turns installing on (`1`) or off (`0`).
pub const SWITCH: &str = "MXM_INSTALL_CLAP";

/// The file each install locks in the CLAP folder, so two bundles of one plugin take turns.
const LOCK: &str = ".mxm-install.lock";

/// How many folders deep [`found_first`] looks, as newDAWn's scan (`catalog::SCAN_DEPTH`).
const SCAN_DEPTH: usize = 16;

/// Whether a bundle installs, from [`SWITCH`] and whether `CI` is set. Set, the variable decides:
/// `1` installs and any other value (`0`) doesn't. Unset, a bundle installs unless under CI (GitHub
/// Actions sets `CI`), so CI runners don't.
pub fn wanted(switch: Option<&str>, ci: bool) -> bool {
    match switch {
        Some(value) => value == "1",
        None => !ci,
    }
}

/// The packages a `bundle` or `bundle-universal` command line names, read as nice-plug reads them
/// (`-p a -p b …`, or the first argument), or `None` for any other command. `args` leaves out the
/// program's name.
pub fn bundled_packages(args: &[String]) -> Option<Vec<String>> {
    let (command, rest) = args.split_first()?;
    if command != "bundle" && command != "bundle-universal" {
        return None;
    }
    let mut packages = Vec::new();
    if rest.first().map(String::as_str) == Some("-p") {
        let mut rest = rest;
        while let [flag, package, after @ ..] = rest
            && flag == "-p"
        {
            packages.push(package.clone());
            rest = after;
        }
    } else if let Some(package) = rest.first() {
        packages.push(package.clone());
    }
    Some(packages)
}

/// This user's CLAP folder, which newDAWn, Bitwig and the other hosts read and which needs no
/// administrator: `None` when the variable it is found from isn't set.
pub fn user_folder() -> Option<PathBuf> {
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA")?;
        Some(
            PathBuf::from(local)
                .join("Programs")
                .join("Common")
                .join("CLAP"),
        )
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Audio/Plug-Ins/CLAP"))
    } else {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".clap"))
    }
}

/// The folders newDAWn scans before the user's (`catalog::standard_folders`): those in `CLAP_PATH`,
/// then the system's. A plugin found twice keeps its first bundle.
fn scanned_first() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = std::env::var_os("CLAP_PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    if cfg!(windows) {
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            folders.push(PathBuf::from(common).join("CLAP"));
        }
    } else if cfg!(target_os = "macos") {
        folders.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    }
    folders
}

/// Installs each of `packages` that `bundles` (`bundler.toml`'s `(package, bundle name)` pairs)
/// names, from `bundled` into `folder`, with its control map if `staged` holds one. One outcome per
/// plugin, the installed path or why not; never an error, so a failed install never fails the
/// bundle.
pub fn install(
    bundles: &[(String, String)],
    packages: &[String],
    bundled: &Path,
    staged: &[PathBuf],
    folder: &Path,
) -> Vec<(String, Result<PathBuf>)> {
    bundles
        .iter()
        .filter(|(package, _)| packages.contains(package))
        .map(|(_, bundle)| {
            let map_name = format!("{bundle}.control-map.json");
            let map = staged.iter().find(|path| {
                path.file_name()
                    .is_some_and(|name| name == map_name.as_str())
            });
            (bundle.clone(), install_one(bundle, bundled, map, folder))
        })
        .collect()
}

/// Prints each outcome of [`install`], and a warning for a copy newDAWn finds before the one
/// installed.
pub fn report(outcomes: &[(String, Result<PathBuf>)], folder: &Path) {
    for (bundle, outcome) in outcomes {
        match outcome {
            Ok(path) => {
                println!("Installed {}", path.display());
                let name = path.file_name().unwrap_or_default();
                for copy in found_first(name, folder) {
                    eprintln!(
                        "Warning: {} is found before the one installed, so newDAWn loads it \
                         instead; remove it to use this build",
                        copy.display()
                    );
                }
            }
            // The error says whether the plugin went in: "not installed: …", or installed but
            // not its control map.
            Err(error) => eprintln!("{bundle}: {error:#}"),
        }
    }
}

/// Every bundle called `name` in the folders newDAWn scans before `folder`, and their subfolders.
fn found_first(name: &std::ffi::OsStr, folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for first in scanned_first() {
        if first != folder {
            find(&first, name, 0, &mut found);
        }
    }
    found
}

fn find(folder: &Path, name: &std::ffi::OsStr, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name() == name && (cfg!(target_os = "macos") || path.is_file()) {
            found.push(path);
        } else if path.is_dir() && path.extension().is_none_or(|extension| extension != "vst3") {
            find(&path, name, depth + 1, found);
        }
    }
}

fn install_one(
    bundle: &str,
    bundled: &Path,
    map: Option<&PathBuf>,
    folder: &Path,
) -> Result<PathBuf> {
    let installed = folder.join(format!("{bundle}.clap"));
    let installed_map = folder.join(format!("{bundle}.control-map.json"));
    // Held until this install ends, its map placed too.
    let _lock = put_in(
        &bundled.join(format!("{bundle}.clap")),
        &installed,
        &installed_map,
        folder,
    )
    .context("not installed")?;
    if let Some(map) = map {
        place(map, &installed_map).with_context(|| {
            format!(
                "{} is installed, but not its control map",
                installed.display()
            )
        })?;
    }
    Ok(installed)
}

/// The plugin put in at `installed`, under the folder's lock, which it returns. The installed map
/// is set aside first and gone once the plugin is in, so a map that then fails to go in leaves
/// none beside the new plugin, never an older one; if the plugin can't go in, the map goes back.
fn put_in(
    source: &Path,
    installed: &Path,
    installed_map: &Path,
    folder: &Path,
) -> Result<std::fs::File> {
    if !source.exists() {
        bail!("the bundle wasn't made at {}", source.display());
    }
    std::fs::create_dir_all(folder)
        .with_context(|| format!("{} can't be made", folder.display()))?;
    let lock_path = folder.join(LOCK);
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("{} can't be opened", lock_path.display()))?;
    lock.lock()
        .with_context(|| format!("{} can't be locked", lock_path.display()))?;

    clear_leftovers(installed);
    clear_leftovers(installed_map);
    let map_aside = set_aside(installed_map)?;
    if let Err(error) = place(source, installed) {
        if let Some(map_aside) = &map_aside {
            let _ = std::fs::rename(map_aside, installed_map);
        }
        return Err(error);
    }
    if let Some(map_aside) = map_aside {
        let _ = remove(&map_aside);
    }
    Ok(lock)
}

/// What an install of `installed` left beside it: a copy that was cut off before it went in, and
/// every copy set aside that nothing holds any more. One still held stays for a later install.
fn clear_leftovers(installed: &Path) {
    let _ = remove(&with_suffix(installed, "tmp"));
    let _ = remove(&with_suffix(installed, "new"));
    let (Some(folder), Some(name)) = (installed.parent(), installed.file_name()) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let aside = format!("{}.", name.to_string_lossy());
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with(&aside) && file_name.ends_with(".old") {
            let _ = remove(&entry.path());
        }
    }
}

/// Puts `source`, a file or a macOS bundle folder, at `destination` whole: copied beside it first,
/// then renamed in, so a host never finds half of one and an old bundle is never merged into.
fn place(source: &Path, destination: &Path) -> Result<()> {
    let fresh = with_suffix(destination, if source.is_dir() { "new" } else { "tmp" });
    remove(&fresh)?;
    let copied = copy(source, &fresh).and_then(|()| swap_in(&fresh, destination));
    if copied.is_err() {
        let _ = remove(&fresh);
    }
    copied
}

/// Renames `fresh` to `destination`. Over a file a host holds, which Windows won't replace, the
/// held one is renamed aside first, and put back if the fresh one still can't go in.
fn swap_in(fresh: &Path, destination: &Path) -> Result<()> {
    if fresh.is_file() && std::fs::rename(fresh, destination).is_ok() {
        return Ok(());
    }
    let aside = set_aside(destination)?;
    if let Err(error) = std::fs::rename(fresh, destination) {
        if let Some(aside) = &aside {
            let _ = std::fs::rename(aside, destination);
        }
        return Err(error).with_context(|| format!("{} can't be put in", destination.display()));
    }
    if let Some(aside) = aside {
        // Gone now unless a host holds it; then at a later install.
        let _ = remove(&aside);
    }
    Ok(())
}

/// Renames what is at `path`, if anything, to `<name>.<stamp>.old` beside it, which a host holding
/// it allows on Windows where replacing or removing it doesn't. Returns where it went.
fn set_aside(path: &Path) -> Result<Option<PathBuf>> {
    if std::fs::symlink_metadata(path).is_err() {
        return Ok(None);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    let aside = with_suffix(path, &format!("{stamp}.old"));
    std::fs::rename(path, &aside).with_context(|| {
        format!(
            "{} can't be replaced or moved aside; close the hosts using it and bundle again",
            path.display()
        )
    })?;
    Ok(Some(aside))
}

/// `path` with `.suffix` added to its whole name: `a.clap` becomes `a.clap.suffix`.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

/// Copies a file, or a folder and everything in it.
fn copy(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to).with_context(|| format!("{} can't be made", to.display()))?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(from, to)
            .with_context(|| format!("{} can't be copied to {}", from.display(), to.display()))?;
    }
    Ok(())
}

/// Removes a file or a folder, if there is one.
fn remove(path: &Path) -> Result<()> {
    let removed = match std::fs::symlink_metadata(path) {
        Err(_) => return Ok(()),
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
    };
    removed.with_context(|| format!("{} can't be removed", path.display()))
}

#[cfg(test)]
mod tests;
