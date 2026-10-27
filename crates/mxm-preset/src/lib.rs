//! The collection's preset system — one crate for every instrument and every effect.
//!
//! Five instruments carried this as five verbatim copies of a `preset.rs` and of the editor's
//! preset controls, deliberately, as the evidence an extraction would be made from. Banks and
//! categories touch every line of both, and doing that five times over is five bugs; so this is
//! that extraction (`plans/plan-preset-banks.md` in the private archive, phase 0). The rules live
//! in `plugins/AGENTS.md`, *A preset is parameter values* (in full, this repository's
//! `docs/plugin-conventions.md`), and are not restated line by line here.
//!
//! What a plugin supplies is an [`Instrument`]: its permanent id, its parameters in declaration
//! order, where the loaded preset's identity is persisted, and its factory files, installed beside it.
//! Everything else — the format, the library on disk, favourites, the three-state identity, the
//! app-bar controls — is here, and identical for a synth with eighty parameters and an effect with
//! four.
//!
//! The two decisions that shape everything:
//!
//! - **A preset is parameter values, not a CLAP state blob.** Applying one is bracketed parameter
//!   writes a host can see. An opt-in asset instrument may fill the format's reserved `state` with
//!   versioned durable sound content; wrapper state and opaque plugin state still do not belong.
//! - **Two numbers per parameter, and only `v` is read.** `v` is the normalised value and is
//!   authoritative; `text` is the plugin's own formatting, kept so the file can be read, and never
//!   loaded. One `set_parameter_normalized` serves a float, an enum and a boolean with no per-type
//!   conversion, and there is deliberately no mismatch detection — checking would mean formatting
//!   `v` back through a nice-plug API, which is the conversion the format exists to avoid.

pub mod clap;
pub mod format;
pub mod identity;
pub mod library;
pub mod transaction;
pub mod ui;

use mxm_params::ErasedParam;
use nice_plug::prelude::ParamSetter;

pub use format::{Category, EXTENSION, INIT_NAME, Origin, Preset, Refused, SCHEMA_VERSION, Value};
pub use identity::{
    Loaded, LoadedPreset, PresetIdentity, add_switches_off, loaded, mark_loaded, mark_none,
    snapshot,
};
pub use library::{
    BANK_EXTENSION, BANK_FILE, Bank, BankFile, BankInfo, Entry, Found, ImportRefused, Library,
    ORDER_FILE, banks_root, dir_name, factory, factory_dir_of, favourite_key, file_name,
    installed_factory_dir, is_favourite, open_folder, read_factory_dir, read_favourites, user_root,
    write_favourites,
};
pub use transaction::{
    DeferredBeginError, DeferredContent, DeferredPresetTransaction, DeferredRequest,
    DeferredRequestId, DeferredStatus,
};
pub use ui::{DeferredPresetUi, DeferredPresetUiRequest, PresetUi};

use std::sync::RwLock;

/// What the preset system needs from a plugin, and all it needs.
///
/// Implemented by the plugin's `Params` struct: it has the parameters, and it is where nice-plug's
/// `#[persist]` keeps the loaded preset's identity with the patch.
pub trait Instrument {
    /// The permanent `CLAP_ID`. A preset for one plugin loaded into another is refused by it.
    fn clap_id(&self) -> &'static str;

