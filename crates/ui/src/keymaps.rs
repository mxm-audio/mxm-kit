//! The keymap files the DAW and every MXM editor share: one folder, `<config>/mxm/keymaps/`, beside
//! the editors' `editor.json` (the owner, 2026-10-12: "The idea is to have a unified key language
//! in the daw and in the plugins").
//!
//! The folder holds a plain-text keymap per file (`<name>.keymap`, `mxm_keys::Keymap::parse`'s
//! format), `active.txt` naming the one in use, and `.shipped/`, what was copied of each keymap the
//! kit ships. A host that offers the choice [`seed`]s the folder and [`set_wanted`] writes the
//! choice; every host, and every editor window, reads the keymap in use with [`in_use`] once: a
//! host when it starts and when a keymap is chosen, an editor when its window opens. The keymap
//! isn't something that changes while one works (the owner, 2026-10-12: "The keymap is not
//! supposed to change"), so nothing watches the files.
//!
//! Every write here is whole, a temporary file renamed over the old one, so a reader never sees
//! half of one of ours. Reading never writes.

use std::path::{Path, PathBuf};

use mxm_keys::Keymap;
use mxm_keys::keymap::{DEFAULT, SHIPPED};

/// The file that names the keymap in use.
pub const ACTIVE_FILE: &str = "active.txt";
/// The folder that keeps what was copied of each shipped keymap.
const SHIPPED_RECORDS: &str = ".shipped";

/// The shared keymap folder, `<config>/mxm/keymaps` (`%APPDATA%\mxm\keymaps` on Windows); `None`
/// when the platform reports no config directory.
#[must_use]
pub fn folder() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("mxm").join("keymaps"))
}

/// The file a keymap is read from.
#[must_use]
pub fn file(folder: &Path, name: &str) -> PathBuf {
    folder.join(format!("{name}.keymap"))
}

/// Makes the folder and copies each shipped keymap in, to be edited. What was copied is kept in
/// `.shipped/`: a copy nobody has edited follows the shipped keymap when that changes, and goes
/// when the kit stops shipping it; an edited one is left alone, and is the user's own once its
/// keymap no longer ships. Returns what went wrong, never raises it.
pub fn seed(folder: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    if let Err(error) = std::fs::create_dir_all(folder) {
        problems.push(format!("The keymap folder can't be made: {error}"));
        return problems;
    }
    let records = folder.join(SHIPPED_RECORDS);
    let records_made = std::fs::create_dir_all(&records)
        .map_err(|error| problems.push(format!("{} can't be made: {error}", records.display())))
        .is_ok();
    // As bytes, so an edit that isn't UTF-8 still counts as an edit; a file that is there and
    // doesn't read is said and left alone.
    let read = |path: &Path| match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    };
    for (stem, text) in SHIPPED {
        let record = file(&records, stem);
        let file = file(folder, stem);
        let (current, copied) = match (read(&file), read(&record)) {
            (Ok(current), Ok(copied)) => (current, copied),
            (Err(problem), _) | (_, Err(problem)) => {
                problems.push(problem);
                continue;
            }
        };
        let shipped = Some(text.as_bytes());
        let unedited = match (&current, &copied) {
            (None, _) => true,
            // The shipped text already, its record missing or older (an update stopped between
            // the two writes): the record catches up, so the copy keeps following.
            (Some(current), _) if current == text.as_bytes() => true,
            (Some(current), Some(copied)) => current == copied,
            (Some(_), None) => false,
        };
        if !unedited {
            continue;
        }
        let written = if current.as_deref() == shipped {
            Ok(())
        } else {
            write_whole(&file, text)
        };
        let written = written.and_then(|()| {
            if copied.as_deref() == shipped {
                Ok(())
            } else {
                write_whole(&record, text)
            }
        });
        if let Err(error) = written {
            problems.push(format!("{} can't be written: {error}", file.display()));
        }
    }
    // The records of keymaps the kit no longer ships; a records folder that couldn't be made was
    // said already.
    let mut gone = Vec::new();
    let entries = if records_made {
        std::fs::read_dir(&records)
            .map_err(|error| problems.push(format!("{}: {error}", records.display())))
            .into_iter()
            .flatten()
            .collect()
    } else {
        Vec::new()
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(error) => {
                problems.push(format!("{}: {error}", records.display()));
                continue;
            }
        };
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        if let Some(stem) = stem
            && path
                .extension()
                .is_some_and(|extension| extension == "keymap")
            && !SHIPPED.iter().any(|(shipped, _)| *shipped == stem)
        {
            gone.push(stem.to_owned());
        }
    }
    for stem in gone {
        let record = file(&records, &stem);
        let copy = file(folder, &stem);
        let (current, copied) = match (read(&copy), read(&record)) {
            (Ok(current), Ok(copied)) => (current, copied),
            (Err(problem), _) | (_, Err(problem)) => {
                problems.push(problem);
                continue;
            }
        };
        if current.is_some()
            && current == copied
            && let Err(error) = std::fs::remove_file(&copy)
        {
            problems.push(format!("{} can't be removed: {error}", copy.display()));
            continue;
        }
        if let Err(error) = std::fs::remove_file(&record) {
            problems.push(format!("{} can't be removed: {error}", record.display()));
        }
    }
    problems
}

