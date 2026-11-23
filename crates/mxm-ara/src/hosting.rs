//! The host's half of ARA 2: what a DAW needs to host ARA plug-ins (newDAWn's
//! `plans/plan-ara-host.md`), over the same bindings as the plug-in side.
//!
//! - [`Factory`]: a plug-in's `ARAFactory`, read field by field as far as its size goes; the API
//!   generation negotiated; [`Factory::initialize`] counted per factory per process, since a live
//!   engine and an offline render share one loaded library.
//! - [`Host`]: what the host gives the plug-in, as one trait: its recordings' samples (audio
//!   access), its musical context's tempo, bars and key (content access), and what the plug-in
//!   tells it (model updates, the playback controller's requests). Archives are this module's:
//!   [`DocumentController::store`] and [`DocumentController::restore`] hand bytes in and out.
//! - [`DocumentController`]: a document of the plug-in's, every call of its interface on the
//!   thread that made it, `None` where the plug-in lacks the call.
//! - [`PlugInExtension`]: a bound instance's roles, its renderers' regions, its view's selection.
//!
//! Refs are plain numbers. A host ref is the host's own (never 0); a plug-in ref
//! ([`SourceRef`], [`RegionRef`], …) is the plug-in's, only ever passed back to it.
//!
//! The host's tables advertise the host's own size (`ARA_IMPLEMENTED_STRUCT_SIZE` of what it
//! implements); the plug-in's are read only as far as their `structSize` goes, zero past it, and
//! refused below their minimum.

use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::ffi::{CString, c_char, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::thread::ThreadId;

use crate::content::{BarSignature, ContentGrade, ContentType, ContentUpdateFlags};
use crate::content::{KeySignature, TempoEntry};
use crate::sys;
use crate::util::{c_string, implemented_size, opt_string, read_field};

/// The lowest API generation this host takes: ARA 2.0 final. ARA 1's binding of every role and
/// its archives aren't hosted.
pub const LOWEST_HOSTED_GENERATION: i32 = sys::kARAAPIGeneration_2_0_Final;
/// The highest this host knows: ARA 2.3 final.
pub const HIGHEST_HOSTED_GENERATION: i32 = sys::kARAAPIGeneration_2_3_Final;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Copies a plug-in's versioned struct as far as its `structSize` goes; the rest stays zero
/// (null), as for an older plug-in that lacks it.
///
/// # Safety
/// `p` must point to a struct whose `structSize` bytes are readable.
unsafe fn read_sized<S>(p: *const S) -> S {
    // SAFETY: zero is a valid value for ARA's structs (sizes, nulls, `None`s).
    let mut out: S = unsafe { std::mem::zeroed() };
    // SAFETY: the caller's.
    let size = unsafe { crate::util::struct_size(p) }.min(std::mem::size_of::<S>());
    // SAFETY: `size` readable bytes at `p` (the caller's contract), within `out`.
    unsafe { std::ptr::copy_nonoverlapping(p.cast::<u8>(), (&raw mut out).cast::<u8>(), size) };
    out
}

/// A pointer for an optional C array: null when empty, as ARA wants.
fn array<T>(v: &[T]) -> *const T {
    if v.is_empty() {
        std::ptr::null()
    } else {
        v.as_ptr()
    }
}

// ---- asserts -----------------------------------------------------------------------------------

struct AssertSlot(UnsafeCell<sys::ARAAssertFunction>);
// SAFETY: the plug-in reads the slot through the address it is given; it is written only here,
// once, at compile time.
unsafe impl Sync for AssertSlot {}

static ASSERT_SLOT: AssertSlot = AssertSlot(UnsafeCell::new(Some(record_assert)));
static ASSERT_COUNT: AtomicUsize = AtomicUsize::new(0);
/// The most recent asserts kept for [`take_asserts`]; the count goes on past them.
const ASSERTS_KEPT: usize = 64;
static ASSERTS: Mutex<Vec<(i32, String)>> = Mutex::new(Vec::new());

unsafe extern "C" fn record_assert(
    category: sys::ARAAssertCategory,
    _argument: *const c_void,
    diagnosis: *const c_char,
) {
    ASSERT_COUNT.fetch_add(1, Ordering::Relaxed);
    // SAFETY: the plug-in passes a NUL-terminated diagnosis, or null.
    let text = unsafe { opt_string(diagnosis) }.unwrap_or_default();
    let mut kept = lock(&ASSERTS);
    if kept.len() == ASSERTS_KEPT {
        kept.remove(0);
    }
    kept.push((category, text));
}

/// How many asserts every hosted plug-in has reported through the host's assert function in this
/// process. A test holds it at zero.
pub fn assert_count() -> usize {
    ASSERT_COUNT.load(Ordering::Relaxed)
}

/// The last asserts reported (category, diagnosis), taken, for a log.
pub fn take_asserts() -> Vec<(i32, String)> {
    std::mem::take(&mut *lock(&ASSERTS))
}

// ---- the factory -------------------------------------------------------------------------------

/// A plug-in's `ARAFactory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Factory(usize);

/// The factory's fields, as a host reads them (the test host's too, `test_host`).
#[derive(Clone, Debug, PartialEq)]
pub struct FactoryInfo {
    pub struct_size: usize,
    pub lowest_api_generation: i32,
    pub highest_api_generation: i32,
    pub factory_id: String,
    pub plug_in_name: String,
    pub manufacturer_name: String,
    pub information_url: String,
    pub version: String,
    pub document_archive_id: String,
    pub compatible_document_archive_ids: Vec<String>,
    pub analyzeable_content_types: Vec<i32>,
    pub supported_playback_transformation_flags: i32,
    pub supports_storing_audio_file_chunks: Option<bool>,
}

/// The factory's fields read as far as its `structSize` goes, a field past it at its default.
///
/// # Safety
/// `f` must point to a plug-in's `ARAFactory` of at least a `structSize`.
pub(crate) unsafe fn read_info(f: *const sys::ARAFactory) -> FactoryInfo {
    type F = sys::ARAFactory;
    // SAFETY: the caller's; every field read is size-checked.
    unsafe {
        let s = |p: Option<*const c_char>| p.and_then(|p| opt_string(p)).unwrap_or_default();
        let compatible = read_field!(f, F, compatibleDocumentArchiveIDsCount)
            .zip(read_field!(f, F, compatibleDocumentArchiveIDs))
            .filter(|(n, p)| *n > 0 && !p.is_null())
            .map(|(n, p)| {
                std::slice::from_raw_parts(p, n)
                    .iter()
                    .map(|id| opt_string(*id).unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default();
        let types = read_field!(f, F, analyzeableContentTypesCount)
            .zip(read_field!(f, F, analyzeableContentTypes))
            .filter(|(n, p)| *n > 0 && !p.is_null())
            .map(|(n, p)| std::slice::from_raw_parts(p, n).to_vec())
            .unwrap_or_default();
        FactoryInfo {
            struct_size: crate::util::struct_size(f),
            lowest_api_generation: read_field!(f, F, lowestSupportedApiGeneration).unwrap_or(0),
            highest_api_generation: read_field!(f, F, highestSupportedApiGeneration).unwrap_or(0),
            factory_id: s(read_field!(f, F, factoryID)),
            plug_in_name: s(read_field!(f, F, plugInName)),
            manufacturer_name: s(read_field!(f, F, manufacturerName)),
            information_url: s(read_field!(f, F, informationURL)),
            version: s(read_field!(f, F, version)),
            document_archive_id: s(read_field!(f, F, documentArchiveID)),
            compatible_document_archive_ids: compatible,
            analyzeable_content_types: types,
            supported_playback_transformation_flags: read_field!(
                f,
                F,
                supportedPlaybackTransformationFlags
            )
            .unwrap_or(0),
            supports_storing_audio_file_chunks: read_field!(f, F, supportsStoringAudioFileChunks)
                .map(|b| b != sys::kARAFalse),
        }
    }
}

impl Factory {
    /// The factory at `ptr`, `None` for a null one or one smaller than ARA's minimum.
    ///
    /// # Safety
    /// `ptr` must be null or a plug-in's `const ARAFactory*`, valid while its library stays loaded,
    /// which the host keeps loaded while it uses the factory.
    pub unsafe fn from_raw(ptr: *const c_void) -> Option<Self> {
        if ptr.is_null() {
            return None;
        }
        // SAFETY: the caller's.
        let size = unsafe { crate::util::struct_size(ptr.cast::<sys::ARAFactory>()) };
        (size >= sys::kARAFactoryMinSize as usize).then_some(Factory(ptr as usize))
    }

    fn raw(self) -> *const sys::ARAFactory {
        self.0 as *const sys::ARAFactory
    }

    pub fn info(self) -> FactoryInfo {
        // SAFETY: the plug-in's factory, at least ARA's minimum size (`from_raw`'s contract).
        unsafe { read_info(self.raw()) }
    }

    /// The generation to run the factory at: 2.3 when it offers it, else the highest it offers at
    /// 2.0 final or above; `None` when it offers none of those.
    pub fn api_generation(self) -> Option<i32> {
        let info = self.info();
        let generation = info.highest_api_generation.min(HIGHEST_HOSTED_GENERATION);
        (generation >= LOWEST_HOSTED_GENERATION.max(info.lowest_api_generation))
            .then_some(generation)
    }

    /// The factory initialized (`initializeARAWithConfiguration`), at [`Factory::api_generation`],
    /// shared with every other use of it in this process: the last [`Initialized`] dropped
    /// uninitializes it. `None` when no generation suits or the factory lacks the calls.
    pub fn initialize(self) -> Option<Arc<Initialized>> {
        static LIVE: Mutex<Vec<(usize, Weak<Initialized>)>> = Mutex::new(Vec::new());
        let mut live = lock(&LIVE);
        live.retain(|(_, held)| held.strong_count() > 0);
        if let Some(held) = live
            .iter()
            .find(|(factory, _)| *factory == self.0)
            .and_then(|(_, held)| held.upgrade())
        {
            return Some(held);
        }
        let generation = self.api_generation()?;
        let f = self.raw();
        type F = sys::ARAFactory;
        // SAFETY: the plug-in's factory; the calls' presence is size-checked.
        let (init, uninit) = unsafe {
            (
                read_field!(f, F, initializeARAWithConfiguration).flatten()?,
                read_field!(f, F, uninitializeARA).flatten()?,
            )
        };
        let config = sys::ARAInterfaceConfiguration {
            structSize: implemented_size!(sys::ARAInterfaceConfiguration, assertFunctionAddress),
            desiredApiGeneration: generation,
            assertFunctionAddress: ASSERT_SLOT.0.get(),
        };
        // SAFETY: the plug-in's entry point, with a valid configuration whose assert slot is a
        // static.
        unsafe { init(&config) };
        let made = Arc::new(Initialized {
            factory: self,
            generation,
            uninit,
        });
        live.push((self.0, Arc::downgrade(&made)));
        Some(made)
    }
}

/// A factory initialized; `uninitializeARA` when the last goes.
pub struct Initialized {
    factory: Factory,
    generation: i32,
    uninit: unsafe extern "C" fn(),
}

impl std::fmt::Debug for Initialized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Initialized")
            .field("factory", &self.factory)
            .field("generation", &self.generation)
            .finish()
    }
}

impl Drop for Initialized {
    fn drop(&mut self) {
        // SAFETY: the plug-in's entry point, after every controller of this initialization, which
        // each hold it, has gone.
        unsafe { (self.uninit)() };
    }
}

impl Initialized {
    pub fn factory(&self) -> Factory {
        self.factory
    }

    pub fn generation(&self) -> i32 {
        self.generation
    }

    /// A new document controller (`createDocumentControllerWithDocument`), its calls on the
    /// calling thread from now on; `None` if the plug-in refuses.
    pub fn create_document(
        self: &Arc<Self>,
        host: Arc<dyn Host>,
        name: &str,
    ) -> Option<DocumentController> {
        let shared = Shared::new(host);
        let name = c_string(name);
        let properties = sys::ARADocumentProperties {
            structSize: implemented_size!(sys::ARADocumentProperties, name),
            name: name.as_ptr(),
        };
        let f = self.factory.raw();
        // SAFETY: the plug-in's entry point with valid host structs, which the controller holds
        // until it is destroyed.
        unsafe {
            let create =
                read_field!(f, sys::ARAFactory, createDocumentControllerWithDocument).flatten()?;
            let instance = create(shared.instance.get(), &properties);
            if instance.is_null()
                || crate::util::struct_size(instance)
                    < sys::kARADocumentControllerInstanceMinSize as usize
            {
                return None;
            }
            let instance = read_sized(instance);
            let table = instance.documentControllerInterface;
            if table.is_null()
                || crate::util::struct_size(table)
                    < sys::kARADocumentControllerInterfaceMinSize as usize
            {
                return None;
            }
            Some(DocumentController {
                shared,
                dc: instance.documentControllerRef as usize,
                i: read_sized(table),
                thread: std::thread::current().id(),
                init: self.clone(),
                destroyed: false,
            })
        }
    }
}

// ---- the host ----------------------------------------------------------------------------------

/// Where an analysis is (`ARAAnalysisProgressState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisState {
    Started,
    Updated,
    Completed,
}

/// A musical context's content as the host serves it: the tempo map (at least two entries,
/// strictly rising in both time and quarters), the bar signatures (at least one, each lasting
/// whole bars), the key signatures, each list as `ARAContentValidator.h` checks it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicalContent {
    pub tempo: Vec<TempoEntry>,
    pub bars: Vec<BarSignature>,
    pub keys: Vec<KeySignature>,
    /// How sure the host is of it: what the user set is approved.
    pub grade: Option<ContentGrade>,
}

