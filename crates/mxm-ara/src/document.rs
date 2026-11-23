//! The document controller: ARA's `ARADocumentControllerInterface` over a [`Graph`] and a
//! [`Controller`].
//!
//! Every call but `getFactory` and `getPlaybackRegionHeadAndTailTime` runs on the model thread
//! (the thread that created the controller; checked), one at a time (re-entrance is refused), and
//! with panics caught (a panic leaves the controller inert rather than unwinding into the host).
//! Every graph change happens with the renderers' guard closed: for the whole of a host's edit,
//! and around any change the host makes outside one.

use std::any::TypeId;
use std::cell::UnsafeCell;
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use crate::archive::{self, Entry, RestoreSelection, StoreSelection};
use crate::content::{
    ContentGrade, ContentType, ContentUpdateFlags, PlugInContentReader, TimeRange,
};
use crate::controller::{ArchivedObject, Controller, Cx, GraphAccess, Init, Payload, Scope};
use crate::extension::ExtInner;
use crate::factory::{FactoryBox, factory_box};
use crate::graph::{self, *};
use crate::guard::AccessGuard;
use crate::host::{ArchiveReader, ArchiveWriter, HostInterfaces, Readers};
use crate::notify::{Notification, Notifier};
use crate::sys;
use crate::util::{c_string, implemented_size, struct_size, thread_mark};
use crate::validate::{Category, check, report};

// ---- live objects ------------------------------------------------------------------------------

/// Addresses of live objects this library made, with their controller type, so a ref from the
/// host is checked before it is followed.
pub(crate) struct Registry(OnceLock<Mutex<HashMap<usize, TypeId>>>);

impl Registry {
    pub(crate) const fn new() -> Self {
        Registry(OnceLock::new())
    }
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<usize, TypeId>> {
        self.0
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
    pub(crate) fn insert<T: 'static>(&self, address: usize) {
        self.map().insert(address, TypeId::of::<T>());
    }
    pub(crate) fn remove(&self, address: usize) {
        self.map().remove(&address);
    }
    pub(crate) fn contains<T: 'static>(&self, address: usize) -> bool {
        self.map().get(&address) == Some(&TypeId::of::<T>())
    }
}

static CONTROLLERS: Registry = Registry::new();

// ---- the shared document -----------------------------------------------------------------------

/// A document controller's state, shared with the plug-in extensions bound to it (which may
/// outlive it).
pub(crate) struct DocShared<C: Controller> {
    /// The renderers' guard on the graph.
    pub(crate) guard: AccessGuard,
    graph: UnsafeCell<Graph<C>>,
    model: UnsafeCell<ModelState<C>>,
    /// A model call is under way (re-entrance is refused).
    busy: AtomicBool,
    /// The host destroyed the controller; the guard stays closed for good.
    pub(crate) destroyed: AtomicBool,
    /// A hook panicked; the controller refuses further calls.
    poisoned: AtomicBool,
    pub(crate) model_thread: usize,
    /// The bound extensions (`ExtInner<C>` addresses); each removes itself before it is freed.
    pub(crate) extensions: Mutex<Vec<usize>>,
    pub(crate) factory: &'static FactoryBox,
}

// SAFETY: the graph is written only by the model thread with the guard closed (renderers out),
// and read by renderers only inside the guard; the model state is touched only by the model
// thread, which every entry checks, with `busy` refusing re-entrance. The controller and the
// objects' data are `Send` (and the data `Sync`, as renderers share it).
unsafe impl<C: Controller> Send for DocShared<C> {}
unsafe impl<C: Controller> Sync for DocShared<C> {}

/// What only the model thread touches.
pub(crate) struct ModelState<C: Controller> {
    controller: Option<C>,
    host: HostInterfaces,
    notifier: Notifier,
    readers: Readers,
    editing: bool,
    /// Live plug-in content readers (addresses of boxed `PlugInContentReader`s).
    content_readers: HashSet<usize>,
    /// The document's archived state of a newer schema, kept verbatim.
    kept_document: Option<Payload>,
    /// Archive entries of kinds this version doesn't know, kept with the document data.
    unknown_entries: Vec<Entry>,
    _algorithm_strings: Vec<CString>,
    algorithm_properties: Vec<sys::ARAProcessingAlgorithmProperties>,
    api_generation: i32,
}

/// The object the host's `ARADocumentControllerRef` points to.
pub(crate) struct DcHandle<C: Controller> {
    instance: sys::ARADocumentControllerInstance,
    interface: sys::ARADocumentControllerInterface,
    shared: Arc<DocShared<C>>,
}

impl<C: Controller> DocShared<C> {
    /// Runs a model call: on the model thread, not re-entrant, not after a panic or the
    /// controller's destruction, with panics caught. `fallback` otherwise.
    pub(crate) fn call<R>(&self, fallback: R, f: impl FnOnce(&mut Call<'_, C>) -> R) -> R {
        if !check!(
            thread_mark() == self.model_thread,
            Thread,
            std::ptr::null::<u8>(),
            c"ARA model call off the document controller's thread"
        ) {
            return fallback;
        }
        if self.poisoned.load(Ordering::Acquire) || self.destroyed.load(Ordering::Acquire) {
            return fallback;
        }
        if !check!(
            !self.busy.swap(true, Ordering::Acquire),
            State,
            std::ptr::null::<u8>(),
            c"re-entrant call into the document controller"
        ) {
            return fallback;
        }
        // SAFETY: the model thread, alone (`busy`), is the only one touching the model state.
        let model = unsafe { &mut *self.model.get() };
        let mut call = Call {
            shared: self,
            model,
        };
        let result = catch_unwind(AssertUnwindSafe(|| f(&mut call)));
        self.busy.store(false, Ordering::Release);
        match result {
            Ok(r) => r,
            Err(_) => {
                self.poisoned.store(true, Ordering::Release);
                report(
                    Category::State,
                    std::ptr::null(),
                    c"panic in the document controller; it is now inert",
                );
                fallback
            }
        }
    }

    /// The graph, for a renderer inside the guard.
    ///
    /// # Safety
    /// The caller must be inside the guard (or be the model thread outside a change).
    pub(crate) unsafe fn graph(&self) -> &Graph<C> {
        // SAFETY: the caller's contract: no `&mut` to the graph exists meanwhile.
        unsafe { &*self.graph.get() }
    }
}

/// One model call's access to the document.
pub(crate) struct Call<'a, C: Controller> {
    pub(crate) shared: &'a DocShared<C>,
    model: &'a mut ModelState<C>,
}

const NULL: *const u8 = std::ptr::null();

