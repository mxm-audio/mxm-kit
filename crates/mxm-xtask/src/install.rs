//! Installs each plugin a bundle made, as CLAP and, when the plugin exports one, VST3, where the
//! hosts look, so the plugin a host loads is always the last one built (newDAWn's owner,
//! 2026-10-13: "the plugins by default are compiled into the default clap folder. So the latest ones
//! are always in the daw and bitwig"). nice-plug's bundler can't: it writes to `target/bundled`
//! alone.
//!
//! **The folders are the standard ones, and the user's to change** (the owner, 2026-10-16: "Lets
//! keep it as standard as possible", and "It should also be user changable"): [`folder`] is the
//! folder `MXM_CLAP_DIR` or `MXM_VST3_DIR` names, else [`standard_folder`]: on Windows the system
//! folders, where every installer puts a plugin and every host looks unconfigured (JUCE's
//! copy-after-build default too, which warns they may need write access granted once); on macOS
//! and Linux this user's. The control map goes beside the `.clap` only: a host reads MXM's roles
//! from the CLAP.
//!
//! On by default, off with `MXM_INSTALL_CLAP=0` and under CI ([`wanted`]). Never fatal: a bundle a
//! test made still succeeds when its install doesn't, and says why.
//!
//! **A plugin a host holds is replaced all the same.** Windows refuses to overwrite or delete a
//! loaded library but lets it be renamed, and lets a `.vst3` bundle folder be renamed with its
//! module loaded (tried 2026-10-16), so the installed copy is set aside as
//! `<name>.<extension>.<stamp>.old` and the new one renamed in. A host already running keeps the old
//! code until it loads the plugin again, and the `.old` goes at a later install, once nothing holds
//! it. Hosts list only `*.clap` and `*.vst3`, so no leftover, nor the lock file, is ever offered as
//! a plugin.

use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The variable that turns installing on (`1`) or off (`0`), for every format.
pub const SWITCH: &str = "MXM_INSTALL_CLAP";

/// The file each install locks in its folder, so two bundles of one plugin take turns.
const LOCK: &str = ".mxm-install.lock";

/// How many folders deep [`found_first`] looks, as newDAWn's scan (`catalog::SCAN_DEPTH`).
const SCAN_DEPTH: usize = 16;

/// A format a bundle installs as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Clap,
    Vst3,
}

impl Format {
    /// The bundle's extension, which is also what a host lists.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Clap => "clap",
            Format::Vst3 => "vst3",
        }
    }

    /// The variable that chooses where this format installs.
    pub fn variable(self) -> &'static str {
        match self {
            Format::Clap => "MXM_CLAP_DIR",
            Format::Vst3 => "MXM_VST3_DIR",
        }
    }

    /// The variable listing the folders a host searches before its own, as the format defines it.
    fn search_variable(self) -> &'static str {
        match self {
            Format::Clap => "CLAP_PATH",
            Format::Vst3 => "VST3_PATH",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Format::Clap => "CLAP",
            Format::Vst3 => "VST3",
        }
    }
}

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

/// Where `format` installs: the folder [`Format::variable`] names, else [`standard_folder`].
/// `None` when neither is known.
pub fn folder(format: Format) -> Option<PathBuf> {
    chosen(std::env::var_os(format.variable()), standard_folder(format))
}

/// The folder a set, non-empty variable names, else the standard one.
fn chosen(variable: Option<OsString>, standard: Option<PathBuf>) -> Option<PathBuf> {
    match variable {
        Some(named) if !named.is_empty() => Some(PathBuf::from(named)),
        _ => standard,
    }
}

/// The folder everyone installs `format` in: on Windows the system's, `%COMMONPROGRAMFILES%\CLAP`
/// and `…\VST3`; on macOS this user's `~/Library/Audio/Plug-Ins/…`; on Linux `~/.clap` and
/// `~/.vst3`. `None` when the variable it is found from isn't set.
pub fn standard_folder(format: Format) -> Option<PathBuf> {
    if cfg!(windows) {
        let common = std::env::var_os("COMMONPROGRAMFILES")?;
        Some(PathBuf::from(common).join(format.name()))
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Audio/Plug-Ins")
                .join(format.name()),
        )
    } else {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(format!(".{}", format.extension())))
    }
}