/// One read's buffers: one slice per channel, all `count` long, of the reader's sample size.
pub enum Samples<'a> {
    F32(Vec<&'a mut [f32]>),
    F64(Vec<&'a mut [f64]>),
}

/// What the host gives a plug-in's document controller. Every call names the host's own refs:
/// those it passed when it created the object (`create_audio_source`'s `host_ref`), and those it
/// handed out here (a reader's).
///
/// Threads, as ARA has them: readers are made and destroyed, and content is read, on the
/// document controller's thread; samples are read on any thread but the audio thread, one thread
/// at a time per reader, and **a read never waits on the document controller's thread or a render
/// thread**: a plug-in may wait for its reads while rendering offline. Notifications come during
/// [`DocumentController::notify_model_updates`]; the playback controller's requests on any thread
/// but the audio thread.
pub trait Host: Send + Sync {
    /// A reader of source `source`'s samples (`createAudioReaderForSource`), the host's ref for
    /// it (never 0), or `None` to refuse (its samples can't be given now).
    fn create_audio_reader(&self, source: usize, use_64_bit: bool) -> Option<usize>;
    /// `readAudioSamples`: the samples from `position` on, at the source's own rate, into
    /// `buffers`; silence before its start and past its end. False if they can't be read.
    fn read_audio_samples(&self, reader: usize, position: i64, buffers: Samples<'_>) -> bool;
    fn destroy_audio_reader(&self, reader: usize);

    /// Musical context `context`'s content, `None` for none.
    fn musical_context_content(&self, _context: usize) -> Option<MusicalContent> {
        None
    }

    fn analysis_progress(&self, _source: usize, _state: AnalysisState, _value: f32) {}
    /// The plug-in's state of a source changed (its analysis among it): its archive changed.
    fn audio_source_content_changed(&self, _source: usize, _flags: ContentUpdateFlags) {}
    /// The plug-in's state of a modification changed (an edit): its archive changed.
    fn audio_modification_content_changed(&self, _modification: usize, _flags: ContentUpdateFlags) {
    }
    /// A region now sounds otherwise; without `SIGNAL_UNCHANGED`, its head and tail may have
    /// changed too.
    fn playback_region_content_changed(&self, _region: usize, _flags: ContentUpdateFlags) {}
    /// ARA 2.3: the plug-in's document state tied to no source or modification changed.
    fn document_data_changed(&self) {}
    fn archiving_progress(&self, _value: f32) {}
    fn unarchiving_progress(&self, _value: f32) {}

    /// The playback controller's requests, which the host may ignore or delay.
    fn request_start_playback(&self) {}
    fn request_stop_playback(&self) {}
    fn request_set_playback_position(&self, _seconds: f64) {}
    fn request_set_cycle_range(&self, _start: f64, _duration: f64) {}
    fn request_enable_cycle(&self, _enable: bool) {}
}

/// A content reader's events, boxed so their addresses hold until it is destroyed.
enum ContentEvents {
    Tempo(Vec<sys::ARAContentTempoEntry>),
    Bars(Vec<sys::ARAContentBarSignature>),
    Keys {
        events: Vec<sys::ARAContentKeySignature>,
        _names: Vec<CString>,
    },
}

impl ContentEvents {
    fn len(&self) -> usize {
        match self {
            ContentEvents::Tempo(v) => v.len(),
            ContentEvents::Bars(v) => v.len(),
            ContentEvents::Keys { events, .. } => events.len(),
        }
    }

    fn get(&self, index: usize) -> *const c_void {
        fn at<T>(v: &[T], index: usize) -> *const c_void {
            v.get(index)
                .map_or(std::ptr::null(), |e| std::ptr::from_ref(e).cast())
        }
        match self {
            ContentEvents::Tempo(v) => at(v, index),
            ContentEvents::Bars(v) => at(v, index),
            ContentEvents::Keys { events, .. } => at(events, index),
        }
    }
}

/// An archive being read, one restore's: the bytes and the id they were written with.
struct ArchiveIn {
    bytes: Vec<u8>,
    archive_id: CString,
}

/// What every controller host ref points at, boxed so its address holds while the controller
/// lives.
struct Shared {
    host: Arc<dyn Host>,
    audio: sys::ARAAudioAccessControllerInterface,
    archiving: sys::ARAArchivingControllerInterface,
    content: sys::ARAContentAccessControllerInterface,
    updates: sys::ARAModelUpdateControllerInterface,
    playback: sys::ARAPlaybackControllerInterface,
    instance: UnsafeCell<sys::ARADocumentControllerHostInstance>,
    /// Each source's channels, by its host ref, for a read's buffers.
    channels: Mutex<HashMap<usize, usize>>,
    /// Each live reader's channels and sample size, by its host ref.
    readers: Mutex<HashMap<usize, (usize, bool)>>,
    content_readers: Mutex<HashMap<usize, Box<ContentEvents>>>,
    next_content_reader: AtomicUsize,
    /// The archive a restore reads, and the one a store writes, each only during its call.
    archive_in: Mutex<Option<ArchiveIn>>,
    archive_out: Mutex<Option<Vec<u8>>>,
}

// SAFETY: `instance` is written once, before any plug-in sees it; the rest is locked, atomic or
// immutable, and the host is `Send + Sync`.
unsafe impl Sync for Shared {}
// SAFETY: as above.
unsafe impl Send for Shared {}

/// The archive reader's and writer's host ref: there is one of each at a time per controller.
const ARCHIVE_REF: usize = 1;

impl Shared {
    fn new(host: Arc<dyn Host>) -> Box<Shared> {
        let shared = Box::new(Shared {
            host,
            audio: sys::ARAAudioAccessControllerInterface {
                structSize: implemented_size!(
                    sys::ARAAudioAccessControllerInterface,
                    destroyAudioReader
                ),
                createAudioReaderForSource: Some(create_audio_reader),
                readAudioSamples: Some(read_audio_samples),
                destroyAudioReader: Some(destroy_audio_reader),
            },
            archiving: sys::ARAArchivingControllerInterface {
                structSize: implemented_size!(
                    sys::ARAArchivingControllerInterface,
                    getDocumentArchiveID
                ),
                getArchiveSize: Some(get_archive_size),
                readBytesFromArchive: Some(read_bytes_from_archive),
                writeBytesToArchive: Some(write_bytes_to_archive),
                notifyDocumentArchivingProgress: Some(archiving_progress),
                notifyDocumentUnarchivingProgress: Some(unarchiving_progress),
                getDocumentArchiveID: Some(get_document_archive_id),
            },
            content: sys::ARAContentAccessControllerInterface {
                structSize: implemented_size!(
                    sys::ARAContentAccessControllerInterface,
                    destroyContentReader
                ),
                isMusicalContextContentAvailable: Some(is_musical_context_content_available),
                getMusicalContextContentGrade: Some(get_musical_context_content_grade),
                createMusicalContextContentReader: Some(create_musical_context_content_reader),
                isAudioSourceContentAvailable: Some(is_audio_source_content_available),
                getAudioSourceContentGrade: Some(get_audio_source_content_grade),
                createAudioSourceContentReader: Some(create_audio_source_content_reader),
                getContentReaderEventCount: Some(get_content_reader_event_count),
                getContentReaderDataForEvent: Some(get_content_reader_data_for_event),
                destroyContentReader: Some(destroy_content_reader),
            },
            updates: sys::ARAModelUpdateControllerInterface {
                structSize: implemented_size!(
                    sys::ARAModelUpdateControllerInterface,
                    notifyDocumentDataChanged
                ),
                notifyAudioSourceAnalysisProgress: Some(notify_analysis_progress),
                notifyAudioSourceContentChanged: Some(notify_source_content_changed),
                notifyAudioModificationContentChanged: Some(notify_modification_content_changed),
                notifyPlaybackRegionContentChanged: Some(notify_region_content_changed),
                notifyDocumentDataChanged: Some(notify_document_data_changed),
            },
            playback: sys::ARAPlaybackControllerInterface {
                structSize: implemented_size!(
                    sys::ARAPlaybackControllerInterface,
                    requestEnableCycle
                ),
                requestStartPlayback: Some(request_start),
                requestStopPlayback: Some(request_stop),
                requestSetPlaybackPosition: Some(request_position),
                requestSetCycleRange: Some(request_cycle),
                requestEnableCycle: Some(request_enable_cycle),
            },
            // SAFETY: zeroed is valid (nulls); filled in below, before any plug-in sees it.
            instance: UnsafeCell::new(unsafe { std::mem::zeroed() }),
            channels: Mutex::new(HashMap::new()),
            readers: Mutex::new(HashMap::new()),
            content_readers: Mutex::new(HashMap::new()),
            next_content_reader: AtomicUsize::new(1),
            archive_in: Mutex::new(None),
            archive_out: Mutex::new(None),
        });
        let me = std::ptr::from_ref(&*shared).cast_mut().cast::<c_void>();
        // SAFETY: not yet shared.
        unsafe {
            *shared.instance.get() = sys::ARADocumentControllerHostInstance {
                structSize: implemented_size!(
                    sys::ARADocumentControllerHostInstance,
                    playbackControllerInterface
                ),
                audioAccessControllerHostRef: me.cast(),
                audioAccessControllerInterface: &raw const shared.audio,
                archivingControllerHostRef: me.cast(),
                archivingControllerInterface: &raw const shared.archiving,
                contentAccessControllerHostRef: me.cast(),
                contentAccessControllerInterface: &raw const shared.content,
                modelUpdateControllerHostRef: me.cast(),
                modelUpdateControllerInterface: &raw const shared.updates,
                playbackControllerHostRef: me.cast(),
                playbackControllerInterface: &raw const shared.playback,
            };
        }
        shared
    }
}

/// # Safety
/// `r` must be a controller host ref this module handed out (a live `Shared`'s address).
unsafe fn shared<'a, T>(r: *mut T) -> &'a Shared {
    // SAFETY: every controller host ref is the address of the live `Shared`.
    unsafe { &*(r as *const Shared) }
}

unsafe extern "C" fn create_audio_reader(
    c: sys::ARAAudioAccessControllerHostRef,
    source: sys::ARAAudioSourceHostRef,
    use_64_bit: sys::ARABool,
) -> sys::ARAAudioReaderHostRef {
    // SAFETY: the controller host ref this module gave the plug-in.
    let s = unsafe { shared(c) };
    let source = source as usize;
    let Some(channels) = lock(&s.channels).get(&source).copied() else {
        return std::ptr::null_mut();
    };
    let wide = use_64_bit != sys::kARAFalse;
    match s.host.create_audio_reader(source, wide) {
        Some(reader) if reader != 0 => {
            lock(&s.readers).insert(reader, (channels, wide));
            reader as sys::ARAAudioReaderHostRef
        }
        _ => std::ptr::null_mut(),
    }
}

unsafe extern "C" fn read_audio_samples(
    c: sys::ARAAudioAccessControllerHostRef,
    reader: sys::ARAAudioReaderHostRef,
    position: sys::ARASamplePosition,
    count: sys::ARASampleCount,
    buffers: *const *mut c_void,
) -> sys::ARABool {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    let reader = reader as usize;
    let Some((channels, wide)) = lock(&s.readers).get(&reader).copied() else {
        return sys::kARAFalse;
    };
    let Ok(count) = usize::try_from(count) else {
        return sys::kARAFalse;
    };
    if buffers.is_null() {
        return sys::kARAFalse;
    }
    // SAFETY: the plug-in passes one buffer per channel of `count` samples of the reader's size,
    // for this call.
    let samples = unsafe {
        let buffer = |channel: usize| *buffers.add(channel);
        if wide {
            Samples::F64(
                (0..channels)
                    .map(|ch| std::slice::from_raw_parts_mut(buffer(ch).cast::<f64>(), count))
                    .collect(),
            )
        } else {
            Samples::F32(
                (0..channels)
                    .map(|ch| std::slice::from_raw_parts_mut(buffer(ch).cast::<f32>(), count))
                    .collect(),
            )
        }
    };
    crate::util::ara_bool(s.host.read_audio_samples(reader, position, samples))
}

unsafe extern "C" fn destroy_audio_reader(
    c: sys::ARAAudioAccessControllerHostRef,
    reader: sys::ARAAudioReaderHostRef,
) {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    let reader = reader as usize;
    if lock(&s.readers).remove(&reader).is_some() {
        s.host.destroy_audio_reader(reader);
    }
}

unsafe extern "C" fn get_archive_size(
    c: sys::ARAArchivingControllerHostRef,
    r: sys::ARAArchiveReaderHostRef,
) -> sys::ARASize {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    if r as usize != ARCHIVE_REF {
        return 0;
    }
    lock(&s.archive_in)
        .as_ref()
        .map_or(0, |archive| archive.bytes.len())
}

unsafe extern "C" fn read_bytes_from_archive(
    c: sys::ARAArchivingControllerHostRef,
    r: sys::ARAArchiveReaderHostRef,
    position: sys::ARASize,
    length: sys::ARASize,
    buffer: *mut sys::ARAByte,
) -> sys::ARABool {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    if r as usize != ARCHIVE_REF || buffer.is_null() {
        return sys::kARAFalse;
    }
    let archive = lock(&s.archive_in);
    let Some(bytes) = archive
        .as_ref()
        .and_then(|archive| archive.bytes.get(position..position.checked_add(length)?))
    else {
        return sys::kARAFalse;
    };
    // SAFETY: the plug-in's buffer holds `length` bytes.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, length) };
    sys::kARATrue
}

unsafe extern "C" fn write_bytes_to_archive(
    c: sys::ARAArchivingControllerHostRef,
    w: sys::ARAArchiveWriterHostRef,
    position: sys::ARASize,
    length: sys::ARASize,
    buffer: *const sys::ARAByte,
) -> sys::ARABool {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    if w as usize != ARCHIVE_REF || (buffer.is_null() && length > 0) {
        return sys::kARAFalse;
    }
    let mut archive = lock(&s.archive_out);
    let (Some(bytes), Some(end)) = (archive.as_mut(), position.checked_add(length)) else {
        return sys::kARAFalse;
    };
    if bytes.len() < end {
        bytes.resize(end, 0);
    }
    if length > 0 {
        // SAFETY: the plug-in's buffer holds `length` bytes.
        let from = unsafe { std::slice::from_raw_parts(buffer, length) };
        bytes[position..end].copy_from_slice(from);
    }
    sys::kARATrue
}

unsafe extern "C" fn archiving_progress(c: sys::ARAArchivingControllerHostRef, value: f32) {
    // SAFETY: as above.
    unsafe { shared(c) }.host.archiving_progress(value);
}

unsafe extern "C" fn unarchiving_progress(c: sys::ARAArchivingControllerHostRef, value: f32) {
    // SAFETY: as above.
    unsafe { shared(c) }.host.unarchiving_progress(value);
}

unsafe extern "C" fn get_document_archive_id(
    c: sys::ARAArchivingControllerHostRef,
    r: sys::ARAArchiveReaderHostRef,
) -> sys::ARAPersistentID {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    if r as usize != ARCHIVE_REF {
        return std::ptr::null();
    }
    // The string lives in the archive, which the restore holds until it returns.
    lock(&s.archive_in)
        .as_ref()
        .map_or(std::ptr::null(), |archive| archive.archive_id.as_ptr())
}

fn content_of(s: &Shared, context: usize, ty: sys::ARAContentType) -> Option<MusicalContent> {
    let content = s.host.musical_context_content(context)?;
    let has = match ty {
        sys::kARAContentTypeTempoEntries => !content.tempo.is_empty(),
        sys::kARAContentTypeBarSignatures => !content.bars.is_empty(),
        sys::kARAContentTypeKeySignatures => !content.keys.is_empty(),
        _ => false,
    };
    has.then_some(content)
}

unsafe extern "C" fn is_musical_context_content_available(
    c: sys::ARAContentAccessControllerHostRef,
    context: sys::ARAMusicalContextHostRef,
    ty: sys::ARAContentType,
) -> sys::ARABool {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    crate::util::ara_bool(content_of(s, context as usize, ty).is_some())
}

unsafe extern "C" fn get_musical_context_content_grade(
    c: sys::ARAContentAccessControllerHostRef,
    context: sys::ARAMusicalContextHostRef,
    ty: sys::ARAContentType,
) -> sys::ARAContentGrade {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    content_of(s, context as usize, ty)
        .and_then(|content| content.grade)
        .unwrap_or(ContentGrade::Initial)
        .raw()
}

unsafe extern "C" fn create_musical_context_content_reader(
    c: sys::ARAContentAccessControllerHostRef,
    context: sys::ARAMusicalContextHostRef,
    ty: sys::ARAContentType,
    _range: *const sys::ARAContentTimeRange,
) -> sys::ARAContentReaderHostRef {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    let Some(content) = content_of(s, context as usize, ty) else {
        return std::ptr::null_mut();
    };
    let events = match ty {
        sys::kARAContentTypeTempoEntries => ContentEvents::Tempo(
            content
                .tempo
                .iter()
                .map(|t| sys::ARAContentTempoEntry {
                    timePosition: t.time_position,
                    quarterPosition: t.quarter_position,
                })
                .collect(),
        ),
        sys::kARAContentTypeBarSignatures => ContentEvents::Bars(
            content
                .bars
                .iter()
                .map(|b| sys::ARAContentBarSignature {
                    numerator: b.numerator,
                    denominator: b.denominator,
                    position: b.position,
                })
                .collect(),
        ),
        _ => {
            let names: Vec<CString> = content
                .keys
                .iter()
                .map(|k| c_string(k.name.as_deref().unwrap_or("")))
                .collect();
            let events = content
                .keys
                .iter()
                .zip(&names)
                .map(|(k, n)| sys::ARAContentKeySignature {
                    root: k.root,
                    intervals: k.intervals.map(|used| if used { 0xFF } else { 0 }),
                    name: if k.name.is_some() {
                        n.as_ptr()
                    } else {
                        std::ptr::null()
                    },
                    position: k.position,
                })
                .collect();
            ContentEvents::Keys {
                events,
                _names: names,
            }
        }
    };
    let id = s.next_content_reader.fetch_add(1, Ordering::Relaxed);
    lock(&s.content_readers).insert(id, Box::new(events));
    id as sys::ARAContentReaderHostRef
}

unsafe extern "C" fn is_audio_source_content_available(
    _c: sys::ARAContentAccessControllerHostRef,
    _source: sys::ARAAudioSourceHostRef,
    _ty: sys::ARAContentType,
) -> sys::ARABool {
    sys::kARAFalse
}

unsafe extern "C" fn get_audio_source_content_grade(
    _c: sys::ARAContentAccessControllerHostRef,
    _source: sys::ARAAudioSourceHostRef,
    _ty: sys::ARAContentType,
) -> sys::ARAContentGrade {
    sys::kARAContentGradeInitial
}

unsafe extern "C" fn create_audio_source_content_reader(
    _c: sys::ARAContentAccessControllerHostRef,
    _source: sys::ARAAudioSourceHostRef,
    _ty: sys::ARAContentType,
    _range: *const sys::ARAContentTimeRange,
) -> sys::ARAContentReaderHostRef {
    std::ptr::null_mut()
}

unsafe extern "C" fn get_content_reader_event_count(
    c: sys::ARAContentAccessControllerHostRef,
    r: sys::ARAContentReaderHostRef,
) -> sys::ARAInt32 {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    lock(&s.content_readers)
        .get(&(r as usize))
        .map_or(0, |events| events.len() as sys::ARAInt32)
}

unsafe extern "C" fn get_content_reader_data_for_event(
    c: sys::ARAContentAccessControllerHostRef,
    r: sys::ARAContentReaderHostRef,
    index: sys::ARAInt32,
) -> *const c_void {
    // SAFETY: as above.
    let s = unsafe { shared(c) };
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null();
    };
    // The events are boxed, so the pointer holds past the lock, until the reader is destroyed.
    lock(&s.content_readers)
        .get(&(r as usize))
        .map_or(std::ptr::null(), |events| events.get(index))
}

unsafe extern "C" fn destroy_content_reader(
    c: sys::ARAContentAccessControllerHostRef,
    r: sys::ARAContentReaderHostRef,
) {
    // SAFETY: as above.
    lock(&unsafe { shared(c) }.content_readers).remove(&(r as usize));
}

unsafe extern "C" fn notify_analysis_progress(
    c: sys::ARAModelUpdateControllerHostRef,
    source: sys::ARAAudioSourceHostRef,
    state: sys::ARAAnalysisProgressState,
    value: f32,
) {
    let state = match state {
        sys::kARAAnalysisProgressStarted => AnalysisState::Started,
        sys::kARAAnalysisProgressCompleted => AnalysisState::Completed,
        _ => AnalysisState::Updated,
    };
    // SAFETY: as above.
    unsafe { shared(c) }
        .host
        .analysis_progress(source as usize, state, value);
}

unsafe extern "C" fn notify_source_content_changed(
    c: sys::ARAModelUpdateControllerHostRef,
    source: sys::ARAAudioSourceHostRef,
    _range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    // SAFETY: as above.
    unsafe { shared(c) }
        .host
        .audio_source_content_changed(source as usize, ContentUpdateFlags(flags));
}

unsafe extern "C" fn notify_modification_content_changed(
    c: sys::ARAModelUpdateControllerHostRef,
    modification: sys::ARAAudioModificationHostRef,
    _range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    // SAFETY: as above.
    unsafe { shared(c) }
        .host
        .audio_modification_content_changed(modification as usize, ContentUpdateFlags(flags));
}

unsafe extern "C" fn notify_region_content_changed(
    c: sys::ARAModelUpdateControllerHostRef,
    region: sys::ARAPlaybackRegionHostRef,
    _range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    // SAFETY: as above.
    unsafe { shared(c) }
        .host
        .playback_region_content_changed(region as usize, ContentUpdateFlags(flags));
}

unsafe extern "C" fn notify_document_data_changed(c: sys::ARAModelUpdateControllerHostRef) {
    // SAFETY: as above.
    unsafe { shared(c) }.host.document_data_changed();
}

unsafe extern "C" fn request_start(c: sys::ARAPlaybackControllerHostRef) {
    // SAFETY: as above.
    unsafe { shared(c) }.host.request_start_playback();
}

unsafe extern "C" fn request_stop(c: sys::ARAPlaybackControllerHostRef) {
    // SAFETY: as above.
    unsafe { shared(c) }.host.request_stop_playback();
}

unsafe extern "C" fn request_position(
    c: sys::ARAPlaybackControllerHostRef,
    t: sys::ARATimePosition,
) {
    // SAFETY: as above.
    unsafe { shared(c) }.host.request_set_playback_position(t);
}

unsafe extern "C" fn request_cycle(
    c: sys::ARAPlaybackControllerHostRef,
    start: sys::ARATimePosition,
    duration: sys::ARATimeDuration,
) {
    // SAFETY: as above.
    unsafe { shared(c) }
        .host
        .request_set_cycle_range(start, duration);
}

unsafe extern "C" fn request_enable_cycle(c: sys::ARAPlaybackControllerHostRef, e: sys::ARABool) {
    // SAFETY: as above.
    unsafe { shared(c) }
        .host
        .request_enable_cycle(e != sys::kARAFalse);
}

// ---- the document controller -------------------------------------------------------------------

macro_rules! plug_in_ref {
    ($(#[$doc:meta])* $name:ident, $raw:ty) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(usize);

        impl $name {
            fn raw(self) -> $raw {
                self.0 as $raw
            }

            fn of(raw: $raw) -> Option<Self> {
                (!raw.is_null()).then_some(Self(raw as usize))
            }
        }
    };
}

plug_in_ref!(
    /// The plug-in's musical context.
    MusicalContextRef,
    sys::ARAMusicalContextRef
);
plug_in_ref!(
    /// The plug-in's region sequence.
    RegionSequenceRef,
    sys::ARARegionSequenceRef
);
plug_in_ref!(
    /// The plug-in's audio source.
    SourceRef,
    sys::ARAAudioSourceRef
);
plug_in_ref!(
    /// The plug-in's audio modification.
    ModificationRef,
    sys::ARAAudioModificationRef
);
plug_in_ref!(
    /// The plug-in's playback region.
    RegionRef,
    sys::ARAPlaybackRegionRef
);

/// An RGB colour, each 0 to 1 (`ARAColor`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    fn raw(self) -> sys::ARAColor {
        sys::ARAColor {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }
}

/// A musical context's properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicalContextProperties {
    pub name: Option<String>,
    pub order_index: i32,
    pub color: Option<Color>,
}

