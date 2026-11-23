//! ARA's content: the types, their grades and update scopes, the events a host gives
//! ([`HostContent`]) and the events a plug-in offers ([`ContentEvents`]).

use std::ffi::{CString, c_void};

use crate::sys;
use crate::util::{c_string, opt_string};

/// A kind of content shared between host and plug-in (`ARAContentType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContentType {
    Notes,
    TempoEntries,
    BarSignatures,
    StaticTuning,
    KeySignatures,
    SheetChords,
}

impl ContentType {
    pub fn raw(self) -> sys::ARAContentType {
        match self {
            ContentType::Notes => sys::kARAContentTypeNotes,
            ContentType::TempoEntries => sys::kARAContentTypeTempoEntries,
            ContentType::BarSignatures => sys::kARAContentTypeBarSignatures,
            ContentType::StaticTuning => sys::kARAContentTypeStaticTuning,
            ContentType::KeySignatures => sys::kARAContentTypeKeySignatures,
            ContentType::SheetChords => sys::kARAContentTypeSheetChords,
        }
    }

    /// `None` for a type this API generation doesn't define.
    pub fn from_raw(raw: sys::ARAContentType) -> Option<Self> {
        Some(match raw {
            sys::kARAContentTypeNotes => ContentType::Notes,
            sys::kARAContentTypeTempoEntries => ContentType::TempoEntries,
            sys::kARAContentTypeBarSignatures => ContentType::BarSignatures,
            sys::kARAContentTypeStaticTuning => ContentType::StaticTuning,
            sys::kARAContentTypeKeySignatures => ContentType::KeySignatures,
            sys::kARAContentTypeSheetChords => ContentType::SheetChords,
            _ => return None,
        })
    }
}

/// How reliable content is (`ARAContentGrade`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContentGrade {
    Initial,
    Detected,
    Adjusted,
    Approved,
}

impl ContentGrade {
    pub fn raw(self) -> sys::ARAContentGrade {
        match self {
            ContentGrade::Initial => sys::kARAContentGradeInitial,
            ContentGrade::Detected => sys::kARAContentGradeDetected,
            ContentGrade::Adjusted => sys::kARAContentGradeAdjusted,
            ContentGrade::Approved => sys::kARAContentGradeApproved,
        }
    }

    pub fn from_raw(raw: sys::ARAContentGrade) -> Self {
        match raw {
            sys::kARAContentGradeDetected => ContentGrade::Detected,
            sys::kARAContentGradeAdjusted => ContentGrade::Adjusted,
            sys::kARAContentGradeApproved => ContentGrade::Approved,
            _ => ContentGrade::Initial,
        }
    }
}

/// What a content update leaves unchanged (`ARAContentUpdateFlags`): the flags are guarantees,
/// and none set means everything changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ContentUpdateFlags(pub sys::ARAContentUpdateFlags);

impl ContentUpdateFlags {
    pub const EVERYTHING_CHANGED: Self = Self(sys::kARAContentUpdateEverythingChanged);
    pub const SIGNAL_UNCHANGED: Self = Self(sys::kARAContentUpdateSignalScopeRemainsUnchanged);
    pub const NOTES_UNCHANGED: Self = Self(sys::kARAContentUpdateNoteScopeRemainsUnchanged);
    pub const TIMING_UNCHANGED: Self = Self(sys::kARAContentUpdateTimingScopeRemainsUnchanged);
    pub const TUNING_UNCHANGED: Self = Self(sys::kARAContentUpdateTuningScopeRemainsUnchanged);
    pub const HARMONICS_UNCHANGED: Self = Self(sys::kARAContentUpdateHarmonicScopeRemainsUnchanged);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the samples (the signal) may have changed.
    pub fn signal_changed(self) -> bool {
        !self.contains(Self::SIGNAL_UNCHANGED)
    }
}

impl std::ops::BitOr for ContentUpdateFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A range of time in seconds (`ARAContentTimeRange`): the start is in it, the end is not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeRange {
    pub start: f64,
    pub duration: f64,
}