    /// Every parameter with its permanent id, **in declaration order** — the order a preset file
    /// is written in and the order Init writes them.
    fn parameters(&self) -> Vec<(&'static str, &dyn ErasedParam)>;

    /// Optional monotonic revision for one parameter's unmodulated base value.
    ///
    /// Deferred consumers return `Some`: publication then treats any revision advance after begin
    /// as a later edit, even when the value was moved away and back. Existing synchronous consumers
    /// may retain the `None` default; the deferred seam falls back to value comparison for them.
    fn parameter_edit_revision(&self, _id: &str) -> Option<u64> {
        None
    }

    /// Whether an older preset that omits this newly added parameter should explicitly write its
    /// default instead of preserving the current value and reporting an incomplete file.
    ///
    /// This is an opt-in compatibility bridge for a permanent parameter added after shipped preset
    /// content. New files still serialize every parameter, and unknown ids remain errors.
    fn default_missing_legacy_parameter(&self, _id: &str) -> bool {
        false
    }

    /// Whether capture may omit this parameter when it is exactly at its default.
    ///
    /// This is for sparse parameter families whose defaults carry no information in a preset.
    /// Implementations must also opt the same id into
    /// [`Self::default_missing_legacy_parameter`] so resolving the sparse file actively restores
    /// the default instead of preserving live state.
    fn omit_default_parameter(&self, _id: &str) -> bool {
        false
    }

    /// Whether capture should omit this parameter because another piece of the patch makes its
    /// current value irrelevant.
    ///
    /// This is for sparse compound state such as an absent modulation route's dormant amount. The
    /// id must also opt into [`Self::default_missing_legacy_parameter`], because resolving the
    /// omission must actively restore its default rather than preserve unrelated live state.
    fn omit_captured_parameter(&self, _id: &str) -> bool {
        false
    }

    /// Where the loaded preset's identity is persisted with the patch.
    fn identity(&self) -> &RwLock<PresetIdentity>;

    /// The factory set, `(file name, json)`: the files installed with the plugin
    /// ([`installed_factory_dir`]), read when asked, in their folder's order ([`read_factory_dir`]).
    /// Init is not among them: it is generated from the parameter defaults. A plugin's tests read
    /// its repository's folder through [`read_factory_dir`] instead.
    fn factory_files(&self) -> Vec<(String, String)> {
        installed_factory_dir()
            .map(|dir| read_factory_dir(&dir))
            .unwrap_or_default()
    }

    /// Whether a factory recipe deliberately leaves this patch parameter at its live
    /// value. The default-empty opt-in is provenance-sensitive: only an apply path carrying trusted
    /// [`Origin::Factory`] may consult it. User/bank files and origin-less direct calls remain
    /// complete presets and report an omission.
    fn is_factory_preserved(&self, _id: &str) -> bool {
        false
    }

    /// Optional durable sound content belonging to Init. Ordinary parameter-only instruments and
    /// source-preserving Init implementations return `None`; an authored-model effect may return
    /// its versioned default model so both the browser's generated Init and the Init button restore
    /// the complete sound rather than parameters alone.
    fn init_preset_state(&self) -> Option<serde_json::Value> {
        None
    }

    /// Optional durable sound content captured in a user preset. Ordinary synths return `None`;
    /// asset instruments return a versioned JSON value, never a wrapper/host state blob.
    fn capture_preset_state(&self) -> Option<serde_json::Value> {
        None
    }

    /// Cheap deterministic identity for the current durable sound content. This is compared with
    /// the loaded baseline without cloning a sample payload every GUI frame.
    fn preset_state_fingerprint(&self) -> Option<u64> {
        None
    }

    /// Validate an optional durable payload before any parameter gesture is emitted. Existing
    /// parameter-only consumers reject an unexpected state field by default.
    fn validate_preset_state(&self, state: Option<&serde_json::Value>) -> Result<(), String> {
        if state.is_none() {
            Ok(())
        } else {
            Err("this plugin does not accept durable preset content".to_owned())
        }
    }

    /// Apply a payload after successful preflight but before parameter gestures. `None` means a
    /// factory recipe that deliberately preserves currently loaded content.
    ///
    /// **An asset instrument prepares here and becomes audible in [`Self::commit_preset_state`].**
    /// Preparing and publishing in one step means the new content can be rendered for a block
    /// through the *old* patch, because the parameter gestures have not been written yet. Doing
    /// the expensive half here keeps the failure atomic — a rejected payload has still emitted no
    /// gesture — and leaves the audible step until the patch around it is in place.
    fn apply_preset_state(&self, state: Option<&serde_json::Value>) -> Result<(), String> {
        self.validate_preset_state(state)
    }

    /// Make whatever [`Self::apply_preset_state`] prepared audible, after the parameter gestures.
    ///
    /// Called once per successful application, including for a recipe that carried no content of
    /// its own, so it must be idempotent and cheap. Parameter-only instruments need nothing here.
    fn commit_preset_state(&self) {}

    /// A preset load or Init is replacing the patch, whatever it carried: the moment to drop what
    /// the instrument keeps outside the patch but owes to it. MXM Drum Machine's typed slot names
    /// go back to the ones it makes up, because the drums changed. Content the preset itself
    /// carries is [`Self::apply_preset_state`]'s. The default does nothing.
    ///
    /// Called once for every successful load ([`ui::apply_preset_checked`] and everything that goes
    /// through it: the preset bar, the browser, stepping), every Init ([`ui::init_patch`]) and every
    /// deferred transaction the audio callback acknowledged
    /// ([`DeferredPresetTransaction::acknowledge`]); never for a refused one.
    ///
    /// **It must not read the patch.** On a synchronous load or Init it runs as the gestures are
    /// emitted, which a CLAP host may apply in a later process or flush, so the parameters may still
    /// hold the patch being replaced; and a load installs its identity after this returns. A
    /// deferred transaction calls it from `acknowledge`, after the callback confirmed publication
    /// and after the identity is installed. Either way, act on what the instrument holds itself,
    /// never on live parameter values or [`Self::identity`].
    fn patch_replaced(&self) {}

    /// Whether this parameter describes the **loaded source** rather than the patch around it.
    ///
    /// A root note and a region are measurements of one particular recording. Writing them from
    /// somewhere that does not bring that recording crops or detunes whatever the player has
    /// loaded, using numbers authored against a different sound. So two callers consult this:
    /// Init leaves them alone, and [`Preset::resolve`] skips them for a preset that carries no
    /// durable content of its own — a factory recipe. A user preset embeds its audio and therefore
    /// does write them.
    ///
    /// Only an asset instrument has any; everything else says no to every id.
    fn is_source_owned(&self, _id: &str) -> bool {
        false
    }

    /// Whether this parameter configures the plugin instance rather than the preset's sound.
    ///
    /// Instance settings remain ordinary host/project-state parameters, but preset capture,
    /// application, Init, completeness reporting and dirty comparison all exclude them. Output
    /// routing and external MIDI channel assignment are the canonical examples: auditioning a
    /// sound must not rewire the DAW or controller mapping around it.
    fn is_instance_setting(&self, _id: &str) -> bool {
        false
    }
}

/// Every stored `text` in `instrument`'s factory files that is not its own formatting of the `v`
/// beside it, one line each naming the file, the id, what is stored and what the plugin prints —
/// empty when every file is current.
///
/// **`text` is never loaded, which is exactly why nothing else notices it going stale**: a changed
/// reading leaves every file loading the same sound while telling whoever reads it something else.
/// `mxm-mono-00` shipped 134 route readings from before a scale entered them (`+5.83 oct` where it
/// prints `+3.50 oct`, `plans/plan-sibling-audit.md` D17). Every plugin's preset tests assert this
/// is empty. It compares text in a test and never at load: the format's *no mismatch detection* is
/// about loading. A file that does not parse is reported; an id the instrument does not have is
/// left to the plugin's own completeness test.
///
/// **Each file is checked in its own state.** A reading may depend on another parameter —
/// `mxm-mono-08`'s Mod frequency reads in the range its Mod high switch selects, through the
/// switch's callback — so every value the file stores is applied first, callbacks and all, onto the
/// instrument's own values, and each text is compared with what the instrument prints in that
/// state. The instrument's values are put back afterwards. [`refresh_texts`] is the same reading
/// for a generator.
pub fn stale_factory_texts(instrument: &dyn Instrument) -> Vec<String> {
    let mut stale = Vec::new();
    for (name, json) in instrument.factory_files() {
        let preset = match Preset::parse(&json, instrument.clap_id()) {
            Ok(preset) => preset,
            Err(refused) => {
                stale.push(format!("{name:?} does not parse: {refused:?}"));
                continue;
            }
        };
        for (id, printed) in texts_in_state(instrument, &preset) {
            let stored = &preset.params[&id].text;
            if printed != *stored {
                stale.push(format!(
                    "{name:?} `{id}` stores {stored:?}, the plugin prints {printed:?}"
                ));
            }
        }
    }
    stale
}

/// Rewrites every `text` in `preset` as `instrument` prints its `v` **in the preset's own state** —
/// what a factory set's generator stores, so a reading that depends on another parameter (see
/// [`stale_factory_texts`]) is written in the range the preset selects, not the default's. An id
/// the instrument does not have keeps its text.
pub fn refresh_texts(instrument: &dyn Instrument, preset: &mut Preset) {
    for (id, text) in texts_in_state(instrument, preset) {
        if let Some(value) = preset.params.get_mut(&id) {
            value.text = text;
        }
    }
}

/// Each id `preset` stores that `instrument` has, with what the instrument prints for its `v` once
/// every stored value has been applied onto the instrument's own, callbacks and all. The
/// instrument's values are put back before it returns.
fn texts_in_state(instrument: &dyn Instrument, preset: &Preset) -> Vec<(String, String)> {
    let parameters = instrument.parameters();
    let host = Applying;
    let setter = ParamSetter::new(&host);
    let before: Vec<f32> = parameters.iter().map(|(_, p)| p.normalised()).collect();
    let stored: Vec<_> = preset
        .params
        .iter()
        .filter_map(|(id, value)| {
            let (_, param) = parameters.iter().find(|(bound, _)| bound == id)?;
            Some((id, *param, value.v))
        })
        .collect();
    for (_, param, v) in &stored {
        param.set(&setter, *v);
    }
    let texts = stored
        .iter()
        .map(|(id, param, v)| ((*id).clone(), param.format(*v)))
        .collect();
    for ((_, param), value) in parameters.iter().zip(before) {
        param.set(&setter, value);
    }
    texts
}

/// A host that applies each write as it arrives, as the CLAP wrapper does by the next process call,
/// so a parameter's callbacks run: the state a dependent reading formats against.
struct Applying;

impl nice_plug::context::gui::GuiContextInner for Applying {
    // A test or offline host has no host to ask for a restart (nice-plug 0.4).
    fn request_restart(&self) {}
    fn plugin_api(&self) -> nice_plug::prelude::PluginApi {
        nice_plug::prelude::PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(
        &self,
        param: nice_plug::params::internals::ParamPtr,
        normalized: f32,
    ) {
        // SAFETY: `stale_factory_texts` writes only through parameters `instrument` owns, which
        // outlive the call; this is the setter the wrapper itself applies a host's value with.
        unsafe {
            param._internal_set_normalized_value(normalized);
        }
    }
    unsafe fn raw_end_set_parameter(&self, _param: nice_plug::params::internals::ParamPtr) {}
    fn get_state(&self) -> nice_plug::prelude::PluginState {
        nice_plug::prelude::PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _state: nice_plug::prelude::PluginState) {}
}

#[cfg(test)]
mod stale_text_tests {
    use std::sync::RwLock;