/// This user's CLAP folder on Windows, where CLAP installs went before they went to the system
/// folder. An MXM plugin's copy there is removed once its install elsewhere has succeeded
/// ([`retire`]), or hosts reading both folders list it twice. `None` elsewhere: there the
/// standard folder is still this user's.
pub fn retired_folder() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("Programs")
            .join("Common")
            .join("CLAP"),
    )
}

/// The folders newDAWn scans for `format`, in its order: those its search variable lists, then
/// the platform's (its `catalog::standard_folders` and `mxm_vst3_host::standard_folders`). A
/// plugin found twice keeps its first bundle.
fn scan_order(format: Format) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = std::env::var_os(format.search_variable())
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(windows) {
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            folders.push(PathBuf::from(common).join(format.name()));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            folders.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("Common")
                    .join(format.name()),
            );
        }
    } else if cfg!(target_os = "macos") {
        let user = home.map(|home| home.join("Library/Audio/Plug-Ins").join(format.name()));
        let system = PathBuf::from("/Library/Audio/Plug-Ins").join(format.name());
        match format {
            Format::Clap => {
                folders.push(system);
                folders.extend(user);
            }
            Format::Vst3 => {
                folders.extend(user);
                folders.push(system);
                folders.push(PathBuf::from("/Network/Library/Audio/Plug-Ins/VST3"));
            }
        }
    } else {
        folders.extend(home.map(|home| home.join(format!(".{}", format.extension()))));
        match format {
            Format::Clap => folders.push(PathBuf::from("/usr/lib/clap")),
            Format::Vst3 => {
                folders.push(PathBuf::from("/usr/lib/vst3"));
                folders.push(PathBuf::from("/usr/local/lib/vst3"));
            }
        }
    }
    folders
}

/// Where each format goes, and the folder an older install of the CLAP is taken out of. The VST3's
/// is needed only when a bundle made one, so a CLAP-only plugin installs without it.
#[derive(Clone, Debug)]
pub struct Folders {
    pub clap: PathBuf,
    pub vst3: Option<PathBuf>,
    pub retired: Option<PathBuf>,
}

impl Folders {
    /// [`folder`] for each format and [`retired_folder`], or why the CLAP's isn't known.
    pub fn chosen() -> Result<Self> {
        Ok(Self {
            clap: folder(Format::Clap).with_context(|| unknown(Format::Clap))?,
            vst3: folder(Format::Vst3),
            retired: retired_folder(),
        })
    }

    fn of(&self, format: Format) -> Option<&Path> {
        match format {
            Format::Clap => Some(&self.clap),
            Format::Vst3 => self.vst3.as_deref(),
        }
    }
}

fn unknown(format: Format) -> String {
    format!(
        "the {} folder isn't known: set {}",
        format.name(),
        format.variable()
    )
}

/// Takes each of `packages`' VST3 out of `bundled` before a build: nice-plug's bundler makes a
/// `.vst3` only when the library exports one and leaves an older one where it is, so without this a
/// plugin that stopped exporting one would have the old one installed as if it were this build's.
/// A bundle that can't be removed fails the build, as one nice-plug can't replace does.
pub fn clear_stale_vst3s(
    bundles: &[(String, String)],
    packages: &[String],
    bundled: &Path,
) -> Result<()> {
    for (_, bundle) in bundles
        .iter()
        .filter(|(package, _)| packages.contains(package))
    {
        let stale = bundled.join(format!("{bundle}.vst3"));
        remove(&stale).context("an earlier VST3 bundle is in the way; close what holds it")?;
    }
    Ok(())
}

/// What one install of one format did.
#[derive(Debug)]
pub struct Outcome {
    pub bundle: String,
    pub format: Format,
    /// The installed bundle, or why it isn't.
    pub result: Result<PathBuf>,
    /// An older copy that should have gone and couldn't: hosts reading its folder list the plugin
    /// twice until it does.
    pub left_behind: Vec<PathBuf>,
}

