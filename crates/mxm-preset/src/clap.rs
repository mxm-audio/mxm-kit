//! A plugin's presets for a host, through nice-plug's preset capability
//! (`ops/plans/plan-external-presets.md` in the workspace): this format's metadata for a host's
//! index, and a loader that applies a file exactly as the browser applies an entry.
//!
//! A plugin opts in with two lines in its `ClapPlugin`: `const CLAP_PRESETS = Some(PRESETS)`, and a
//! `preset_loader` that returns a [`Loader`] over its parameters.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nice_plug::prelude::{ClapPresetLoader, ClapPresets, ParamSetter, PresetMetadata};
use nice_plug::wrapper::clap::presets::in_folder;

use crate::identity::mark_loaded_with_baseline;
use crate::ui::apply_preset_checked_from_with_baseline;
use crate::{EXTENSION, Instrument, Origin, Preset, Refused, factory_dir};

/// A plugin's `ClapPlugin::CLAP_PRESETS`: this format's files, in the factory folder installed
/// for its version.
pub const PRESETS: ClapPresets = ClapPresets {
    file_extension: EXTENSION,
    factory_folder: factory_dir,
    metadata,
};

/// A preset file's name and category for the plugin `clap_id`; nothing for another plugin's file.
fn metadata(path: &Path, clap_id: &str) -> Result<Option<PresetMetadata>, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    match Preset::parse(&text, clap_id) {
        Ok(preset) => Ok(Some(PresetMetadata {
            name: preset.name,
            category: Some(preset.category.label().to_owned()),
        })),
        Err(Refused::WrongPlugin { .. }) => Ok(None),
        Err(refused) => Err(refused.to_string()),
    }
}

/// Loads a preset file into an instrument for a host: a plugin's `preset_loader`.
pub struct Loader<I>(pub Arc<I>);

impl<I: Instrument + Send + Sync + 'static> ClapPresetLoader for Loader<I> {
    fn load(&self, path: &Path, setter: &ParamSetter) -> Result<(), String> {
        load_file(&*self.0, path, setter)
    }
}

/// Loads the preset file at `path` as the browser loads an entry: through the same checked
/// apply, then marked loaded with the values it set. It is factory content only when it lies in
/// the factory folder installed for the instrument's version, never because it says so; anywhere
/// else it loads as the user's own. A file the instrument refuses, or a durable payload it
/// rejects, isn't loaded, and says why; parameters a loaded file leaves out or doesn't know keep
/// their values, as in the browser.
pub fn load_file(
    instrument: &dyn Instrument,
    path: &Path,
    setter: &ParamSetter<'_>,
) -> Result<(), String> {
    let factory = factory_dir(instrument.clap_id(), instrument.version());
    load_file_with_factory(instrument, path, setter, factory)
}

/// [`load_file`], `factory` being the factory folder.
fn load_file_with_factory(
    instrument: &dyn Instrument,
    path: &Path,
    setter: &ParamSetter<'_>,
    factory: Option<PathBuf>,
) -> Result<(), String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let preset =
        Preset::parse(&text, instrument.clap_id()).map_err(|refused| refused.to_string())?;
    let origin = match factory {
        Some(folder) if in_folder(path, &folder) => Origin::Factory,
        _ => Origin::User,
    };
    let (applied, problems, baseline) =
        apply_preset_checked_from_with_baseline(instrument, setter, &preset, Some(&origin));
    match (applied, baseline) {
        (true, Some(baseline)) => {
            mark_loaded_with_baseline(instrument, &preset.name, origin, baseline);
            Ok(())
        }
        _ => Err(problems.join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::next_temp_serial;
    use crate::loaded;
    use crate::testing::{CLAP_ID, FACTORY, TestInstrument};
    use mxm_plugin_test::NoHost;

    fn file(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mxm-preset-clap-{}-{}",
            std::process::id(),
            next_temp_serial()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn a_file_is_indexed_by_its_name_and_category_and_another_plugins_is_passed_over() {
        let (_, json) = FACTORY[0];
        let path = file("sub-bass.json", json);
        let preset = Preset::parse(json, CLAP_ID).unwrap();
        assert_eq!(
            metadata(&path, CLAP_ID),
            Ok(Some(PresetMetadata {
                name: preset.name,
                category: Some(preset.category.label().to_owned()),
            }))
        );
        assert_eq!(metadata(&path, "dk.mxm.another"), Ok(None));
        let broken = file("broken.json", "{ not a preset");
        assert!(metadata(&broken, CLAP_ID).is_err());
    }

    /// A file in the factory folder loads as factory content and one anywhere else as the user's
    /// own, each marked loaded by its name; one for another plugin isn't loaded, and says why.
    #[test]
    fn a_file_loads_as_the_browser_loads_an_entry() {
        let instrument = TestInstrument::new();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let (_, json) = FACTORY[1];
        let path = file("brass.json", json);
        let factory = path.parent().map(Path::to_path_buf);
        let elsewhere = file("brass.json", json);
        let name = Preset::parse(json, CLAP_ID).unwrap().name;

        assert_eq!(
            load_file_with_factory(&instrument, &path, &setter, factory.clone()),
            Ok(())
        );
        assert_eq!(loaded(&instrument).name(), Some(name.as_str()));
        assert_eq!(loaded(&instrument).origin(), Some(&Origin::Factory));

        assert_eq!(
            load_file_with_factory(&instrument, &elsewhere, &setter, factory.clone()),
            Ok(())
        );
        assert_eq!(loaded(&instrument).name(), Some(name.as_str()));
        assert_eq!(loaded(&instrument).origin(), Some(&Origin::User));

        let other = file("other.json", &json.replace(CLAP_ID, "dk.mxm.another"));
        let refused = load_file_with_factory(&instrument, &other, &setter, factory).unwrap_err();
        assert!(refused.contains("dk.mxm.another"), "{refused}");
    }
}
