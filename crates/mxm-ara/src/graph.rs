//! The model graph the document controller keeps: the document, its musical contexts, region
//! sequences and audio sources, each source's audio modifications and each modification's
//! playback regions, with their properties and parent links.
//!
//! Every object is a heap node whose address is its ARA ref (and its id), stable for its life.
//! The model thread edits the graph only while the renderers' guard is closed; renderers read it
//! only inside the guard, so a `&Graph` never sees a change.

use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::controller::{Controller, Payload};
use crate::sys;
use crate::util::{opt_string, read_field};

macro_rules! ids {
    ($($(#[$doc:meta])* $name:ident => $raw:ty;)*) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub(crate) usize);

        impl $name {
            /// The ARA ref the host holds for this object.
            pub fn to_ref(self) -> $raw {
                self.0 as $raw
            }

            /// The id behind an ARA ref (whether it is live is the graph's to say).
            pub fn from_ref(r: $raw) -> Self {
                Self(r as usize)
            }
        }
    )*};
}

ids! {
    /// A musical context (`ARAMusicalContextRef`).
    MusicalContextId => sys::ARAMusicalContextRef;
    /// A region sequence (`ARARegionSequenceRef`).
    RegionSequenceId => sys::ARARegionSequenceRef;
    /// An audio source (`ARAAudioSourceRef`).
    AudioSourceId => sys::ARAAudioSourceRef;
    /// An audio modification (`ARAAudioModificationRef`).
    AudioModificationId => sys::ARAAudioModificationRef;
    /// A playback region (`ARAPlaybackRegionRef`).
    PlaybackRegionId => sys::ARAPlaybackRegionRef;
}

/// A plug-in instance bound to the document (its plug-in extension).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtensionId(pub(crate) usize);

/// An ARA color, 0.0 to 1.0 per channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// # Safety
/// `p` must be null or point to a valid color.
unsafe fn color(p: *const sys::ARAColor) -> Option<Color> {
    if p.is_null() {
        None
    } else {
        // SAFETY: non-null and valid (caller's contract).
        let c = unsafe { p.read_unaligned() };
        Some(Color {
            r: c.r,
            g: c.g,
            b: c.b,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentProperties {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MusicalContextProperties {
    pub name: Option<String>,
    pub order_index: i32,
    pub color: Option<Color>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegionSequenceProperties {
    pub name: Option<String>,
    pub order_index: i32,
    pub musical_context: MusicalContextId,
    pub color: Option<Color>,
}

/// Spatial channel layout as the companion API defines it (`ARAChannelArrangementDataType` and
/// a copy of the data it points to); absent for mono and stereo.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelArrangement {
    pub data_type: sys::ARAChannelArrangementDataType,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioSourceProperties {
    pub name: Option<String>,
    pub persistent_id: String,
    pub sample_count: i64,
    pub sample_rate: f64,
    pub channel_count: i32,
    pub merits_64_bit_samples: bool,
    pub channel_arrangement: Option<ChannelArrangement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioModificationProperties {
    pub name: Option<String>,
    pub persistent_id: String,
}

/// A playback region's properties: which part of its modification plays where in the song.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackRegionProperties {
    pub transformation_flags: sys::ARAPlaybackTransformationFlags,
    pub start_in_modification_time: f64,
    pub duration_in_modification_time: f64,
    pub start_in_playback_time: f64,
    pub duration_in_playback_time: f64,
    /// Required since ARA 2.0; `None` only from a host whose struct predates it.
    pub region_sequence: Option<RegionSequenceId>,
    pub name: Option<String>,
    pub color: Option<Color>,
}

impl PlaybackRegionProperties {
    pub fn end_in_playback_time(&self) -> f64 {
        self.start_in_playback_time + self.duration_in_playback_time
    }

    pub fn end_in_modification_time(&self) -> f64 {
        self.start_in_modification_time + self.duration_in_modification_time
    }
}

// Copying properties out of the host's structs, field by field as each struct's size allows.
// Each takes a pointer the caller has checked to be non-null with at least the minimum size.

pub(crate) unsafe fn document_properties(
    p: *const sys::ARADocumentProperties,
) -> DocumentProperties {
    // SAFETY: (all below) the caller's contract; `read_field!` stops at `structSize`.
    unsafe {
        DocumentProperties {
            name: read_field!(p, sys::ARADocumentProperties, name).and_then(|n| opt_string(n)),
        }
    }
}

pub(crate) unsafe fn musical_context_properties(
    p: *const sys::ARAMusicalContextProperties,
) -> MusicalContextProperties {
    unsafe {
        MusicalContextProperties {
            name: read_field!(p, sys::ARAMusicalContextProperties, name)
                .and_then(|n| opt_string(n)),
            order_index: read_field!(p, sys::ARAMusicalContextProperties, orderIndex).unwrap_or(0),
            color: read_field!(p, sys::ARAMusicalContextProperties, color).and_then(|c| color(c)),
        }
    }
}

/// The musical context is resolved by the caller (it must be live).
pub(crate) unsafe fn region_sequence_properties(
    p: *const sys::ARARegionSequenceProperties,
) -> (RegionSequenceProperties, MusicalContextId) {
    unsafe {
        let context = MusicalContextId::from_ref(
            read_field!(p, sys::ARARegionSequenceProperties, musicalContextRef)
                .unwrap_or(std::ptr::null_mut()),
        );
        (
            RegionSequenceProperties {
                name: read_field!(p, sys::ARARegionSequenceProperties, name)
                    .and_then(|n| opt_string(n)),
                order_index: read_field!(p, sys::ARARegionSequenceProperties, orderIndex)
                    .unwrap_or(0),
                musical_context: context,
                color: read_field!(p, sys::ARARegionSequenceProperties, color)
                    .and_then(|c| color(c)),
            },
            context,
        )
    }
}

pub(crate) unsafe fn audio_source_properties(
    p: *const sys::ARAAudioSourceProperties,
) -> AudioSourceProperties {
    unsafe {
        let channel_count =
            read_field!(p, sys::ARAAudioSourceProperties, channelCount).unwrap_or(0);
        let data_type = read_field!(p, sys::ARAAudioSourceProperties, channelArrangementDataType)
            .unwrap_or(sys::kARAChannelArrangementUndefined);
        let data = read_field!(p, sys::ARAAudioSourceProperties, channelArrangement)
            .unwrap_or(std::ptr::null());
        AudioSourceProperties {
            name: read_field!(p, sys::ARAAudioSourceProperties, name).and_then(|n| opt_string(n)),
            persistent_id: read_field!(p, sys::ARAAudioSourceProperties, persistentID)
                .and_then(|n| opt_string(n))
                .unwrap_or_default(),
            sample_count: read_field!(p, sys::ARAAudioSourceProperties, sampleCount).unwrap_or(0),
            sample_rate: read_field!(p, sys::ARAAudioSourceProperties, sampleRate).unwrap_or(0.0),
            channel_count,
            merits_64_bit_samples: read_field!(
                p,
                sys::ARAAudioSourceProperties,
                merits64BitSamples
            )
            .is_some_and(|b| b != sys::kARAFalse),
            channel_arrangement: channel_arrangement(data_type, data, channel_count),
        }
    }
}

/// Copies a channel arrangement of a type whose size is known; `None` for undefined or unknown.
unsafe fn channel_arrangement(
    data_type: sys::ARAChannelArrangementDataType,
    data: *const std::ffi::c_void,
    channel_count: i32,
) -> Option<ChannelArrangement> {
    if data.is_null() {
        return None;
    }
    let len = match data_type {
        sys::kARAChannelArrangementVST3SpeakerArrangement => 8, // Steinberg::Vst::SpeakerArrangement (uint64)
        sys::kARAChannelArrangementAAXStemFormat => 4,          // AAX_EStemFormat (int32)
        sys::kARAChannelArrangementCLAPChannelMap => usize::try_from(channel_count).unwrap_or(0), // uint8 per channel
        sys::kARAChannelArrangementCLAPAmbisonicInfo => 8, // clap_ambisonic_config (2 x uint32)
        sys::kARAChannelArrangementCoreAudioChannelLayout => {
            // AudioChannelLayout: tag, bitmap, count (3 x UInt32), then `count` 20-byte descriptions.
            // SAFETY: the header's 12 bytes exist for this data type (caller's contract).
            let count = unsafe { data.cast::<u32>().add(2).read_unaligned() } as usize;
            12 + 20 * count.min(256)
        }
        _ => return None,
    };
    // SAFETY: `len` bytes of the arrangement exist for its declared type.
    let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec();
    Some(ChannelArrangement {
        data_type,
        data: bytes,
    })
}

pub(crate) unsafe fn audio_modification_properties(
    p: *const sys::ARAAudioModificationProperties,
) -> AudioModificationProperties {
    unsafe {
        AudioModificationProperties {
            name: read_field!(p, sys::ARAAudioModificationProperties, name)
                .and_then(|n| opt_string(n)),
            persistent_id: read_field!(p, sys::ARAAudioModificationProperties, persistentID)
                .and_then(|n| opt_string(n))
                .unwrap_or_default(),
        }
    }
}

/// The region sequence is copied as given; the caller checks it is live.
pub(crate) unsafe fn playback_region_properties(
    p: *const sys::ARAPlaybackRegionProperties,
) -> PlaybackRegionProperties {
    unsafe {
        PlaybackRegionProperties {
            transformation_flags: read_field!(
                p,
                sys::ARAPlaybackRegionProperties,
                transformationFlags
            )
            .unwrap_or(0),
            start_in_modification_time: read_field!(
                p,
                sys::ARAPlaybackRegionProperties,
                startInModificationTime
            )
            .unwrap_or(0.0),
            duration_in_modification_time: read_field!(
                p,
                sys::ARAPlaybackRegionProperties,
                durationInModificationTime
            )
            .unwrap_or(0.0),
            start_in_playback_time: read_field!(
                p,
                sys::ARAPlaybackRegionProperties,
                startInPlaybackTime
            )
            .unwrap_or(0.0),
            duration_in_playback_time: read_field!(
                p,
                sys::ARAPlaybackRegionProperties,
                durationInPlaybackTime
            )
            .unwrap_or(0.0),
            region_sequence: read_field!(p, sys::ARAPlaybackRegionProperties, regionSequenceRef)
                .filter(|r| !r.is_null())
                .map(RegionSequenceId::from_ref),
            name: read_field!(p, sys::ARAPlaybackRegionProperties, name)
                .and_then(|n| opt_string(n)),
            color: read_field!(p, sys::ARAPlaybackRegionProperties, color).and_then(|c| color(c)),
        }
    }
}

// ---- nodes -------------------------------------------------------------------------------------

/// A musical context: the song's tempo, bars and harmony, which its content readers give.
pub struct MusicalContext {
    pub(crate) id: MusicalContextId,
    pub(crate) host_ref: usize,
    pub(crate) properties: MusicalContextProperties,
    pub(crate) region_sequences: Vec<RegionSequenceId>,
}

impl MusicalContext {
    pub fn id(&self) -> MusicalContextId {
        self.id
    }
    pub fn properties(&self) -> &MusicalContextProperties {
        &self.properties
    }
    pub fn region_sequences(&self) -> &[RegionSequenceId] {
        &self.region_sequences
    }
}

/// A region sequence: a host track or lane.
pub struct RegionSequence {
    pub(crate) id: RegionSequenceId,
    pub(crate) properties: RegionSequenceProperties,
    pub(crate) playback_regions: Vec<PlaybackRegionId>,
}

impl RegionSequence {
    pub fn id(&self) -> RegionSequenceId {
        self.id
    }
    pub fn properties(&self) -> &RegionSequenceProperties {
        &self.properties
    }
    pub fn musical_context(&self) -> MusicalContextId {
        self.properties.musical_context
    }
    pub fn playback_regions(&self) -> &[PlaybackRegionId] {
        &self.playback_regions
    }
}

/// An audio source: a recording the host reads to the plug-in through audio readers.
pub struct AudioSource<C: Controller> {
    pub(crate) id: AudioSourceId,
    pub(crate) host_ref: usize,
    pub(crate) properties: AudioSourceProperties,
    pub(crate) sample_access_enabled: bool,
    pub(crate) deactivated: bool,
    pub(crate) audio_modifications: Vec<AudioModificationId>,
    pub(crate) kept_archive: Option<Payload>,
    pub data: C::AudioSource,
}

impl<C: Controller> AudioSource<C> {
    pub fn id(&self) -> AudioSourceId {
        self.id
    }
    pub fn properties(&self) -> &AudioSourceProperties {
        &self.properties
    }
    pub fn persistent_id(&self) -> &str {
        &self.properties.persistent_id
    }
    pub fn is_sample_access_enabled(&self) -> bool {
        self.sample_access_enabled
    }
    pub fn is_deactivated_for_undo_history(&self) -> bool {
        self.deactivated
    }
    pub fn audio_modifications(&self) -> &[AudioModificationId] {
        &self.audio_modifications
    }
    /// The archived state of a newer schema than this plug-in reads, kept verbatim and written
    /// back unchanged on every store (`Controller::ARCHIVE_SCHEMA`).
    pub fn kept_archive(&self) -> Option<&Payload> {
        self.kept_archive.as_ref()
    }
    /// Drops the kept newer state, so the next store asks the controller again.
    pub fn discard_kept_archive(&mut self) -> Option<Payload> {
        self.kept_archive.take()
    }
}

/// An audio modification: the edits to its audio source, shared by every region that plays it.
pub struct AudioModification<C: Controller> {
    pub(crate) id: AudioModificationId,
    pub(crate) host_ref: usize,
    pub(crate) audio_source: AudioSourceId,
    pub(crate) properties: AudioModificationProperties,
    pub(crate) deactivated: bool,
    pub(crate) playback_regions: Vec<PlaybackRegionId>,
    pub(crate) kept_archive: Option<Payload>,
    pub data: C::AudioModification,
}

impl<C: Controller> AudioModification<C> {
    pub fn id(&self) -> AudioModificationId {
        self.id
    }
    pub fn audio_source(&self) -> AudioSourceId {
        self.audio_source
    }
    pub fn properties(&self) -> &AudioModificationProperties {
        &self.properties
    }
    pub fn persistent_id(&self) -> &str {
        &self.properties.persistent_id
    }
    pub fn is_deactivated_for_undo_history(&self) -> bool {
        self.deactivated
    }
    pub fn playback_regions(&self) -> &[PlaybackRegionId] {
        &self.playback_regions
    }
    /// See [`AudioSource::kept_archive`].
    pub fn kept_archive(&self) -> Option<&Payload> {
        self.kept_archive.as_ref()
    }
    pub fn discard_kept_archive(&mut self) -> Option<Payload> {
        self.kept_archive.take()
    }
}

/// A playback region: a section of a modification placed in song time.
pub struct PlaybackRegion<C: Controller> {
    pub(crate) id: PlaybackRegionId,
    pub(crate) host_ref: usize,
    pub(crate) audio_modification: AudioModificationId,
    pub(crate) audio_source: AudioSourceId,
    pub(crate) properties: PlaybackRegionProperties,
    pub data: C::PlaybackRegion,
}

impl<C: Controller> PlaybackRegion<C> {
    pub fn id(&self) -> PlaybackRegionId {
        self.id
    }
    pub fn audio_modification(&self) -> AudioModificationId {
        self.audio_modification
    }
    pub fn audio_source(&self) -> AudioSourceId {
        self.audio_source
    }
    pub fn properties(&self) -> &PlaybackRegionProperties {
        &self.properties
    }
    pub fn region_sequence(&self) -> Option<RegionSequenceId> {
        self.properties.region_sequence
    }
}

/// What a playback region's ref points to: its head and tail time, which any thread may read
/// (`getPlaybackRegionHeadAndTailTime` may come from the audio thread, outside the guard), and
/// the region itself in an allocation of its own. Only ever shared: nothing takes `&mut` to a
/// cell, so a reader of the times never aliases an edit of the region.
pub(crate) struct RegionCell<C: Controller> {
    head: AtomicU64,
    tail: AtomicU64,
    node: NonNull<PlaybackRegion<C>>,
}

impl<C: Controller> RegionCell<C> {
    pub(crate) fn head_and_tail(&self) -> (f64, f64) {
        (
            f64::from_bits(self.head.load(Ordering::Acquire)),
            f64::from_bits(self.tail.load(Ordering::Acquire)),
        )
    }

    pub(crate) fn set_head_and_tail(&self, head: f64, tail: f64) {
        self.head.store(head.to_bits(), Ordering::Release);
        self.tail.store(tail.to_bits(), Ordering::Release);
    }
}

/// The graph's ownership of a [`RegionCell`] and its region.
pub(crate) struct RegionBox<C: Controller>(NonNull<RegionCell<C>>);

impl<C: Controller> RegionBox<C> {
    /// Places the region; its id is the cell's address.
    pub(crate) fn new(make: impl FnOnce(PlaybackRegionId) -> PlaybackRegion<C>) -> Self {
        let cell = Box::into_raw(Box::new(RegionCell {
            head: AtomicU64::new(0f64.to_bits()),
            tail: AtomicU64::new(0f64.to_bits()),
            node: NonNull::dangling(),
        }));
        let node = Box::into_raw(Box::new(make(PlaybackRegionId(cell as usize))));
        // SAFETY: `cell` was just allocated and is not yet shared.
        unsafe { (*cell).node = NonNull::new_unchecked(node) };
        // SAFETY: from `Box::into_raw`, never null.
        RegionBox(unsafe { NonNull::new_unchecked(cell) })
    }

    pub(crate) fn id(&self) -> PlaybackRegionId {
        PlaybackRegionId(self.0.as_ptr() as usize)
    }

    pub(crate) fn cell(&self) -> &RegionCell<C> {
        // SAFETY: owned by this box until it drops; only ever shared.
        unsafe { self.0.as_ref() }
    }

    pub(crate) fn region(&self) -> &PlaybackRegion<C> {
        self.get()
    }

    fn get(&self) -> &PlaybackRegion<C> {
        // SAFETY: the region lives while the box does; a shared borrow of the graph means no
        // `&mut` to it exists (the graph's edit discipline).
        unsafe { self.cell().node.as_ref() }
    }

    fn get_mut(&mut self) -> &mut PlaybackRegion<C> {
        // SAFETY: as above, and `&mut self` (from `&mut Graph`) makes this the only access.
        unsafe { &mut *self.cell().node.as_ptr() }
    }

    /// The cell behind a playback region ref, for reading its times from any thread.
    ///
    /// # Safety
    /// `region` must be a live region's ref (the host's contract for every call naming it).
    pub(crate) unsafe fn cell_of<'a>(region: PlaybackRegionId) -> &'a RegionCell<C> {
        // SAFETY: the caller's contract.
        unsafe { &*(region.0 as *const RegionCell<C>) }
    }
}

impl<C: Controller> Drop for RegionBox<C> {
    fn drop(&mut self) {
        // SAFETY: both allocations came from `Box::into_raw` in `new` and are freed once.
        unsafe {
            let cell = Box::from_raw(self.0.as_ptr());
            drop(Box::from_raw(cell.node.as_ptr()));
        }
    }
}

// SAFETY: a RegionBox owns its region like a Box; the cell's atomics are thread-safe.
unsafe impl<C: Controller> Send for RegionBox<C> {}
unsafe impl<C: Controller> Sync for RegionBox<C> {}

/// The document's model graph.
pub struct Graph<C: Controller> {
    pub(crate) document: DocumentProperties,
    pub(crate) musical_contexts: HashMap<usize, Box<MusicalContext>>,
    pub(crate) region_sequences: HashMap<usize, Box<RegionSequence>>,
    pub(crate) audio_sources: HashMap<usize, Box<AudioSource<C>>>,
    pub(crate) audio_modifications: HashMap<usize, Box<AudioModification<C>>>,
    pub(crate) playback_regions: HashMap<usize, RegionBox<C>>,
    /// The document's objects in order: contexts and sequences by order index (sorted at the
    /// end of each edit), sources by creation.
    pub(crate) context_order: Vec<MusicalContextId>,
    pub(crate) sequence_order: Vec<RegionSequenceId>,
    pub(crate) source_order: Vec<AudioSourceId>,
}

impl<C: Controller> Graph<C> {
    pub(crate) fn new(document: DocumentProperties) -> Self {
        Graph {
            document,
            musical_contexts: HashMap::new(),
            region_sequences: HashMap::new(),
            audio_sources: HashMap::new(),
            audio_modifications: HashMap::new(),
            playback_regions: HashMap::new(),
            context_order: Vec::new(),
            sequence_order: Vec::new(),
            source_order: Vec::new(),
        }
    }

    pub fn document(&self) -> &DocumentProperties {
        &self.document
    }

    pub fn musical_context(&self, id: MusicalContextId) -> Option<&MusicalContext> {
        self.musical_contexts.get(&id.0).map(|b| &**b)
    }
    pub fn region_sequence(&self, id: RegionSequenceId) -> Option<&RegionSequence> {
        self.region_sequences.get(&id.0).map(|b| &**b)
    }
    pub fn audio_source(&self, id: AudioSourceId) -> Option<&AudioSource<C>> {
        self.audio_sources.get(&id.0).map(|b| &**b)
    }
    pub fn audio_source_mut(&mut self, id: AudioSourceId) -> Option<&mut AudioSource<C>> {
        self.audio_sources.get_mut(&id.0).map(|b| &mut **b)
    }
    pub fn audio_modification(&self, id: AudioModificationId) -> Option<&AudioModification<C>> {
        self.audio_modifications.get(&id.0).map(|b| &**b)
    }
    pub fn audio_modification_mut(
        &mut self,
        id: AudioModificationId,
    ) -> Option<&mut AudioModification<C>> {
        self.audio_modifications.get_mut(&id.0).map(|b| &mut **b)
    }
    pub fn playback_region(&self, id: PlaybackRegionId) -> Option<&PlaybackRegion<C>> {
        self.playback_regions.get(&id.0).map(RegionBox::get)
    }
    pub fn playback_region_mut(&mut self, id: PlaybackRegionId) -> Option<&mut PlaybackRegion<C>> {
        self.playback_regions.get_mut(&id.0).map(RegionBox::get_mut)
    }
    /// A region's head and tail time in seconds, as last reported to the host.
    pub fn playback_region_head_and_tail_time(&self, id: PlaybackRegionId) -> Option<(f64, f64)> {
        self.playback_regions
            .get(&id.0)
            .map(|b| b.cell().head_and_tail())
    }

    /// Musical contexts in the host's order.
    pub fn musical_contexts(&self) -> impl Iterator<Item = &MusicalContext> {
        self.context_order
            .iter()
            .filter_map(|id| self.musical_context(*id))
    }
    /// Region sequences in the host's order.
    pub fn region_sequences(&self) -> impl Iterator<Item = &RegionSequence> {
        self.sequence_order
            .iter()
            .filter_map(|id| self.region_sequence(*id))
    }
    /// Audio sources in the order they were created.
    pub fn audio_sources(&self) -> impl Iterator<Item = &AudioSource<C>> {
        self.source_order
            .iter()
            .filter_map(|id| self.audio_source(*id))
    }

    pub fn audio_source_by_persistent_id(&self, id: &str) -> Option<&AudioSource<C>> {
        self.audio_sources()
            .find(|s| s.properties.persistent_id == id)
    }
    pub fn audio_modification_by_persistent_id(&self, id: &str) -> Option<&AudioModification<C>> {
        self.audio_sources()
            .flat_map(|s| s.audio_modifications.iter())
            .filter_map(|m| self.audio_modification(*m))
            .find(|m| m.properties.persistent_id == id)
    }

    /// The musical context a region plays in, through its region sequence.
    pub fn musical_context_of(&self, region: PlaybackRegionId) -> Option<MusicalContextId> {
        let sequence = self.playback_region(region)?.region_sequence()?;
        Some(self.region_sequence(sequence)?.musical_context())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.musical_contexts.is_empty()
            && self.region_sequences.is_empty()
            && self.audio_sources.is_empty()
            && self.audio_modifications.is_empty()
            && self.playback_regions.is_empty()
    }

    pub(crate) fn sort_orders(&mut self) {
        let contexts = &self.musical_contexts;
        self.context_order.sort_by_key(|id| {
            contexts
                .get(&id.0)
                .map_or(i32::MAX, |c| c.properties.order_index)
        });
        let sequences = &self.region_sequences;
        self.sequence_order.sort_by_key(|id| {
            sequences
                .get(&id.0)
                .map_or(i32::MAX, |s| s.properties.order_index)
        });
        for context in self.musical_contexts.values_mut() {
            context.region_sequences.sort_by_key(|id| {
                sequences
                    .get(&id.0)
                    .map_or(i32::MAX, |s| s.properties.order_index)
            });
        }
    }
}
