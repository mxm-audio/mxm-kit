//! Each plugin's factory presets, installed with it (`ops/plans/plan-external-presets.md` in the
//! workspace): `plugins/<crate>/presets/` is checked before the build, then staged beside the
//! bundled `.clap` as `<Bundle Name>.presets/`, as its control map is, and put inside the `.vst3`
//! as `Contents/Resources/presets/`, as its `moduleinfo.json` is. Each replaces what an earlier
//! bundle left, so a file taken out of the repository is gone from the bundle, and a plugin with
//! no `presets/` folder has none staged.
//!
//! The check reads the files as JSON, not through `mxm-preset`, which would bring the plugin
//! framework into the build tool, but field by field as `mxm-preset`'s `Preset` reads them, so a
//! file it passes is one the plugin parses: the schema's version, a name, a plugin, each
//! parameter a value `v` with its text, and a category and comment, if there, as text. All are of
//! one plugin, and the folder's order names only files that are there. Each plugin's own tests
//! parse every file for its instrument.

use anyhow::{Context, Result, anyhow, bail};
use std::path::{Path, PathBuf};

use crate::control_maps::bundle_names;

/// A plugin's presets in its repository: `plugins/<crate>/presets/`.
pub const SOURCE: &str = "presets";
/// The file in a presets folder that gives their order, one file name a line (`mxm-preset`'s
/// `ORDER_FILE`).
pub const ORDER: &str = "order.txt";
/// The preset schema the plugins read (`mxm-preset`'s `SCHEMA_VERSION`): a file of another is
/// refused when it's loaded, so it's refused before the build.
pub const SCHEMA_VERSION: u64 = 1;

/// The presets folder staged beside a bundled `.clap`, and installed beside it.
pub fn staged_name(bundle: &str) -> String {
    format!("{bundle}.presets")
}

/// Where a `.vst3` carries its presets.
pub fn in_vst3(vst3: &Path) -> PathBuf {
    vst3.join("Contents").join("Resources").join("presets")
}

/// Checks every plugin's presets. A plugin with no `presets/` folder passes.
pub fn check(workspace_root: &Path) -> Result<()> {
    for (crate_dir, _) in bundle_names(workspace_root)? {
        let folder = crate_dir.join(SOURCE);
        if folder.is_dir() {
            check_folder(&folder)
                .with_context(|| format!("{} isn't a set of presets", folder.display()))?;
        }
    }
    Ok(())
}

/// One folder: each `.json` a preset of one plugin, and the order naming only its presets, by
/// their file names: anything else is an order the plugin can't follow.
pub fn check_folder(folder: &Path) -> Result<()> {
    let mut plugin: Option<String> = None;
    let files = preset_files(folder)?;
    for path in &files {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("{} can't be read", path.display()))?;
        let preset: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("{} isn't JSON", path.display()))?;
        shaped_as_a_preset(&preset).map_err(|why| anyhow!("{} {why}", path.display()))?;
        let Some(this) = preset["plugin"].as_str().filter(|id| !id.is_empty()) else {
            bail!("{} names no plugin", path.display());
        };
        match &plugin {
            Some(first) if first != this => bail!(
                "{} is for `{this}`, and the folder's others are for `{first}`",
                path.display()
            ),
            _ => plugin = Some(this.to_owned()),
        }
    }
    let order = folder.join(ORDER);
    if order.exists() {
        let text = std::fs::read_to_string(&order)
            .with_context(|| format!("{} can't be read", order.display()))?;
        let names: Vec<&std::ffi::OsStr> =
            files.iter().filter_map(|path| path.file_name()).collect();
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if !names.contains(&std::ffi::OsStr::new(line)) {
                return Err(anyhow!(
                    "{} names {line}, which isn't one of the folder's presets",
                    order.display()
                ));
            }
        }
    }
    Ok(())
}

