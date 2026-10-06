//! The slice of a nice-plug parameter that presets and controls need, as a trait object.
//!
//! Lifted whole from the plugins' `editor/binding.rs`, where it was the same in all five; each
//! binding module now re-exports it. It lives here rather than in `crates/ui` because that crate's
//! contract is *egui and nothing else, no plugin framework*, and this names nice-plug.

use nice_plug::params::Param;
use nice_plug::prelude::ParamSetter;

pub use mxm_ui::control::Press;

/// How one keyboard press moves a parameter: its own step, or a musical one.
///
/// **Declared by the parameter's owner, never guessed.** A unit string cannot tell a filter
/// cutoff from an LFO rate — both are `" Hz"` — and `" st"` also marks continuous tunes the owner
/// ruled out of the semitone law (2026-09-23). So each plugin's binding names the law for the few
/// parameters that have one, and everything else keeps [`StepLaw::Own`].
///
/// Every law is computed in the parameter's plain units, from whatever value the press starts at,
/// and clamped to the range; see [`ErasedParam::step_from`]. Fine is left/right and coarse up/down.
///
/// **`Alt` is a finer layer, and in each layer up/down is the larger step** (the owner, 2026-09-24:
/// *"So that 10%, 1% and 0,1% can be set precisely"*, and on a pitch *"octave, semitone, cent"*;
/// of `Alt` with up/down, *"Make it make meaning"*). Without `Alt` a press moves 10 % or 1 % of the
/// travel, or an octave or a semitone; with it, 1 % or 0.1 %, or ten cents or a cent. **A press
/// never moves less than one of the parameter's own steps**: where a finer size cannot land on its
/// grid — a whole-semitone tune, an option list — it moves one step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum StepLaw {
    /// Fine is nice-plug's own step and coarse a tenth of the range snapped onto it — what
    /// [`ErasedParam::stepping`] reports.
    #[default]
    Own,
    /// A semitone fine, an octave coarse, **from the whole semitone the readout shows**. That is
    /// the value rounded half to even, as the readout's `{:.0}` rounds it, so a continuous bend
    /// range at 2.37 or at 2.5 reads "2" and moves to 3 or 1: the display always changes by one
    /// semitone or one octave. A parameter stepped in whole semitones moves exactly ±1 or ±12.
    ///
    /// Under `Alt`: ten cents and a cent, onto the cent grid.
    Semitones,
    /// Exactly one cent fine, ten coarse, with no grid; under `Alt`, a cent and a tenth. The cents knobs are continuous and read
    /// tenths, so snapping to whole cents would visibly move 7.9 by a tenth.
    Cents,
    /// A semitone fine (×2^(1/12)) and an octave coarse (×2), for a pitch or a filter corner in
    /// hertz, and under `Alt` ten cents and a cent. A ratio can neither leave 0 Hz nor reach it, and `mxm-fx-convolution`'s Low cut is
    /// 0–4000 Hz with 0 meaning Open: up from 0 takes the parameter's own step, and a step down
    /// that would land below the first own step above the minimum lands on the minimum.
    Hertz,
    /// A signed pitch **interval** whose plain value times `octaves_per_unit` is octaves — a
    /// modulation depth into pitch, read as `+0.30 oct` (the owner, 2026-09-23, for mxm-mono-08's
    /// pitch routes: *"it makes it very hard to set musical values in pitch"*).
    ///
    /// Such a reading is finer than a semitone, so a press goes **to the next whole semitone (fine)
    /// or the next whole octave (coarse) in the direction pressed**: +0.30 oct steps to +0.33 or
    /// +0.25, and to +1.00 or +0.00, and from a whole interval it moves exactly one. Two presses
    /// always land on a musical interval. Under `Alt` the lattice is ten cents and a cent.
    /// `octaves_per_unit` is the plugin's own reach, the same number its reading multiplies by.
    Interval { octaves_per_unit: f64 },
    /// A control **voltage** whose whole value, times `octaves_per_unit`, is octaves once a route
    /// turns it into pitch, and whose readout is not a pitch: mxm-mono-08's stage levels, read as
    /// percentages, are one octave full scale through a `+1.00 oct` route.
    ///
    /// **Coarse goes to the next whole semitone in the direction pressed**, as `Interval`'s fine
    /// does, and **fine moves exactly `fine` plain units with no grid** (the owner, 2026-09-23: a
    /// one-octave fader stepped by octaves has two positions, so coarse is the semitone and fine is
    /// 1 %). A fine press therefore detunes off a semitone, and the next coarse press lands back on
    /// one: 2 % up from 0 and then a coarse press up is exactly one semitone, not one and 2 %.
    /// Under `Alt`, coarse goes to the next ten cents and fine moves a tenth of `fine`.
    Voltage { octaves_per_unit: f64, fine: f64 },
}

