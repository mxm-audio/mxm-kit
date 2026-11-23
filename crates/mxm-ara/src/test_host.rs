//! A Rust ARA host for tests: it drives an `ARAFactory` and its document controller only through
//! ARA's C function tables, as a real host does, so it can drive any ARA plug-in, mxm-ara's own or
//! one loaded from a bundle.
//!
//! It keeps in-memory audio sources (read through its audio access controller with an optional
//! delay per read, to catch a read under way), an archive as a `Vec<u8>`, musical contexts with
//! tempo, bar and key content, and a record of the plug-in's model update notifications. It also
//! records every ARA rule the plug-in breaks on its side ([`TestHost::violations`]) and every
//! assert the plug-in reports ([`TestHost::asserts`]).
//!
//! One `TestHost` lives at a time in a process (the constructor waits for the last to go), since
//! ARA's initialization is per library.

use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::ThreadId;
use std::time::Duration;

use crate::content::{BarSignature, HostContent, KeySignature, TempoEntry};
use crate::sys;
use crate::util::{implemented_size, opt_string};

/// How the host presents itself.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// How long each audio read takes.
    pub read_delay: Duration,
    /// `structSize` of the model update controller; an ARA 2.0 host's lacks
    /// `notifyDocumentDataChanged` ([`HostConfig::ara_2_0`]).
    pub model_update_struct_size: usize,
    /// `structSize` of the archiving controller; one before 2.0 final lacks `getDocumentArchiveID`.
    pub archiving_struct_size: usize,
    /// What `getDocumentArchiveID` answers; the factory's own ID when `None`.
    pub document_archive_id: Option<String>,
    pub content_access: bool,
    pub model_updates: bool,
    pub playback_controller: bool,
}

impl Default for HostConfig {
    fn default() -> Self {
        HostConfig {
            read_delay: Duration::ZERO,
            model_update_struct_size: implemented_size!(
                sys::ARAModelUpdateControllerInterface,
                notifyDocumentDataChanged
            ),
            archiving_struct_size: implemented_size!(
                sys::ARAArchivingControllerInterface,
                getDocumentArchiveID
            ),
            document_archive_id: None,
            content_access: true,
            model_updates: true,
            playback_controller: true,
        }
    }
}

impl HostConfig {
    /// A host of ARA 2.0's interfaces: no `notifyDocumentDataChanged` (added in 2.3).
    pub fn ara_2_0() -> Self {
        HostConfig {
            model_update_struct_size: implemented_size!(
                sys::ARAModelUpdateControllerInterface,
                notifyPlaybackRegionContentChanged
            ),
            ..HostConfig::default()
        }
    }

    /// The archiving controller without `getDocumentArchiveID` (before 2.0 final).
    pub fn without_archive_id(self) -> Self {
        HostConfig {
            archiving_struct_size: implemented_size!(
                sys::ARAArchivingControllerInterface,
                notifyDocumentUnarchivingProgress
            ),
            ..self
        }
    }
}

/// What the plug-in told the host, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    AnalysisProgress {
        source: usize,
        state: i32,
        value: f32,
    },
    AudioSourceContentChanged {
        source: usize,
        flags: i32,
    },
    AudioModificationContentChanged {
        modification: usize,
        flags: i32,
    },
    PlaybackRegionContentChanged {
        region: usize,
        flags: i32,
    },
    DocumentDataChanged,
    ArchivingProgress(f32),
    UnarchivingProgress(f32),
    PlaybackRequest(&'static str),
}

/// A source's reads as the host saw them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceStats {
    pub reads_started: usize,
    pub reads_in_flight: usize,
    pub live_readers: usize,
    /// The threads that read (one entry per distinct thread).
    pub reading_threads: Vec<ThreadId>,
}

#[derive(PartialEq, Clone, Copy)]
enum Access {
    Disabled,
    Enabled,
}

struct HostSource {
    channels: Arc<Vec<Vec<f32>>>,
    access: Access,
    stats: SourceStats,
}

struct HostReader {
    source: usize,
    use_64_bit: bool,
    in_use: bool,
}

enum HostContentEvents {
    Tempo(Vec<sys::ARAContentTempoEntry>),
    Bars(Vec<sys::ARAContentBarSignature>),
    Keys {
        events: Vec<sys::ARAContentKeySignature>,
        _names: Vec<CString>,
    },
}

/// A musical context's content as the host keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextContent {
    pub tempo: Vec<TempoEntry>,
    pub bars: Vec<BarSignature>,
    pub keys: Vec<KeySignature>,
}

#[derive(Default)]
struct HostData {
    next_ref: usize,
    sources: HashMap<usize, HostSource>,
    contexts: HashMap<usize, ContextContent>,
    readers: HashMap<usize, HostReader>,
    content_readers: HashMap<usize, Box<HostContentEvents>>,
    archive_in: Vec<u8>,
    archive_out: Vec<u8>,
    in_notify: bool,
    events: Vec<HostEvent>,
    violations: Vec<String>,
}

impl HostData {
    fn new_ref(&mut self) -> usize {
        self.next_ref += 1;
        self.next_ref
    }
}

struct HostState {
    config: HostConfig,
    main_thread: ThreadId,
    archive_id: CString,
    audio: sys::ARAAudioAccessControllerInterface,
    archiving: sys::ARAArchivingControllerInterface,
    content: sys::ARAContentAccessControllerInterface,
    updates: sys::ARAModelUpdateControllerInterface,
    playback: sys::ARAPlaybackControllerInterface,
    instance: UnsafeCell<sys::ARADocumentControllerHostInstance>,
    data: Mutex<HostData>,
}

// SAFETY: `instance` is written once, before any plug-in sees it; the rest is a mutex or
// immutable.
unsafe impl Sync for HostState {}

impl HostState {
    fn data(&self) -> MutexGuard<'_, HostData> {
        self.data.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn violation(&self, what: &str) {
        self.data().violations.push(what.to_owned());
    }

    fn on_main_thread(&self, what: &str) {
        if std::thread::current().id() != self.main_thread {
            self.violation(&format!("{what} off the model thread"));
        }
    }

    fn event(&self, e: HostEvent) {
        self.data().events.push(e);
    }
}

/// # Safety
/// `r` must be a host ref this host handed out (its state's address).
unsafe fn state<'a, T>(r: *mut T) -> &'a HostState {
    // SAFETY: every controller host ref is the address of the live `HostState`.
    unsafe { &*(r as *const HostState) }
}