/// Whether `preset` has the shape `mxm-preset` parses, and if not, what it lacks: an object of
/// the schema's version, a name and a plugin, its `params` an object of values, each a number
/// `v` and, if there, a text, and a category and comment, if there, as text.
fn shaped_as_a_preset(preset: &serde_json::Value) -> Result<(), String> {
    let text_or_absent = |name: &str| preset.get(name).is_none_or(serde_json::Value::is_string);
    match preset.get("schema_version").map(serde_json::Value::as_u64) {
        Some(Some(SCHEMA_VERSION)) => {}
        Some(Some(found)) => {
            return Err(format!(
                "is schema version {found}, and the plugins read version {SCHEMA_VERSION}"
            ));
        }
        _ => return Err("isn't a preset: it needs a schema_version".to_owned()),
    }
    if preset
        .get("name")
        .and_then(serde_json::Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err("isn't a preset: it needs a name".to_owned());
    }
    if !text_or_absent("plugin") || !text_or_absent("category") || !text_or_absent("_comment") {
        return Err("isn't a preset: its plugin, category and _comment are text".to_owned());
    }
    let Some(params) = preset.get("params").and_then(serde_json::Value::as_object) else {
        return Err("isn't a preset: it needs params".to_owned());
    };
    for (id, value) in params {
        let number = value.get("v").is_some_and(serde_json::Value::is_number);
        let text = value.get("text").is_none_or(serde_json::Value::is_string);
        if !(value.is_object() && number && text) {
            return Err(format!(
                "has a parameter `{id}` that isn't a value: a number `v`, and its `text`"
            ));
        }
    }
    Ok(())
}

/// Stages each plugin's presets beside its `.clap`, in the repository's `target/bundled` where
/// its control map is staged whichever folder Cargo builds in, and inside its `.vst3` in
/// `bundled`, the folder Cargo built it in (`CARGO_TARGET_DIR` moves it), each replacing what an
/// earlier bundle left there. Returns the folders staged beside the `.clap`s.
pub fn stage(workspace_root: &Path, bundled: &Path) -> Result<Vec<PathBuf>> {
    let staging = workspace_root.join("target").join("bundled");
    let mut staged = Vec::new();
    for (crate_dir, bundle) in bundle_names(workspace_root)? {
        let source = crate_dir.join(SOURCE);
        let beside = staging.join(staged_name(&bundle));
        let vst3 = bundled.join(format!("{bundle}.vst3"));
        remove(&beside)?;
        if vst3.is_dir() {
            remove(&in_vst3(&vst3))?;
        }
        if !source.is_dir() {
            continue;
        }
        copy_presets(&source, &beside)?;
        if vst3.is_dir() {
            copy_presets(&source, &in_vst3(&vst3))?;
        }
        staged.push(beside);
    }
    Ok(staged)
}