/// Names the keymap in use, for every host and editor that reads the folder.
pub fn set_wanted(folder: &Path, name: &str) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    let active = folder.join(ACTIVE_FILE);
    write_whole(&active, name).map_err(|error| format!("{}: {error}", active.display()))
}

/// Writes a file whole: a temporary file beside it, renamed over it (`rename` replaces the old
/// file on every platform, Windows included).
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)
}

/// The keymap in use, as the folder gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct InUse {
    /// What `active.txt` names, or the default.
    pub name: String,
    pub keymap: Keymap,
    /// Why a file didn't read, if one didn't: the keymap is then the shipped one of that name, or
    /// the default.
    pub problem: Option<String>,
}

/// Reads the keymap in use: the one `active.txt` names (the default when it's missing or empty),
/// from its file, or the shipped text when a shipped keymap's file isn't there (a folder nobody
/// seeded). A file that doesn't read gives the shipped keymap of that name, or the default, and
/// says why.
#[must_use]
pub fn in_use(folder: &Path) -> InUse {
    let active = folder.join(ACTIVE_FILE);
    let name = match std::fs::read_to_string(&active) {
        Ok(text) if !text.trim().is_empty() => text.trim().to_owned(),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return InUse {
                name: DEFAULT.to_owned(),
                keymap: Keymap::default(),
                problem: Some(format!("{}: {error}", active.display())),
            };
        }
        _ => DEFAULT.to_owned(),
    };
    match read(folder, &name) {
        Ok(keymap) => InUse {
            name,
            keymap,
            problem: None,
        },
        Err(problem) => InUse {
            keymap: Keymap::shipped(&name).unwrap_or_default(),
            name,
            problem: Some(problem),
        },
    }
}

