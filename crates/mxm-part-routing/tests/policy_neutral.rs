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
