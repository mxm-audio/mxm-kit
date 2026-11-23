//! The host's interfaces, wrapped safely: its function pointers are copied once, at the document
//! controller's creation, as far as each struct's `structSize` says the host implements them.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::content::{ContentGrade, HostContent, HostContentEvents, TimeRange};
use crate::graph::AudioSourceId;
use crate::sys;
use crate::util::{implemented_size, opt_string, read_field, struct_size};
use crate::validate::check;

/// `ARAAudioAccessControllerInterface`, required.
#[derive(Clone, Copy)]
pub(crate) struct AudioAccess {
    host_ref: sys::ARAAudioAccessControllerHostRef,
    create: unsafe extern "C" fn(
        sys::ARAAudioAccessControllerHostRef,
        sys::ARAAudioSourceHostRef,
        sys::ARABool,
    ) -> sys::ARAAudioReaderHostRef,
    read: unsafe extern "C" fn(
        sys::ARAAudioAccessControllerHostRef,
        sys::ARAAudioReaderHostRef,
        sys::ARASamplePosition,
        sys::ARASampleCount,
        *const *mut c_void,
    ) -> sys::ARABool,
    destroy: unsafe extern "C" fn(sys::ARAAudioAccessControllerHostRef, sys::ARAAudioReaderHostRef),
}

/// `ARAArchivingControllerInterface`, required.
#[derive(Clone, Copy)]
pub(crate) struct Archiving {
    pub(crate) host_ref: sys::ARAArchivingControllerHostRef,
    size: unsafe extern "C" fn(
        sys::ARAArchivingControllerHostRef,
        sys::ARAArchiveReaderHostRef,
    ) -> sys::ARASize,
    read: unsafe extern "C" fn(
        sys::ARAArchivingControllerHostRef,
        sys::ARAArchiveReaderHostRef,
        sys::ARASize,
        sys::ARASize,
        *mut sys::ARAByte,
    ) -> sys::ARABool,
    write: unsafe extern "C" fn(
        sys::ARAArchivingControllerHostRef,
        sys::ARAArchiveWriterHostRef,
        sys::ARASize,
        sys::ARASize,
        *const sys::ARAByte,
    ) -> sys::ARABool,
    archiving_progress: unsafe extern "C" fn(sys::ARAArchivingControllerHostRef, f32),
    unarchiving_progress: unsafe extern "C" fn(sys::ARAArchivingControllerHostRef, f32),
    /// ARA 2.0 final: absent from older hosts.
    archive_id: Option<
        unsafe extern "C" fn(
            sys::ARAArchivingControllerHostRef,
            sys::ARAArchiveReaderHostRef,
        ) -> sys::ARAPersistentID,
    >,
}

/// `ARAContentAccessControllerInterface`, optional.
#[derive(Clone, Copy)]
pub(crate) struct ContentAccess {
    host_ref: sys::ARAContentAccessControllerHostRef,
    i: sys::ARAContentAccessControllerInterface,
}

/// `ARAModelUpdateControllerInterface`, optional.
#[derive(Clone, Copy)]
pub(crate) struct ModelUpdates {
    pub(crate) host_ref: sys::ARAModelUpdateControllerHostRef,
    pub(crate) analysis_progress: unsafe extern "C" fn(
        sys::ARAModelUpdateControllerHostRef,
        sys::ARAAudioSourceHostRef,
        sys::ARAAnalysisProgressState,
        f32,
    ),
    pub(crate) source_content: unsafe extern "C" fn(
        sys::ARAModelUpdateControllerHostRef,
        sys::ARAAudioSourceHostRef,
        *const sys::ARAContentTimeRange,
        sys::ARAContentUpdateFlags,
    ),
    pub(crate) modification_content: unsafe extern "C" fn(
        sys::ARAModelUpdateControllerHostRef,
        sys::ARAAudioModificationHostRef,
        *const sys::ARAContentTimeRange,
        sys::ARAContentUpdateFlags,
    ),
    /// ARA 2.0: absent from older hosts.
    pub(crate) region_content: Option<
        unsafe extern "C" fn(
            sys::ARAModelUpdateControllerHostRef,
            sys::ARAPlaybackRegionHostRef,
            *const sys::ARAContentTimeRange,
            sys::ARAContentUpdateFlags,
        ),
    >,
    /// ARA 2.3: absent from older hosts.
    pub(crate) document_data: Option<unsafe extern "C" fn(sys::ARAModelUpdateControllerHostRef)>,
}

