//! Shared headless check that a time reading survives the host's text round trip.
//!
//! **A reading that switches from milliseconds to seconds has to choose its unit from the rounded
//! reading, not from the raw value.** Nine plugins chose it from the raw value. A value just past a
//! second printed `1.00 s`, which parses to exactly one second — and a skewed range's normalised
//! inverse lands one second at 0.99999994, which printed `1000 ms`. `clap-validator`'s
//! `param-conversions` fails whenever its random values land in that sliver, and its fixed grid never
//! does, which is why the grid alone passed. `mxm-mono-pr1` paid for the same shape at its kHz
//! boundary; `docs/code-review-notes.md` has the lesson.

use nice_plug::prelude::Params;

/// The validator's own grid, as `mxm-grain-fx`, `mxm-shimmer` and `mxm-mono-pr1` use it.
const GRID_STEPS: u16 = 19;

/// Readings either side of one second, written in seconds so every plugin's parser takes them:
/// the whole-millisecond and tenth-of-a-millisecond rounding edges below it, one second itself,
/// and the `1.00 s` bucket above it.
const BOUNDARY: [&str; 9] = [
    "0.9994 s",
    "0.9995 s",
    "0.9996 s",
    "0.99994 s",
    "0.99995 s",
    "0.99996 s",
    "1 s",
    "1.004 s",
    "1.006 s",
];

/// Every parameter whose reading is in milliseconds at the bottom of its travel and in seconds
/// at the top is printed, parsed and printed again — across the validator's grid and either side
/// of the switch — and must read the same text.
///
/// **`must_include` is what keeps this from passing on nothing.** The selection reads the
/// parameters' own text, so a formatter that changed its spelling would quietly drop every
/// control out of the check; naming the ones the caller knows switch units makes that a failure.
pub fn time_readings_round_trip(params: &impl Params, must_include: &[&str]) {
    let mut checked = Vec::new();
    for (id, ptr, _group) in params.param_map() {
        // SAFETY: `params` owns every parameter these pointers refer to and outlives the loop;
        // this is the same access `param-conversions` makes through CLAP.
        unsafe {
            let bottom = ptr.normalized_value_to_string(0.0, true);
            let top = ptr.normalized_value_to_string(1.0, true);
            if !(bottom.ends_with(" ms") && top.ends_with(" s")) {
                continue;
            }
            let boundary = BOUNDARY.map(|text| {
                ptr.string_to_normalized_value(text)
                    .unwrap_or_else(|| panic!("{id} does not read {text:?}"))
            });
            let grid = (0..=GRID_STEPS).map(|step| f32::from(step) / f32::from(GRID_STEPS));
            for normalized in grid.chain(boundary) {
                let first = ptr.normalized_value_to_string(normalized, true);
                let Some(reparsed) = ptr.string_to_normalized_value(&first) else {
                    panic!("{id} formats {normalized} as {first:?} and then rejects it");
                };
                let second = ptr.normalized_value_to_string(reparsed, true);
                assert_eq!(
                    second, first,
                    "{id} read {first:?} and then {second:?} at {normalized}"
                );
            }
        }
        checked.push(id);
    }
    for id in must_include {
        assert!(
            checked.iter().any(|checked| checked == id),
            "{id} should switch from milliseconds to seconds and was not checked; \
             checked {checked:?}"
        );
    }
}