/// A region sequence's properties: a track.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionSequenceProperties {
    pub name: Option<String>,
    pub order_index: i32,
    pub context: MusicalContextRef,
    pub color: Option<Color>,
}

/// An audio source's properties: a recording, at its own rate.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceProperties {
    pub name: Option<String>,
    pub persistent_id: String,
    pub sample_count: i64,
    pub sample_rate: f64,
    pub channel_count: usize,
    pub merits_64_bit_samples: bool,
}

/// An audio modification's properties.
#[derive(Clone, Debug, PartialEq)]
pub struct ModificationProperties {
    pub name: Option<String>,
    pub persistent_id: String,
}

/// A playback region's properties: where a modification plays, in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionProperties {
    /// `ARAPlaybackTransformationFlags`, only those the factory supports.
    pub transformation_flags: i32,
    pub start_in_modification: f64,
    pub duration_in_modification: f64,
    pub start_in_playback: f64,
    pub duration_in_playback: f64,
    pub sequence: RegionSequenceRef,
    pub name: Option<String>,
    pub color: Option<Color>,
}

/// What a store writes (`ARAStoreObjectsFilter`); `None` for everything.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreFilter {
    pub document_data: bool,
    pub sources: Vec<SourceRef>,
    pub modifications: Vec<ModificationRef>,
}