/// Installs each of `packages` that `bundles` (`bundler.toml`'s `(package, bundle name)` pairs)
/// names, from `bundled` into `folders`: the CLAP, with its control map if `staged` holds one, and
/// the VST3 when the bundle made one. One outcome per plugin and format; never an error, so a
/// failed install never fails the bundle.
pub fn install(
    bundles: &[(String, String)],
    packages: &[String],
    bundled: &Path,
    staged: &[PathBuf],
    folders: &Folders,
) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    for (_, bundle) in bundles
        .iter()
        .filter(|(package, _)| packages.contains(package))
    {
        let map_name = format!("{bundle}.control-map.json");
        let map = staged.iter().find(|path| {
            path.file_name()
                .is_some_and(|name| name == map_name.as_str())
        });
        let result = install_one(bundle, Format::Clap, bundled, map, &folders.clap);
        // The older copy goes only once this one is in, so a failed install leaves a working one.
        let left_behind = match (&result, &folders.retired) {
            (Ok(_), Some(retired)) if !may_be_same_folder(retired, &folders.clap) => {
                retire(bundle, retired)
            }
            _ => Vec::new(),
        };
        outcomes.push(Outcome {
            bundle: bundle.clone(),
            format: Format::Clap,
            result,
            left_behind,
        });
        if bundled.join(format!("{bundle}.vst3")).exists() {
            let result = match &folders.vst3 {
                Some(folder) => install_one(bundle, Format::Vst3, bundled, None, folder),
                None => Err(anyhow::anyhow!(unknown(Format::Vst3))).context("not installed"),
            };
            outcomes.push(Outcome {
                bundle: bundle.clone(),
                format: Format::Vst3,
                result,
                left_behind: Vec::new(),
            });
        }
    }
    outcomes
}

/// Prints each outcome of [`install`]: what went in, a copy newDAWn finds before the one installed,
/// a folder newDAWn doesn't scan, and an older copy that couldn't be taken out.
pub fn report(outcomes: &[Outcome], folders: &Folders) {
    for outcome in outcomes {
        let label = format!("{} ({})", outcome.bundle, outcome.format.name());
        match &outcome.result {
            Ok(path) => {
                println!("Installed {}", path.display());
                let Some(folder) = folders.of(outcome.format) else {
                    continue;
                };
                let order = scan_order(outcome.format);
                if scanning_root(folder, &order).is_none() {
                    eprintln!(
                        "Note: newDAWn doesn't look in {}; add it to {} for it to",
                        folder.display(),
                        outcome.format.search_variable()
                    );
                }
                for copy in found_first(path, &order) {
                    eprintln!(
                        "Warning: {} is found before the one installed, so newDAWn loads it \
                         instead; remove it to use this build",
                        copy.display()
                    );
                }
            }
            // The error says whether the plugin went in: "not installed: …", or installed but
            // not its control map.
            Err(error) => eprintln!("{label}: {error:#}"),
        }
        for left in &outcome.left_behind {
            eprintln!(
                "Warning: {label}: the older copy {} couldn't be removed; hosts reading that \
                 folder list the plugin twice until it is",
                left.display()
            );
        }
    }
}

/// Every other bundle of `installed`'s name that newDAWn finds before it: in the folders it scans
/// before the one holding `installed` (and their subfolders), and in that one, where it takes the
/// bundles in sorted order. Nothing when newDAWn doesn't scan `installed`'s folder at all.
fn found_first(installed: &Path, order: &[PathBuf]) -> Vec<PathBuf> {
    let (Some(folder), Some(name)) = (installed.parent(), installed.file_name()) else {
        return Vec::new();
    };
    let Some(at) = scanning_root(folder, order) else {
        return Vec::new();
    };
    let installed = resolved(installed);
    let mut found = Vec::new();
    for first in &order[..at] {
        find(first, name, 0, &mut found);
    }
    let mut beside = Vec::new();
    find(&order[at], name, 0, &mut beside);
    found.extend(beside.into_iter().filter(|copy| resolved(copy) < installed));
    // Never the installed bundle itself, which an earlier root can reach through a junction.
    found.retain(|copy| resolved(copy) != installed);
    found
}

/// Which of newDAWn's scan roots holds `folder`: the first that is it or one of its parents.
fn scanning_root(folder: &Path, order: &[PathBuf]) -> Option<usize> {
    let folder = resolved(folder);
    order
        .iter()
        .position(|root| folder.starts_with(resolved(root)))
}