impl<C: Controller> Call<'_, C> {
    pub(crate) fn graph(&self) -> &Graph<C> {
        // SAFETY: the model thread is the graph's only writer, and it is not writing now: a
        // `&mut` comes only from `graph_mut`, which needs `&mut self`.
        unsafe { &*self.shared.graph.get() }
    }

    fn graph_mut(&mut self) -> &mut Graph<C> {
        debug_assert!(
            self.shared.guard.is_closed(),
            "graph changed with renderers inside"
        );
        // SAFETY: the guard is closed (renderers out) and `&mut self` excludes the model
        // thread's other borrows.
        unsafe { &mut *self.shared.graph.get() }
    }

    fn close(&self) {
        self.shared.guard.close();
    }

    fn open(&self) {
        self.shared.guard.open();
    }

    /// Runs a hook with a context; `None` when the controller is gone.
    pub(crate) fn hook<R>(
        &mut self,
        scope: Scope,
        f: impl FnOnce(&mut C, &mut Cx<'_, C>) -> R,
    ) -> Option<R> {
        let shared = self.shared;
        let ModelState {
            controller,
            host,
            notifier,
            readers,
            api_generation,
            ..
        } = &mut *self.model;
        let controller = controller.as_mut()?;
        let graph = if shared.guard.is_closed() {
            // SAFETY: renderers are out, and this hook is the model thread's only access.
            GraphAccess::Exclusive(unsafe { &mut *shared.graph.get() })
        } else {
            // SAFETY: no writer while the model thread runs this hook.
            GraphAccess::Shared(unsafe { &*shared.graph.get() })
        };
        let mut cx = Cx {
            graph,
            host,
            notifier,
            readers,
            scope,
            api_generation: *api_generation,
        };
        Some(f(controller, &mut cx))
    }

    fn controller(&self) -> Option<&C> {
        self.model.controller.as_ref()
    }

    pub(crate) fn notifier(&self) -> &Notifier {
        &self.model.notifier
    }

    fn require_editing(&self, argument: *const c_void) {
        check!(
            self.model.editing,
            State,
            argument,
            c"model graph changed outside beginEditing/endEditing"
        );
    }

    fn require_no_content_readers(&self) {
        check!(
            self.model.content_readers.is_empty(),
            State,
            NULL,
            c"call made while content readers are alive"
        );
    }

    fn reap_readers(&mut self, source: Option<AudioSourceId>) {
        let audio = self.model.host.audio;
        self.model.readers.reap(&audio, source);
    }

    fn cancel_readers(&mut self, source: Option<AudioSourceId>) {
        let audio = self.model.host.audio;
        self.model.readers.cancel(&audio, source);
    }

    /// Recomputes head and tail times (of one region, or all), where any thread reads them.
    fn refresh_head_and_tail(&self, only: Option<PlaybackRegionId>) {
        let Some(controller) = self.controller() else {
            return;
        };
        let graph = self.graph();
        for b in graph.playback_regions.values() {
            if only.is_none_or(|id| id == b.id()) {
                let (head, tail) = controller.playback_region_head_and_tail_time(graph, b.id());
                b.cell().set_head_and_tail(head.max(0.0), tail.max(0.0));
            }
        }
    }

    /// Calls `f` on every bound extension (model thread, with the graph's guard closed when it
    /// changes the extensions' lists).
    fn each_extension(&self, mut f: impl FnMut(&ExtInner<C>)) {
        let extensions = self
            .shared
            .extensions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for &address in extensions.iter() {
            // SAFETY: an extension removes itself from this list, under this lock, before it is
            // freed, so every address here is live.
            f(unsafe { &*(address as *const ExtInner<C>) });
        }
    }

    // ---- update management ----

    fn begin_editing(&mut self) {
        self.require_no_content_readers();
        if !check!(
            !self.model.editing,
            State,
            NULL,
            c"beginEditing while already editing"
        ) {
            return;
        }
        self.close();
        self.model.editing = true;
        self.hook(Scope::Nothing, |c, cx| c.will_begin_editing(cx));
    }

    fn end_editing(&mut self) {
        self.require_no_content_readers();
        if !check!(
            self.model.editing,
            State,
            NULL,
            c"endEditing without beginEditing"
        ) {
            return;
        }
        self.graph_mut().sort_orders();
        self.refresh_head_and_tail(None);
        self.hook(Scope::Any, |c, cx| c.will_end_editing(cx));
        self.model.editing = false;
        self.open();
        self.reap_readers(None);
        self.hook(Scope::Any, |c, cx| c.did_end_editing(cx));
    }

    fn notify_model_updates(&mut self) {
        self.require_no_content_readers();
        if !check!(
            !self.model.editing,
            State,
            NULL,
            c"notifyModelUpdates while editing"
        ) {
            return;
        }
        self.hook(Scope::Nothing, |c, cx| c.will_notify_model_updates(cx));
        let queue = self.model.notifier.drain();
        let Some(u) = self.model.host.updates else {
            return;
        };
        let graph = self.graph();
        let range_ptr = |r: &Option<sys::ARAContentTimeRange>| {
            r.as_ref().map_or(std::ptr::null(), std::ptr::from_ref)
        };
        for n in queue {
            // SAFETY: (each call) the host's model update controller, called from within
            // notifyModelUpdates as ARA requires, about objects still live in the graph.
            match n {
                Notification::Progress {
                    source,
                    state,
                    value,
                } => {
                    if let Some(s) = graph.audio_source(source) {
                        unsafe {
                            (u.analysis_progress)(u.host_ref, s.host_ref as _, state.raw(), value)
                        }
                    }
                }
                Notification::SourceContent {
                    source,
                    range,
                    flags,
                } => {
                    if let Some(s) = graph.audio_source(source) {
                        let r = range.map(TimeRange::raw);
                        unsafe {
                            (u.source_content)(u.host_ref, s.host_ref as _, range_ptr(&r), flags.0)
                        }
                    }
                }
                Notification::ModificationContent {
                    modification,
                    range,
                    flags,
                } => {
                    if let Some(m) = graph.audio_modification(modification) {
                        let r = range.map(TimeRange::raw);
                        unsafe {
                            (u.modification_content)(
                                u.host_ref,
                                m.host_ref as _,
                                range_ptr(&r),
                                flags.0,
                            )
                        }
                    }
                }
                Notification::RegionContent {
                    region,
                    range,
                    flags,
                } => {
                    if let (Some(f), Some(p)) = (u.region_content, graph.playback_region(region)) {
                        let r = range.map(TimeRange::raw);
                        unsafe { f(u.host_ref, p.host_ref as _, range_ptr(&r), flags.0) }
                    }
                }
                Notification::DocumentData => {
                    if let Some(f) = u.document_data {
                        unsafe { f(u.host_ref) }
                    }
                }
            }
        }
    }

    // ---- the document ----

    unsafe fn update_document_properties(&mut self, properties: *const sys::ARADocumentProperties) {
        self.require_editing(properties as *const c_void);
        // SAFETY: (all `unsafe` calls below) the host's pointers are valid for the call, and are
        // read only after their size checks.
        if !valid(
            properties,
            implemented_size!(sys::ARADocumentProperties, name),
        ) {
            return;
        }
        let new = unsafe { graph::document_properties(properties) };
        self.close();
        self.graph_mut().document = new;
        self.hook(Scope::Nothing, |c, cx| c.did_update_document_properties(cx));
        self.open();
    }

    // ---- musical contexts ----

    unsafe fn create_musical_context(
        &mut self,
        host_ref: sys::ARAMusicalContextHostRef,
        properties: *const sys::ARAMusicalContextProperties,
    ) -> sys::ARAMusicalContextRef {
        self.require_editing(properties as *const c_void);
        if !valid(
            properties,
            sys::kARAMusicalContextPropertiesMinSize as usize,
        ) {
            return std::ptr::null_mut();
        }
        let properties = unsafe { graph::musical_context_properties(properties) };
        self.close();
        let mut node = Box::new(MusicalContext {
            id: MusicalContextId(0),
            host_ref: host_ref as usize,
            properties,
            region_sequences: Vec::new(),
        });
        let id = MusicalContextId(std::ptr::from_ref(&*node) as usize);
        node.id = id;
        let g = self.graph_mut();
        g.musical_contexts.insert(id.0, node);
        g.context_order.push(id);
        self.hook(Scope::Context(id), |c, cx| {
            c.did_create_musical_context(cx, id)
        });
        self.open();
        id.to_ref()
    }

    unsafe fn update_musical_context_properties(
        &mut self,
        context: sys::ARAMusicalContextRef,
        properties: *const sys::ARAMusicalContextProperties,
    ) {
        self.require_editing(properties as *const c_void);
        let id = MusicalContextId::from_ref(context);
        if !self.known_context(id)
            || !valid(
                properties,
                sys::kARAMusicalContextPropertiesMinSize as usize,
            )
        {
            return;
        }
        let new = unsafe { graph::musical_context_properties(properties) };
        self.close();
        if let Some(node) = self.graph_mut().musical_contexts.get_mut(&id.0) {
            node.properties = new;
        }
        self.hook(Scope::Context(id), |c, cx| {
            c.did_update_musical_context_properties(cx, id)
        });
        self.open();
    }

    unsafe fn update_musical_context_content(
        &mut self,
        context: sys::ARAMusicalContextRef,
        range: *const sys::ARAContentTimeRange,
        flags: sys::ARAContentUpdateFlags,
    ) {
        self.require_editing(context as *const c_void);
        let id = MusicalContextId::from_ref(context);
        if !self.known_context(id) {
            return;
        }
        let range = unsafe { TimeRange::from_ptr(range) };
        let flags = ContentUpdateFlags(flags);
        self.hook(Scope::Context(id), |c, cx| {
            c.did_update_musical_context_content(cx, id, range, flags)
        });
    }

    fn destroy_musical_context(&mut self, context: sys::ARAMusicalContextRef) {
        self.require_editing(context as *const c_void);
        let id = MusicalContextId::from_ref(context);
        if !self.known_context(id) {
            return;
        }
        let in_use = self
            .graph()
            .musical_context(id)
            .is_some_and(|c| !c.region_sequences.is_empty());
        if !check!(
            !in_use,
            State,
            context,
            c"musical context destroyed while region sequences use it"
        ) {
            return;
        }
        self.close();
        self.hook(Scope::Nothing, |c, cx| {
            c.will_destroy_musical_context(cx, id)
        });
        let g = self.graph_mut();
        g.musical_contexts.remove(&id.0);
        g.context_order.retain(|c| *c != id);
        self.open();
    }

    fn known_context(&self, id: MusicalContextId) -> bool {
        check!(
            self.graph().musical_context(id).is_some(),
            Argument,
            id.0 as *const u8,
            c"unknown musical context"
        )
    }

    // ---- region sequences ----

    unsafe fn create_region_sequence(
        &mut self,
        host_ref: sys::ARARegionSequenceHostRef,
        properties: *const sys::ARARegionSequenceProperties,
    ) -> sys::ARARegionSequenceRef {
        self.require_editing(properties as *const c_void);
        if !valid(
            properties,
            sys::kARARegionSequencePropertiesMinSize as usize,
        ) {
            return std::ptr::null_mut();
        }
        let (properties, context) = unsafe { graph::region_sequence_properties(properties) };
        if !self.known_context(context) {
            return std::ptr::null_mut();
        }
        self.close();
        let _ = host_ref; // ARA never asks the plug-in to name a sequence back to the host.
        let mut node = Box::new(RegionSequence {
            id: RegionSequenceId(0),
            properties,
            playback_regions: Vec::new(),
        });
        let id = RegionSequenceId(std::ptr::from_ref(&*node) as usize);
        node.id = id;
        let g = self.graph_mut();
        g.region_sequences.insert(id.0, node);
        g.sequence_order.push(id);
        if let Some(c) = g.musical_contexts.get_mut(&context.0) {
            c.region_sequences.push(id);
        }
        self.hook(Scope::Nothing, |c, cx| c.did_create_region_sequence(cx, id));
        self.open();
        id.to_ref()
    }

    unsafe fn update_region_sequence_properties(
        &mut self,
        sequence: sys::ARARegionSequenceRef,
        properties: *const sys::ARARegionSequenceProperties,
    ) {
        self.require_editing(properties as *const c_void);
        let id = RegionSequenceId::from_ref(sequence);
        if !self.known_sequence(id)
            || !valid(
                properties,
                sys::kARARegionSequencePropertiesMinSize as usize,
            )
        {
            return;
        }
        let (new, context) = unsafe { graph::region_sequence_properties(properties) };
        if !self.known_context(context) {
            return;
        }
        self.close();
        let g = self.graph_mut();
        let old_context = g
            .region_sequences
            .get(&id.0)
            .map(|s| s.properties.musical_context);
        if old_context != Some(context) {
            if let Some(c) = old_context.and_then(|c| g.musical_contexts.get_mut(&c.0)) {
                c.region_sequences.retain(|s| *s != id);
            }
            if let Some(c) = g.musical_contexts.get_mut(&context.0) {
                c.region_sequences.push(id);
            }
        }
        if let Some(s) = g.region_sequences.get_mut(&id.0) {
            s.properties = new;
        }
        self.hook(Scope::Nothing, |c, cx| {
            c.did_update_region_sequence_properties(cx, id)
        });
        self.open();
    }

    fn destroy_region_sequence(&mut self, sequence: sys::ARARegionSequenceRef) {
        self.require_editing(sequence as *const c_void);
        let id = RegionSequenceId::from_ref(sequence);
        if !self.known_sequence(id) {
            return;
        }
        let in_use = self
            .graph()
            .region_sequence(id)
            .is_some_and(|s| !s.playback_regions.is_empty());
        if !check!(
            !in_use,
            State,
            sequence,
            c"region sequence destroyed while playback regions use it"
        ) {
            return;
        }
        self.close();
        self.hook(Scope::Nothing, |c, cx| {
            c.will_destroy_region_sequence(cx, id)
        });
        self.each_extension(|e| e.forget_region_sequence(id));
        let g = self.graph_mut();
        if let Some(context) = g
            .region_sequences
            .remove(&id.0)
            .map(|s| s.properties.musical_context)
            && let Some(c) = g.musical_contexts.get_mut(&context.0)
        {
            c.region_sequences.retain(|s| *s != id);
        }
        g.sequence_order.retain(|s| *s != id);
        self.open();
    }

    fn known_sequence(&self, id: RegionSequenceId) -> bool {
        check!(
            self.graph().region_sequence(id).is_some(),
            Argument,
            id.0 as *const u8,
            c"unknown region sequence"
        )
    }

    // ---- audio sources ----

    unsafe fn create_audio_source(
        &mut self,
        host_ref: sys::ARAAudioSourceHostRef,
        properties: *const sys::ARAAudioSourceProperties,
    ) -> sys::ARAAudioSourceRef {
        self.require_editing(properties as *const c_void);
        if !valid(properties, sys::kARAAudioSourcePropertiesMinSize as usize) {
            return std::ptr::null_mut();
        }
        let properties = unsafe { graph::audio_source_properties(properties) };
        self.close();
        let mut node = Box::new(AudioSource::<C> {
            id: AudioSourceId(0),
            host_ref: host_ref as usize,
            properties,
            sample_access_enabled: false,
            deactivated: false,
            audio_modifications: Vec::new(),
            kept_archive: None,
            data: Default::default(),
        });
        let id = AudioSourceId(std::ptr::from_ref(&*node) as usize);
        node.id = id;
        let g = self.graph_mut();
        g.audio_sources.insert(id.0, node);
        g.source_order.push(id);
        self.hook(Scope::Source(id), |c, cx| c.did_create_audio_source(cx, id));
        self.open();
        id.to_ref()
    }

    unsafe fn update_audio_source_properties(
        &mut self,
        source: sys::ARAAudioSourceRef,
        properties: *const sys::ARAAudioSourceProperties,
    ) {
        self.require_editing(properties as *const c_void);
        let id = AudioSourceId::from_ref(source);
        if !self.known_source(id)
            || !valid(properties, sys::kARAAudioSourcePropertiesMinSize as usize)
        {
            return;
        }
        let new = unsafe { graph::audio_source_properties(properties) };
        let (format_changed, enabled) = self.graph().audio_source(id).map_or((false, false), |s| {
            let p = &s.properties;
            (
                p.sample_rate != new.sample_rate
                    || p.sample_count != new.sample_count
                    || p.channel_count != new.channel_count,
                s.sample_access_enabled,
            )
        });
        check!(
            !(format_changed && enabled),
            State,
            source,
            c"audio source format changed while its sample access is enabled"
        );
        self.close();
        self.hook(Scope::Source(id), |c, cx| {
            c.will_update_audio_source_properties(cx, id, &new)
        });
        if format_changed {
            self.cancel_readers(Some(id));
        }
        if let Some(s) = self.graph_mut().audio_source_mut(id) {
            s.properties = new;
        }
        self.hook(Scope::Source(id), |c, cx| {
            c.did_update_audio_source_properties(cx, id)
        });
        self.open();
        self.reap_readers(Some(id));
    }

    unsafe fn update_audio_source_content(
        &mut self,
        source: sys::ARAAudioSourceRef,
        range: *const sys::ARAContentTimeRange,
        flags: sys::ARAContentUpdateFlags,
    ) {
        self.require_editing(source as *const c_void);
        let id = AudioSourceId::from_ref(source);
        if !self.known_source(id) {
            return;
        }
        let range = unsafe { TimeRange::from_ptr(range) };
        self.reap_readers(Some(id));
        self.hook(Scope::Source(id), |c, cx| {
            c.did_update_audio_source_content(cx, id, range, ContentUpdateFlags(flags))
        });
    }

    fn enable_audio_source_samples_access(&mut self, source: sys::ARAAudioSourceRef, enable: bool) {
        let id = AudioSourceId::from_ref(source);
        if !self.known_source(id) {
            return;
        }
        if self
            .graph()
            .audio_source(id)
            .is_some_and(|s| s.sample_access_enabled == enable)
        {
            return;
        }
        self.hook(Scope::Source(id), |c, cx| {
            c.will_enable_audio_source_samples_access(cx, id, enable)
        });
        if !enable {
            // Cancel, join the read under way, destroy: before the host's call returns.
            self.cancel_readers(Some(id));
        }
        self.close();
        if let Some(s) = self.graph_mut().audio_source_mut(id) {
            s.sample_access_enabled = enable;
        }
        self.open();
        self.reap_readers(Some(id));
        self.hook(Scope::Source(id), |c, cx| {
            c.did_enable_audio_source_samples_access(cx, id, enable)
        });
        if !enable {
            // A reader the hook made despite the disabled access is cancelled too.
            self.cancel_readers(Some(id));
        }
    }

    fn deactivate_audio_source_for_undo_history(
        &mut self,
        source: sys::ARAAudioSourceRef,
        deactivate: bool,
    ) {
        self.require_editing(source as *const c_void);
        let id = AudioSourceId::from_ref(source);
        if !self.known_source(id) {
            return;
        }
        let Some(s) = self.graph().audio_source(id) else {
            return;
        };
        if s.deactivated == deactivate {
            return;
        }
        if deactivate {
            let g = self.graph();
            let all = s
                .audio_modifications
                .iter()
                .all(|m| g.audio_modification(*m).is_some_and(|m| m.deactivated));
            check!(
                all,
                State,
                source,
                c"audio source deactivated before its audio modifications"
            );
        }
        self.close();
        if let Some(s) = self.graph_mut().audio_source_mut(id) {
            s.deactivated = deactivate;
        }
        self.hook(Scope::Source(id), |c, cx| {
            c.did_deactivate_audio_source_for_undo_history(cx, id, deactivate)
        });
        self.open();
    }

    fn destroy_audio_source(&mut self, source: sys::ARAAudioSourceRef) {
        self.require_editing(source as *const c_void);
        let id = AudioSourceId::from_ref(source);
        if !self.known_source(id) {
            return;
        }
        let in_use = self
            .graph()
            .audio_source(id)
            .is_some_and(|s| !s.audio_modifications.is_empty());
        if !check!(
            !in_use,
            State,
            source,
            c"audio source destroyed before its audio modifications"
        ) {
            return;
        }
        self.hook(Scope::Source(id), |c, cx| {
            c.will_destroy_audio_source(cx, id)
        });
        self.cancel_readers(Some(id));
        self.close();
        let g = self.graph_mut();
        g.audio_sources.remove(&id.0);
        g.source_order.retain(|s| *s != id);
        self.open();
        self.notifier().retain(|n| !matches!(n, Notification::Progress { source, .. } | Notification::SourceContent { source, .. } if *source == id));
    }

    fn known_source(&self, id: AudioSourceId) -> bool {
        check!(
            self.graph().audio_source(id).is_some(),
            Argument,
            id.0 as *const u8,
            c"unknown audio source"
        )
    }

    // ---- audio modifications ----

    unsafe fn create_audio_modification(
        &mut self,
        source: sys::ARAAudioSourceRef,
        host_ref: sys::ARAAudioModificationHostRef,
        properties: *const sys::ARAAudioModificationProperties,
        cloned_from: Option<AudioModificationId>,
    ) -> sys::ARAAudioModificationRef {
        self.require_editing(properties as *const c_void);
        let source = AudioSourceId::from_ref(source);
        if !self.known_source(source)
            || !valid(
                properties,
                sys::kARAAudioModificationPropertiesMinSize as usize,
            )
        {
            return std::ptr::null_mut();
        }
        let properties = unsafe { graph::audio_modification_properties(properties) };
        let kept = cloned_from
            .and_then(|m| self.graph().audio_modification(m))
            .and_then(|m| m.kept_archive.clone());
        self.close();
        let mut node = Box::new(AudioModification::<C> {
            id: AudioModificationId(0),
            host_ref: host_ref as usize,
            audio_source: source,
            properties,
            deactivated: false,
            playback_regions: Vec::new(),
            kept_archive: kept,
            data: Default::default(),
        });
        let id = AudioModificationId(std::ptr::from_ref(&*node) as usize);
        node.id = id;
        let g = self.graph_mut();
        g.audio_modifications.insert(id.0, node);
        if let Some(s) = g.audio_sources.get_mut(&source.0) {
            s.audio_modifications.push(id);
        }
        self.hook(Scope::Nothing, |c, cx| {
            c.did_create_audio_modification(cx, id, cloned_from)
        });
        self.open();
        id.to_ref()
    }

    unsafe fn clone_audio_modification(
        &mut self,
        modification: sys::ARAAudioModificationRef,
        host_ref: sys::ARAAudioModificationHostRef,
        properties: *const sys::ARAAudioModificationProperties,
    ) -> sys::ARAAudioModificationRef {
        let original = AudioModificationId::from_ref(modification);
        if !self.known_modification(original) {
            return std::ptr::null_mut();
        }
        let source = self
            .graph()
            .audio_modification(original)
            .map(|m| m.audio_source.to_ref());
        let Some(source) = source else {
            return std::ptr::null_mut();
        };
        unsafe { self.create_audio_modification(source, host_ref, properties, Some(original)) }
    }

    unsafe fn update_audio_modification_properties(
        &mut self,
        modification: sys::ARAAudioModificationRef,
        properties: *const sys::ARAAudioModificationProperties,
    ) {
        self.require_editing(properties as *const c_void);
        let id = AudioModificationId::from_ref(modification);
        if !self.known_modification(id)
            || !valid(
                properties,
                sys::kARAAudioModificationPropertiesMinSize as usize,
            )
        {
            return;
        }
        let new = unsafe { graph::audio_modification_properties(properties) };
        self.close();
        if let Some(m) = self.graph_mut().audio_modification_mut(id) {
            m.properties = new;
        }
        self.hook(Scope::Nothing, |c, cx| {
            c.did_update_audio_modification_properties(cx, id)
        });
        self.open();
    }

    fn deactivate_audio_modification_for_undo_history(
        &mut self,
        modification: sys::ARAAudioModificationRef,
        deactivate: bool,
    ) {
        self.require_editing(modification as *const c_void);
        let id = AudioModificationId::from_ref(modification);
        if !self.known_modification(id) {
            return;
        }
        let Some(m) = self.graph().audio_modification(id) else {
            return;
        };
        if m.deactivated == deactivate {
            return;
        }
        check!(
            !deactivate || m.playback_regions.is_empty(),
            State,
            modification,
            c"audio modification deactivated before its playback regions were destroyed"
        );
        self.close();
        if let Some(m) = self.graph_mut().audio_modification_mut(id) {
            m.deactivated = deactivate;
        }
        self.hook(Scope::Nothing, |c, cx| {
            c.did_deactivate_audio_modification_for_undo_history(cx, id, deactivate)
        });
        self.open();
    }

    fn destroy_audio_modification(&mut self, modification: sys::ARAAudioModificationRef) {
        self.require_editing(modification as *const c_void);
        let id = AudioModificationId::from_ref(modification);
        if !self.known_modification(id) {
            return;
        }
        let in_use = self
            .graph()
            .audio_modification(id)
            .is_some_and(|m| !m.playback_regions.is_empty());
        if !check!(
            !in_use,
            State,
            modification,
            c"audio modification destroyed before its playback regions"
        ) {
            return;
        }
        self.close();
        self.hook(Scope::Nothing, |c, cx| {
            c.will_destroy_audio_modification(cx, id)
        });
        let g = self.graph_mut();
        if let Some(source) = g.audio_modifications.remove(&id.0).map(|m| m.audio_source)
            && let Some(s) = g.audio_sources.get_mut(&source.0)
        {
            s.audio_modifications.retain(|m| *m != id);
        }
        self.open();
        self.notifier().retain(|n| !matches!(n, Notification::ModificationContent { modification, .. } if *modification == id));
    }

    fn known_modification(&self, id: AudioModificationId) -> bool {
        check!(
            self.graph().audio_modification(id).is_some(),
            Argument,
            id.0 as *const u8,
            c"unknown audio modification"
        )
    }

    // ---- playback regions ----

    /// The region's properties, its sequence checked: `None` (reported) if they can't be used.
    unsafe fn region_properties(
        &self,
        properties: *const sys::ARAPlaybackRegionProperties,
    ) -> Option<PlaybackRegionProperties> {
        if !valid(
            properties,
            sys::kARAPlaybackRegionPropertiesMinSize as usize,
        ) {
            return None;
        }
        let mut p = unsafe { graph::playback_region_properties(properties) };
        check!(
            p.region_sequence.is_some(),
            Argument,
            properties,
            c"playback region without a region sequence (required since ARA 2.0)"
        );
        if let Some(s) = p.region_sequence
            && !self.known_sequence(s)
        {
            p.region_sequence = None;
        }
        check!(
            p.transformation_flags
                & !self
                    .shared
                    .factory
                    .config
                    .supported_playback_transformation_flags
                == 0,
            Argument,
            properties,
            c"playback region uses a transformation the factory doesn't support"
        );
        Some(p)
    }

    unsafe fn create_playback_region(
        &mut self,
        modification: sys::ARAAudioModificationRef,
        host_ref: sys::ARAPlaybackRegionHostRef,
        properties: *const sys::ARAPlaybackRegionProperties,
    ) -> sys::ARAPlaybackRegionRef {
        self.require_editing(properties as *const c_void);
        let modification = AudioModificationId::from_ref(modification);
        if !self.known_modification(modification) {
            return std::ptr::null_mut();
        }
        let Some(properties) = (unsafe { self.region_properties(properties) }) else {
            return std::ptr::null_mut();
        };
        let Some(source) = self
            .graph()
            .audio_modification(modification)
            .map(|m| m.audio_source)
        else {
            return std::ptr::null_mut();
        };
        self.close();
        let sequence = properties.region_sequence;
        let b = RegionBox::new(|id| PlaybackRegion {
            id,
            host_ref: host_ref as usize,
            audio_modification: modification,
            audio_source: source,
            properties,
            data: Default::default(),
        });
        let id = b.id();
        let g = self.graph_mut();
        g.playback_regions.insert(id.0, b);
        if let Some(m) = g.audio_modifications.get_mut(&modification.0) {
            m.playback_regions.push(id);
        }
        if let Some(s) = sequence.and_then(|s| g.region_sequences.get_mut(&s.0)) {
            s.playback_regions.push(id);
        }
        self.hook(Scope::Nothing, |c, cx| c.did_create_playback_region(cx, id));
        self.refresh_head_and_tail(Some(id));
        self.open();
        id.to_ref()
    }

    unsafe fn update_playback_region_properties(
        &mut self,
        region: sys::ARAPlaybackRegionRef,
        properties: *const sys::ARAPlaybackRegionProperties,
    ) {
        self.require_editing(properties as *const c_void);
        let id = PlaybackRegionId::from_ref(region);
        if !self.known_region(id) {
            return;
        }
        let Some(new) = (unsafe { self.region_properties(properties) }) else {
            return;
        };
        self.close();
        self.hook(Scope::Nothing, |c, cx| {
            c.will_update_playback_region_properties(cx, id, &new)
        });
        let g = self.graph_mut();
        let old_sequence = g
            .playback_region(id)
            .and_then(|r| r.properties.region_sequence);
        if old_sequence != new.region_sequence {
            if let Some(s) = old_sequence.and_then(|s| g.region_sequences.get_mut(&s.0)) {
                s.playback_regions.retain(|r| *r != id);
            }
            if let Some(s) = new
                .region_sequence
                .and_then(|s| g.region_sequences.get_mut(&s.0))
            {
                s.playback_regions.push(id);
            }
        }
        if let Some(r) = g.playback_region_mut(id) {
            r.properties = new;
        }
        self.hook(Scope::Nothing, |c, cx| {
            c.did_update_playback_region_properties(cx, id)
        });
        self.refresh_head_and_tail(Some(id));
        self.open();
    }

    fn destroy_playback_region(&mut self, region: sys::ARAPlaybackRegionRef) {
        self.require_editing(region as *const c_void);
        let id = PlaybackRegionId::from_ref(region);
        if !self.known_region(id) {
            return;
        }
        let mut referenced = false;
        self.each_extension(|e| referenced |= e.renders_region(id));
        check!(
            !referenced,
            State,
            region,
            c"playback region destroyed while a renderer still has it"
        );
        self.close();
        self.hook(Scope::Nothing, |c, cx| {
            c.will_destroy_playback_region(cx, id)
        });
        // With the graph's guard closed no renderer reads the extensions' lists.
        self.each_extension(|e| e.forget_region(id));
        let g = self.graph_mut();
        if let Some(b) = g.playback_regions.remove(&id.0) {
            let (m, s) = (
                b.region().audio_modification,
                b.region().properties.region_sequence,
            );
            if let Some(m) = g.audio_modifications.get_mut(&m.0) {
                m.playback_regions.retain(|r| *r != id);
            }
            if let Some(s) = s.and_then(|s| g.region_sequences.get_mut(&s.0)) {
                s.playback_regions.retain(|r| *r != id);
            }
        }
        self.open();
        self.notifier()
            .retain(|n| !matches!(n, Notification::RegionContent { region, .. } if *region == id));
    }

    fn known_region(&self, id: PlaybackRegionId) -> bool {
        check!(
            self.graph().playback_region(id).is_some(),
            Argument,
            id.0 as *const u8,
            c"unknown playback region"
        )
    }

    // ---- content the plug-in offers ----

    fn content_grade(&self, target: Target, ty: sys::ARAContentType) -> Option<ContentGrade> {
        let ty = ContentType::from_raw(ty)?;
        let c = self.controller()?;
        let g = self.graph();
        match target {
            Target::Source(id) => self
                .known_source(id)
                .then(|| c.audio_source_content_grade(g, id, ty))
                .flatten(),
            Target::Modification(id) => self
                .known_modification(id)
                .then(|| c.audio_modification_content_grade(g, id, ty))
                .flatten(),
            Target::Region(id) => self
                .known_region(id)
                .then(|| c.playback_region_content_grade(g, id, ty))
                .flatten(),
        }
    }

    unsafe fn create_content_reader(
        &mut self,
        target: Target,
        ty: sys::ARAContentType,
        range: *const sys::ARAContentTimeRange,
    ) -> sys::ARAContentReaderRef {
        let available = self.content_grade(target, ty).is_some();
        if !check!(
            available,
            State,
            NULL,
            c"content reader created for content that isn't available"
        ) {
            return std::ptr::null_mut();
        }
        let Some(ty) = ContentType::from_raw(ty) else {
            return std::ptr::null_mut();
        };
        let range = unsafe { TimeRange::from_ptr(range) };
        let Some(c) = self.controller() else {
            return std::ptr::null_mut();
        };
        let g = self.graph();
        let events = match target {
            Target::Source(id) => c.audio_source_content(g, id, ty, range),
            Target::Modification(id) => c.audio_modification_content(g, id, ty, range),
            Target::Region(id) => c.playback_region_content(g, id, ty, range),
        };
        let Some(events) = events.filter(|e| e.content_type() == ty) else {
            return std::ptr::null_mut();
        };
        let reader = Box::into_raw(Box::new(PlugInContentReader::new(events)));
        self.model.content_readers.insert(reader as usize);
        reader.cast()
    }

    fn content_reader(&self, reader: sys::ARAContentReaderRef) -> Option<&PlugInContentReader> {
        let live = self.model.content_readers.contains(&(reader as usize));
        // SAFETY: a live reader this controller made and hasn't destroyed.
        check!(live, Argument, reader, c"unknown content reader")
            .then(|| unsafe { &*(reader as *const PlugInContentReader) })
    }

    fn destroy_content_reader(&mut self, reader: sys::ARAContentReaderRef) {
        if check!(
            self.model.content_readers.remove(&(reader as usize)),
            Argument,
            reader,
            c"unknown content reader"
        ) {
            // SAFETY: made by `Box::into_raw` in `create_content_reader`, freed once.
            drop(unsafe { Box::from_raw(reader as *mut PlugInContentReader) });
        }
    }

    // ---- archives ----

    unsafe fn restore_objects_from_archive(
        &mut self,
        reader: sys::ARAArchiveReaderHostRef,
        filter: *const sys::ARARestoreObjectsFilter,
    ) -> bool {
        self.require_editing(filter as *const c_void);
        self.require_no_content_readers();
        let archiving = self.model.host.archiving;
        let archive = ArchiveReader::new(&archiving, reader);
        if let Some(id) = archive.document_archive_id() {
            let config = &self.shared.factory.config;
            let ours = id == config.document_archive_id
                || config
                    .compatible_document_archive_ids
                    .contains(&id.as_str());
            check!(
                ours,
                State,
                reader,
                c"archive of a document archive ID this factory doesn't read"
            );
        }
        let Some(bytes) = archive.read_all() else {
            return false;
        };
        let Ok(entries) = archive::decode(&bytes) else {
            archive.progress(1.0);
            return false;
        };
        let selection = unsafe { RestoreSelection::new(self.graph(), filter) };
        self.close();
        let mut ok = true;
        let total = entries.len().max(1) as f32;
        for kind in [archive::KIND_AUDIO_SOURCE, archive::KIND_AUDIO_MODIFICATION] {
            for (i, e) in entries.iter().enumerate().filter(|(_, e)| e.kind == kind) {
                ok &= self.restore_entry(e, &selection);
                archive.progress(0.5 + 0.5 * i as f32 / total);
            }
        }
        if selection.document_data {
            for e in entries.iter().filter(|e| e.kind == archive::KIND_DOCUMENT) {
                ok &= self.restore_entry(e, &selection);
            }
            self.model.unknown_entries = entries
                .into_iter()
                .filter(|e| {
                    ![
                        archive::KIND_DOCUMENT,
                        archive::KIND_AUDIO_SOURCE,
                        archive::KIND_AUDIO_MODIFICATION,
                    ]
                    .contains(&e.kind)
                })
                .collect();
        }
        self.open();
        archive.progress(1.0);
        ok
    }

    fn restore_entry(&mut self, e: &Entry, selection: &RestoreSelection) -> bool {
        let object = match e.kind {
            archive::KIND_AUDIO_SOURCE => match selection.sources.get(&e.id) {
                Some(id) => ArchivedObject::AudioSource(*id),
                None => return true,
            },
            archive::KIND_AUDIO_MODIFICATION => match selection.modifications.get(&e.id) {
                Some(id) => ArchivedObject::AudioModification(*id),
                None => return true,
            },
            _ => ArchivedObject::Document,
        };
        let newer = e.payload.schema > C::ARCHIVE_SCHEMA;
        let kept = newer.then(|| e.payload.clone());
        match object {
            ArchivedObject::AudioSource(id) => {
                if let Some(s) = self.graph_mut().audio_source_mut(id) {
                    s.kept_archive = kept;
                }
            }
            ArchivedObject::AudioModification(id) => {
                if let Some(m) = self.graph_mut().audio_modification_mut(id) {
                    m.kept_archive = kept;
                }
            }
            ArchivedObject::Document => self.model.kept_document = kept,
        }
        if newer {
            self.hook(Scope::Nothing, |c, cx| c.did_keep_newer_archive(cx, object));
            return true;
        }
        let payload = &e.payload;
        self.hook(Scope::Nothing, |c, cx| match object {
            ArchivedObject::AudioSource(id) => c.restore_audio_source(cx, id, payload),
            ArchivedObject::AudioModification(id) => c.restore_audio_modification(cx, id, payload),
            ArchivedObject::Document => c.restore_document_data(cx, payload),
        })
        .unwrap_or(false)
    }

    /// The entries a store writes: kept newer states verbatim, the controller's payloads else.
    fn store_entries(&mut self, selection: &StoreSelection) -> Vec<Entry> {
        let mut entries = Vec::new();
        for &id in &selection.sources {
            let Some(s) = self.graph().audio_source(id) else {
                continue;
            };
            let key = s.persistent_id().to_owned();
            let payload = match s.kept_archive.clone() {
                Some(kept) => Some(kept),
                None => self
                    .hook(Scope::Nothing, |c, cx| c.store_audio_source(cx, id))
                    .flatten(),
            };
            if let Some(payload) = payload {
                entries.push(Entry {
                    kind: archive::KIND_AUDIO_SOURCE,
                    id: key,
                    payload,
                });
            }
        }
        for &id in &selection.modifications {
            let Some(m) = self.graph().audio_modification(id) else {
                continue;
            };
            let key = m.persistent_id().to_owned();
            let payload = match m.kept_archive.clone() {
                Some(kept) => Some(kept),
                None => self
                    .hook(Scope::Nothing, |c, cx| c.store_audio_modification(cx, id))
                    .flatten(),
            };
            if let Some(payload) = payload {
                entries.push(Entry {
                    kind: archive::KIND_AUDIO_MODIFICATION,
                    id: key,
                    payload,
                });
            }
        }
        if selection.document_data {
            let payload = match self.model.kept_document.clone() {
                Some(kept) => Some(kept),
                None => self
                    .hook(Scope::Nothing, |c, cx| c.store_document_data(cx))
                    .flatten(),
            };
            if let Some(payload) = payload {
                entries.push(Entry {
                    kind: archive::KIND_DOCUMENT,
                    id: String::new(),
                    payload,
                });
            }
            entries.extend(self.model.unknown_entries.iter().cloned());
        }
        entries
    }

    fn write_entries(&self, writer: sys::ARAArchiveWriterHostRef, entries: &[Entry]) -> bool {
        let archiving = self.model.host.archiving;
        let mut w = ArchiveWriter::new(&archiving, writer);
        w.write(&archive::header(entries.len()));
        for (i, e) in entries.iter().enumerate() {
            w.write(&archive::entry_head(e));
            w.write(&e.payload.bytes);
            w.progress((i + 1) as f32 / entries.len() as f32);
        }
        w.progress(1.0);
        w.ok()
    }

    unsafe fn store_objects_to_archive(
        &mut self,
        writer: sys::ARAArchiveWriterHostRef,
        filter: *const sys::ARAStoreObjectsFilter,
    ) -> bool {
        check!(
            !self.model.editing,
            State,
            writer,
            c"storeObjectsToArchive while editing"
        );
        self.require_no_content_readers();
        let selection = unsafe { StoreSelection::new(self.graph(), filter) };
        let entries = self.store_entries(&selection);
        self.write_entries(writer, &entries)
    }

    unsafe fn store_audio_source_to_audio_file_chunk(
        &mut self,
        writer: sys::ARAArchiveWriterHostRef,
        source: sys::ARAAudioSourceRef,
        archive_id: *mut sys::ARAPersistentID,
        open_automatically: *mut sys::ARABool,
    ) -> bool {
        let fb = self.shared.factory;
        if !check!(
            fb.config.supports_storing_audio_file_chunks,
            State,
            writer,
            c"audio file chunks not supported by this factory"
        ) || !check!(
            !archive_id.is_null() && !open_automatically.is_null(),
            Argument,
            archive_id,
            c"null out-parameter"
        ) {
            return false;
        }
        check!(
            !self.model.editing,
            State,
            writer,
            c"storeAudioSourceToAudioFileChunk while editing"
        );
        let id = AudioSourceId::from_ref(source);
        if !self.known_source(id) {
            return false;
        }
        let selection = StoreSelection {
            document_data: false,
            sources: vec![id],
            modifications: Vec::new(),
        };
        let entries = self.store_entries(&selection);
        // SAFETY: both out-parameters checked non-null; the ID lives as long as the library.
        unsafe {
            *archive_id = fb.factory.documentArchiveID;
            *open_automatically = sys::kARAFalse;
        }
        self.write_entries(writer, &entries)
    }

    // ---- teardown ----

    fn destroy(&mut self) {
        check!(
            self.graph().is_empty(),
            State,
            NULL,
            c"destroyDocumentController with model objects alive"
        );
        self.require_no_content_readers();
        self.cancel_readers(None);
        for reader in std::mem::take(&mut self.model.content_readers) {
            // SAFETY: made by `Box::into_raw` in `create_content_reader`, freed once.
            drop(unsafe { Box::from_raw(reader as *mut PlugInContentReader) });
        }
        // Renderers stay out for good.
        self.close();
        drop(self.model.controller.take());
        let graph = std::mem::replace(self.graph_mut(), Graph::new(DocumentProperties::default()));
        drop(graph);
        self.shared.destroyed.store(true, Ordering::Release);
    }
}

