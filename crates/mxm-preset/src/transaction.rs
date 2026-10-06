//! Opt-in deferred preset transactions for durable content that must be prepared asynchronously.
//!
//! The ordinary `apply_preset_checked()` and `init_patch()` paths remain synchronous. This module
//! owns the ordering for consumers whose complete payload or source-preserving overlay cannot be
//! prepared before returning from the initial UI action.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{Instrument, LoadedPreset, Origin, Preset, PresetIdentity, mark_none};

/// How the consumer interprets the durable half of a deferred request.
#[derive(Clone, Debug, PartialEq)]
pub enum DeferredContent {
    /// A complete durable payload, as carried by a portable user preset.
    Complete(Value),
    /// Recipe-owned state merged with the consumer's currently committed source.
    MergeWithCommittedSource(Value),
    /// Source-preserving model reset used by an opt-in Init transaction.
    InitMergeWithCommittedSource(Value),
}

/// Monotonic identity for one deferred transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeferredRequestId(u64);

impl DeferredRequestId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Work returned to the consumer after the shared layer has captured a transaction atomically.
#[derive(Clone, Debug, PartialEq)]
pub struct DeferredRequest {
    pub id: DeferredRequestId,
    pub content: DeferredContent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeferredBeginError {
    Exhausted,
}

/// Observable state of the opt-in transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeferredStatus {
    Idle,
    Pending(DeferredRequestId),
    Ready(DeferredRequestId),
    AwaitingProcessCommit(DeferredRequestId),
    Rejected(DeferredRequestId, String),
}

#[derive(Clone, Debug)]
enum IdentityTarget {
    Loaded {
        name: String,
        origin: Origin,
        /// The preset's intended post-publication values, retained separately from later live edits.
        baseline: BTreeMap<String, f32>,
        /// Captured immediately after the consumer's publication hook, never at acknowledgement.
        state_fingerprint: Option<u64>,
    },
    None,
}

#[derive(Clone, Debug)]
struct Write {
    id: String,
    observed: f32,
    observed_revision: Option<u64>,
    target: f32,
}

#[derive(Clone, Debug)]
struct Pending {
    id: DeferredRequestId,
    content: DeferredContent,
    writes: Vec<Write>,
    identity: IdentityTarget,
    state: PendingState,
}

#[derive(Clone, Debug)]
enum PendingState {
    Preparing,
    Ready,
    AwaitingProcessCommit,
    Rejected(String),
}

/// Shared ordering state for one opt-in consumer.
///
/// Beginning a newer request abandons an older unpublished request. A predecessor that already
/// crossed publication is retained independently until its callback acknowledgement. `ready()` does
/// not expose parameters or identity. `publish_ready()` emits eligible gestures and then invokes the
/// consumer's infallible, pre-reserved process-boundary publication hook. Identity changes only after
/// `acknowledge()`, and a successor cannot publish while its predecessor still awaits that boundary.
#[derive(Default)]
pub struct DeferredPresetTransaction {
    next_id: u64,
    pending: Option<Pending>,
    awaiting: Option<Pending>,
}

impl DeferredPresetTransaction {
    pub const fn new() -> Self {
        Self {
            next_id: 0,
            pending: None,
            awaiting: None,
        }
    }

    /// Begin applying a preset whose durable content has already been classified by the consumer.
    /// Unknown and missing parameter reporting is returned immediately, but nothing is written.
    pub fn begin_preset(
        &mut self,
        instrument: &dyn Instrument,
        preset: &Preset,
        name: &str,
        origin: Origin,
        content: DeferredContent,
    ) -> Result<(DeferredRequest, Vec<String>), DeferredBeginError> {
        let (resolved, problems) = preset.resolve_from(instrument, Some(&origin));
        // The same inventory `identity::snapshot` compares against: instance settings are host
        // state, never part of a loaded sound's baseline.
        let mut baseline: BTreeMap<String, f32> = instrument
            .parameters()
            .into_iter()
            .filter(|(id, _)| !instrument.is_instance_setting(id))
            .map(|(id, parameter)| (id.to_owned(), parameter.normalised()))
            .collect();
        let writes = resolved
            .into_iter()
            .map(|(id, parameter, target)| {
                let observed = baseline[id];
                baseline.insert(id.to_owned(), parameter.canonical_normalised(target));
                Write {
                    id: id.to_owned(),
                    observed,
                    observed_revision: instrument.parameter_edit_revision(id),
                    target,
                }
            })
            .collect();
        let request = self.begin(
            writes,
            IdentityTarget::Loaded {
                name: name.to_owned(),
                origin,
                baseline,
                state_fingerprint: None,
            },
            content,
        )?;
        Ok((request, problems))
    }

