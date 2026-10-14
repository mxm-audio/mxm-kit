use mxm_part_routing::{
    Destination, DestinationRouter, FixedChannelCollision, MonophonicArbitrator, NoteCandidate,
    NoteOwner, PartAssignment, assignment_matches, claimed_channels,
};

// Deliberately unlike the drum machine: three parts, two auxiliaries, lowest-note priority,
// summed strike, and fixed/channel layering. This is a policy-neutral API fixture, not a product.
#[test]
fn different_sized_consumer_supplies_its_own_matching_arbitration_and_destination_policy() {
    let assignments = [
        PartAssignment::FixedNote,
        PartAssignment::Channel(4),
        PartAssignment::FixedNote,
    ];
    let fixed_notes = [24, 0, 72];
    let claimed = claimed_channels(&assignments);
    assert_eq!(claimed, 1 << 4);

    let mut notes = MonophonicArbitrator::<5>::new();
    for (key, strike) in [(72, 0.2), (60, 0.3), (48, 0.4)] {
        assert!(notes.push(NoteCandidate {
            owner: NoteOwner {
                channel: 4,
                key,
                note_id: Some(i32::from(key)),
            },
            strike,
        }));
    }

    let part = 1;
    let result = notes
        .resolve(
            |owner| {
                assignment_matches(
                    assignments[part],
                    fixed_notes[part],
                    owner,
                    claimed,
                    FixedChannelCollision::Layer,
                )
            },
            |candidate, current| candidate.owner.key < current.owner.key,
            |sum, next| sum + next,
        )
        .unwrap();
    assert_eq!(result.owner.key, 48);
    assert!((result.strike - 0.9).abs() < 1.0e-6);

    let mut route = DestinationRouter::new(Destination::Main, 4);
    route.request(Destination::from_parameter(2, 2));
    for _ in 0..4 {
        let active = route
            .next_partition()
            .into_iter()
            .flatten()
            .filter(|gain| gain.gain > 0.0)
            .count();
        assert!(active <= 2);
    }
    assert_eq!(route.destination(), Destination::Auxiliary(1));
}

#[test]
fn fixed_note_collision_policy_is_an_explicit_consumer_choice() {
    let owner = NoteOwner {
        channel: 4,
        key: 24,
        note_id: None,
    };
    let claimed = claimed_channels(&[PartAssignment::Channel(4)]);
    assert!(assignment_matches(
        PartAssignment::FixedNote,
        24,
        owner,
        claimed,
        FixedChannelCollision::Layer,
    ));
    assert!(!assignment_matches(
        PartAssignment::FixedNote,
        24,
        owner,
        claimed,
        FixedChannelCollision::ExcludeClaimed,
    ));
}

// The ownership mechanism under choices no drum machine makes: three parts on one channel each
// admitting any key, the lowest key owning, strikes summed and the sum sounding, pitch from key 60.
struct Lowest;

impl mxm_part_routing::PartPolicy for Lowest {
    fn accepts(&self, part: usize, owner: NoteOwner) -> bool {
        usize::from(owner.channel) == part
    }
    fn outranks(&self, candidate: NoteCandidate, current: NoteCandidate) -> bool {
        candidate.owner.key < current.owner.key
    }
    fn reduce(&self, accumulated: f32, next: f32) -> f32 {
        accumulated + next
    }
    fn sounding(&self, result: &mxm_part_routing::ArbitrationResult) -> f32 {
        result.strike
    }
    fn chromatic_pitch(&self, _part: usize, owner: NoteOwner) -> f32 {
        f32::from(owner.key) - 60.0
    }
}

fn note(channel: u8, key: u8) -> NoteOwner {
    NoteOwner {
        channel,
        key,
        note_id: None,
    }
}

fn address(channel: u8, key: u8) -> mxm_part_routing::NoteAddress {
    mxm_part_routing::NoteAddress {
        channel: Some(channel),
        key: Some(key),
        note_id: None,
    }
}

const PLAY: mxm_part_routing::ResolveFlags = mxm_part_routing::ResolveFlags {
    release_all: false,
    apply_tunings: true,
};

#[test]
fn ownership_follows_the_consumers_laws_and_the_owners_own_address() {
    use mxm_part_routing::{EventGroup, PartState};
    let mut state = PartState::<3, 3>::new();

    let mut group = EventGroup::<4, 3>::new();
    for (channel, key, strike) in [(1, 64, 0.25), (1, 62, 0.5), (2, 70, 0.75)] {
        assert!(group.note_on(NoteCandidate {
            owner: note(channel, key),
            strike,
        }));
    }
    group.tune(address(1, 62), 3.0);
    group.bend(4, 0.5); // channel 4 is channel 1 of three
    let resolved = state.resolve(&group, &Lowest, PLAY);
    assert_eq!(
        resolved.strikes,
        [None, Some(0.75), Some(0.75)],
        "summed, not the winner's"
    );
    assert_eq!(state.owner(1), Some(note(1, 62)), "the lowest key owns");
    assert_eq!(state.chromatic_pitch(1), 2.0);
    assert_eq!(state.tuning(1), 3.0, "the winner takes its group's tuning");
    assert_eq!(state.bend(1), 0.5);

    // A release ends expression but not ownership; a tuning then no longer reaches the part.
    let mut group = EventGroup::<4, 3>::new();
    group.release(address(1, 62));
    state.resolve(&group, &Lowest, PLAY);
    assert!(!state.owns_expression(1));
    let mut group = EventGroup::<4, 3>::new();
    group.tune(address(1, 62), -5.0);
    state.resolve(&group, &Lowest, PLAY);
    assert_eq!(state.tuning(1), 3.0);

    // A choke ends ownership, and cancels a same-offset note-on it matches before any part sees it.
    let mut group = EventGroup::<4, 3>::new();
    group.choke(address(2, 70));
    assert!(group.note_on(NoteCandidate {
        owner: note(2, 70),
        strike: 1.0,
    }));
    let resolved = state.resolve(&group, &Lowest, PLAY);
    assert_eq!(resolved.chokes, [false, false, true]);
    assert_eq!(resolved.strikes, [None; 3]);
    assert_eq!(state.owner(2), None);
}

#[test]
fn the_group_reports_its_bounds_and_the_flags_decide_the_flood() {
    use mxm_part_routing::{EventGroup, PartState, ResolveFlags};
    let mut group = EventGroup::<2, 3>::new();
    for key in [60, 61] {
        assert!(group.note_on(NoteCandidate {
            owner: note(0, key),
            strike: 0.5,
        }));
    }
    assert!(
        !group.note_on(NoteCandidate {
            owner: note(0, 62),
            strike: 0.5,
        }),
        "full: refused, nothing inserted"
    );
    assert!(
        !group.panicked(),
        "the consumer decides what a refusal means"
    );
    for _ in 0..3 {
        group.tune(address(0, 60), 7.0);
    }
    assert!(group.overflowed() && !group.any_choke());
    assert_eq!(group.events().count(), 2);

    let mut state = PartState::<3, 3>::new();
    let flood = ResolveFlags {
        release_all: true,
        apply_tunings: false,
    };
    state.resolve(&group, &Lowest, flood);
    assert_eq!(state.owner(0), Some(note(0, 60)));
    assert_eq!(state.tuning(0), 0.0, "no tuning applies in a flood");
    assert!(
        state.owns_expression(0),
        "a new owner takes expression after the release"
    );
    state.panic();
    assert_eq!(state.owner(0), None);
    assert_eq!(state.channel(0), 0);
}
