//! What the collection's sixteen-slot drum engines share.
//!
//! `mxm-drum-machine` and `mxm-model-drums` were built one after the other, and the second took the
//! first's engine scaffolding whole. These are the items that proved identical between them, moved
//! verbatim (the architecture audit's O4): the kit LFO, the performed-source and output frames, the
//! trigger group, and the seeds and transfer time both engines use. Each engine keeps its own
//! `Engine`, slots, voices and models. Plain values in, plain values out; no allocation.

#![forbid(unsafe_code)]

use mxm_dsp::guard::finite_or;
use mxm_modulation::standard;
use mxm_rng::{Xorshift64, map};

/// Slots in a kit: both engines' count, which [`TriggerGroup`]'s mask and the frames are sized by.
pub const SLOT_COUNT: usize = 16;

/// Highest assignable choke group; 0 means the slot is in none.
pub const CHOKE_GROUP_MAX: u8 = 16;

/// The click-free transfer of a slot between outputs, in seconds.
pub const OUTPUT_TRANSFER_SECONDS: f32 = 0.002;

/// The seed of the engines' per-hit random draws.
pub const RANDOM_SEED: u64 = 0x243f_6a88_85a3_08d3;

/// Distinct, because three LFOs sharing a seed would step together for ever on sample and hold.
pub const LFO_SEEDS: [u64; 3] = [
    0x2545_F491_4F6C_DD1D,
    0x1319_8A2E_0370_7344,
    0x5851_F42D_4C95_7F2D,
];

/// Shapes of the three kit LFOs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LfoShape {
    #[default]
    Sine,
    Triangle,
    RampUp,
    RampDown,
    Square,
    SampleHold,
}

/// The kit's performed sources for one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModulationPatch {
    pub lfo_rate_hz: [f32; 3],
    pub lfo_shape: [LfoShape; 3],
    pub wheel: [f32; SLOT_COUNT],
    pub pressure: [f32; SLOT_COUNT],
}

impl Default for ModulationPatch {
    /// The brief's defaults: 0.70, 1.30 and 4.00 Hz; Sine, Triangle, Sine.
    fn default() -> Self {
        Self {
            lfo_rate_hz: [0.7, 1.3, 4.0],
            lfo_shape: [LfoShape::Sine, LfoShape::Triangle, LfoShape::Sine],
            wheel: [0.0; SLOT_COUNT],
            pressure: [0.0; SLOT_COUNT],
        }
    }
}

/// Main stereo and the sixteen mono slot outputs for one sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputFrame {
    pub main: [f32; 2],
    pub individual: [f32; SLOT_COUNT],
}

impl Default for OutputFrame {
    fn default() -> Self {
        Self {
            main: [0.0; 2],
            individual: [0.0; SLOT_COUNT],
        }
    }
}

/// Trigger assertions at one sample offset: one velocity a slot, no allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriggerGroup {
    velocities: [f32; SLOT_COUNT],
    mask: u16,
}

impl Default for TriggerGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerGroup {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            velocities: [0.0; SLOT_COUNT],
            mask: 0,
        }
    }

    /// A hit on `slot` at `velocity` (0–1); two on one slot keep the harder.
    pub fn push(&mut self, slot: usize, velocity: f32) {
        if slot < SLOT_COUNT && velocity.is_finite() && velocity > 0.0 {
            self.mask |= 1 << slot;
            self.velocities[slot] = self.velocities[slot].max(velocity.clamp(0.0, 1.0));
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.mask == 0
    }

    /// One bit a slot struck at this offset.
    #[must_use]
    pub const fn mask(&self) -> u16 {
        self.mask
    }

    /// The velocity asserted on `slot`: 0 where none was. Panics past `SLOT_COUNT`, as an index does.
    #[must_use]
    pub const fn velocity(&self, slot: usize) -> f32 {
        self.velocities[slot]
    }
}

/// A free-running kit LFO.
#[derive(Debug, Clone, Copy)]
pub struct Lfo {
    phase: f64,
    held: f32,
    noise: Xorshift64,
    seed: u64,
}

impl Lfo {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut lfo = Self {
            phase: 0.0,
            held: 0.0,
            noise: Xorshift64::from_state(seed),
            seed,
        };
        lfo.draw();
        lfo
    }

    /// The sample-and-hold sequence's next value: its own generator, drawn once a cycle.
    fn draw(&mut self) {
        self.held = hash_bipolar(self.noise.next_u64());
    }

    /// The value at the current phase.
    #[must_use]
    pub fn value(&self, shape: LfoShape) -> f32 {
        let phase = self.phase as f32;
        match shape {
            LfoShape::Sine => (std::f32::consts::TAU * phase).sin(),
            LfoShape::Triangle => 4.0 * (phase - (phase + 0.5).floor()).abs() - 1.0,
            LfoShape::RampUp => 2.0 * phase - 1.0,
            LfoShape::RampDown => 1.0 - 2.0 * phase,
            LfoShape::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleHold => self.held,
        }
    }

    /// One sample on: the phase moves, and sample and hold draws at each wrap, whether or not
    /// anything reads the value.
    pub fn advance(&mut self, rate_hz: f32, sample_rate: f32) {
        let rate = f64::from(finite_or(rate_hz, 1.0).clamp(0.01, 40.0));
        let previous = self.phase;
        self.phase = (self.phase + rate / f64::from(sample_rate)).fract();
        if self.phase < previous {
            self.draw();
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.noise = Xorshift64::from_state(self.seed);
        self.draw();
    }
}

/// One Random draw: `standard::random` of a uniform draw.
#[inline]
#[must_use]
pub fn hash_bipolar(bits: u64) -> f32 {
    let unit = map::unit_f32_closed((bits >> 32) as u32);
    standard::random(unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trigger_group_keeps_the_harder_hit_and_ignores_what_is_not_one() {
        let mut group = TriggerGroup::new();
        assert!(group.is_empty());
        group.push(3, 0.25);
        group.push(3, 0.75);
        group.push(3, 0.5);
        group.push(4, f32::NAN);
        group.push(5, 0.0);
        group.push(SLOT_COUNT, 1.0);
        group.push(6, 2.0);
        assert_eq!(group.mask, 1 << 3 | 1 << 6);
        assert_eq!((group.velocities[3], group.velocities[6]), (0.75, 1.0));
    }

    #[test]
    fn the_lfo_draws_once_a_cycle_and_restarts_its_sequence_on_reset() {
        let mut lfo = Lfo::new(LFO_SEEDS[0]);
        let first = lfo.value(LfoShape::SampleHold);
        let mut changes = 0;
        let mut last = first;
        for _ in 0..48_000 {
            lfo.advance(4.0, 48_000.0);
            let now = lfo.value(LfoShape::SampleHold);
            changes += usize::from(now != last);
            last = now;
        }
        assert!(
            (3..=4).contains(&changes),
            "{changes} draws in a second at 4 Hz"
        );
        lfo.reset();
        assert_eq!(lfo.value(LfoShape::SampleHold).to_bits(), first.to_bits());
        assert_eq!(lfo.value(LfoShape::Sine), 0.0);
        lfo.advance(f32::NAN, 48_000.0);
        assert!(
            lfo.value(LfoShape::RampUp).is_finite(),
            "a non-finite rate reads as 1 Hz"
        );
    }
}
