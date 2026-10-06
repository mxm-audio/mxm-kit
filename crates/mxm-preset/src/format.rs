//! A preset as it appears on disk, and what may be done with one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ErasedParam, Instrument};

/// The schema this build writes and reads.
///
/// **Still 1 with the `category` field**: a file without it reads as *Uncategorised*, and an older
/// build ignores the field, so neither direction needs refusing. A version moves when a file an
/// older build read would be *wrong*, not merely poorer.
pub const SCHEMA_VERSION: u32 = 1;

/// The extension a saved preset takes.
pub const EXTENSION: &str = "json";

/// The name the generated Init preset carries.
pub const INIT_NAME: &str = "Init";

/// What every preset file says about which of its two numbers matters.
const COMMENT: &str = "`v` is the value and is authoritative. `text` is this plugin's own formatting \
                       of it, kept so the file can be read, and is never loaded.";

/// Where a preset came from, and therefore what may be done to it and where it is written.
///
/// Part of the persisted identity because a factory, a user and a bank preset **can share a name** —
/// that is what shadowing is — and Save, Rename and Delete differ between them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Compiled into the plugin. Cannot be written at any price.
    Factory,
    /// A file in the user's own presets directory — the bank called *My presets*.
    User,
    /// A file in a bank's directory beside them, named for the bank. Writable like the user's own.
    Bank(String),
}

impl Origin {
    /// The word shown beside a preset for where it came from.
    pub fn label(&self) -> &str {
        match self {
            Origin::Factory => "Factory",
            Origin::User => "Yours",
            Origin::Bank(name) => name,
        }
    }

    /// Whether presets of this origin can be written.
    pub const fn is_writable(&self) -> bool {
        !matches!(self, Origin::Factory)
    }
}

/// What kind of sound a preset is — one word from a fixed, collection-wide list, so two people's
/// banks agree and a browser's category pane is the same in every instrument.
///
/// **On the preset, not the bank**: a shared bank has basses and leads in it, and somebody looking
/// for a bass wants every bass they own in one list. Adding a word here is a note in the schema's
/// history, not a migration; an unknown word in a file reads as *Uncategorised*.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Bass,
    Lead,
    Pad,
    Keys,
    Pluck,
    Brass,
    Strings,
    Percussion,
    Fx,
    Sequence,
    Drone,
    Template,
    #[default]
    #[serde(other)]
    Uncategorised,
}

impl Category {
    /// Every category, in the order a selector lists them. *Uncategorised* last.
    pub const ALL: [Category; 13] = [
        Category::Bass,
        Category::Lead,
        Category::Pad,
        Category::Keys,
        Category::Pluck,
        Category::Brass,
        Category::Strings,
        Category::Percussion,
        Category::Fx,
        Category::Sequence,
        Category::Drone,
        Category::Template,
        Category::Uncategorised,
    ];

    /// The word a person reads.
    pub const fn label(self) -> &'static str {
        match self {
            Category::Bass => "Bass",
            Category::Lead => "Lead",
            Category::Pad => "Pad",
            Category::Keys => "Keys",
            Category::Pluck => "Pluck",
            Category::Brass => "Brass",
            Category::Strings => "Strings",
            Category::Percussion => "Percussion",
            Category::Fx => "FX",
            Category::Sequence => "Sequence",
            Category::Drone => "Drone",
            Category::Template => "Template",
            Category::Uncategorised => "Uncategorised",
        }
    }
}

/// One parameter's value, and the plugin's own rendering of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Value {
    /// The normalised value. **This is the preset.**
    pub v: f32,
    /// How the plugin formatted `v` when the preset was written. A comment; never read back.
    #[serde(default)]
    pub text: String,
}

/// A preset as it appears on disk.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub schema_version: u32,
    /// The permanent `CLAP_ID`. A preset for one instrument loaded into another is refused.
    pub plugin: String,
    pub name: String,
    /// What kind of sound. Missing in files written before the field existed, which reads as
    /// *Uncategorised*.
    #[serde(default)]
    pub category: Category,
    #[serde(rename = "_comment", default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// Every parameter, keyed by its permanent id. **No sparse overlays** — an overlay's meaning
    /// changes when the init patch is retuned, so a preset written as a diff against Init would
    /// drift the day the defaults move.
    ///
    /// A `BTreeMap` so the file is written in a stable order and diffs stay readable.
    pub params: BTreeMap<String, Value>,
    /// Versioned durable sound content for an opt-in asset instrument. `None` keeps the ordinary
    /// parameter-only format and means a factory recipe preserves currently loaded content.
    #[serde(default)]
    pub state: Option<serde_json::Value>,
}