#[derive(Clone, Copy)]
enum Target {
    Source(AudioSourceId),
    Modification(AudioModificationId),
    Region(PlaybackRegionId),
}

/// A host struct pointer: non-null, with at least `min` bytes (reported otherwise).
fn valid<S>(p: *const S, min: usize) -> bool {
    // SAFETY: non-null is checked first; a host struct has at least its `structSize` bytes.
    check!(
        !p.is_null() && unsafe { struct_size(p) } >= min,
        Argument,
        p,
        c"properties struct missing or too small"
    )
}

fn content_types(count: usize, types: *const sys::ARAContentType) -> Vec<ContentType> {
    if count == 0 || types.is_null() {
        return Vec::new();
    }
    // SAFETY: the host's array of `count` types, valid for the call.
    unsafe { std::slice::from_raw_parts(types, count) }
        .iter()
        .filter_map(|t| ContentType::from_raw(*t))
        .collect()
}

// ---- creation and the C entry points ---------------------------------------------------------

pub(crate) unsafe extern "C" fn create_document_controller<C: Controller>(
    host: *const sys::ARADocumentControllerHostInstance,
    properties: *const sys::ARADocumentProperties,
) -> *const sys::ARADocumentControllerInstance {
    let fb = factory_box::<C>();
    let api_generation = fb.generation.load(Ordering::SeqCst);
    if !check!(
        api_generation != 0,
        State,
        host,
        c"createDocumentControllerWithDocument before initializeARAWithConfiguration"
    ) {
        return std::ptr::null();
    }
    // SAFETY: the host's structs are valid for the call; each is size-checked before use.
    let Some(host_interfaces) = (unsafe { HostInterfaces::from_host(host) }) else {
        return std::ptr::null();
    };
    if !valid(
        properties,
        implemented_size!(sys::ARADocumentProperties, name),
    ) {
        return std::ptr::null();
    }
    // SAFETY: as above.
    let document = unsafe { graph::document_properties(properties) };
    let notifier = Notifier::default();
    let made = catch_unwind(AssertUnwindSafe(|| {
        C::new(Init {
            document: &document,
            notifier: notifier.clone(),
            api_generation,
        })
    }));
    let Ok(controller) = made else {
        report(
            Category::State,
            std::ptr::null(),
            c"the controller panicked while being created",
        );
        return std::ptr::null();
    };

    let algorithm_strings: Vec<CString> = C::PROCESSING_ALGORITHMS
        .iter()
        .flat_map(|a| [c_string(a.persistent_id), c_string(a.name)])
        .collect();
    let algorithm_properties = algorithm_strings
        .chunks(2)
        .map(|pair| sys::ARAProcessingAlgorithmProperties {
            structSize: implemented_size!(sys::ARAProcessingAlgorithmProperties, name),
            persistentID: pair[0].as_ptr(),
            name: pair[1].as_ptr(),
        })
        .collect();

    let shared = Arc::new(DocShared {
        guard: AccessGuard::new(),
        graph: UnsafeCell::new(Graph::new(document)),
        model: UnsafeCell::new(ModelState {
            controller: Some(controller),
            host: host_interfaces,
            notifier,
            readers: Readers::default(),
            editing: false,
            content_readers: HashSet::new(),
            kept_document: None,
            unknown_entries: Vec::new(),
            _algorithm_strings: algorithm_strings,
            algorithm_properties,
            api_generation,
        }),
        busy: AtomicBool::new(false),
        destroyed: AtomicBool::new(false),
        poisoned: AtomicBool::new(false),
        model_thread: thread_mark(),
        extensions: Mutex::new(Vec::new()),
        factory: fb,
    });
    let handle = Box::into_raw(Box::new(DcHandle {
        // SAFETY: an all-zero instance (null pointers) is valid; it is filled in just below.
        instance: unsafe { std::mem::zeroed() },
        interface: interface::<C>(),
        shared,
    }));
    // SAFETY: `handle` is freshly allocated and not yet shared.
    unsafe {
        (*handle).instance = sys::ARADocumentControllerInstance {
            structSize: implemented_size!(
                sys::ARADocumentControllerInstance,
                documentControllerInterface
            ),
            documentControllerRef: handle.cast(),
            documentControllerInterface: &raw const (*handle).interface,
        };
    }
    CONTROLLERS.insert::<DcHandle<C>>(handle as usize);
    fb.live_controllers.fetch_add(1, Ordering::SeqCst);
    // SAFETY: as above.
    unsafe { &raw const (*handle).instance }
}

