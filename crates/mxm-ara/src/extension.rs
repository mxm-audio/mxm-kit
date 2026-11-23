//! Plug-in extensions: a plug-in instance bound to a document controller, with its roles, the
//! playback regions assigned to its renderers, and its editor view's selection.

use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::sync::{Arc, Mutex, PoisonError};

use crate::content::TimeRange;
use crate::controller::{Controller, Roles, Scope, ViewSelection};
use crate::document::{DocShared, Registry, shared_for_bind};
use crate::graph::{ExtensionId, Graph, PlaybackRegion, PlaybackRegionId, RegionSequenceId};
use crate::guard::{AccessGuard, Entered};
use crate::sys;
use crate::util::{implemented_size, read_field, struct_size, thread_mark};
use crate::validate::check;

static EXTENSIONS: Registry = Registry::new();

/// The regions and sequences an instance's renderers play; read by its renderer inside both
/// guards (the document's and its own), changed with either closed.
#[derive(Default)]
struct Lists {
    playback_regions: Vec<PlaybackRegionId>,
    editor_regions: Vec<PlaybackRegionId>,
    editor_sequences: Vec<RegionSequenceId>,
}

#[derive(Default)]
struct Ui {
    selection: ViewSelection,
    hidden: Vec<RegionSequenceId>,
}

/// What the host's role refs point to.
pub(crate) struct ExtInner<C: Controller> {
    instance: sys::ARAPlugInExtensionInstance,
    playback_renderer: sys::ARAPlaybackRendererInterface,
    editor_renderer: sys::ARAEditorRendererInterface,
    editor_view: sys::ARAEditorViewInterface,
    doc: Arc<DocShared<C>>,
    roles: Roles,
    guard: AccessGuard,
    lists: UnsafeCell<Lists>,
    ui: Mutex<Ui>,
}

// SAFETY: `lists` is written only by the model thread with this extension's guard or the
// document's guard closed, and read by renderers only inside both; the rest is immutable after
// binding, atomics, or behind a mutex.
unsafe impl<C: Controller> Send for ExtInner<C> {}
unsafe impl<C: Controller> Sync for ExtInner<C> {}

impl<C: Controller> ExtInner<C> {
    fn id(&self) -> ExtensionId {
        ExtensionId(std::ptr::from_ref(self) as usize)
    }

    /// Changes the lists with this extension's renderer kept out (model thread).
    fn change_lists(&self, f: impl FnOnce(&mut Lists)) {
        self.guard.close();
        // SAFETY: the guard is closed (this renderer is out) and the model thread is the only
        // writer.
        f(unsafe { &mut *self.lists.get() });
        self.guard.open();
    }

    fn ui(&self) -> std::sync::MutexGuard<'_, Ui> {
        self.ui.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn renders_region(&self, region: PlaybackRegionId) -> bool {
        // SAFETY: the model thread reads; it is the only writer.
        let lists = unsafe { &*self.lists.get() };
        lists.playback_regions.contains(&region) || lists.editor_regions.contains(&region)
    }

    /// A region is being destroyed (model thread, the document's guard closed).
    pub(crate) fn forget_region(&self, region: PlaybackRegionId) {
        self.change_lists(|l| {
            l.playback_regions.retain(|r| *r != region);
            l.editor_regions.retain(|r| *r != region);
        });
        self.ui()
            .selection
            .playback_regions
            .retain(|r| *r != region);
    }

    /// A region sequence is being destroyed.
    pub(crate) fn forget_region_sequence(&self, sequence: RegionSequenceId) {
        self.change_lists(|l| l.editor_sequences.retain(|s| *s != sequence));
        let mut ui = self.ui();
        ui.selection.region_sequences.retain(|s| *s != sequence);
        ui.hidden.retain(|s| *s != sequence);
    }

    /// A model call on behalf of this extension, with the region checked against the graph.
    fn with_region(
        &self,
        region: sys::ARAPlaybackRegionRef,
        f: impl FnOnce(&mut crate::document::Call<'_, C>, PlaybackRegionId),
    ) {
        let id = PlaybackRegionId::from_ref(region);
        self.doc.call((), |call| {
            if check!(
                call.graph().playback_region(id).is_some(),
                Argument,
                region,
                c"unknown playback region"
            ) {
                f(call, id);
            }
        });
    }
}

/// What an instance's editor renderer was given to preview edits of (`ARAEditorRendererInterface`):
/// regions one by one, and region sequences as a whole, in the order assigned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorAssignment {
    pub playback_regions: Vec<PlaybackRegionId>,
    pub region_sequences: Vec<RegionSequenceId>,
}