    use super::{Instrument, Preset, PresetIdentity, refresh_texts, stale_factory_texts};
    use crate::testing::{CLAP_ID, TestInstrument};
    use mxm_params::ErasedParam;

    /// The test instrument with factory files of the test's own making.
    struct WithFiles(TestInstrument, &'static [(&'static str, &'static str)]);

    impl Instrument for WithFiles {
        fn clap_id(&self) -> &'static str {
            CLAP_ID
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn ErasedParam)> {
            self.0.parameters()
        }
        fn identity(&self) -> &RwLock<PresetIdentity> {
            self.0.identity()
        }
        fn factory_files(&self) -> Vec<(String, String)> {
            crate::owned(self.1)
        }
    }

    /// One file storing each parameter's own formatting of its value, with `cutoff`'s text replaced
    /// by `cutoff_text` when given.
    fn files(cutoff_text: Option<&str>) -> &'static [(&'static str, &'static str)] {
        let instrument = TestInstrument::new();
        let params: Vec<String> = instrument
            .parameters()
            .into_iter()
            .map(|(id, param)| {
                let v = 0.25;
                let text = match (id, cutoff_text) {
                    ("cutoff", Some(text)) => text.to_owned(),
                    _ => param.format(v),
                };
                format!("{id:?}: {{ \"v\": {v}, \"text\": {text:?} }}")
            })
            .collect();
        let json = format!(
            "{{ \"schema_version\": 1, \"plugin\": {CLAP_ID:?}, \"name\": \"Probe\", \"params\": {{ {} }} }}",
            params.join(", ")
        );
        Box::leak(Box::new([("Probe", &*Box::leak(json.into_boxed_str()))]))
    }

