//! **One meaning for every modulation**, on every instrument (the owner's ruling, 2026-09-26).
//!
//! The rest of this crate owns *when* a value is readable and *which* routes are live. This module
//! owns what the performance sources **mean** and what a route the machine never had **reaches**,
//! because a player who routes Velocity to Cutoff on one instrument must get the same gesture on
//! the next. Before it, Velocity → Pitch at full was 7, 12, 24 or 144 semitones depending on the
//! instrument, velocity into three instruments' amplifiers held the note forever, and one
//! instrument's Key sat at 0.47 at middle C.
//!
//! What stays with each instrument: its source and target lists, the reach of every path **the
//! machine itself had** (the copy is of the machine, so its own wiring keeps its own depth), its
//! frame unit and Key unit, and its machine laws — gates, sync, narrowing, CV-summing amplifiers.
//!
//! `plans/plan-modulation-routing.md` §5 (in the private archive) promised that "the source
//! declares its polarity"; this is where it does.

/// The performance sources every instrument carries (`plan-modulation-routing.md` decision 1.7).
///
/// Their names are the standard's: an instrument paints these words and no synonyms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Performance {
    /// The sounding key, after glide. Rest is middle C; bipolar; held through the release.
    Key,
    /// **How far below the hardest note**: `v − 1`. Rest is the hardest note, so a route does
    /// nothing at full velocity and pulls its target down for softer notes. Held through the
    /// release, from the press that last triggered the envelope.
    Velocity,
    /// The mod wheel, 0…1. Rest is down.
    Wheel,
    /// Channel or per-note pressure, 0…1. Rest is none.
    Pressure,
    /// The bend lever, −1…+1, **independent of any bend range**. Rest is centre.
    Bend,
    /// One bipolar draw per note, `2u − 1`. Rest is its mean, zero.
    Random,
}

impl Performance {
    /// Every performance source, in the standard's order.
    pub const ALL: [Self; 6] = [
        Self::Key,
        Self::Velocity,
        Self::Wheel,
        Self::Pressure,
        Self::Bend,
        Self::Random,
    ];

    /// The standard's name for this source, the word every instrument paints.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Key => "Key",
            Self::Velocity => "Velocity",
            Self::Wheel => "Wheel",
            Self::Pressure => "Pressure",
            Self::Bend => "Bend",
            Self::Random => "Random",
        }
    }

    /// The published value at rest — where a route from this source does nothing.
    #[must_use]
    pub const fn rest(self) -> f32 {
        0.0
    }

    /// The published range, as `(low, high)`.
    ///
    /// Key's is the instrument's: `±(60 / key unit)` over the MIDI range, which the frame's unit
    /// bound clips at the top for a unit narrower than the keyboard. Stated here as the unit range.
    #[must_use]
    pub const fn range(self) -> (f32, f32) {
        match self {
            Self::Key | Self::Bend | Self::Random => (-1.0, 1.0),
            Self::Velocity => (-1.0, 0.0),
            Self::Wheel | Self::Pressure => (0.0, 1.0),
        }
    }

    /// Whether the source keeps its value after the key is let go — which is what makes an
    /// additive route from it hold an amplifier open.
    #[must_use]
    pub const fn held_after_release(self) -> bool {
        matches!(self, Self::Key | Self::Velocity | Self::Random)
    }
}

/// Key: `(glided note − 60) ÷ semitones_per_unit`. Rest is middle C.
///
/// The unit is the instrument's — sixty semitones on most, the keyboard CV's own scale where the
/// machine had one — because a unit is internal; what is shared is the meaning (zero at middle C,
/// bipolar) and the readings, which are per octave (`key_scale`).
#[must_use]
pub fn key(glided_note: f32, semitones_per_unit: f32) -> f32 {
    (glided_note - 60.0) / semitones_per_unit
}

/// Velocity as the standard publishes it: `v − 1`, so the hardest note is rest.
///
/// A route from it does nothing at full velocity and moves its target by the whole reach at the
/// softest: on Amplitude, `+100 %` makes the gain follow velocity, which is what "velocity
/// sensitivity" has always meant on a synthesizer.
#[must_use]
pub fn velocity(normalised: f32) -> f32 {
    normalised.clamp(0.0, 1.0) - 1.0
}

