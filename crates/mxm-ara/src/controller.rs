//! What a plug-in implements: [`Controller`], its document controller's behaviour, called at
//! each moment of the ARA model's life with a [`Cx`] that grants the host calls ARA allows there.

use crate::content::{
    ContentEvents, ContentGrade, ContentType, HostContent, HostContentEvents, TimeRange,
};
use crate::factory::FactoryConfig;
use crate::graph::{
    AudioModificationId, AudioSourceId, DocumentProperties, ExtensionId, Graph, MusicalContextId,
    PlaybackRegionId, PlaybackRegionProperties, RegionSequenceId,
};
use crate::host::{AudioReader, HostInterfaces, PlaybackController, Readers, Sample};
use crate::notify::Notifier;

/// An object's archived state: the controller's own bytes, tagged with the schema version that
/// wrote them. The document controller files it under the object's persistent ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payload {
    pub schema: u32,
    pub bytes: Vec<u8>,
}

/// Which archived object a hook is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchivedObject {
    Document,
    AudioSource(AudioSourceId),
    AudioModification(AudioModificationId),
}

/// A processing algorithm offered to the host (`ARAProcessingAlgorithmProperties`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessingAlgorithm {
    pub persistent_id: &'static str,
    pub name: &'static str,
}

/// The roles a plug-in instance plays (`ARAPlugInInstanceRoleFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Roles(pub u32);

impl Roles {
    pub const PLAYBACK_RENDERER: Roles = Roles(crate::sys::kARAPlaybackRendererRole as u32);
    pub const EDITOR_RENDERER: Roles = Roles(crate::sys::kARAEditorRendererRole as u32);
    pub const EDITOR_VIEW: Roles = Roles(crate::sys::kARAEditorViewRole as u32);

    pub fn contains(self, role: Roles) -> bool {
        self.0 & role.0 == role.0
    }
}

impl std::ops::BitOr for Roles {
    type Output = Roles;
    fn bitor(self, rhs: Roles) -> Roles {
        Roles(self.0 | rhs.0)
    }
}

/// The host's selection, as last told to an editor view (`ARAViewSelection`), in its order of
/// importance; objects since destroyed are dropped from it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewSelection {
    pub playback_regions: Vec<PlaybackRegionId>,
    pub region_sequences: Vec<RegionSequenceId>,
    pub time_range: Option<TimeRange>,
}

/// What a new controller is given.
pub struct Init<'a> {
    pub document: &'a DocumentProperties,
    /// The queue for model update notifications; clone it into workers.
    pub notifier: Notifier,
    /// The API generation the host chose (`ARAAPIGeneration`).
    pub api_generation: i32,
}

