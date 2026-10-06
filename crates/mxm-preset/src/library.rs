//! Everything that can be loaded, and everywhere a preset can be written: the user's own presets,
//! the banks beside them, and the banks folder they are sent and received through.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{EXTENSION, Instrument, Origin, Preset, SCHEMA_VERSION};

/// Every factory preset, Init first.
///
/// A malformed factory preset is a **build** mistake rather than a user's, so it is skipped here and
/// caught by the plugin's own `every_factory_preset_parses_and_is_for_this_instrument` rather than
/// surfaced in the interface.
pub fn factory(instrument: &dyn Instrument) -> Vec<Preset> {
    let mut all = vec![Preset::init(instrument)];
    all.extend(
        instrument
            .factory_files()
            .iter()
            .filter_map(|(_, text)| Preset::parse(text, instrument.clap_id()).ok()),
    );
    all
}

/// Where a user's presets live, under the platform config directory, namespaced by the plugin's id
/// so another instrument's presets cannot appear in this one's list.
///
/// Not beside the `.clap`: a plugin instance is never handed the path its bundle was loaded from,
/// and a directory next to a bundle is not part of it on every platform — copy the `.clap` alone and
/// the sounds are gone. Compiled-in factory presets are as portable as the code; a user's own live
/// where the rest of this collection's settings do.
pub fn user_root(clap_id: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("mxm").join(clap_id).join("presets"))
}

/// Where bank files are sent from and received into: `banks`, beside `presets`.
///
/// **A folder, not a dialog.** A native file dialog cannot be driven from the player's CLI, which the
/// collection's developer-channel rule requires of every editor state; a folder can be filled from
/// anywhere, and *Open folder* puts it in front of a person. A dialog may come later as a
/// convenience; the folder is the mechanism.
pub fn banks_root(clap_id: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("mxm").join(clap_id).join("banks"))
}

/// A file or directory stem for `name`, sanitised. An empty name becomes `preset` rather than a
/// dotfile.
fn stem(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let safe = safe.trim_matches('-').to_lowercase();
    if safe.is_empty() {
        "preset".to_owned()
    } else {
        safe
    }
}

/// A file name for a preset called `name`.
pub fn file_name(name: &str) -> String {
    format!("{}.{EXTENSION}", stem(name))
}

/// The directory name a bank called `name` takes, inside the presets root.
pub fn dir_name(name: &str) -> String {
    stem(name)
}

/// The file inside a bank's directory that says what the bank is.
pub const BANK_FILE: &str = "bank.json";

/// The extension a bank takes on the wire — still `.json`, so it opens anywhere.
pub const BANK_EXTENSION: &str = "mxmbank.json";

/// What a bank says about itself, in [`BANK_FILE`] inside its directory.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BankInfo {
    pub schema_version: u32,
    pub plugin: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

/// A bank on the wire: one readable, diffable, hand-editable file with every preset inside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BankFile {
    pub schema_version: u32,
    /// The permanent `CLAP_ID`. A bank for one instrument imported into another is refused whole.
    pub plugin: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub presets: Vec<Preset>,
}

/// A bank as the browser lists it: the user's own presets first, then each directory.
#[derive(Clone, Debug, PartialEq)]
pub struct Bank {
    pub origin: Origin,
    pub name: String,
    pub author: String,
    pub comment: String,
    /// `Some` when the bank's own file could not be read; the directory's name stands in.
    pub problem: Option<String>,
}

/// Why a bank file could not be imported. Every refusal names what was wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportRefused {
    NotABank(String),
    WrongSchema {
        found: u32,
    },
    WrongPlugin {
        found: String,
        expected: String,
    },
    /// A bank of that name is already in the library. Asked, never silently replaced.
    Exists(String),
    Io(String),
}

impl std::fmt::Display for ImportRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportRefused::NotABank(why) => write!(f, "this is not a bank file: {why}"),
            ImportRefused::WrongSchema { found } => write!(
                f,
                "the bank is schema version {found}, but this build understands {SCHEMA_VERSION}"
            ),
            ImportRefused::WrongPlugin { found, expected } => {
                write!(f, "this bank is for `{found}`, and this is `{expected}`")
            }
            ImportRefused::Exists(name) => write!(f, "a bank called {name:?} is already here"),
            ImportRefused::Io(why) => write!(f, "{why}"),
        }
    }
}

/// One row in the browser: a preset, or a file that could not be read.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Factory, the user's own, or the bank it belongs to.
    pub origin: Origin,
    /// `None` for a file that is present and unreadable — shown, disabled, carrying its reason.
    /// Omitting it entirely would tell somebody their preset had vanished.
    pub preset: Option<Preset>,
    pub problem: Option<String>,
    pub path: Option<PathBuf>,
    /// The folder a [`Found`] preset came from, which the browser shows as its category; `None`
    /// for everything else, which is filed under its [`crate::Category`].
    pub group: Option<String>,
    /// Whether it is a [`Found`] preset. Such a preset may name a file where it was found rather
    /// than carry its content, so it is **never exported**: a bank has to load on a machine without
    /// that folder.
    pub found: bool,
}

/// **A factory preset found at run time rather than compiled in** — mxm-fx-convolution's impulse
/// files, one preset per file in the collection's impulses folder (the owner, 2026-09-28: *what is
/// in that folder is the default presets*). The plugin scans and hands the list over
/// ([`Library::with_found`]); the library never looks anywhere itself, as its root is injected.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub preset: Preset,
    /// The folder it was found in, shown as its category in the browser.
    pub group: Option<String>,
}