/// The mod wheel, 0…1.
#[must_use]
pub fn wheel(normalised: f32) -> f32 {
    normalised.clamp(0.0, 1.0)
}

/// Pressure, 0…1.
#[must_use]
pub fn pressure(normalised: f32) -> f32 {
    normalised.clamp(0.0, 1.0)
}

/// The bend lever, −1…+1 — the lever's position, **not** the pitch it bends by, which is the
/// instrument's own bend range and has nothing to do with a route.
#[must_use]
pub fn bend(lever: f32) -> f32 {
    lever.clamp(-1.0, 1.0)
}

/// Random from a uniform draw in 0…1: bipolar, mean zero.
#[must_use]
pub fn random(unit_draw: f32) -> f32 {
    unit_draw.clamp(0.0, 1.0) * 2.0 - 1.0
}

/// The reach of a route **the machine did not have**, at an amount of one and the source at its
/// extreme — best practice, the same on every instrument. A path the machine had keeps the
/// machine's reach instead.
pub mod reach {
    /// Pitch: an octave, the widest interval a gesture usually throws. On a linear fader a
    /// 50-cent vibrato sits at about four per cent of the travel.
    pub const PITCH_SEMITONES: f32 = 12.0;
    /// Key on pitch, per octave of keyboard: full tracking. `−100 %` is a fixed pitch.
    pub const KEY_PITCH_SEMITONES_PER_OCTAVE: f32 = 12.0;
    /// Cutoff and rates: four octaves (×16), a full gesture sweep.
    pub const OCTAVES: f32 = 4.0;
    /// Key on an exponential target (cutoff, rate), per octave of keyboard: one-to-one tracking.
    pub const KEY_OCTAVES_PER_OCTAVE: f32 = 1.0;
    /// A pulse width: forty-five per cent of the cycle, the swing every machine's PWM already has.
    pub const WIDTH: f32 = 0.45;
    /// A control in its own range (resonance, timbre, an axis, pan): neutral to the top.
    pub const CONTROL: f32 = 1.0;
    /// Amplitude: the whole factor's swing, silence to double.
    pub const AMPLITUDE: f32 = 1.0;
    /// Key on a linear target (a width, a control, amplitude), per octave of keyboard: a fifth of
    /// its reach, so five octaves either side of middle C span the whole reach.
    pub const KEY_LINEAR_FRACTION_PER_OCTAVE: f32 = 0.2;
}

/// The full scale a Key route takes to deliver `per_octave` target units per octave of keyboard,
/// given the instrument's Key unit — `per_octave × semitones_per_unit ÷ 12`.
#[must_use]
pub const fn key_scale(per_octave: f32, semitones_per_unit: f32) -> f32 {
    per_octave * semitones_per_unit / 12.0
}

/// How far the Amplitude routes may take a note, either way: **silence to double** (+6 dB).
///
/// The owner's ruling of 2026-09-26, which closes `plans/plan-collection-sync.md` D8 (in the
/// private archive). It keeps mono-03's accent at the ×2 its circuit reaches and makes Velocity at
/// `+100 %` a gain equal to velocity. Amplitude is therefore a trim around the patch's level, not a VCA: a swell from
/// silence belongs to a machine's own amplifier input where it has one.
pub const AMPLITUDE_SUM_BOUND: f32 = 1.0;

/// The Amplitude factor for a summed route value: `1 + clamp(Σ, ±AMPLITUDE_SUM_BOUND)`.
///
/// **The one amplitude law** (the owner's ruling, 2026-09-26): a linear factor on the level, never
/// added to it, floored at exact silence. Several routes together cannot go past what one can.
#[must_use]
pub fn amplitude_factor(sum: f32) -> f32 {
    if sum.is_finite() {
        1.0 + sum.clamp(-AMPLITUDE_SUM_BOUND, AMPLITUDE_SUM_BOUND)
    } else {
        1.0
    }
}

/// Which way a one-sided target throws away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    /// The target ignores a negative sum.
    Negative,
    /// The target ignores a positive sum.
    Positive,
}

