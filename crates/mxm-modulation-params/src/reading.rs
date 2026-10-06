//! **A route's amount, formatted once** — its reading, its parse and its parameter, for every
//! instrument (the owner's ruling of 2026-09-26: every modulation means the same on every
//! instrument). Ten plugins carried their own copy of this; a unit printed differently on one of
//! them was a modulation that read differently there.
//!
//! A reading is **what the pair delivers at this amount, in the target's own unit** — semitones,
//! octaves, a percentage — and per octave of keyboard for a Key route
//! (`plan-modulation-routing.md` decision 1.11, in the private archive; `docs/code-review-notes.md`
//! §7, *what a route's amount reads*). The plugin supplies the reach, because only it knows its
//! scale table and its sources' peaks; this module supplies everything else.

use std::sync::Arc;

use mxm_modulation::standard::Offer;
use nice_plug::prelude::{FloatParam, FloatRange, SmoothingStyle};

use crate::signed;

/// A reading's unit: what it says, how many decimals it shows, and what a target unit is worth in
/// it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unit {
    /// What follows the number, separator included: `" st"`, but `"x"` for a multiplier, which
    /// reads as the scan-speed knob it moves does.
    pub suffix: &'static str,
    /// Decimals shown.
    pub places: usize,
    /// How many displayed units one target unit is: a hundred for a percentage.
    pub scale: f32,
}

/// Semitones, to two decimals, so a reading resolves below a semitone (`plugins/AGENTS.md`; in
/// full, this repository's `docs/plugin-conventions.md`).
pub const SEMITONES: Unit = Unit {
    suffix: " st",
    places: 2,
    scale: 1.0,
};

/// Octaves, to two decimals.
pub const OCTAVES: Unit = Unit {
    suffix: " oct",
    places: 2,
    scale: 1.0,
};

/// A percentage of the target's own range, whole numbers.
pub const PERCENT: Unit = Unit {
    suffix: " %",
    places: 0,
    scale: 100.0,
};

/// A multiplier on a rate, to two decimals, read against the source's own rate — `+0.60x`.
pub const TIMES: Unit = Unit {
    suffix: "x",
    places: 2,
    scale: 1.0,
};

/// What a route delivers at an amount of one, in its target's unit, and how to say it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reach {
    /// Delivered at an amount of one with the source at its peak — per octave of keyboard for a
    /// Key route.
    pub full: f32,
    /// The reach a **negative** amount multiplies by — `full` unless the machine's law is not a
    /// mirror (`mxm-mono-02`'s inverted envelope reaches less far down than up).
    pub below: f32,
    /// The target's unit.
    pub unit: Unit,
    /// Whether this is a Key route, which reads per octave: `st/oct`, `oct/oct`, `%/oct`.
    pub per_octave: bool,
}

impl Reach {
    /// A reach of `full` target units at an amount of one.
    #[must_use]
    pub const fn new(full: f32, unit: Unit) -> Self {
        Self {
            full,
            below: full,
            unit,
            per_octave: false,
        }
    }

    /// A reach whose negative half is `below` rather than `full`: a machine law that is not a
    /// mirror.
    #[must_use]
    pub const fn below(self, below: f32) -> Self {
        Self { below, ..self }
    }

    /// The reach that applies at `amount`: `below` for a negative amount.
    #[must_use]
    pub fn at(&self, amount: f32) -> f32 {
        if amount < 0.0 { self.below } else { self.full }
    }

    /// A Key route's reach: `full` target units per octave of keyboard.
    #[must_use]
    pub const fn per_octave(full: f32, unit: Unit) -> Self {
        Self {
            full,
            below: full,
            unit,
            per_octave: true,
        }
    }
}

/// How the fader for an amount travels, and which halves it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fader {
    /// −1…+1, linear.
    Linear,
    /// −1…+1, square-law about zero — a machine pitch column wider than ±24 semitones, so a
    /// vibrato's cents sit in the first tenth of the travel (`plugins/AGENTS.md`; in full, this
    /// repository's `docs/plugin-conventions.md`).
    SquareLaw,
    /// 0…+1: a one-sided target whose live half is positive.
    PositiveOnly,
    /// −1…0: a one-sided target whose live half is negative.
    NegativeOnly,
}