    /// Begin source-preserving Init. Defaults are captured but remain invisible until preparation
    /// reports Ready; source-owned parameters and instance settings are omitted exactly as on the
    /// synchronous path.
    pub fn begin_init(
        &mut self,
        instrument: &dyn Instrument,
        overlay: Value,
    ) -> Result<DeferredRequest, DeferredBeginError> {
        let writes = instrument
            .parameters()
            .into_iter()
            .filter(|(id, _)| {
                !instrument.is_source_owned(id) && !instrument.is_instance_setting(id)
            })
            .map(|(id, parameter)| Write {
                id: id.to_owned(),
                observed: parameter.normalised(),
                observed_revision: instrument.parameter_edit_revision(id),
                target: parameter.default_normalised(),
            })
            .collect();
        self.begin(
            writes,
            IdentityTarget::None,
            DeferredContent::InitMergeWithCommittedSource(overlay),
        )
    }

    fn begin(
        &mut self,
        writes: Vec<Write>,
        identity: IdentityTarget,
        content: DeferredContent,
    ) -> Result<DeferredRequest, DeferredBeginError> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| matches!(pending.state, PendingState::AwaitingProcessCommit))
        {
            debug_assert!(self.awaiting.is_none());
            self.awaiting = self.pending.take();
        }
        let id = DeferredRequestId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(DeferredBeginError::Exhausted)?;
        self.pending = Some(Pending {
            id,
            content: content.clone(),
            writes,
            identity,
            state: PendingState::Preparing,
        });
        Ok(DeferredRequest { id, content })
    }

    /// Mark the newest request prepared. A stale completion is ignored.
    pub fn ready(&mut self, id: DeferredRequestId) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if pending.id != id || !matches!(pending.state, PendingState::Preparing) {
            return false;
        }
        pending.state = PendingState::Ready;
        true
    }

    /// Reject the newest request without exposing parameter, identity, or durable-state changes.
    pub fn reject(&mut self, id: DeferredRequestId, reason: impl Into<String>) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if pending.id != id || matches!(pending.state, PendingState::AwaitingProcessCommit) {
            return false;
        }
        pending.state = PendingState::Rejected(reason.into());
        true
    }

    /// Emit the Ready request's still-eligible parameter gestures, then queue its already-prepared
    /// candidate through an infallible pre-reserved consumer hook. A parameter is eligible only when
    /// its unmodulated bits still match the value snapshot and, when supplied, its monotonic base
    /// revision is also unchanged. Requiring both closes the interval between those two begin-time
    /// reads; the revision additionally detects an observed away-and-back edit. Identity remains
    /// unchanged until `acknowledge()`.
    pub fn publish_ready(
        &mut self,
        instrument: &dyn Instrument,
        setter: &nice_plug::prelude::ParamSetter<'_>,
        mut publish: impl FnMut(DeferredRequestId),
    ) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if !matches!(pending.state, PendingState::Ready) || self.awaiting.is_some() {
            return false;
        }
        let parameters: BTreeMap<&str, &dyn crate::ErasedParam> =
            instrument.parameters().into_iter().collect();
        for write in &pending.writes {
            let Some(parameter) = parameters.get(write.id.as_str()) else {
                continue;
            };
            let value_unchanged = parameter.normalised().to_bits() == write.observed.to_bits();
            let revision_unchanged = write.observed_revision.is_none_or(|observed| {
                instrument.parameter_edit_revision(&write.id) == Some(observed)
            });
            let untouched = value_unchanged && revision_unchanged;
            if !untouched {
                continue;
            }
            parameter.begin(setter);
            parameter.set(setter, write.target);
            parameter.end(setter);
        }
        publish(pending.id);
        if let IdentityTarget::Loaded {
            state_fingerprint, ..
        } = &mut pending.identity
        {
            *state_fingerprint = instrument.preset_state_fingerprint();
        }
        pending.state = PendingState::AwaitingProcessCommit;
        true
    }

    /// Install the identity/baseline only after the audio callback acknowledged publication.
    pub fn acknowledge(&mut self, id: DeferredRequestId, instrument: &dyn Instrument) -> bool {
        let slot = if self
            .awaiting
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            &mut self.awaiting
        } else {
            &mut self.pending
        };
        let Some(pending) = slot.as_ref() else {
            return false;
        };
        if pending.id != id || !matches!(pending.state, PendingState::AwaitingProcessCommit) {
            return false;
        }
        match &pending.identity {
            IdentityTarget::Loaded {
                name,
                origin,
                baseline,
                state_fingerprint,
            } => {
                if let Ok(mut identity) = instrument.identity().write() {
                    let mut installed = PresetIdentity::none();
                    installed.loaded = Some(LoadedPreset {
                        name: name.clone(),
                        origin: origin.clone(),
                    });
                    installed.baseline.clone_from(baseline);
                    installed.state_fingerprint = *state_fingerprint;
                    *identity = installed;
                }
            }
            IdentityTarget::None => mark_none(instrument),
        }
        *slot = None;
        true
    }

    /// Cancel only the named live request. Stale cancellation cannot abandon a successor.
    pub fn cancel(&mut self, id: DeferredRequestId) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            self.pending = None;
            true
        } else if self
            .awaiting
            .as_ref()
            .is_some_and(|pending| pending.id == id)
        {
            self.awaiting = None;
            true
        } else {
            false
        }
    }

    pub fn status(&self) -> DeferredStatus {
        let pending = self.pending.as_ref().or(self.awaiting.as_ref());
        let Some(pending) = pending else {
            return DeferredStatus::Idle;
        };
        match &pending.state {
            PendingState::Preparing => DeferredStatus::Pending(pending.id),
            PendingState::Ready => DeferredStatus::Ready(pending.id),
            PendingState::AwaitingProcessCommit => {
                DeferredStatus::AwaitingProcessCommit(pending.id)
            }
            PendingState::Rejected(reason) => DeferredStatus::Rejected(pending.id, reason.clone()),
        }
    }

    pub fn content(&self, id: DeferredRequestId) -> Option<&DeferredContent> {
        [self.pending.as_ref(), self.awaiting.as_ref()]
            .into_iter()
            .flatten()
            .find(|pending| pending.id == id)
            .map(|pending| &pending.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestInstrument;
    use crate::{Category, Loaded, loaded, mark_loaded};
    use nice_plug::params::internals::ParamPtr;
    use nice_plug::params::{InternalParamMut, Param};
    use nice_plug::prelude::{PluginApi, PluginState};

    struct CountingHost(std::sync::atomic::AtomicUsize);

    impl nice_plug::context::gui::GuiContextInner for CountingHost {
        // A test double has no host to ask for a restart (nice-plug 0.4).
        fn request_restart(&self) {}
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    #[test]
    fn pending_rejected_cancelled_and_superseded_requests_expose_nothing() {
        let instrument = TestInstrument::new();
        mark_loaded(&instrument, "Before", Origin::Factory);
        let before = instrument.cutoff.modulated_normalized_value();
        let preset = Preset::capture("After", Category::Fx, &instrument);
        let mut transaction = DeferredPresetTransaction::new();
        let first = transaction
            .begin_preset(
                &instrument,
                &preset,
                "After",
                Origin::User,
                DeferredContent::MergeWithCommittedSource(serde_json::json!({"decay": 75})),
            )
            .unwrap()
            .0;
        assert!(matches!(transaction.status(), DeferredStatus::Pending(id) if id == first.id));
        assert_eq!(loaded(&instrument).name(), Some("Before"));
        assert_eq!(instrument.cutoff.modulated_normalized_value(), before);
        assert!(transaction.reject(first.id, "no room"));
        assert_eq!(loaded(&instrument).name(), Some("Before"));

        let second = transaction
            .begin_init(&instrument, serde_json::json!({}))
            .unwrap();
        let third = transaction
            .begin_init(&instrument, serde_json::json!({}))
            .unwrap();
        assert!(!transaction.ready(second.id));
        assert!(!transaction.cancel(second.id));
        assert!(transaction.cancel(third.id));
        assert_eq!(loaded(&instrument).name(), Some("Before"));
    }

    #[test]
    fn ready_writes_then_publication_ack_installs_identity_and_later_edits_win() {
        let instrument = TestInstrument::new();
        let mut preset = Preset::capture("Deferred", Category::Fx, &instrument);
        preset.params.get_mut("cutoff").unwrap().v = 0.1;
        preset.params.get_mut("level").unwrap().v = 0.2;
        let mut transaction = DeferredPresetTransaction::new();
        let request = transaction
            .begin_preset(
                &instrument,
                &preset,
                "Deferred",
                Origin::User,
                DeferredContent::Complete(serde_json::json!({"audio": "embedded"})),
            )
            .unwrap()
            .0;
        // Simulate a later independent base edit while preparation is pending.
        unsafe { instrument.level._internal_set_normalized_value(0.6) };
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = nice_plug::prelude::ParamSetter::new(&host);
        assert!(transaction.ready(request.id));
        assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 0);
        let mut published = None;
        assert!(transaction.publish_ready(&instrument, &setter, |id| published = Some(id)));
        assert_eq!(published, Some(request.id));
        assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 6);
        assert!((instrument.level.modulated_normalized_value() - 0.6).abs() < 1.0e-6);
        assert_eq!(
            loaded(&instrument),
            Loaded::None,
            "identity published early"
        );
        // A second edit after publication but before callback acknowledgement must also remain
        // outside the preset's intended baseline.
        unsafe { instrument.cutoff._internal_set_normalized_value(0.7) };
        assert!(transaction.acknowledge(request.id, &instrument));
        assert_eq!(
            loaded(&instrument),
            Loaded::Modified {
                name: "Deferred".to_owned(),
                origin: Origin::User,
            },
            "the later edit must not be recaptured into a Clean baseline"
        );
        let identity = instrument.identity().read().unwrap();
        assert!((identity.baseline["level"] - 0.2).abs() < 1.0e-6);
        assert!((identity.baseline["cutoff"] - 0.1).abs() < 1.0e-6);
        assert!((instrument.level.modulated_normalized_value() - 0.6).abs() < 1.0e-6);
        assert!((instrument.cutoff.modulated_normalized_value() - 0.7).abs() < 1.0e-6);
    }

    #[test]
    fn published_predecessor_survives_a_cancelled_successor() {
        let instrument = TestInstrument::new();
        let first_preset = Preset::capture("First", Category::Fx, &instrument);
        let mut transaction = DeferredPresetTransaction::new();
        let first = transaction
            .begin_preset(
                &instrument,
                &first_preset,
                "First",
                Origin::User,
                DeferredContent::Complete(serde_json::json!({"response": "first"})),
            )
            .unwrap()
            .0;
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = nice_plug::prelude::ParamSetter::new(&host);
        assert!(transaction.ready(first.id));
        assert!(transaction.publish_ready(&instrument, &setter, |_| {}));

        let second_preset = Preset::capture("Second", Category::Fx, &instrument);
        let second = transaction
            .begin_preset(
                &instrument,
                &second_preset,
                "Second",
                Origin::User,
                DeferredContent::Complete(serde_json::json!({"response": "second"})),
            )
            .unwrap()
            .0;
        assert!(transaction.ready(second.id));
        assert!(
            !transaction.publish_ready(&instrument, &setter, |_| {}),
            "a successor must not publish over the predecessor's callback slot"
        );
        assert!(transaction.cancel(second.id));
        assert!(transaction.acknowledge(first.id, &instrument));
        assert_eq!(
            loaded(&instrument),
            Loaded::Clean {
                name: "First".to_owned(),
                origin: Origin::User,
            }
        );
    }

    #[test]
    fn an_edit_between_value_and_revision_capture_is_not_overwritten() {
        struct EditDuringRevisionCapture {
            inner: TestInstrument,
            inject: std::sync::atomic::AtomicBool,
        }

        impl Instrument for EditDuringRevisionCapture {
            fn clap_id(&self) -> &'static str {
                self.inner.clap_id()
            }

            fn parameters(&self) -> Vec<(&'static str, &dyn crate::ErasedParam)> {
                self.inner.parameters()
            }

            fn parameter_edit_revision(&self, id: &str) -> Option<u64> {
                if id == "cutoff" && self.inject.swap(false, std::sync::atomic::Ordering::AcqRel) {
                    // This is deliberately after begin_preset() read the value and before it reads
                    // the revision. The first observation incorporates the edit into revision zero.
                    unsafe {
                        let _ = self.inner.cutoff._internal_set_normalized_value(0.9);
                    }
                }
                self.inner.observe_revision(id)
            }

            fn identity(&self) -> &std::sync::RwLock<crate::PresetIdentity> {
                self.inner.identity()
            }

            fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
                self.inner.factory_files()
            }
        }

        let instrument = EditDuringRevisionCapture {
            inner: TestInstrument::new(),
            inject: std::sync::atomic::AtomicBool::new(true),
        };
        let mut preset = Preset::capture("Deferred", Category::Fx, &instrument);
        preset.params.get_mut("cutoff").unwrap().v = 0.1;
        let mut transaction = DeferredPresetTransaction::new();
        let request = transaction
            .begin_preset(
                &instrument,
                &preset,
                "Deferred",
                Origin::User,
                DeferredContent::Complete(serde_json::json!({})),
            )
            .unwrap()
            .0;

        assert_eq!(instrument.inner.cutoff.unmodulated_normalized_value(), 0.9);
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = nice_plug::prelude::ParamSetter::new(&host);
        assert!(transaction.ready(request.id));
        assert!(transaction.publish_ready(&instrument, &setter, |_| {}));
        assert_eq!(
            instrument.inner.cutoff.unmodulated_normalized_value(),
            0.9,
            "matching revision alone must not overwrite the intervening base edit"
        );
        assert!(transaction.acknowledge(request.id, &instrument));
        assert_eq!(
            loaded(&instrument),
            Loaded::Modified {
                name: "Deferred".to_owned(),
                origin: Origin::User,
            }
        );
    }

    #[test]
    fn an_away_and_back_edit_is_not_overwritten_by_deferred_publication() {
        let instrument = TestInstrument::new();
        let observed = instrument.cutoff.modulated_normalized_value();
        let mut preset = Preset::capture("Deferred", Category::Fx, &instrument);
        preset.params.get_mut("cutoff").unwrap().v = 0.1;
        let mut transaction = DeferredPresetTransaction::new();
        let request = transaction
            .begin_preset(
                &instrument,
                &preset,
                "Deferred",
                Origin::User,
                DeferredContent::Complete(serde_json::json!({})),
            )
            .unwrap()
            .0;

        // Value equality cannot distinguish this from no edit. Sampling the unmodulated bits at
        // each observable boundary advances the revision for both legs of the edit.
        unsafe {
            let _ = instrument.cutoff._internal_set_normalized_value(0.9);
        }
        let _ = instrument.observe_revision("cutoff");
        unsafe {
            let _ = instrument.cutoff._internal_set_normalized_value(observed);
        }
        let _ = instrument.observe_revision("cutoff");
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = nice_plug::prelude::ParamSetter::new(&host);
        assert!(transaction.ready(request.id));
        assert!(transaction.publish_ready(&instrument, &setter, |_| {}));
        assert_eq!(instrument.cutoff.modulated_normalized_value(), observed);
        assert!(transaction.acknowledge(request.id, &instrument));
        assert_eq!(
            loaded(&instrument),
            Loaded::Modified {
                name: "Deferred".to_owned(),
                origin: Origin::User,
            }
        );
    }

    /// Declares `sync` an instance setting, as a routing selector would be.
    struct Instanced(TestInstrument);

    impl Instrument for Instanced {
        fn clap_id(&self) -> &'static str {
            self.0.clap_id()
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn crate::ErasedParam)> {
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
    fn deferred_preset_and_init_leave_instance_settings_out_of_writes_and_baseline() {
        let instrument = Instanced(TestInstrument::new());
        unsafe { instrument.0.sync._internal_set_plain_value(true) };
        let preset = Preset::capture("Deferred", Category::Fx, &instrument);
        let mut transaction = DeferredPresetTransaction::new();
        let request = transaction
            .begin_preset(
                &instrument,
                &preset,
                "Deferred",
                Origin::User,
                DeferredContent::Complete(serde_json::json!({})),
            )
            .unwrap()
            .0;
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = nice_plug::prelude::ParamSetter::new(&host);
        assert!(transaction.ready(request.id));
        assert!(transaction.publish_ready(&instrument, &setter, |_| {}));
        // Begin, set and end for cutoff and level only.
        assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 6);
        assert!(transaction.acknowledge(request.id, &instrument));
        assert!(
            !instrument
                .identity()
                .read()
                .unwrap()
                .baseline
                .contains_key("sync")
        );
        unsafe { instrument.0.sync._internal_set_plain_value(false) };
        assert!(
            !loaded(&instrument).is_modified(),
            "an instance setting dirtied the sound"
        );

        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = nice_plug::prelude::ParamSetter::new(&host);
        let init = transaction
            .begin_init(&instrument, serde_json::json!({}))
            .unwrap();
        assert!(transaction.ready(init.id));
        assert!(transaction.publish_ready(&instrument, &setter, |_| {}));
        assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 6);
    }

    #[test]
    fn deferred_init_clears_identity_only_after_acknowledged_process_commit() {
        let instrument = TestInstrument::new();
        mark_loaded(&instrument, "Before", Origin::Factory);
        let mut transaction = DeferredPresetTransaction::new();
        let request = transaction
            .begin_init(&instrument, serde_json::json!({"model": "neutral"}))
            .unwrap();
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = nice_plug::prelude::ParamSetter::new(&host);
        assert!(transaction.ready(request.id));
        assert!(transaction.publish_ready(&instrument, &setter, |_| {}));
        assert_eq!(loaded(&instrument).name(), Some("Before"));
        assert!(transaction.acknowledge(request.id, &instrument));
        assert_eq!(loaded(&instrument), Loaded::None);
    }
}