/// What a target does with its summed routes, as far as deciding which pairs mean anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Law {
    /// Added in the target's own unit: pitch, cutoff, rate, width, a control, pan.
    Sum,
    /// [`amplitude_factor`].
    Factor,
    /// Added, but one sign does nothing: a dip-only tremolo, a narrowing-only width, a period
    /// that only shortens.
    OneSided(Sign),
    /// A machine's amplifier built from added CVs (an envelope plus a level): a held source added
    /// here keeps the note sounding after release.
    MachineAmplifier,
    /// Acts on a crossing: a gate, a sync.
    Edge,
    /// Sampled on a clock: an S&H input.
    Sample,
    /// An audio input: a mixer channel, a ring modulator's carrier.
    AudioInput,
    /// A multiplier module: each factor is neutral at its source's **top**, not at its rest
    /// (`crate::product_with_tops`) — a wheel at rest closes the product, which is what the
    /// module is for.
    Product,
}

/// Whether a pair is offered, and which half of its amount can do anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// Not minted: it can never mean anything (its ids retire, never reused).
    Refused,
    /// Both halves.
    Both,
    /// Only a positive amount does anything.
    PositiveOnly,
    /// Only a negative amount does anything.
    NegativeOnly,
}

/// Whether a *(target, source)* pair is offered — the criterion, written once.
///
/// **A pair the machine had is always offered** (`machine_path`): the copy keeps what the machine
/// did. A player-added pair from a performance source is refused where it can never mean anything:
///
/// - into an **edge** target — a gesture crosses a threshold only when a hand moves, so a gate
///   latches open and a sync resets once;
/// - Velocity or Random into a **sampler** — they are constant for the note;
/// - into an **audio input** — a performance source there is a constant offset;
/// - a source held after release, bipolar, into a **machine amplifier** — either half holds the
///   note open (Key, Random).
///
/// A **one-sided** target keeps only the half that reaches its live side; Velocity into a machine
/// amplifier keeps only the half that closes it. A generator source (an LFO, an envelope, an
/// oscillator — `None`) is offered on both halves: it moves by itself, so every law can use it.
#[must_use]
pub const fn offer(law: Law, source: Option<Performance>, machine_path: bool) -> Offer {
    if machine_path {
        return Offer::Both;
    }
    let Some(source) = source else {
        return Offer::Both;
    };
    match law {
        Law::Sum | Law::Factor | Law::Product => Offer::Both,
        Law::Edge | Law::AudioInput => Offer::Refused,
        Law::Sample => match source {
            Performance::Velocity | Performance::Random => Offer::Refused,
            _ => Offer::Both,
        },
        Law::MachineAmplifier => match source {
            Performance::Key | Performance::Random => Offer::Refused,
            // `v − 1` is never positive, so a positive amount can only close the amplifier.
            Performance::Velocity => Offer::PositiveOnly,
            _ => Offer::Both,
        },
        Law::OneSided(ignored) => {
            let (low, high) = source.range();
            match (ignored, low < 0.0, high > 0.0) {
                // A bipolar source reaches both sides whatever its amount's sign.
                (_, true, true) => Offer::Both,
                // Only positive values: the amount's sign is the sum's.
                (Sign::Negative, false, true) => Offer::PositiveOnly,
                (Sign::Positive, false, true) => Offer::NegativeOnly,
                // Only negative values (Velocity): the amount's sign is flipped.
                (Sign::Negative, true, false) => Offer::NegativeOnly,
                (Sign::Positive, true, false) => Offer::PositiveOnly,
                (_, false, false) => Offer::Refused,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Velocity at full does nothing**, which is the whole of the owner's first ruling.
    #[test]
    fn velocity_is_how_far_below_the_hardest_note() {
        assert_eq!(velocity(1.0), 0.0);
        assert_eq!(velocity(0.0), -1.0);
        assert_eq!(velocity(0.5), -0.5);
        assert_eq!(velocity(2.0), 0.0, "clamped");
    }

    /// Velocity through the Amplitude law at `+100 %` is a gain equal to velocity.
    #[test]
    fn amplitude_from_velocity_at_full_depth_follows_velocity() {
        for v in [0.0, 0.25, 0.5, 0.82, 1.0] {
            assert!(
                (amplitude_factor(velocity(v)) - v).abs() < 1e-6,
                "velocity {v}"
            );
        }
    }

    /// The factor is silence to double, and several routes cannot go past one.
    #[test]
    fn the_amplitude_factor_is_silence_to_double() {
        assert_eq!(amplitude_factor(-5.0), 0.0);
        assert_eq!(amplitude_factor(5.0), 2.0);
        assert_eq!(amplitude_factor(0.0), 1.0);
        assert_eq!(amplitude_factor(f32::NAN), 1.0);
    }

    /// Middle C is Key's rest, and a key unit of sixty makes five octaves a unit.
    #[test]
    fn key_rests_at_middle_c() {
        assert_eq!(key(60.0, 60.0), 0.0);
        assert_eq!(key(120.0, 60.0), 1.0);
        assert_eq!(key(72.0, 12.0), 1.0);
    }

    /// A Key route's scale delivers its per-octave reach whatever the instrument's unit.
    #[test]
    fn a_key_scale_delivers_its_reach_per_octave_in_any_unit() {
        for unit in [60.0, 72.0, 120.0, 12.0] {
            let one_octave = key(72.0, unit);
            let delivered = one_octave * key_scale(reach::KEY_PITCH_SEMITONES_PER_OCTAVE, unit);
            assert!((delivered - 12.0).abs() < 1e-4, "unit {unit}: {delivered}");
        }
    }

    /// Every performance source is at rest where a route does nothing.
    #[test]
    fn every_performance_source_publishes_zero_at_rest() {
        assert_eq!(key(60.0, 60.0), Performance::Key.rest());
        assert_eq!(velocity(1.0), Performance::Velocity.rest());
        assert_eq!(wheel(0.0), Performance::Wheel.rest());
        assert_eq!(pressure(0.0), Performance::Pressure.rest());
        assert_eq!(bend(0.0), Performance::Bend.rest());
        assert_eq!(random(0.5), Performance::Random.rest());
    }

    /// The criterion, pinned: a machine pair is never refused, and each refusal is the reason its
    /// doc comment gives.
    #[test]
    fn the_offer_criterion() {
        use Performance::*;
        assert_eq!(
            offer(Law::Edge, Some(Velocity), true),
            Offer::Both,
            "machine pairs stay"
        );
        assert_eq!(offer(Law::Edge, Some(Wheel), false), Offer::Refused);
        assert_eq!(
            offer(Law::Edge, None, false),
            Offer::Both,
            "an LFO into a gate is a clock"
        );
        assert_eq!(offer(Law::Sample, Some(Velocity), false), Offer::Refused);
        assert_eq!(offer(Law::Sample, Some(Wheel), false), Offer::Both);
        assert_eq!(
            offer(Law::AudioInput, Some(Pressure), false),
            Offer::Refused
        );
        assert_eq!(
            offer(Law::MachineAmplifier, Some(Key), false),
            Offer::Refused
        );
        assert_eq!(
            offer(Law::MachineAmplifier, Some(Velocity), false),
            Offer::PositiveOnly
        );
        assert_eq!(
            offer(Law::MachineAmplifier, Some(Wheel), false),
            Offer::Both
        );
        assert_eq!(offer(Law::Sum, Some(Velocity), false), Offer::Both);
        assert_eq!(offer(Law::Factor, Some(Random), false), Offer::Both);
    }

    /// A one-sided target keeps the half that reaches its live side, by the source's polarity.
    #[test]
    fn a_one_sided_target_keeps_its_live_half() {
        use Performance::*;
        let dips = Law::OneSided(Sign::Negative);
        assert_eq!(offer(dips, Some(Wheel), false), Offer::PositiveOnly);
        assert_eq!(offer(dips, Some(Velocity), false), Offer::NegativeOnly);
        assert_eq!(offer(dips, Some(Bend), false), Offer::Both);
        let rises = Law::OneSided(Sign::Positive);
        assert_eq!(offer(rises, Some(Pressure), false), Offer::NegativeOnly);
        assert_eq!(offer(rises, Some(Velocity), false), Offer::PositiveOnly);
    }
}
