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

/// The CLAP going into `clap`, the VST3 into a `VST3` beside it, and no older copy to retire.
fn folders(clap: &Path) -> Folders {
    Folders {
        clap: clap.to_path_buf(),
        vst3: Some(clap.with_file_name("VST3")),
        retired: None,
    }
}

fn installs(bundled: &Path, staged: &[PathBuf], folder: &Path) -> Vec<Outcome> {
    install(
        &bundles(),
        &strings(&["x-plugin"]),
        bundled,
        staged,
        &folders(folder),
    )
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
    assert_eq!(outcomes[0].result.as_ref().unwrap(), &folder.join("x.clap"));
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
    assert!(outcomes[0].result.is_ok(), "{:?}", outcomes[0].result);
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
            .result
            .is_ok()
    );

    // A staged map that is gone by the time it is copied.
    std::fs::write(bundled.join("x.clap"), "second").unwrap();
    std::fs::remove_file(&map).unwrap();
    let outcomes = installs(&bundled, std::slice::from_ref(&map), &folder);
    let error = format!("{:#}", outcomes[0].result.as_ref().unwrap_err());
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
    let outcomes = install(
        &bundles(),
        &strings(&["other"]),
        &scratch.0,
        &[],
        &folders(&folder),
    );
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
    let error = format!("{:#}", outcomes[0].result.as_ref().unwrap_err());
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
    assert!(installs(&scratch.0, &[], &folder)[0].result.is_ok());
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
    assert!(installs(&bundled, &[], &folder)[0].result.is_ok());

    std::fs::remove_file(contents.join("dropped")).unwrap();
    std::fs::write(contents.join("binary"), "second").unwrap();
    assert!(installs(&bundled, &[], &folder)[0].result.is_ok());
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

/// A VST3 bundle folder with `module` as its library, as nice-plug's bundler makes one on Windows.
fn vst3_bundle(bundled: &Path, module: &[u8]) -> PathBuf {
    let folder = bundled.join("x.vst3").join("Contents").join("x86_64-win");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("x.vst3"), module).unwrap();
    folder.join("x.vst3")
}

#[test]
fn a_vst3_installs_beside_the_clap_and_the_map_stays_with_the_clap() {
    let scratch = Scratch::new("vst3");
    let bundled = scratch.0.join("bundled");
    let folder = scratch.0.join("CLAP");
    std::fs::create_dir_all(&bundled).unwrap();
    std::fs::write(bundled.join("x.clap"), "built").unwrap();
    vst3_bundle(&bundled, b"module");
    let map = bundled.join("x.control-map.json");
    std::fs::write(&map, "{}").unwrap();

    let outcomes = installs(&bundled, std::slice::from_ref(&map), &folder);
    let formats: Vec<Format> = outcomes.iter().map(|outcome| outcome.format).collect();
    assert_eq!(formats, [Format::Clap, Format::Vst3]);
    let vst3 = scratch.0.join("VST3");
    assert_eq!(outcomes[1].result.as_ref().unwrap(), &vst3.join("x.vst3"));
    assert_eq!(names_in(&folder), [LOCK, "x.clap", "x.control-map.json"]);
    assert_eq!(names_in(&vst3), [LOCK, "x.vst3"]);
    let module = vst3.join("x.vst3/Contents/x86_64-win/x.vst3");
    assert_eq!(std::fs::read_to_string(module).unwrap(), "module");
}

#[test]
fn a_plugin_with_no_vst3_installs_its_clap_alone() {
    let scratch = Scratch::new("clap-only");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let folder = scratch.0.join("CLAP");
    let outcomes = installs(&scratch.0, &[], &folder);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].format, Format::Clap);
    assert!(outcomes[0].result.is_ok());
    assert!(!scratch.0.join("VST3").exists());
}

#[test]
fn the_older_copy_goes_once_the_new_one_is_in_and_nothing_else_there_does() {
    let scratch = Scratch::new("retired");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let retired = scratch.0.join("user");
    std::fs::create_dir_all(&retired).unwrap();
    for (name, contents) in [
        ("x.clap", "old"),
        ("x.control-map.json", "{}"),
        ("x.clap.123.old", "older"),
        ("y.clap", "another plugin"),
        ("notes.txt", "the user's"),
    ] {
        std::fs::write(retired.join(name), contents).unwrap();
    }
    let folders = Folders {
        clap: scratch.0.join("CLAP"),
        vst3: Some(scratch.0.join("VST3")),
        retired: Some(retired.clone()),
    };
    let outcomes = install(
        &bundles(),
        &strings(&["x-plugin"]),
        &scratch.0,
        &[],
        &folders,
    );
    assert!(outcomes[0].result.is_ok());
    assert!(outcomes[0].left_behind.is_empty());
    // Only the lock it took is added, as an install there makes; no host lists it.
    assert_eq!(names_in(&retired), [LOCK, "notes.txt", "y.clap"]);
}

