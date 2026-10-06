//! Shared checks that a plugin's route parameters say what its DSP does — the plugin half of the
//! modulation standard's conformance (`plans/plan-modulation-standard.md` in the private archive,
//! Layer 2).
//!
//! Layer 1 — `mxm_modulation::conformance`, run by each `*-dsp` crate over its own graph — proves the
//! DSP means what the standard says. This proves the parameters a player and a host see say the same
//! thing: that a refused pair has no parameter, an offered one travels only the halves the offer
//! allows, a performance route's reading carries its target's unit and states what the DSP delivers,
//! and every reading survives the host's round trip.

use mxm_modulation::conformance::{self, Declaration, Kind};
use mxm_modulation::standard::Offer;
use mxm_modulation_params::reading::Fader;
use nice_plug::prelude::{FloatParam, Param};

/// The units a kind of target may read in, separator included, and how many of each one
/// target unit is. A control reads as a percentage of its range, or as a multiplier where the
/// control it moves reads that way (a scan speed). A machine law has no standard unit.
fn units(kind: Kind) -> &'static [(&'static str, f32)] {
    match kind {
        Kind::Pitch => &[(" st", 1.0)],
        Kind::Cutoff | Kind::Rate => &[(" oct", 1.0)],
        Kind::Amplitude | Kind::Width | Kind::NarrowingWidth | Kind::Pan => &[(" %", 100.0)],
        Kind::Control => &[(" %", 100.0), ("x", 1.0)],
        Kind::Machine(_) => &[],
    }
}

/// A reading's number, and half a step of its last shown decimal.
fn number(text: &str) -> Option<(f32, f32)> {
    let digits: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
        .collect();
    let places = digits
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    Some((digits.parse().ok()?, 0.5 * 10f32.powi(-(places as i32))))
}

/// Every pair's amount parameter against the DSP's declaration. `amount(target, source)` is the
/// plugin's parameter for a pair, `None` where it minted none. Returns every failure.
pub fn amounts<'a>(
    d: &impl Declaration,
    amount: impl Fn(usize, usize) -> Option<&'a FloatParam>,
) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    for target in 0..d.targets() {
        for source in 0..d.sources() {
            let name = d.name(target, source);
            let offer = d.offered(target, source);
            let param = match (amount(target, source), offer) {
                (None, Offer::Refused) => continue,
                (Some(_), Offer::Refused) => {
                    failures.push(format!("{name}: refused, but a parameter is minted"));
                    continue;
                }
                (None, _) => {
                    failures.push(format!("{name}: offered {offer:?}, but has no parameter"));
                    continue;
                }
                (Some(param), _) => param,
            };

            // The travel is the offer's: a one-sided pair has only its live half.
            let (low, high) = Fader::for_offer(offer, false).bounds();
            let travel = (param.preview_plain(0.0), param.preview_plain(1.0));
            if travel != (low, high) {
                failures.push(format!(
                    "{name}: travels {travel:?}, the offer {offer:?} allows {low}…{high}"
                ));
            }

            // Every reading survives the host's round trip: at the ends, at zero and either
            // side of it, where a rounded negative zero used to print `-0`.
            let zero = param.preview_normalized(0.0);
            for normalised in [0.0, zero - 1e-4, zero, zero + 1e-4, 1.0] {
                let normalised = normalised.clamp(0.0, 1.0);
                let text = param.normalized_value_to_string(normalised, true);
                let again = param
                    .string_to_normalized_value(&text)
                    .map(|back| param.normalized_value_to_string(back, true));
                if again.as_deref() != Some(text.as_str()) {
                    failures.push(format!(
                        "{name}: {text:?} comes back through the host as {again:?}"
                    ));
                }
            }

            // A performance route's reading, at the live end of its travel, carries its
            // target's unit and states what the DSP delivers there.
            let Some(performance) = d.performance(source) else {
                continue;
            };
            let end = if high > 0.0 { high } else { low };
            let text = param.normalized_value_to_string(param.preview_normalized(end), true);
            let per_octave = performance == mxm_modulation::standard::Performance::Key;
            let unit = units(d.kind(target)).iter().find(|(suffix, _)| {
                if per_octave {
                    text.ends_with(&format!("{suffix}/oct"))
                } else {
                    text.ends_with(suffix)
                }
            });
            let Some(&(suffix, scale)) = unit else {
                if !units(d.kind(target)).is_empty() {
                    failures.push(format!(
                        "{name}: reads {text:?}, not in its target's unit{}",
                        if per_octave { " per octave" } else { "" }
                    ));
                }
                continue;
            };
            let unit_text = if per_octave {
                format!("{suffix}/oct")
            } else {
                suffix.to_owned()
            };
            let raw = conformance::extreme(performance, d.key_unit());
            let delivered = d.deliver(target, source, end, raw).abs() * scale;
            match number(&text) {
                Some((shown, step)) if (shown.abs() - delivered).abs() <= step + 1e-3 => {}
                _ => failures.push(format!(
                    "{name}: reads {text:?} at {end:+}, the DSP delivers {delivered}{unit_text}"
                )),
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