/// What a restore reads (`ARARestoreObjectsFilter`): archived persistent ids, each with the
/// current id it maps to (`None`: the same).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RestoreFilter {
    pub document_data: bool,
    pub sources: Vec<(String, Option<String>)>,
    pub modifications: Vec<(String, Option<String>)>,
}

/// `getPlaybackRegionHeadAndTailTime`, which ARA lets a host call from any thread, apart from the
/// document controller.
#[derive(Clone, Copy)]
pub struct HeadTail {
    f: unsafe extern "C" fn(
        sys::ARADocumentControllerRef,
        sys::ARAPlaybackRegionRef,
        *mut f64,
        *mut f64,
    ),
    dc: usize,
}

impl HeadTail {
    /// The region's head and tail, in seconds. The region must be live: the call trusts it.
    pub fn of(&self, region: RegionRef) -> (f64, f64) {
        let (mut head, mut tail) = (0.0, 0.0);
        // SAFETY: the plug-in's entry point, any thread, a region of its document.
        unsafe { (self.f)(self.dc as _, region.raw(), &mut head, &mut tail) };
        (head.max(0.0), tail.max(0.0))
    }
}

/// A document of the plug-in's, created by [`Initialized::create_document`]: every call on the
/// thread that created it (a call from another is refused, `None`), each `None` where the
/// plug-in's interface lacks it. Destroyed when dropped; the host's interfaces live until then.
pub struct DocumentController {
    shared: Box<Shared>,
    dc: usize,
    i: sys::ARADocumentControllerInterface,
    thread: ThreadId,
    init: Arc<Initialized>,
    destroyed: bool,
}

