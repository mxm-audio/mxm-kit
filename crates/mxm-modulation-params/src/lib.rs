//! The parameter and interface half of the collection's modulation routing.
//!
//! `plans/plan-modulation-routing.md` §4.2 and §4.3, in the private archive. This is a **second
//! crate** rather than a feature of [`mxm_modulation`] because it names nice-plug and egui and
//! therefore sits at 1.95, where the routing half must stay at 1.87 so every `*-dsp` crate can take
//! it. Cargo unifies features across a build graph, so a feature gate would not have held that line
//! — `docs/adding-an-instrument.md` gotcha 10 records what that already cost here once.
//!
//! It owns the shape of a route's two parameters, the reading of them into the arrays
//! [`mxm_modulation`] wants, and the interface for a target's routes — including **the gesture that
//! removes one**, which is a single parameter write and nothing else.
//!
//! # What a plugin still declares for itself
//!
//! Its own `#[derive(Params)]` structs, because the derive needs concrete fields and every
//! instrument's source list is its own. `mxm-mono-08` already builds 120 permanent routing ids from
//! two small reusable structs and one `#[nested(id_prefix = …)]` per destination; that is the shape,
//! and this crate deliberately does not try to generate it. What is shared is everything *around*
//! those fields.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod reading;
pub mod ui;

use mxm_preset::ErasedParam;

/// One route: the *(target, source)* pair's two parameters, borrowed for a block or a frame.
///
/// **Presence and amount, and nothing else.** No selector — a route's source is fixed by which pair
/// it is — no per-route polarity switch, no "via" modifier. The presence is the enable.
pub struct Route<'a> {
    /// What this route's source is called, for the interface and for accessibility.
    pub source: &'a str,
    /// Whether the route exists at all. Clearing it is how a source is removed from a target.
    pub present: &'a dyn ErasedParam,
    /// How much, signed, in the target's own declared domain.
    pub amount: &'a dyn ErasedParam,
    /// The presence's permanent id, used as its keyboard-cursor scope.
    ///
    /// **The scopes used to be the literals `"present"` and `"amount"`**, which meant every row in
    /// every stack registered the same two keys: the cursor registry is keyed by name, so fifty-five
    /// rows collapsed into two entries and no row could be told from any other. Naming each scope
    /// after the parameter it edits is what makes a route reachable *individually*, and it is what
    /// lets the shared keyboard-coverage check see routes at all — it matches on parameter ids.
    pub present_id: &'a str,
    /// The amount's permanent id, used as its keyboard-cursor scope. See [`Route::present_id`].
    pub amount_id: &'a str,
}

impl Route<'_> {
    /// Whether this route is live. A stepped parameter's normalised value is its cell.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.present.normalised() >= 0.5
    }
}

/// Reads a target's presences into the array [`mxm_modulation::Compacted::build`] wants.
///
/// Call once per processing interval, not per sample: topology is discrete and changes only on a
/// parameter event, which is the whole of the efficiency design.
///
/// **Presence alone, never the amount.** A present route at zero depth is still a route; deriving
/// topology from a smoothed value would drop it mid-ramp and click.
#[must_use]
pub fn presences<const N: usize>(routes: &[Route<'_>; N]) -> [bool; N] {
    // Every route is absent until its presence says otherwise, so a mis-sized array fails closed.
    let mut present = [false; N];
    for (slot, route) in present.iter_mut().zip(routes.iter()) {
        *slot = route.is_present();
    }
    present
}

/// Reads a target's amounts into the array the combination laws want, in the target's own domain.
///
/// `full_scale` is the target's declared reach — six octaves for a cutoff, seven semitones for a
/// pitch — and an amount is a signed fraction of it. An **absent** route's amount is still read
/// here and simply never used, because [`mxm_modulation::Compacted`] is what decides which routes
/// are summed; that is what leaves the depth intact for when the route is added back.
#[must_use]
pub fn amounts<const N: usize>(routes: &[Route<'_>; N], full_scale: f32) -> [f32; N] {
    let mut out = [0.0; N];
    for (slot, route) in out.iter_mut().zip(routes.iter()) {
        // A bipolar amount is stored normalised in 0..=1 with 0.5 as zero.
        *slot = (route.amount.normalised() * 2.0 - 1.0) * full_scale;
    }
    out
}

/// Adds a source to a target: sets that pair's presence. **One parameter write.**
pub fn add(route: &Route<'_>, setter: &nice_plug::prelude::ParamSetter<'_>) {
    route.present.begin(setter);
    route.present.set(setter, 1.0);
    route.present.end(setter);
}

/// Removes a source from a target: clears that pair's presence. **One parameter write, and the
/// amount is deliberately left alone.**
///
/// Leaving the depth behind is what makes re-adding the source restore what the player last set —
/// design system §8.7's *"removing an assignment is undoable"*, with no undo stack — and it is what
/// keeps the gesture to a single write, which the player reads as a step lock rather than as a
/// preset load (`docs/adding-an-instrument.md` gotcha 4).
pub fn remove(route: &Route<'_>, setter: &nice_plug::prelude::ParamSetter<'_>) {
    route.present.begin(setter);
    route.present.set(setter, 0.0);
    route.present.end(setter);
}

/// A route's reading of `value`, signed, to `places` decimals — **never a negative zero**.
///
/// `format!("{:+.0}", -0.004)` prints `-0`. A host parses that back to zero and prints `+0`, so the
/// text is not idempotent through the host's conversion, and `clap-validator`'s `param-conversions`
/// fails whenever its random values land in the sliver either side of zero — which is why one clean
/// run proves nothing. **Only a reading that shows zero changes**: every other value prints exactly
/// as `{:+.N}` does. A stored reading of that zero does change, so a factory file that kept one is
/// regenerated with it.
#[must_use]
pub fn signed(value: f32, places: usize) -> String {
    let text = format!("{value:+.places$}");
    match text.strip_prefix('-') {
        Some(digits) if digits.bytes().all(|b| b == b'0' || b == b'.') => format!("+{digits}"),
        _ => text,
    }
}