/// Whether `a` and `b` may be one folder: told apart only when both resolve on disk (`..`, case and
/// junctions resolved) to different places, so an install is never taken back out through another
/// spelling of its own folder.
fn may_be_same_folder(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => true,
    }
}

/// Whether `a` and `b` are certainly one folder: both resolve on disk to the same place.
fn is_same_folder(a: &Path, b: &Path) -> bool {
    matches!(
        (std::fs::canonicalize(a), std::fs::canonicalize(b)),
        (Ok(a), Ok(b)) if a == b
    )
}

/// `path` as the file system resolves it, or as written when it doesn't exist.
fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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
        if entry.file_name() == name {
            found.push(path);
        } else if path.is_dir() && path.extension().is_none_or(|extension| extension != "vst3") {
            find(&path, name, depth + 1, found);
        }
    }
}

/// Takes `bundle`'s own files out of `folder`, by exact name: its `.clap`, its control map and
/// what an install there left beside them, under the folder's lock, so an install there (an older
/// kit's, or one `MXM_CLAP_DIR` sends there) is never cut into. One a host holds is set aside as
/// `.old`, which no host lists. Returns what is still there under a name a host reads.
fn retire(bundle: &str, folder: &Path) -> Vec<PathBuf> {
    let files = [
        folder.join(format!("{bundle}.clap")),
        folder.join(format!("{bundle}.control-map.json")),
    ];
    if !folder.is_dir() {
        return Vec::new();
    }
    let Ok(_lock) = lock(folder) else {
        return files.into_iter().filter(|file| file.exists()).collect();
    };
    let mut left = Vec::new();
    for path in files {
        clear_leftovers(&path);
        if remove(&path).is_err() && set_aside(&path).is_err() {
            left.push(path);
        }
    }
    left
}

/// The folder's install lock, held until the file returned is dropped, so installs into it take
/// turns.
fn lock(folder: &Path) -> Result<std::fs::File> {
    let lock_path = folder.join(LOCK);
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("{} can't be opened", lock_path.display()))?;
    lock.lock()
        .with_context(|| format!("{} can't be locked", lock_path.display()))?;
    Ok(lock)
}

fn install_one(
    bundle: &str,
    format: Format,
    bundled: &Path,
    map: Option<&PathBuf>,
    folder: &Path,
) -> Result<PathBuf> {
    let name = format!("{bundle}.{}", format.extension());
    let installed = folder.join(&name);
    // A destination that is where the bundle was made or its map staged (`MXM_CLAP_DIR` naming a
    // `target/bundled`) is refused, touching nothing: putting the plugin in would set aside the map
    // it copies from, and what is there isn't kept to an install's rules.
    let staged_here = map
        .and_then(|map| map.parent())
        .is_some_and(|staged| is_same_folder(folder, staged));
    if is_same_folder(folder, bundled) || staged_here {
        bail!(
            "not installed: {} is where the bundle is made or its map staged; set {} to another \
             folder",
            folder.display(),
            format.variable()
        );
    }
    // Only the CLAP carries the control map; the VST3 has no installed map to set aside.
    let installed_map =
        (format == Format::Clap).then(|| folder.join(format!("{bundle}.control-map.json")));
    // Held until this install ends, its map placed too.
    let _lock = put_in(
        &bundled.join(&name),
        &installed,
        installed_map.as_deref(),
        folder,
    )
    .context("not installed")?;
    if let (Some(map), Some(installed_map)) = (map, &installed_map) {
        place(map, installed_map).with_context(|| {
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
    installed_map: Option<&Path>,
    folder: &Path,
) -> Result<std::fs::File> {
    if !source.exists() {
        bail!("the bundle wasn't made at {}", source.display());
    }
    std::fs::create_dir_all(folder)
        .with_context(|| format!("{} can't be made", folder.display()))?;
    let lock = lock(folder)?;

    clear_leftovers(installed);
    let map_aside = match installed_map {
        Some(installed_map) => {
            clear_leftovers(installed_map);
            set_aside(installed_map)?
        }
        None => None,
    };
    if let Err(error) = place(source, installed) {
        if let (Some(map_aside), Some(installed_map)) = (&map_aside, installed_map) {
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

/// Puts `source`, a file or a bundle folder, at `destination` whole: copied beside it first, then
/// renamed in, so a host never finds half of one and an old bundle is never merged into.
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