impl std::fmt::Debug for DocumentController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentController")
            .field("dc", &self.dc)
            .field("factory", &self.init.factory)
            .finish()
    }
}

/// Calls the plug-in's `$f` with the controller ref first, on the controller's thread; `None`
/// off it or where the plug-in lacks the call.
macro_rules! dc {
    ($self:ident . $f:ident ( $($arg:expr),* )) => {{
        if std::thread::current().id() != $self.thread || $self.destroyed {
            None
        } else {
            $self.i.$f.map(|f| {
                // SAFETY: the plug-in's interface, on the controller's thread, with arguments the
                // caller made valid for the call.
                unsafe { f($self.dc as _ $(, $arg)*) }
            })
        }
    }};
}

fn opt_c(s: &Option<String>) -> Option<CString> {
    s.as_deref().map(c_string)
}

fn opt_ptr(s: &Option<CString>) -> *const c_char {
    s.as_ref().map_or(std::ptr::null(), |s| s.as_ptr())
}

impl DocumentController {
    /// The `ARADocumentControllerRef`, for binding instances to this document.
    pub fn controller_ref(&self) -> *const c_void {
        self.dc as *const c_void
    }

    pub fn factory(&self) -> Factory {
        self.init.factory
    }

    pub fn begin_editing(&self) -> Option<()> {
        dc!(self.beginEditing())
    }

