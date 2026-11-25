//! The collection's standard LFO: a free-running phase, seven shapes, and random draws from
//! [`mxm_rng::Xorshift32`]. Output −1 to 1.
//!
//! **For an LFO that copies no machine.** A machine's own LFO (MXM Poly-06's is the Juno-106's
//! delayed triangle) stays with its instrument; a plugin whose LFO is its own takes this one, so its
//! shapes, their order and their stored ids are the collection's (the owner, 2026-11-25: "Some of the
//! LFOs are based on specific machins and so should not be in a library.; but yes make a standard one
//! in the library").
//!
//! # Source and licence
//!
//! Lifted from MXM Modal Synth's `crates/mxm-modal-synth-dsp/src/lfo.rs` (its general LFO, which
//! copies no machine), GPL-3.0-or-later there and relicensed MIT here by its copyright holder (the
//! owner, 2026-11-25: "Yes, make the lfo MIT"). The shapes, the random laws and the cycle's draws
//! are its own; Modal Synth's block-rate ceiling, its retrigger choice and its fade-in stay with it.
//! Its draws are this crate's to the bit: its zero seed is `Xorshift32`'s golden-ratio constant and
//! its `(x >> 8) / 2²⁴ · 2 − 1` is [`mxm_rng::map::bipolar_f32`] exactly.
//!
//! # The call's contract is the caller's
//!
//! [`Lfo::advance`] takes the step it is called at, so the LFO knows no engine's block rate. A rate
//! that is not finite, or is below zero, holds still; a `dt` that is not finite or not above zero
//! moves nothing; and one call moves the phase less than one cycle ([`MAX_CYCLE_PER_CALL`]), so each
//! cycle draws exactly one random value however the caller steps it.

use mxm_rng::Xorshift32;

/// The most of a cycle one [`Lfo::advance`] may move: half. A rate past `MAX_CYCLE_PER_CALL / dt` is
/// held there, which keeps one draw a cycle and keeps the fastest wave a wave at the caller's step.
pub const MAX_CYCLE_PER_CALL: f32 = 0.5;

/// The seven shapes, in the order and with the meanings every plugin stores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LfoShape {
    /// A sine, from 0 rising.
    #[default]
    Sine,
    /// From 0, rising, as the sine.
    Triangle,
    /// A ramp from −1 up to 1 across the cycle.
    SawUp,
    /// A ramp from 1 down to −1 across the cycle.
    SawDown,
    /// 1 for the first half of the cycle, −1 for the second.
    Square,
    /// A new random value at each cycle, held through it.
    SampleAndHold,
    /// A raised-cosine glide from one random point to the next, a point a cycle.
    SmoothRandom,
}

impl LfoShape {
    /// Every shape, in order.
    pub const ALL: [Self; 7] = [
        Self::Sine,
        Self::Triangle,
        Self::SawUp,
        Self::SawDown,
        Self::Square,
        Self::SampleAndHold,
        Self::SmoothRandom,
    ];
}

/// One LFO. `Copy`, allocation-free and panic-free: run it on the audio thread.
#[derive(Clone, Copy, Debug)]
pub struct Lfo {
    /// 0 to 1.
    phase: f32,
    /// Sample-and-hold's value.
    held: f32,
    /// The random glide's last point and next.
    from: f32,
    to: f32,
    rng: Xorshift32,
    seed: u32,
}

impl Lfo {
    /// At phase 0, its random stream from `seed` (zero is `Xorshift32`'s golden-ratio constant).
    #[must_use]
    pub fn new(seed: u32) -> Self {
        let mut lfo = Self {
            phase: 0.0,
            held: 0.0,
            from: 0.0,
            to: 0.0,
            rng: Xorshift32::new(seed),
            seed,
        };
        lfo.reset();
        lfo
    }

    /// As made: phase 0 and the random stream from its seed.
    pub fn reset(&mut self) {
        self.rng = Xorshift32::new(self.seed);
        self.phase = 0.0;
        self.held = self.rng.bipolar_f32();
        self.from = self.rng.bipolar_f32();
        self.to = self.rng.bipolar_f32();
    }

    /// Phase 0 again, as a note restarting it would: a new held value, and the random glide sets
    /// off from where it was toward a new point, so it does not jump.
    pub fn restart(&mut self) {
        self.from = self.smooth();
        self.phase = 0.0;
        self.held = self.rng.bipolar_f32();
        self.to = self.rng.bipolar_f32();
    }

    /// Where in its cycle it is, 0 to 1.
    #[must_use]
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Puts it at `phase` in its cycle, wrapped into 0 to 1; a phase that is not finite is 0. For
    /// LFOs that share a rate and must not move together: each set off at its own phase.
    pub fn set_phase(&mut self, phase: f32) {
        self.phase = if phase.is_finite() {
            phase.rem_euclid(1.0).min(f32::from_bits(0x3f7f_ffff))
        } else {
            0.0
        };
    }

    /// The raised cosine from 0 to 1 over `t` in 0 to 1.
    #[inline]
    fn raised_cosine(t: f32) -> f32 {
        0.5 - 0.5 * (core::f32::consts::PI * t).cos()
    }

