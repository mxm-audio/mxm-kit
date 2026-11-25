//! The standard LFO's parameter half: its shape as the one enum every plugin stores, so an LFO that
//! copies no machine reads and writes its shape alike everywhere (`mxm-lfo`).
//!
//! The ids and names are MXM Modal Synth's, whose general LFO `mxm-lfo` is (relicensed MIT by its
//! copyright holder, the owner, 2026-11-25). **Every id is permanent**: a project or preset stores
//! it. A plugin declares its own LFO's rate, sync and shape parameters with its own ids; only the
//! shape's options are shared.

use nice_plug::prelude::*;

pub use mxm_lfo::{Lfo, LfoShape};

/// An LFO's shape, as a parameter stores it.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoShapeChoice {
    /// A sine, from 0 rising.
    #[id = "sine"]
    #[name = "Sine"]
    Sine,
    /// A triangle, from 0 rising, as the sine.
    #[id = "triangle"]
    #[name = "Triangle"]
    Triangle,
    /// A ramp from −1 up to 1 across the cycle.
    #[id = "saw_up"]
    #[name = "Saw up"]
    SawUp,
    /// A ramp from 1 down to −1 across the cycle.
    #[id = "saw_down"]
    #[name = "Saw down"]
    SawDown,
    /// 1 for the first half of the cycle, −1 for the second.
    #[id = "square"]
    #[name = "Square"]
    Square,
    /// A new random value at each cycle, held through it.
    #[id = "sample_and_hold"]
    #[name = "Sample and hold"]
    SampleAndHold,
    /// A raised-cosine glide from one random value to the next, a value a cycle.
    #[id = "smooth_random"]
    #[name = "Smooth random"]
    SmoothRandom,
}

impl From<LfoShapeChoice> for LfoShape {
    fn from(shape: LfoShapeChoice) -> Self {
        match shape {
            LfoShapeChoice::Sine => Self::Sine,
            LfoShapeChoice::Triangle => Self::Triangle,
            LfoShapeChoice::SawUp => Self::SawUp,
            LfoShapeChoice::SawDown => Self::SawDown,
            LfoShapeChoice::Square => Self::Square,
            LfoShapeChoice::SampleAndHold => Self::SampleAndHold,
            LfoShapeChoice::SmoothRandom => Self::SmoothRandom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The stored ids, for ever**, in the DSP's order, each mapping to its own shape.
    #[test]
    fn the_ids_are_permanent_and_in_the_dsps_order() {
        assert_eq!(
            LfoShapeChoice::ids().unwrap(),
            [
                "sine",
                "triangle",
                "saw_up",
                "saw_down",
                "square",
                "sample_and_hold",
                "smooth_random"
            ]
        );
        assert_eq!(
            LfoShapeChoice::variants(),
            [
                "Sine",
                "Triangle",
                "Saw up",
                "Saw down",
                "Square",
                "Sample and hold",
                "Smooth random"
            ]
        );
        for (i, shape) in LfoShape::ALL.into_iter().enumerate() {
            assert_eq!(LfoShape::from(LfoShapeChoice::from_index(i)), shape);
        }
    }
}
