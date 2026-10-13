use super::*;

/// A folder of the test's own under the system's temporary folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mxm-xtask-{test}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| word.to_string()).collect()
}

/// `bundler.toml`'s one plugin, as the tests' bundles are named.
fn bundles() -> Vec<(String, String)> {
    vec![("x-plugin".to_owned(), "x".to_owned())]
}

fn installs(bundled: &Path, staged: &[PathBuf], folder: &Path) -> Vec<(String, Result<PathBuf>)> {
    install(&bundles(), &strings(&["x-plugin"]), bundled, staged, folder)
}

fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_bundle_command_line_names_its_packages_as_nice_plug_reads_them() {
    let read = |words: &[&str]| bundled_packages(&strings(words));
    assert_eq!(
        read(&["bundle", "mxm-mono-01", "--release"]),
        Some(strings(&["mxm-mono-01"]))
    );
    assert_eq!(
        read(&["bundle", "-p", "a", "-p", "b", "--release"]),
        Some(strings(&["a", "b"]))
    );
    assert_eq!(read(&["bundle-universal", "a"]), Some(strings(&["a"])));
    assert_eq!(read(&["fetch"]), None);
    assert_eq!(read(&[]), None);
}

#[test]
fn installing_is_on_unless_turned_off_or_under_ci() {
    assert!(wanted(None, false));
    assert!(!wanted(None, true));
    assert!(!wanted(Some("0"), false));
    assert!(wanted(Some("1"), true));
    // Set, only `1` installs: no other value turns it on under CI.
    assert!(!wanted(Some("false"), true));
    assert!(!wanted(Some("false"), false));
}

#[test]
fn a_plugin_and_its_map_are_installed_and_replaced_whole() {
    let scratch = Scratch::new("replaced");
    let bundled = scratch.0.join("bundled");
    let folder = scratch.0.join("CLAP");
    std::fs::create_dir_all(&bundled).unwrap();
    std::fs::write(bundled.join("x.clap"), "first").unwrap();
    let map = bundled.join("x.control-map.json");
    std::fs::write(&map, "{}").unwrap();

    // Into a folder that isn't there yet.
    let outcomes = installs(&bundled, std::slice::from_ref(&map), &folder);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].1.as_ref().unwrap(), &folder.join("x.clap"));
    assert_eq!(
        std::fs::read_to_string(folder.join("x.clap")).unwrap(),
        "first"
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("x.control-map.json")).unwrap(),
        "{}"
    );

    // A second build, without a map this time: no older map stays beside it.
    std::fs::write(bundled.join("x.clap"), "second").unwrap();
    let outcomes = installs(&bundled, &[], &folder);
    assert!(outcomes[0].1.is_ok(), "{:?}", outcomes[0].1);
    assert_eq!(
        std::fs::read_to_string(folder.join("x.clap")).unwrap(),
        "second"
    );
    assert_eq!(names_in(&folder), [LOCK, "x.clap"]);
}