    /// Smooth random: the glide across the whole cycle, so it has no corners and never leaves −1
    /// to 1.
    #[inline]
    fn smooth(&self) -> f32 {
        self.from + (self.to - self.from) * Self::raised_cosine(self.phase)
    }

    /// **The random glide, finished within `glide` of the cycle**, then held: `glide` 1 is
    /// [`LfoShape::SmoothRandom`] exactly; a small `glide` steps to each new point and holds it, a
    /// sample and hold whose step is a short raised cosine rather than a corner. `glide` is held to
    /// 0 to 1; at 0 (or one that is not finite) the step is at once. Inside −1 to 1 always.
    #[inline]
    #[must_use]
    pub fn random_glide(&self, glide: f32) -> f32 {
        let glide = if glide.is_finite() {
            glide.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let t = if glide <= 0.0 {
            1.0
        } else {
            (self.phase / glide).min(1.0)
        };
        self.from + (self.to - self.from) * Self::raised_cosine(t)
    }

    /// The output at the current phase.
    #[inline]
    #[must_use]
    pub fn value(&self, shape: LfoShape) -> f32 {
        let p = self.phase;
        match shape {
            LfoShape::Sine => (core::f32::consts::TAU * p).sin(),
            LfoShape::Triangle => {
                if p < 0.25 {
                    4.0 * p
                } else if p < 0.75 {
                    2.0 - 4.0 * p
                } else {
                    4.0 * p - 4.0
                }
            }
            LfoShape::SawUp => 2.0 * p - 1.0,
            LfoShape::SawDown => 1.0 - 2.0 * p,
            LfoShape::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::SampleAndHold => self.held,
            LfoShape::SmoothRandom => self.smooth(),
        }
    }

    /// The fastest rate one call at `dt` may move at: [`MAX_CYCLE_PER_CALL`] of a cycle a call.
    #[inline]
    #[must_use]
    pub fn max_rate(dt: f32) -> f32 {
        MAX_CYCLE_PER_CALL / dt
    }

    /// One step: the output at the current phase, then the phase moves on `rate_hz × dt`, under the
    /// module's contract; each new cycle draws the next random values.
    #[inline]
    pub fn advance(&mut self, shape: LfoShape, rate_hz: f32, dt: f32) -> f32 {
        let value = self.value(shape);
        self.step(rate_hz, dt);
        value
    }

    /// The phase moved on `rate_hz × dt` without reading a shape: for a caller that reads
    /// [`Lfo::random_glide`] or several shapes itself.
    #[inline]
    pub fn step(&mut self, rate_hz: f32, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let rate = if rate_hz.is_finite() {
            rate_hz.clamp(0.0, Self::max_rate(dt))
        } else {
            0.0
        };
        self.phase += rate * dt;
        if self.phase >= 1.0 {
            self.phase = self.phase.fract();
            self.held = self.rng.bipolar_f32();
            self.from = self.to;
            self.to = self.rng.bipolar_f32();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 16.0 / 48_000.0;

    fn at(shape: LfoShape, phase: f32) -> f32 {
        let mut lfo = Lfo::new(7);
        lfo.set_phase(phase);
        lfo.value(shape)
    }

    /// Each periodic shape's values at the quarters of its cycle, from phase 0.
    #[test]
    fn the_shapes_at_their_quarters() {
        for (shape, want) in [
            (LfoShape::Sine, [0.0, 1.0, 0.0, -1.0]),
            (LfoShape::Triangle, [0.0, 1.0, 0.0, -1.0]),
            (LfoShape::SawUp, [-1.0, -0.5, 0.0, 0.5]),
            (LfoShape::SawDown, [1.0, 0.5, 0.0, -0.5]),
            (LfoShape::Square, [1.0, 1.0, -1.0, -1.0]),
        ] {
            for (q, w) in want.iter().enumerate() {
                let v = at(shape, q as f32 * 0.25);
                assert!((v - w).abs() < 1e-6, "{shape:?} at {q}/4: {v}");
            }
        }
    }

    /// **Modal Synth's draws, to the bit**: the stream its own xorshift and `unit · 2 − 1` gave.
    #[test]
    fn the_draws_are_modal_synths() {
        for seed in [0u32, 1, 7, 0xdead_beef] {
            let mut x = if seed == 0 { 0x9e37_79b9 } else { seed };
            let mut modal = || {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0
            };
            let lfo = Lfo::new(seed);
            assert_eq!(lfo.held.to_bits(), modal().to_bits());
            assert_eq!(lfo.from.to_bits(), modal().to_bits());
            assert_eq!(lfo.to.to_bits(), modal().to_bits());
        }
    }

    /// Sample and hold holds a value through a cycle and draws a new one at each; smooth random
    /// moves without a step, inside −1 to 1, and arrives at each new point at the cycle's end.
    #[test]
    fn the_random_shapes_hold_and_glide() {
        let mut lfo = Lfo::new(11);
        let rate = 3.0;
        let mut last_held = lfo.value(LfoShape::SampleAndHold);
        let mut last_smooth = lfo.value(LfoShape::SmoothRandom);
        let mut changes = 0;
        for _ in 0..20_000 {
            let before = lfo.phase();
            lfo.step(rate, DT);
            let held = lfo.value(LfoShape::SampleAndHold);
            let smooth = lfo.value(LfoShape::SmoothRandom);
            assert!((-1.0..1.0).contains(&held) && (-1.0..=1.0).contains(&smooth));
            if lfo.phase() > before {
                assert_eq!(held, last_held, "a held value moved inside its cycle");
            } else {
                changes += 1;
            }
            assert!(
                (smooth - last_smooth).abs() < 0.02,
                "the smooth random stepped"
            );
            last_held = held;
            last_smooth = smooth;
        }
        // 20 000 steps of 16 samples at 48 kHz is 6⅔ s: 20 cycles at 3 Hz, a draw at each end of
        // one but the last, which rounding may leave a step short.
        assert!((19..=20).contains(&changes), "{changes} cycles");
    }

    /// The glide: at 1 it is smooth random; small, it reaches each new point within its share of
    /// the cycle and holds it; at 0 it is the new point at once.
    #[test]
    fn the_random_glide_runs_from_a_step_to_smooth_random() {
        let mut lfo = Lfo::new(5);
        lfo.set_phase(0.4);
        assert_eq!(lfo.random_glide(1.0), lfo.value(LfoShape::SmoothRandom));
        assert_eq!(lfo.random_glide(0.0), lfo.to);
        assert_eq!(
            lfo.random_glide(0.2),
            lfo.to,
            "past its glide, held at the new point"
        );
        assert_eq!(lfo.random_glide(f32::NAN), lfo.to);
        lfo.set_phase(0.05);
        let partway = lfo.random_glide(0.2);
        let (lo, hi) = (lfo.from.min(lfo.to), lfo.from.max(lfo.to));
        assert!(
            partway > lo && partway < hi,
            "{partway} not between {lo} and {hi}"
        );
    }

    /// The contract: a rate that is not finite or is below zero holds still, a `dt` that is not
    /// finite or not above zero moves nothing, and one call moves less than one cycle.
    #[test]
    fn the_call_contract() {
        for (rate, dt) in [
            (f32::NAN, DT),
            (f32::INFINITY, DT),
            (-1.0, DT),
            (1.0, 0.0),
            (1.0, -DT),
            (1.0, f32::NAN),
        ] {
            let mut lfo = Lfo::new(3);
            lfo.set_phase(0.3);
            let held = lfo.value(LfoShape::SampleAndHold);
            lfo.step(rate, dt);
            assert_eq!(lfo.phase(), 0.3, "rate {rate}, dt {dt}");
            assert_eq!(lfo.value(LfoShape::SampleAndHold), held);
        }
        let mut lfo = Lfo::new(3);
        lfo.set_phase(0.1);
        lfo.step(1e9, DT);
        assert!((lfo.phase() - 0.6).abs() < 1e-6, "{}", lfo.phase());
    }

    /// A seed's stream repeats, `reset` returns to it, and two seeds differ.
    #[test]
    fn a_seed_repeats_and_reset_returns_to_it() {
        let run = |lfo: &mut Lfo| {
            (0..5000)
                .map(|_| lfo.advance(LfoShape::SampleAndHold, 7.0, DT).to_bits())
                .collect::<Vec<_>>()
        };
        let (mut a, mut b) = (Lfo::new(42), Lfo::new(42));
        let first = run(&mut a);
        assert_eq!(first, run(&mut b));
        a.reset();
        assert_eq!(first, run(&mut a));
        assert_ne!(first, run(&mut Lfo::new(43)));
    }

    /// Restart goes to phase 0 without the random glide jumping.
    #[test]
    fn restart_does_not_jump_the_glide() {
        let mut lfo = Lfo::new(9);
        lfo.set_phase(0.37);
        let before = lfo.value(LfoShape::SmoothRandom);
        lfo.restart();
        assert_eq!(lfo.phase(), 0.0);
        assert_eq!(lfo.value(LfoShape::SmoothRandom), before);
    }

    /// `set_phase` wraps into 0 to 1 and takes a phase that is not finite as 0.
    #[test]
    fn set_phase_wraps() {
        let mut lfo = Lfo::new(1);
        for (given, want) in [(1.25, 0.25), (-0.25, 0.75), (f32::NAN, 0.0), (3.0, 0.0)] {
            lfo.set_phase(given);
            assert!(
                (lfo.phase() - want).abs() < 1e-6,
                "{given}: {}",
                lfo.phase()
            );
        }
        lfo.set_phase(0.999_999_99);
        assert!(lfo.phase() < 1.0);
    }

    /// Every shape stays inside −1 to 1 over many cycles at many rates.
    #[test]
    fn every_shape_stays_in_range() {
        for shape in LfoShape::ALL {
            for rate in [0.01, 0.7, 13.0, 1000.0] {
                let mut lfo = Lfo::new(17);
                for _ in 0..4000 {
                    let v = lfo.advance(shape, rate, DT);
                    assert!((-1.0..=1.0).contains(&v), "{shape:?} at {rate}: {v}");
                }
            }
        }
    }
}
