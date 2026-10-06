//! The standard's conformance checks, **for an instrument's tests** — behind the `conformance`
//! feature, which only `[dev-dependencies]` enable, so no shipped graph carries it.
//!
//! An instrument describes its routing as a [`Declaration`] and runs [`check_declaration`]; it
//! proves the same things for every instrument, which is what "every modulation means the same"
//! needs if it is to stay true after the next conversion:
//!
//! - a pair from a performance source that the machine did not have is offered exactly as
//!   [`standard::offer`] says, and refused pairs are not offered;
//! - **at a source's rest a route does nothing** — Velocity at full, Key at middle C, a gesture at
//!   rest — on both halves of the amount;
//! - an added pair delivers the standard reach ([`standard::reach`]), per octave for Key;
//! - every offered pair moves its target by a meaningful amount, so no row a player can add is dead.
//!
//! Two checks need a real voice: [`check_publishers`] — a voice publishes what the standard says
//! — and [`check_release_silence`] — after a release, no performance route holds a note open.

use crate::standard::{self, Law, Offer, Performance, Sign, reach};

/// What a target is, for the standard's law and reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Summed semitones.
    Pitch,
    /// Summed octaves on a cutoff.
    Cutoff,
    /// Summed octaves on a rate.
    Rate,
    /// [`standard::amplitude_factor`]. `deliver` reports the factor minus one.
    Amplitude,
    /// A summed pulse width, as a fraction of the cycle.
    Width,
    /// A pulse width that **only narrows** from square — `0.5 − Σ`, a negative sum discarded — as a
    /// fraction of the cycle: the standard's one-sided width. `deliver` reports the narrowing.
    NarrowingWidth,
    /// A control summed in its own 0…1 range: resonance, timbre, an axis.
    Control,
    /// A pan position, −1…+1.
    Pan,
    /// A machine's own law with no standard reach — a period, a speed, a machine amplifier, an
    /// edge, a sampler, an audio input.
    Machine(Law),
}

impl Kind {
    /// The law this kind of target applies, for [`standard::offer`].
    #[must_use]
    pub const fn law(self) -> Law {
        match self {
            Self::Amplitude => Law::Factor,
            Self::NarrowingWidth => Law::OneSided(Sign::Negative),
            Self::Machine(law) => law,
            _ => Law::Sum,
        }
    }

    /// What an added route delivers at an amount of one and its source at the extreme, in the
    /// target's unit; `None` for a machine law, which has no standard reach.
    #[must_use]
    pub const fn added_reach(self) -> Option<f32> {
        match self {
            Self::Pitch => Some(reach::PITCH_SEMITONES),
            Self::Cutoff | Self::Rate => Some(reach::OCTAVES),
            Self::Amplitude => Some(reach::AMPLITUDE),
            Self::Width | Self::NarrowingWidth => Some(reach::WIDTH),
            Self::Control | Self::Pan => Some(reach::CONTROL),
            Self::Machine(_) => None,
        }
    }

    /// What an added Key route delivers per octave of keyboard at an amount of one.
    #[must_use]
    pub const fn key_reach_per_octave(self) -> Option<f32> {
        match self {
            Self::Pitch => Some(reach::KEY_PITCH_SEMITONES_PER_OCTAVE),
            Self::Cutoff | Self::Rate => Some(reach::KEY_OCTAVES_PER_OCTAVE),
            Self::Amplitude => Some(reach::AMPLITUDE * reach::KEY_LINEAR_FRACTION_PER_OCTAVE),
            Self::Width | Self::NarrowingWidth => {
                Some(reach::WIDTH * reach::KEY_LINEAR_FRACTION_PER_OCTAVE)
            }
            Self::Control | Self::Pan => {
                Some(reach::CONTROL * reach::KEY_LINEAR_FRACTION_PER_OCTAVE)
            }
            Self::Machine(_) => None,
        }
    }

    /// The least a route at full depth may move this kind of target and still mean something.
    #[must_use]
    pub const fn meaningful(self) -> f32 {
        match self {
            Self::Pitch => 0.5,
            Self::Cutoff | Self::Rate => 0.25,
            Self::Amplitude | Self::Control | Self::Pan => 0.1,
            Self::Width | Self::NarrowingWidth => 0.05,
            Self::Machine(_) => 0.01,
        }
    }
}