/// The live handle behind a controller ref, or `None` (reported).
fn handle<'a, C: Controller>(dc: sys::ARADocumentControllerRef) -> Option<&'a DcHandle<C>> {
    let live = CONTROLLERS.contains::<DcHandle<C>>(dc as usize);
    // SAFETY: a live handle of this controller type, freed only by `destroyDocumentController`
    // on the model thread, which cannot run during another model call.
    check!(live, Argument, dc, c"unknown document controller")
        .then(|| unsafe { &*(dc as *const DcHandle<C>) })
}

/// The shared document behind a controller ref, for binding an extension on the model thread.
pub(crate) fn shared_for_bind<C: Controller>(
    dc: sys::ARADocumentControllerRef,
) -> Option<Arc<DocShared<C>>> {
    let h = handle::<C>(dc)?;
    if !check!(
        thread_mark() == h.shared.model_thread,
        Thread,
        dc,
        c"plug-in bound off the document controller's thread"
    ) {
        return None;
    }
    Some(Arc::clone(&h.shared))
}

fn dispatch<C: Controller, R>(
    dc: sys::ARADocumentControllerRef,
    fallback: R,
    f: impl FnOnce(&mut Call<'_, C>) -> R,
) -> R {
    match handle::<C>(dc) {
        Some(h) => h.shared.call(fallback, f),
        None => fallback,
    }
}

fn ara(b: bool) -> sys::ARABool {
    crate::util::ara_bool(b)
}

// The entry points. Every pointer the host passes is valid for the call (ARA's contract); each
// is size-checked before its fields are read.

unsafe extern "C" fn destroy_document_controller<C: Controller>(dc: sys::ARADocumentControllerRef) {
    let Some(h) = handle::<C>(dc) else { return };
    if !check!(
        thread_mark() == h.shared.model_thread,
        Thread,
        dc,
        c"destroyDocumentController off its thread"
    ) {
        return;
    }
    if !check!(
        !h.shared.busy.load(Ordering::Acquire),
        State,
        dc,
        c"destroyDocumentController from inside a call"
    ) {
        return;
    }
    if h.shared.poisoned.load(Ordering::Acquire) {
        // Inert after a panic: free what is left without running the controller again.
        h.shared.poisoned.store(false, Ordering::Release);
        // SAFETY: the model thread, not busy.
        unsafe { (*h.shared.model.get()).controller = None };
    }
    h.shared.call((), |call| call.destroy());
    CONTROLLERS.remove(dc as usize);
    h.shared
        .factory
        .live_controllers
        .fetch_sub(1, Ordering::SeqCst);
    // SAFETY: made by `Box::into_raw` in `create_document_controller`, unregistered, freed once.
    // Extensions still bound keep the shared document alive.
    drop(unsafe { Box::from_raw(dc as *mut DcHandle<C>) });
}

unsafe extern "C" fn get_factory<C: Controller>(
    _dc: sys::ARADocumentControllerRef,
) -> *const sys::ARAFactory {
    crate::factory::factory::<C>()
}

unsafe extern "C" fn begin_editing<C: Controller>(dc: sys::ARADocumentControllerRef) {
    dispatch::<C, _>(dc, (), |c| c.begin_editing());
}

unsafe extern "C" fn end_editing<C: Controller>(dc: sys::ARADocumentControllerRef) {
    dispatch::<C, _>(dc, (), |c| c.end_editing());
}

unsafe extern "C" fn notify_model_updates<C: Controller>(dc: sys::ARADocumentControllerRef) {
    dispatch::<C, _>(dc, (), |c| c.notify_model_updates());
}

unsafe extern "C" fn begin_restoring_document_from_archive<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    _reader: sys::ARAArchiveReaderHostRef,
) -> sys::ARABool {
    // Deprecated since 2.0 final; mapped to its successors as ARA_Library maps it.
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        c.begin_editing();
        sys::kARATrue
    })
}