impl TimeRange {
    pub(crate) fn raw(self) -> sys::ARAContentTimeRange {
        sys::ARAContentTimeRange {
            start: self.start,
            duration: self.duration,
        }
    }

    /// # Safety
    /// `p` must be null or point to a valid range.
    pub(crate) unsafe fn from_ptr(p: *const sys::ARAContentTimeRange) -> Option<Self> {
        if p.is_null() {
            return None;
        }
        // SAFETY: non-null and valid (caller's contract); read unaligned for packed layouts.
        let r = unsafe { p.read_unaligned() };
        Some(TimeRange {
            start: r.start,
            duration: r.duration,
        })
    }
}

/// A tempo sync point (`ARAContentTempoEntry`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoEntry {
    pub time_position: f64,
    pub quarter_position: f64,
}

/// A bar signature change (`ARAContentBarSignature`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarSignature {
    pub numerator: i32,
    pub denominator: i32,
    pub position: f64,
}

/// A key signature change (`ARAContentKeySignature`): the root in the circle of fifths from C,
/// and which of the twelve intervals above it the scale uses.
#[derive(Clone, Debug, PartialEq)]
pub struct KeySignature {
    pub root: i32,
    pub intervals: [bool; 12],
    pub name: Option<String>,
    pub position: f64,
}

impl KeySignature {
    /// The root as a pitch class, C = 0.
    pub fn root_pitch_class(&self) -> u8 {
        (self.root * 7).rem_euclid(12) as u8
    }
}

/// The static tuning (`ARAContentTuning`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    pub concert_pitch_frequency: f32,
    pub root: i32,
    pub tunings: [f32; 12],
    pub name: Option<String>,
}

/// A note (`ARAContentNote`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub frequency: f32,
    pub pitch_number: i32,
    pub volume: f32,
    pub start_position: f64,
    pub attack_duration: f64,
    pub note_duration: f64,
    pub signal_duration: f64,
}

/// A sheet chord (`ARAContentChord`): interval bytes as ARA defines them (0 unused, 0xFF used,
/// or a diatonic degree).
#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub root: i32,
    pub bass: i32,
    pub intervals: [u8; 12],
    pub name: Option<String>,
    pub position: f64,
}

/// A content event the host can give: its type, and how to copy it out of the host's struct.
pub trait HostContent: Sized {
    const TYPE: ContentType;

    /// # Safety
    /// `data` must point to the host's event struct for [`HostContent::TYPE`], valid for the call.
    unsafe fn copy_from(data: *const c_void) -> Self;
}

impl HostContent for TempoEntry {
    const TYPE: ContentType = ContentType::TempoEntries;
    unsafe fn copy_from(data: *const c_void) -> Self {
        // SAFETY: the caller's contract; unaligned for packed layouts.
        let e = unsafe { data.cast::<sys::ARAContentTempoEntry>().read_unaligned() };
        TempoEntry {
            time_position: e.timePosition,
            quarter_position: e.quarterPosition,
        }
    }
}

impl HostContent for BarSignature {
    const TYPE: ContentType = ContentType::BarSignatures;
    unsafe fn copy_from(data: *const c_void) -> Self {
        // SAFETY: as above.
        let e = unsafe { data.cast::<sys::ARAContentBarSignature>().read_unaligned() };
        BarSignature {
            numerator: e.numerator,
            denominator: e.denominator,
            position: e.position,
        }
    }
}

impl HostContent for KeySignature {
    const TYPE: ContentType = ContentType::KeySignatures;
    unsafe fn copy_from(data: *const c_void) -> Self {
        // SAFETY: as above; the name lives as long as the event.
        let e = unsafe { data.cast::<sys::ARAContentKeySignature>().read_unaligned() };
        KeySignature {
            root: e.root,
            intervals: e.intervals.map(|u| u != 0),
            name: unsafe { opt_string(e.name) },
            position: e.position,
        }
    }
}