/// Everything that can be loaded, and everywhere a preset can be written.
///
/// **The root is injected, not looked up at the point of use.** `dirs::config_dir()` is called once,
/// by [`Library::at_config_dir`], and what comes back is a field. That is what lets a test drive the
/// `None` path deliberately and — far more importantly — what stops a test writing into the config
/// directory of whoever is running it.
pub struct Library {
    root: Option<PathBuf>,
    banks: Option<PathBuf>,
    found: Vec<Found>,
}

impl Library {
    /// The real one, for the plugin with `clap_id`.
    pub fn at_config_dir(clap_id: &str) -> Self {
        Self {
            root: user_root(clap_id),
            banks: banks_root(clap_id),
            found: Vec::new(),
        }
    }

    /// A library rooted anywhere — a sandbox for tests, or nowhere at all.
    ///
    /// The banks folder is `banks` beside a root called `presets`, as the real layout has it; a
    /// root called anything else has no banks folder, and a sandbox that wants one names its root
    /// `presets`.
    pub fn at(root: Option<PathBuf>) -> Self {
        let banks = root.as_ref().and_then(|root| {
            (root.file_name().and_then(|n| n.to_str()) == Some("presets"))
                .then(|| root.parent().map(|parent| parent.join("banks")))
                .flatten()
        });
        Self {
            root,
            banks,
            found: Vec::new(),
        }
    }

    /// This library with factory presets found at run time, listed after the compiled set as
    /// [`Origin::Factory`] under their folders ([`Found`]).
    #[must_use]
    pub fn with_found(mut self, found: Vec<Found>) -> Self {
        self.found = found;
        self
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The folder bank files are exported to and imported from, if there is one.
    pub fn banks_dir(&self) -> Option<&Path> {
        self.banks.as_deref()
    }

    /// Why saving is unavailable, if it is.
    ///
    /// Shown rather than left to a failed save: a Save button that does nothing is worse than one
    /// that says why it cannot.
    pub fn why_read_only(&self) -> Option<&'static str> {
        self.root
            .is_none()
            .then_some("this system has no config directory, so presets cannot be saved here")
    }