#[test]
fn a_map_that_fails_to_go_in_leaves_none_beside_the_new_plugin() {
    let scratch = Scratch::new("map-failed");
    let bundled = scratch.0.join("bundled");
    let folder = scratch.0.join("CLAP");
    std::fs::create_dir_all(&bundled).unwrap();
    std::fs::write(bundled.join("x.clap"), "first").unwrap();
    let map = bundled.join("x.control-map.json");
    std::fs::write(&map, "{}").unwrap();
    assert!(
        installs(&bundled, std::slice::from_ref(&map), &folder)[0]
            .1
            .is_ok()
    );

    // A staged map that is gone by the time it is copied.
    std::fs::write(bundled.join("x.clap"), "second").unwrap();
    std::fs::remove_file(&map).unwrap();
    let outcomes = installs(&bundled, std::slice::from_ref(&map), &folder);
    let error = format!("{:#}", outcomes[0].1.as_ref().unwrap_err());
    assert!(
        error.contains("is installed, but not its control map"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(folder.join("x.clap")).unwrap(),
        "second"
    );
    assert_eq!(names_in(&folder), [LOCK, "x.clap"]);
}

#[test]
fn only_a_package_bundler_toml_names_and_the_command_line_bundled_is_installed() {
    let scratch = Scratch::new("named");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let folder = scratch.0.join("CLAP");
    let outcomes = install(&bundles(), &strings(&["other"]), &scratch.0, &[], &folder);
    assert!(outcomes.is_empty());
    assert!(!folder.exists());
}

#[test]
fn a_failed_install_is_an_outcome_not_an_error() {
    let scratch = Scratch::new("failed");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    // A file stands where the folder would be made.
    let folder = scratch.0.join("CLAP");
    std::fs::write(&folder, "not a folder").unwrap();
    let outcomes = installs(&scratch.0, &[], &folder);
    assert_eq!(outcomes.len(), 1);
    let error = format!("{:#}", outcomes[0].1.as_ref().unwrap_err());
    assert!(error.starts_with("not installed"), "{error}");
}

#[test]
fn what_an_earlier_install_left_goes_at_the_next() {
    let scratch = Scratch::new("leftovers");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let folder = scratch.0.join("CLAP");
    std::fs::create_dir_all(folder.join("x.clap.new")).unwrap();
    for left in [
        "x.clap.tmp",
        "x.clap.123.old",
        "x.control-map.json.tmp",
        "y.clap.123.old",
    ] {
        std::fs::write(folder.join(left), "left").unwrap();
    }
    assert!(installs(&scratch.0, &[], &folder)[0].1.is_ok());
    // Another plugin's leftover is that plugin's to clear.
    assert_eq!(names_in(&folder), [LOCK, "x.clap", "y.clap.123.old"]);
}

#[test]
fn a_bundle_folder_is_swapped_in_whole() {
    let scratch = Scratch::new("folder");
    let bundled = scratch.0.join("bundled");
    let folder = scratch.0.join("CLAP");
    let contents = bundled.join("x.clap").join("Contents");
    std::fs::create_dir_all(&contents).unwrap();
    std::fs::write(contents.join("binary"), "first").unwrap();
    std::fs::write(contents.join("dropped"), "first").unwrap();
    assert!(installs(&bundled, &[], &folder)[0].1.is_ok());

    std::fs::remove_file(contents.join("dropped")).unwrap();
    std::fs::write(contents.join("binary"), "second").unwrap();
    assert!(installs(&bundled, &[], &folder)[0].1.is_ok());
    let installed = folder.join("x.clap").join("Contents");
    assert_eq!(
        std::fs::read_to_string(installed.join("binary")).unwrap(),
        "second"
    );
    assert!(
        !installed.join("dropped").exists(),
        "an old bundle is never merged into"
    );
    assert_eq!(names_in(&folder), [LOCK, "x.clap"]);
}

#[cfg(windows)]
mod held {
    use super::*;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut std::ffi::c_void;
        fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
    }

    /// A library loaded as a host loads a plugin, freed when dropped.
    struct Loaded(*mut std::ffi::c_void);

    impl Loaded {
        fn new(path: &Path) -> Self {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            // SAFETY: a null-terminated path; the library is a copy of a system one, whose
            // initialisation does nothing to this process.
            let module = unsafe { LoadLibraryW(wide.as_ptr()) };
            assert!(!module.is_null(), "{} didn't load", path.display());
            Self(module)
        }
    }

    impl Drop for Loaded {
        fn drop(&mut self) {
            // SAFETY: the module `new` loaded, freed once.
            unsafe { FreeLibrary(self.0) };
        }
    }

    #[test]
    fn a_plugin_a_host_holds_is_replaced_and_its_old_copy_cleared_once_freed() {
        let scratch = Scratch::new("held");
        let bundled = scratch.0.join("bundled");
        let folder = scratch.0.join("CLAP");
        std::fs::create_dir_all(&bundled).unwrap();
        let system = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
        let library = PathBuf::from(system).join("System32").join("version.dll");
        std::fs::copy(&library, bundled.join("x.clap")).unwrap();
        assert!(installs(&bundled, &[], &folder)[0].1.is_ok());

        let host = Loaded::new(&folder.join("x.clap"));
        assert!(
            std::fs::write(folder.join("x.clap"), "over").is_err(),
            "a loaded library can't be overwritten, or this test proves nothing"
        );
        std::fs::write(bundled.join("x.clap"), "second").unwrap();
        let outcome = &installs(&bundled, &[], &folder)[0].1;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(
            std::fs::read_to_string(folder.join("x.clap")).unwrap(),
            "second"
        );
        let names = names_in(&folder);
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(
            names[2].starts_with("x.clap.") && names[2].ends_with(".old"),
            "{names:?}"
        );

        drop(host);
        assert!(installs(&bundled, &[], &folder)[0].1.is_ok());
        assert_eq!(names_in(&folder), [LOCK, "x.clap"]);
    }
}