/// `ARAPlaybackControllerInterface`, optional.
#[derive(Clone, Copy)]
pub(crate) struct Playback {
    host_ref: sys::ARAPlaybackControllerHostRef,
    i: sys::ARAPlaybackControllerInterface,
}

/// Every host interface a document controller was given.
pub(crate) struct HostInterfaces {
    pub(crate) audio: AudioAccess,
    pub(crate) archiving: Archiving,
    pub(crate) content: Option<ContentAccess>,
    pub(crate) updates: Option<ModelUpdates>,
    pub(crate) playback: Option<Playback>,
}

// SAFETY: the host refs and function pointers are the host's, which ARA requires to stay valid
// until the document controller is destroyed and to be callable from the threads each interface
// documents (the audio reader from worker threads, one per reader). The document controller and
// its readers call them only within those rules.
unsafe impl Send for AudioAccess {}
unsafe impl Sync for AudioAccess {}
unsafe impl Send for HostInterfaces {}
unsafe impl Sync for HostInterfaces {}

/// Checks an interface struct: non-null, at least its minimum size. Reports and returns false
/// otherwise.
unsafe fn usable<S>(p: *const S, min: usize) -> bool {
    // SAFETY: non-null checked first; a non-null interface has at least its `structSize`.
    check!(
        !p.is_null() && unsafe { struct_size(p) } >= min,
        Argument,
        p,
        c"host interface missing or too small"
    )
}

impl HostInterfaces {
    /// Copies the host's interfaces; `None` (reported) if a required one is missing.
    ///
    /// # Safety
    /// `host` must be null or point to a host instance valid for the call, whose interfaces stay
    /// valid until the document controller is destroyed.
    pub(crate) unsafe fn from_host(
        host: *const sys::ARADocumentControllerHostInstance,
    ) -> Option<Self> {
        type H = sys::ARADocumentControllerHostInstance;
        // SAFETY (whole body): every struct is checked non-null and at least its minimum size
        // before a field is read, and `read_field!` reads nothing past a struct's `structSize`.
        unsafe {
            if !usable(host, implemented_size!(H, playbackControllerInterface)) {
                return None;
            }
            let audio_i = read_field!(host, H, audioAccessControllerInterface)?;
            let archiving_i = read_field!(host, H, archivingControllerInterface)?;
            if !usable(
                audio_i,
                implemented_size!(sys::ARAAudioAccessControllerInterface, destroyAudioReader),
            ) || !usable(
                archiving_i,
                implemented_size!(
                    sys::ARAArchivingControllerInterface,
                    notifyDocumentUnarchivingProgress
                ),
            ) {
                return None;
            }
            type A = sys::ARAAudioAccessControllerInterface;
            let audio = AudioAccess {
                host_ref: read_field!(host, H, audioAccessControllerHostRef)?,
                create: read_field!(audio_i, A, createAudioReaderForSource)??,
                read: read_field!(audio_i, A, readAudioSamples)??,
                destroy: read_field!(audio_i, A, destroyAudioReader)??,
            };
            type R = sys::ARAArchivingControllerInterface;
            let archiving = Archiving {
                host_ref: read_field!(host, H, archivingControllerHostRef)?,
                size: read_field!(archiving_i, R, getArchiveSize)??,
                read: read_field!(archiving_i, R, readBytesFromArchive)??,
                write: read_field!(archiving_i, R, writeBytesToArchive)??,
                archiving_progress: read_field!(archiving_i, R, notifyDocumentArchivingProgress)??,
                unarchiving_progress: read_field!(
                    archiving_i,
                    R,
                    notifyDocumentUnarchivingProgress
                )??,
                archive_id: read_field!(archiving_i, R, getDocumentArchiveID).flatten(),
            };

            let content_i =
                read_field!(host, H, contentAccessControllerInterface).unwrap_or(std::ptr::null());
            let content = if content_i.is_null() {
                None
            } else if usable(
                content_i,
                implemented_size!(
                    sys::ARAContentAccessControllerInterface,
                    destroyContentReader
                ),
            ) {
                // The minimum size is the whole struct, so it can be copied whole.
                let i = content_i.read_unaligned();
                let complete = { i.isMusicalContextContentAvailable }.is_some()
                    && { i.getMusicalContextContentGrade }.is_some()
                    && { i.createMusicalContextContentReader }.is_some()
                    && { i.isAudioSourceContentAvailable }.is_some()
                    && { i.getAudioSourceContentGrade }.is_some()
                    && { i.createAudioSourceContentReader }.is_some()
                    && { i.getContentReaderEventCount }.is_some()
                    && { i.getContentReaderDataForEvent }.is_some()
                    && { i.destroyContentReader }.is_some();
                complete.then(|| ContentAccess {
                    host_ref: read_field!(host, H, contentAccessControllerHostRef)
                        .unwrap_or(std::ptr::null_mut()),
                    i,
                })
            } else {
                None
            };

            let updates_i =
                read_field!(host, H, modelUpdateControllerInterface).unwrap_or(std::ptr::null());
            type U = sys::ARAModelUpdateControllerInterface;
            let updates = if updates_i.is_null()
                || !usable(
                    updates_i,
                    implemented_size!(U, notifyAudioModificationContentChanged),
                ) {
                None
            } else {
                (|| {
                    Some(ModelUpdates {
                        host_ref: read_field!(host, H, modelUpdateControllerHostRef)?,
                        analysis_progress: read_field!(
                            updates_i,
                            U,
                            notifyAudioSourceAnalysisProgress
                        )??,
                        source_content: read_field!(
                            updates_i,
                            U,
                            notifyAudioSourceContentChanged
                        )??,
                        modification_content: read_field!(
                            updates_i,
                            U,
                            notifyAudioModificationContentChanged
                        )??,
                        region_content: read_field!(
                            updates_i,
                            U,
                            notifyPlaybackRegionContentChanged
                        )
                        .flatten(),
                        document_data: read_field!(updates_i, U, notifyDocumentDataChanged)
                            .flatten(),
                    })
                })()
            };

            let playback_i =
                read_field!(host, H, playbackControllerInterface).unwrap_or(std::ptr::null());
            let playback = if playback_i.is_null()
                || !usable(
                    playback_i,
                    implemented_size!(sys::ARAPlaybackControllerInterface, requestEnableCycle),
                ) {
                None
            } else {
                let i = playback_i.read_unaligned();
                let complete = { i.requestStartPlayback }.is_some()
                    && { i.requestStopPlayback }.is_some()
                    && { i.requestSetPlaybackPosition }.is_some()
                    && { i.requestSetCycleRange }.is_some()
                    && { i.requestEnableCycle }.is_some();
                complete.then(|| Playback {
                    host_ref: read_field!(host, H, playbackControllerHostRef)
                        .unwrap_or(std::ptr::null_mut()),
                    i,
                })
            };

            Some(HostInterfaces {
                audio,
                archiving,
                content,
                updates,
                playback,
            })
        }
    }
}