/// Copies a plug-in's versioned struct as far as its `structSize` goes; the rest stays zero
/// (null), as for an older plug-in that lacks it.
///
/// # Safety
/// `p` must point to a struct whose `structSize` bytes are readable.
unsafe fn read_sized<S>(p: *const S) -> S {
    // SAFETY: zero is a valid value for ARA's structs (sizes, nulls, `None`s).
    let mut out: S = unsafe { std::mem::zeroed() };
    let size = unsafe { crate::util::struct_size(p) }.min(std::mem::size_of::<S>());
    // SAFETY: `size` readable bytes at `p` (the caller's contract), within `out`.
    unsafe { std::ptr::copy_nonoverlapping(p.cast::<u8>(), (&raw mut out).cast::<u8>(), size) };
    out
}

// ---- the host's interfaces ---------------------------------------------------------------------

unsafe extern "C" fn create_audio_reader(
    c: sys::ARAAudioAccessControllerHostRef,
    source: sys::ARAAudioSourceHostRef,
    use_64_bit: sys::ARABool,
) -> sys::ARAAudioReaderHostRef {
    let st = unsafe { state(c) };
    st.on_main_thread("createAudioReaderForSource");
    let mut d = st.data();
    let source = source as usize;
    match d.sources.get(&source).map(|s| s.access) {
        None => {
            d.violations
                .push("reader created for an unknown source".into());
            std::ptr::null_mut()
        }
        Some(access) => {
            if access != Access::Enabled {
                d.violations
                    .push("reader created while sample access is disabled".into());
            }
            let id = d.new_ref();
            d.readers.insert(
                id,
                HostReader {
                    source,
                    use_64_bit: use_64_bit != sys::kARAFalse,
                    in_use: false,
                },
            );
            if let Some(s) = d.sources.get_mut(&source) {
                s.stats.live_readers += 1;
            }
            id as sys::ARAAudioReaderHostRef
        }
    }
}

unsafe extern "C" fn read_audio_samples(
    c: sys::ARAAudioAccessControllerHostRef,
    reader: sys::ARAAudioReaderHostRef,
    position: sys::ARASamplePosition,
    count: sys::ARASampleCount,
    buffers: *const *mut c_void,
) -> sys::ARABool {
    let st = unsafe { state(c) };
    let (channels, use_64_bit, source) = {
        let mut d = st.data();
        let Some(r) = d.readers.get_mut(&(reader as usize)) else {
            d.violations.push("read through an unknown reader".into());
            return sys::kARAFalse;
        };
        let was_in_use = std::mem::replace(&mut r.in_use, true);
        let (source, use_64_bit) = (r.source, r.use_64_bit);
        if was_in_use {
            d.violations
                .push("one reader used by two threads at once".into());
        }
        let Some(s) = d.sources.get_mut(&source) else {
            d.violations.push("read from a destroyed source".into());
            if let Some(r) = d.readers.get_mut(&(reader as usize)) {
                r.in_use = false;
            }
            return sys::kARAFalse;
        };
        let enabled = s.access == Access::Enabled;
        s.stats.reads_started += 1;
        s.stats.reads_in_flight += 1;
        let me = std::thread::current().id();
        if !s.stats.reading_threads.contains(&me) {
            s.stats.reading_threads.push(me);
        }
        let channels = Arc::clone(&s.channels);
        if !enabled {
            d.violations
                .push("read while sample access is disabled".into());
        }
        (channels, use_64_bit, source)
    };
    std::thread::sleep(st.config.read_delay);
    for (ch, samples) in channels.iter().enumerate() {
        // SAFETY: the plug-in passes one buffer per channel of `count` samples of its format.
        let buffer = unsafe { *buffers.add(ch) };
        for i in 0..count.max(0) as usize {
            let at = position + i as i64;
            let v = if at >= 0 {
                samples.get(at as usize).copied().unwrap_or(0.0)
            } else {
                0.0
            };
            // SAFETY: as above.
            unsafe {
                if use_64_bit {
                    *buffer.cast::<f64>().add(i) = f64::from(v);
                } else {
                    *buffer.cast::<f32>().add(i) = v;
                }
            }
        }
    }
    let mut d = st.data();
    if let Some(r) = d.readers.get_mut(&(reader as usize)) {
        r.in_use = false;
    }
    if let Some(s) = d.sources.get_mut(&source) {
        s.stats.reads_in_flight -= 1;
    }
    sys::kARATrue
}

unsafe extern "C" fn destroy_audio_reader(
    c: sys::ARAAudioAccessControllerHostRef,
    reader: sys::ARAAudioReaderHostRef,
) {
    let st = unsafe { state(c) };
    st.on_main_thread("destroyAudioReader");
    let mut d = st.data();
    match d.readers.remove(&(reader as usize)) {
        None => d.violations.push("unknown reader destroyed".into()),
        Some(r) => {
            if r.in_use {
                d.violations
                    .push("reader destroyed while a read is under way".into());
            }
            if let Some(s) = d.sources.get_mut(&r.source) {
                s.stats.live_readers -= 1;
            }
        }
    }
}

unsafe extern "C" fn get_archive_size(
    c: sys::ARAArchivingControllerHostRef,
    _r: sys::ARAArchiveReaderHostRef,
) -> sys::ARASize {
    unsafe { state(c) }.data().archive_in.len()
}

unsafe extern "C" fn read_bytes_from_archive(
    c: sys::ARAArchivingControllerHostRef,
    _r: sys::ARAArchiveReaderHostRef,
    position: sys::ARASize,
    length: sys::ARASize,
    buffer: *mut sys::ARAByte,
) -> sys::ARABool {
    let st = unsafe { state(c) };
    let mut d = st.data();
    let Some(bytes) = d.archive_in.get(position..position + length) else {
        d.violations.push("read past the archive's end".into());
        return sys::kARAFalse;
    };
    // SAFETY: the plug-in's buffer holds `length` bytes.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, length) };
    sys::kARATrue
}

unsafe extern "C" fn write_bytes_to_archive(
    c: sys::ARAArchivingControllerHostRef,
    _w: sys::ARAArchiveWriterHostRef,
    position: sys::ARASize,
    length: sys::ARASize,
    buffer: *const sys::ARAByte,
) -> sys::ARABool {
    let st = unsafe { state(c) };
    let mut d = st.data();
    if d.archive_out.len() < position + length {
        d.archive_out.resize(position + length, 0);
    }
    // SAFETY: the plug-in's buffer holds `length` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(buffer, length) };
    d.archive_out[position..position + length].copy_from_slice(bytes);
    sys::kARATrue
}

unsafe extern "C" fn archiving_progress(c: sys::ARAArchivingControllerHostRef, value: f32) {
    unsafe { state(c) }.event(HostEvent::ArchivingProgress(value));
}

unsafe extern "C" fn unarchiving_progress(c: sys::ARAArchivingControllerHostRef, value: f32) {
    unsafe { state(c) }.event(HostEvent::UnarchivingProgress(value));
}