#[test]
fn a_failed_install_leaves_the_older_copy_where_it_is() {
    let scratch = Scratch::new("retired-failed");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let retired = scratch.0.join("user");
    std::fs::create_dir_all(&retired).unwrap();
    std::fs::write(retired.join("x.clap"), "old").unwrap();
    // A file stands where the folder would be made, as an unwritable system folder refuses.
    let clap = scratch.0.join("CLAP");
    std::fs::write(&clap, "not a folder").unwrap();
    let folders = Folders {
        clap,
        vst3: Some(scratch.0.join("VST3")),
        retired: Some(retired.clone()),
    };
    let outcomes = install(
        &bundles(),
        &strings(&["x-plugin"]),
        &scratch.0,
        &[],
        &folders,
    );
    assert!(outcomes[0].result.is_err());
    assert_eq!(names_in(&retired), ["x.clap"]);
}

/// An install into the older folder holds its lock; retiring waits its turn and never cuts in.
#[test]
fn retiring_waits_for_an_install_holding_the_folders_lock() {
    let scratch = Scratch::new("retired-locked");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let retired = scratch.0.join("user");
    std::fs::create_dir_all(&retired).unwrap();
    std::fs::write(retired.join("x.clap"), "old").unwrap();
    let held = lock(&retired).unwrap();
    let folders = Folders {
        clap: scratch.0.join("CLAP"),
        vst3: Some(scratch.0.join("VST3")),
        retired: Some(retired.clone()),
    };
    let bundled = scratch.0.clone();
    let installing = std::thread::spawn(move || {
        install(&bundles(), &strings(&["x-plugin"]), &bundled, &[], &folders)
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        retired.join("x.clap").exists(),
        "retired while another install held the folder"
    );
    drop(held);
    let outcomes = installing.join().unwrap();
    assert!(outcomes[0].result.is_ok());
    assert!(outcomes[0].left_behind.is_empty());
    assert!(!retired.join("x.clap").exists());
}

#[test]
fn a_stale_vst3_is_cleared_before_the_build_so_it_is_never_installed() {
    let scratch = Scratch::new("stale-vst3");
    let bundled = scratch.0.join("bundled");
    std::fs::create_dir_all(&bundled).unwrap();
    // An earlier build's VST3, and another plugin's, which isn't this command line's to clear.
    vst3_bundle(&bundled, b"earlier");
    let other = bundled.join("y.vst3");
    std::fs::create_dir_all(&other).unwrap();
    clear_stale_vst3s(&bundles(), &strings(&["x-plugin"]), &bundled).unwrap();
    assert!(!bundled.join("x.vst3").exists());
    assert!(other.exists());

    // This build made only the CLAP: only the CLAP is installed.
    std::fs::write(bundled.join("x.clap"), "built").unwrap();
    let folder = scratch.0.join("CLAP");
    let outcomes = installs(&bundled, &[], &folder);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].format, Format::Clap);
}

#[test]
fn a_clap_only_plugin_needs_no_vst3_folder_and_a_vst3_says_it_has_none() {
    let scratch = Scratch::new("no-vst3-folder");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let mut folders = folders(&scratch.0.join("CLAP"));
    folders.vst3 = None;
    let install_x = |folders: &Folders| {
        install(
            &bundles(),
            &strings(&["x-plugin"]),
            &scratch.0,
            &[],
            folders,
        )
    };
    let outcomes = install_x(&folders);
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].result.is_ok());

    vst3_bundle(&scratch.0, b"module");
    let outcomes = install_x(&folders);
    assert!(outcomes[0].result.is_ok(), "the CLAP still goes in");
    let error = format!("{:#}", outcomes[1].result.as_ref().unwrap_err());
    assert!(
        error.starts_with("not installed") && error.contains("MXM_VST3_DIR"),
        "{error}"
    );
}

#[test]
fn a_folder_the_variable_names_comes_before_the_standard_one() {
    let standard = Some(PathBuf::from("standard"));
    assert_eq!(
        chosen(Some("mine".into()), standard.clone()),
        Some(PathBuf::from("mine"))
    );
    assert_eq!(chosen(Some("".into()), standard.clone()), standard);
    assert_eq!(chosen(None, standard.clone()), standard);
    assert_eq!(chosen(None, None), None);
}