// ---- audio readers -----------------------------------------------------------------------------

mod private {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// A sample format an audio reader reads: `f32`, or `f64` (ARA's `use64BitSamples`).
pub trait Sample: Copy + Default + Send + 'static + private::Sealed {
    const USE_64_BIT: bool;
}
impl Sample for f32 {
    const USE_64_BIT: bool = false;
}
impl Sample for f64 {
    const USE_64_BIT: bool = true;
}

/// Why a read gave no samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The document controller cancelled the reader (sample access disabled, the source changed
    /// or went): drop it, and stop.
    Cancelled,
    /// The host's I/O failed; it filled the buffers with silence and tells the user.
    Host,
    /// One buffer per channel of the source, all the same length, is required.
    Shape,
}

struct SlotState {
    /// The host's reader, 0 once destroyed.
    host_reader: usize,
    /// Its `AudioReader` was dropped; the host's reader awaits destruction on the model thread.
    abandoned: bool,
}

/// One host audio reader, shared by its `AudioReader` (on a worker) and the model thread, which
/// cancels it, waits on its lock for a read under way, and destroys it.
pub(crate) struct ReaderSlot {
    source: AudioSourceId,
    cancelled: AtomicBool,
    state: Mutex<SlotState>,
    audio: AudioAccess,
}

impl ReaderSlot {
    fn lock(&self) -> std::sync::MutexGuard<'_, SlotState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// An audio reader of one audio source (ARA's `ARAAudioReaderHostRef`): created on the model
/// thread through [`crate::Cx::create_audio_reader`], used on one worker thread at a time,
/// never on the audio thread. Reads block while the host fetches the data.
pub struct AudioReader<S: Sample> {
    slot: Arc<ReaderSlot>,
    channels: usize,
    pointers: Pointers,
    _sample: PhantomData<fn() -> S>,
}