unsafe extern "C" fn end_restoring_document_from_archive<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    reader: sys::ARAArchiveReaderHostRef,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        let ok = unsafe { c.restore_objects_from_archive(reader, std::ptr::null()) };
        c.end_editing();
        ara(ok)
    })
}

unsafe extern "C" fn store_document_to_archive<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    writer: sys::ARAArchiveWriterHostRef,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        ara(unsafe { c.store_objects_to_archive(writer, std::ptr::null()) })
    })
}

unsafe extern "C" fn update_document_properties<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    properties: *const sys::ARADocumentProperties,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_document_properties(properties)
    });
}

unsafe extern "C" fn create_musical_context<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    host_ref: sys::ARAMusicalContextHostRef,
    properties: *const sys::ARAMusicalContextProperties,
) -> sys::ARAMusicalContextRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_musical_context(host_ref, properties)
    })
}

unsafe extern "C" fn update_musical_context_properties<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    context: sys::ARAMusicalContextRef,
    properties: *const sys::ARAMusicalContextProperties,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_musical_context_properties(context, properties)
    });
}

unsafe extern "C" fn update_musical_context_content<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    context: sys::ARAMusicalContextRef,
    range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_musical_context_content(context, range, flags)
    });
}

unsafe extern "C" fn destroy_musical_context<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    context: sys::ARAMusicalContextRef,
) {
    dispatch::<C, _>(dc, (), |c| c.destroy_musical_context(context));
}

