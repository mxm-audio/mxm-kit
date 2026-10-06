//! Framework-free primitives for multi-part note and output routing.
//!
//! Consumers own parameter identities, audio-port layouts and musical policy. This crate owns the
//! bounded assignment, matching, arbitration and destination-transfer mechanisms those policies use.

/// The host-facing destination of one independently routed part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Main,
    Auxiliary(usize),
}

impl Destination {
    /// Converts a `0 = Main, 1.. = Auxiliary` selector while containing hostile values.
    #[must_use]
    pub fn from_parameter(value: i32, auxiliary_count: usize) -> Self {
        if value <= 0 || auxiliary_count == 0 {
            Self::Main
        } else {
            Self::Auxiliary((value as usize - 1).min(auxiliary_count - 1))
        }
    }

    /// Contains a directly constructed auxiliary index to the consumer's available destinations.
    #[must_use]
    pub fn clamped(self, auxiliary_count: usize) -> Self {
        match self {
            Self::Auxiliary(index) if auxiliary_count > 0 => {
                Self::Auxiliary(index.min(auxiliary_count - 1))
            }
            Self::Auxiliary(_) | Self::Main => Self::Main,
        }
    }

    /// Converts back to the common `0 = Main, 1.. = Auxiliary` selector domain.
    #[must_use]
    pub const fn parameter_value(self) -> usize {
        match self {
            Self::Main => 0,
            Self::Auxiliary(index) => index.saturating_add(1),
        }
    }
}

/// Policy-neutral note assignment for one part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartAssignment {
    FixedNote,
    Channel(u8),
}

impl PartAssignment {
    /// Converts a `0 = FixedNote, 1..16 = Channel` selector and clamps hostile values.
    #[must_use]
    pub fn from_parameter(value: i32) -> Self {
        if value <= 0 {
            Self::FixedNote
        } else {
            Self::Channel((value - 1).min(15) as u8)
        }
    }

    #[must_use]
    pub const fn channel(self) -> Option<u8> {
        match self {
            Self::FixedNote => None,
            Self::Channel(channel) => Some(if channel > 15 { 15 } else { channel }),
        }
    }
}

/// What fixed-note parts do when another part claims an event's channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedChannelCollision {
    Layer,
    ExcludeClaimed,
}

/// Returns the claimed MIDI-channel bitset for a consumer-sized assignment slice.
#[must_use]
pub fn claimed_channels(assignments: &[PartAssignment]) -> u16 {
    assignments.iter().fold(0, |mask, assignment| {
        assignment
            .channel()
            .map_or(mask, |channel| mask | (1_u16 << channel))
    })
}

/// Matches one note owner against one fixed-note or chromatic part.
#[must_use]
pub fn assignment_matches(
    assignment: PartAssignment,
    fixed_note: u8,
    owner: NoteOwner,
    claimed: u16,
    collision: FixedChannelCollision,
) -> bool {
    match assignment {
        PartAssignment::FixedNote => {
            owner.key == fixed_note
                && (collision == FixedChannelCollision::Layer
                    || claimed & (1_u16 << owner.channel.min(15)) == 0)
        }
        PartAssignment::Channel(channel) => owner.channel == channel.min(15),
    }
}

/// The note that owns a currently sounding part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteOwner {
    pub channel: u8,
    pub key: u8,
    /// `None` is CLAP's `note_id == -1`: key-addressed rather than ID-addressed.
    pub note_id: Option<i32>,
}

/// A release/choke address. `None` is CLAP's `-1` wildcard for that field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteAddress {
    pub channel: Option<u8>,
    pub key: Option<u8>,
    pub note_id: Option<i32>,
}

impl NoteAddress {
    /// CLAP note-address matching: channel first, then an explicit note ID or wildcard-ID key.
    #[must_use]
    pub fn matches(self, owner: NoteOwner) -> bool {
        if self.channel.is_some_and(|channel| channel != owner.channel) {
            return false;
        }
        match self.note_id {
            Some(note_id) => owner.note_id == Some(note_id),
            None => self.key.is_none_or(|key| key == owner.key),
        }
    }
}

/// One candidate in a bounded same-offset monophonic arbitration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteCandidate {
    pub owner: NoteOwner,
    pub strike: f32,
}

/// The winning owner and policy-reduced strike.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArbitrationResult {
    pub owner: NoteOwner,
    /// Every accepted candidate's strike, combined by the caller's `reduce_strike`.
    pub strike: f32,
    /// The winning candidate's own strike, unreduced. A caller whose instrument says a hit is its
    /// own velocity takes this instead: one note owns the hit, so the hit is that note's.
    pub owner_strike: f32,
}