/// What [`Preset::resolve`] found to write: each parameter's id, the parameter, and the normalised
/// value it takes.
pub type Writes<'a> = Vec<(&'static str, &'a dyn ErasedParam, f32)>;

/// Why a preset could not be used. Every refusal names what was wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    NotAPreset(String),
    WrongSchema { found: u32 },
    WrongPlugin { found: String, expected: String },
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NotAPreset(why) => write!(f, "this is not a preset file: {why}"),
            Refused::WrongSchema { found } => write!(
                f,
                "the preset is schema version {found}, but this build understands {SCHEMA_VERSION}"
            ),
            Refused::WrongPlugin { found, expected } => {
                write!(f, "this preset is for `{found}`, and this is `{expected}`")
            }
        }
    }
}

impl Preset {
    /// Captures the current patch under `name`, as a `category`.
    ///
    /// Parameters explicitly declared sparse by the instrument are omitted when exactly at their
    /// default, or when a compound patch says their current value is irrelevant. Resolution writes
    /// the default back through the instrument's matching missing-parameter bridge.
    pub fn capture(
        name: impl Into<String>,
        category: Category,
        instrument: &dyn Instrument,
    ) -> Self {
        let params = instrument
            .parameters()
            .into_iter()
            .filter(|(id, _)| !instrument.is_instance_setting(id))
            .filter(|(id, _)| !instrument.omit_captured_parameter(id))
            .filter(|(id, param)| {
                !instrument.omit_default_parameter(id)
                    || param.normalised() != param.default_normalised()
            })
            .map(|(id, param)| {
                (
                    id.to_owned(),
                    Value {
                        v: param.normalised(),
                        text: param.text(),
                    },
                )
            })
            .collect();

        Self {
            schema_version: SCHEMA_VERSION,
            plugin: instrument.clap_id().to_owned(),
            name: name.into(),
            category,
            comment: COMMENT.to_owned(),
            params,
            state: instrument.capture_preset_state(),
        }
    }

    /// **Init, built from the parameter defaults rather than from a file.**
    ///
    /// Declared sparse defaults are omitted from the temporary preset and restored by resolution,
    /// exactly as they are for a captured file.
    ///
    /// The init patch stays compiled in so a user cannot delete it — there is no file to delete,
    /// and no second copy of the defaults to drift from the plugin's `params.rs`, whose defaults
    /// `plugins/AGENTS.md` makes normative. Generating it makes the browser's *Init* and the
    /// utility menu's *Init patch* one call rather than two things that agree today.
    pub fn init(instrument: &dyn Instrument) -> Self {
        let entries = instrument
            .parameters()
            .into_iter()
            .filter(|(id, _)| !instrument.is_instance_setting(id))
            .filter(|(id, _)| !instrument.omit_default_parameter(id))
            .map(|(id, param)| {
                (
                    id.to_owned(),
                    Value {
                        v: param.default_normalised(),
                        // The **default's** formatting, not the current value's: this is a preset of
                        // what the parameters default to, not of where they happen to be.
                        text: param.format(param.default_normalised()),
                    },
                )
            })
            .collect();

        Self {
            schema_version: SCHEMA_VERSION,
            plugin: instrument.clap_id().to_owned(),
            name: INIT_NAME.to_owned(),
            category: Category::Template,
            comment: COMMENT.to_owned(),
            params: entries,
            state: instrument.init_preset_state(),
        }
    }

    /// Parses a preset, refusing anything that is not one for the plugin with `clap_id`.
    ///
    /// A refusal is whole: a preset that is for another instrument or another schema has nothing
    /// usable in it. An **unrecognised parameter id** is different and is only reported — see
    /// [`Preset::resolve`].
    pub fn parse(text: &str, clap_id: &str) -> Result<Self, Refused> {
        let preset: Preset =
            serde_json::from_str(text).map_err(|e| Refused::NotAPreset(e.to_string()))?;

        if preset.schema_version != SCHEMA_VERSION {
            return Err(Refused::WrongSchema {
                found: preset.schema_version,
            });
        }
        if preset.plugin != clap_id {
            return Err(Refused::WrongPlugin {
                found: preset.plugin,
                expected: clap_id.to_owned(),
            });
        }
        Ok(preset)
    }