unsafe extern "C" fn create_audio_source<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    host_ref: sys::ARAAudioSourceHostRef,
    properties: *const sys::ARAAudioSourceProperties,
) -> sys::ARAAudioSourceRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_audio_source(host_ref, properties)
    })
}

unsafe extern "C" fn update_audio_source_properties<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    properties: *const sys::ARAAudioSourceProperties,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_audio_source_properties(source, properties)
    });
}

unsafe extern "C" fn update_audio_source_content<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_audio_source_content(source, range, flags)
    });
}

unsafe extern "C" fn enable_audio_source_samples_access<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    enable: sys::ARABool,
) {
    dispatch::<C, _>(dc, (), |c| {
        c.enable_audio_source_samples_access(source, enable != sys::kARAFalse)
    });
}

unsafe extern "C" fn deactivate_audio_source_for_undo_history<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    deactivate: sys::ARABool,
) {
    dispatch::<C, _>(dc, (), |c| {
        c.deactivate_audio_source_for_undo_history(source, deactivate != sys::kARAFalse)
    });
}

unsafe extern "C" fn destroy_audio_source<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
) {
    dispatch::<C, _>(dc, (), |c| c.destroy_audio_source(source));
}

unsafe extern "C" fn create_audio_modification<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    host_ref: sys::ARAAudioModificationHostRef,
    properties: *const sys::ARAAudioModificationProperties,
) -> sys::ARAAudioModificationRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_audio_modification(source, host_ref, properties, None)
    })
}