    pub fn end_editing(&self) -> Option<()> {
        dc!(self.endEditing())
    }

    /// Runs `f` inside `beginEditing` … `endEditing`.
    pub fn edit<R>(&self, f: impl FnOnce(&Self) -> R) -> R {
        self.begin_editing();
        let r = f(self);
        self.end_editing();
        r
    }

    /// `notifyModelUpdates`: the plug-in tells the host what changed, through [`Host`], now.
    /// Call it often, never inside an edit or a restore.
    pub fn notify_model_updates(&self) -> Option<()> {
        dc!(self.notifyModelUpdates())
    }

    pub fn update_document_name(&self, name: &str) -> Option<()> {
        let name = c_string(name);
        let p = sys::ARADocumentProperties {
            structSize: implemented_size!(sys::ARADocumentProperties, name),
            name: name.as_ptr(),
        };
        dc!(self.updateDocumentProperties(&p))
    }

    pub fn create_musical_context(
        &self,
        host_ref: usize,
        properties: &MusicalContextProperties,
    ) -> Option<MusicalContextRef> {
        let name = opt_c(&properties.name);
        let color = properties.color.map(Color::raw);
        let p = sys::ARAMusicalContextProperties {
            structSize: implemented_size!(sys::ARAMusicalContextProperties, color),
            name: opt_ptr(&name),
            orderIndex: properties.order_index,
            color: color.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
        };
        dc!(self.createMusicalContext(host_ref as _, &p)).and_then(MusicalContextRef::of)
    }

    pub fn update_musical_context(
        &self,
        context: MusicalContextRef,
        properties: &MusicalContextProperties,
    ) -> Option<()> {
        let name = opt_c(&properties.name);
        let color = properties.color.map(Color::raw);
        let p = sys::ARAMusicalContextProperties {
            structSize: implemented_size!(sys::ARAMusicalContextProperties, color),
            name: opt_ptr(&name),
            orderIndex: properties.order_index,
            color: color.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
        };
        dc!(self.updateMusicalContextProperties(context.raw(), &p))
    }

    /// The host's content of the context changed (its tempo, bars or key): everything, or what
    /// `flags` leave out.
    pub fn update_musical_context_content(
        &self,
        context: MusicalContextRef,
        flags: ContentUpdateFlags,
    ) -> Option<()> {
        dc!(self.updateMusicalContextContent(context.raw(), std::ptr::null(), flags.0))
    }

    pub fn destroy_musical_context(&self, context: MusicalContextRef) -> Option<()> {
        dc!(self.destroyMusicalContext(context.raw()))
    }

    fn sequence_properties(
        properties: &RegionSequenceProperties,
        name: &Option<CString>,
        color: &Option<sys::ARAColor>,
    ) -> sys::ARARegionSequenceProperties {
        sys::ARARegionSequenceProperties {
            structSize: implemented_size!(sys::ARARegionSequenceProperties, color),
            name: opt_ptr(name),
            orderIndex: properties.order_index,
            musicalContextRef: properties.context.raw(),
            color: color.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
        }
    }

    pub fn create_region_sequence(
        &self,
        host_ref: usize,
        properties: &RegionSequenceProperties,
    ) -> Option<RegionSequenceRef> {
        let (name, color) = (opt_c(&properties.name), properties.color.map(Color::raw));
        let p = Self::sequence_properties(properties, &name, &color);
        dc!(self.createRegionSequence(host_ref as _, &p)).and_then(RegionSequenceRef::of)
    }

    pub fn update_region_sequence(
        &self,
        sequence: RegionSequenceRef,
        properties: &RegionSequenceProperties,
    ) -> Option<()> {
        let (name, color) = (opt_c(&properties.name), properties.color.map(Color::raw));
        let p = Self::sequence_properties(properties, &name, &color);
        dc!(self.updateRegionSequenceProperties(sequence.raw(), &p))
    }

    pub fn destroy_region_sequence(&self, sequence: RegionSequenceRef) -> Option<()> {
        dc!(self.destroyRegionSequence(sequence.raw()))
    }

    fn source_properties(
        properties: &SourceProperties,
        name: &Option<CString>,
        id: &CString,
    ) -> sys::ARAAudioSourceProperties {
        sys::ARAAudioSourceProperties {
            structSize: implemented_size!(sys::ARAAudioSourceProperties, merits64BitSamples),
            name: opt_ptr(name),
            persistentID: id.as_ptr(),
            sampleCount: properties.sample_count,
            sampleRate: properties.sample_rate,
            channelCount: properties.channel_count as sys::ARAChannelCount,
            merits64BitSamples: crate::util::ara_bool(properties.merits_64_bit_samples),
            channelArrangementDataType: sys::kARAChannelArrangementUndefined,
            channelArrangement: std::ptr::null(),
        }
    }

    /// A source the host knows as `host_ref` (never 0): its readers are made through [`Host`]
    /// with it. Its sample access starts disabled.
    pub fn create_audio_source(
        &self,
        host_ref: usize,
        properties: &SourceProperties,
    ) -> Option<SourceRef> {
        let (name, id) = (opt_c(&properties.name), c_string(&properties.persistent_id));
        let p = Self::source_properties(properties, &name, &id);
        let made = dc!(self.createAudioSource(host_ref as _, &p)).and_then(SourceRef::of)?;
        lock(&self.shared.channels).insert(host_ref, properties.channel_count);
        Some(made)
    }

