//! Which preset is loaded, and whether the patch has moved since — three states from two fields.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Instrument, Origin};

/// What is loaded, and the values it was loaded with.
///
/// **Persisted with the patch, not beside it**: the plugin's `Params` carries this under nice-plug's
/// `#[persist]`, with its own version number because nice-plug's state version is `Plugin::VERSION`,
/// which moves for unrelated reasons.
///
/// **Set together and cleared together**, which is what makes three states out of two fields:
/// *no preset* when `loaded` is `None`, the preset's name when the current parameters and optional
/// durable-state fingerprint match their baselines, and the name marked when either differs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PresetIdentity {
    /// Its own number. `Plugin::VERSION` moves for reasons that have nothing to do with this shape.
    #[serde(default = "identity_version")]
    pub version: u32,
    /// The loaded preset's name and where it came from. `None` is *no preset*.
    #[serde(default)]
    pub loaded: Option<LoadedPreset>,
    /// Every parameter as it stands **after** the preset is applied.
    ///
    /// A save reads the live parameters. A load canonicalises each intended target through its
    /// parameter before emitting host gestures, because CLAP may apply those gestures later; raw
    /// file numbers are never used directly for stepped, enum or boolean baselines.
    #[serde(default)]
    pub baseline: BTreeMap<String, f32>,
    /// Cheap identity for opt-in durable sound content such as embedded sample audio.
    #[serde(default)]
    pub state_fingerprint: Option<u64>,
}

/// The identity half: which preset, and from where.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadedPreset {
    pub name: String,
    pub origin: Origin,
}

const fn identity_version() -> u32 {
    1
}

impl PresetIdentity {
    /// The state a fresh instance is in, and the state Init returns it to.
    pub fn none() -> Self {
        Self {
            version: identity_version(),
            loaded: None,
            baseline: BTreeMap::new(),
            state_fingerprint: None,
        }
    }

    /// **The whole of dirty**: something is loaded, and its parameters or durable content moved.
    pub fn is_dirty(
        &self,
        current: &BTreeMap<String, f32>,
        state_fingerprint: Option<u64>,
    ) -> bool {
        self.loaded.is_some()
            && (&self.baseline != current || self.state_fingerprint != state_fingerprint)
    }
}

/// Every parameter's current value, keyed by id — the shape a baseline takes.
///
/// Used when the current patch itself is the intended result, such as Save. Preset loading starts
/// from this snapshot and replaces each written value with the parameter's canonical target, so an
/// asynchronous CLAP host cannot make the identity describe the patch that was being replaced.
pub fn snapshot(instrument: &dyn Instrument) -> BTreeMap<String, f32> {
    instrument
        .parameters()
        .into_iter()
        .filter(|(id, _)| !instrument.is_instance_setting(id))
        .map(|(id, param)| (id.to_owned(), param.normalised()))
        .collect()
}

/// What the browser shows for the current patch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loaded {
    /// Nothing is loaded: a fresh instance, a legacy state, after Init, or after deleting the
    /// preset that was loaded.
    None,
    /// The loaded preset, unmodified.
    Clean { name: String, origin: Origin },
    /// The loaded preset, with the values moved since.
    Modified { name: String, origin: Origin },
}

impl Loaded {
    pub fn name(&self) -> Option<&str> {
        match self {
            Loaded::None => None,
            Loaded::Clean { name, .. } | Loaded::Modified { name, .. } => Some(name),
        }
    }

    pub fn origin(&self) -> Option<&Origin> {
        match self {
            Loaded::None => None,
            Loaded::Clean { origin, .. } | Loaded::Modified { origin, .. } => Some(origin),
        }
    }

    pub fn is_modified(&self) -> bool {
        matches!(self, Loaded::Modified { .. })
    }
}

/// What to show, computed by comparing the persisted baseline against the live parameters.
///
/// **Computed when anybody looks**, not observed as it happens: that is what catches automation that
/// ran while the editor was closed, and it costs the audio thread nothing.
pub fn loaded(instrument: &dyn Instrument) -> Loaded {
    let Ok(identity) = instrument.identity().read() else {
        return Loaded::None;
    };
    let Some(loaded) = identity.loaded.clone() else {
        return Loaded::None;
    };

    if identity.is_dirty(&snapshot(instrument), instrument.preset_state_fingerprint()) {
        Loaded::Modified {
            name: loaded.name,
            origin: loaded.origin,
        }
    } else {
        Loaded::Clean {
            name: loaded.name,
            origin: loaded.origin,
        }
    }
}

/// Records that `name` is now loaded, **capturing the baseline from the parameters**.
///
/// Used after a save, where the live parameters are the intended result. A preset load uses
/// [`mark_loaded_with_baseline`] instead because a CLAP host may apply its queued gestures after
/// this main-thread call returns.
pub fn mark_loaded(instrument: &dyn Instrument, name: &str, origin: Origin) {
    mark_loaded_with_baseline(instrument, name, origin, snapshot(instrument));
}