impl HostContent for Tuning {
    const TYPE: ContentType = ContentType::StaticTuning;
    unsafe fn copy_from(data: *const c_void) -> Self {
        // SAFETY: as above.
        let e = unsafe { data.cast::<sys::ARAContentTuning>().read_unaligned() };
        Tuning {
            concert_pitch_frequency: e.concertPitchFrequency,
            root: e.root,
            tunings: e.tunings,
            name: unsafe { opt_string(e.name) },
        }
    }
}

impl HostContent for Note {
    const TYPE: ContentType = ContentType::Notes;
    unsafe fn copy_from(data: *const c_void) -> Self {
        // SAFETY: as above.
        let e = unsafe { data.cast::<sys::ARAContentNote>().read_unaligned() };
        Note {
            frequency: e.frequency,
            pitch_number: e.pitchNumber,
            volume: e.volume,
            start_position: e.startPosition,
            attack_duration: e.attackDuration,
            note_duration: e.noteDuration,
            signal_duration: e.signalDuration,
        }
    }
}

impl HostContent for Chord {
    const TYPE: ContentType = ContentType::SheetChords;
    unsafe fn copy_from(data: *const c_void) -> Self {
        // SAFETY: as above.
        let e = unsafe { data.cast::<sys::ARAContentChord>().read_unaligned() };
        Chord {
            root: e.root,
            bass: e.bass,
            intervals: e.intervals,
            name: unsafe { opt_string(e.name) },
            position: e.position,
        }
    }
}

/// Content the host gave: its grade and its events, copied.
#[derive(Clone, Debug, PartialEq)]
pub struct HostContentEvents<T> {
    pub grade: ContentGrade,
    pub events: Vec<T>,
}

/// Content a plug-in offers the host through a content reader.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentEvents {
    Notes(Vec<Note>),
    TempoEntries(Vec<TempoEntry>),
    BarSignatures(Vec<BarSignature>),
    StaticTuning(Tuning),
    KeySignatures(Vec<KeySignature>),
    SheetChords(Vec<Chord>),
}

impl ContentEvents {
    pub fn content_type(&self) -> ContentType {
        match self {
            ContentEvents::Notes(_) => ContentType::Notes,
            ContentEvents::TempoEntries(_) => ContentType::TempoEntries,
            ContentEvents::BarSignatures(_) => ContentType::BarSignatures,
            ContentEvents::StaticTuning(_) => ContentType::StaticTuning,
            ContentEvents::KeySignatures(_) => ContentType::KeySignatures,
            ContentEvents::SheetChords(_) => ContentType::SheetChords,
        }
    }
}

/// A plug-in content reader as the host holds it: the events in ARA's layout, with the strings
/// they point to, alive until the host destroys the reader.
pub(crate) struct PlugInContentReader {
    events: RawEvents,
    _names: Vec<CString>,
}

enum RawEvents {
    Notes(Vec<sys::ARAContentNote>),
    Tempo(Vec<sys::ARAContentTempoEntry>),
    Bars(Vec<sys::ARAContentBarSignature>),
    Tuning(Vec<sys::ARAContentTuning>),
    Keys(Vec<sys::ARAContentKeySignature>),
    Chords(Vec<sys::ARAContentChord>),
}

fn name_ptr(names: &mut Vec<CString>, name: &Option<String>) -> *const std::ffi::c_char {
    match name {
        Some(n) => {
            names.push(c_string(n));
            names.last().map_or(std::ptr::null(), |c| c.as_ptr())
        }
        None => std::ptr::null(),
    }
}

fn usage(used: bool) -> u8 {
    if used { 0xFF } else { 0 }
}