/// Scratch space for the buffers' pointers, reused by every read.
struct Pointers(Vec<*mut c_void>);
// SAFETY: only ever holds pointers into the caller's buffers during one `read`.
unsafe impl Send for Pointers {}

impl<S: Sample> AudioReader<S> {
    pub fn source(&self) -> AudioSourceId {
        self.slot.source
    }

    /// The source's channel count when the reader was made: the buffers `read` takes.
    pub fn channel_count(&self) -> usize {
        self.channels
    }

    /// Whether the document controller has cancelled it: the next read is refused.
    pub fn is_cancelled(&self) -> bool {
        self.slot.cancelled.load(Ordering::Acquire)
    }

    /// Reads `buffers[0].len()` samples per channel from `position` (frames of the source) into
    /// one buffer per channel. Out-of-range samples are silence.
    pub fn read(&mut self, position: i64, buffers: &mut [&mut [S]]) -> Result<(), ReadError> {
        if buffers.len() != self.channels {
            return Err(ReadError::Shape);
        }
        let frames = buffers.first().map_or(0, |b| b.len());
        if buffers.iter().any(|b| b.len() != frames) {
            return Err(ReadError::Shape);
        }
        self.pointers.0.clear();
        self.pointers
            .0
            .extend(buffers.iter_mut().map(|b| b.as_mut_ptr().cast::<c_void>()));
        let state = self.slot.lock();
        if self.slot.cancelled.load(Ordering::Acquire) || state.host_reader == 0 {
            return Err(ReadError::Cancelled);
        }
        if frames == 0 {
            return Ok(());
        }
        let audio = self.slot.audio;
        // SAFETY: the host reader is live (destroyed only under this lock, after cancelling),
        // used by this thread alone (`&mut self`, and the lock), and the buffers hold `frames`
        // samples of the reader's format per channel.
        let ok = unsafe {
            (audio.read)(
                audio.host_ref,
                state.host_reader as sys::ARAAudioReaderHostRef,
                position,
                frames as i64,
                self.pointers.0.as_ptr(),
            )
        };
        drop(state);
        self.pointers.0.clear();
        if ok != sys::kARAFalse {
            Ok(())
        } else {
            Err(ReadError::Host)
        }
    }
}

impl<S: Sample> Drop for AudioReader<S> {
    fn drop(&mut self) {
        // The host's reader may only be destroyed on the model thread, in a call about its source
        // or at the end of an edit; it is reaped there.
        self.slot.lock().abandoned = true;
    }
}

/// The live readers of a document controller (model thread only).
#[derive(Default)]
pub(crate) struct Readers {
    slots: Vec<Arc<ReaderSlot>>,
}

impl Readers {
    pub(crate) fn create<S: Sample>(
        &mut self,
        audio: &AudioAccess,
        source: AudioSourceId,
        source_host_ref: usize,
        channels: usize,
    ) -> Option<AudioReader<S>> {
        // SAFETY: called on the model thread, in a call about this source or at the end of an
        // edit, while its sample access is enabled (`Cx::create_audio_reader` checked).
        let host_reader = unsafe {
            (audio.create)(
                audio.host_ref,
                source_host_ref as sys::ARAAudioSourceHostRef,
                crate::util::ara_bool(S::USE_64_BIT),
            )
        };
        if host_reader.is_null() {
            return None;
        }
        let slot = Arc::new(ReaderSlot {
            source,
            cancelled: AtomicBool::new(false),
            state: Mutex::new(SlotState {
                host_reader: host_reader as usize,
                abandoned: false,
            }),
            audio: *audio,
        });
        self.slots.push(Arc::clone(&slot));
        Some(AudioReader {
            slot,
            channels,
            pointers: Pointers(Vec::with_capacity(channels)),
            _sample: PhantomData,
        })
    }

    /// Cancels the readers of `source` (all, for `None`), joins any read under way and destroys
    /// the host's readers. Model thread, in a call about that source or the end of an edit.
    pub(crate) fn cancel(&mut self, audio: &AudioAccess, source: Option<AudioSourceId>) {
        self.slots.retain(|slot| {
            if source.is_some_and(|s| s != slot.source) {
                return true;
            }
            slot.cancelled.store(true, Ordering::Release);
            // Waits for a read under way: the join.
            let mut state = slot.lock();
            destroy_host_reader(audio, &mut state);
            false
        });
    }