/// The editor renderer's assignment of a live extension of `C`, read on the model thread, the
/// lists' only writer; `None` for an id that isn't one.
pub(crate) fn editor_assignment<C: Controller>(extension: ExtensionId) -> Option<EditorAssignment> {
    if !EXTENSIONS.contains::<ExtInner<C>>(extension.0) {
        return None;
    }
    // SAFETY: a live extension (unregistered before it is freed, on the model thread, where this
    // runs), whose lists only the model thread writes.
    let lists = unsafe { &*(*(extension.0 as *const ExtInner<C>)).lists.get() };
    Some(EditorAssignment {
        playback_regions: lists.editor_regions.clone(),
        region_sequences: lists.editor_sequences.clone(),
    })
}

/// The extension behind a role ref, or `None` (reported).
fn inner<'a, C: Controller>(address: usize) -> Option<&'a ExtInner<C>> {
    let live = EXTENSIONS.contains::<ExtInner<C>>(address);
    // SAFETY: a live extension, freed only when its `PlugInExtension` drops, which unregisters it
    // first; host calls come on the model thread, where that drop also happens.
    check!(
        live,
        Argument,
        address as *const u8,
        c"unknown plug-in extension"
    )
    .then(|| unsafe { &*(address as *const ExtInner<C>) })
}

// ---- the role interfaces -----------------------------------------------------------------------

unsafe extern "C" fn playback_renderer_add_region<C: Controller>(
    r: sys::ARAPlaybackRendererRef,
    region: sys::ARAPlaybackRegionRef,
) {
    let Some(e) = inner::<C>(r as usize) else {
        return;
    };
    e.with_region(region, |call, id| {
        e.change_lists(|l| {
            if !l.playback_regions.contains(&id) {
                l.playback_regions.push(id);
            }
        });
        call.hook(Scope::Nothing, |c, cx| {
            c.did_add_playback_region_to_renderer(cx, e.id(), id)
        });
    });
}

unsafe extern "C" fn playback_renderer_remove_region<C: Controller>(
    r: sys::ARAPlaybackRendererRef,
    region: sys::ARAPlaybackRegionRef,
) {
    let Some(e) = inner::<C>(r as usize) else {
        return;
    };
    e.with_region(region, |call, id| {
        let mut had = false;
        e.change_lists(|l| {
            had = l.playback_regions.contains(&id);
            l.playback_regions.retain(|x| *x != id);
        });
        if check!(
            had,
            Argument,
            region,
            c"region removed from a renderer that doesn't have it"
        ) {
            call.hook(Scope::Nothing, |c, cx| {
                c.did_remove_playback_region_from_renderer(cx, e.id(), id)
            });
        }
    });
}

unsafe extern "C" fn editor_renderer_add_region<C: Controller>(
    r: sys::ARAEditorRendererRef,
    region: sys::ARAPlaybackRegionRef,
) {
    let Some(e) = inner::<C>(r as usize) else {
        return;
    };
    e.with_region(region, |call, id| {
        e.change_lists(|l| {
            if !l.editor_regions.contains(&id) {
                l.editor_regions.push(id);
            }
        });
        call.hook(Scope::Nothing, |c, cx| {
            c.did_change_editor_renderer(cx, e.id())
        });
    });
}

unsafe extern "C" fn editor_renderer_remove_region<C: Controller>(
    r: sys::ARAEditorRendererRef,
    region: sys::ARAPlaybackRegionRef,
) {
    let Some(e) = inner::<C>(r as usize) else {
        return;
    };
    e.with_region(region, |call, id| {
        e.change_lists(|l| l.editor_regions.retain(|x| *x != id));
        call.hook(Scope::Nothing, |c, cx| {
            c.did_change_editor_renderer(cx, e.id())
        });
    });
}

unsafe extern "C" fn editor_renderer_add_sequence<C: Controller>(
    r: sys::ARAEditorRendererRef,
    sequence: sys::ARARegionSequenceRef,
) {
    let Some(e) = inner::<C>(r as usize) else {
        return;
    };
    let id = RegionSequenceId::from_ref(sequence);
    e.doc.call((), |call| {
        if check!(
            call.graph().region_sequence(id).is_some(),
            Argument,
            sequence,
            c"unknown region sequence"
        ) {
            e.change_lists(|l| {
                if !l.editor_sequences.contains(&id) {
                    l.editor_sequences.push(id);
                }
            });
            call.hook(Scope::Nothing, |c, cx| {
                c.did_change_editor_renderer(cx, e.id())
            });
        }
    });
}