impl PlugInContentReader {
    pub(crate) fn new(events: ContentEvents) -> Self {
        let mut names = Vec::new();
        let events = match events {
            ContentEvents::Notes(v) => RawEvents::Notes(
                v.iter()
                    .map(|n| sys::ARAContentNote {
                        frequency: n.frequency,
                        pitchNumber: n.pitch_number,
                        volume: n.volume,
                        startPosition: n.start_position,
                        attackDuration: n.attack_duration,
                        noteDuration: n.note_duration,
                        signalDuration: n.signal_duration,
                    })
                    .collect(),
            ),
            ContentEvents::TempoEntries(v) => RawEvents::Tempo(
                v.iter()
                    .map(|t| sys::ARAContentTempoEntry {
                        timePosition: t.time_position,
                        quarterPosition: t.quarter_position,
                    })
                    .collect(),
            ),
            ContentEvents::BarSignatures(v) => RawEvents::Bars(
                v.iter()
                    .map(|b| sys::ARAContentBarSignature {
                        numerator: b.numerator,
                        denominator: b.denominator,
                        position: b.position,
                    })
                    .collect(),
            ),
            ContentEvents::StaticTuning(t) => RawEvents::Tuning(vec![sys::ARAContentTuning {
                concertPitchFrequency: t.concert_pitch_frequency,
                root: t.root,
                tunings: t.tunings,
                name: name_ptr(&mut names, &t.name),
            }]),
            ContentEvents::KeySignatures(v) => RawEvents::Keys(
                v.iter()
                    .map(|k| sys::ARAContentKeySignature {
                        root: k.root,
                        intervals: k.intervals.map(usage),
                        name: name_ptr(&mut names, &k.name),
                        position: k.position,
                    })
                    .collect(),
            ),
            ContentEvents::SheetChords(v) => RawEvents::Chords(
                v.iter()
                    .map(|c| sys::ARAContentChord {
                        root: c.root,
                        bass: c.bass,
                        intervals: c.intervals,
                        name: name_ptr(&mut names, &c.name),
                        position: c.position,
                    })
                    .collect(),
            ),
        };
        // The CStrings' heap buffers don't move when the Vec holding them grows, so the
        // pointers taken above stay valid as long as `names` lives.
        PlugInContentReader {
            events,
            _names: names,
        }
    }

    pub(crate) fn len(&self) -> usize {
        match &self.events {
            RawEvents::Notes(v) => v.len(),
            RawEvents::Tempo(v) => v.len(),
            RawEvents::Bars(v) => v.len(),
            RawEvents::Tuning(v) => v.len(),
            RawEvents::Keys(v) => v.len(),
            RawEvents::Chords(v) => v.len(),
        }
    }

    /// The event's data in ARA's layout, or null for an index out of range.
    pub(crate) fn event(&self, index: usize) -> *const c_void {
        fn at<T>(v: &[T], i: usize) -> *const c_void {
            v.get(i)
                .map_or(std::ptr::null(), |e| std::ptr::from_ref(e).cast())
        }
        match &self.events {
            RawEvents::Notes(v) => at(v, index),
            RawEvents::Tempo(v) => at(v, index),
            RawEvents::Bars(v) => at(v, index),
            RawEvents::Tuning(v) => at(v, index),
            RawEvents::Keys(v) => at(v, index),
            RawEvents::Chords(v) => at(v, index),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_signatures_root_counts_fifths_from_c() {
        let key = |root| KeySignature {
            root,
            intervals: [false; 12],
            name: None,
            position: 0.0,
        };
        assert_eq!(key(0).root_pitch_class(), 0);
        assert_eq!(key(1).root_pitch_class(), 7, "G");
        assert_eq!(key(-1).root_pitch_class(), 5, "F");
        assert_eq!(key(-5).root_pitch_class(), 1, "Db");
        assert_eq!(key(7).root_pitch_class(), 1, "C#");
    }

    #[test]
    fn offered_events_keep_their_names_alive() {
        let reader = PlugInContentReader::new(ContentEvents::KeySignatures(vec![KeySignature {
            root: 2,
            intervals: [true; 12],
            name: Some("D major".into()),
            position: 4.0,
        }]));
        assert_eq!(reader.len(), 1);
        // SAFETY: index 0 exists and points to a key signature event.
        let back = unsafe { KeySignature::copy_from(reader.event(0)) };
        assert_eq!(back.name.as_deref(), Some("D major"));
        assert_eq!(back.position, 4.0);
        assert!(reader.event(1).is_null());
    }
}
