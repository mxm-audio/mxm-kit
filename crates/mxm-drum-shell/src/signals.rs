//! The telemetry Resample and the audition share between the audio thread, the capture thread and
//! the editor: a field of each drum plugin's own `Telemetry`, which also carries what only it
//! measures (its meter, its peaks, its tempo).
//!
//! Every audio-thread side is one relaxed atomic operation; nothing locks or allocates there. The
//! one lock, the export's outcome, is between the capture thread and an editor frame.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use crate::SLOT_COUNT;

/// A capture `process` asked for: the rate to render at, and the engage it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureRequest {
    /// The rate the engine is running at.
    pub sample_rate: f32,
    /// The engage that asked ([`crate::capture::Resample`]'s count), which the worker stamps on the
    /// kit so a kit made for an engage since left is refused.
    pub engagement: u32,
}

/// How the last sample-pack export finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Export {
    /// Written, with what it holds and how much clipped.
    Written { files: usize, clipped: usize },
    /// The platform has no local data folder, so there was nowhere to write.
    NoDestination,
    /// It failed, with the reason to show.
    Failed(String),
}

/// Resample's and the audition's signals.
#[derive(Debug)]
pub struct Signals {
    /// The slots the editor asked to hear, a bit a slot (`request_audition`).
    audition: AtomicU32,
    /// The last export's outcome, waiting for an editor frame to show it.
    export: Mutex<Option<Export>>,
    /// The capture `process` wants, or zero for no request: the rate as `f32` bits in the low
    /// half, the engage that asked in the high half ([`CaptureRequest`]).
    ///
    /// **The whole channel from the audio thread to the renderer is this one word.** `process`
    /// cannot render seconds of audio and cannot take a lock, so it stores a request here; the
    /// plugin's capture thread reads it and does the work. Nothing else is needed, because a
    /// capture is a pure function of the parameters, which both sides can see — and the engage
    /// travels with it so a kit that lands after that engage has ended is known for what it is:
    /// the patch as it was.
    ///
    /// **The rate travels with the request** because only `process` knows it. A kit rendered at
    /// the wrong rate plays back at the wrong speed, and the renderer has no other way to learn
    /// what the host activated at (a constant here once meant a 96 kHz session played its kit at
    /// half speed). Zero is the sentinel rather than a second flag: `activate` refuses any rate
    /// below a drum plugin's floor, so zero can never be a real one.
    ///
    /// Overwriting rather than queueing is the point. A host sweeping a captured axis asks every
    /// block, and coalescing those into "the most recent rate" is what stops the renderer being
    /// handed work faster than it can retire it — the shape that crashed hosts when these
    /// requests were nice-plug background tasks.
    capture_wanted: AtomicU64,
    /// The engage whose kit plays, plus one, or zero for none (`set_frozen`): what Export asks for
    /// the patches that kit was captured from (`capture::Gate::heard_for`).
    frozen: AtomicU64,
    /// The capture thread could not be started, so Resample can never freeze and Export never
    /// runs: the editor says so (`set_capture_unavailable`).
    capture_unavailable: AtomicBool,
}

impl Default for Signals {
    fn default() -> Self {
        Self::new()
    }
}