    /// The directory a preset of `origin` lives in. `None` for the factory, which has no files, and
    /// for a library with no root.
    pub fn dir_for(&self, origin: &Origin) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        match origin {
            Origin::Factory => None,
            Origin::User => Some(root.clone()),
            Origin::Bank(name) => Some(root.join(dir_name(name))),
        }
    }

    /// The banks in the library: the user's own presets first, then every bank directory, by name.
    ///
    /// A directory without a readable [`BANK_FILE`] is still a bank — its presets are somebody's —
    /// under the directory's own name, carrying the problem.
    pub fn banks(&self) -> Vec<Bank> {
        let mut banks = vec![Bank {
            origin: Origin::User,
            name: "My presets".to_owned(),
            author: String::new(),
            comment: String::new(),
            problem: None,
        }];
        let Some(root) = self.root.as_ref() else {
            return banks;
        };
        let Ok(dir) = std::fs::read_dir(root) else {
            return banks;
        };
        let mut found: Vec<Bank> = Vec::new();
        for entry in dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("bank")
                .to_owned();
            let info: Result<BankInfo, String> = std::fs::read_to_string(path.join(BANK_FILE))
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()));
            let bank = match info {
                // The folder's name wins over the file's when they disagree: the folder is where
                // the presets are, and `dir_for` has to find them from the name.
                Ok(info) if stem(&info.name) == dir_name => Bank {
                    origin: Origin::Bank(info.name.clone()),
                    name: info.name,
                    author: info.author,
                    comment: info.comment,
                    problem: None,
                },
                Ok(info) => Bank {
                    origin: Origin::Bank(dir_name.clone()),
                    name: dir_name,
                    author: info.author,
                    comment: info.comment,
                    problem: None,
                },
                Err(why) => Bank {
                    origin: Origin::Bank(dir_name.clone()),
                    name: dir_name,
                    author: String::new(),
                    comment: String::new(),
                    problem: Some(format!("{BANK_FILE} could not be read: {why}")),
                },
            };
            found.push(bank);
        }
        found.sort_by_key(|bank| bank.name.to_lowercase());
        banks.extend(found);
        banks
    }

    /// Every factory preset — the compiled set, then any [`Found`] — then the user's own, then each
    /// bank's, each group in name order.
    ///
    /// A missing directory is not an error — it is an empty list, which is what a fresh install
    /// looks like.
    pub fn list(&self, instrument: &dyn Instrument) -> Vec<Entry> {
        let mut entries: Vec<Entry> = factory(instrument)
            .into_iter()
            .map(|preset| Entry {
                name: preset.name.clone(),
                origin: Origin::Factory,
                preset: Some(preset),
                problem: None,
                path: None,
                group: None,
                found: false,
            })
            .collect();
        entries.extend(self.found.iter().map(|found| Entry {
            name: found.preset.name.clone(),
            origin: Origin::Factory,
            preset: Some(found.preset.clone()),
            problem: None,
            path: None,
            group: found.group.clone(),
            found: true,
        }));
        for bank in self.banks() {
            if let Some(dir) = self.dir_for(&bank.origin) {
                entries.extend(self.presets_in(&dir, &bank.origin, instrument));
            }
        }
        entries
    }

    /// The preset files in one directory, in name order.
    fn presets_in(&self, dir: &Path, origin: &Origin, instrument: &dyn Instrument) -> Vec<Entry> {
        let Ok(read) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut found: Vec<Entry> = Vec::new();
        for file in read.flatten() {
            let path = file.path();
            if path.is_dir() || path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
                continue;
            }
            // **The favourites index and a bank's own file live here too**, and are `.json` files
            // that are not presets. Without this they appear in the browser as broken rows
            // carrying a parse error, which is exactly the shape of a bug that looks like
            // somebody's preset being corrupt.
            let file_name = path.file_name().and_then(|n| n.to_str());
            if file_name == Some(FAVOURITES) || file_name == Some(BANK_FILE) {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned();

            let (preset, problem) = match std::fs::read_to_string(&path) {
                Ok(text) => match Preset::parse(&text, instrument.clap_id()) {
                    Ok(preset) => (Some(preset), None),
                    Err(why) => (None, Some(why.to_string())),
                },
                Err(why) => (None, Some(why.to_string())),
            };

            found.push(Entry {
                name: preset.as_ref().map_or(stem, |p| p.name.clone()),
                origin: origin.clone(),
                preset,
                problem,
                path: Some(path),
                group: None,
                found: false,
            });
        }
        found.sort_by_key(|entry| entry.name.to_lowercase());
        found
    }

    /// Whether a preset of this name already exists where `origin` keeps its files.
    ///
    /// Save As asks before overwriting one: a preset is something somebody made, and losing it to a
    /// name clash is not a rounding error.
    pub fn preset_exists(&self, name: &str, origin: &Origin) -> bool {
        self.path_for(name, origin)
            .is_some_and(|path| path.exists())
    }

    pub fn path_for(&self, name: &str, origin: &Origin) -> Option<PathBuf> {
        self.dir_for(origin).map(|dir| dir.join(file_name(name)))
    }

    /// Writes a preset where `origin` keeps its files, replacing any file of the same name. A bank
    /// directory that does not exist yet is made, with a [`BANK_FILE`] naming it.
    ///
    /// **Temporary file, then rename** — a crash mid-write cannot destroy the previous copy. The
    /// temporary name carries the **process id and a counter**, because two instances of a plugin
    /// in one DAW can save at the same moment; and a **failed write removes its temporary file**
    /// rather than leaving it in somebody's config directory for ever.
    pub fn save(&self, preset: &Preset, origin: &Origin) -> Result<PathBuf, String> {
        if matches!(origin, Origin::Factory) {
            return Err(
                "a factory preset cannot be changed; Save As writes a copy of your own".to_owned(),
            );
        }
        let Some(dir) = self.dir_for(origin) else {
            return Err(self.why_read_only().unwrap_or("cannot save").to_owned());
        };
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("{} could not be made: {e}", dir.display()))?;
        if let Origin::Bank(name) = origin
            && !dir.join(BANK_FILE).exists()
        {
            write_atomically(
                &dir.join(BANK_FILE),
                &bank_info_json(name, "", "", &preset.plugin),
            )?;
        }
        write_atomically(&dir.join(file_name(&preset.name)), &preset.to_json())
    }

    /// **Save the new file, confirm it, then remove the old.** The reverse loses the preset if the
    /// write fails.
    pub fn rename(&self, preset: &Preset, to: &str, origin: &Origin) -> Result<PathBuf, String> {
        let mut renamed = preset.clone();
        renamed.name = to.to_owned();
        let written = self.save(&renamed, origin)?;

        if let Some(old) = self.path_for(&preset.name, origin)
            && old != written
        {
            let _ = std::fs::remove_file(old);
        }
        Ok(written)
    }

    /// Removes a preset from where `origin` keeps its files. A factory preset has no file and is
    /// never reached here.
    pub fn delete(&self, name: &str, origin: &Origin) -> Result<(), String> {
        let Some(path) = self.path_for(name, origin) else {
            return Err(self.why_read_only().unwrap_or("cannot delete").to_owned());
        };
        std::fs::remove_file(&path)
            .map_err(|e| format!("{} could not be removed: {e}", path.display()))
    }

    /// Makes an empty bank of `name`, and hands back the origin to save into.
    pub fn new_bank(&self, name: &str, author: &str, clap_id: &str) -> Result<Origin, String> {
        let origin = Origin::Bank(name.to_owned());
        let Some(dir) = self.dir_for(&origin) else {
            return Err(self
                .why_read_only()
                .unwrap_or("cannot make a bank")
                .to_owned());
        };
        if dir.exists() {
            return Err(format!("a bank called {name:?} is already here"));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("{} could not be made: {e}", dir.display()))?;
        write_atomically(
            &dir.join(BANK_FILE),
            &bank_info_json(name, author, "", clap_id),
        )?;
        Ok(origin)
    }

    /// The bank files waiting in the banks folder, in name order — what *Import* offers.
    pub fn importable(&self) -> Vec<PathBuf> {
        let Some(banks) = self.banks.as_ref() else {
            return Vec::new();
        };
        let Ok(dir) = std::fs::read_dir(banks) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = dir
            .flatten()
            .map(|f| f.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(&format!(".{BANK_EXTENSION}")))
            })
            .collect();
        files.sort();
        files
    }

    /// Unpacks a bank file into the library as a directory of presets.
    ///
    /// Refused whole when it is not a bank, not this schema, or not this plugin's — as a preset is —
    /// and refused with [`ImportRefused::Exists`] when a bank of that name is already here, unless
    /// `replace` says so: a person is asked, never silently overwritten. A preset inside the file
    /// that is for another plugin is skipped, not imported wrong.
    pub fn import_bank(
        &self,
        path: &Path,
        clap_id: &str,
        replace: bool,
    ) -> Result<Bank, ImportRefused> {
        let text = std::fs::read_to_string(path).map_err(|e| ImportRefused::Io(e.to_string()))?;
        let file: BankFile =
            serde_json::from_str(&text).map_err(|e| ImportRefused::NotABank(e.to_string()))?;
        if file.schema_version != SCHEMA_VERSION {
            return Err(ImportRefused::WrongSchema {
                found: file.schema_version,
            });
        }
        if file.plugin != clap_id {
            return Err(ImportRefused::WrongPlugin {
                found: file.plugin,
                expected: clap_id.to_owned(),
            });
        }
        let origin = Origin::Bank(file.name.clone());
        let dir = self.dir_for(&origin).ok_or_else(|| {
            ImportRefused::Io(self.why_read_only().unwrap_or("cannot import").to_owned())
        })?;
        if dir.exists() {
            if !replace {
                return Err(ImportRefused::Exists(file.name));
            }
            std::fs::remove_dir_all(&dir).map_err(|e| ImportRefused::Io(e.to_string()))?;
        }
        std::fs::create_dir_all(&dir).map_err(|e| ImportRefused::Io(e.to_string()))?;
        write_atomically(
            &dir.join(BANK_FILE),
            &bank_info_json(&file.name, &file.author, &file.comment, clap_id),
        )
        .map_err(ImportRefused::Io)?;
        for preset in file.presets.iter().filter(|p| p.plugin == clap_id) {
            self.save(preset, &origin).map_err(ImportRefused::Io)?;
        }
        Ok(Bank {
            origin,
            name: file.name,
            author: file.author,
            comment: file.comment,
            problem: None,
        })
    }

    /// Packs `presets` into one bank file in the banks folder, and hands back where it went.
    ///
    /// The presets are whatever the caller chose — a bank's directory, or everything a filter
    /// matched — which is what makes *save a category* the same act as *export a bank*.
    pub fn export_bank(
        &self,
        name: &str,
        author: &str,
        comment: &str,
        clap_id: &str,
        presets: &[Preset],
    ) -> Result<PathBuf, String> {
        let Some(banks) = self.banks.as_ref() else {
            return Err("this library has no banks folder to export into".to_owned());
        };
        if presets.is_empty() {
            return Err("nothing to export: no preset matches".to_owned());
        }
        std::fs::create_dir_all(banks)
            .map_err(|e| format!("{} could not be made: {e}", banks.display()))?;
        let file = BankFile {
            schema_version: SCHEMA_VERSION,
            plugin: clap_id.to_owned(),
            name: name.to_owned(),
            author: author.to_owned(),
            comment: comment.to_owned(),
            presets: presets.to_vec(),
        };
        let text = serde_json::to_string_pretty(&file).unwrap_or_default();
        write_atomically(
            &banks.join(format!("{}.{BANK_EXTENSION}", stem(name))),
            &text,
        )
    }
}