/// The preset files in `folder`, its `.json`s, in name order.
fn preset_files(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .with_context(|| format!("{} can't be read", folder.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    Ok(files)
}

/// Copies a presets folder's `.json`s and its order, and nothing else in it.
fn copy_presets(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("{} can't be made", to.display()))?;
    let mut files = preset_files(from)?;
    if from.join(ORDER).is_file() {
        files.push(from.join(ORDER));
    }
    for file in files {
        let name = file.file_name().unwrap_or_default();
        std::fs::copy(&file, to.join(name))
            .with_context(|| format!("{} can't be copied to {}", file.display(), to.display()))?;
    }
    Ok(())
}

fn remove(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("{} can't be removed", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "mxm-xtask-presets-{test}-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn preset(plugin: &str, name: &str) -> String {
        format!(
            r#"{{"schema_version": 1, "plugin": "{plugin}", "name": "{name}", "params": {{}}}}"#
        )
    }

    /// A repository with one plugin, `x-plugin`, bundled as `x`.
    fn repository(scratch: &Scratch) -> PathBuf {
        let root = scratch.0.join("repo");
        let presets = root.join("plugins").join("x-plugin").join(SOURCE);
        std::fs::create_dir_all(&presets).unwrap();
        std::fs::write(root.join("bundler.toml"), "[x-plugin]\nname = \"x\"\n").unwrap();
        root
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
    fn presets_are_staged_beside_the_clap_and_in_the_vst3_replacing_the_last() {
        let scratch = Scratch::new("staged");
        let root = repository(&scratch);
        let presets = root.join("plugins").join("x-plugin").join(SOURCE);
        std::fs::write(presets.join("a.json"), preset("dk.x", "A")).unwrap();
        std::fs::write(presets.join("b.json"), preset("dk.x", "B")).unwrap();
        std::fs::write(presets.join(ORDER), "b.json\na.json\n").unwrap();
        std::fs::write(presets.join("notes.md"), "not shipped").unwrap();
        let bundled = root.join("target").join("bundled");
        // Cargo builds in a folder of its own (`CARGO_TARGET_DIR`): the `.vst3` is there.
        let built = scratch.0.join("shared").join("bundled");
        let vst3 = built.join("x.vst3");
        std::fs::create_dir_all(vst3.join("Contents").join("x86_64-win")).unwrap();

        let staged = stage(&root, &built).unwrap();
        assert_eq!(staged, [bundled.join("x.presets")]);
        let shipped = ["a.json", "b.json", ORDER];
        assert_eq!(names_in(&bundled.join("x.presets")), shipped);
        assert_eq!(names_in(&in_vst3(&vst3)), shipped);

        // A file taken out of the repository is gone from the next bundle.
        std::fs::remove_file(presets.join("a.json")).unwrap();
        std::fs::write(presets.join(ORDER), "b.json\n").unwrap();
        stage(&root, &built).unwrap();
        assert_eq!(names_in(&bundled.join("x.presets")), ["b.json", ORDER]);
        assert_eq!(names_in(&in_vst3(&vst3)), ["b.json", ORDER]);

        // A plugin without presets has none staged.
        std::fs::remove_dir_all(&presets).unwrap();
        assert!(stage(&root, &built).unwrap().is_empty());
        assert!(!bundled.join("x.presets").exists());
        assert!(!in_vst3(&vst3).exists());
    }

    #[test]
    fn a_folder_is_one_plugins_presets_in_an_order_naming_only_what_is_there() {
        let scratch = Scratch::new("checked");
        let folder = scratch.0.join("presets");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.json"), preset("dk.x", "A")).unwrap();
        std::fs::write(folder.join(ORDER), "a.json\n").unwrap();
        check_folder(&folder).unwrap();

        std::fs::write(folder.join(ORDER), "a.json\nmissing.json\n").unwrap();
        assert!(
            check_folder(&folder).is_err(),
            "the order names a missing file"
        );
        std::fs::write(folder.join("notes.txt"), "not a preset").unwrap();
        std::fs::write(scratch.0.join("outside.json"), preset("dk.x", "Out")).unwrap();
        for line in ["notes.txt", "../outside.json", "./a.json"] {
            std::fs::write(folder.join(ORDER), format!("{line}\n")).unwrap();
            assert!(check_folder(&folder).is_err(), "the order names {line}");
        }
        std::fs::remove_file(folder.join("notes.txt")).unwrap();
        std::fs::write(folder.join(ORDER), "a.json\n").unwrap();

        std::fs::write(folder.join("b.json"), preset("dk.y", "B")).unwrap();
        assert!(check_folder(&folder).is_err(), "two plugins' presets");
        std::fs::write(folder.join("b.json"), r#"{"name": "B"}"#).unwrap();
        assert!(check_folder(&folder).is_err(), "not a preset's shape");
        std::fs::write(folder.join("b.json"), "{ not json").unwrap();
        assert!(check_folder(&folder).is_err(), "not JSON");
    }

    /// A file the plugin's own parsing would refuse, or a value it would skip, is refused before
    /// the build, and one it reads passes.
    #[test]
    fn a_file_the_plugin_would_refuse_is_refused_before_the_build() {
        let scratch = Scratch::new("shaped");
        let folder = scratch.0.join("presets");
        std::fs::create_dir_all(&folder).unwrap();
        let file = |fields: &str| {
            format!(r#"{{"schema_version": 1, "plugin": "dk.x", "name": "A"{fields}}}"#)
        };
        let read = [
            file(r#", "params": {}"#),
            file(r#", "params": {"cutoff": {"v": 0.5, "text": "1 kHz"}}"#),
            file(r#", "params": {"cutoff": {"v": 1}}, "category": "Bass", "_comment": "x""#),
            file(r#", "params": {}, "state": {"anything": [1, 2]}"#),
        ];
        for text in &read {
            std::fs::write(folder.join("a.json"), text).unwrap();
            check_folder(&folder).unwrap_or_else(|why| panic!("{text}: {why:#}"));
        }
        let refused = [
            r#"{"schema_version": 2, "plugin": "dk.x", "name": "A", "params": {}}"#.to_owned(),
            r#"{"schema_version": "1", "plugin": "dk.x", "name": "A", "params": {}}"#.to_owned(),
            file(r#", "params": {"cutoff": "invalid"}"#),
            file(r#", "params": {"cutoff": {"text": "1 kHz"}}"#),
            file(r#", "params": {"cutoff": {"v": "0.5"}}"#),
            file(r#", "params": {"cutoff": {"v": 0.5, "text": 3}}"#),
            file(r#", "params": [] "#),
            file(r#", "params": {}, "category": 4"#),
            file(r#", "params": {}, "_comment": ["x"]"#),
            r#"{"schema_version": 1, "plugin": 7, "name": "A", "params": {}}"#.to_owned(),
        ];
        for text in &refused {
            std::fs::write(folder.join("a.json"), text).unwrap();
            assert!(check_folder(&folder).is_err(), "{text}");
        }
    }
}