/// An instrument's routing, as the checks see it.
pub trait Declaration {
    /// How many sources it declares.
    fn sources(&self) -> usize;
    /// How many targets it declares.
    fn targets(&self) -> usize;
    /// The performance source a source index is, or `None` for a generator or a machine source.
    fn performance(&self, source: usize) -> Option<Performance>;
    /// What a target is.
    fn kind(&self, target: usize) -> Kind;
    /// Whether the machine itself had this path — it keeps the machine's reach, and is always
    /// offered.
    fn machine(&self, target: usize, source: usize) -> bool;
    /// Whether and how the instrument offers this pair.
    fn offered(&self, target: usize, source: usize) -> Offer;
    /// The instrument's Key unit, in semitones per published unit.
    fn key_unit(&self) -> f32;
    /// What one route delivers, **in the target's unit** (Amplitude: the factor minus one), at
    /// `amount` with the source publishing the standard's raw value `raw` — through the
    /// instrument's own scale table and laws, not a copy of them.
    fn deliver(&self, target: usize, source: usize, amount: f32, raw: f32) -> f32;
    /// A pair's name, for a failure message.
    fn name(&self, target: usize, source: usize) -> String;
    /// Whether an **owner's ruling** refuses this pair, whatever the standard would offer —
    /// `mxm-mono-pr1`'s wheel into its own bus, which the multiplier already means. Such a pair must
    /// not be offered, and is otherwise exempt.
    fn ruled_out(&self, _target: usize, _source: usize) -> bool {
        false
    }
}

/// The raw value a performance source publishes at the extreme the reach is stated at: an octave
/// above middle C for Key, the softest note for Velocity, full travel for a gesture.
#[must_use]
pub fn extreme(source: Performance, key_unit: f32) -> f32 {
    match source {
        // One octave up from middle C: Key's reach is per octave.
        Performance::Key => 12.0 / key_unit,
        // The softest note.
        Performance::Velocity => -1.0,
        _ => 1.0,
    }
}

/// The amounts an offer lets a player set, as the two ends of the fader.
fn ends(offer: Offer) -> &'static [f32] {
    match offer {
        Offer::Refused => &[],
        Offer::Both => &[-1.0, 1.0],
        Offer::PositiveOnly => &[1.0],
        Offer::NegativeOnly => &[-1.0],
    }
}