unsafe extern "C" fn get_document_archive_id(
    c: sys::ARAArchivingControllerHostRef,
    _r: sys::ARAArchiveReaderHostRef,
) -> sys::ARAPersistentID {
    unsafe { state(c) }.archive_id.as_ptr()
}

unsafe extern "C" fn is_musical_context_content_available(
    c: sys::ARAContentAccessControllerHostRef,
    context: sys::ARAMusicalContextHostRef,
    ty: sys::ARAContentType,
) -> sys::ARABool {
    let st = unsafe { state(c) };
    st.on_main_thread("isMusicalContextContentAvailable");
    let d = st.data();
    let Some(content) = d.contexts.get(&(context as usize)) else {
        return sys::kARAFalse;
    };
    let available = match ty {
        sys::kARAContentTypeTempoEntries => !content.tempo.is_empty(),
        sys::kARAContentTypeBarSignatures => !content.bars.is_empty(),
        sys::kARAContentTypeKeySignatures => !content.keys.is_empty(),
        _ => false,
    };
    crate::util::ara_bool(available)
}

unsafe extern "C" fn get_musical_context_content_grade(
    _c: sys::ARAContentAccessControllerHostRef,
    _context: sys::ARAMusicalContextHostRef,
    _ty: sys::ARAContentType,
) -> sys::ARAContentGrade {
    sys::kARAContentGradeAdjusted
}