unsafe extern "C" fn editor_renderer_remove_sequence<C: Controller>(
    r: sys::ARAEditorRendererRef,
    sequence: sys::ARARegionSequenceRef,
) {
    let Some(e) = inner::<C>(r as usize) else {
        return;
    };
    let id = RegionSequenceId::from_ref(sequence);
    e.doc.call((), |call| {
        e.change_lists(|l| l.editor_sequences.retain(|s| *s != id));
        call.hook(Scope::Nothing, |c, cx| {
            c.did_change_editor_renderer(cx, e.id())
        });
    });
}

/// Copies a C array of refs.
unsafe fn ref_list<T>(count: usize, refs: *const *mut T) -> Vec<usize> {
    if count == 0 || refs.is_null() {
        return Vec::new();
    }
    // SAFETY: the host's array of `count` refs, valid for the call.
    unsafe { std::slice::from_raw_parts(refs, count) }
        .iter()
        .map(|r| *r as usize)
        .collect()
}

unsafe extern "C" fn editor_view_notify_selection<C: Controller>(
    v: sys::ARAEditorViewRef,
    selection: *const sys::ARAViewSelection,
) {
    let Some(e) = inner::<C>(v as usize) else {
        return;
    };
    type S = sys::ARAViewSelection;
    // SAFETY: the host's selection, valid for the call, size-checked before its fields are read.
    let valid = check!(
        !selection.is_null()
            && unsafe { struct_size(selection) } >= implemented_size!(S, timeRange),
        Argument,
        selection,
        c"ARAViewSelection missing or too small"
    );
    if !valid {
        return;
    }
    let (regions, sequences, range) = unsafe {
        (
            ref_list(
                read_field!(selection, S, playbackRegionRefsCount).unwrap_or(0),
                read_field!(selection, S, playbackRegionRefs).unwrap_or(std::ptr::null()),
            ),
            ref_list(
                read_field!(selection, S, regionSequenceRefsCount).unwrap_or(0),
                read_field!(selection, S, regionSequenceRefs).unwrap_or(std::ptr::null()),
            ),
            TimeRange::from_ptr(read_field!(selection, S, timeRange).unwrap_or(std::ptr::null())),
        )
    };
    e.doc.call((), |call| {
        let g = call.graph();
        let new = ViewSelection {
            playback_regions: regions
                .into_iter()
                .map(PlaybackRegionId)
                .filter(|r| {
                    check!(
                        g.playback_region(*r).is_some(),
                        Argument,
                        selection,
                        c"selection names an unknown region"
                    )
                })
                .collect(),
            region_sequences: sequences
                .into_iter()
                .map(RegionSequenceId)
                .filter(|s| {
                    check!(
                        g.region_sequence(*s).is_some(),
                        Argument,
                        selection,
                        c"selection names an unknown sequence"
                    )
                })
                .collect(),
            time_range: range,
        };
        e.ui().selection = new.clone();
        call.hook(Scope::Nothing, |c, cx| {
            c.did_notify_selection(cx, e.id(), &new)
        });
    });
}

unsafe extern "C" fn editor_view_notify_hide_region_sequences<C: Controller>(
    v: sys::ARAEditorViewRef,
    count: sys::ARASize,
    refs: *const sys::ARARegionSequenceRef,
) {
    let Some(e) = inner::<C>(v as usize) else {
        return;
    };
    // SAFETY: the host's array, valid for the call.
    let sequences = unsafe { ref_list(count, refs) };
    e.doc.call((), |call| {
        let g = call.graph();
        let hidden: Vec<RegionSequenceId> = sequences
            .into_iter()
            .map(RegionSequenceId)
            .filter(|s| {
                check!(
                    g.region_sequence(*s).is_some(),
                    Argument,
                    refs,
                    c"unknown region sequence hidden"
                )
            })
            .collect();
        e.ui().hidden = hidden.clone();
        call.hook(Scope::Nothing, |c, cx| {
            c.did_notify_hide_region_sequences(cx, e.id(), &hidden)
        });
    });
}

// ---- the plug-in's side -------------------------------------------------------------------------

/// A plug-in instance's binding to an ARA document controller (its `ARAPlugInExtensionInstance`).
/// The plug-in keeps it for the instance's life; dropping it unbinds (on the model thread, as
/// ARA asks of hosts). Either it or the document controller may go first.
pub struct PlugInExtension<C: Controller> {
    inner: Box<ExtInner<C>>,
}