/// Every check that needs no voice. Returns every failure, not just the first, so a conversion's
/// whole distance from the standard is visible at once.
///
/// # Errors
///
/// A list of failures, each naming its pair and what it broke.
pub fn check_declaration(d: &impl Declaration) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    for target in 0..d.targets() {
        let kind = d.kind(target);
        for source in 0..d.sources() {
            let name = d.name(target, source);
            let machine = d.machine(target, source);
            let offered = d.offered(target, source);
            if d.ruled_out(target, source) {
                if offered != Offer::Refused {
                    failures.push(format!(
                        "{name}: refused by a ruling, but offered {offered:?}"
                    ));
                }
                continue;
            }
            let Some(performance) = d.performance(source) else {
                continue;
            };
            let expected = standard::offer(kind.law(), Some(performance), machine);
            if offered != expected {
                failures.push(format!(
                    "{name}: offered {offered:?}, the standard says {expected:?}"
                ));
                continue;
            }
            // A multiplier's factor is neutral at its source's top, not at rest, and has no reach:
            // its module's own tests prove the law, and only the offer applies here.
            if kind.law() == Law::Product {
                continue;
            }
            // **A machine pair is held to a move only where an added one would be offered.** It
            // keeps both halves whatever its target's law — the copy keeps what the machine did —
            // but into a target that throws a sign away its discarded half subtracts from another
            // route and moves nothing alone, and a machine law with no added pairs at all (an
            // edge's reset depth, a sampler) owes no move beyond its rest.
            let live = if machine {
                ends(standard::offer(kind.law(), Some(performance), false))
            } else {
                ends(offered)
            };
            // A bipolar source moves a one-sided target on its other extreme — the lever pushed
            // down under a negative amount — so the larger of the two is what the pair can do.
            let bipolar = performance.range().0 < 0.0 && performance.range().1 > 0.0;
            let reaches = |amount: f32, raw: f32| {
                let one = d.deliver(target, source, amount, raw);
                if !bipolar {
                    return one;
                }
                let other = d.deliver(target, source, amount, -raw);
                if other.abs() > one.abs() || !other.is_finite() {
                    other
                } else {
                    one
                }
            };
            for &amount in ends(offered) {
                let at_rest = d.deliver(target, source, amount, performance.rest());
                if at_rest != 0.0 {
                    failures.push(format!(
                        "{name}: at {amount:+} with {} at rest it delivers {at_rest}, not nothing",
                        performance.name()
                    ));
                }
                let raw = extreme(performance, d.key_unit());
                let moved = reaches(amount, raw);
                if !moved.is_finite() || (live.contains(&amount) && moved.abs() < kind.meaningful())
                {
                    failures.push(format!(
                        "{name}: at {amount:+} it delivers {moved}, below the {} a {kind:?} route must reach",
                        kind.meaningful()
                    ));
                }
                if machine {
                    continue;
                }
                let standard_reach = if performance == Performance::Key {
                    kind.key_reach_per_octave()
                } else {
                    kind.added_reach()
                };
                // At half travel, so a target's own bound cannot hide a reach past it: Amplitude's
                // factor clamps at ±1, and a twelve-decibel reach read as exactly one at full.
                // Not a `let` chain: this crate holds MSRV 1.87, and chains arrived in 1.88.
                if let Some(reach) = standard_reach {
                    let half = reaches(amount * 0.5, raw).abs();
                    if (half - 0.5 * reach).abs() > reach * 1e-3 {
                        failures.push(format!(
                            "{name}: at {:+} it delivers {half}, the standard reach is {reach} at full",
                            amount * 0.5
                        ));
                    }
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// What a host sends a performance source, for [`check_publishers`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    /// A key, played with no glide.
    Note(u8),
    /// A 0…1 value as a host sends it: a velocity, the wheel, pressure.
    Normalised(f32),
    /// The bend lever, −1…+1.
    Lever(f32),
}

/// The inputs [`check_publishers`] plays each source at, and what the standard publishes for them.
fn publisher_cases(source: Performance, key_unit: f32) -> Vec<(Input, f32)> {
    match source {
        // Not 127: a frame bounds every value to unit magnitude, so on a sixty-semitone unit the
        // top seven keys track no further, which each instrument states.
        Performance::Key => [60u8, 72, 36, 0, 108]
            .map(|n| (Input::Note(n), standard::key(f32::from(n), key_unit)))
            .to_vec(),
        Performance::Velocity => [1.0, 0.5, 0.01]
            .map(|v| (Input::Normalised(v), standard::velocity(v)))
            .to_vec(),
        Performance::Wheel => [0.0, 0.5, 1.0]
            .map(|x| (Input::Normalised(x), standard::wheel(x)))
            .to_vec(),
        Performance::Pressure => [0.0, 0.5, 1.0]
            .map(|x| (Input::Normalised(x), standard::pressure(x)))
            .to_vec(),
        Performance::Bend => [0.0, -1.0, 0.5]
            .map(|x| (Input::Lever(x), standard::bend(x)))
            .to_vec(),
        // A voice draws its own; the instrument proves its draw's range where it seeds it.
        Performance::Random => Vec::new(),
    }
}

/// **A voice publishes what the standard says.** For every performance source the declaration has,
/// `published(source, input)` plays one note on a real voice with that input and returns what the
/// voice's frame holds for the source, **in the standard's raw units** — un-scaled from any frame
/// unit. Key at middle C and Velocity at full must read exactly zero; every value must match the
/// standard's publisher to within a frame unit's rounding. Random is the instrument's own to prove.
///
/// # Errors
///
/// Every input a voice published differently.
pub fn check_publishers(
    d: &impl Declaration,
    mut published: impl FnMut(usize, Input) -> f32,
) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    for source in 0..d.sources() {
        let Some(performance) = d.performance(source) else {
            continue;
        };
        for (input, expected) in publisher_cases(performance, d.key_unit()) {
            let got = published(source, input);
            let exact = expected == 0.0;
            if (exact && got != 0.0) || (got - expected).abs() > 1e-6 {
                failures.push(format!(
                    "{} at {input:?} publishes {got}, the standard says {expected}",
                    performance.name()
                ));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

/// One performance route rendered through a real voice, for [`check_release_silence`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Case {
    /// The route's target.
    pub target: usize,
    /// The route's source.
    pub source: usize,
    /// Its amount.
    pub amount: f32,
    /// The note's velocity, 0…1 as a host sends it.
    pub velocity: f32,
    /// The note's key.
    pub key: u8,
}

/// **After a release, no performance route holds a note open.** For every offered pair from a
/// performance source, on each offered half, at the softest and hardest velocities and the lowest
/// and highest keys, `ends_silent` plays one note with that route alone, releases it, runs out the
/// tail with every gesture back at rest, and says whether the voice reached exact silence or went
/// inert. `exempt` lists pairs the machine itself holds open (a declared drone).
///
/// # Errors
///
/// Every case that stayed sounding.
pub fn check_release_silence(
    d: &impl Declaration,
    exempt: &[(usize, usize)],
    mut ends_silent: impl FnMut(Case) -> bool,
) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    for target in 0..d.targets() {
        for source in 0..d.sources() {
            if d.performance(source).is_none()
                || exempt.contains(&(target, source))
                || d.ruled_out(target, source)
            {
                continue;
            }
            for &amount in ends(d.offered(target, source)) {
                for velocity in [0.01, 1.0] {
                    for key in [0, 127] {
                        let case = Case {
                            target,
                            source,
                            amount,
                            velocity,
                            key,
                        };
                        if !ends_silent(case) {
                            failures.push(format!(
                                "{}: at {amount:+}, velocity {velocity}, key {key} it sounds after release",
                                d.name(target, source)
                            ));
                        }
                    }
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