    /// Destroys the host readers whose `AudioReader`s were dropped (of `source`, or all).
    pub(crate) fn reap(&mut self, audio: &AudioAccess, source: Option<AudioSourceId>) {
        self.slots.retain(|slot| {
            if source.is_some_and(|s| s != slot.source) {
                return true;
            }
            let mut state = slot.lock();
            if state.abandoned {
                destroy_host_reader(audio, &mut state);
                false
            } else {
                true
            }
        });
    }
}

fn destroy_host_reader(audio: &AudioAccess, state: &mut SlotState) {
    if state.host_reader != 0 {
        // SAFETY: a live host reader, not in use (we hold its lock), destroyed on the model
        // thread in a call where ARA allows it (the callers' contract).
        unsafe {
            (audio.destroy)(
                audio.host_ref,
                state.host_reader as sys::ARAAudioReaderHostRef,
            )
        };
        state.host_reader = 0;
    }
}

// ---- content the host gives --------------------------------------------------------------------

impl ContentAccess {
    pub(crate) fn read_musical_context<T: HostContent>(
        &self,
        context_host_ref: usize,
        range: Option<TimeRange>,
    ) -> Option<HostContentEvents<T>> {
        let i = &self.i;
        let target = context_host_ref as sys::ARAMusicalContextHostRef;
        let ty = T::TYPE.raw();
        // SAFETY: checked complete at creation; called on the model thread where ARA allows it.
        unsafe {
            if (i.isMusicalContextContentAvailable?)(self.host_ref, target, ty) == sys::kARAFalse {
                return None;
            }
            let grade = (i.getMusicalContextContentGrade?)(self.host_ref, target, ty);
            let raw_range = range.map(TimeRange::raw);
            let range_ptr = raw_range
                .as_ref()
                .map_or(std::ptr::null(), std::ptr::from_ref);
            let reader =
                (i.createMusicalContextContentReader?)(self.host_ref, target, ty, range_ptr);
            self.drain(reader, grade)
        }
    }

    pub(crate) fn read_audio_source<T: HostContent>(
        &self,
        source_host_ref: usize,
        range: Option<TimeRange>,
    ) -> Option<HostContentEvents<T>> {
        let i = &self.i;
        let target = source_host_ref as sys::ARAAudioSourceHostRef;
        let ty = T::TYPE.raw();
        // SAFETY: as above.
        unsafe {
            if (i.isAudioSourceContentAvailable?)(self.host_ref, target, ty) == sys::kARAFalse {
                return None;
            }
            let grade = (i.getAudioSourceContentGrade?)(self.host_ref, target, ty);
            let raw_range = range.map(TimeRange::raw);
            let range_ptr = raw_range
                .as_ref()
                .map_or(std::ptr::null(), std::ptr::from_ref);
            let reader = (i.createAudioSourceContentReader?)(self.host_ref, target, ty, range_ptr);
            self.drain(reader, grade)
        }
    }

    /// Copies every event out of a host content reader and destroys it.
    unsafe fn drain<T: HostContent>(
        &self,
        reader: sys::ARAContentReaderHostRef,
        grade: sys::ARAContentGrade,
    ) -> Option<HostContentEvents<T>> {
        if reader.is_null() {
            return None;
        }
        let i = &self.i;
        // SAFETY: a live reader of type T; each event's data is valid until the next call.
        unsafe {
            let count = (i.getContentReaderEventCount?)(self.host_ref, reader).max(0);
            let mut events = Vec::with_capacity(count as usize);
            for index in 0..count {
                let data = (i.getContentReaderDataForEvent?)(self.host_ref, reader, index);
                if !data.is_null() {
                    events.push(T::copy_from(data));
                }
            }
            (i.destroyContentReader?)(self.host_ref, reader);
            Some(HostContentEvents {
                grade: ContentGrade::from_raw(grade),
                events,
            })
        }
    }
}

// ---- the playback controller -------------------------------------------------------------------

/// Requests to the host's transport (`ARAPlaybackControllerInterface`); the host may ignore them.
/// Not from the audio thread.
pub struct PlaybackController<'a> {
    playback: &'a Playback,
}

impl<'a> PlaybackController<'a> {
    pub(crate) fn new(playback: &'a Playback) -> Self {
        PlaybackController { playback }
    }