/// Fixed-capacity, allocation-free same-offset note arbitration.
#[derive(Debug, Clone, Copy)]
pub struct MonophonicArbitrator<const CAPACITY: usize> {
    candidates: [Option<NoteCandidate>; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> Default for MonophonicArbitrator<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAPACITY: usize> MonophonicArbitrator<CAPACITY> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            candidates: [None; CAPACITY],
            len: 0,
        }
    }

    /// Adds one candidate. Returns `false` without partial mutation when full.
    pub fn push(&mut self, candidate: NoteCandidate) -> bool {
        if self.len == CAPACITY {
            return false;
        }
        self.candidates[self.len] = Some(candidate);
        self.len += 1;
        true
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Keeps only the candidates `keep` accepts, in their original order.
    ///
    /// For removals that depend on the note alone, such as a same-offset choke cancelling it: run
    /// once per group, not once per part inside `resolve`, so the cost stays linear in parts.
    pub fn retain(&mut self, keep: impl Fn(NoteOwner) -> bool) {
        let mut kept = 0;
        for index in 0..self.len {
            let Some(candidate) = self.candidates[index] else {
                continue;
            };
            if keep(candidate.owner) {
                self.candidates[kept] = Some(candidate);
                kept += 1;
            }
        }
        for slot in &mut self.candidates[kept..self.len] {
            *slot = None;
        }
        self.len = kept;
    }

    /// Resolves the candidates one part accepts under consumer-supplied musical laws.
    ///
    /// `outranks(candidate, current)` chooses the owner. `reduce_strike(accumulated, next)`
    /// combines every accepted strike independently of ownership.
    #[must_use]
    pub fn resolve(
        &self,
        accepts: impl Fn(NoteOwner) -> bool,
        outranks: impl Fn(NoteCandidate, NoteCandidate) -> bool,
        reduce_strike: impl Fn(f32, f32) -> f32,
    ) -> Option<ArbitrationResult> {
        let mut winner: Option<NoteCandidate> = None;
        let mut strike = 0.0;
        for candidate in self.candidates[..self.len].iter().flatten().copied() {
            if !accepts(candidate.owner) {
                continue;
            }
            strike = if winner.is_some() {
                reduce_strike(strike, candidate.strike)
            } else {
                candidate.strike
            };
            if winner.is_none_or(|current| outranks(candidate, current)) {
                winner = Some(candidate);
            }
        }
        winner.map(|winner| ArbitrationResult {
            owner: winner.owner,
            strike,
            owner_strike: winner.strike,
        })
    }
}

/// One non-zero destination gain. A transition exposes at most two of these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DestinationGain {
    pub destination: Destination,
    pub gain: f32,
}

/// Allocation-free equal-gain partition between destinations.
///
/// A request for the origin while moving reverses in place. A request for any third destination is
/// latched latest-wins and starts after the current transfer. Therefore no part ever reaches more
/// than two destinations in one sample.
#[derive(Debug, Clone, Copy)]
pub struct DestinationRouter {
    from: Destination,
    to: Destination,
    step: u32,
    steps: u32,
    moving: bool,
    pending: Option<Destination>,
}

impl DestinationRouter {
    #[must_use]
    pub const fn new(destination: Destination, transition_samples: u32) -> Self {
        Self {
            from: destination,
            to: destination,
            step: 0,
            steps: if transition_samples < 2 {
                2
            } else {
                transition_samples
            },
            moving: false,
            pending: None,
        }
    }

    pub fn set_transition_samples(&mut self, samples: u32) {
        self.steps = samples.max(2);
        self.step = self.step.min(self.steps - 1);
    }

    #[must_use]
    pub const fn destination(&self) -> Destination {
        if self.moving { self.to } else { self.from }
    }

    pub fn reset(&mut self, destination: Destination) {
        self.from = destination;
        self.to = destination;
        self.step = 0;
        self.moving = false;
        self.pending = None;
    }

    pub fn request(&mut self, destination: Destination) {
        if !self.moving {
            if destination != self.from {
                self.to = destination;
                self.step = 0;
                self.moving = true;
            }
            return;
        }

        if destination == self.to {
            self.pending = None;
        } else if destination == self.from {
            core::mem::swap(&mut self.from, &mut self.to);
            // `step` names the sample that would be rendered next. Reflect the last rendered
            // partition, not that future sample, so reversal begins continuously.
            self.step = self.steps.saturating_sub(self.step).min(self.steps - 1);
            self.pending = None;
        } else {
            self.pending = Some(destination);
        }
    }