unsafe extern "C" fn create_musical_context_content_reader(
    c: sys::ARAContentAccessControllerHostRef,
    context: sys::ARAMusicalContextHostRef,
    ty: sys::ARAContentType,
    _range: *const sys::ARAContentTimeRange,
) -> sys::ARAContentReaderHostRef {
    let st = unsafe { state(c) };
    st.on_main_thread("createMusicalContextContentReader");
    let mut d = st.data();
    let Some(content) = d.contexts.get(&(context as usize)).cloned() else {
        d.violations
            .push("content reader for an unknown musical context".into());
        return std::ptr::null_mut();
    };
    let events = match ty {
        sys::kARAContentTypeTempoEntries => HostContentEvents::Tempo(
            content
                .tempo
                .iter()
                .map(|t| sys::ARAContentTempoEntry {
                    timePosition: t.time_position,
                    quarterPosition: t.quarter_position,
                })
                .collect(),
        ),
        sys::kARAContentTypeBarSignatures => HostContentEvents::Bars(
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
        sys::kARAContentTypeKeySignatures => {
            let names: Vec<CString> = content
                .keys
                .iter()
                .map(|k| crate::util::c_string(k.name.as_deref().unwrap_or("")))
                .collect();
            let raw = content
                .keys
                .iter()
                .zip(&names)
                .map(|(k, n)| sys::ARAContentKeySignature {
                    root: k.root,
                    intervals: k.intervals.map(|u| if u { 0xFF } else { 0 }),
                    name: if k.name.is_some() {
                        n.as_ptr()
                    } else {
                        std::ptr::null()
                    },
                    position: k.position,
                })
                .collect();
            HostContentEvents::Keys {
                events: raw,
                _names: names,
            }
        }
        _ => {
            d.violations
                .push("content reader for content the host doesn't have".into());
            return std::ptr::null_mut();
        }
    };
    let id = d.new_ref();
    d.content_readers.insert(id, Box::new(events));
    id as sys::ARAContentReaderHostRef
}

unsafe extern "C" fn is_audio_source_content_available(
    _c: sys::ARAContentAccessControllerHostRef,
    _s: sys::ARAAudioSourceHostRef,
    _ty: sys::ARAContentType,
) -> sys::ARABool {
    sys::kARAFalse
}

unsafe extern "C" fn get_audio_source_content_grade(
    _c: sys::ARAContentAccessControllerHostRef,
    _s: sys::ARAAudioSourceHostRef,
    _ty: sys::ARAContentType,
) -> sys::ARAContentGrade {
    sys::kARAContentGradeInitial
}

unsafe extern "C" fn create_audio_source_content_reader(
    c: sys::ARAContentAccessControllerHostRef,
    _s: sys::ARAAudioSourceHostRef,
    _ty: sys::ARAContentType,
    _range: *const sys::ARAContentTimeRange,
) -> sys::ARAContentReaderHostRef {
    unsafe { state(c) }.violation("audio source content reader made where none is available");
    std::ptr::null_mut()
}

unsafe extern "C" fn get_content_reader_event_count(
    c: sys::ARAContentAccessControllerHostRef,
    r: sys::ARAContentReaderHostRef,
) -> sys::ARAInt32 {
    let d = unsafe { state(c) }.data();
    match d.content_readers.get(&(r as usize)).map(|b| &**b) {
        Some(HostContentEvents::Tempo(v)) => v.len() as i32,
        Some(HostContentEvents::Bars(v)) => v.len() as i32,
        Some(HostContentEvents::Keys { events: v, .. }) => v.len() as i32,
        None => 0,
    }
}

unsafe extern "C" fn get_content_reader_data_for_event(
    c: sys::ARAContentAccessControllerHostRef,
    r: sys::ARAContentReaderHostRef,
    index: sys::ARAInt32,
) -> *const c_void {
    let d = unsafe { state(c) }.data();
    let i = index as usize;
    // The events are boxed, so the pointers outlive the lock (until the reader is destroyed).
    match d.content_readers.get(&(r as usize)).map(|b| &**b) {
        Some(HostContentEvents::Tempo(v)) => v
            .get(i)
            .map_or(std::ptr::null(), |e| std::ptr::from_ref(e).cast()),
        Some(HostContentEvents::Bars(v)) => v
            .get(i)
            .map_or(std::ptr::null(), |e| std::ptr::from_ref(e).cast()),
        Some(HostContentEvents::Keys { events: v, .. }) => v
            .get(i)
            .map_or(std::ptr::null(), |e| std::ptr::from_ref(e).cast()),
        None => std::ptr::null(),
    }
}

unsafe extern "C" fn destroy_content_reader(
    c: sys::ARAContentAccessControllerHostRef,
    r: sys::ARAContentReaderHostRef,
) {
    let st = unsafe { state(c) };
    if st.data().content_readers.remove(&(r as usize)).is_none() {
        st.violation("unknown content reader destroyed");
    }
}

fn check_in_notify(st: &HostState, what: &str) {
    if !st.data().in_notify {
        st.violation(&format!("{what} outside notifyModelUpdates"));
    }
}

unsafe extern "C" fn notify_analysis_progress(
    c: sys::ARAModelUpdateControllerHostRef,
    source: sys::ARAAudioSourceHostRef,
    state_: sys::ARAAnalysisProgressState,
    value: f32,
) {
    let st = unsafe { state(c) };
    check_in_notify(st, "notifyAudioSourceAnalysisProgress");
    st.event(HostEvent::AnalysisProgress {
        source: source as usize,
        state: state_,
        value,
    });
}

unsafe extern "C" fn notify_source_content_changed(
    c: sys::ARAModelUpdateControllerHostRef,
    source: sys::ARAAudioSourceHostRef,
    _range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    let st = unsafe { state(c) };
    check_in_notify(st, "notifyAudioSourceContentChanged");
    st.event(HostEvent::AudioSourceContentChanged {
        source: source as usize,
        flags,
    });
}

unsafe extern "C" fn notify_modification_content_changed(
    c: sys::ARAModelUpdateControllerHostRef,
    modification: sys::ARAAudioModificationHostRef,
    _range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    let st = unsafe { state(c) };
    check_in_notify(st, "notifyAudioModificationContentChanged");
    st.event(HostEvent::AudioModificationContentChanged {
        modification: modification as usize,
        flags,
    });
}

unsafe extern "C" fn notify_region_content_changed(
    c: sys::ARAModelUpdateControllerHostRef,
    region: sys::ARAPlaybackRegionHostRef,
    _range: *const sys::ARAContentTimeRange,
    flags: sys::ARAContentUpdateFlags,
) {
    let st = unsafe { state(c) };
    check_in_notify(st, "notifyPlaybackRegionContentChanged");
    st.event(HostEvent::PlaybackRegionContentChanged {
        region: region as usize,
        flags,
    });
}

unsafe extern "C" fn notify_document_data_changed(c: sys::ARAModelUpdateControllerHostRef) {
    let st = unsafe { state(c) };
    check_in_notify(st, "notifyDocumentDataChanged");
    st.event(HostEvent::DocumentDataChanged);
}

unsafe extern "C" fn request_start(c: sys::ARAPlaybackControllerHostRef) {
    unsafe { state(c) }.event(HostEvent::PlaybackRequest("start"));
}
unsafe extern "C" fn request_stop(c: sys::ARAPlaybackControllerHostRef) {
    unsafe { state(c) }.event(HostEvent::PlaybackRequest("stop"));
}
unsafe extern "C" fn request_position(
    c: sys::ARAPlaybackControllerHostRef,
    _t: sys::ARATimePosition,
) {
    unsafe { state(c) }.event(HostEvent::PlaybackRequest("position"));
}
unsafe extern "C" fn request_cycle(
    c: sys::ARAPlaybackControllerHostRef,
    _s: sys::ARATimePosition,
    _d: sys::ARATimeDuration,
) {
    unsafe { state(c) }.event(HostEvent::PlaybackRequest("cycle range"));
}
unsafe extern "C" fn request_enable_cycle(c: sys::ARAPlaybackControllerHostRef, _e: sys::ARABool) {
    unsafe { state(c) }.event(HostEvent::PlaybackRequest("enable cycle"));
}

// ---- asserts -----------------------------------------------------------------------------------

struct AssertSlot(UnsafeCell<sys::ARAAssertFunction>);
// SAFETY: written only by the plug-in side through the address it is given, read by its asserts.
unsafe impl Sync for AssertSlot {}

static ASSERT_SLOT: AssertSlot = AssertSlot(UnsafeCell::new(Some(record_assert)));
static ASSERTS: Mutex<Vec<(i32, String)>> = Mutex::new(Vec::new());
static ONE_HOST: Mutex<()> = Mutex::new(());

unsafe extern "C" fn record_assert(
    category: sys::ARAAssertCategory,
    _argument: *const c_void,
    diagnosis: *const c_char,
) {
    // SAFETY: the plug-in passes a NUL-terminated diagnosis.
    let text = unsafe { opt_string(diagnosis) }.unwrap_or_default();
    ASSERTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((category, text));
}

// ---- the host ----------------------------------------------------------------------------------

/// The factory's fields, as a host reads them: the host's half's (`hosting`).
pub use crate::hosting::FactoryInfo;

/// The test host. Its `HostState` lives in a box whose address is every controller host ref.
pub struct TestHost {
    factory: *const sys::ARAFactory,
    state: Box<HostState>,
    initialized: std::cell::Cell<bool>,
    _one: MutexGuard<'static, ()>,
}

impl Drop for TestHost {
    /// Uninitializes ARA if this host initialized it (after its documents, which borrow it).
    fn drop(&mut self) {
        if self.initialized.get() {
            self.uninitialize();
        }
    }
}

impl TestHost {
    /// A host of controller `C`'s factory, in this process.
    pub fn of<C: crate::Controller>(config: HostConfig) -> Self {
        // SAFETY: the factory lives as long as the library.
        unsafe { Self::new(crate::factory::<C>(), config) }
    }

    /// A host of `factory`.
    ///
    /// # Safety
    /// `factory` must be a plug-in's `const ARAFactory*`, valid while the host lives (while its
    /// library stays loaded).
    pub unsafe fn new(factory: *const sys::ARAFactory, config: HostConfig) -> Self {
        let one = ONE_HOST.lock().unwrap_or_else(PoisonError::into_inner);
        ASSERTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        // SAFETY: the plug-in's factory, valid while its library is loaded.
        let archive_id = config.document_archive_id.clone().map_or_else(
            || unsafe { CStr::from_ptr((*factory).documentArchiveID) }.to_owned(),
            |s| crate::util::c_string(&s),
        );
        let mut updates_size = config.model_update_struct_size;
        updates_size = updates_size.min(implemented_size!(
            sys::ARAModelUpdateControllerInterface,
            notifyDocumentDataChanged
        ));
        let state = Box::new(HostState {
            main_thread: std::thread::current().id(),
            archive_id,
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
                structSize: config.archiving_struct_size,
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
            // Every slot is filled, whatever the size says: a plug-in that reads past the size
            // calls a function the host didn't declare, which the tests see.
            updates: sys::ARAModelUpdateControllerInterface {
                structSize: updates_size,
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
            data: Mutex::new(HostData::default()),
            config,
        });
        let me = std::ptr::from_ref(&*state).cast_mut().cast::<c_void>();
        let c = &state.config;
        // SAFETY: not yet shared.
        unsafe {
            *state.instance.get() = sys::ARADocumentControllerHostInstance {
                structSize: implemented_size!(
                    sys::ARADocumentControllerHostInstance,
                    playbackControllerInterface
                ),
                audioAccessControllerHostRef: me.cast(),
                audioAccessControllerInterface: &raw const state.audio,
                archivingControllerHostRef: me.cast(),
                archivingControllerInterface: &raw const state.archiving,
                contentAccessControllerHostRef: me.cast(),
                contentAccessControllerInterface: if c.content_access {
                    &raw const state.content
                } else {
                    std::ptr::null()
                },
                modelUpdateControllerHostRef: me.cast(),
                modelUpdateControllerInterface: if c.model_updates {
                    &raw const state.updates
                } else {
                    std::ptr::null()
                },
                playbackControllerHostRef: me.cast(),
                playbackControllerInterface: if c.playback_controller {
                    &raw const state.playback
                } else {
                    std::ptr::null()
                },
            };
        }
        TestHost {
            factory,
            state,
            initialized: std::cell::Cell::new(false),
            _one: one,
        }
    }

    pub fn factory_info(&self) -> FactoryInfo {
        // SAFETY: the plug-in's factory, valid while the host lives (`new`'s contract).
        unsafe { crate::hosting::read_info(self.factory) }
    }

    /// `initializeARAWithConfiguration` with the host's assert function.
    pub fn initialize(&self, api_generation: i32) {
        let config = sys::ARAInterfaceConfiguration {
            structSize: implemented_size!(sys::ARAInterfaceConfiguration, assertFunctionAddress),
            desiredApiGeneration: api_generation,
            assertFunctionAddress: ASSERT_SLOT.0.get(),
        };
        // SAFETY: the plug-in's entry point, with a valid configuration.
        unsafe {
            ((*self.factory)
                .initializeARAWithConfiguration
                .expect("initializeARAWithConfiguration"))(&config)
        };
        self.initialized.set(true);
    }

    pub fn uninitialize(&self) {
        self.initialized.set(false);
        // SAFETY: the plug-in's entry point.
        unsafe { ((*self.factory).uninitializeARA.expect("uninitializeARA"))() };
    }

    /// A new document; `None` if the plug-in refuses.
    pub fn create_document(&self, name: &str) -> Option<TestDocument<'_>> {
        let name = crate::util::c_string(name);
        let properties = sys::ARADocumentProperties {
            structSize: implemented_size!(sys::ARADocumentProperties, name),
            name: name.as_ptr(),
        };
        // SAFETY: the plug-in's entry point with valid host structs that outlive the controller.
        unsafe {
            let create = (*self.factory).createDocumentControllerWithDocument?;
            let instance = create(self.state.instance.get(), &properties);
            if instance.is_null() {
                return None;
            }
            let i = read_sized((*instance).documentControllerInterface);
            Some(TestDocument {
                host: self,
                dc: (*instance).documentControllerRef,
                i,
                destroyed: false,
            })
        }
    }

    /// The model update notifications and progress reports so far (taken).
    pub fn take_events(&self) -> Vec<HostEvent> {
        std::mem::take(&mut self.state.data().events)
    }

    /// ARA rules the plug-in broke, as the host saw them.
    pub fn violations(&self) -> Vec<String> {
        self.state.data().violations.clone()
    }

    /// Asserts the plug-in reported through the host's assert function (category, diagnosis).
    pub fn asserts(&self) -> Vec<(i32, String)> {
        ASSERTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn source_stats(&self, source: &Source) -> SourceStats {
        self.state
            .data()
            .sources
            .get(&source.host_ref)
            .map(|s| s.stats.clone())
            .unwrap_or_default()
    }

    /// Changes a musical context's content as the host keeps it (then tell the plug-in with
    /// [`TestDocument::update_musical_context_content`]).
    pub fn set_context_content(&self, context: &Context, content: ContextContent) {
        self.state.data().contexts.insert(context.host_ref, content);
    }
}

// ---- the document ------------------------------------------------------------------------------

/// A musical context: the plug-in's ref and the host's.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub plugin_ref: sys::ARAMusicalContextRef,
    pub host_ref: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Sequence {
    pub plugin_ref: sys::ARARegionSequenceRef,
    pub host_ref: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub plugin_ref: sys::ARAAudioSourceRef,
    pub host_ref: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Modification {
    pub plugin_ref: sys::ARAAudioModificationRef,
    pub host_ref: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub plugin_ref: sys::ARAPlaybackRegionRef,
    pub host_ref: usize,
}

// SAFETY: opaque refs, only ever passed back to the plug-in that made them.
unsafe impl Send for Context {}
unsafe impl Sync for Context {}
unsafe impl Send for Sequence {}
unsafe impl Sync for Sequence {}
unsafe impl Send for Source {}
unsafe impl Sync for Source {}
unsafe impl Send for Modification {}
unsafe impl Sync for Modification {}
unsafe impl Send for Region {}
unsafe impl Sync for Region {}

/// `getPlaybackRegionHeadAndTailTime`, which ARA lets a host call from any thread (its audio
/// thread too), apart from the document.
#[derive(Clone, Copy)]
pub struct HeadTailQuery {
    f: unsafe extern "C" fn(
        sys::ARADocumentControllerRef,
        sys::ARAPlaybackRegionRef,
        *mut f64,
        *mut f64,
    ),
    dc: usize,
}

impl HeadTailQuery {
    pub fn query(&self, region: &Region) -> (f64, f64) {
        let (mut head, mut tail) = (-1.0, -1.0);
        // SAFETY: the plug-in's entry point, any thread, a live region.
        unsafe { (self.f)(self.dc as _, region.plugin_ref, &mut head, &mut tail) };
        (head, tail)
    }
}

/// A recording for the plug-in.
#[derive(Clone, Debug)]
pub struct SourceSpec {
    pub name: String,
    pub persistent_id: String,
    pub sample_rate: f64,
    /// One buffer per channel.
    pub channels: Vec<Vec<f32>>,
}

/// Where a region plays.
#[derive(Clone, Debug, Default)]
pub struct RegionSpec {
    pub start_in_modification: f64,
    pub duration_in_modification: f64,
    pub start_in_playback: f64,
    pub duration_in_playback: f64,
    pub name: Option<String>,
}

/// A store filter (`ARAStoreObjectsFilter`).
#[derive(Clone, Debug, Default)]
pub struct StoreFilter {
    pub document_data: bool,
    pub sources: Vec<Source>,
    pub modifications: Vec<Modification>,
}

/// A restore filter (`ARARestoreObjectsFilter`): archived persistent IDs, each with the current
/// ID it maps to (`None`: the same).
#[derive(Clone, Debug, Default)]
pub struct RestoreFilter {
    pub document_data: bool,
    pub sources: Vec<(String, Option<String>)>,
    pub modifications: Vec<(String, Option<String>)>,
}

/// A document controller the host made, driven through its interface table.
pub struct TestDocument<'h> {
    host: &'h TestHost,
    dc: sys::ARADocumentControllerRef,
    i: sys::ARADocumentControllerInterface,
    destroyed: bool,
}

macro_rules! dc_call {
    ($self:ident . $f:ident ( $($arg:expr),* )) => {{
        let f = $self.i.$f.expect(stringify!($f));
        // SAFETY: the plug-in's interface, called on the model thread with valid arguments.
        unsafe { f($self.dc $(, $arg)*) }
    }};
}

fn cs(s: &str) -> CString {
    crate::util::c_string(s)
}

impl<'h> TestDocument<'h> {
    /// The `ARADocumentControllerRef`, for binding plug-in instances.
    pub fn controller_ref(&self) -> *const c_void {
        self.dc.cast_const().cast()
    }

    pub fn host(&self) -> &'h TestHost {
        self.host
    }

    pub fn get_factory(&self) -> *const sys::ARAFactory {
        dc_call!(self.getFactory())
    }

    pub fn begin_editing(&self) {
        dc_call!(self.beginEditing())
    }

    pub fn end_editing(&self) {
        dc_call!(self.endEditing())
    }

    /// Runs `f` inside `beginEditing`/`endEditing`.
    pub fn edit<R>(&self, f: impl FnOnce(&Self) -> R) -> R {
        self.begin_editing();
        let r = f(self);
        self.end_editing();
        r
    }

    pub fn notify_model_updates(&self) {
        self.host.state.data().in_notify = true;
        dc_call!(self.notifyModelUpdates());
        self.host.state.data().in_notify = false;
    }

    pub fn update_document_name(&self, name: &str) {
        let name = cs(name);
        let p = sys::ARADocumentProperties {
            structSize: implemented_size!(sys::ARADocumentProperties, name),
            name: name.as_ptr(),
        };
        dc_call!(self.updateDocumentProperties(&p))
    }

    pub fn create_musical_context(&self, name: &str, content: ContextContent) -> Context {
        let host_ref = {
            let mut d = self.host.state.data();
            let r = d.new_ref();
            d.contexts.insert(r, content);
            r
        };
        let name = cs(name);
        let p = sys::ARAMusicalContextProperties {
            structSize: implemented_size!(sys::ARAMusicalContextProperties, color),
            name: name.as_ptr(),
            orderIndex: 0,
            color: std::ptr::null(),
        };
        let plugin_ref = dc_call!(self.createMusicalContext(host_ref as _, &p));
        Context {
            plugin_ref,
            host_ref,
        }
    }

    pub fn update_musical_context_content(&self, context: &Context) {
        dc_call!(self.updateMusicalContextContent(
            context.plugin_ref,
            std::ptr::null(),
            sys::kARAContentUpdateEverythingChanged
        ))
    }

    pub fn destroy_musical_context(&self, context: &Context) {
        dc_call!(self.destroyMusicalContext(context.plugin_ref))
    }

    pub fn create_region_sequence(
        &self,
        name: &str,
        order_index: i32,
        context: &Context,
    ) -> Sequence {
        let host_ref = self.host.state.data().new_ref();
        let name = cs(name);
        let p = sys::ARARegionSequenceProperties {
            structSize: implemented_size!(sys::ARARegionSequenceProperties, color),
            name: name.as_ptr(),
            orderIndex: order_index,
            musicalContextRef: context.plugin_ref,
            color: std::ptr::null(),
        };
        let plugin_ref = dc_call!(self.createRegionSequence(host_ref as _, &p));
        Sequence {
            plugin_ref,
            host_ref,
        }
    }

    pub fn update_region_sequence(
        &self,
        sequence: &Sequence,
        name: &str,
        order_index: i32,
        context: &Context,
    ) {
        let name = cs(name);
        let p = sys::ARARegionSequenceProperties {
            structSize: implemented_size!(sys::ARARegionSequenceProperties, color),
            name: name.as_ptr(),
            orderIndex: order_index,
            musicalContextRef: context.plugin_ref,
            color: std::ptr::null(),
        };
        dc_call!(self.updateRegionSequenceProperties(sequence.plugin_ref, &p))
    }

    pub fn destroy_region_sequence(&self, sequence: &Sequence) {
        dc_call!(self.destroyRegionSequence(sequence.plugin_ref))
    }

    fn source_properties(
        spec: &SourceSpec,
        name: &CString,
        id: &CString,
    ) -> sys::ARAAudioSourceProperties {
        sys::ARAAudioSourceProperties {
            structSize: implemented_size!(sys::ARAAudioSourceProperties, channelArrangement),
            name: name.as_ptr(),
            persistentID: id.as_ptr(),
            sampleCount: spec.channels.first().map_or(0, |c| c.len() as i64),
            sampleRate: spec.sample_rate,
            channelCount: spec.channels.len() as i32,
            merits64BitSamples: sys::kARAFalse,
            channelArrangementDataType: sys::kARAChannelArrangementUndefined,
            channelArrangement: std::ptr::null(),
        }
    }

    pub fn create_audio_source(&self, spec: &SourceSpec) -> Source {
        let host_ref = {
            let mut d = self.host.state.data();
            let r = d.new_ref();
            d.sources.insert(
                r,
                HostSource {
                    channels: Arc::new(spec.channels.clone()),
                    access: Access::Disabled,
                    stats: SourceStats::default(),
                },
            );
            r
        };
        let (name, id) = (cs(&spec.name), cs(&spec.persistent_id));
        let p = Self::source_properties(spec, &name, &id);
        let plugin_ref = dc_call!(self.createAudioSource(host_ref as _, &p));
        Source {
            plugin_ref,
            host_ref,
        }
    }

    /// New properties and samples (only while sample access is disabled, as ARA requires).
    pub fn update_audio_source(&self, source: &Source, spec: &SourceSpec) {
        if let Some(s) = self.host.state.data().sources.get_mut(&source.host_ref) {
            s.channels = Arc::new(spec.channels.clone());
        }
        let (name, id) = (cs(&spec.name), cs(&spec.persistent_id));
        let p = Self::source_properties(spec, &name, &id);
        dc_call!(self.updateAudioSourceProperties(source.plugin_ref, &p))
    }

    pub fn update_audio_source_content(&self, source: &Source, flags: i32) {
        dc_call!(self.updateAudioSourceContent(source.plugin_ref, std::ptr::null(), flags))
    }

    /// `enableAudioSourceSamplesAccess`, checking that a disable returns with no read under way
    /// and no reader left (recorded as violations otherwise).
    pub fn enable_samples_access(&self, source: &Source, enable: bool) {
        if enable && let Some(s) = self.host.state.data().sources.get_mut(&source.host_ref) {
            s.access = Access::Enabled;
        }
        dc_call!(
            self.enableAudioSourceSamplesAccess(source.plugin_ref, crate::util::ara_bool(enable))
        );
        if !enable {
            let mut d = self.host.state.data();
            let Some(s) = d.sources.get_mut(&source.host_ref) else {
                return;
            };
            s.access = Access::Disabled;
            let (in_flight, live) = (s.stats.reads_in_flight, s.stats.live_readers);
            if in_flight > 0 {
                d.violations
                    .push("a read was still under way when sample access was disabled".into());
            }
            if live > 0 {
                d.violations
                    .push("readers were left when sample access was disabled".into());
            }
        }
    }

    pub fn deactivate_audio_source(&self, source: &Source, deactivate: bool) {
        dc_call!(self.deactivateAudioSourceForUndoHistory(
            source.plugin_ref,
            crate::util::ara_bool(deactivate)
        ))
    }

    pub fn destroy_audio_source(&self, source: &Source) {
        dc_call!(self.destroyAudioSource(source.plugin_ref));
        let mut d = self.host.state.data();
        if let Some(s) = d.sources.get_mut(&source.host_ref) {
            s.access = Access::Disabled;
            if s.stats.live_readers > 0 || s.stats.reads_in_flight > 0 {
                d.violations
                    .push("readers outlived their destroyed audio source".into());
            }
        }
    }

    pub fn create_audio_modification(
        &self,
        source: &Source,
        persistent_id: &str,
        name: &str,
    ) -> Modification {
        let host_ref = self.host.state.data().new_ref();
        let (name, id) = (cs(name), cs(persistent_id));
        let p = sys::ARAAudioModificationProperties {
            structSize: implemented_size!(sys::ARAAudioModificationProperties, persistentID),
            name: name.as_ptr(),
            persistentID: id.as_ptr(),
        };
        let plugin_ref =
            dc_call!(self.createAudioModification(source.plugin_ref, host_ref as _, &p));
        Modification {
            plugin_ref,
            host_ref,
        }
    }

    pub fn clone_audio_modification(
        &self,
        original: &Modification,
        persistent_id: &str,
    ) -> Modification {
        let host_ref = self.host.state.data().new_ref();
        let id = cs(persistent_id);
        let p = sys::ARAAudioModificationProperties {
            structSize: implemented_size!(sys::ARAAudioModificationProperties, persistentID),
            name: std::ptr::null(),
            persistentID: id.as_ptr(),
        };
        let plugin_ref =
            dc_call!(self.cloneAudioModification(original.plugin_ref, host_ref as _, &p));
        Modification {
            plugin_ref,
            host_ref,
        }
    }

    pub fn deactivate_audio_modification(&self, modification: &Modification, deactivate: bool) {
        dc_call!(self.deactivateAudioModificationForUndoHistory(
            modification.plugin_ref,
            crate::util::ara_bool(deactivate)
        ))
    }

    pub fn destroy_audio_modification(&self, modification: &Modification) {
        dc_call!(self.destroyAudioModification(modification.plugin_ref))
    }

    /// The properties a region spec makes, at ARA 2.3's full size.
    pub fn region_properties(
        spec: &RegionSpec,
        sequence: &Sequence,
        name: Option<&CString>,
    ) -> sys::ARAPlaybackRegionProperties {
        sys::ARAPlaybackRegionProperties {
            structSize: implemented_size!(sys::ARAPlaybackRegionProperties, color),
            transformationFlags: sys::kARAPlaybackTransformationNoChanges,
            startInModificationTime: spec.start_in_modification,
            durationInModificationTime: spec.duration_in_modification,
            startInPlaybackTime: spec.start_in_playback,
            durationInPlaybackTime: spec.duration_in_playback,
            musicalContextRef: std::ptr::null_mut(),
            regionSequenceRef: sequence.plugin_ref,
            name: name.map_or(std::ptr::null(), |n| n.as_ptr()),
            color: std::ptr::null(),
        }
    }

    pub fn create_playback_region(
        &self,
        modification: &Modification,
        sequence: &Sequence,
        spec: &RegionSpec,
    ) -> Region {
        let name = spec.name.as_deref().map(cs);
        let p = Self::region_properties(spec, sequence, name.as_ref());
        self.create_playback_region_with(modification, &p)
    }

    /// With properties as given (an older host's smaller struct, say).
    pub fn create_playback_region_with(
        &self,
        modification: &Modification,
        properties: &sys::ARAPlaybackRegionProperties,
    ) -> Region {
        let host_ref = self.host.state.data().new_ref();
        let plugin_ref =
            dc_call!(self.createPlaybackRegion(modification.plugin_ref, host_ref as _, properties));
        Region {
            plugin_ref,
            host_ref,
        }
    }

    pub fn update_playback_region(&self, region: &Region, sequence: &Sequence, spec: &RegionSpec) {
        let name = spec.name.as_deref().map(cs);
        let p = Self::region_properties(spec, sequence, name.as_ref());
        dc_call!(self.updatePlaybackRegionProperties(region.plugin_ref, &p))
    }

    pub fn destroy_playback_region(&self, region: &Region) {
        dc_call!(self.destroyPlaybackRegion(region.plugin_ref))
    }

    pub fn head_and_tail_query(&self) -> HeadTailQuery {
        HeadTailQuery {
            f: self
                .i
                .getPlaybackRegionHeadAndTailTime
                .expect("getPlaybackRegionHeadAndTailTime"),
            dc: self.dc as usize,
        }
    }

    pub fn head_and_tail_time(&self, region: &Region) -> (f64, f64) {
        let (mut head, mut tail) = (-1.0, -1.0);
        dc_call!(self.getPlaybackRegionHeadAndTailTime(region.plugin_ref, &mut head, &mut tail));
        (head, tail)
    }

    /// `storeObjectsToArchive`: the archive, or `None` if the plug-in reports failure.
    pub fn store(&self, filter: Option<&StoreFilter>) -> Option<Vec<u8>> {
        self.host.state.data().archive_out.clear();
        let sources: Vec<_> = filter
            .map(|f| f.sources.iter().map(|s| s.plugin_ref).collect())
            .unwrap_or_default();
        let modifications: Vec<_> = filter
            .map(|f| f.modifications.iter().map(|m| m.plugin_ref).collect())
            .unwrap_or_default();
        let raw = filter.map(|f| sys::ARAStoreObjectsFilter {
            structSize: implemented_size!(sys::ARAStoreObjectsFilter, audioModificationRefs),
            documentData: crate::util::ara_bool(f.document_data),
            audioSourceRefsCount: sources.len(),
            audioSourceRefs: if sources.is_empty() {
                std::ptr::null()
            } else {
                sources.as_ptr()
            },
            audioModificationRefsCount: modifications.len(),
            audioModificationRefs: if modifications.is_empty() {
                std::ptr::null()
            } else {
                modifications.as_ptr()
            },
        });
        let ok = dc_call!(self.storeObjectsToArchive(
            1usize as _,
            raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref)
        ));
        let bytes = std::mem::take(&mut self.host.state.data().archive_out);
        (ok != sys::kARAFalse).then_some(bytes)
    }

    /// `restoreObjectsFromArchive` (call it inside an edit); the plug-in's result.
    pub fn restore(&self, archive: &[u8], filter: Option<&RestoreFilter>) -> bool {
        self.host.state.data().archive_in = archive.to_vec();
        let ids = |list: &[(String, Option<String>)]| -> (Vec<CString>, Option<Vec<CString>>) {
            let archived = list.iter().map(|(a, _)| cs(a)).collect();
            let mapped = list.iter().any(|(_, c)| c.is_some());
            let current = mapped.then(|| {
                list.iter()
                    .map(|(a, c)| cs(c.as_deref().unwrap_or(a)))
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
            audioSourceArchiveIDs: if sap.is_empty() {
                std::ptr::null()
            } else {
                sap.as_ptr()
            },
            audioSourceCurrentIDs: scp.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
            audioModificationIDsCount: map_.len(),
            audioModificationArchiveIDs: if map_.is_empty() {
                std::ptr::null()
            } else {
                map_.as_ptr()
            },
            audioModificationCurrentIDs: mcp.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
        });
        let ok = dc_call!(self.restoreObjectsFromArchive(
            1usize as _,
            raw.as_ref().map_or(std::ptr::null(), std::ptr::from_ref)
        ));
        ok != sys::kARAFalse
    }

    /// Reads content the plug-in offers for a source, as a host would.
    pub fn read_audio_source_content<T: HostContent>(&self, source: &Source) -> Option<Vec<T>> {
        let ty = T::TYPE.raw();
        if dc_call!(self.isAudioSourceContentAvailable(source.plugin_ref, ty)) == sys::kARAFalse {
            return None;
        }
        let reader =
            dc_call!(self.createAudioSourceContentReader(source.plugin_ref, ty, std::ptr::null()));
        self.drain_reader(reader)
    }

    pub fn read_playback_region_content<T: HostContent>(&self, region: &Region) -> Option<Vec<T>> {
        let ty = T::TYPE.raw();
        if dc_call!(self.isPlaybackRegionContentAvailable(region.plugin_ref, ty)) == sys::kARAFalse
        {
            return None;
        }
        let reader = dc_call!(self.createPlaybackRegionContentReader(
            region.plugin_ref,
            ty,
            std::ptr::null()
        ));
        self.drain_reader(reader)
    }

    fn drain_reader<T: HostContent>(&self, reader: sys::ARAContentReaderRef) -> Option<Vec<T>> {
        if reader.is_null() {
            return None;
        }
        let n = dc_call!(self.getContentReaderEventCount(reader));
        let events = (0..n)
            .map(|i| {
                let data = dc_call!(self.getContentReaderDataForEvent(reader, i));
                // SAFETY: the plug-in's event of type T, valid until the next call.
                unsafe { T::copy_from(data) }
            })
            .collect();
        dc_call!(self.destroyContentReader(reader));
        Some(events)
    }

    /// `destroyDocumentController`.
    pub fn destroy(mut self) {
        self.destroy_now();
    }

    fn destroy_now(&mut self) {
        if !self.destroyed {
            self.destroyed = true;
            dc_call!(self.destroyDocumentController());
        }
    }
}

impl Drop for TestDocument<'_> {
    fn drop(&mut self) {
        self.destroy_now();
    }
}