    #[test]
    fn a_file_storing_the_plugins_own_text_is_current() {
        let stale = stale_factory_texts(&WithFiles(TestInstrument::new(), files(None)));
        assert!(stale.is_empty(), "{stale:?}");
    }

    #[test]
    fn a_text_the_plugin_no_longer_prints_is_reported_by_file_and_id() {
        let stale = stale_factory_texts(&WithFiles(TestInstrument::new(), files(Some("stale"))));
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(
            stale[0].starts_with("\"Probe\" `cutoff` stores \"stale\""),
            "{stale:?}"
        );
    }

    /// A reading that depends on a switch, as `mxm-mono-08`'s Mod frequency depends on Mod high:
    /// `level` reads tenfold while `high` is on, through `high`'s callback.
    struct Dependent {
        high: nice_plug::prelude::BoolParam,
        level: nice_plug::prelude::FloatParam,
        preset: RwLock<PresetIdentity>,
        files: &'static [(&'static str, &'static str)],
    }

    impl Dependent {
        fn new(files: &'static [(&'static str, &'static str)]) -> Self {
            use nice_plug::prelude::{BoolParam, FloatParam, FloatRange};
            use std::sync::Arc;
            use std::sync::atomic::{AtomicBool, Ordering};
            let flag = Arc::new(AtomicBool::new(false));
            let reads = flag.clone();
            Self {
                high: BoolParam::new("High", false)
                    .with_callback(Arc::new(move |on| flag.store(on, Ordering::Relaxed))),
                level: FloatParam::new("Level", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                    .with_value_to_string(Arc::new(move |v| {
                        let scale = if reads.load(Ordering::Relaxed) {
                            10.0
                        } else {
                            1.0
                        };
                        format!("{:.2}", v * scale)
                    })),
                preset: RwLock::new(PresetIdentity::none()),
                files,
            }
        }
    }

