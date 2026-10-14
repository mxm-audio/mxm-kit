//! Note ownership across parts: the events of one sample offset, collected, and the per-part state
//! they resolve against.
//!
//! The mechanism the drum machines both carried (the architecture audit's O4). Every musical choice
//! stays the consumer's, passed in through [`PartPolicy`] and [`ResolveFlags`]: which notes a part
//! admits, which candidate owns it, how strikes combine and which one sounds, where a new owner sits in
//! pitch, and what a flood of events does. Event translation (from CLAP or any host) and the
//! containment of values (velocity, tuning and bend ranges) happen before anything reaches here.

use crate::{ArbitrationResult, MonophonicArbitrator, NoteAddress, NoteCandidate, NoteOwner};

/// A release, choke or tuning addressed to a note, collected at one sample offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PartEvent {
    Release(NoteAddress),
    Choke(NoteAddress),
    Tuning {
        address: NoteAddress,
        semitones: f32,
    },
}

/// One sample offset's events, collected before any part resolves them, so the host's order of
/// events at one offset cannot change what sounds.
///
/// `CAPACITY` bounds the note-ons and, separately, the stored part events; `CHANNELS` the channel
/// expression. Past either bound the group reports it and decides nothing: [`note_on`](Self::note_on)
/// returns `false` and inserts nothing, and a part event past capacity sets
/// [`overflowed`](Self::overflowed). The consumer chooses the response.
#[derive(Debug, Clone, Copy)]
pub struct EventGroup<const CAPACITY: usize, const CHANNELS: usize> {
    note_ons: MonophonicArbitrator<CAPACITY>,
    events: [Option<PartEvent>; CAPACITY],
    stored: usize,
    overflowed: bool,
    any_choke: bool,
    panic: bool,
    all_notes_off: bool,
    bend: [Option<f32>; CHANNELS],
    wheel: [Option<f32>; CHANNELS],
    pressure: [Option<f32>; CHANNELS],
}

impl<const CAPACITY: usize, const CHANNELS: usize> Default for EventGroup<CAPACITY, CHANNELS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAPACITY: usize, const CHANNELS: usize> EventGroup<CAPACITY, CHANNELS> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            note_ons: MonophonicArbitrator::new(),
            events: [None; CAPACITY],
            stored: 0,
            overflowed: false,
            any_choke: false,
            panic: false,
            all_notes_off: false,
            bend: [None; CHANNELS],
            wheel: [None; CHANNELS],
            pressure: [None; CHANNELS],
        }
    }

    /// Adds a note-on. Returns `false`, inserting nothing, when the group already holds `CAPACITY`.
    pub fn note_on(&mut self, candidate: NoteCandidate) -> bool {
        self.note_ons.push(candidate)
    }

    pub fn release(&mut self, address: NoteAddress) {
        self.store(PartEvent::Release(address));
    }

    pub fn choke(&mut self, address: NoteAddress) {
        self.store(PartEvent::Choke(address));
    }

    pub fn tune(&mut self, address: NoteAddress, semitones: f32) {
        self.store(PartEvent::Tuning { address, semitones });
    }

    /// The latest pitch bend on `channel` (taken modulo `CHANNELS`), as the consumer scaled it.
    pub fn bend(&mut self, channel: usize, value: f32) {
        self.bend[channel % CHANNELS] = Some(value);
    }

    /// The latest modulation wheel on `channel` (modulo `CHANNELS`).
    pub fn wheel(&mut self, channel: usize, value: f32) {
        self.wheel[channel % CHANNELS] = Some(value);
    }

    /// The latest channel pressure on `channel` (modulo `CHANNELS`).
    pub fn pressure(&mut self, channel: usize, value: f32) {
        self.pressure[channel % CHANNELS] = Some(value);
    }

    pub fn panic(&mut self) {
        self.panic = true;
    }

    pub fn all_notes_off(&mut self) {
        self.all_notes_off = true;
    }

    #[must_use]
    pub const fn panicked(&self) -> bool {
        self.panic
    }

    #[must_use]
    pub const fn all_notes_off_requested(&self) -> bool {
        self.all_notes_off
    }

    /// More releases, chokes and tunings arrived than the group stores.
    #[must_use]
    pub const fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Any choke arrived, stored or not.
    #[must_use]
    pub const fn any_choke(&self) -> bool {
        self.any_choke
    }

    /// The stored releases, chokes and tunings, in arrival order.
    pub fn events(&self) -> impl Iterator<Item = PartEvent> + '_ {
        self.events[..self.stored].iter().flatten().copied()
    }

    fn store(&mut self, event: PartEvent) {
        self.any_choke |= matches!(event, PartEvent::Choke(_));
        if self.stored < CAPACITY {
            self.events[self.stored] = Some(event);
            self.stored += 1;
        } else {
            self.overflowed = true;
        }
    }
}