    /// New properties: only while its sample access is disabled, when they change its samples.
    pub fn update_audio_source(
        &self,
        source: SourceRef,
        host_ref: usize,
        properties: &SourceProperties,
    ) -> Option<()> {
        let (name, id) = (opt_c(&properties.name), c_string(&properties.persistent_id));
        let p = Self::source_properties(properties, &name, &id);
        dc!(self.updateAudioSourceProperties(source.raw(), &p))?;
        lock(&self.shared.channels).insert(host_ref, properties.channel_count);
        Some(())
    }

    pub fn update_audio_source_content(
        &self,
        source: SourceRef,
        flags: ContentUpdateFlags,
    ) -> Option<()> {
        dc!(self.updateAudioSourceContent(source.raw(), std::ptr::null(), flags.0))
    }

    /// `enableAudioSourceSamplesAccess`: a disable returns once the plug-in's reads of it are
    /// over and its readers destroyed.
    pub fn enable_audio_source_samples_access(
        &self,
        source: SourceRef,
        enable: bool,
    ) -> Option<()> {
        dc!(self.enableAudioSourceSamplesAccess(source.raw(), crate::util::ara_bool(enable)))
    }

    pub fn deactivate_audio_source_for_undo_history(
        &self,
        source: SourceRef,
        deactivate: bool,
    ) -> Option<()> {
        dc!(self
            .deactivateAudioSourceForUndoHistory(source.raw(), crate::util::ara_bool(deactivate)))
    }

    pub fn destroy_audio_source(&self, source: SourceRef, host_ref: usize) -> Option<()> {
        dc!(self.destroyAudioSource(source.raw()))?;
        lock(&self.shared.channels).remove(&host_ref);
        Some(())
    }

    /// `isAudioSourceContentAnalysisIncomplete` for one content type.
    pub fn is_analysis_incomplete(&self, source: SourceRef, ty: ContentType) -> Option<bool> {
        dc!(self.isAudioSourceContentAnalysisIncomplete(source.raw(), ty.raw()))
            .map(|incomplete| incomplete != sys::kARAFalse)
    }

    fn modification_properties(
        name: &Option<CString>,
        id: &CString,
    ) -> sys::ARAAudioModificationProperties {
        sys::ARAAudioModificationProperties {
            structSize: implemented_size!(sys::ARAAudioModificationProperties, persistentID),
            name: opt_ptr(name),
            persistentID: id.as_ptr(),
        }
    }

    pub fn create_audio_modification(
        &self,
        source: SourceRef,
        host_ref: usize,
        properties: &ModificationProperties,
    ) -> Option<ModificationRef> {
        let (name, id) = (opt_c(&properties.name), c_string(&properties.persistent_id));
        let p = Self::modification_properties(&name, &id);
        dc!(self.createAudioModification(source.raw(), host_ref as _, &p))
            .and_then(ModificationRef::of)
    }

    /// A copy of `original` with the plug-in's edits (`cloneAudioModification`).
    pub fn clone_audio_modification(
        &self,
        original: ModificationRef,
        host_ref: usize,
        properties: &ModificationProperties,
    ) -> Option<ModificationRef> {
        let (name, id) = (opt_c(&properties.name), c_string(&properties.persistent_id));
        let p = Self::modification_properties(&name, &id);
        dc!(self.cloneAudioModification(original.raw(), host_ref as _, &p))
            .and_then(ModificationRef::of)
    }

    pub fn update_audio_modification(
        &self,
        modification: ModificationRef,
        properties: &ModificationProperties,
    ) -> Option<()> {
        let (name, id) = (opt_c(&properties.name), c_string(&properties.persistent_id));
        let p = Self::modification_properties(&name, &id);
        dc!(self.updateAudioModificationProperties(modification.raw(), &p))
    }

    pub fn deactivate_audio_modification_for_undo_history(
        &self,
        modification: ModificationRef,
        deactivate: bool,
    ) -> Option<()> {
        dc!(self.deactivateAudioModificationForUndoHistory(
            modification.raw(),
            crate::util::ara_bool(deactivate)
        ))
    }

    pub fn destroy_audio_modification(&self, modification: ModificationRef) -> Option<()> {
        dc!(self.destroyAudioModification(modification.raw()))
    }

    fn region_properties(
        properties: &RegionProperties,
        name: &Option<CString>,
        color: &Option<sys::ARAColor>,
    ) -> sys::ARAPlaybackRegionProperties {
        sys::ARAPlaybackRegionProperties {
            structSize: implemented_size!(sys::ARAPlaybackRegionProperties, color),
            transformationFlags: properties.transformation_flags,
            startInModificationTime: properties.start_in_modification,
            durationInModificationTime: properties.duration_in_modification,
            startInPlaybackTime: properties.start_in_playback,
            durationInPlaybackTime: properties.duration_in_playback,
            musicalContextRef: std::ptr::null_mut(),
            regionSequenceRef: properties.sequence.raw(),
            name: opt_ptr(name),
            color: color.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
        }
    }

    pub fn create_playback_region(
        &self,
        modification: ModificationRef,
        host_ref: usize,
        properties: &RegionProperties,
    ) -> Option<RegionRef> {
        let (name, color) = (opt_c(&properties.name), properties.color.map(Color::raw));
        let p = Self::region_properties(properties, &name, &color);
        dc!(self.createPlaybackRegion(modification.raw(), host_ref as _, &p))
            .and_then(RegionRef::of)
    }

    pub fn update_playback_region(
        &self,
        region: RegionRef,
        properties: &RegionProperties,
    ) -> Option<()> {
        let (name, color) = (opt_c(&properties.name), properties.color.map(Color::raw));
        let p = Self::region_properties(properties, &name, &color);
        dc!(self.updatePlaybackRegionProperties(region.raw(), &p))
    }

    pub fn destroy_playback_region(&self, region: RegionRef) -> Option<()> {
        dc!(self.destroyPlaybackRegion(region.raw()))
    }

    /// The head-and-tail query, for any thread.
    pub fn head_tail(&self) -> Option<HeadTail> {
        Some(HeadTail {
            f: self.i.getPlaybackRegionHeadAndTailTime?,
            dc: self.dc,
        })
    }