#[test]
fn the_standard_folders_are_the_ones_everyone_uses() {
    let clap = standard_folder(Format::Clap).unwrap();
    let vst3 = standard_folder(Format::Vst3).unwrap();
    if cfg!(windows) {
        let common = PathBuf::from(std::env::var_os("COMMONPROGRAMFILES").unwrap());
        assert_eq!(clap, common.join("CLAP"));
        assert_eq!(vst3, common.join("VST3"));
    } else if cfg!(target_os = "macos") {
        assert!(clap.ends_with("Library/Audio/Plug-Ins/CLAP"), "{clap:?}");
        assert!(vst3.ends_with("Library/Audio/Plug-Ins/VST3"), "{vst3:?}");
    } else {
        assert!(clap.ends_with(".clap"), "{clap:?}");
        assert!(vst3.ends_with(".vst3"), "{vst3:?}");
    }
}

#[test]
fn a_copy_newdawn_scans_first_is_found_and_one_it_scans_after_is_not() {
    let scratch = Scratch::new("found-first");
    let [first, installed, after] =
        ["first", "installed", "after"].map(|name| scratch.0.join(name));
    for folder in [&first, &installed, &after] {
        std::fs::create_dir_all(folder.join("nested")).unwrap();
    }
    std::fs::write(first.join("nested").join("x.clap"), "older").unwrap();
    std::fs::write(installed.join("x.clap"), "this build").unwrap();
    std::fs::write(after.join("x.clap"), "after").unwrap();
    let order = [first.clone(), installed.clone(), after.clone()];
    assert_eq!(
        found_first(&installed.join("x.clap"), &order),
        [first.join("nested").join("x.clap")]
    );
    // A VST3 bundle is found as the folder it is, and not looked into.
    vst3_bundle(&first, b"older");
    vst3_bundle(&installed, b"this build");
    assert_eq!(
        found_first(&installed.join("x.vst3"), &order),
        [first.join("x.vst3")]
    );
    // A folder newDAWn doesn't scan hides nothing and is hidden by nothing.
    let elsewhere = scratch.0.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert_eq!(scanning_root(&elsewhere, &order), None);
    assert!(found_first(&elsewhere.join("x.clap"), &order).is_empty());
}

/// `CLAP_PATH=…\plugins` with the install in `…\plugins\mxm`: newDAWn scans it, takes that root's
/// bundles in sorted order, and the installed bundle is never a copy of itself.
#[test]
fn a_folder_inside_a_scanned_one_is_scanned_and_never_warns_about_itself() {
    let scratch = Scratch::new("nested-root");
    let root = scratch.0.join("plugins");
    for folder in ["a", "mxm", "z"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join("x.clap"), folder).unwrap();
    }
    let order = [root.clone()];
    assert_eq!(scanning_root(&root.join("mxm"), &order), Some(0));
    assert_eq!(
        found_first(&root.join("mxm").join("x.clap"), &order),
        [root.join("a").join("x.clap")]
    );
}

/// `MXM_CLAP_DIR` naming the older per-user folder by another spelling installs into it, and the
/// plugin just installed is not then taken back out as the older copy.
#[test]
fn an_install_into_the_older_folder_by_another_spelling_is_not_taken_back_out() {
    let scratch = Scratch::new("retired-alias");
    std::fs::write(scratch.0.join("x.clap"), "built").unwrap();
    let retired = scratch.0.join("user");
    std::fs::create_dir_all(&retired).unwrap();
    std::fs::write(retired.join("x.clap"), "old").unwrap();
    let folders = Folders {
        clap: retired.join("..").join("user"),
        vst3: Some(scratch.0.join("VST3")),
        retired: Some(retired.clone()),
    };
    let outcomes = install(
        &bundles(),
        &strings(&["x-plugin"]),
        &scratch.0,
        &[],
        &folders,
    );
    assert!(outcomes[0].result.is_ok());
    assert_eq!(
        std::fs::read_to_string(retired.join("x.clap")).unwrap(),
        "built"
    );
}

/// `MXM_CLAP_DIR` naming `target/bundled` itself, as written or by another spelling, is refused and
/// touches nothing there.
#[test]
fn a_destination_that_is_the_bundle_folder_is_refused_and_touched_nowhere() {
    let scratch = Scratch::new("in-place");
    let bundled = scratch.0.join("bundled");
    std::fs::create_dir_all(&bundled).unwrap();
    std::fs::write(bundled.join("x.clap"), "built").unwrap();
    let map = bundled.join("x.control-map.json");
    std::fs::write(&map, "{}").unwrap();
    for destination in [bundled.clone(), bundled.join("..").join("bundled")] {
        let outcomes = installs(&bundled, std::slice::from_ref(&map), &destination);
        let error = format!("{:#}", outcomes[0].result.as_ref().unwrap_err());
        assert!(
            error.starts_with("not installed") && error.contains("MXM_CLAP_DIR"),
            "{error}"
        );
        assert_eq!(
            std::fs::read_to_string(bundled.join("x.clap")).unwrap(),
            "built"
        );
        assert_eq!(std::fs::read_to_string(&map).unwrap(), "{}");
        assert_eq!(names_in(&bundled), ["x.clap", "x.control-map.json"]);
    }
}