    pub fn request_start_playback(&self) {
        // SAFETY: (each) checked present at creation; the host allows these from any non-render thread.
        if let Some(f) = self.playback.i.requestStartPlayback {
            unsafe { f(self.playback.host_ref) }
        }
    }
    pub fn request_stop_playback(&self) {
        if let Some(f) = self.playback.i.requestStopPlayback {
            unsafe { f(self.playback.host_ref) }
        }
    }
    pub fn request_set_playback_position(&self, seconds: f64) {
        if let Some(f) = self.playback.i.requestSetPlaybackPosition {
            unsafe { f(self.playback.host_ref, seconds) }
        }
    }
    pub fn request_set_cycle_range(&self, start: f64, duration: f64) {
        if let Some(f) = self.playback.i.requestSetCycleRange {
            unsafe { f(self.playback.host_ref, start, duration) }
        }
    }
    pub fn request_enable_cycle(&self, enable: bool) {
        if let Some(f) = self.playback.i.requestEnableCycle {
            unsafe { f(self.playback.host_ref, crate::util::ara_bool(enable)) }
        }
    }
}

// ---- archives ----------------------------------------------------------------------------------

/// The host's archive reader for one `restoreObjectsFromArchive` call.
pub struct ArchiveReader<'a> {
    archiving: &'a Archiving,
    reader: sys::ARAArchiveReaderHostRef,
}

impl<'a> ArchiveReader<'a> {
    pub(crate) fn new(archiving: &'a Archiving, reader: sys::ARAArchiveReaderHostRef) -> Self {
        ArchiveReader { archiving, reader }
    }

    pub fn size(&self) -> usize {
        // SAFETY: the host's reader, valid for this restore call.
        unsafe { (self.archiving.size)(self.archiving.host_ref, self.reader) }
    }

    /// Reads `buffer.len()` bytes at `position`; false on the host's I/O error.
    pub fn read(&self, position: usize, buffer: &mut [u8]) -> bool {
        if buffer.is_empty() {
            return true;
        }
        // SAFETY: as above; the buffer holds `len` bytes. Reading past the end is the caller's
        // error, which `read_all` never makes.
        unsafe {
            (self.archiving.read)(
                self.archiving.host_ref,
                self.reader,
                position,
                buffer.len(),
                buffer.as_mut_ptr(),
            ) != sys::kARAFalse
        }
    }

    /// The whole archive, read in chunks with unarchiving progress; `None` on an I/O error.
    pub fn read_all(&self) -> Option<Vec<u8>> {
        const CHUNK: usize = 1 << 20;
        let size = self.size();
        let mut bytes = vec![0u8; size];
        let mut position = 0;
        while position < size {
            let end = (position + CHUNK).min(size);
            if !self.read(position, &mut bytes[position..end]) {
                return None;
            }
            position = end;
            self.progress(position as f32 / size as f32 * 0.5);
        }
        Some(bytes)
    }

    /// The document archive ID the archive was stored with (ARA 2.0 final hosts).
    pub fn document_archive_id(&self) -> Option<String> {
        let f = self.archiving.archive_id?;
        // SAFETY: as above; the host owns the string while the reader lives.
        unsafe { opt_string(f(self.archiving.host_ref, self.reader)) }
    }

    pub fn progress(&self, value: f32) {
        // SAFETY: as above.
        unsafe {
            (self.archiving.unarchiving_progress)(self.archiving.host_ref, value.clamp(0.0, 1.0))
        }
    }
}

/// The host's archive writer for one store call, written front to back.
pub struct ArchiveWriter<'a> {
    archiving: &'a Archiving,
    writer: sys::ARAArchiveWriterHostRef,
    position: usize,
    ok: bool,
}

impl<'a> ArchiveWriter<'a> {
    pub(crate) fn new(archiving: &'a Archiving, writer: sys::ARAArchiveWriterHostRef) -> Self {
        ArchiveWriter {
            archiving,
            writer,
            position: 0,
            ok: true,
        }
    }

    /// Appends bytes; false once the host's I/O failed (later writes are skipped).
    pub fn write(&mut self, bytes: &[u8]) -> bool {
        if !self.ok || bytes.is_empty() {
            return self.ok;
        }
        // SAFETY: the host's writer, valid for this store call; `bytes` is readable.
        self.ok = unsafe {
            (self.archiving.write)(
                self.archiving.host_ref,
                self.writer,
                self.position,
                bytes.len(),
                bytes.as_ptr(),
            ) != sys::kARAFalse
        };
        self.position += bytes.len();
        self.ok
    }

    pub fn ok(&self) -> bool {
        self.ok
    }

    pub fn progress(&self, value: f32) {
        // SAFETY: as above.
        unsafe {
            (self.archiving.archiving_progress)(self.archiving.host_ref, value.clamp(0.0, 1.0))
        }
    }
}