/// A plug-in's ARA document controller. One value per document, created when the host creates
/// the document controller and dropped when it destroys it; every hook runs on the model thread.
///
/// The `did_create_*` hooks run inside the host's edit with the renderers kept out, so
/// [`Cx::graph_mut`] gives the new object's data to fill in. Every hook has a default that does
/// nothing (or offers nothing).
#[allow(unused_variables)]
pub trait Controller: Send + Sized + 'static {
    /// Per audio source: the plug-in's take cache, detection and its worker, say.
    type AudioSource: Default + Send + Sync + 'static;
    /// Per audio modification: the edits.
    type AudioModification: Default + Send + Sync + 'static;
    /// Per playback region: what its renderers read (a render slot swapped atomically, say).
    type PlaybackRegion: Default + Send + Sync + 'static;

    /// The factory: names, IDs and capabilities.
    const FACTORY: FactoryConfig;

    /// The newest archive schema this controller reads and writes. An archived object of a newer
    /// schema is kept verbatim and written back unchanged ([`crate::graph::AudioSource::kept_archive`]).
    const ARCHIVE_SCHEMA: u32 = 1;

    /// Processing algorithms offered to the host; none by default.
    const PROCESSING_ALGORITHMS: &'static [ProcessingAlgorithm] = &[];

    fn new(init: Init<'_>) -> Self;

    // ---- the document and its edits ----

    /// The host begins an edit; renderers are already out.
    fn will_begin_editing(&mut self, cx: &mut Cx<'_, Self>) {}
    /// The edit's last moment with renderers out ([`Cx::graph_mut`] works); any audio source may
    /// be read and any musical context's content.
    fn will_end_editing(&mut self, cx: &mut Cx<'_, Self>) {}
    /// After the edit, renderers back in; any source and context may still be read.
    fn did_end_editing(&mut self, cx: &mut Cx<'_, Self>) {}
    /// The host polls for notifications: queue any now ([`Cx::notifier`]); they go out after this.
    fn will_notify_model_updates(&mut self, cx: &mut Cx<'_, Self>) {}
    fn did_update_document_properties(&mut self, cx: &mut Cx<'_, Self>) {}

    // ---- musical contexts and region sequences ----

    /// Its content (tempo, bars, key) may be read here ([`Cx::read_musical_context_content`]).
    fn did_create_musical_context(&mut self, cx: &mut Cx<'_, Self>, context: MusicalContextId) {}
    fn did_update_musical_context_properties(
        &mut self,
        cx: &mut Cx<'_, Self>,
        context: MusicalContextId,
    ) {
    }
    fn did_update_musical_context_content(
        &mut self,
        cx: &mut Cx<'_, Self>,
        context: MusicalContextId,
        range: Option<TimeRange>,
        flags: crate::ContentUpdateFlags,
    ) {
    }
    fn will_destroy_musical_context(&mut self, cx: &mut Cx<'_, Self>, context: MusicalContextId) {}
    fn did_create_region_sequence(&mut self, cx: &mut Cx<'_, Self>, sequence: RegionSequenceId) {}
    fn did_update_region_sequence_properties(
        &mut self,
        cx: &mut Cx<'_, Self>,
        sequence: RegionSequenceId,
    ) {
    }
    fn will_destroy_region_sequence(&mut self, cx: &mut Cx<'_, Self>, sequence: RegionSequenceId) {}

    // ---- audio sources ----

    fn did_create_audio_source(&mut self, cx: &mut Cx<'_, Self>, source: AudioSourceId) {}
    /// Before new properties land. If the sample rate, count or channels change, the source's
    /// readers are cancelled and joined after this returns.
    fn will_update_audio_source_properties(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        new: &crate::graph::AudioSourceProperties,
    ) {
    }
    fn did_update_audio_source_properties(&mut self, cx: &mut Cx<'_, Self>, source: AudioSourceId) {
    }
    /// The host changed its samples or content; [`Cx::cancel_audio_readers`] stops a read.
    fn did_update_audio_source_content(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        range: Option<TimeRange>,
        flags: crate::ContentUpdateFlags,
    ) {
    }
    /// Before sample access flips. On disabling, every reader of the source is then cancelled,
    /// its read under way joined, and destroyed, before the host's call returns.
    fn will_enable_audio_source_samples_access(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        enable: bool,
    ) {
    }
    /// After it flipped: enabled, readers may be created here.
    fn did_enable_audio_source_samples_access(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        enable: bool,
    ) {
    }
    fn did_deactivate_audio_source_for_undo_history(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        deactivate: bool,
    ) {
    }
    /// Its readers are cancelled and joined after this returns.
    fn will_destroy_audio_source(&mut self, cx: &mut Cx<'_, Self>, source: AudioSourceId) {}

    // ---- audio modifications ----

    /// `cloned_from` is set for `cloneAudioModification`: copy its edits into the new one.
    fn did_create_audio_modification(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
        cloned_from: Option<AudioModificationId>,
    ) {
    }
    fn did_update_audio_modification_properties(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
    ) {
    }
    fn did_deactivate_audio_modification_for_undo_history(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
        deactivate: bool,
    ) {
    }
    fn will_destroy_audio_modification(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
    ) {
    }

    // ---- playback regions ----

    fn did_create_playback_region(&mut self, cx: &mut Cx<'_, Self>, region: PlaybackRegionId) {}
    fn will_update_playback_region_properties(
        &mut self,
        cx: &mut Cx<'_, Self>,
        region: PlaybackRegionId,
        new: &PlaybackRegionProperties,
    ) {
    }
    fn did_update_playback_region_properties(
        &mut self,
        cx: &mut Cx<'_, Self>,
        region: PlaybackRegionId,
    ) {
    }
    /// The region is taken out of every renderer's and view's lists after this.
    fn will_destroy_playback_region(&mut self, cx: &mut Cx<'_, Self>, region: PlaybackRegionId) {}
    /// Head and tail time in seconds; recomputed after each change to the region and at the end
    /// of each edit, and kept where any thread may read it.
    fn playback_region_head_and_tail_time(
        &self,
        graph: &Graph<Self>,
        region: PlaybackRegionId,
    ) -> (f64, f64) {
        (0.0, 0.0)
    }

    // ---- plug-in instances (extensions) ----

    fn did_bind(&mut self, cx: &mut Cx<'_, Self>, extension: ExtensionId, roles: crate::Roles) {}
    fn will_unbind(&mut self, cx: &mut Cx<'_, Self>, extension: ExtensionId) {}
    fn did_add_playback_region_to_renderer(
        &mut self,
        cx: &mut Cx<'_, Self>,
        extension: ExtensionId,
        region: PlaybackRegionId,
    ) {
    }
    fn did_remove_playback_region_from_renderer(
        &mut self,
        cx: &mut Cx<'_, Self>,
        extension: ExtensionId,
        region: PlaybackRegionId,
    ) {
    }
    fn did_change_editor_renderer(&mut self, cx: &mut Cx<'_, Self>, extension: ExtensionId) {}
    fn did_notify_selection(
        &mut self,
        cx: &mut Cx<'_, Self>,
        extension: ExtensionId,
        selection: &ViewSelection,
    ) {
    }
    fn did_notify_hide_region_sequences(
        &mut self,
        cx: &mut Cx<'_, Self>,
        extension: ExtensionId,
        hidden: &[RegionSequenceId],
    ) {
    }

    // ---- archives ----

    /// The document's private data, stored when the host asks for it (`documentData`).
    fn store_document_data(&mut self, cx: &mut Cx<'_, Self>) -> Option<Payload> {
        None
    }
    fn store_audio_source(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
    ) -> Option<Payload> {
        None
    }
    fn store_audio_modification(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
    ) -> Option<Payload> {
        None
    }
    /// Restores from a payload of a schema this controller reads (at most `ARCHIVE_SCHEMA`); false
    /// for a malformed one, which makes the host's restore call report failure.
    fn restore_document_data(&mut self, cx: &mut Cx<'_, Self>, payload: &Payload) -> bool {
        true
    }
    fn restore_audio_source(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        payload: &Payload,
    ) -> bool {
        true
    }
    fn restore_audio_modification(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
        payload: &Payload,
    ) -> bool {
        true
    }
    /// An object's archived state was of a newer schema and is kept verbatim.
    fn did_keep_newer_archive(&mut self, cx: &mut Cx<'_, Self>, object: ArchivedObject) {}

    // ---- content the plug-in offers (none by default) ----

    /// `None`: this content isn't available for the source.
    fn audio_source_content_grade(
        &self,
        graph: &Graph<Self>,
        source: AudioSourceId,
        ty: ContentType,
    ) -> Option<ContentGrade> {
        None
    }
    fn audio_source_content(
        &self,
        graph: &Graph<Self>,
        source: AudioSourceId,
        ty: ContentType,
        range: Option<TimeRange>,
    ) -> Option<ContentEvents> {
        None
    }
    fn audio_modification_content_grade(
        &self,
        graph: &Graph<Self>,
        modification: AudioModificationId,
        ty: ContentType,
    ) -> Option<ContentGrade> {
        None
    }
    fn audio_modification_content(
        &self,
        graph: &Graph<Self>,
        modification: AudioModificationId,
        ty: ContentType,
        range: Option<TimeRange>,
    ) -> Option<ContentEvents> {
        None
    }
    fn playback_region_content_grade(
        &self,
        graph: &Graph<Self>,
        region: PlaybackRegionId,
        ty: ContentType,
    ) -> Option<ContentGrade> {
        None
    }
    fn playback_region_content(
        &self,
        graph: &Graph<Self>,
        region: PlaybackRegionId,
        ty: ContentType,
        range: Option<TimeRange>,
    ) -> Option<ContentEvents> {
        None
    }
    fn is_audio_source_content_analysis_incomplete(
        &self,
        graph: &Graph<Self>,
        source: AudioSourceId,
        ty: ContentType,
    ) -> bool {
        false
    }
    fn request_audio_source_content_analysis(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        types: &[ContentType],
    ) {
    }

    // ---- the rest of ARA 2 ----

    fn processing_algorithm_for_audio_source(
        &self,
        graph: &Graph<Self>,
        source: AudioSourceId,
    ) -> i32 {
        0
    }
    fn request_processing_algorithm_for_audio_source(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        index: i32,
    ) {
    }
    fn is_licensed_for_capabilities(
        &mut self,
        run_modal_activation_dialog: bool,
        types: &[ContentType],
        transformation_flags: i32,
    ) -> bool {
        true
    }
    /// Whether the modification leaves its source's sound as it is; false negatives are allowed.
    fn is_audio_modification_preserving_audio_source_signal(
        &self,
        graph: &Graph<Self>,
        modification: AudioModificationId,
    ) -> bool {
        false
    }
}

/// Where a hook runs, which decides the host calls ARA allows in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// No audio or content reading.
    Nothing,
    /// A call about this audio source.
    Source(AudioSourceId),
    /// A call creating or updating this musical context.
    Context(MusicalContextId),
    /// `endEditing`: any source, any context.
    Any,
}

pub(crate) enum GraphAccess<'a, C: Controller> {
    Shared(&'a Graph<C>),
    Exclusive(&'a mut Graph<C>),
}

/// A hook's context: the graph, the notifier, and the host calls allowed where it runs.
pub struct Cx<'a, C: Controller> {
    pub(crate) graph: GraphAccess<'a, C>,
    pub(crate) host: &'a HostInterfaces,
    pub(crate) notifier: &'a Notifier,
    pub(crate) readers: &'a mut Readers,
    pub(crate) scope: Scope,
    pub(crate) api_generation: i32,
}

impl<C: Controller> Cx<'_, C> {
    pub fn graph(&self) -> &Graph<C> {
        match &self.graph {
            GraphAccess::Shared(g) => g,
            GraphAccess::Exclusive(g) => g,
        }
    }

    /// The graph to change the plug-in's data in: `Some` while renderers are kept out (inside
    /// the host's edits and the hooks that run in them), `None` otherwise.
    pub fn graph_mut(&mut self) -> Option<&mut Graph<C>> {
        match &mut self.graph {
            GraphAccess::Shared(_) => None,
            GraphAccess::Exclusive(g) => Some(g),
        }
    }

    pub fn notifier(&self) -> &Notifier {
        self.notifier
    }

    pub fn api_generation(&self) -> i32 {
        self.api_generation
    }

    fn may_access_source(&self, source: AudioSourceId) -> bool {
        matches!(self.scope, Scope::Any) || self.scope == Scope::Source(source)
    }

    /// An audio reader for the source, to move to a worker thread. Only in a call about this
    /// source (or the end of an edit) and while its sample access is enabled; `None` otherwise.
    pub fn create_audio_reader<S: Sample>(
        &mut self,
        source: AudioSourceId,
    ) -> Option<AudioReader<S>> {
        if !self.may_access_source(source) {
            return None;
        }
        let node = self.graph().audio_source(source)?;
        if !node.is_sample_access_enabled() || node.is_deactivated_for_undo_history() {
            return None;
        }
        let channels = usize::try_from(node.properties().channel_count).ok()?;
        let host_ref = node.host_ref;
        self.readers
            .create::<S>(&self.host.audio, source, host_ref, channels)
    }

    /// Cancels every reader of the source, waits for a read under way to finish, and destroys
    /// them; their holders' next read returns `ReadError::Cancelled`. Only in a call about this
    /// source (or the end of an edit); false otherwise.
    pub fn cancel_audio_readers(&mut self, source: AudioSourceId) -> bool {
        if !self.may_access_source(source) {
            return false;
        }
        self.readers.cancel(&self.host.audio, Some(source));
        true
    }

    /// The host's content of a musical context: tempo entries, bar signatures, key signatures
    /// (the song's key)... Only in a call creating or updating this context, or at the end of an
    /// edit; `None` there, or where the host has none.
    pub fn read_musical_context_content<T: HostContent>(
        &self,
        context: MusicalContextId,
        range: Option<TimeRange>,
    ) -> Option<HostContentEvents<T>> {
        if !(matches!(self.scope, Scope::Any) || self.scope == Scope::Context(context)) {
            return None;
        }
        let node = self.graph().musical_context(context)?;
        self.host
            .content
            .as_ref()?
            .read_musical_context::<T>(node.host_ref, range)
    }

    /// The host's content of an audio source (known notes or tempo, say): where audio readers
    /// may be created.
    pub fn read_audio_source_content<T: HostContent>(
        &self,
        source: AudioSourceId,
        range: Option<TimeRange>,
    ) -> Option<HostContentEvents<T>> {
        if !self.may_access_source(source) {
            return None;
        }
        let node = self.graph().audio_source(source)?;
        self.host
            .content
            .as_ref()?
            .read_audio_source::<T>(node.host_ref, range)
    }

    /// What an instance's editor renderer was given (its regions and region sequences), as the
    /// host last assigned them; `None` for an instance not bound to this plug-in. Any hook.
    pub fn editor_renderer(&self, extension: ExtensionId) -> Option<crate::EditorAssignment> {
        crate::extension::editor_assignment::<C>(extension)
    }

    /// The host's playback controller, if it offers one.
    pub fn playback_controller(&self) -> Option<PlaybackController<'_>> {
        self.host.playback.as_ref().map(PlaybackController::new)
    }
}