unsafe extern "C" fn clone_audio_modification<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
    host_ref: sys::ARAAudioModificationHostRef,
    properties: *const sys::ARAAudioModificationProperties,
) -> sys::ARAAudioModificationRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.clone_audio_modification(modification, host_ref, properties)
    })
}

unsafe extern "C" fn update_audio_modification_properties<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
    properties: *const sys::ARAAudioModificationProperties,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_audio_modification_properties(modification, properties)
    });
}

unsafe extern "C" fn deactivate_audio_modification_for_undo_history<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
    deactivate: sys::ARABool,
) {
    dispatch::<C, _>(dc, (), |c| {
        c.deactivate_audio_modification_for_undo_history(modification, deactivate != sys::kARAFalse)
    });
}

unsafe extern "C" fn destroy_audio_modification<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
) {
    dispatch::<C, _>(dc, (), |c| c.destroy_audio_modification(modification));
}

unsafe extern "C" fn create_playback_region<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
    host_ref: sys::ARAPlaybackRegionHostRef,
    properties: *const sys::ARAPlaybackRegionProperties,
) -> sys::ARAPlaybackRegionRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_playback_region(modification, host_ref, properties)
    })
}

unsafe extern "C" fn update_playback_region_properties<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    region: sys::ARAPlaybackRegionRef,
    properties: *const sys::ARAPlaybackRegionProperties,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_playback_region_properties(region, properties)
    });
}

unsafe extern "C" fn destroy_playback_region<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    region: sys::ARAPlaybackRegionRef,
) {
    dispatch::<C, _>(dc, (), |c| c.destroy_playback_region(region));
}

unsafe extern "C" fn is_audio_source_content_available<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    ty: sys::ARAContentType,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        ara(c
            .content_grade(Target::Source(AudioSourceId::from_ref(source)), ty)
            .is_some())
    })
}

unsafe extern "C" fn is_audio_source_content_analysis_incomplete<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    ty: sys::ARAContentType,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        let id = AudioSourceId::from_ref(source);
        let incomplete = c.known_source(id)
            && ContentType::from_raw(ty)
                .zip(c.controller())
                .is_some_and(|(ty, ctl)| {
                    ctl.is_audio_source_content_analysis_incomplete(c.graph(), id, ty)
                });
        ara(incomplete)
    })
}

unsafe extern "C" fn request_audio_source_content_analysis<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    count: sys::ARASize,
    types: *const sys::ARAContentType,
) {
    dispatch::<C, _>(dc, (), |c| {
        let id = AudioSourceId::from_ref(source);
        if !c.known_source(id) {
            return;
        }
        let types = content_types(count, types);
        let analyzeable = c.shared.factory.config.analyzeable_content_types;
        check!(
            !types.is_empty() && types.iter().all(|t| analyzeable.contains(t)),
            Argument,
            source,
            c"analysis requested for content types the factory doesn't analyze"
        );
        c.hook(Scope::Nothing, |ctl, cx| {
            ctl.request_audio_source_content_analysis(cx, id, &types)
        });
    });
}

unsafe extern "C" fn get_audio_source_content_grade<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    ty: sys::ARAContentType,
) -> sys::ARAContentGrade {
    dispatch::<C, _>(dc, sys::kARAContentGradeInitial, |c| {
        c.content_grade(Target::Source(AudioSourceId::from_ref(source)), ty)
            .unwrap_or(ContentGrade::Initial)
            .raw()
    })
}

unsafe extern "C" fn create_audio_source_content_reader<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    ty: sys::ARAContentType,
    range: *const sys::ARAContentTimeRange,
) -> sys::ARAContentReaderRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_content_reader(Target::Source(AudioSourceId::from_ref(source)), ty, range)
    })
}

unsafe extern "C" fn is_audio_modification_content_available<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
    ty: sys::ARAContentType,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        ara(c
            .content_grade(
                Target::Modification(AudioModificationId::from_ref(modification)),
                ty,
            )
            .is_some())
    })
}

unsafe extern "C" fn get_audio_modification_content_grade<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
    ty: sys::ARAContentType,
) -> sys::ARAContentGrade {
    dispatch::<C, _>(dc, sys::kARAContentGradeInitial, |c| {
        c.content_grade(
            Target::Modification(AudioModificationId::from_ref(modification)),
            ty,
        )
        .unwrap_or(ContentGrade::Initial)
        .raw()
    })
}

unsafe extern "C" fn create_audio_modification_content_reader<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
    ty: sys::ARAContentType,
    range: *const sys::ARAContentTimeRange,
) -> sys::ARAContentReaderRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_content_reader(
            Target::Modification(AudioModificationId::from_ref(modification)),
            ty,
            range,
        )
    })
}

unsafe extern "C" fn is_playback_region_content_available<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    region: sys::ARAPlaybackRegionRef,
    ty: sys::ARAContentType,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        ara(c
            .content_grade(Target::Region(PlaybackRegionId::from_ref(region)), ty)
            .is_some())
    })
}

unsafe extern "C" fn get_playback_region_content_grade<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    region: sys::ARAPlaybackRegionRef,
    ty: sys::ARAContentType,
) -> sys::ARAContentGrade {
    dispatch::<C, _>(dc, sys::kARAContentGradeInitial, |c| {
        c.content_grade(Target::Region(PlaybackRegionId::from_ref(region)), ty)
            .unwrap_or(ContentGrade::Initial)
            .raw()
    })
}

unsafe extern "C" fn create_playback_region_content_reader<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    region: sys::ARAPlaybackRegionRef,
    ty: sys::ARAContentType,
    range: *const sys::ARAContentTimeRange,
) -> sys::ARAContentReaderRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_content_reader(
            Target::Region(PlaybackRegionId::from_ref(region)),
            ty,
            range,
        )
    })
}

unsafe extern "C" fn get_content_reader_event_count<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    reader: sys::ARAContentReaderRef,
) -> sys::ARAInt32 {
    dispatch::<C, _>(dc, 0, |c| {
        c.content_reader(reader)
            .map_or(0, |r| i32::try_from(r.len()).unwrap_or(i32::MAX))
    })
}

unsafe extern "C" fn get_content_reader_data_for_event<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    reader: sys::ARAContentReaderRef,
    index: sys::ARAInt32,
) -> *const c_void {
    dispatch::<C, _>(dc, std::ptr::null(), |c| {
        let Some(r) = c.content_reader(reader) else {
            return std::ptr::null();
        };
        let data = usize::try_from(index).map_or(std::ptr::null(), |i| r.event(i));
        check!(
            !data.is_null(),
            Argument,
            reader,
            c"content event index out of range"
        );
        data
    })
}

unsafe extern "C" fn destroy_content_reader<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    reader: sys::ARAContentReaderRef,
) {
    dispatch::<C, _>(dc, (), |c| c.destroy_content_reader(reader));
}

unsafe extern "C" fn create_region_sequence<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    host_ref: sys::ARARegionSequenceHostRef,
    properties: *const sys::ARARegionSequenceProperties,
) -> sys::ARARegionSequenceRef {
    dispatch::<C, _>(dc, std::ptr::null_mut(), |c| unsafe {
        c.create_region_sequence(host_ref, properties)
    })
}

unsafe extern "C" fn update_region_sequence_properties<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    sequence: sys::ARARegionSequenceRef,
    properties: *const sys::ARARegionSequenceProperties,
) {
    dispatch::<C, _>(dc, (), |c| unsafe {
        c.update_region_sequence_properties(sequence, properties)
    });
}

unsafe extern "C" fn destroy_region_sequence<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    sequence: sys::ARARegionSequenceRef,
) {
    dispatch::<C, _>(dc, (), |c| c.destroy_region_sequence(sequence));
}