/// The slice of [`Param`] a control actually needs.
///
/// A trait object rather than a generic, because a section holds `FloatParam`s and `EnumParam`s
/// side by side and they are different types. Everything here is by-normalised-value, which is
/// also exactly what `mxm-ui` speaks.
pub trait ErasedParam {
    fn name(&self) -> &str;
    fn text(&self) -> String;
    /// The parameter's **own** value, with no host modulation in it. This is what a knob shows.
    fn normalised(&self) -> f32;
    /// How far a host's modulation is currently **audibly** moving it, if at all.
    ///
    /// **Not folded into `normalised`**, because a knob shows the value and modulation is something
    /// laid over it — a control drawn at `value + modulation` would jump under your hand while the
    /// thing you are actually setting sat still. Keeping them apart is also what lets the editor say
    /// *this one is being moved by something else*, which is the whole of the mark.
    ///
    /// **Audible, not raw**: nice-plug applies modulation as `(unmodulated + offset).clamp(0, 1)`,
    /// and this is the difference the clamp left behind. A host pushing a parameter further past a
    /// limit it has already reached therefore shows no mark, because nothing about the sound has
    /// changed — which is the right answer for a mark that means *this is not what the knob says*,
    /// and the wrong one for a mark that meant *a host is sending something*. It is the former.
    /// The MXM player never meets the case: it sends `lock - patch`, and both are in range.
    fn modulation(&self) -> f32;
    fn default_normalised(&self) -> f32;
    /// Canonicalize a requested normalized value without changing the live parameter.
    ///
    /// Deferred preset identity needs the value the parameter would actually retain even when a
    /// later user edit means that target is deliberately not written. This round-trip snaps enums,
    /// booleans and stepped parameters through nice-plug's own conversion law. The default is the
    /// identity for existing continuous test/adapter implementations that have no stepping law;
    /// nice-plug parameters override it below.
    fn canonical_normalised(&self, normalised: f32) -> f32 {
        normalised
    }
    /// How this parameter would render `normalised`, without setting it.
    ///
    /// What lets a preset carry a readable `text` for a value the parameter is not currently at —
    /// writing the value first and reading it back would need a `ParamSetter`, which is a host's to
    /// give and not available where presets are built.
    fn format(&self, normalised: f32) -> String;
    fn parse(&self, text: &str) -> Option<f32>;
    /// `Some(n)` for a stepped parameter with `n + 1` values.
    ///
    /// **Not a step size, and not usable as one.** `FloatParam::step_count` is unconditionally
    /// `None` in nice-plug, so a parameter declared `with_step_size` reports itself continuous
    /// here. Use [`ErasedParam::stepping`] to move a value; this is for a control that needs to
    /// know how many *cells* to draw.
    fn steps(&self) -> Option<usize>;
    /// How far one keyboard press moves this parameter from where it is now, normalised.
    ///
    /// The four directions of `mxm_ui::control::Steps`, as
    /// `(fine_up, fine_down, coarse_up, coarse_down)` — all non-negative magnitudes.
    ///
    /// **Fine is 1 % of the travel and coarse 10 %** (the owner, 2026-09-24), each snapped onto the
    /// parameter's own grid through nice-plug's conversion — a declared step size, an enum's or a
    /// bool's cell count — so a press can never land between two legal values, and the range's
    /// skew is honoured: a press near 20 Hz moves a few hertz and one near 20 kHz moves hundreds.
    /// **Never less than one step**: where the fraction rounds to nothing on a coarse grid it is
    /// `Param::next_step`'s one step, so on a parameter with ten or fewer values both axes mean
    /// the adjacent value — there is nothing coarser than the next waveform. The `Alt` layer is
    /// [`ErasedParam::step_from`]'s.
    fn stepping(&self) -> (f64, f64, f64, f64);
    /// The normalised value one keyboard `press` lands on from `normalised`, under `law`.
    ///
    /// Where [`ErasedParam::stepping`] answers "how far from the current value", this answers
    /// "where from **any** value", which is what a musical law needs: a semitone on a skewed hertz
    /// range, or a step onto the whole semitone a readout shows, depends on where the press
    /// starts, and several presses in one frame each start where the one before landed. See
    /// [`StepLaw`] for the laws.
    ///
    /// The default is `stepping()`'s own magnitudes added to `normalised`, for a hand-written
    /// implementation with no plain value to compute in — under `Alt`, fine for up/down and a
    /// tenth of fine for left/right; every nice-plug parameter takes the blanket implementation
    /// below.
    fn step_from(&self, normalised: f64, press: Press, law: StepLaw) -> f64 {
        let _ = law;
        let (fine_up, fine_down, coarse_up, coarse_down) = self.stepping();
        let (up, down) = match (press.coarse, press.finer) {
            (true, false) => (coarse_up, coarse_down),
            (false, false) | (true, true) => (fine_up, fine_down),
            (false, true) => (fine_up / 10.0, fine_down / 10.0),
        };
        let delta = if press.up { up } else { -down };
        (normalised + delta).clamp(0.0, 1.0)
    }
    fn begin(&self, setter: &ParamSetter<'_>);
    fn set(&self, setter: &ParamSetter<'_>, normalised: f32);
    fn end(&self, setter: &ParamSetter<'_>);
}

impl<P: Param> ErasedParam for P {
    fn name(&self) -> &str {
        Param::name(self)
    }

    fn text(&self) -> String {
        // With the unit, because §6 says the unit is part of the formatted value.
        //
        // **Unmodulated**, to match the knob: the two are one control, and a position and a number
        // that disagreed would be worse than either alone.
        self.normalized_value_to_string(self.unmodulated_normalized_value(), true)
    }

    fn normalised(&self) -> f32 {
        self.unmodulated_normalized_value()
    }

    fn modulation(&self) -> f32 {
        self.modulated_normalized_value() - self.unmodulated_normalized_value()
    }

    fn default_normalised(&self) -> f32 {
        self.default_normalized_value()
    }

    fn canonical_normalised(&self, normalised: f32) -> f32 {
        self.preview_normalized(self.preview_plain(normalised))
    }

    fn format(&self, normalised: f32) -> String {
        // With the unit, because §6 says the unit is part of the formatted value.
        self.normalized_value_to_string(normalised, true)
    }

    fn parse(&self, text: &str) -> Option<f32> {
        self.string_to_normalized_value(text)
    }

    fn steps(&self) -> Option<usize> {
        self.step_count()
    }

    fn stepping(&self) -> (f64, f64, f64, f64) {
        own_magnitudes(self, f64::from(self.unmodulated_normalized_value()))
    }

    fn step_from(&self, normalised: f64, press: Press, law: StepLaw) -> f64 {
        let from = normalised.clamp(0.0, 1.0);
        let to = match law {
            StepLaw::Own => own_step(self, from, press),
            StepLaw::Semitones => {
                if press.finer {
                    let grid = if press.coarse { 0.1 } else { 0.01 };
                    normalised_of(self, next_on_grid(plain_of(self, from), grid, press.up))
                } else {
                    let shown = plain_of(self, from).round_ties_even();
                    let size = if press.coarse { 12.0 } else { 1.0 };
                    normalised_of(self, if press.up { shown + size } else { shown - size })
                }
            }
            StepLaw::Cents => {
                let size = match (press.coarse, press.finer) {
                    (true, false) => 10.0,
                    (false, false) | (true, true) => 1.0,
                    (false, true) => 0.1,
                };
                let plain = plain_of(self, from);
                normalised_of(self, if press.up { plain + size } else { plain - size })
            }
            StepLaw::Hertz => hertz_step(self, from, press),
            StepLaw::Interval { octaves_per_unit } => {
                let grid = match (press.coarse, press.finer) {
                    (true, false) => 12.0,
                    (false, false) => 1.0,
                    (true, true) => 0.1,
                    (false, true) => 0.01,
                };
                interval_step(self, from, press, octaves_per_unit, grid)
            }
            StepLaw::Voltage {
                octaves_per_unit,
                fine,
            } => {
                if press.coarse {
                    let grid = if press.finer { 0.1 } else { 1.0 };
                    interval_step(self, from, press, octaves_per_unit, grid)
                } else {
                    let size = if press.finer { fine / 10.0 } else { fine };
                    let plain = plain_of(self, from);
                    normalised_of(self, if press.up { plain + size } else { plain - size })
                }
            }
        };
        let to = to.clamp(0.0, 1.0);
        // **Never less than one step.** A finer size that rounds back onto the value it started
        // from — a cent on a whole-semitone tune — moves one of the parameter's own steps instead.
        if press.finer && (snapped(self, to) - snapped(self, from)).abs() < 1e-9 {
            let fine = Press {
                finer: false,
                coarse: false,
                ..press
            };
            return own_step(self, from, fine).clamp(0.0, 1.0);
        }
        to
    }

    fn begin(&self, setter: &ParamSetter<'_>) {
        setter.begin_set_parameter(self);
    }

    fn set(&self, setter: &ParamSetter<'_>, normalised: f32) {
        setter.set_parameter_normalized(self, normalised);
    }

    fn end(&self, setter: &ParamSetter<'_>) {
        setter.end_set_parameter(self);
    }
}

/// A press's share of the travel: coarse, fine and — under `Alt` — the finest (the owner,
/// 2026-09-24: *"10%, 1% and 0,1%"*).
const COARSE: f64 = 0.10;
const FINE: f64 = 0.01;
const FINEST: f64 = 0.001;

/// The parameter's own fine and coarse magnitudes at `here`, as `(fine_up, fine_down, coarse_up,
/// coarse_down)`. See [`ErasedParam::stepping`].
fn own_magnitudes<P: Param>(param: &P, here: f64) -> (f64, f64, f64, f64) {
    (
        own_magnitude(param, here, FINE, true),
        own_magnitude(param, here, FINE, false),
        own_magnitude(param, here, COARSE, true),
        own_magnitude(param, here, COARSE, false),
    )
}

/// How far a press of `fraction` of the travel moves from `here`, snapped onto the parameter's
/// own grid through nice-plug's conversion, and never less than one of its steps.
fn own_magnitude<P: Param>(param: &P, here: f64, fraction: f64, up: bool) -> f64 {
    let target = if up { here + fraction } else { here - fraction }.clamp(0.0, 1.0);
    let moved = (snapped(param, target) - here).abs();
    if moved > 1e-9 {
        return moved;
    }
    one_step(param, here, up)
}

/// One of the parameter's own steps from `here`: `Param::next_step`/`previous_step`, which honour
/// a declared step size, a cell count and the range's skew. Zero at the end it points past.
fn one_step<P: Param>(param: &P, here: f64, up: bool) -> f64 {
    // `Plain` is not `Copy` in general, so preview it once per direction.
    let from = param.preview_plain(here as f32);
    let to = if up {
        param.next_step(from, true)
    } else {
        param.previous_step(from, true)
    };
    (f64::from(param.preview_normalized(to)) - here).abs()
}

/// `normalised` as the parameter would hold it: through its plain value, so onto its grid.
fn snapped<P: Param>(param: &P, normalised: f64) -> f64 {
    f64::from(param.preview_normalized(param.preview_plain(normalised as f32)))
}

/// A press's share of the travel: 10 % and 1 % without `Alt`, 1 % and 0.1 % with it.
fn fraction(press: Press) -> f64 {
    match (press.coarse, press.finer) {
        (true, false) => COARSE,
        (false, false) | (true, true) => FINE,
        (false, true) => FINEST,
    }
}

/// [`StepLaw::Own`]'s press from `from`: its magnitude measured there, not at the current value.
fn own_step<P: Param>(param: &P, from: f64, press: Press) -> f64 {
    let moved = own_magnitude(param, from, fraction(press), press.up);
    if press.up { from + moved } else { from - moved }
}

/// The next point of a `grid`-spaced lattice from `value` in the direction pressed: the one past
/// it when `value` is on the lattice, the nearest in that direction when it is between two.
///
/// The slack is a ten-thousandth of a cell, so a value a float's error off a lattice point counts
/// as on it rather than stepping to the point it is already showing.
fn next_on_grid(value: f64, grid: f64, up: bool) -> f64 {
    const SLACK: f64 = 1e-4;
    let cells = value / grid;
    let to = if up {
        (cells + SLACK).floor() + 1.0
    } else {
        (cells - SLACK).ceil() - 1.0
    };
    to * grid
}

/// The next point of a `grid`-semitone lattice from `from`, for a parameter whose plain value times
/// `octaves_per_unit` is octaves. A zero reach has no semitones to land on, and takes the
/// parameter's own step.
fn interval_step<P: Param>(
    param: &P,
    from: f64,
    press: Press,
    octaves_per_unit: f64,
    grid: f64,
) -> f64 {
    let semitones_per_unit = 12.0 * octaves_per_unit.abs();
    if semitones_per_unit <= 0.0 {
        return own_step(param, from, press);
    }
    let semitones = plain_of(param, from) * semitones_per_unit;
    normalised_of(
        param,
        next_on_grid(semitones, grid, press.up) / semitones_per_unit,
    )
}

/// [`StepLaw::Hertz`]'s press from `from`.
fn hertz_step<P: Param>(param: &P, from: f64, press: Press) -> f64 {
    let plain = plain_of(param, from);
    let cents = match (press.coarse, press.finer) {
        (true, false) => 1200.0,
        (false, false) => 100.0,
        (true, true) => 10.0,
        (false, true) => 1.0,
    };
    let ratio = 2.0_f64.powf(cents / 1200.0);
    if press.up {
        // No ratio leaves zero: the parameter's own step does, once, and the ratio takes over.
        if plain <= 0.0 {
            return own_step(param, from, press);
        }
        return normalised_of(param, plain * ratio);
    }
    // No ratio reaches zero either, so a step that would land below the first own step above the
    // minimum lands on the minimum — where fx-convolution's Low cut reads Open. **The first step of
    // this press's own layer**, the one that leaves the minimum, so the way down returns to it: under
    // `Alt` a tenth of a percent, so a cent above it steps back onto it rather than to Open.
    let first = plain_of(param, own_magnitude(param, 0.0, fraction(press), true));
    let target = plain / ratio;
    // A ratio undone lands back on the step it left, give or take a float's rounding.
    if target < first * (1.0 - 1e-6) {
        0.0
    } else {
        normalised_of(param, target)
    }
}

/// The plain value at `normalised`, as nice-plug converts it for every parameter type.
///
/// Through `ParamPtr` because `Param::Plain` is a different type per parameter (`f32`, `i32`, an
/// enum) and the laws need one number to do arithmetic on; `ParamPtr` is nice-plug's own
/// conversion to `f32`, the one its wrappers use.
fn plain_of<P: Param>(param: &P, normalised: f64) -> f64 {
    let ptr = param.as_ptr();
    // SAFETY: `ptr` points at `param`, which this borrow keeps alive for the whole call, and the
    // pointer does not outlive the call.
    f64::from(unsafe { ptr.preview_plain(normalised as f32) })
}

/// The normalised value of `plain`, clamped to the parameter's range first: a skewed range raises
/// its offset from the minimum to a power, and a plain value below the minimum would come back NaN.
fn normalised_of<P: Param>(param: &P, plain: f64) -> f64 {
    let (low, high) = (plain_of(param, 0.0), plain_of(param, 1.0));
    let plain = plain.clamp(low.min(high), low.max(high));
    let ptr = param.as_ptr();
    // SAFETY: as in `plain_of`: the borrow keeps `param` alive for the call.
    f64::from(unsafe { ptr.preview_normalized(plain as f32) })
}

#[cfg(test)]
mod tests {
    use nice_plug::params::{FloatParam, IntParam};
    use nice_plug::prelude::{FloatRange, IntRange};

    use super::*;

    const FINE_UP: Press = Press {
        up: true,
        coarse: false,
        finer: false,
    };
    const FINE_DOWN: Press = Press {
        up: false,
        coarse: false,
        finer: false,
    };
    const COARSE_UP: Press = Press {
        up: true,
        coarse: true,
        finer: false,
    };
    const COARSE_DOWN: Press = Press {
        up: false,
        coarse: true,
        finer: false,
    };
    /// `Alt` + right, and `Alt` + up: the finer layer.
    const FINEST_UP: Press = Press {
        up: true,
        coarse: false,
        finer: true,
    };
    const FINEST_DOWN: Press = Press {
        up: false,
        coarse: false,
        finer: true,
    };
    const ALT_COARSE_UP: Press = Press {
        up: true,
        coarse: true,
        finer: true,
    };

    /// Where `press` lands from `plain`, in plain units.
    fn lands<P: Param>(param: &P, plain: f64, press: Press, law: StepLaw) -> f64 {
        let from = normalised_of(param, plain);
        plain_of(param, param.step_from(from, press, law))
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "landed on {actual}, expected {expected}"
        );
    }

    /// mono-00's Coarse: an integer ±24. A semitone is one, an octave twelve, and the end holds.
    #[test]
    fn semitones_on_an_integer_tune_move_one_and_twelve() {
        let coarse = IntParam::new("Coarse", 0, IntRange::Linear { min: -24, max: 24 });
        close(lands(&coarse, 0.0, FINE_UP, StepLaw::Semitones), 1.0, 1e-6);
        close(
            lands(&coarse, 0.0, FINE_DOWN, StepLaw::Semitones),
            -1.0,
            1e-6,
        );
        close(
            lands(&coarse, 0.0, COARSE_UP, StepLaw::Semitones),
            12.0,
            1e-6,
        );
        close(
            lands(&coarse, 20.0, COARSE_UP, StepLaw::Semitones),
            24.0,
            1e-6,
        );
    }

    /// The six continuous bend ranges read whole semitones, so a step starts from the one shown:
    /// 2.37 and 2.5 both read "2" (`{:.0}` rounds half to even), 2.6 reads "3".
    #[test]
    fn semitones_on_a_continuous_range_step_from_the_semitone_the_readout_shows() {
        let bend = FloatParam::new(
            "Bend range",
            2.0,
            FloatRange::Linear {
                min: 0.0,
                max: 24.0,
            },
        );
        close(lands(&bend, 2.37, FINE_UP, StepLaw::Semitones), 3.0, 1e-4);
        close(lands(&bend, 2.37, FINE_DOWN, StepLaw::Semitones), 1.0, 1e-4);
        close(
            lands(&bend, 2.37, COARSE_UP, StepLaw::Semitones),
            14.0,
            1e-4,
        );
        close(
            lands(&bend, 2.37, COARSE_DOWN, StepLaw::Semitones),
            0.0,
            1e-4,
        );
        close(lands(&bend, 2.6, FINE_UP, StepLaw::Semitones), 4.0, 1e-4);
        close(lands(&bend, 2.6, FINE_DOWN, StepLaw::Semitones), 2.0, 1e-4);
        close(lands(&bend, 2.5, FINE_UP, StepLaw::Semitones), 3.0, 1e-4);
        close(lands(&bend, 2.5, FINE_DOWN, StepLaw::Semitones), 1.0, 1e-4);
        close(lands(&bend, 3.5, FINE_UP, StepLaw::Semitones), 5.0, 1e-4);
        close(lands(&bend, 3.5, FINE_DOWN, StepLaw::Semitones), 3.0, 1e-4);
        assert_eq!(
            format!("{:.0}", 2.5_f32),
            "2",
            "the readout the law follows"
        );
    }

    /// A tune knob is continuous and reads tenths, so a cent is a cent from wherever it sits.
    #[test]
    fn cents_move_exactly_one_and_ten_from_anywhere() {
        let tune = FloatParam::new(
            "Tune",
            0.0,
            FloatRange::Linear {
                min: -100.0,
                max: 100.0,
            },
        );
        close(lands(&tune, 7.9, FINE_UP, StepLaw::Cents), 8.9, 1e-3);
        close(lands(&tune, 7.9, FINE_DOWN, StepLaw::Cents), 6.9, 1e-3);
        close(lands(&tune, 7.9, COARSE_UP, StepLaw::Cents), 17.9, 1e-3);
        close(lands(&tune, 7.9, COARSE_DOWN, StepLaw::Cents), -2.1, 1e-3);
        close(lands(&tune, 95.0, COARSE_UP, StepLaw::Cents), 100.0, 1e-3);
        close(
            lands(&tune, -95.0, COARSE_DOWN, StepLaw::Cents),
            -100.0,
            1e-3,
        );
    }

    /// A cutoff on a skewed hertz range: a semitone and an octave are ratios wherever they start.
    #[test]
    fn hertz_moves_by_a_semitone_ratio_and_an_octave() {
        let cutoff = FloatParam::new(
            "Cutoff",
            1000.0,
            FloatRange::Skewed {
                min: 20.0,
                max: 20_000.0,
                factor: FloatRange::skew_factor(-2.0),
            },
        );
        close(lands(&cutoff, 440.0, FINE_UP, StepLaw::Hertz), 466.16, 0.05);
        close(
            lands(&cutoff, 440.0, FINE_DOWN, StepLaw::Hertz),
            415.30,
            0.05,
        );
        close(
            lands(&cutoff, 440.0, COARSE_UP, StepLaw::Hertz),
            880.0,
            0.05,
        );
        close(
            lands(&cutoff, 440.0, COARSE_DOWN, StepLaw::Hertz),
            220.0,
            0.05,
        );
        close(
            lands(&cutoff, 15_000.0, COARSE_UP, StepLaw::Hertz),
            20_000.0,
            0.5,
        );
        close(
            lands(&cutoff, 30.0, COARSE_DOWN, StepLaw::Hertz),
            20.0,
            0.01,
        );
    }

    /// fx-convolution's Low cut: 0 Hz is Open, and no ratio leaves it or reaches it. The own step
    /// leaves it, and a step down past the first own step lands back on it exactly.
    #[test]
    fn hertz_leaves_a_zero_minimum_and_returns_to_it() {
        let low_cut = FloatParam::new(
            "Low cut",
            0.0,
            FloatRange::Linear {
                min: 0.0,
                max: 4000.0,
            },
        );
        let up = lands(&low_cut, 0.0, FINE_UP, StepLaw::Hertz);
        assert!(up > 0.0, "the own step leaves Open: {up}");
        let mut value = up;
        for _ in 0..3 {
            value = lands(&low_cut, value, FINE_DOWN, StepLaw::Hertz);
        }
        assert_eq!(value, 0.0, "and the way down returns to Open exactly");
        let coarse = lands(&low_cut, 0.0, COARSE_UP, StepLaw::Hertz);
        assert!(
            coarse > up,
            "coarse leaves Open further than fine: {coarse} against {up}"
        );
    }

    /// mxm-mono-08's pitch route: an amount of −1…1, square-law about zero, that reads as five
    /// octaves at full. A press goes to the next whole semitone or octave in the pressed direction.
    #[test]
    fn an_interval_steps_to_the_next_semitone_and_octave_in_the_direction_pressed() {
        let depth = FloatParam::new(
            "Complex pitch from Envelope",
            0.0,
            FloatRange::SymmetricalSkewed {
                min: -1.0,
                max: 1.0,
                factor: FloatRange::skew_factor(-1.0),
                center: 0.0,
            },
        );
        let law = StepLaw::Interval {
            octaves_per_unit: 5.0,
        };
        let octaves = |plain: f64| plain * 5.0;
        let from_octaves = |octaves: f64| octaves / 5.0;

        // +0.30 oct is 3.6 semitones: between two, so the next one each way.
        let at = from_octaves(0.30);
        close(octaves(lands(&depth, at, FINE_UP, law)), 4.0 / 12.0, 1e-4);
        close(octaves(lands(&depth, at, FINE_DOWN, law)), 3.0 / 12.0, 1e-4);
        close(octaves(lands(&depth, at, COARSE_UP, law)), 1.0, 1e-4);
        close(octaves(lands(&depth, at, COARSE_DOWN, law)), 0.0, 1e-4);

        // On an interval, exactly one step.
        let at = from_octaves(1.0);
        close(octaves(lands(&depth, at, FINE_UP, law)), 13.0 / 12.0, 1e-4);
        close(octaves(lands(&depth, at, COARSE_UP, law)), 2.0, 1e-4);
        close(octaves(lands(&depth, at, COARSE_DOWN, law)), 0.0, 1e-4);

        // Below zero the same, and the ends hold.
        let at = from_octaves(-0.30);
        close(octaves(lands(&depth, at, FINE_UP, law)), -3.0 / 12.0, 1e-4);
        close(
            octaves(lands(&depth, at, FINE_DOWN, law)),
            -4.0 / 12.0,
            1e-4,
        );
        close(
            octaves(lands(&depth, from_octaves(4.5), COARSE_UP, law)),
            5.0,
            1e-4,
        );
        close(
            octaves(lands(&depth, from_octaves(-4.5), COARSE_DOWN, law)),
            -5.0,
            1e-4,
        );
    }

    /// mxm-mono-08's stage level: 0–100 %, one octave full scale through a pitch route. Coarse
    /// goes to the next whole semitone, fine moves exactly 1 %, and a coarse press after a fine one
    /// lands back on a semitone.
    #[test]
    fn a_voltage_steps_a_semitone_coarse_and_an_exact_amount_fine() {
        let level = FloatParam::new(
            "Step 1 level",
            0.0,
            FloatRange::Linear { min: 0.0, max: 1.0 },
        );
        let law = StepLaw::Voltage {
            octaves_per_unit: 1.0,
            fine: 0.01,
        };
        let semitone = 1.0 / 12.0;

        // The owner's case: 2 % up by fine, then coarse up is one semitone, not one and 2 %.
        let two = lands(&level, lands(&level, 0.0, FINE_UP, law), FINE_UP, law);
        close(two, 0.02, 1e-6);
        close(lands(&level, two, COARSE_UP, law), semitone, 1e-6);
        close(lands(&level, two, COARSE_DOWN, law), 0.0, 1e-6);

        // On a semitone, coarse moves exactly one; fine detunes off it by exactly 1 %.
        close(
            lands(&level, semitone, COARSE_UP, law),
            2.0 * semitone,
            1e-6,
        );
        close(lands(&level, semitone, COARSE_DOWN, law), 0.0, 1e-6);
        close(lands(&level, semitone, FINE_UP, law), semitone + 0.01, 1e-6);
        close(
            lands(&level, semitone, FINE_DOWN, law),
            semitone - 0.01,
            1e-6,
        );

        // Twelve coarse presses walk the whole range, and the ends hold.
        let mut at = 0.0;
        for _ in 0..12 {
            at = lands(&level, at, COARSE_UP, law);
        }
        close(at, 1.0, 1e-6);
        close(lands(&level, 1.0, COARSE_UP, law), 1.0, 1e-6);
        close(lands(&level, 1.0, FINE_UP, law), 1.0, 1e-6);
        close(lands(&level, 0.0, FINE_DOWN, law), 0.0, 1e-6);
    }

    /// The owner's three sizes, 2026-09-24: *"10%, 1% and 0,1%"*, with `Alt` + up/down a percent —
    /// in each layer up/down is the larger step.
    #[test]
    fn own_moves_ten_one_and_a_tenth_of_a_percent_of_the_travel() {
        let level = FloatParam::new(
            "Level",
            50.0,
            FloatRange::Linear {
                min: 0.0,
                max: 100.0,
            },
        );
        close(lands(&level, 50.0, COARSE_UP, StepLaw::Own), 60.0, 1e-3);
        close(lands(&level, 50.0, FINE_UP, StepLaw::Own), 51.0, 1e-3);
        close(lands(&level, 50.0, ALT_COARSE_UP, StepLaw::Own), 51.0, 1e-3);
        close(lands(&level, 50.0, FINEST_UP, StepLaw::Own), 50.1, 1e-3);
        close(lands(&level, 50.0, FINEST_DOWN, StepLaw::Own), 49.9, 1e-3);
        // The end holds, whichever layer.
        close(lands(&level, 100.0, FINEST_UP, StepLaw::Own), 100.0, 1e-6);
    }

    /// A press never moves less than one of the parameter's own steps: on an option list and on a
    /// whole-semitone tune, the finer layer still reaches the adjacent value.
    #[test]
    fn a_finer_press_on_a_stepped_parameter_moves_one_step() {
        let choice = IntParam::new("Mode", 1, IntRange::Linear { min: 0, max: 2 });
        close(lands(&choice, 1.0, FINEST_UP, StepLaw::Own), 2.0, 1e-6);
        close(lands(&choice, 1.0, FINEST_DOWN, StepLaw::Own), 0.0, 1e-6);
        let coarse = IntParam::new("Coarse", 0, IntRange::Linear { min: -24, max: 24 });
        close(
            lands(&coarse, 0.0, FINEST_UP, StepLaw::Semitones),
            1.0,
            1e-6,
        );
        close(
            lands(&coarse, 0.0, ALT_COARSE_UP, StepLaw::Semitones),
            1.0,
            1e-6,
        );
    }

    /// fx-convolution's Low cut under `Alt`: the finest step leaves Open, a cent goes up from there,
    /// and the way down undoes the cent before it returns to Open — the first step of its own layer.
    #[test]
    fn hertz_under_alt_leaves_a_zero_minimum_and_returns_by_its_own_layer() {
        let low_cut = FloatParam::new(
            "Low cut",
            0.0,
            FloatRange::Linear {
                min: 0.0,
                max: 4000.0,
            },
        );
        let first = lands(&low_cut, 0.0, FINEST_UP, StepLaw::Hertz);
        close(first, 4.0, 1e-3);
        let cent = lands(&low_cut, first, FINEST_UP, StepLaw::Hertz);
        assert!(cent > first, "a cent up from the first step: {cent}");
        close(
            lands(&low_cut, cent, FINEST_DOWN, StepLaw::Hertz),
            first,
            1e-3,
        );
        assert_eq!(lands(&low_cut, first, FINEST_DOWN, StepLaw::Hertz), 0.0);
    }

    /// On a pitch the finer layer is ten cents and a cent (the owner: *"octave, semitone, cent"*).
    #[test]
    fn a_pitch_under_alt_moves_ten_cents_and_a_cent() {
        let cutoff = FloatParam::new(
            "Cutoff",
            1000.0,
            FloatRange::Skewed {
                min: 20.0,
                max: 20_000.0,
                factor: FloatRange::skew_factor(-2.0),
            },
        );
        let cent = 2.0_f64.powf(1.0 / 1200.0);
        close(
            lands(&cutoff, 440.0, FINEST_UP, StepLaw::Hertz),
            440.0 * cent,
            0.02,
        );
        close(
            lands(&cutoff, 440.0, ALT_COARSE_UP, StepLaw::Hertz),
            440.0 * cent.powi(10),
            0.02,
        );
        // A continuous bend range, read in semitones: onto the cent grid from anywhere.
        let bend = FloatParam::new(
            "Bend range",
            2.0,
            FloatRange::Linear {
                min: 0.0,
                max: 24.0,
            },
        );
        close(
            lands(&bend, 2.374, FINEST_UP, StepLaw::Semitones),
            2.38,
            1e-3,
        );
        close(
            lands(&bend, 2.374, ALT_COARSE_UP, StepLaw::Semitones),
            2.4,
            1e-3,
        );
        close(
            lands(&bend, 2.374, FINEST_DOWN, StepLaw::Semitones),
            2.37,
            1e-3,
        );
    }

    /// `Own` is today's law, evaluated wherever the press starts.
    #[test]
    fn own_matches_the_fixed_magnitudes_at_the_current_value() {
        let cutoff = FloatParam::new(
            "Cutoff",
            1000.0,
            FloatRange::Skewed {
                min: 20.0,
                max: 20_000.0,
                factor: FloatRange::skew_factor(-2.0),
            },
        );
        let here = f64::from(cutoff.unmodulated_normalized_value());
        let (fine_up, fine_down, coarse_up, coarse_down) = cutoff.stepping();
        for (press, expected) in [
            (FINE_UP, here + fine_up),
            (FINE_DOWN, here - fine_down),
            (COARSE_UP, here + coarse_up),
            (COARSE_DOWN, here - coarse_down),
        ] {
            close(cutoff.step_from(here, press, StepLaw::Own), expected, 1e-6);
        }
    }
}