/// Records a loaded preset against its already-canonical target baseline.
///
/// Synchronous preset application means gesture emission is synchronous, not that a CLAP host
/// changes the parameter atomics before returning from each setter call. Retaining the targets
/// prevents a freshly loaded preset from becoming Modified when those queued writes later arrive.
pub(crate) fn mark_loaded_with_baseline(
    instrument: &dyn Instrument,
    name: &str,
    origin: Origin,
    baseline: BTreeMap<String, f32>,
) {
    if let Ok(mut identity) = instrument.identity().write() {
        identity.loaded = Some(LoadedPreset {
            name: name.to_owned(),
            origin,
        });
        identity.baseline = baseline;
        identity.state_fingerprint = instrument.preset_state_fingerprint();
    }
}

/// Clears the identity: **no preset**, rather than *modified*.
///
/// What Init does, and what deleting the loaded preset does. "Modified" would be wrong in both —
/// there is nothing left that it is a modification of.
pub fn mark_none(instrument: &dyn Instrument) {
    if let Ok(mut identity) = instrument.identity().write() {
        *identity = PresetIdentity::none();
    }
}

/// **Switches a plugin added after its states were saved, restored Off from an older state.** For a
/// `Plugin::filter_state`.
///
/// nice-plug restores only the parameters a state names, so an older state loaded over an instance
/// with one of `ids` on would leave it on; each missing id is inserted Off, which is what the older
/// state meant. A loaded preset's baseline (`#[persist = "preset"]`, the collection's key) that lacks
/// one gains it at normalised zero, so an otherwise unchanged preset does not reopen as Modified. A
/// state that already names an id, or an identity with nothing loaded, is left as it is.
pub fn add_switches_off(state: &mut nice_plug::prelude::PluginState, ids: &[&str]) {
    for id in ids {
        state
            .params
            .entry((*id).to_owned())
            .or_insert(nice_plug::plugin::ParamValue::Bool(false));
    }
    let Some(serialized) = state.fields.get_mut("preset") else {
        return;
    };
    let Ok(mut identity) =
        nice_plug::params::persist::deserialize_field::<PresetIdentity>(serialized)
    else {
        return;
    };
    if identity.loaded.is_none() || ids.iter().all(|id| identity.baseline.contains_key(*id)) {
        return;
    }
    for id in ids {
        identity.baseline.entry((*id).to_owned()).or_insert(0.0);
    }
    if let Ok(canonical) = nice_plug::params::persist::serialize_field(&identity) {
        *serialized = canonical;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestInstrument;
    // `Param` for reading a value, `InternalParamMut` for writing one without a host. See
    // `nudge` for why a test reaches for the second.
    use nice_plug::params::{InternalParamMut, Param};

    struct Instanced(TestInstrument);

    impl crate::Instrument for Instanced {
        fn clap_id(&self) -> &'static str {
            self.0.clap_id()
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn crate::ErasedParam)> {
            self.0.parameters()
        }
        fn identity(&self) -> &std::sync::RwLock<PresetIdentity> {
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
    fn instance_settings_do_not_participate_in_the_dirty_baseline() {
        let instrument = Instanced(TestInstrument::new());
        mark_loaded(&instrument, "Sound", Origin::Factory);
        // SAFETY: the test owns the instrument and no process or GUI thread can access it.
        unsafe { instrument.0.sync._internal_set_plain_value(true) };
        assert!(!loaded(&instrument).is_modified());
        // SAFETY: same exclusive test ownership.
        unsafe { instrument.0.cutoff._internal_set_plain_value(0.7) };
        assert!(loaded(&instrument).is_modified());
    }

    /// Moves the cutoff without a `ParamSetter`, which a test does not have.
    ///
    /// The value goes in through the same normalised route the editor uses, so what is being tested
    /// is the comparison and not a shortcut around it.
    fn nudge(instrument: &TestInstrument, to: f32) {
        // SAFETY: the same call the wrapper makes when a host writes a parameter. It is
        // `_internal_` because plugin code normally goes through a `ParamSetter`, which a unit test
        // has no host to obtain one from — and going through the setter is what is being tested
        // elsewhere, not here.
        unsafe { instrument.cutoff._internal_set_normalized_value(to) };
    }

    #[test]
    fn a_fresh_instance_has_no_preset_rather_than_a_clean_one() {
        // Three states, not two. "Clean" would claim something is loaded.
        assert_eq!(loaded(&TestInstrument::new()), Loaded::None);
    }

    #[test]
    fn a_loaded_preset_is_clean_the_instant_it_loads() {
        // The baseline is read back **from the parameters**, not from the file.
        let instrument = TestInstrument::new();
        mark_loaded(&instrument, "Sub bass", Origin::Factory);
        assert_eq!(
            loaded(&instrument),
            Loaded::Clean {
                name: "Sub bass".to_owned(),
                origin: Origin::Factory
            }
        );
    }

    #[test]
    fn moving_a_parameter_marks_it_modified_and_moving_it_back_does_not_latch() {
        // Dirty is a comparison, not a latch: undoing your one change undoes the marker with it.
        let instrument = TestInstrument::new();
        let before = instrument.cutoff.modulated_normalized_value();
        mark_loaded(&instrument, "Sub bass", Origin::Factory);

        nudge(&instrument, 0.25);
        assert!(loaded(&instrument).is_modified(), "moved");

        nudge(&instrument, before);
        assert!(!loaded(&instrument).is_modified(), "and moved back");
    }

    #[test]
    fn a_change_made_while_nobody_was_looking_is_still_caught() {
        // Host automation with the editor closed. The comparison happens when anybody looks rather
        // than when a value moves, which is why nothing has to watch.
        let instrument = TestInstrument::new();
        mark_loaded(&instrument, "Brass", Origin::Factory);
        nudge(&instrument, 0.11);
        assert!(
            loaded(&instrument).is_modified(),
            "the editor was never open, and it still knows"
        );
    }

    #[test]
    fn init_clears_the_identity_rather_than_marking_it_modified() {
        // There is nothing left that it would be a modification *of*.
        let instrument = TestInstrument::new();
        mark_loaded(&instrument, "Sub bass", Origin::Factory);
        nudge(&instrument, 0.2);
        assert!(loaded(&instrument).is_modified());

        mark_none(&instrument);
        assert_eq!(loaded(&instrument), Loaded::None);
    }

    #[test]
    fn durable_content_participates_in_dirty_without_latching() {
        let instrument = TestInstrument::new();
        let identity = PresetIdentity {
            version: 1,
            loaded: Some(LoadedPreset {
                name: "Sample".to_owned(),
                origin: Origin::User,
            }),
            baseline: snapshot(&instrument),
            state_fingerprint: Some(41),
        };
        assert!(!identity.is_dirty(&snapshot(&instrument), Some(41)));
        assert!(identity.is_dirty(&snapshot(&instrument), Some(42)));
        assert!(!identity.is_dirty(&snapshot(&instrument), Some(41)));
    }

    #[test]
    fn a_baseline_with_no_identity_is_never_dirty() {
        // The predicate is `identity.is_some() && values != baseline`, and this is the half that
        // stops a stale baseline from marking an unloaded patch modified.
        let instrument = TestInstrument::new();
        let identity = PresetIdentity {
            version: 1,
            loaded: None,
            baseline: BTreeMap::new(),
            state_fingerprint: None,
        };
        assert!(!identity.is_dirty(&snapshot(&instrument), None));
    }

    /// **An older state restores an added switch Off, and a loaded preset stays clean**
    /// ([`add_switches_off`]): the state gains the switch Off, the baseline gains it at zero, and a
    /// state or identity that already names it is left alone.
    #[test]
    fn an_older_state_gains_an_added_switch_off_and_its_preset_stays_clean() {
        use nice_plug::params::persist::{deserialize_field, serialize_field};
        use nice_plug::plugin::ParamValue;
        let identity = PresetIdentity {
            loaded: Some(LoadedPreset {
                name: "Old".to_owned(),
                origin: Origin::Factory,
            }),
            baseline: [("level".to_owned(), 0.5)].into_iter().collect(),
            ..PresetIdentity::default()
        };
        let mut state = nice_plug::prelude::PluginState {
            version: String::new(),
            params: [("level".to_owned(), ParamValue::F32(0.5))]
                .into_iter()
                .collect(),
            fields: [("preset".to_owned(), serialize_field(&identity).unwrap())]
                .into_iter()
                .collect(),
        };
        add_switches_off(&mut state, &["ratesync"]);
        assert!(matches!(
            state.params.get("ratesync"),
            Some(ParamValue::Bool(false))
        ));
        let migrated: PresetIdentity = deserialize_field(&state.fields["preset"]).unwrap();
        assert_eq!(migrated.baseline.get("ratesync"), Some(&0.0));
        assert_eq!(migrated.baseline.get("level"), Some(&0.5));

        // A state that names the switch keeps its value, and a baseline that has it keeps its own.
        state
            .params
            .insert("ratesync".to_owned(), ParamValue::Bool(true));
        let named = PresetIdentity {
            baseline: [("ratesync".to_owned(), 1.0)].into_iter().collect(),
            ..identity.clone()
        };
        state
            .fields
            .insert("preset".to_owned(), serialize_field(&named).unwrap());
        add_switches_off(&mut state, &["ratesync"]);
        assert!(matches!(
            state.params.get("ratesync"),
            Some(ParamValue::Bool(true))
        ));
        let kept: PresetIdentity = deserialize_field(&state.fields["preset"]).unwrap();
        assert_eq!(kept.baseline.get("ratesync"), Some(&1.0));

        // Nothing loaded: no baseline to extend.
        let mut unloaded = nice_plug::prelude::PluginState {
            version: String::new(),
            params: Default::default(),
            fields: [(
                "preset".to_owned(),
                serialize_field(&PresetIdentity::default()).unwrap(),
            )]
            .into_iter()
            .collect(),
        };
        add_switches_off(&mut unloaded, &["ratesync"]);
        let none: PresetIdentity = deserialize_field(&unloaded.fields["preset"]).unwrap();
        assert!(none.baseline.is_empty());
    }
}
