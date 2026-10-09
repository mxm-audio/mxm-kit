//! `IEventList`: the note events of one block, fixed in size so the audio thread never allocates,
//! and the VST3 events CLAP's notes and MIDI become.

use std::cell::{Cell, UnsafeCell};
use vst3::Steinberg::Vst::Event_::EventTypes_::{kNoteOffEvent, kNoteOnEvent, kPolyPressureEvent};
use vst3::Steinberg::Vst::{
    Event, IEventList, IEventListTrait, NoteOffEvent, NoteOnEvent, PolyPressureEvent,
};
use vst3::Steinberg::{int32, kInvalidArgument, kResultFalse, kResultOk, tresult};
use vst3::{Class, ComWrapper};

pub(crate) struct EventList {
    events: UnsafeCell<Box<[Event]>>,
    len: Cell<usize>,
}

// SAFETY: a list is only ever touched by the audio thread that owns the processor using it, and by
// the plugin within the `process` call that thread makes. It moves between threads only with the
// processor, whole.
unsafe impl Send for EventList {}
// SAFETY: as above: never used from two threads at once.
unsafe impl Sync for EventList {}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl EventList {
    pub(crate) fn new(capacity: usize) -> Self {
        // SAFETY: `Event` is plain data (integers, floats and a union of such); all zeros is a
        // valid note-on.
        let empty: Event = unsafe { std::mem::zeroed() };
        Self {
            events: UnsafeCell::new(vec![empty; capacity.max(1)].into_boxed_slice()),
            len: Cell::new(0),
        }
    }

    pub(crate) fn clear(&self) {
        self.len.set(0);
    }

    /// Adds `event`; false when the list is full.
    pub(crate) fn push(&self, event: Event) -> bool {
        // SAFETY: only this thread touches the events (see the `Send` impl), and no reference to
        // them outlives a method.
        let events = unsafe { &mut *self.events.get() };
        let len = self.len.get();
        if len == events.len() {
            return false;
        }
        events[len] = event;
        self.len.set(len + 1);
        true
    }

    pub(crate) fn as_ptr(list: &ComWrapper<EventList>) -> *mut IEventList {
        list.as_com_ref::<IEventList>()
            .map(|list| list.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.len.get() as int32
    }

    unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: as in `push`.
        let events = unsafe { &*self.events.get() };
        match usize::try_from(index) {
            Ok(index) if index < self.len.get() => {
                // SAFETY: checked non-null.
                unsafe { *e = events[index] };
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn addEvent(&self, e: *mut Event) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: checked non-null; `Event` is `Copy`.
        if self.push(unsafe { *e }) {
            kResultOk
        } else {
            kResultFalse
        }
    }
}

fn event(offset: u32, kind: u32) -> Event {
    // SAFETY: as in `EventList::new`.
    let mut event: Event = unsafe { std::mem::zeroed() };
    event.busIndex = 0;
    event.sampleOffset = offset as int32;
    event.r#type = kind as u16;
    event
}

/// A note-on: `velocity` 0 to 1, `note_id` -1 when there is none.
pub(crate) fn note_on(offset: u32, channel: i16, pitch: i16, velocity: f32, note_id: i32) -> Event {
    let mut event = event(offset, kNoteOnEvent as u32);
    event.__field0.noteOn = NoteOnEvent {
        channel,
        pitch,
        tuning: 0.0,
        velocity,
        length: 0,
        noteId: note_id,
    };
    event
}

/// A note-off: `velocity` 0 to 1, `note_id` -1 when there is none.
pub(crate) fn note_off(
    offset: u32,
    channel: i16,
    pitch: i16,
    velocity: f32,
    note_id: i32,
) -> Event {
    let mut event = event(offset, kNoteOffEvent as u32);
    event.__field0.noteOff = NoteOffEvent {
        channel,
        pitch,
        velocity,
        noteId: note_id,
        tuning: 0.0,
    };
    event
}

/// Polyphonic pressure, 0 to 1.
pub(crate) fn poly_pressure(
    offset: u32,
    channel: i16,
    pitch: i16,
    pressure: f32,
    note_id: i32,
) -> Event {
    let mut event = event(offset, kPolyPressureEvent as u32);
    event.__field0.polyPressure = PolyPressureEvent {
        channel,
        pitch,
        pressure,
        noteId: note_id,
    };
    event
}