fn bank_info_json(name: &str, author: &str, comment: &str, clap_id: &str) -> String {
    serde_json::to_string_pretty(&BankInfo {
        schema_version: SCHEMA_VERSION,
        plugin: clap_id.to_owned(),
        name: name.to_owned(),
        author: author.to_owned(),
        comment: comment.to_owned(),
    })
    .unwrap_or_default()
}

/// Temporary file, then rename; a failed write removes its temporary file.
fn write_atomically(path: &Path, text: &str) -> Result<PathBuf, String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no directory", path.display()))?;
    let temp = dir.join(format!(
        "{}.{}.{}.tmp",
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("preset"),
        std::process::id(),
        next_temp_serial()
    ));
    if let Err(why) = std::fs::write(&temp, text) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("{} could not be written: {why}", temp.display()));
    }
    if let Err(why) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("{} could not be replaced: {why}", path.display()));
    }
    Ok(path.to_path_buf())
}

/// Shows `path` in the platform's file manager. Every platform gets an arm, or none may.
pub fn open_folder(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// The file the starred names live in, beside the presets.
const FAVOURITES: &str = "favourites.json";

/// The key a favourite is stored under: the origin and the name, because a bank may carry a preset
/// of the same name as one of yours and starring one must not star the other.
pub fn favourite_key(name: &str, origin: &Origin) -> String {
    match origin {
        Origin::Factory => format!("factory::{name}"),
        Origin::User => format!("user::{name}"),
        Origin::Bank(bank) => format!("bank::{bank}::{name}"),
    }
}

/// Whether `name` of `origin` is starred. **A plain name still counts** for a factory or user
/// preset: that is what the index held before banks, and nobody's stars go out for a format change.
pub fn is_favourite(favourites: &BTreeSet<String>, name: &str, origin: &Origin) -> bool {
    favourites.contains(&favourite_key(name, origin))
        || (!matches!(origin, Origin::Bank(_)) && favourites.contains(name))
}

/// Which presets have been starred.
///
/// **Per-person state, beside the presets rather than inside them.** A favourite is a fact about
/// somebody's setup, not about the sound: a `favourite` field in a preset would arrive on another
/// machine carrying somebody else's stars, and a factory preset — compiled in and immutable —
/// could never be starred at all.
///
/// A missing or unreadable file is **no favourites**, never an error: nothing here is worth
/// interrupting somebody over.
pub fn read_favourites(library: &Library) -> BTreeSet<String> {
    let Some(path) = library.root().map(|root| root.join(FAVOURITES)) else {
        return BTreeSet::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the starred keys. Silently does nothing where there is nowhere to write.
pub fn write_favourites(library: &Library, favourites: &BTreeSet<String>) {
    let Some(root) = library.root() else {
        return;
    };
    if std::fs::create_dir_all(root).is_err() {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(favourites) {
        let _ = std::fs::write(root.join(FAVOURITES), text);
    }
}

/// Distinguishes two temporary files written by one process in the same instant.
pub(crate) fn next_temp_serial() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Category;
    use crate::testing::{CLAP_ID, FACTORY, TestInstrument};

    /// A library rooted in a fresh temporary directory, removed when the test ends.
    ///
    /// **Never the real config directory.** A test that wrote there would put files in the home
    /// directory of whoever ran it, which is why `Library` takes its root rather than looking one
    /// up. The root is called `presets`, so the sandbox has a `banks` folder beside it as the real
    /// layout does.
    pub(crate) struct Sandbox {
        home: PathBuf,
        pub(crate) root: PathBuf,
        pub(crate) library: Library,
    }

    impl Sandbox {
        pub(crate) fn new(name: &str) -> Self {
            let home = std::env::temp_dir().join(format!(
                "mxm-preset-{}-{}-{}",
                name,
                std::process::id(),
                next_temp_serial()
            ));
            let _ = std::fs::remove_dir_all(&home);
            let root = home.join("presets");
            Self {
                library: Library::at(Some(root.clone())),
                root,
                home,
            }
        }

        /// Every file in the root, so a test can assert on what is *not* there.
        fn files(&self) -> Vec<String> {
            Self::names_in(&self.root)
        }

        fn names_in(dir: &Path) -> Vec<String> {
            let Ok(read) = std::fs::read_dir(dir) else {
                return Vec::new();
            };
            let mut names: Vec<String> = read
                .flatten()
                .map(|f| f.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    fn capture(name: &str, instrument: &TestInstrument) -> Preset {
        Preset::capture(name, Category::Uncategorised, instrument)
    }

    #[test]
    fn the_factory_list_begins_with_init() {
        let instrument = TestInstrument::new();
        let all = factory(&instrument);
        assert_eq!(all[0].name, crate::INIT_NAME);
        assert_eq!(all.len(), FACTORY.len() + 1);
        assert_eq!(
            all[1].category,
            Category::Bass,
            "the file's category is read"
        );
    }

    #[test]
    fn favourites_live_beside_the_presets_and_not_inside_them() {
        // A favourite is a fact about somebody's setup, not about the sound. Inside the file it
        // would travel with a copied preset — and a factory preset, compiled in and immutable,
        // could never be starred at all.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("favourites");
        sandbox
            .library
            .save(&capture("Mine", &instrument), &Origin::User)
            .expect("save");

        let mut stars = BTreeSet::new();
        stars.insert(favourite_key("Sub bass", &Origin::Factory)); // no file to write to
        stars.insert(favourite_key("Mine", &Origin::User));
        write_favourites(&sandbox.library, &stars);

        assert_eq!(read_favourites(&sandbox.library), stars);

        // And the preset file itself is untouched by any of it.
        let saved = std::fs::read_to_string(sandbox.root.join("mine.json")).expect("read");
        assert!(
            !saved.contains("favourite"),
            "a star must not be written into the preset"
        );
    }

    #[test]
    fn a_favourite_is_keyed_by_its_bank_and_a_plain_name_still_counts() {
        // Two banks may each carry a "Sub bass"; starring one must not star the other. And the
        // index before banks held plain names: nobody's stars go out for a format change.
        let mut stars = BTreeSet::new();
        stars.insert(favourite_key("Sub bass", &Origin::Bank("Cold".to_owned())));
        stars.insert("Brass".to_owned());

        assert!(is_favourite(
            &stars,
            "Sub bass",
            &Origin::Bank("Cold".to_owned())
        ));
        assert!(!is_favourite(
            &stars,
            "Sub bass",
            &Origin::Bank("Warm".to_owned())
        ));
        assert!(!is_favourite(&stars, "Sub bass", &Origin::User));
        assert!(
            is_favourite(&stars, "Brass", &Origin::Factory),
            "a plain name from before"
        );
        assert!(is_favourite(&stars, "Brass", &Origin::User));
        assert!(
            !is_favourite(&stars, "Brass", &Origin::Bank("Cold".to_owned())),
            "but a plain name never reaches into a bank"
        );
    }

    #[test]
    fn no_favourites_file_is_no_favourites_rather_than_an_error() {
        let sandbox = Sandbox::new("no-favourites");
        assert!(read_favourites(&sandbox.library).is_empty());
        assert!(read_favourites(&Library::at(None)).is_empty());
    }

    #[test]
    fn an_unreadable_favourites_file_is_ignored_rather_than_interrupting() {
        // Nothing here is worth stopping somebody over.
        let sandbox = Sandbox::new("bad-favourites");
        std::fs::create_dir_all(&sandbox.root).expect("mkdir");
        std::fs::write(sandbox.root.join(FAVOURITES), "not json").expect("write");
        assert!(read_favourites(&sandbox.library).is_empty());
    }

    #[test]
    fn the_favourites_file_is_not_listed_as_a_preset() {
        // It lives in the same directory and ends in `.json`, which is exactly the shape of a file
        // that would otherwise appear in the browser as a broken preset.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("favourites-not-listed");
        let mut stars = BTreeSet::new();
        stars.insert("Sub bass".to_owned());
        write_favourites(&sandbox.library, &stars);

        let entries = sandbox.library.list(&instrument);
        assert!(
            !entries.iter().any(|e| e.origin == Origin::User),
            "the favourites index is not a preset: {:?}",
            entries.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_missing_directory_is_an_empty_list_and_not_an_error() {
        // What a fresh install looks like. The factory presets are still there, because they are
        // compiled in rather than read from anywhere.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("missing");
        let entries = sandbox.library.list(&instrument);
        assert_eq!(entries.len(), FACTORY.len() + 1);
        assert!(entries.iter().all(|e| e.origin == Origin::Factory));
        assert_eq!(
            sandbox.library.banks().len(),
            1,
            "only My presets, and it is always there"
        );
    }

    #[test]
    fn a_saved_preset_comes_back_in_the_list() {
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("round-trip");
        let preset = Preset::capture("My sound", Category::Keys, &instrument);
        sandbox.library.save(&preset, &Origin::User).expect("save");

        let entries = sandbox.library.list(&instrument);
        let mine = entries
            .iter()
            .find(|e| e.name == "My sound")
            .expect("it must be listed");
        assert_eq!(mine.origin, Origin::User);
        assert_eq!(mine.preset.as_ref().expect("readable"), &preset);
    }

    #[test]
    fn a_successful_save_leaves_no_temporary_file() {
        // Temp-then-rename is what stops a crash mid-write destroying the previous copy; a temp
        // file that outlives the write is litter in somebody's config directory.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("no-temp");
        sandbox
            .library
            .save(&capture("Kept", &instrument), &Origin::User)
            .expect("save");
        assert_eq!(sandbox.files(), vec!["kept.json".to_owned()]);
    }

    #[test]
    fn a_temporary_name_is_not_shared_between_instances() {
        // One fixed temp name is safe for one process and not for two instances of a plugin in a
        // DAW saving at the same moment. This is the property that makes the difference, without
        // needing two processes.
        assert_ne!(next_temp_serial(), next_temp_serial());
    }

    #[test]
    fn an_unreadable_file_is_listed_as_broken_rather_than_hidden() {
        // Omitting it would tell somebody their preset had vanished. It is shown, disabled, and
        // carries the reason.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("broken");
        std::fs::create_dir_all(&sandbox.root).expect("mkdir");
        std::fs::write(sandbox.root.join("broken.json"), "{ not json").expect("write");

        let entries = sandbox.library.list(&instrument);
        let broken = entries
            .iter()
            .find(|e| e.name == "broken")
            .expect("it must still be listed");
        assert!(broken.preset.is_none(), "and must not be selectable");
        assert!(broken.problem.is_some(), "carrying its reason");
    }

    #[test]
    fn a_preset_for_another_plugin_is_listed_with_its_reason() {
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("foreign");
        std::fs::create_dir_all(&sandbox.root).expect("mkdir");

        let mut foreign = capture("Foreign", &instrument);
        foreign.plugin = "dk.mxm.mxm-mono-01".to_owned();
        std::fs::write(sandbox.root.join("foreign.json"), foreign.to_json()).expect("write");

        let entries = sandbox.library.list(&instrument);
        let row = entries
            .iter()
            .find(|e| e.name == "foreign")
            .expect("listed");
        assert!(
            row.problem
                .as_ref()
                .is_some_and(|p| p.contains("mxm-mono-01")),
            "{:?}",
            row.problem
        );
    }

    #[test]
    fn renaming_writes_the_new_file_before_removing_the_old() {
        // In that order. The reverse loses the preset if the write fails.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("rename");
        let preset = capture("Before", &instrument);
        sandbox.library.save(&preset, &Origin::User).expect("save");

        sandbox
            .library
            .rename(&preset, "After", &Origin::User)
            .expect("rename");
        assert_eq!(sandbox.files(), vec!["after.json".to_owned()]);

        let entries = sandbox.library.list(&instrument);
        assert!(entries.iter().any(|e| e.name == "After"));
        assert!(!entries.iter().any(|e| e.name == "Before"));
    }

    #[test]
    fn a_user_preset_may_share_a_name_with_a_factory_one_and_both_stay_visible() {
        // They live in different places, and hiding either would be telling somebody a preset had
        // vanished. Shown quietly, marked by origin — routine, not a warning.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("shadow");
        sandbox
            .library
            .save(&capture("Sub bass", &instrument), &Origin::User)
            .expect("save");

        let entries = sandbox.library.list(&instrument);
        let both: Vec<Origin> = entries
            .iter()
            .filter(|e| e.name == "Sub bass")
            .map(|e| e.origin.clone())
            .collect();
        assert_eq!(both, vec![Origin::Factory, Origin::User]);
    }

    #[test]
    fn deleting_a_user_preset_removes_only_its_file() {
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("delete");
        sandbox
            .library
            .save(&capture("Gone", &instrument), &Origin::User)
            .expect("save");
        sandbox
            .library
            .save(&capture("Kept", &instrument), &Origin::User)
            .expect("save");

        sandbox
            .library
            .delete("Gone", &Origin::User)
            .expect("delete");
        assert_eq!(sandbox.files(), vec!["kept.json".to_owned()]);
    }

    #[test]
    fn a_library_with_no_root_says_why_rather_than_failing_a_save() {
        // `dirs::config_dir()` returning `None` is rare and real. A Save button that does nothing
        // is worse than one that says why it cannot.
        let instrument = TestInstrument::new();
        let library = Library::at(None);

        assert!(library.why_read_only().is_some());
        assert_eq!(library.list(&instrument).len(), FACTORY.len() + 1);
        assert!(
            library
                .save(&capture("Nowhere", &instrument), &Origin::User)
                .is_err()
        );
        assert!(library.banks_dir().is_none());
    }

    #[test]
    fn a_factory_preset_cannot_be_saved_over() {
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("factory-write");
        assert!(
            sandbox
                .library
                .save(&capture("Sub bass", &instrument), &Origin::Factory)
                .is_err()
        );
        assert!(sandbox.files().is_empty());
    }

    #[test]
    fn a_name_becomes_a_safe_file_and_never_a_dotfile() {
        assert_eq!(file_name("Chorus pad"), "chorus-pad.json");
        assert_eq!(file_name("../../etc/passwd"), "etc-passwd.json");
        assert_eq!(file_name(""), "preset.json");
        assert_eq!(file_name("..."), "preset.json");
        assert_eq!(dir_name("Cold Pads"), "cold-pads");
    }

    #[test]
    fn the_user_root_is_under_the_plugins_own_id_and_the_banks_folder_beside_it() {
        // Namespaced by CLAP id so another instrument's presets cannot appear in this one's list.
        let Some(root) = user_root("dk.mxm.test") else {
            return;
        };
        assert!(root.ends_with("presets"));
        assert!(root.to_string_lossy().contains("dk.mxm.test"));
        let banks = banks_root("dk.mxm.test").expect("a config dir exists, so this does");
        assert_eq!(banks.parent(), root.parent());
        assert!(banks.ends_with("banks"));
    }

    // ---- banks ----

    #[test]
    fn a_bank_is_a_directory_beside_your_presets_and_its_presets_are_listed_under_it() {
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("bank-dir");
        let cold = sandbox
            .library
            .new_bank("Cold Pads", "Someone", CLAP_ID)
            .expect("a new bank");
        assert_eq!(cold, Origin::Bank("Cold Pads".to_owned()));
        sandbox
            .library
            .save(&Preset::capture("Glass", Category::Pad, &instrument), &cold)
            .expect("save into the bank");
        sandbox
            .library
            .save(&capture("Mine", &instrument), &Origin::User)
            .expect("save my own");

        let banks = sandbox.library.banks();
        assert_eq!(
            banks.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["My presets", "Cold Pads"]
        );
        assert_eq!(banks[1].author, "Someone");

        let entries = sandbox.library.list(&instrument);
        let glass = entries.iter().find(|e| e.name == "Glass").expect("listed");
        assert_eq!(glass.origin, cold);
        assert!(
            !entries
                .iter()
                .any(|e| e.name == BANK_FILE.trim_end_matches(".json")),
            "the bank's own file is not a preset"
        );
        assert_eq!(
            Sandbox::names_in(&sandbox.root.join("cold-pads")),
            ["bank.json", "glass.json"]
        );
    }

    #[test]
    fn a_bank_without_a_readable_bank_file_is_still_a_bank_under_its_directorys_name() {
        // Its presets are somebody's. Hidden, they would have vanished; shown, with the problem.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("bank-broken");
        let dir = sandbox.root.join("odd");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join("glass.json"),
            capture("Glass", &instrument).to_json(),
        )
        .expect("write");

        let banks = sandbox.library.banks();
        assert_eq!(banks[1].name, "odd");
        assert!(banks[1].problem.is_some());
        let entries = sandbox.library.list(&instrument);
        assert!(
            entries
                .iter()
                .any(|e| e.name == "Glass" && e.origin == Origin::Bank("odd".to_owned()))
        );
    }

    #[test]
    fn a_bank_round_trips_through_the_banks_folder() {
        // Export packs a set of presets into one file in the banks folder; import unpacks the file
        // into a directory. Both ends are the same presets.
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("bank-wire");
        let presets = vec![
            Preset::capture("Glass", Category::Pad, &instrument),
            Preset::capture("Fog", Category::Pad, &instrument),
        ];
        let file = sandbox
            .library
            .export_bank("Cold Pads", "Someone", "two pads", CLAP_ID, &presets)
            .expect("export");
        assert!(file.ends_with("cold-pads.mxmbank.json"));
        assert_eq!(sandbox.library.importable(), vec![file.clone()]);

        let bank = sandbox
            .library
            .import_bank(&file, CLAP_ID, false)
            .expect("import");
        assert_eq!(bank.name, "Cold Pads");
        assert_eq!(bank.author, "Someone");
        assert_eq!(bank.comment, "two pads");

        let entries = sandbox.library.list(&instrument);
        let imported: Vec<&Preset> = entries
            .iter()
            .filter(|e| e.origin == bank.origin)
            .filter_map(|e| e.preset.as_ref())
            .collect();
        assert_eq!(imported.len(), 2);
        assert!(imported.contains(&&presets[0]) && imported.contains(&&presets[1]));
    }

    #[test]
    fn importing_over_an_existing_bank_asks_first() {
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("bank-exists");
        let file = sandbox
            .library
            .export_bank(
                "Cold Pads",
                "",
                "",
                CLAP_ID,
                &[capture("Glass", &instrument)],
            )
            .expect("export");
        sandbox
            .library
            .import_bank(&file, CLAP_ID, false)
            .expect("the first import");

        assert_eq!(
            sandbox.library.import_bank(&file, CLAP_ID, false),
            Err(ImportRefused::Exists("Cold Pads".to_owned()))
        );
        // Told to replace, the directory is remade from the file — a preset saved into the bank
        // since is gone, as replacing means.
        sandbox
            .library
            .save(
                &capture("Extra", &instrument),
                &Origin::Bank("Cold Pads".to_owned()),
            )
            .expect("save");
        sandbox
            .library
            .import_bank(&file, CLAP_ID, true)
            .expect("replaced");
        assert_eq!(
            Sandbox::names_in(&sandbox.root.join("cold-pads")),
            ["bank.json", "glass.json"]
        );
    }

    #[test]
    fn a_bank_for_another_plugin_is_refused_whole_and_a_foreign_preset_inside_is_skipped() {
        let instrument = TestInstrument::new();
        let sandbox = Sandbox::new("bank-foreign");
        let banks = sandbox
            .library
            .banks_dir()
            .expect("a banks folder")
            .to_path_buf();
        std::fs::create_dir_all(&banks).expect("mkdir");

        let mut foreign = BankFile {
            schema_version: SCHEMA_VERSION,
            plugin: "dk.mxm.mxm-mono-01".to_owned(),
            name: "Theirs".to_owned(),
            author: String::new(),
            comment: String::new(),
            presets: vec![capture("Glass", &instrument)],
        };
        let path = banks.join("theirs.mxmbank.json");
        std::fs::write(&path, serde_json::to_string(&foreign).expect("json")).expect("write");
        assert_eq!(
            sandbox.library.import_bank(&path, CLAP_ID, false),
            Err(ImportRefused::WrongPlugin {
                found: "dk.mxm.mxm-mono-01".to_owned(),
                expected: CLAP_ID.to_owned(),
            })
        );

        // Ours, but carrying one preset that is not: that one is skipped, the rest imported.
        foreign.plugin = CLAP_ID.to_owned();
        foreign.presets.push({
            let mut p = capture("Stray", &instrument);
            p.plugin = "dk.mxm.mxm-mono-01".to_owned();
            p
        });
        std::fs::write(&path, serde_json::to_string(&foreign).expect("json")).expect("write");
        let bank = sandbox
            .library
            .import_bank(&path, CLAP_ID, false)
            .expect("import");
        let names: Vec<String> = sandbox
            .library
            .list(&instrument)
            .into_iter()
            .filter(|e| e.origin == bank.origin)
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["Glass"]);
    }

    #[test]
    fn something_that_is_not_a_bank_is_refused_with_its_reason() {
        let sandbox = Sandbox::new("bank-not");
        let banks = sandbox
            .library
            .banks_dir()
            .expect("a banks folder")
            .to_path_buf();
        std::fs::create_dir_all(&banks).expect("mkdir");
        let path = banks.join("junk.mxmbank.json");
        std::fs::write(&path, "{ not json").expect("write");
        assert!(matches!(
            sandbox.library.import_bank(&path, CLAP_ID, false),
            Err(ImportRefused::NotABank(_))
        ));
        let missing = banks.join("missing.mxmbank.json");
        assert!(matches!(
            sandbox.library.import_bank(&missing, CLAP_ID, false),
            Err(ImportRefused::Io(_))
        ));
    }

    #[test]
    fn the_banks_folder_lists_only_bank_files() {
        let sandbox = Sandbox::new("bank-listing");
        let banks = sandbox
            .library
            .banks_dir()
            .expect("a banks folder")
            .to_path_buf();
        std::fs::create_dir_all(&banks).expect("mkdir");
        std::fs::write(banks.join("b.mxmbank.json"), "{}").expect("write");
        std::fs::write(banks.join("a.mxmbank.json"), "{}").expect("write");
        std::fs::write(banks.join("notes.txt"), "").expect("write");
        std::fs::write(banks.join("preset.json"), "{}").expect("write");
        let names: Vec<String> = sandbox
            .library
            .importable()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.mxmbank.json", "b.mxmbank.json"]);
    }

    #[test]
    fn exporting_nothing_is_refused_rather_than_writing_an_empty_bank() {
        let sandbox = Sandbox::new("bank-empty");
        assert!(
            sandbox
                .library
                .export_bank("Nothing", "", "", CLAP_ID, &[])
                .is_err()
        );
        assert!(sandbox.library.importable().is_empty());
    }
}
