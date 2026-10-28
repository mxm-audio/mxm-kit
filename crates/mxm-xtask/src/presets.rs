//! Each plugin's factory presets, installed for its version (`ops/plans/plan-external-presets.md`
//! in the workspace): `plugins/<crate>/presets/` is checked before the build, then staged in the
//! repository's `target/bundled` as `<Bundle Name>.presets/`, as its control map is, and installed
//! in `mxm/<clap id>/factory/<version>/` under the local data directory ([`folder_for`]), where the
//! plugin reads them (`mxm-preset`'s `factory_dir`; the owner, 2026-10-28: a data folder, one per
//! version). A staging replaces what an earlier bundle left, so a file taken out of the repository
//! is gone from the next install, and a plugin with no `presets/` folder has none staged.
//!
//! The check reads the files not through `mxm-preset`, which would bring the plugin framework
//! into the build tool, but into a copy of its `Preset`'s shape, read by the same serde, so a file
//! it passes is one the plugin parses (`PresetFile`): of the schema's version, a name and a
//! plugin, each parameter a value `v` with its text. All are of one plugin, and the folder's
//! order names only files that are there. Each plugin's own tests parse every file for its
//! instrument.

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
pub const SCHEMA_VERSION: u32 = 1;

/// The presets folder staged in a repository's `target/bundled`.
pub fn staged_name(bundle: &str) -> String {
    format!("{bundle}.presets")
}

/// Where factory presets are installed: `mxm` under the local data directory. `None` where the
/// platform has none.
pub fn root() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("mxm"))
}

/// The folder under `root` ([`root`]) that the presets of the plugin `clap_id` at `version` go in,
/// the one the plugin reads (`mxm-preset`'s `factory_dir`).
pub fn folder_for(root: &Path, clap_id: &str, version: &str) -> PathBuf {
    root.join(clap_id).join("factory").join(version)
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
/// their file names: anything else is an order the plugin can't follow. Returns that plugin's id,
/// or `None` for a folder with no presets.
pub fn check_folder(folder: &Path) -> Result<Option<String>> {
    let mut plugin: Option<String> = None;
    let files = preset_files(folder)?;
    for path in &files {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("{} can't be read", path.display()))?;
        let preset: PresetFile = serde_json::from_str(&text)
            .with_context(|| format!("{} isn't a preset", path.display()))?;
        if preset.schema_version != SCHEMA_VERSION {
            bail!(
                "{} is schema version {}, and the plugins read version {SCHEMA_VERSION}",
                path.display(),
                preset.schema_version
            );
        }
        if preset.name.is_empty() {
            bail!("{} has no name", path.display());
        }
        let this = preset.plugin.as_str();
        if this.is_empty() {
            bail!("{} names no plugin", path.display());
        }
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
    Ok(plugin)
}

/// A preset file as `mxm-preset`'s `Preset` reads it, field for field and type for type, so serde
/// refuses here what it refuses there: a missing or repeated field, or one of another type. Its
/// fields are read only to be checked.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct PresetFile {
    schema_version: u32,
    plugin: String,
    name: String,
    /// `mxm-preset`'s `Category` takes any text, an unknown one as *Uncategorised*.
    #[serde(default)]
    category: String,
    #[serde(rename = "_comment", default)]
    comment: String,
    params: std::collections::BTreeMap<String, ValueFile>,
    #[serde(default)]
    state: Option<serde_json::Value>,
}

/// One parameter's value as `mxm-preset`'s `Value` reads it.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ValueFile {
    v: f32,
    #[serde(default)]
    text: String,
}

/// Stages each plugin's presets in the repository's `target/bundled`, where its control map is
/// staged whichever folder Cargo builds in, each replacing what an earlier bundle left there.
/// Returns the folders staged.
pub fn stage(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    let staging = workspace_root.join("target").join("bundled");
    let mut staged = Vec::new();
    for (crate_dir, bundle) in bundle_names(workspace_root)? {
        let source = crate_dir.join(SOURCE);
        let folder = staging.join(staged_name(&bundle));
        remove(&folder)?;
        if !source.is_dir() {
            continue;
        }
        copy_presets(&source, &folder)?;
        staged.push(folder);
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
    fn presets_are_staged_replacing_the_last() {
        let scratch = Scratch::new("staged");
        let root = repository(&scratch);
        let presets = root.join("plugins").join("x-plugin").join(SOURCE);
        std::fs::write(presets.join("a.json"), preset("dk.x", "A")).unwrap();
        std::fs::write(presets.join("b.json"), preset("dk.x", "B")).unwrap();
        std::fs::write(presets.join(ORDER), "b.json\na.json\n").unwrap();
        std::fs::write(presets.join("notes.md"), "not shipped").unwrap();
        let bundled = root.join("target").join("bundled");

        let staged = stage(&root).unwrap();
        assert_eq!(staged, [bundled.join("x.presets")]);
        assert_eq!(
            names_in(&bundled.join("x.presets")),
            ["a.json", "b.json", ORDER]
        );
        assert_eq!(check_folder(&staged[0]).unwrap(), Some("dk.x".to_owned()));

        // A file taken out of the repository is gone from the next bundle.
        std::fs::remove_file(presets.join("a.json")).unwrap();
        std::fs::write(presets.join(ORDER), "b.json\n").unwrap();
        stage(&root).unwrap();
        assert_eq!(names_in(&bundled.join("x.presets")), ["b.json", ORDER]);

        // A plugin without presets has none staged.
        std::fs::remove_dir_all(&presets).unwrap();
        assert!(stage(&root).unwrap().is_empty());
        assert!(!bundled.join("x.presets").exists());
    }

    #[test]
    fn a_plugins_presets_go_in_its_versions_own_folder() {
        let root = Path::new("data-local").join("mxm");
        assert_eq!(
            folder_for(&root, "dk.mxm.mxm-mono-01", "1.2.0"),
            root.join("dk.mxm.mxm-mono-01")
                .join("factory")
                .join("1.2.0")
        );
    }

    #[test]
    fn a_folder_is_one_plugins_presets_in_an_order_naming_only_what_is_there() {
        let scratch = Scratch::new("checked");
        let folder = scratch.0.join("presets");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("a.json"), preset("dk.x", "A")).unwrap();
        std::fs::write(folder.join(ORDER), "a.json\n").unwrap();
        assert_eq!(check_folder(&folder).unwrap(), Some("dk.x".to_owned()));

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
            r#"{"schema_version": 1, "schema_version": 1, "plugin": "dk.x", "name": "A", "params": {}}"#
                .to_owned(),
            file(r#", "params": {"cutoff": {"v": 0.5, "v": 0.6}}"#),
            file(r#", "params": {}, "category": null"#),
            r#"{"schema_version": 1, "plugin": "dk.x", "name": "", "params": {}}"#.to_owned(),
        ];
        for text in &refused {
            std::fs::write(folder.join("a.json"), text).unwrap();
            assert!(check_folder(&folder).is_err(), "{text}");
        }
    }
}
