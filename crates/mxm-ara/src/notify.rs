//! Model update notifications: queued from any thread but the audio thread, sent to the host
//! only when it polls (`notifyModelUpdates`), which is the one place ARA allows them.

use std::sync::{Arc, Mutex, PoisonError};

use crate::content::{ContentUpdateFlags, TimeRange};
use crate::graph::{AudioModificationId, AudioSourceId, PlaybackRegionId};
use crate::sys;

/// An analysis' progress, as the host shows it (`ARAAnalysisProgressState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisProgress {
    /// Required first.
    Started,
    Updated,
    /// Required last, whether it completed or was cancelled.
    Completed,
}

impl AnalysisProgress {
    pub(crate) fn raw(self) -> sys::ARAAnalysisProgressState {
        match self {
            AnalysisProgress::Started => sys::kARAAnalysisProgressStarted,
            AnalysisProgress::Updated => sys::kARAAnalysisProgressUpdated,
            AnalysisProgress::Completed => sys::kARAAnalysisProgressCompleted,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Notification {
    Progress {
        source: AudioSourceId,
        state: AnalysisProgress,
        value: f32,
    },
    SourceContent {
        source: AudioSourceId,
        range: Option<TimeRange>,
        flags: ContentUpdateFlags,
    },
    ModificationContent {
        modification: AudioModificationId,
        range: Option<TimeRange>,
        flags: ContentUpdateFlags,
    },
    RegionContent {
        region: PlaybackRegionId,
        range: Option<TimeRange>,
        flags: ContentUpdateFlags,
    },
    DocumentData,
}

/// The queue of a document's model update notifications. Cheap to clone into workers; not for
/// the audio thread (it locks). Notifications about objects destroyed before the host polls are
/// dropped.
#[derive(Clone, Default)]
pub struct Notifier {
    queue: Arc<Mutex<Vec<Notification>>>,
}

impl Notifier {
    fn push(&self, n: Notification) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        // An update replaces the previous update of the same source still waiting.
        if let Notification::Progress {
            source,
            state: AnalysisProgress::Updated,
            value,
        } = n
            && let Some(Notification::Progress {
                source: s,
                state: AnalysisProgress::Updated,
                value: v,
            }) = queue.last_mut()
            && *s == source
        {
            *v = value;
            return;
        }
        queue.push(n);
    }

    /// An analysis of the source began (`kARAAnalysisProgressStarted`).
    pub fn analysis_started(&self, source: AudioSourceId) {
        self.push(Notification::Progress {
            source,
            state: AnalysisProgress::Started,
            value: 0.0,
        });
    }

    /// The analysis' progress, 0 to 1.
    pub fn analysis_progress(&self, source: AudioSourceId, value: f32) {
        self.push(Notification::Progress {
            source,
            state: AnalysisProgress::Updated,
            value: value.clamp(0.0, 1.0),
        });
    }

    /// The analysis ended, completed or cancelled (`kARAAnalysisProgressCompleted`).
    pub fn analysis_completed(&self, source: AudioSourceId) {
        self.push(Notification::Progress {
            source,
            state: AnalysisProgress::Completed,
            value: 1.0,
        });
    }

    /// The source's content (its analysis, say) changed; `range` `None` is all of it. Not for a
    /// change the host made itself.
    pub fn audio_source_content_changed(
        &self,
        source: AudioSourceId,
        range: Option<TimeRange>,
        flags: ContentUpdateFlags,
    ) {
        self.push(Notification::SourceContent {
            source,
            range,
            flags,
        });
    }

    /// The modification's content (its edits) changed.
    pub fn audio_modification_content_changed(
        &self,
        modification: AudioModificationId,
        range: Option<TimeRange>,
        flags: ContentUpdateFlags,
    ) {
        self.push(Notification::ModificationContent {
            modification,
            range,
            flags,
        });
    }

    /// A region's output changed (range in playback time). Dropped for a host before ARA 2.0.
    pub fn playback_region_content_changed(
        &self,
        region: PlaybackRegionId,
        range: Option<TimeRange>,
        flags: ContentUpdateFlags,
    ) {
        self.push(Notification::RegionContent {
            region,
            range,
            flags,
        });
    }

    /// The document's private data changed, so the host saves (ARA 2.3's
    /// `notifyDocumentDataChanged`). Dropped for a host before 2.3.
    pub fn document_data_changed(&self) {
        self.push(Notification::DocumentData);
    }

    /// Drops queued notifications about an object being destroyed, whose address a new object
    /// may take before the host polls.
    pub(crate) fn retain(&self, keep: impl FnMut(&Notification) -> bool) {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(keep);
    }

    pub(crate) fn drain(&self) -> Vec<Notification> {
        std::mem::take(&mut *self.queue.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// How many notifications wait for the host's next poll.
    pub fn pending(&self) -> usize {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}