impl<C: Controller> PlugInExtension<C> {
    /// Binds to the host's document controller (`ARADocumentControllerRef`) in the roles given
    /// (`ARAPlugInInstanceRoleFlags`): a role the host doesn't know is assumed, as ARA_Library
    /// does. Model thread only; `None` (reported) for a controller that isn't this plug-in's.
    pub fn bind(
        document_controller: *const c_void,
        known_roles: u32,
        assigned_roles: u32,
    ) -> Option<Self> {
        let doc = shared_for_bind::<C>(document_controller as sys::ARADocumentControllerRef)?;
        check!(
            assigned_roles | known_roles == known_roles,
            Argument,
            document_controller,
            c"assigned roles the host doesn't know"
        );
        let has = |role: Roles| known_roles & role.0 == 0 || assigned_roles & role.0 != 0;
        let mut roles = Roles(0);
        for role in [
            Roles::PLAYBACK_RENDERER,
            Roles::EDITOR_RENDERER,
            Roles::EDITOR_VIEW,
        ] {
            if has(role) {
                roles = roles | role;
            }
        }
        let mut inner = Box::new(ExtInner {
            // SAFETY: an all-zero instance (null pointers) is valid; filled in below.
            instance: unsafe { std::mem::zeroed() },
            playback_renderer: sys::ARAPlaybackRendererInterface {
                structSize: implemented_size!(
                    sys::ARAPlaybackRendererInterface,
                    removePlaybackRegion
                ),
                addPlaybackRegion: Some(playback_renderer_add_region::<C>),
                removePlaybackRegion: Some(playback_renderer_remove_region::<C>),
            },
            editor_renderer: sys::ARAEditorRendererInterface {
                structSize: implemented_size!(
                    sys::ARAEditorRendererInterface,
                    removeRegionSequence
                ),
                addPlaybackRegion: Some(editor_renderer_add_region::<C>),
                removePlaybackRegion: Some(editor_renderer_remove_region::<C>),
                addRegionSequence: Some(editor_renderer_add_sequence::<C>),
                removeRegionSequence: Some(editor_renderer_remove_sequence::<C>),
            },
            editor_view: sys::ARAEditorViewInterface {
                structSize: implemented_size!(
                    sys::ARAEditorViewInterface,
                    notifyHideRegionSequences
                ),
                notifySelection: Some(editor_view_notify_selection::<C>),
                notifyHideRegionSequences: Some(editor_view_notify_hide_region_sequences::<C>),
            },
            doc,
            roles,
            guard: AccessGuard::new(),
            lists: UnsafeCell::new(Lists::default()),
            ui: Mutex::new(Ui::default()),
        });
        let me: *mut ExtInner<C> = &mut *inner;
        let role = |r: Roles| roles.contains(r);
        inner.instance = sys::ARAPlugInExtensionInstance {
            structSize: implemented_size!(sys::ARAPlugInExtensionInstance, editorViewInterface),
            // ARA 1's interface: not offered, as this library serves 2.0 final and later.
            plugInExtensionRef: std::ptr::null_mut(),
            plugInExtensionInterface: std::ptr::null(),
            playbackRendererRef: if role(Roles::PLAYBACK_RENDERER) {
                me.cast()
            } else {
                std::ptr::null_mut()
            },
            playbackRendererInterface: if role(Roles::PLAYBACK_RENDERER) {
                &raw const inner.playback_renderer
            } else {
                std::ptr::null()
            },
            editorRendererRef: if role(Roles::EDITOR_RENDERER) {
                me.cast()
            } else {
                std::ptr::null_mut()
            },
            editorRendererInterface: if role(Roles::EDITOR_RENDERER) {
                &raw const inner.editor_renderer
            } else {
                std::ptr::null()
            },
            editorViewRef: if role(Roles::EDITOR_VIEW) {
                me.cast()
            } else {
                std::ptr::null_mut()
            },
            editorViewInterface: if role(Roles::EDITOR_VIEW) {
                &raw const inner.editor_view
            } else {
                std::ptr::null()
            },
        };
        let address = me as usize;
        EXTENSIONS.insert::<ExtInner<C>>(address);
        inner
            .doc
            .extensions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(address);
        let id = inner.id();
        inner.doc.call((), |call| {
            call.hook(Scope::Nothing, |c, cx| c.did_bind(cx, id, roles));
        });
        Some(PlugInExtension { inner })
    }

    /// The `const ARAPlugInExtensionInstance*` the companion API hands the host; valid until this
    /// is dropped.
    pub fn instance(&self) -> *const c_void {
        (&raw const self.inner.instance).cast()
    }

    pub fn id(&self) -> ExtensionId {
        self.inner.id()
    }

    pub fn roles(&self) -> Roles {
        self.inner.roles
    }

    pub fn is_playback_renderer(&self) -> bool {
        self.inner.roles.contains(Roles::PLAYBACK_RENDERER)
    }