/// A bound plug-in instance as the host sees it: its `ARAPlugInExtensionInstance`.
pub struct TestExtension {
    instance: *const sys::ARAPlugInExtensionInstance,
}

impl TestExtension {
    /// # Safety
    /// `instance` must be a plug-in's `const ARAPlugInExtensionInstance*`, used only while that
    /// instance lives.
    pub unsafe fn new(instance: *const c_void) -> Self {
        TestExtension {
            instance: instance.cast(),
        }
    }

    fn get(&self) -> sys::ARAPlugInExtensionInstance {
        // SAFETY: the plug-in's instance struct (the constructor's contract).
        unsafe { read_sized(self.instance) }
    }

    pub fn struct_size(&self) -> usize {
        self.get().structSize
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

    pub fn add_playback_region(&self, region: &Region) {
        let e = self.get();
        // SAFETY: the plug-in's renderer interface (checked present by `expect`).
        unsafe {
            let i = read_sized(e.playbackRendererInterface);
            (i.addPlaybackRegion.expect("addPlaybackRegion"))(
                e.playbackRendererRef,
                region.plugin_ref,
            )
        }
    }

    pub fn remove_playback_region(&self, region: &Region) {
        let e = self.get();
        // SAFETY: as above.
        unsafe {
            let i = read_sized(e.playbackRendererInterface);
            (i.removePlaybackRegion.expect("removePlaybackRegion"))(
                e.playbackRendererRef,
                region.plugin_ref,
            )
        }
    }

    pub fn add_editor_region(&self, region: &Region) {
        let e = self.get();
        // SAFETY: as above.
        unsafe {
            let i = read_sized(e.editorRendererInterface);
            (i.addPlaybackRegion.expect("addPlaybackRegion"))(
                e.editorRendererRef,
                region.plugin_ref,
            )
        }
    }

    pub fn add_editor_sequence(&self, sequence: &Sequence) {
        let e = self.get();
        // SAFETY: as above.
        unsafe {
            let i = read_sized(e.editorRendererInterface);
            (i.addRegionSequence.expect("addRegionSequence"))(
                e.editorRendererRef,
                sequence.plugin_ref,
            )
        }
    }

    pub fn notify_selection(
        &self,
        regions: &[Region],
        sequences: &[Sequence],
        time_range: Option<(f64, f64)>,
    ) {
        let e = self.get();
        let r: Vec<_> = regions.iter().map(|r| r.plugin_ref).collect();
        let s: Vec<_> = sequences.iter().map(|s| s.plugin_ref).collect();
        let range =
            time_range.map(|(start, duration)| sys::ARAContentTimeRange { start, duration });
        let selection = sys::ARAViewSelection {
            structSize: implemented_size!(sys::ARAViewSelection, timeRange),
            playbackRegionRefsCount: r.len(),
            playbackRegionRefs: if r.is_empty() {
                std::ptr::null()
            } else {
                r.as_ptr()
            },
            regionSequenceRefsCount: s.len(),
            regionSequenceRefs: if s.is_empty() {
                std::ptr::null()
            } else {
                s.as_ptr()
            },
            timeRange: range.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
        };
        // SAFETY: as above.
        unsafe {
            let i = read_sized(e.editorViewInterface);
            (i.notifySelection.expect("notifySelection"))(e.editorViewRef, &selection)
        }
    }

    pub fn hide_region_sequences(&self, sequences: &[Sequence]) {
        let e = self.get();
        let s: Vec<_> = sequences.iter().map(|s| s.plugin_ref).collect();
        // SAFETY: as above.
        unsafe {
            let i = read_sized(e.editorViewInterface);
            (i.notifyHideRegionSequences
                .expect("notifyHideRegionSequences"))(
                e.editorViewRef,
                s.len(),
                if s.is_empty() {
                    std::ptr::null()
                } else {
                    s.as_ptr()
                },
            )
        }
    }
}