    impl Instrument for Dependent {
        fn clap_id(&self) -> &'static str {
            CLAP_ID
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn ErasedParam)> {
            vec![("high", &self.high), ("level", &self.level)]
        }
        fn identity(&self) -> &RwLock<PresetIdentity> {
            &self.preset
        }
        fn factory_files(&self) -> Vec<(String, String)> {
            crate::owned(self.files)
        }
    }

    /// One file with `high` on and `level` at 0.25, storing `level_text` for it.
    fn dependent_file(level_text: &str) -> &'static [(&'static str, &'static str)] {
        let json = format!(
            "{{ \"schema_version\": 1, \"plugin\": {CLAP_ID:?}, \"name\": \"High\", \"params\": {{ \"high\": {{ \"v\": 1.0, \"text\": \"On\" }}, \"level\": {{ \"v\": 0.25, \"text\": {level_text:?} }} }} }}"
        );
        Box::leak(Box::new([("High", &*Box::leak(json.into_boxed_str()))]))
    }

    /// **A file is read in its own state**: with its switch on, a dependent reading prints in the
    /// switch's range, so the low-range text a default instrument would print is stale.
    #[test]
    fn a_reading_that_depends_on_another_parameter_is_checked_in_the_files_state() {
        let current = stale_factory_texts(&Dependent::new(dependent_file("2.50")));
        assert!(current.is_empty(), "{current:?}");
        let stale = stale_factory_texts(&Dependent::new(dependent_file("0.25")));
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(stale[0].contains("`level` stores \"0.25\""), "{stale:?}");
    }

    /// **A generator writes each text in its preset's state**: refreshing the stale file's texts
    /// makes it current.
    #[test]
    fn refreshed_texts_are_written_in_the_presets_own_state() {
        let instrument = Dependent::new(dependent_file("0.25"));
        let mut preset =
            Preset::parse(&instrument.factory_files()[0].1, CLAP_ID).expect("the file parses");
        refresh_texts(&instrument, &mut preset);
        assert_eq!(preset.params["level"].text, "2.50");
        assert_eq!(preset.params["high"].text, "On");
        assert_eq!(
            instrument.level.format(0.25),
            "0.25",
            "the instrument is put back"
        );
    }

    /// The instrument's own values are put back once the files have been read.
    #[test]
    fn the_instruments_values_are_restored_after_the_check() {
        let instrument = Dependent::new(dependent_file("2.50"));
        let _ = stale_factory_texts(&instrument);
        assert_eq!(instrument.high.normalised(), 0.0);
        assert_eq!(instrument.level.normalised(), 0.5);
        assert_eq!(instrument.level.format(0.25), "0.25");
    }

