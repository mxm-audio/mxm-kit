//! The collection's preset system — one crate for every instrument and every effect.
//!
//! Five instruments carried this as five verbatim copies of a `preset.rs` and of the editor's
//! preset controls, deliberately, as the evidence an extraction would be made from. Banks and
//! categories touch every line of both, and doing that five times over is five bugs; so this is
//! that extraction (`plans/plan-preset-banks.md`, phase 0). The rules live in `plugins/AGENTS.md`,
//! *A preset is parameter values*, and are not restated line by line here.
//!
//! What a plugin supplies is an [`Instrument`]: its permanent id, its parameters in declaration
//! order, where the loaded preset's identity is persisted, and its compiled-in factory files.
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

pub mod binding;
pub mod erased;
pub mod format;
pub mod identity;
pub mod library;
pub mod transaction;
pub mod ui;

pub use erased::{ErasedParam, StepLaw};
pub use format::{Category, EXTENSION, INIT_NAME, Origin, Preset, Refused, SCHEMA_VERSION, Value};
pub use identity::{
    Loaded, LoadedPreset, PresetIdentity, add_switches_off, loaded, mark_loaded, mark_none,
    snapshot,
};
pub use library::{
    BANK_EXTENSION, BANK_FILE, Bank, BankFile, BankInfo, Entry, Found, ImportRefused, Library,
    banks_root, dir_name, factory, favourite_key, file_name, is_favourite, open_folder,
    read_favourites, user_root, write_favourites,
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

    /// The factory set, compiled in with `include_str!`, as `(name, json)`. Init is not among
    /// them: it is generated from the parameter defaults.
    fn factory_files(&self) -> &'static [(&'static str, &'static str)];

    /// Whether a compiled-in factory recipe deliberately leaves this patch parameter at its live
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

#[cfg(test)]
pub(crate) mod testing {
    //! A three-parameter instrument for the tests here: enough to hold every rule and nothing an
    //! instrument would have to be consulted about.

    use std::sync::RwLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use nice_plug::prelude::{BoolParam, FloatParam, FloatRange, Param};

    use crate::{ErasedParam, Instrument, PresetIdentity};

    pub(crate) const CLAP_ID: &str = "dk.mxm.test";

    pub(crate) struct TestInstrument {
        pub cutoff: FloatParam,
        pub level: FloatParam,
        pub sync: BoolParam,
        observed_bits: [AtomicU64; 3],
        revisions: [AtomicU64; 3],
        preset: RwLock<PresetIdentity>,
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
            }
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

        fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
            FACTORY
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