    /// Returns this sample's partition and advances the transfer by one sample.
    #[must_use]
    pub fn next_partition(&mut self) -> [Option<DestinationGain>; 2] {
        if !self.moving {
            return [
                Some(DestinationGain {
                    destination: self.from,
                    gain: 1.0,
                }),
                None,
            ];
        }

        let phase = self.step as f32 / (self.steps - 1) as f32;
        let gains = [
            Some(DestinationGain {
                destination: self.from,
                gain: 1.0 - phase,
            }),
            Some(DestinationGain {
                destination: self.to,
                gain: phase,
            }),
        ];

        if self.step + 1 >= self.steps {
            self.from = self.to;
            self.moving = false;
            self.step = 0;
            if let Some(pending) = self.pending.take() {
                if pending != self.from {
                    self.to = pending;
                    self.moving = true;
                }
            }
        } else {
            self.step += 1;
        }
        gains
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_assignment_values_are_contained() {
        assert_eq!(Destination::from_parameter(i32::MIN, 16), Destination::Main);
        assert_eq!(
            Destination::from_parameter(i32::MAX, 3),
            Destination::Auxiliary(2)
        );
        assert_eq!(
            Destination::Auxiliary(usize::MAX).clamped(2),
            Destination::Auxiliary(1)
        );
        assert_eq!(
            PartAssignment::from_parameter(i32::MIN),
            PartAssignment::FixedNote
        );
        assert_eq!(
            PartAssignment::from_parameter(i32::MAX),
            PartAssignment::Channel(15)
        );
    }

    #[test]
    fn note_addresses_follow_channel_id_and_wildcard_key_rules() {
        let owner = NoteOwner {
            channel: 3,
            key: 64,
            note_id: Some(91),
        };
        assert!(
            NoteAddress {
                channel: None,
                key: None,
                note_id: None
            }
            .matches(owner)
        );
        assert!(
            NoteAddress {
                channel: Some(3),
                key: Some(64),
                note_id: None
            }
            .matches(owner)
        );
        assert!(
            NoteAddress {
                channel: Some(3),
                key: Some(12),
                note_id: Some(91)
            }
            .matches(owner)
        );
        assert!(
            !NoteAddress {
                channel: Some(2),
                key: None,
                note_id: None
            }
            .matches(owner)
        );
        assert!(
            !NoteAddress {
                channel: Some(3),
                key: Some(64),
                note_id: Some(92)
            }
            .matches(owner)
        );
    }

    #[test]
    fn retain_removes_in_place_and_keeps_the_survivors_order() {
        let candidate = |key: u8| NoteCandidate {
            owner: NoteOwner {
                channel: 0,
                key,
                note_id: None,
            },
            strike: 0.5,
        };
        let mut notes = MonophonicArbitrator::<4>::new();
        for key in [10, 11, 12, 13] {
            assert!(notes.push(candidate(key)));
        }
        notes.retain(|owner| owner.key % 2 == 1);
        assert_eq!(notes.len(), 2);
        let first = notes
            .resolve(|_| true, |_, _| false, |strike, _| strike)
            .unwrap();
        assert_eq!(first.owner.key, 11, "survivors lost their order");
        // The freed capacity is usable again.
        assert!(notes.push(candidate(20)) && notes.push(candidate(21)));
        assert!(!notes.push(candidate(22)));
    }

    #[test]
    fn transfer_is_an_equal_gain_partition_and_never_names_a_third_destination() {
        let mut router = DestinationRouter::new(Destination::Main, 9);
        router.request(Destination::Auxiliary(0));
        for _ in 0..9 {
            let [from, to] = router.next_partition();
            let from = from.unwrap();
            let to = to.unwrap();
            assert_eq!(
                (from.destination, to.destination),
                (Destination::Main, Destination::Auxiliary(0))
            );
            assert!((from.gain + to.gain - 1.0).abs() < 1.0e-6);
        }
        assert_eq!(
            router.next_partition()[0].unwrap().destination,
            Destination::Auxiliary(0)
        );
    }

    #[test]
    fn reversal_is_continuous_and_a_third_request_is_latest_wins() {
        let mut router = DestinationRouter::new(Destination::Main, 9);
        router.request(Destination::Auxiliary(0));
        for _ in 0..4 {
            let _ = router.next_partition();
        }
        let before = router.next_partition();
        router.request(Destination::Main);
        let after = router.next_partition();
        assert_eq!(
            before[0].unwrap().destination,
            after[1].unwrap().destination
        );
        assert!((before[0].unwrap().gain - after[1].unwrap().gain).abs() < 1.0e-6);

        router.request(Destination::Auxiliary(1));
        router.request(Destination::Auxiliary(2));
        for _ in 0..20 {
            let _ = router.next_partition();
        }
        assert_eq!(router.destination(), Destination::Auxiliary(2));
    }

    #[test]
    fn reset_and_same_destination_are_exactly_idle() {
        let mut router = DestinationRouter::new(Destination::Auxiliary(3), 0);
        router.request(Destination::Auxiliary(3));
        assert_eq!(
            router.next_partition(),
            [
                Some(DestinationGain {
                    destination: Destination::Auxiliary(3),
                    gain: 1.0,
                }),
                None
            ]
        );
        router.reset(Destination::Main);
        assert_eq!(router.destination(), Destination::Main);
    }
}