/// Reads the keymap of this name from its file, or the shipped text when a shipped keymap's file
/// isn't there; says why when it doesn't read. A host checks a keymap with it before naming it
/// in `active.txt` (`set_wanted`).
pub fn read(folder: &Path, name: &str) -> Result<Keymap, String> {
    let path = file(folder, name);
    match std::fs::read_to_string(&path) {
        Ok(text) => Keymap::parse(&text).map_err(|error| format!("{}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Keymap::shipped(name)
            .ok_or_else(|| format!("No keymap named {name:?} in {}", folder.display())),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use mxm_keys::{Job, Key};

    /// A folder of this test's own beside the test binary, in the build folder: never the real
    /// config directory, which would change the keys of every editor on the machine.
    fn scratch(name: &str) -> PathBuf {
        let exe = std::env::current_exe().expect("the test binary has a path");
        let dir = exe
            .parent()
            .expect("in a folder")
            .join(format!("mxm-ui-keymaps-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The default keymap, edited: its keys change.
    fn edited() -> String {
        let edited = SHIPPED[0].1.replace("one-arrow = no", "one-arrow = yes");
        assert_ne!(edited, SHIPPED[0].1, "the default has the line to edit");
        edited
    }

    #[test]
    fn seeding_copies_the_shipped_keymaps_and_keeps_an_edited_one() {
        let folder = scratch("seed");
        assert!(seed(&folder).is_empty());
        for (stem, text) in SHIPPED {
            assert_eq!(std::fs::read_to_string(file(&folder, stem)).unwrap(), text);
        }

        // An unedited copy of an older shipped text follows the new one.
        let study = file(&folder, DEFAULT);
        let record = file(&folder.join(SHIPPED_RECORDS), DEFAULT);
        std::fs::write(&study, "# older\n").unwrap();
        std::fs::write(&record, "# older\n").unwrap();
        assert!(seed(&folder).is_empty());
        assert_eq!(std::fs::read_to_string(&study).unwrap(), SHIPPED[0].1);

        // An update that stopped between the keymap and its record: the record catches up.
        std::fs::remove_file(&record).unwrap();
        assert!(seed(&folder).is_empty());
        assert_eq!(std::fs::read_to_string(&record).unwrap(), SHIPPED[0].1);

        // An edited one is left alone, even one that isn't UTF-8.
        std::fs::write(&study, edited()).unwrap();
        assert!(seed(&folder).is_empty());
        assert_eq!(std::fs::read_to_string(&study).unwrap(), edited());
        let mangled = [edited().as_bytes(), &[0xff, 0xfe]].concat();
        std::fs::write(&study, &mangled).unwrap();
        assert!(seed(&folder).is_empty());
        assert_eq!(std::fs::read(&study).unwrap(), mangled);
    }

    #[test]
    fn a_keymap_no_longer_shipped_goes_unless_it_was_edited() {
        let folder = scratch("gone");
        assert!(seed(&folder).is_empty());
        let records = folder.join(SHIPPED_RECORDS);
        // Keymaps an older kit shipped and copied: one as copied, one edited since, and one whose
        // copy was deleted by hand.
        for stem in ["unedited", "edited", "deleted"] {
            std::fs::write(file(&folder, stem), "name = Old\n").unwrap();
            std::fs::write(file(&records, stem), "name = Old\n").unwrap();
        }
        std::fs::write(file(&folder, "edited"), "name = Mine\n").unwrap();
        std::fs::remove_file(file(&folder, "deleted")).unwrap();

        assert!(seed(&folder).is_empty());
        assert!(!file(&folder, "unedited").exists());
        assert_eq!(
            std::fs::read_to_string(file(&folder, "edited")).unwrap(),
            "name = Mine\n"
        );
        for stem in ["unedited", "edited", "deleted"] {
            assert!(!file(&records, stem).exists(), "{stem}'s record goes");
        }
        assert!(file(&folder, DEFAULT).exists() && file(&records, DEFAULT).exists());
    }

    #[test]
    fn the_keymap_in_use_is_rewritten_whole() {
        let folder = scratch("wanted");
        assert_eq!(
            in_use(&folder).name,
            DEFAULT,
            "nothing chosen is the default"
        );
        set_wanted(&folder, "mine").unwrap();
        assert_eq!(in_use(&folder).name, "mine");
        set_wanted(&folder, "yours").unwrap();
        assert_eq!(in_use(&folder).name, "yours");
        assert!(!folder.join("active.txt.tmp").exists());
    }

    #[test]
    fn a_missing_folder_gives_the_default() {
        assert_eq!(
            in_use(&scratch("missing")),
            InUse {
                name: DEFAULT.to_owned(),
                keymap: Keymap::default(),
                problem: None,
            }
        );
    }

    #[test]
    fn the_keymap_named_is_read_from_its_file() {
        let folder = scratch("read");
        assert!(seed(&folder).is_empty());
        set_wanted(&folder, DEFAULT).unwrap();
        assert_eq!(in_use(&folder).keymap, Keymap::default());

        // A keymap of the user's own, the default with VIEW's key moved: that's what is read.
        let view = Keymap::default()
            .keys(Job::View)
            .next()
            .expect("the default binds VIEW");
        let free = ('A'..='Z')
            .filter_map(|letter| Key::from_name(&letter.to_string()))
            .find(|key| Keymap::default().job(*key).is_none())
            .expect("the default leaves a letter free");
        let text = SHIPPED[0].1.replace(
            &format!("view = {}", view.name()),
            &format!("view = {}", free.name()),
        );
        std::fs::write(file(&folder, "mine"), text).unwrap();
        set_wanted(&folder, "mine").unwrap();
        let read = in_use(&folder);
        assert_eq!(
            (read.keymap.job(free), read.problem),
            (Some(Job::View), None)
        );
    }

    #[test]
    fn a_file_that_doesnt_read_is_the_shipped_keymap_of_its_name() {
        let folder = scratch("broken");
        assert!(seed(&folder).is_empty());
        set_wanted(&folder, DEFAULT).unwrap();
        std::fs::write(file(&folder, DEFAULT), "nonsense\n").unwrap();
        let read = in_use(&folder);
        assert_eq!(Some(read.keymap), Keymap::shipped(DEFAULT));
        assert!(read.problem.is_some());
    }

    #[test]
    fn an_active_file_that_doesnt_read_is_the_default() {
        let folder = scratch("active");
        assert!(seed(&folder).is_empty());
        std::fs::write(folder.join(ACTIVE_FILE), [0xff, 0xfe, 0x00]).unwrap();
        let read = in_use(&folder);
        assert_eq!(
            (read.name.as_str(), &read.keymap),
            (DEFAULT, &Keymap::default())
        );
        assert!(read.problem.is_some());
    }
}