    #[test]
    fn a_file_that_does_not_parse_is_reported_rather_than_skipped() {
        let files: &'static [(&str, &str)] = &[("Broken", "{ not json")];
        let stale = stale_factory_texts(&WithFiles(TestInstrument::new(), files));
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(
            stale[0].starts_with("\"Broken\" does not parse"),
            "{stale:?}"
        );
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! A three-parameter instrument for the tests here: enough to hold every rule and nothing an
    //! instrument would have to be consulted about.

    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use nice_plug::prelude::{BoolParam, FloatParam, FloatRange, Param};

    use crate::{Instrument, PresetIdentity};
    use mxm_params::ErasedParam;

    pub(crate) const CLAP_ID: &str = "dk.mxm.test";

    pub(crate) struct TestInstrument {
        pub cutoff: FloatParam,
        pub level: FloatParam,
        pub sync: BoolParam,
        observed_bits: [AtomicU64; 3],
        revisions: [AtomicU64; 3],
        preset: RwLock<PresetIdentity>,
        /// How many times [`Instrument::patch_replaced`] was called.
        pub replaced: AtomicUsize,
    }

    impl TestInstrument {
        pub(crate) fn new() -> Self {
            Self {
                cutoff: FloatParam::new("Cutoff", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
                level: FloatParam::new("Level", 0.8, FloatRange::Linear { min: 0.0, max: 1.0 }),
                sync: BoolParam::new("Sync", false),
                observed_bits: std::array::from_fn(|_| AtomicU64::new(0)),
                revisions: std::array::from_fn(|_| AtomicU64::new(0)),
                preset: RwLock::new(PresetIdentity::none()),
                replaced: AtomicUsize::new(0),
            }
        }

        pub(crate) fn replaced(&self) -> usize {
            self.replaced.load(Ordering::Acquire)
        }

        pub(crate) fn observe_revision(&self, id: &str) -> Option<u64> {
            const MARKER: u64 = 1 << 63;
            let (index, normalised) = match id {
                "cutoff" => (0, self.cutoff.unmodulated_normalized_value()),
                "level" => (1, self.level.unmodulated_normalized_value()),
                "sync" => (2, self.sync.unmodulated_normalized_value()),
                _ => return None,
            };
            let encoded = MARKER | u64::from(normalised.to_bits());
            let previous = self.observed_bits[index].swap(encoded, Ordering::AcqRel);
            if previous != 0 && previous != encoded {
                self.revisions[index].fetch_add(1, Ordering::AcqRel);
            }
            Some(self.revisions[index].load(Ordering::Acquire))
        }
    }

    impl Instrument for TestInstrument {
        fn clap_id(&self) -> &'static str {
            CLAP_ID
        }

        fn parameters(&self) -> Vec<(&'static str, &dyn ErasedParam)> {
            vec![
                ("cutoff", &self.cutoff),
                ("level", &self.level),
                ("sync", &self.sync),
            ]
        }

        fn parameter_edit_revision(&self, id: &str) -> Option<u64> {
            self.observe_revision(id)
        }

        fn identity(&self) -> &RwLock<PresetIdentity> {
            &self.preset
        }

        fn factory_files(&self) -> Vec<(String, String)> {
            crate::owned(FACTORY)
        }

        fn patch_replaced(&self) {
            self.replaced.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub(crate) const FACTORY: &[(&str, &str)] = &[("Sub bass", SUB_BASS), ("Brass", BRASS)];

    const SUB_BASS: &str = r#"{
  "schema_version": 1,
  "plugin": "dk.mxm.test",
  "name": "Sub bass",
  "category": "bass",
  "params": {
    "cutoff": { "v": 0.3, "text": "0.3" },
    "level": { "v": 0.9, "text": "0.9" },
    "sync": { "v": 0.0, "text": "Off" }
  }
}"#;

    const BRASS: &str = r#"{
  "schema_version": 1,
  "plugin": "dk.mxm.test",
  "name": "Brass",
  "category": "brass",
  "params": {
    "cutoff": { "v": 0.7, "text": "0.7" },
    "level": { "v": 0.8, "text": "0.8" },
    "sync": { "v": 1.0, "text": "On" }
  }
}"#;
}

/// Static `(name, json)` pairs, owned, as [`Instrument::factory_files`] hands them out: for the
/// tests' instruments.
#[cfg(test)]
pub(crate) fn owned(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(name, json)| ((*name).to_owned(), (*json).to_owned()))
        .collect()
}