    pub fn is_editor_renderer(&self) -> bool {
        self.inner.roles.contains(Roles::EDITOR_RENDERER)
    }

    pub fn is_editor_view(&self) -> bool {
        self.inner.roles.contains(Roles::EDITOR_VIEW)
    }

    /// Whether the host has destroyed the document controller (the instance then renders
    /// nothing through ARA).
    pub fn is_document_destroyed(&self) -> bool {
        self.inner
            .doc
            .destroyed
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// The renderer's view of the model: `None` while the host edits the graph or changes this
    /// instance's regions (render silence for the block), never waiting, locking or allocating.
    /// What it reads can't change until it's dropped, so drop it before the block ends. Any
    /// thread but the model thread may read this way.
    pub fn render_access(&self) -> Option<RenderAccess<'_, C>> {
        let e = &*self.inner;
        let doc = e.doc.guard.try_enter()?;
        let ext = e.guard.try_enter()?;
        // SAFETY: inside both guards nothing changes the graph or the lists.
        let (graph, lists) = unsafe { (e.doc.graph(), &*e.lists.get()) };
        Some(RenderAccess {
            graph,
            lists,
            _doc: doc,
            _ext: ext,
        })
    }

    /// The host's selection, as last told to this editor view.
    pub fn selection(&self) -> ViewSelection {
        self.inner.ui().selection.clone()
    }

    /// The region sequences the host hides from view.
    pub fn hidden_region_sequences(&self) -> Vec<RegionSequenceId> {
        self.inner.ui().hidden.clone()
    }

    /// Runs `f` with the controller and the graph, on the model thread and outside any other
    /// model call; `None` otherwise, or once the document controller is gone.
    pub fn with_controller<R>(&self, f: impl FnOnce(&mut C, &Graph<C>) -> R) -> Option<R> {
        if thread_mark() != self.inner.doc.model_thread {
            return None;
        }
        self.inner.doc.call(None, |call| {
            call.hook(Scope::Nothing, |c, cx| f(c, cx.graph()))
        })
    }
}

impl<C: Controller> Drop for PlugInExtension<C> {
    fn drop(&mut self) {
        let e = &*self.inner;
        let address = std::ptr::from_ref(e) as usize;
        EXTENSIONS.remove(address);
        e.doc
            .extensions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|a| *a != address);
        if !e.doc.destroyed.load(std::sync::atomic::Ordering::Acquire) {
            let id = e.id();
            e.doc.call((), |call| {
                call.hook(Scope::Nothing, |c, cx| c.will_unbind(cx, id));
            });
        }
    }
}

/// A renderer inside the guards: the graph and this instance's regions, unchanging while held.
pub struct RenderAccess<'a, C: Controller> {
    graph: &'a Graph<C>,
    lists: &'a Lists,
    _doc: Entered<'a>,
    _ext: Entered<'a>,
}

impl<C: Controller> RenderAccess<'_, C> {
    pub fn graph(&self) -> &Graph<C> {
        self.graph
    }

    /// The regions assigned to this instance's playback renderer, in the order assigned.
    pub fn playback_regions(&self) -> impl Iterator<Item = &PlaybackRegion<C>> {
        self.lists
            .playback_regions
            .iter()
            .filter_map(|id| self.graph.playback_region(*id))
    }

    /// The regions assigned to its editor renderer, one by one.
    pub fn editor_playback_regions(&self) -> impl Iterator<Item = &PlaybackRegion<C>> {
        self.lists
            .editor_regions
            .iter()
            .filter_map(|id| self.graph.playback_region(*id))
    }

    /// The region sequences assigned to its editor renderer as a whole.
    pub fn editor_region_sequences(&self) -> &[RegionSequenceId] {
        &self.lists.editor_sequences
    }
}

/// Binds an instance once, as nice-plug's `AraHooks::bind` needs: keeps the extension in `slot`
/// and returns its `ARAPlugInExtensionInstance*`, or null if the instance is already bound (ARA
/// allows one binding per instance) or the controller isn't this plug-in's.
pub fn bind_into<C: Controller>(
    slot: &mut Option<PlugInExtension<C>>,
    document_controller: *const c_void,
    known_roles: u32,
    assigned_roles: u32,
) -> *const c_void {
    if !check!(
        slot.is_none(),
        State,
        document_controller,
        c"plug-in instance bound twice"
    ) {
        return std::ptr::null();
    }
    match PlugInExtension::bind(document_controller, known_roles, assigned_roles) {
        Some(extension) => slot.insert(extension).instance(),
        None => std::ptr::null(),
    }
}