/// With a custom `CARGO_TARGET_DIR` the bundle is made in one folder and its map staged in the
/// repository's `target/bundled`: `MXM_CLAP_DIR` naming the latter is refused as well.
#[test]
fn a_destination_where_the_map_is_staged_is_refused_and_touched_nowhere() {
    let scratch = Scratch::new("staged-here");
    let bundled = scratch.0.join("custom").join("bundled");
    let staged = scratch.0.join("repository").join("target").join("bundled");
    std::fs::create_dir_all(&bundled).unwrap();
    std::fs::create_dir_all(&staged).unwrap();
    std::fs::write(bundled.join("x.clap"), "built").unwrap();
    let map = staged.join("x.control-map.json");
    std::fs::write(&map, "{}").unwrap();
    let outcomes = installs(&bundled, std::slice::from_ref(&map), &staged);
    let error = format!("{:#}", outcomes[0].result.as_ref().unwrap_err());
    assert!(error.starts_with("not installed"), "{error}");
    assert_eq!(std::fs::read_to_string(&map).unwrap(), "{}");
    assert_eq!(names_in(&staged), ["x.control-map.json"]);
}

/// An earlier scan root that reaches the install folder through a link finds the installed bundle
/// again: it is never reported as a copy of itself.
#[test]
fn the_installed_bundle_reached_through_a_link_is_not_a_copy() {
    let scratch = Scratch::new("linked-root");
    let first = scratch.0.join("first");
    let installed = scratch.0.join("installed");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::write(installed.join("x.clap"), "this build").unwrap();
    let link = first.join("link");
    #[cfg(windows)]
    let linked = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&installed)
        .output()
        .is_ok_and(|out| out.status.success());
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&installed, &link).is_ok();
    assert!(
        linked,
        "the link couldn't be made, so this test proves nothing"
    );
    assert!(link.join("x.clap").exists());
    let order = [first.clone(), installed.clone()];
    assert!(found_first(&installed.join("x.clap"), &order).is_empty());
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
        assert!(installs(&bundled, &[], &folder)[0].result.is_ok());

        let host = Loaded::new(&folder.join("x.clap"));
        assert!(
            std::fs::write(folder.join("x.clap"), "over").is_err(),
            "a loaded library can't be overwritten, or this test proves nothing"
        );
        std::fs::write(bundled.join("x.clap"), "second").unwrap();
        let outcome = &installs(&bundled, &[], &folder)[0].result;
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
        assert!(installs(&bundled, &[], &folder)[0].result.is_ok());
        assert_eq!(names_in(&folder), [LOCK, "x.clap"]);
    }

    /// The VST3's bundle is a folder, and Windows lets one be renamed with its module loaded
    /// (tried 2026-10-16), so it is swapped whole like a `.clap`.
    #[test]
    fn a_vst3_a_host_holds_is_replaced_and_its_old_bundle_cleared_once_freed() {
        let scratch = Scratch::new("held-vst3");
        let bundled = scratch.0.join("bundled");
        let folder = scratch.0.join("CLAP");
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::write(bundled.join("x.clap"), "clap").unwrap();
        let system = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
        let library =
            std::fs::read(PathBuf::from(system).join("System32").join("version.dll")).unwrap();
        let module = vst3_bundle(&bundled, &library);
        assert!(installs(&bundled, &[], &folder)[1].result.is_ok());

        let vst3 = scratch.0.join("VST3");
        let installed = vst3.join("x.vst3/Contents/x86_64-win/x.vst3");
        let host = Loaded::new(&installed);
        assert!(
            std::fs::write(&installed, "over").is_err(),
            "a loaded module can't be overwritten, or this test proves nothing"
        );
        std::fs::write(&module, "second").unwrap();
        let outcome = &installs(&bundled, &[], &folder)[1].result;
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "second");
        let names = names_in(&vst3);
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(
            names[2].starts_with("x.vst3.") && names[2].ends_with(".old"),
            "{names:?}"
        );
        // A host looks for `.vst3` at any depth: the held module in the bundle set aside is
        // renamed too, so the new bundle is the only plugin there.
        assert_eq!(plugins_in(&vst3), [vst3.join("x.vst3")]);

        drop(host);
        assert!(installs(&bundled, &[], &folder)[1].result.is_ok());
        assert_eq!(names_in(&vst3), [LOCK, "x.vst3"]);
    }

    /// Every `.vst3` a host scanning `folder` at any depth would offer: a bundle folder, not
    /// looked into, or a lone file.
    fn plugins_in(folder: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "vst3")
            {
                found.push(path);
            } else if path.is_dir() {
                found.extend(plugins_in(&path));
            }
        }
        found
    }
}