/// The consumer's musical choices, which [`PartState::resolve`] applies and never makes.
pub trait PartPolicy {
    /// Whether `part` admits a new strike from `owner` (its assignment). A part's current owner is
    /// matched on its own terms, not through this.
    fn accepts(&self, part: usize, owner: NoteOwner) -> bool;
    /// Whether `candidate` takes ownership from `current`.
    fn outranks(&self, candidate: NoteCandidate, current: NoteCandidate) -> bool;
    /// Combines every accepted strike, so host order cannot change the result.
    fn reduce(&self, accumulated: f32, next: f32) -> f32;
    /// The strike that sounds: the reduced one or the winner's own.
    fn sounding(&self, result: &ArbitrationResult) -> f32;
    /// Where a new owner of `part` sits in pitch, in semitones.
    fn chromatic_pitch(&self, part: usize, owner: NoteOwner) -> f32;
}

/// The consumer's response to a flood, for one resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveFlags {
    /// Every part gives up expression (an all-notes-off, or a flood whose releases were not all
    /// stored).
    pub release_all: bool,
    /// Stored tunings apply. False where a flood dropped some, so host order cannot pick which.
    pub apply_tunings: bool,
}

/// What one resolution asks of the parts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartResolution<const PARTS: usize> {
    /// Each part's new strike, if it took one.
    pub strikes: [Option<f32>; PARTS],
    /// Each part whose owner was choked.
    pub chokes: [bool; PARTS],
}

/// Who owns each part, whether it still owns expression, and the expression it reads.
#[derive(Debug, Clone, Copy)]
pub struct PartState<const PARTS: usize, const CHANNELS: usize> {
    bend: [f32; CHANNELS],
    wheel: [f32; CHANNELS],
    pressure: [f32; CHANNELS],
    channel: [u8; PARTS],
    tuning: [f32; PARTS],
    chromatic_pitch: [f32; PARTS],
    owner: [Option<NoteOwner>; PARTS],
    owns_expression: [bool; PARTS],
}

