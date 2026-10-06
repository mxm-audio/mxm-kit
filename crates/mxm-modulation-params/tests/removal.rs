//! What removing a source from a target means at the parameter layer.
//!
//! The owner's requirement, and the half that gets forgotten: a source can be taken off a target,
//! taking it off must actually silence it, and putting it back must restore what was there. The
//! routing crate proves the DSP end (`crates/mxm-modulation/tests/contracts.rs`); this proves the
//! reading that feeds it.

use mxm_modulation::{Compacted, SourceFrame, sum};
use mxm_modulation_params::{Route, amounts, presences};
use mxm_preset::ErasedParam;
use nice_plug::prelude::ParamSetter;

const N: usize = 3;

/// A stand-in for one parameter, so these tests need no plugin and no host.
///
/// Only the handful of `ErasedParam` a route reads is meaningful here; the rest is enough to
/// compile. That is deliberate — a fuller fake would be a second parameter implementation to keep
/// in step with nice-plug's.
struct Fake {
    value: std::cell::Cell<f32>,
    default: f32,
}

impl Fake {
    fn new(value: f32) -> Self {
        Self {
            value: std::cell::Cell::new(value),
            default: value,
        }
    }
}

impl ErasedParam for Fake {
    fn name(&self) -> &str {
        "fake"
    }
    fn text(&self) -> String {
        format!("{}", self.value.get())
    }
    fn normalised(&self) -> f32 {
        self.value.get()
    }
    fn modulation(&self) -> f32 {
        0.0
    }
    fn default_normalised(&self) -> f32 {
        self.default
    }
    fn format(&self, normalised: f32) -> String {
        format!("{normalised}")
    }
    fn parse(&self, _text: &str) -> Option<f32> {
        None
    }
    fn steps(&self) -> Option<usize> {
        None
    }
    fn stepping(&self) -> (f64, f64, f64, f64) {
        (0.01, 0.01, 0.1, 0.1)
    }
    fn begin(&self, _setter: &ParamSetter<'_>) {}
    fn set(&self, _setter: &ParamSetter<'_>, normalised: f32) {
        self.value.set(normalised);
    }
    fn end(&self, _setter: &ParamSetter<'_>) {}
}

/// `0.75` normalised on a bipolar amount is `+0.5` of full scale.
const DEPTH: f32 = 0.75;

fn parts() -> (Vec<Fake>, Vec<Fake>) {
    let present = vec![Fake::new(1.0), Fake::new(0.0), Fake::new(0.0)];
    let amount = vec![Fake::new(DEPTH), Fake::new(DEPTH), Fake::new(DEPTH)];
    (present, amount)
}

fn routes<'a>(present: &'a [Fake], amount: &'a [Fake]) -> [Route<'a>; N] {
    [
        Route {
            source: "LFO",
            present: &present[0],
            amount: &amount[0],
            present_id: "mod_x_lfoon",
            amount_id: "mod_x_lfo",
        },
        Route {
            source: "Envelope",
            present: &present[1],
            amount: &amount[1],
            present_id: "mod_x_envelopeon",
            amount_id: "mod_x_envelope",
        },
        Route {
            source: "Key",
            present: &present[2],
            amount: &amount[2],
            present_id: "mod_x_keyon",
            amount_id: "mod_x_key",
        },
    ]
}

/// A source is on a target when its presence says so, and off when it does not — whatever the
/// amount holds.
#[test]
fn presence_alone_decides_which_routes_are_live() {
    let (present, amount) = parts();
    let r = routes(&present, &amount);
    assert_eq!(presences(&r), [true, false, false]);

    present[1].value.set(1.0);
    assert_eq!(
        presences(&routes(&present, &amount)),
        [true, true, false],
        "adding a source makes its route live and touches no other"
    );
}

/// **Removing a source silences it**, and the depth it had is still there afterwards.
#[test]
fn removing_a_source_silences_it_and_keeps_its_depth() {
    let (present, amount) = parts();
    let full_scale = 6.0;

    let mut frame = SourceFrame::<N>::new();
    frame.begin_sample();
    frame.write(0, 1.0);
    frame.write(1, 1.0);
    frame.write(2, 1.0);

    let mut live = Compacted::<N>::new();

    let r = routes(&present, &amount);
    live.build(&presences(&r));
    let before = sum(&frame, &live, &amounts(&r, full_scale), f32::INFINITY);
    assert!(before > 0.0, "the LFO route should sound to begin with");

    // The removal, as the editor performs it: one write, clearing presence.
    present[0].value.set(0.0);
    let r = routes(&present, &amount);
    live.build(&presences(&r));
    let after = sum(&frame, &live, &amounts(&r, full_scale), f32::INFINITY);
    assert_eq!(after, 0.0, "a removed source must contribute nothing");

    assert_eq!(
        amount[0].normalised(),
        DEPTH,
        "and its amount is untouched, which is what makes re-adding restore the depth"
    );

    // Put it back.
    present[0].value.set(1.0);
    let r = routes(&present, &amount);
    live.build(&presences(&r));
    let restored = sum(&frame, &live, &amounts(&r, full_scale), f32::INFINITY);
    assert_eq!(
        restored, before,
        "re-adding restores exactly what was there"
    );
}

/// An amount is signed: below the centre inverts the route rather than reducing it.
#[test]
fn an_amount_below_centre_inverts_the_route() {
    let (present, amount) = parts();
    amount[0].value.set(0.25); // -0.5 of full scale
    let r = routes(&present, &amount);
    let read = amounts(&r, 6.0);
    assert!(
        (read[0] + 3.0).abs() < 1e-5,
        "expected -3.0, got {}",
        read[0]
    );

    amount[0].value.set(0.5); // the centre
    let r = routes(&present, &amount);
    assert_eq!(amounts(&r, 6.0)[0], 0.0, "the centre is no modulation");
}

/// A present route at zero depth is still a route: it stays in the compacted list and on screen.
#[test]
fn a_present_route_at_zero_depth_remains_a_route() {
    let (present, amount) = parts();
    amount[0].value.set(0.5);
    let r = routes(&present, &amount);
    assert!(presences(&r)[0], "the route is present");

    let mut live = Compacted::<N>::new();
    live.build(&presences(&r));
    assert_eq!(live.len(), 1, "zero depth is not absence");
}