    /// `storeObjectsToArchive` (never inside an edit): the archive, `None` if the plug-in fails.
    pub fn store(&self, filter: Option<&StoreFilter>) -> Option<Vec<u8>> {
        let sources: Vec<_> = filter
            .map(|f| f.sources.iter().map(|s| s.raw()).collect())
            .unwrap_or_default();
        let modifications: Vec<_> = filter
            .map(|f| f.modifications.iter().map(|m| m.raw()).collect())
            .unwrap_or_default();
        let raw = filter.map(|f| sys::ARAStoreObjectsFilter {
            structSize: implemented_size!(sys::ARAStoreObjectsFilter, audioModificationRefs),
            documentData: crate::util::ara_bool(f.document_data),
            audioSourceRefsCount: sources.len(),
            audioSourceRefs: array(&sources),
            audioModificationRefsCount: modifications.len(),
            audioModificationRefs: array(&modifications),
        });
        *lock(&self.shared.archive_out) = Some(Vec::new());
        let ok = dc!(self.storeObjectsToArchive(
            ARCHIVE_REF as _,
            raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref)
        ));
        let bytes = lock(&self.shared.archive_out).take();
        (ok? != sys::kARAFalse).then_some(bytes?)
    }

    /// `restoreObjectsFromArchive`, inside an edit after the graph is built with the archive's
    /// persistent ids; `archive_id` is the `documentArchiveID` it was written with. The plug-in's
    /// result.
    pub fn restore(
        &self,
        archive: &[u8],
        archive_id: &str,
        filter: Option<&RestoreFilter>,
    ) -> Option<bool> {
        let ids = |list: &[(String, Option<String>)]| -> (Vec<CString>, Option<Vec<CString>>) {
            let archived = list.iter().map(|(a, _)| c_string(a)).collect();
            let mapped = list.iter().any(|(_, current)| current.is_some());
            let current = mapped.then(|| {
                list.iter()
                    .map(|(a, current)| c_string(current.as_deref().unwrap_or(a)))
                    .collect()
            });
            (archived, current)
        };
        let (sa, sc) = filter.map(|f| ids(&f.sources)).unwrap_or_default();
        let (ma, mc) = filter.map(|f| ids(&f.modifications)).unwrap_or_default();
        let ptrs = |v: &[CString]| v.iter().map(|c| c.as_ptr()).collect::<Vec<_>>();
        let (sap, scp, map_, mcp) = (
            ptrs(&sa),
            sc.as_deref().map(ptrs),
            ptrs(&ma),
            mc.as_deref().map(ptrs),
        );
        let raw = filter.map(|f| sys::ARARestoreObjectsFilter {
            structSize: implemented_size!(
                sys::ARARestoreObjectsFilter,
                audioModificationCurrentIDs
            ),
            documentData: crate::util::ara_bool(f.document_data),
            audioSourceIDsCount: sap.len(),
            audioSourceArchiveIDs: array(&sap),
            audioSourceCurrentIDs: scp.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            audioModificationIDsCount: map_.len(),
            audioModificationArchiveIDs: array(&map_),
            audioModificationCurrentIDs: mcp.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
        });
        *lock(&self.shared.archive_in) = Some(ArchiveIn {
            bytes: archive.to_vec(),
            archive_id: c_string(archive_id),
        });
        let ok = dc!(self.restoreObjectsFromArchive(
            ARCHIVE_REF as _,
            raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref)
        ));
        *lock(&self.shared.archive_in) = None;
        ok.map(|ok| ok != sys::kARAFalse)
    }

    fn destroy_now(&mut self) {
        if !self.destroyed {
            let _ = dc!(self.destroyDocumentController());
            self.destroyed = true;
        }
    }
}

impl Drop for DocumentController {
    /// `destroyDocumentController`, before the host's interfaces go, and before ARA may be
    /// uninitialized. Off the controller's thread the call is skipped, as every call is.
    fn drop(&mut self) {
        self.destroy_now();
    }
}

// ---- a bound instance --------------------------------------------------------------------------

/// A plug-in instance bound to a document (its companion API's binding gave the
/// `ARAPlugInExtensionInstance`): its renderers' regions and its view's selection. Every call on
/// the document controller's thread.
#[derive(Debug)]
pub struct PlugInExtension(usize);

impl PlugInExtension {
    /// The instance at `ptr`, `None` for a null one or one smaller than ARA's minimum.
    ///
    /// # Safety
    /// `ptr` must be null or a plug-in's `const ARAPlugInExtensionInstance*`, used only while that
    /// instance lives.
    pub unsafe fn from_raw(ptr: *const c_void) -> Option<Self> {
        if ptr.is_null() {
            return None;
        }
        // SAFETY: the caller's.
        let size =
            unsafe { crate::util::struct_size(ptr.cast::<sys::ARAPlugInExtensionInstance>()) };
        (size >= sys::kARAPlugInExtensionInstanceMinSize as usize).then_some(Self(ptr as usize))
    }

    fn get(&self) -> sys::ARAPlugInExtensionInstance {
        // SAFETY: the plug-in's instance struct (`from_raw`'s contract).
        unsafe { read_sized(self.0 as *const sys::ARAPlugInExtensionInstance) }
    }

    pub fn has_playback_renderer(&self) -> bool {
        !self.get().playbackRendererInterface.is_null()
    }

    pub fn has_editor_renderer(&self) -> bool {
        !self.get().editorRendererInterface.is_null()
    }

    pub fn has_editor_view(&self) -> bool {
        !self.get().editorViewInterface.is_null()
    }

    fn playback(
        &self,
    ) -> Option<(
        sys::ARAPlaybackRendererRef,
        sys::ARAPlaybackRendererInterface,
    )> {
        let e = self.get();
        let table = e.playbackRendererInterface;
        // SAFETY: the plug-in's table, read as far as it goes.
        (!table.is_null()).then(|| (e.playbackRendererRef, unsafe { read_sized(table) }))
    }

    fn editor(&self) -> Option<(sys::ARAEditorRendererRef, sys::ARAEditorRendererInterface)> {
        let e = self.get();
        let table = e.editorRendererInterface;
        // SAFETY: as above.
        (!table.is_null()).then(|| (e.editorRendererRef, unsafe { read_sized(table) }))
    }

    fn view(&self) -> Option<(sys::ARAEditorViewRef, sys::ARAEditorViewInterface)> {
        let e = self.get();
        let table = e.editorViewInterface;
        // SAFETY: as above.
        (!table.is_null()).then(|| (e.editorViewRef, unsafe { read_sized(table) }))
    }

    /// A region for its playback renderer to play: only while the instance is not active.
    pub fn add_playback_region(&self, region: RegionRef) -> Option<()> {
        let (r, i) = self.playback()?;
        // SAFETY: the plug-in's renderer, a region of its document.
        i.addPlaybackRegion.map(|f| unsafe { f(r, region.raw()) })
    }

    /// Only while the instance is not active.
    pub fn remove_playback_region(&self, region: RegionRef) -> Option<()> {
        let (r, i) = self.playback()?;
        // SAFETY: as above.
        i.removePlaybackRegion
            .map(|f| unsafe { f(r, region.raw()) })
    }

    /// A region its editor renderer previews; any time.
    pub fn add_editor_region(&self, region: RegionRef) -> Option<()> {
        let (r, i) = self.editor()?;
        // SAFETY: as above.
        i.addPlaybackRegion.map(|f| unsafe { f(r, region.raw()) })
    }

    pub fn remove_editor_region(&self, region: RegionRef) -> Option<()> {
        let (r, i) = self.editor()?;
        // SAFETY: as above.
        i.removePlaybackRegion
            .map(|f| unsafe { f(r, region.raw()) })
    }

    pub fn add_editor_sequence(&self, sequence: RegionSequenceRef) -> Option<()> {
        let (r, i) = self.editor()?;
        // SAFETY: as above.
        i.addRegionSequence.map(|f| unsafe { f(r, sequence.raw()) })
    }

    pub fn remove_editor_sequence(&self, sequence: RegionSequenceRef) -> Option<()> {
        let (r, i) = self.editor()?;
        // SAFETY: as above.
        i.removeRegionSequence
            .map(|f| unsafe { f(r, sequence.raw()) })
    }

    /// The host's selection, for its view (`notifySelection`): regions, sequences and a time
    /// range (start, duration in seconds).
    pub fn notify_selection(
        &self,
        regions: &[RegionRef],
        sequences: &[RegionSequenceRef],
        time_range: Option<(f64, f64)>,
    ) -> Option<()> {
        let (v, i) = self.view()?;
        let r: Vec<_> = regions.iter().map(|r| r.raw()).collect();
        let s: Vec<_> = sequences.iter().map(|s| s.raw()).collect();
        let range =
            time_range.map(|(start, duration)| sys::ARAContentTimeRange { start, duration });
        let selection = sys::ARAViewSelection {
            structSize: implemented_size!(sys::ARAViewSelection, timeRange),
            playbackRegionRefsCount: r.len(),
            playbackRegionRefs: array(&r),
            regionSequenceRefsCount: s.len(),
            regionSequenceRefs: array(&s),
            timeRange: range.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
        };
        // SAFETY: the plug-in's view, a selection of its document's objects.
        i.notifySelection.map(|f| unsafe { f(v, &selection) })
    }

    /// The sequences its view should hide (`notifyHideRegionSequences`).
    pub fn hide_region_sequences(&self, sequences: &[RegionSequenceRef]) -> Option<()> {
        let (v, i) = self.view()?;
        let s: Vec<_> = sequences.iter().map(|s| s.raw()).collect();
        // SAFETY: as above.
        i.notifyHideRegionSequences
            .map(|f| unsafe { f(v, s.len(), array(&s)) })
    }
}