impl<const PARTS: usize, const CHANNELS: usize> Default for PartState<PARTS, CHANNELS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const PARTS: usize, const CHANNELS: usize> PartState<PARTS, CHANNELS> {
    /// Nothing owned, every value at rest, every part on channel 0.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bend: [0.0; CHANNELS],
            wheel: [0.0; CHANNELS],
            pressure: [0.0; CHANNELS],
            channel: [0; PARTS],
            tuning: [0.0; PARTS],
            chromatic_pitch: [0.0; PARTS],
            owner: [None; PARTS],
            owns_expression: [false; PARTS],
        }
    }

    /// Ends every ownership and returns all expression to rest. Each part's channel is kept: it is
    /// where its last owner played, which a ringing tail still reads.
    pub fn panic(&mut self) {
        self.bend = [0.0; CHANNELS];
        self.wheel = [0.0; CHANNELS];
        self.pressure = [0.0; CHANNELS];
        self.tuning = [0.0; PARTS];
        self.chromatic_pitch = [0.0; PARTS];
        self.owner = [None; PARTS];
        self.owns_expression = [false; PARTS];
    }

    #[must_use]
    pub const fn bend(&self, channel: usize) -> f32 {
        self.bend[channel % CHANNELS]
    }

    #[must_use]
    pub const fn wheel(&self, channel: usize) -> f32 {
        self.wheel[channel % CHANNELS]
    }

    #[must_use]
    pub const fn pressure(&self, channel: usize) -> f32 {
        self.pressure[channel % CHANNELS]
    }

    /// The channel `part`'s current or last owner played on.
    #[must_use]
    pub const fn channel(&self, part: usize) -> u8 {
        self.channel[part]
    }

    #[must_use]
    pub const fn tuning(&self, part: usize) -> f32 {
        self.tuning[part]
    }

    #[must_use]
    pub const fn chromatic_pitch(&self, part: usize) -> f32 {
        self.chromatic_pitch[part]
    }

    #[must_use]
    pub const fn owner(&self, part: usize) -> Option<NoteOwner> {
        self.owner[part]
    }

    #[must_use]
    pub const fn owns_expression(&self, part: usize) -> bool {
        self.owns_expression[part]
    }

    /// Resolves one offset's group against every part.
    ///
    /// Channel expression takes each channel's latest value. A same-offset choke removes the
    /// note-ons it matches before any part arbitrates. Each owned part then reduces its own events,
    /// matched on the owner's own terms so a reassigned part can still reach its ringing note: a
    /// choke ends ownership and tuning; otherwise a tuning applies while the part owns expression
    /// and a release ends expression — reduced, not replayed, so host order cannot matter. Last, each
    /// part arbitrates the note-ons it admits; a winner takes ownership and expression, its channel
    /// and chromatic pitch, and the group's last matching tuning.
    pub fn resolve<const CAPACITY: usize>(
        &mut self,
        group: &EventGroup<CAPACITY, CHANNELS>,
        policy: &impl PartPolicy,
        flags: ResolveFlags,
    ) -> PartResolution<PARTS> {
        for channel in 0..CHANNELS {
            if let Some(value) = group.bend[channel] {
                self.bend[channel] = value;
            }
            if let Some(value) = group.wheel[channel] {
                self.wheel[channel] = value;
            }
            if let Some(value) = group.pressure[channel] {
                self.pressure[channel] = value;
            }
        }

        let mut strikes = group.note_ons;
        if group.any_choke {
            strikes.retain(|owner| {
                !group.events().any(
                    |event| matches!(event, PartEvent::Choke(address) if address.matches(owner)),
                )
            });
        }

        let mut resolution = PartResolution {
            strikes: [None; PARTS],
            chokes: [false; PARTS],
        };
        for part in 0..PARTS {
            if let Some(owner) = self.owner[part] {
                let (mut choked, mut released, mut tuning) = (false, false, None);
                for event in group.events() {
                    match event {
                        PartEvent::Choke(address) if address.matches(owner) => choked = true,
                        PartEvent::Release(address) if address.matches(owner) => released = true,
                        PartEvent::Tuning { address, semitones }
                            if flags.apply_tunings && address.matches(owner) =>
                        {
                            tuning = Some(semitones);
                        }
                        _ => {}
                    }
                }
                if choked {
                    resolution.chokes[part] = true;
                    self.owner[part] = None;
                    self.owns_expression[part] = false;
                    self.tuning[part] = 0.0;
                } else {
                    if let Some(semitones) = tuning.filter(|_| self.owns_expression[part]) {
                        self.tuning[part] = semitones;
                    }
                    if released {
                        self.owns_expression[part] = false;
                    }
                }
            }
            if flags.release_all {
                self.owns_expression[part] = false;
            }

            let winner = strikes.resolve(
                |owner| policy.accepts(part, owner),
                |candidate, current| policy.outranks(candidate, current),
                |accumulated, next| policy.reduce(accumulated, next),
            );
            if let Some(winner) = winner {
                let owner = winner.owner;
                self.owner[part] = Some(owner);
                self.owns_expression[part] = true;
                self.channel[part] = owner.channel;
                self.chromatic_pitch[part] = policy.chromatic_pitch(part, owner);
                self.tuning[part] = 0.0;
                if flags.apply_tunings {
                    for event in group.events() {
                        if let PartEvent::Tuning { address, semitones } = event {
                            if address.matches(owner) {
                                self.tuning[part] = semitones;
                            }
                        }
                    }
                }
                resolution.strikes[part] = Some(policy.sounding(&winner));
            }
        }
        resolution
    }
}