/// Any thread, the audio thread included: it reads only the region's cell, never the graph or
/// a lock, and trusts the region ref, as ARA lets it.
unsafe extern "C" fn get_playback_region_head_and_tail_time<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    region: sys::ARAPlaybackRegionRef,
    head: *mut sys::ARATimeDuration,
    tail: *mut sys::ARATimeDuration,
) {
    if !check!(
        !dc.is_null() && !region.is_null() && !head.is_null() && !tail.is_null(),
        Argument,
        region,
        c"null argument"
    ) {
        return;
    }
    // SAFETY: a live region's ref (the host's contract); only its cell's atomics are read.
    let (h, t) =
        unsafe { RegionBox::<C>::cell_of(PlaybackRegionId::from_ref(region)) }.head_and_tail();
    // SAFETY: both checked non-null; the host's out-parameters.
    unsafe {
        head.write_unaligned(h);
        tail.write_unaligned(t);
    }
}

unsafe extern "C" fn restore_objects_from_archive<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    reader: sys::ARAArchiveReaderHostRef,
    filter: *const sys::ARARestoreObjectsFilter,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        ara(unsafe { c.restore_objects_from_archive(reader, filter) })
    })
}

unsafe extern "C" fn store_objects_to_archive<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    writer: sys::ARAArchiveWriterHostRef,
    filter: *const sys::ARAStoreObjectsFilter,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        ara(unsafe { c.store_objects_to_archive(writer, filter) })
    })
}

unsafe extern "C" fn get_processing_algorithms_count<C: Controller>(
    dc: sys::ARADocumentControllerRef,
) -> sys::ARAInt32 {
    dispatch::<C, _>(dc, 0, |c| c.model.algorithm_properties.len() as i32)
}

unsafe extern "C" fn get_processing_algorithm_properties<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    index: sys::ARAInt32,
) -> *const sys::ARAProcessingAlgorithmProperties {
    dispatch::<C, _>(dc, std::ptr::null(), |c| {
        let p = usize::try_from(index)
            .ok()
            .and_then(|i| c.model.algorithm_properties.get(i));
        check!(
            p.is_some(),
            Argument,
            NULL,
            c"processing algorithm index out of range"
        );
        p.map_or(std::ptr::null(), std::ptr::from_ref)
    })
}

unsafe extern "C" fn get_processing_algorithm_for_audio_source<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
) -> sys::ARAInt32 {
    dispatch::<C, _>(dc, 0, |c| {
        let id = AudioSourceId::from_ref(source);
        if !check!(
            !c.model.algorithm_properties.is_empty(),
            State,
            source,
            c"no processing algorithms offered"
        ) || !c.known_source(id)
        {
            return 0;
        }
        c.controller().map_or(0, |ctl| {
            ctl.processing_algorithm_for_audio_source(c.graph(), id)
        })
    })
}

unsafe extern "C" fn request_processing_algorithm_for_audio_source<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    source: sys::ARAAudioSourceRef,
    index: sys::ARAInt32,
) {
    dispatch::<C, _>(dc, (), |c| {
        c.require_editing(source as *const c_void);
        let id = AudioSourceId::from_ref(source);
        let in_range = usize::try_from(index).is_ok_and(|i| i < c.model.algorithm_properties.len());
        if !check!(
            in_range,
            Argument,
            source,
            c"processing algorithm index out of range"
        ) || !c.known_source(id)
        {
            return;
        }
        c.hook(Scope::Nothing, |ctl, cx| {
            ctl.request_processing_algorithm_for_audio_source(cx, id, index)
        });
    });
}

unsafe extern "C" fn is_licensed_for_capabilities<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    run_modal: sys::ARABool,
    count: sys::ARASize,
    types: *const sys::ARAContentType,
    flags: sys::ARAPlaybackTransformationFlags,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        let types = content_types(count, types);
        let Some(ctl) = c.model.controller.as_mut() else {
            return sys::kARAFalse;
        };
        ara(ctl.is_licensed_for_capabilities(run_modal != sys::kARAFalse, &types, flags))
    })
}

unsafe extern "C" fn store_audio_source_to_audio_file_chunk<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    writer: sys::ARAArchiveWriterHostRef,
    source: sys::ARAAudioSourceRef,
    archive_id: *mut sys::ARAPersistentID,
    open_automatically: *mut sys::ARABool,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        ara(unsafe {
            c.store_audio_source_to_audio_file_chunk(writer, source, archive_id, open_automatically)
        })
    })
}

unsafe extern "C" fn is_audio_modification_preserving_audio_source_signal<C: Controller>(
    dc: sys::ARADocumentControllerRef,
    modification: sys::ARAAudioModificationRef,
) -> sys::ARABool {
    dispatch::<C, _>(dc, sys::kARAFalse, |c| {
        let id = AudioModificationId::from_ref(modification);
        let preserving = c.known_modification(id)
            && c.controller().is_some_and(|ctl| {
                ctl.is_audio_modification_preserving_audio_source_signal(c.graph(), id)
            });
        ara(preserving)
    })
}

/// The whole of ARA 2.3's document controller interface.
fn interface<C: Controller>() -> sys::ARADocumentControllerInterface {
    sys::ARADocumentControllerInterface {
        structSize: implemented_size!(
            sys::ARADocumentControllerInterface,
            isAudioModificationPreservingAudioSourceSignal
        ),
        destroyDocumentController: Some(destroy_document_controller::<C>),
        getFactory: Some(get_factory::<C>),
        beginEditing: Some(begin_editing::<C>),
        endEditing: Some(end_editing::<C>),
        notifyModelUpdates: Some(notify_model_updates::<C>),
        beginRestoringDocumentFromArchive: Some(begin_restoring_document_from_archive::<C>),
        endRestoringDocumentFromArchive: Some(end_restoring_document_from_archive::<C>),
        storeDocumentToArchive: Some(store_document_to_archive::<C>),
        updateDocumentProperties: Some(update_document_properties::<C>),
        createMusicalContext: Some(create_musical_context::<C>),
        updateMusicalContextProperties: Some(update_musical_context_properties::<C>),
        updateMusicalContextContent: Some(update_musical_context_content::<C>),
        destroyMusicalContext: Some(destroy_musical_context::<C>),
        createAudioSource: Some(create_audio_source::<C>),
        updateAudioSourceProperties: Some(update_audio_source_properties::<C>),
        updateAudioSourceContent: Some(update_audio_source_content::<C>),
        enableAudioSourceSamplesAccess: Some(enable_audio_source_samples_access::<C>),
        deactivateAudioSourceForUndoHistory: Some(deactivate_audio_source_for_undo_history::<C>),
        destroyAudioSource: Some(destroy_audio_source::<C>),
        createAudioModification: Some(create_audio_modification::<C>),
        cloneAudioModification: Some(clone_audio_modification::<C>),
        updateAudioModificationProperties: Some(update_audio_modification_properties::<C>),
        deactivateAudioModificationForUndoHistory: Some(
            deactivate_audio_modification_for_undo_history::<C>,
        ),
        destroyAudioModification: Some(destroy_audio_modification::<C>),
        createPlaybackRegion: Some(create_playback_region::<C>),
        updatePlaybackRegionProperties: Some(update_playback_region_properties::<C>),
        destroyPlaybackRegion: Some(destroy_playback_region::<C>),
        isAudioSourceContentAvailable: Some(is_audio_source_content_available::<C>),
        isAudioSourceContentAnalysisIncomplete: Some(
            is_audio_source_content_analysis_incomplete::<C>,
        ),
        requestAudioSourceContentAnalysis: Some(request_audio_source_content_analysis::<C>),
        getAudioSourceContentGrade: Some(get_audio_source_content_grade::<C>),
        createAudioSourceContentReader: Some(create_audio_source_content_reader::<C>),
        isAudioModificationContentAvailable: Some(is_audio_modification_content_available::<C>),
        getAudioModificationContentGrade: Some(get_audio_modification_content_grade::<C>),
        createAudioModificationContentReader: Some(create_audio_modification_content_reader::<C>),
        isPlaybackRegionContentAvailable: Some(is_playback_region_content_available::<C>),
        getPlaybackRegionContentGrade: Some(get_playback_region_content_grade::<C>),
        createPlaybackRegionContentReader: Some(create_playback_region_content_reader::<C>),
        getContentReaderEventCount: Some(get_content_reader_event_count::<C>),
        getContentReaderDataForEvent: Some(get_content_reader_data_for_event::<C>),
        destroyContentReader: Some(destroy_content_reader::<C>),
        createRegionSequence: Some(create_region_sequence::<C>),
        updateRegionSequenceProperties: Some(update_region_sequence_properties::<C>),
        destroyRegionSequence: Some(destroy_region_sequence::<C>),
        getPlaybackRegionHeadAndTailTime: Some(get_playback_region_head_and_tail_time::<C>),
        restoreObjectsFromArchive: Some(restore_objects_from_archive::<C>),
        storeObjectsToArchive: Some(store_objects_to_archive::<C>),
        getProcessingAlgorithmsCount: Some(get_processing_algorithms_count::<C>),
        getProcessingAlgorithmProperties: Some(get_processing_algorithm_properties::<C>),
        getProcessingAlgorithmForAudioSource: Some(get_processing_algorithm_for_audio_source::<C>),
        requestProcessingAlgorithmForAudioSource: Some(
            request_processing_algorithm_for_audio_source::<C>,
        ),
        isLicensedForCapabilities: Some(is_licensed_for_capabilities::<C>),
        storeAudioSourceToAudioFileChunk: Some(store_audio_source_to_audio_file_chunk::<C>),
        isAudioModificationPreservingAudioSourceSignal: Some(
            is_audio_modification_preserving_audio_source_signal::<C>,
        ),
    }
}