impl Fader {
    /// The fader an offer asks for: a one-sided offer has one half, and a two-sided one is linear
    /// unless `square_law`.
    #[must_use]
    pub const fn for_offer(offer: Offer, square_law: bool) -> Self {
        match offer {
            Offer::PositiveOnly => Self::PositiveOnly,
            Offer::NegativeOnly => Self::NegativeOnly,
            Offer::Both | Offer::Refused if square_law => Self::SquareLaw,
            Offer::Both | Offer::Refused => Self::Linear,
        }
    }

    /// The amount's range.
    #[must_use]
    pub const fn range(self) -> FloatRange {
        match self {
            Self::Linear => FloatRange::Linear {
                min: -1.0,
                max: 1.0,
            },
            Self::SquareLaw => FloatRange::SymmetricalSkewed {
                min: -1.0,
                max: 1.0,
                factor: 0.5,
                center: 0.0,
            },
            Self::PositiveOnly => FloatRange::Linear { min: 0.0, max: 1.0 },
            Self::NegativeOnly => FloatRange::Linear {
                min: -1.0,
                max: 0.0,
            },
        }
    }

    /// The amount's lowest and highest values.
    #[must_use]
    pub const fn bounds(self) -> (f32, f32) {
        match self {
            Self::Linear | Self::SquareLaw => (-1.0, 1.0),
            Self::PositiveOnly => (0.0, 1.0),
            Self::NegativeOnly => (-1.0, 0.0),
        }
    }
}

/// A route's reading at `amount`: what it delivers, signed and never a negative zero.
#[must_use]
pub fn reading(reach: Reach, amount: f32) -> String {
    let shown = amount * reach.at(amount) * reach.unit.scale;
    let number = signed(shown, reach.unit.places);
    if reach.per_octave {
        format!("{number}{}/oct", reach.unit.suffix)
    } else {
        format!("{number}{}", reach.unit.suffix)
    }
}

/// [`reading`]'s inverse: a number typed in the reading's unit, back to an amount within the
/// fader's range. The unit's text is ignored, so `7`, `+7 st` and `7.00 st` land on one amount.
///
/// **The half is chosen by the amount's sign, as [`Reach::at`] chooses it**, not by the number's:
/// a reach that runs negative — a period that shortens — delivers a negative number for a positive
/// amount, so the amount is `delivered / full` where that is not negative and `delivered / below`
/// where that is.
#[must_use]
pub fn parse(reach: Reach, fader: Fader, text: &str) -> Option<f32> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
        .collect();
    let entered: f32 = cleaned.parse().ok()?;
    let delivered = entered / reach.unit.scale;
    if !delivered.is_finite() {
        return None;
    }
    let above = (reach.full != 0.0).then(|| delivered / reach.full);
    let below = (reach.below != 0.0).then(|| delivered / reach.below);
    let amount = match (above, below) {
        (Some(amount), _) if amount >= 0.0 => amount,
        (_, Some(amount)) if amount < 0.0 => amount,
        _ => return None,
    };
    let (low, high) = fader.bounds();
    Some(amount.clamp(low, high))
}

/// A route amount parameter: **an amount, so it starts at zero**, on `fader`'s travel, smoothed
/// over `smoothing_ms`, reading and parsing through `reach`.
#[must_use]
pub fn amount_param(name: String, reach: Reach, fader: Fader, smoothing_ms: f32) -> FloatParam {
    amount_param_at(name, 0.0, reach, fader, smoothing_ms)
}

/// [`amount_param`] starting at `init` rather than zero — **only for a machine route whose depth was
/// never a control**, so it has no zero to inherit: `mxm-mono-03`'s accent outputs, `mxm-mono-00`'s
/// `INIT_AT_FULL`. Every other amount starts at zero.
#[must_use]
pub fn amount_param_at(
    name: String,
    init: f32,
    reach: Reach,
    fader: Fader,
    smoothing_ms: f32,
) -> FloatParam {
    FloatParam::new(name, init, fader.range())
        .with_smoother(SmoothingStyle::Linear(smoothing_ms))
        .with_value_to_string(Arc::new(move |value| reading(reach, value)))
        .with_string_to_value(Arc::new(move |text| parse(reach, fader, text)))
}