    /// The file's text, ready to write.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// The normalised values to write, in the order the parameters are declared, and what was wrong.
    ///
    /// **Resolved before anything is written.** A preset naming a parameter this build does not have
    /// — an older file, a hand-edit, a typo — is reported and skipped, and a parameter the preset
    /// does not mention keeps its current value rather than jumping to its default. Neither costs
    /// the rest of the preset, and neither is discovered halfway through applying it.
    ///
    /// Returns the writes rather than performing them: the bracketing a host sees is
    /// [`crate::ui::apply_preset`]'s.
    pub fn resolve<'a>(&self, instrument: &'a dyn Instrument) -> (Writes<'a>, Vec<String>) {
        self.resolve_from(instrument, None)
    }

    /// Resolves with trusted library provenance. Only `Some(Factory)` unlocks an instrument's
    /// default-empty factory-preserved set; origin-less callers and writable origins stay strict.
    pub fn resolve_from<'a>(
        &self,
        instrument: &'a dyn Instrument,
        origin: Option<&Origin>,
    ) -> (Writes<'a>, Vec<String>) {
        let mut writes = Vec::new();
        let mut problems = Vec::new();

        let parameters = instrument.parameters();
        let known: Vec<&'static str> = parameters.iter().map(|(id, _)| *id).collect();

        for id in self.params.keys() {
            if !known.contains(&id.as_str()) {
                problems.push(format!("`{id}` is not a parameter of this instrument"));
            }
        }

        // A recipe brings no audio, so it may not write the numbers that describe audio. See
        // `Instrument::is_source_owned`.
        let recipe = self.state.is_none();
        for (id, param) in parameters {
            if instrument.is_instance_setting(id) {
                continue;
            }
            if recipe && instrument.is_source_owned(id) {
                continue;
            }
            let Some(value) = self.params.get(id) else {
                if matches!(origin, Some(Origin::Factory)) && instrument.is_factory_preserved(id) {
                    continue;
                }
                if instrument.default_missing_legacy_parameter(id) {
                    writes.push((id, param, param.default_normalised()));
                } else {
                    problems.push(format!(
                        "the preset does not mention `{id}`, which kept its current value"
                    ));
                }
                continue;
            };
            // A non-finite value has no meaning as a normalised parameter and clamping it would
            // invent one. Skipped and named, like an unknown id.
            if !value.v.is_finite() {
                problems.push(format!("`{id}` has no usable value in the preset"));
                continue;
            }
            writes.push((id, param, value.v.clamp(0.0, 1.0)));
        }

        (writes, problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{CLAP_ID, TestInstrument};

    /// An asset instrument for which `cutoff` describes the loaded source rather than the patch.
    struct Sampled(TestInstrument);

    impl Instrument for Sampled {
        fn clap_id(&self) -> &'static str {
            self.0.clap_id()
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn ErasedParam)> {
            self.0.parameters()
        }
        fn identity(&self) -> &std::sync::RwLock<crate::PresetIdentity> {
            self.0.identity()
        }
        fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
            self.0.factory_files()
        }
        fn is_source_owned(&self, id: &str) -> bool {
            id == "cutoff"
        }
        fn validate_preset_state(&self, _state: Option<&serde_json::Value>) -> Result<(), String> {
            Ok(())
        }
    }

    struct Instanced(TestInstrument);

    impl Instrument for Instanced {
        fn clap_id(&self) -> &'static str {
            self.0.clap_id()
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn ErasedParam)> {
            self.0.parameters()
        }
        fn identity(&self) -> &std::sync::RwLock<crate::PresetIdentity> {
            self.0.identity()
        }
        fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
            self.0.factory_files()
        }
        fn is_instance_setting(&self, id: &str) -> bool {
            id == "sync"
        }
    }

    #[test]
    fn instance_settings_are_absent_from_capture_init_writes_and_missing_reports() {
        let instrument = Instanced(TestInstrument::new());
        let capture = Preset::capture("Sound", Category::Bass, &instrument);
        let init = Preset::init(&instrument);
        assert!(!capture.params.contains_key("sync"));
        assert!(!init.params.contains_key("sync"));

        let mut legacy = capture.clone();
        legacy.params.insert(
            "sync".to_owned(),
            Value {
                v: 1.0,
                text: "On".to_owned(),
            },
        );
        for origin in [None, Some(&Origin::Factory), Some(&Origin::User)] {
            let (writes, problems) = legacy.resolve_from(&instrument, origin);
            assert!(problems.is_empty(), "{problems:?}");
            assert!(writes.iter().all(|(id, _, _)| *id != "sync"));
        }
    }

    /// A parameter-only effect whose factory recipes preserve its live insert/send balance.
    struct FactoryPreserving(TestInstrument);

    impl Instrument for FactoryPreserving {
        fn clap_id(&self) -> &'static str {
            self.0.clap_id()
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn ErasedParam)> {
            self.0.parameters()
        }
        fn identity(&self) -> &std::sync::RwLock<crate::PresetIdentity> {
            self.0.identity()
        }
        fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
            self.0.factory_files()
        }
        fn is_factory_preserved(&self, id: &str) -> bool {
            id == "level"
        }
    }

    /// **A recipe brings no audio, so it may not write the numbers that describe audio.** A factory
    /// sound whose `state` is null preserves whatever the player has loaded; its authored region
    /// and root belong to some other recording, and applying them would crop the loaded one.
    #[test]
    fn a_recipe_leaves_source_owned_parameters_alone_and_a_preset_with_content_writes_them() {
        let instrument = Sampled(TestInstrument::new());
        let mut recipe = Preset::capture("Recipe", Category::Pad, &instrument);
        recipe.state = None;
        recipe.params.get_mut("cutoff").expect("captured").v = 0.1;
        recipe.params.get_mut("level").expect("captured").v = 0.1;
        let (writes, problems) = recipe.resolve(&instrument);
        assert!(problems.is_empty(), "{problems:?}");
        let ids: Vec<&str> = writes.iter().map(|(id, _, _)| *id).collect();
        assert!(
            !ids.contains(&"cutoff"),
            "a recipe cropped the loaded source"
        );
        assert!(ids.contains(&"level"), "a recipe wrote nothing else either");

        let mut carried = recipe.clone();
        carried.state = Some(serde_json::json!({ "audio": "its own" }));
        let ids: Vec<&str> = carried
            .resolve(&instrument)
            .0
            .iter()
            .map(|(id, _, _)| *id)
            .collect();
        assert!(
            ids.contains(&"cutoff"),
            "a preset that brings its own audio must place it"
        );
    }

    #[test]
    fn only_trusted_factory_provenance_excuses_a_factory_preserved_omission() {
        let instrument = FactoryPreserving(TestInstrument::new());
        let mut preset = Preset::capture("Recipe", Category::Fx, &instrument);
        preset.params.remove("level");

        let factory = preset.resolve_from(&instrument, Some(&Origin::Factory));
        assert!(factory.1.is_empty(), "{:?}", factory.1);
        assert!(!factory.0.iter().any(|(id, _, _)| *id == "level"));

        for origin in [
            None,
            Some(&Origin::User),
            Some(&Origin::Bank("Imported".into())),
        ] {
            let (_, problems) = preset.resolve_from(&instrument, origin);
            assert_eq!(problems.len(), 1, "{origin:?}: {problems:?}");
            assert!(problems[0].contains("level"));
        }
    }

    #[test]
    fn a_consumer_that_does_not_opt_in_still_requires_complete_factory_files() {
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Broken", Category::Fx, &instrument);
        preset.params.remove("level");
        let (_, problems) = preset.resolve_from(&instrument, Some(&Origin::Factory));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("level"));
    }

    #[test]
    fn a_captured_preset_round_trips_through_its_file() {
        let instrument = TestInstrument::new();
        let preset = Preset::capture("Captured", Category::Pad, &instrument);
        let back = Preset::parse(&preset.to_json(), CLAP_ID).expect("our own file must parse");
        assert_eq!(back, preset);
        assert_eq!(
            back.category,
            Category::Pad,
            "the category travels with the file"
        );
    }

    #[test]
    fn a_preset_carries_every_parameter_rather_than_a_diff() {
        // No sparse overlays: an overlay's meaning changes when the init patch is retuned.
        let instrument = TestInstrument::new();
        let preset = Preset::capture("All", Category::Uncategorised, &instrument);
        assert_eq!(preset.params.len(), instrument.parameters().len());
    }

    #[test]
    fn a_file_without_a_category_is_uncategorised_and_still_loads() {
        // Every preset written before the field existed. Refusing them would be a migration nobody
        // asked for; an older build reading a newer file ignores the word the same way.
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Old", Category::Lead, &instrument);
        preset.category = Category::Uncategorised;
        let text = preset
            .to_json()
            .replace("\"category\": \"uncategorised\",", "");
        assert!(!text.contains("category"), "the premise: no field at all");
        let back = Preset::parse(&text, CLAP_ID).expect("parses");
        assert_eq!(back.category, Category::Uncategorised);
    }

    #[test]
    fn an_unknown_category_word_reads_as_uncategorised() {
        // A file from a build with a word this one lacks. The sound loads; only the shelf is lost.
        let instrument = TestInstrument::new();
        let text = Preset::capture("Odd", Category::Pad, &instrument)
            .to_json()
            .replace("\"pad\"", "\"theremin\"");
        let back = Preset::parse(&text, CLAP_ID).expect("parses");
        assert_eq!(back.category, Category::Uncategorised);
    }

    #[test]
    fn every_category_has_a_label_and_uncategorised_is_last() {
        assert_eq!(Category::ALL.len(), 13);
        assert_eq!(
            *Category::ALL.last().expect("some"),
            Category::Uncategorised
        );
        for category in Category::ALL {
            assert!(!category.label().is_empty());
        }
    }

    #[test]
    fn a_preset_for_another_instrument_is_refused_by_name() {
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Foreign", Category::Uncategorised, &instrument);
        preset.plugin = "dk.mxm.mxm-mono-01".to_owned();

        let refused = Preset::parse(&preset.to_json(), CLAP_ID).expect_err("it is not ours");
        assert_eq!(
            refused,
            Refused::WrongPlugin {
                found: "dk.mxm.mxm-mono-01".to_owned(),
                expected: CLAP_ID.to_owned(),
            }
        );
        assert!(
            refused.to_string().contains("mxm-mono-01") && refused.to_string().contains(CLAP_ID),
            "the refusal must name both: {refused}"
        );
    }

    #[test]
    fn a_future_schema_is_refused_rather_than_guessed_at() {
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Future", Category::Uncategorised, &instrument);
        preset.schema_version = 99;
        assert_eq!(
            Preset::parse(&preset.to_json(), CLAP_ID),
            Err(Refused::WrongSchema { found: 99 })
        );
    }

    #[test]
    fn the_init_preset_is_the_parameter_defaults() {
        // **The reason Init has no file.** As a factory JSON it would be a second copy of the
        // defaults, free to disagree with `params.rs`; generated, it cannot part company.
        let instrument = TestInstrument::new();
        let init = Preset::init(&instrument);
        let parameters = instrument.parameters();
        assert_eq!(
            init.params.len(),
            parameters.len(),
            "Init must carry every parameter"
        );
        for (id, param) in parameters {
            let value = init
                .params
                .get(id)
                .unwrap_or_else(|| panic!("Init is missing `{id}`"));
            assert_eq!(
                value.v,
                param.default_normalised(),
                "Init's `{id}` is not the parameter's default"
            );
        }
        assert_eq!(init.category, Category::Template);
    }

    #[test]
    fn the_text_field_is_never_read_back() {
        // It is a comment. Somebody who hand-edits `text` and not `v` gets the old sound, and there
        // is deliberately no mismatch detection — checking would mean formatting `v` through a
        // nice-plug API, which is the per-type conversion this format exists to avoid.
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Edited", Category::Uncategorised, &instrument);
        for value in preset.params.values_mut() {
            value.text = "nonsense".to_owned();
        }

        let (writes, problems) = preset.resolve(&instrument);
        assert!(problems.is_empty(), "{problems:?}");
        for (id, _, v) in writes {
            assert_eq!(
                v, preset.params[id].v,
                "`{id}` must take the number, not the comment"
            );
        }
    }

    #[test]
    fn an_unknown_parameter_is_reported_and_costs_nothing_else() {
        // One typo in a hand-edited file must not cost the other parameters.
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Typo", Category::Uncategorised, &instrument);
        preset.params.insert(
            "cutof".to_owned(),
            Value {
                v: 0.5,
                text: String::new(),
            },
        );

        let (writes, problems) = preset.resolve(&instrument);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("cutof"), "{problems:?}");
        assert_eq!(
            writes.len(),
            instrument.parameters().len(),
            "every real parameter still applies"
        );
    }

    #[test]
    fn a_missing_parameter_keeps_its_current_value_and_says_so() {
        // Not its *default*: a preset that silently reset a parameter it forgot to mention would be
        // an overlay with the opposite sign.
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Partial", Category::Uncategorised, &instrument);
        preset.params.remove("cutoff");

        let (writes, problems) = preset.resolve(&instrument);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("cutoff"), "{problems:?}");
        assert!(
            !writes.iter().any(|(id, _, _)| *id == "cutoff"),
            "it must not be written at all"
        );
    }

    #[test]
    fn a_non_finite_value_is_skipped_rather_than_clamped() {
        // Clamping would invent a value. NaN has no meaning as a normalised parameter.
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Broken", Category::Uncategorised, &instrument);
        preset.params.get_mut("cutoff").expect("cutoff").v = f32::NAN;

        let (writes, problems) = preset.resolve(&instrument);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(!writes.iter().any(|(id, _, _)| *id == "cutoff"));
    }
}