impl Signals {
    /// Nothing asked, nothing frozen.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            audition: AtomicU32::new(0),
            export: Mutex::new(None),
            capture_wanted: AtomicU64::new(0),
            frozen: AtomicU64::new(0),
            capture_unavailable: AtomicBool::new(false),
        }
    }

    /// Reports how the last sample-pack export went, for the editor to show. Not an audio-thread
    /// channel: the capture thread writes it and an editor frame reads it, so it can afford a lock.
    pub fn publish_export(&self, outcome: Export) {
        *self
            .export
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(outcome);
    }

    /// Takes the last export outcome, if one has not been shown yet.
    pub fn take_export(&self) -> Option<Export> {
        self.export
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// Asks for a fresh capture at the rate the engine runs at. **Audio thread**: one relaxed store.
    /// Asking twice before it is read is asking once, the later rate winning.
    pub fn request_capture(&self, sample_rate: f32, engagement: u32) {
        self.capture_wanted.store(
            u64::from(engagement) << 32 | u64::from(sample_rate.to_bits()),
            Ordering::Relaxed,
        );
    }

    /// Whether a capture request waits, without taking it: the worker's test for a render a newer
    /// request has overtaken.
    pub fn capture_requested(&self) -> bool {
        self.capture_wanted.load(Ordering::Relaxed) != 0
    }

    /// Takes a pending capture request, if there is one.
    pub fn take_capture_request(&self) -> Option<CaptureRequest> {
        match self.capture_wanted.swap(0, Ordering::Relaxed) {
            0 => None,
            word => Some(CaptureRequest {
                sample_rate: f32::from_bits(word as u32),
                engagement: (word >> 32) as u32,
            }),
        }
    }

    /// Says which engage's kit plays, or that none does. **Audio thread** (an install or a
    /// disengage) and `activate`: one relaxed store.
    pub fn set_frozen(&self, engagement: Option<u32>) {
        self.frozen.store(
            engagement.map_or(0, |e| u64::from(e) + 1),
            Ordering::Relaxed,
        );
    }

    /// The engage whose kit plays, if one does.
    #[must_use]
    pub fn frozen(&self) -> Option<u32> {
        match self.frozen.load(Ordering::Relaxed) {
            0 => None,
            word => Some((word - 1) as u32),
        }
    }

    /// Says whether the capture thread failed to start. **`activate`.**
    pub fn set_capture_unavailable(&self, unavailable: bool) {
        self.capture_unavailable
            .store(unavailable, Ordering::Relaxed);
    }

    /// Whether the capture thread failed to start.
    #[must_use]
    pub fn capture_unavailable(&self) -> bool {
        self.capture_unavailable.load(Ordering::Relaxed)
    }

    /// Asks for `slot` to be struck, as a test of its sound (the editor's model name, clicked or
    /// OPEN on it); `process` strikes it at its next buffer's first sample.
    pub fn request_audition(&self, slot: usize) {
        if slot < SLOT_COUNT {
            self.audition.fetch_or(1 << slot, Ordering::Relaxed);
        }
    }

    /// The slots asked for since the last call, a bit a slot, each taken once.
    #[must_use]
    pub fn take_audition(&self) -> u32 {
        self.audition.swap(0, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capture_request_carries_the_rate_and_the_engage_it_was_made_at() {
        let signals = Signals::new();
        assert_eq!(signals.take_capture_request(), None);
        signals.request_capture(96_000.0, 7);
        assert!(signals.capture_requested());
        assert_eq!(
            signals.take_capture_request(),
            Some(CaptureRequest {
                sample_rate: 96_000.0,
                engagement: 7
            })
        );
        assert_eq!(signals.take_capture_request(), None, "taken once");
        signals.request_capture(44_100.0, 1);
        signals.request_capture(192_000.0, 2);
        assert_eq!(
            signals.take_capture_request(),
            Some(CaptureRequest {
                sample_rate: 192_000.0,
                engagement: 2
            }),
            "asking twice asks once, the later winning"
        );
    }

    #[test]
    fn the_engage_that_plays_is_told_and_none_is_none() {
        let signals = Signals::new();
        assert_eq!(signals.frozen(), None);
        signals.set_frozen(Some(0));
        assert_eq!(signals.frozen(), Some(0), "engage zero is an engage");
        signals.set_frozen(Some(u32::MAX));
        assert_eq!(signals.frozen(), Some(u32::MAX));
        signals.set_frozen(None);
        assert_eq!(signals.frozen(), None);
    }

    #[test]
    fn a_slot_asked_to_be_heard_is_taken_once_and_out_of_range_is_ignored() {
        let signals = Signals::new();
        signals.request_audition(0);
        signals.request_audition(15);
        signals.request_audition(SLOT_COUNT);
        assert_eq!(signals.take_audition(), 1 | 1 << 15);
        assert_eq!(signals.take_audition(), 0);
    }

    #[test]
    fn an_export_outcome_waits_for_one_frame() {
        let signals = Signals::new();
        assert_eq!(signals.take_export(), None);
        signals.publish_export(Export::Written {
            files: 3,
            clipped: 0,
        });
        assert_eq!(
            signals.take_export(),
            Some(Export::Written {
                files: 3,
                clipped: 0
            })
        );
        assert_eq!(signals.take_export(), None);
    }
}
