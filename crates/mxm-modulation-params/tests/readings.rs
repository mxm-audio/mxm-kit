//! A route's reading: signed, and never a negative zero — which a host's round trip cannot survive.

use mxm_modulation_params::signed;

/// Whether a signed reading's digits are all zero, whichever sign it carries.
fn shows_zero(text: &str) -> bool {
    text.trim_start_matches(['+', '-'])
        .bytes()
        .all(|b| b == b'0' || b == b'.')
}

#[test]
fn a_value_that_rounds_to_zero_reads_positive_zero_from_either_side() {
    for (value, places, want) in [
        (-0.0_f32, 0, "+0"),
        (-0.004, 0, "+0"),
        (-0.49, 0, "+0"),
        (0.49, 0, "+0"),
        (-0.0, 2, "+0.00"),
        (-0.004, 2, "+0.00"),
        (0.004, 2, "+0.00"),
    ] {
        assert_eq!(signed(value, places), want, "{value} to {places} places");
    }
}

#[test]
fn every_other_value_prints_as_the_plain_signed_format_does() {
    for places in 0..=3 {
        for step in -3000..=3000 {
            let value = step as f32 * 0.000_37;
            let plain = format!("{value:+.places$}");
            let shown = signed(value, places);
            if shows_zero(&plain) {
                assert_eq!(shown[1..], plain[1..], "{value} to {places} places");
                assert!(
                    shown.starts_with('+'),
                    "{value} to {places} places: {shown}"
                );
            } else {
                assert_eq!(shown, plain, "{value} to {places} places");
            }
        }
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(signed(value, 2), format!("{value:+.2}"));
    }
}

mod shared_reading {
    use mxm_modulation_params::reading::{
        Fader, OCTAVES, PERCENT, Reach, SEMITONES, amount_param, parse, reading,
    };
    use nice_plug::prelude::Param;

    /// The shared reading prints what the per-plugin copies printed, so moving an instrument onto
    /// it changes no reading: poly-06's machine routes at full.
    #[test]
    fn the_shared_reading_prints_what_the_copies_printed() {
        assert_eq!(reading(Reach::new(7.0, SEMITONES), 1.0), "+7.00 st");
        assert_eq!(reading(Reach::new(0.45, PERCENT), 1.0), "+45 %");
        assert_eq!(reading(Reach::new(3.0, OCTAVES), -1.0), "-3.00 oct");
        assert_eq!(
            reading(Reach::per_octave(1.0, OCTAVES), 1.0),
            "+1.00 oct/oct"
        );
        assert_eq!(
            reading(Reach::per_octave(12.0, SEMITONES), 0.5),
            "+6.00 st/oct"
        );
        assert_eq!(reading(Reach::per_octave(0.2, PERCENT), 1.0), "+20 %/oct");
    }

    /// Typed in the reading's unit, a number comes back to the amount that reads it, with or
    /// without the unit's words, clamped to the fader's range.
    #[test]
    fn a_typed_reading_lands_on_its_amount_within_the_faders_range() {
        let pitch = Reach::new(12.0, SEMITONES);
        assert_eq!(parse(pitch, Fader::Linear, "+6.00 st"), Some(0.5));
        assert_eq!(parse(pitch, Fader::Linear, "-6"), Some(-0.5));
        assert_eq!(parse(pitch, Fader::Linear, "99"), Some(1.0));
        assert_eq!(parse(pitch, Fader::PositiveOnly, "-6"), Some(0.0));
        assert_eq!(parse(pitch, Fader::NegativeOnly, "6"), Some(0.0));
        assert_eq!(
            parse(Reach::new(1.0, PERCENT), Fader::Linear, "50 %"),
            Some(0.5)
        );
        assert_eq!(parse(pitch, Fader::Linear, "st"), None);
        // A law that is not a mirror reads and parses each half at its own reach.
        let envelope = Reach::new(10.0, OCTAVES).below(4.0);
        assert_eq!(reading(envelope, 1.0), "+10.00 oct");
        assert_eq!(reading(envelope, -1.0), "-4.00 oct");
        assert_eq!(parse(envelope, Fader::Linear, "-2 oct"), Some(-0.5));
        assert_eq!(parse(envelope, Fader::Linear, "5 oct"), Some(0.5));
        assert_eq!(parse(Reach::new(0.0, SEMITONES), Fader::Linear, "1"), None);
    }

    /// Through the host's own conversion a reading is idempotent, either side of zero and at the
    /// ends of every fader — `clap-validator`'s `param-conversions`, the one clean run of which
    /// proves nothing on its own.
    #[test]
    fn a_route_amount_survives_the_hosts_round_trip_on_every_fader() {
        for fader in [
            Fader::Linear,
            Fader::SquareLaw,
            Fader::PositiveOnly,
            Fader::NegativeOnly,
        ] {
            for reach in [
                Reach::new(12.0, SEMITONES),
                Reach::new(4.0, OCTAVES),
                Reach::new(1.0, PERCENT),
                Reach::per_octave(1.0, OCTAVES),
                Reach::new(10.0, OCTAVES).below(4.0),
            ] {
                let param = amount_param("Route".into(), reach, fader, 10.0);
                for normalised in [0.0, 0.25, 0.4999, 0.5, 0.5001, 0.75, 1.0] {
                    let text = param.normalized_value_to_string(normalised, true);
                    let back = param
                        .string_to_normalized_value(&text)
                        .unwrap_or_else(|| panic!("{fader:?} {reach:?}: {text} did not parse"));
                    let again = param.normalized_value_to_string(back, true);
                    assert_eq!(text, again, "{fader:?} {reach:?} at {normalised}");
                }
                assert_eq!(param.default_plain_value(), 0.0, "an amount starts at zero");
            }
        }
    }
}

/// **`parse` is `reading`'s inverse on both halves of a reach that runs negative and is not a mirror**
/// — the half is the amount's sign, as `Reach::at` says, never the number's.
///
/// Falsified before trusted: choosing the half by the entered number's sign parses `-5` (the reading
/// at `+0.5`) back to `+1`.
#[test]
fn a_negative_asymmetric_reach_reads_back_on_both_halves() {
    use mxm_modulation_params::reading::{Fader, PERCENT, Reach, parse, reading};
    let reach = Reach::new(-0.1, PERCENT).below(-0.04);
    for amount in [0.5_f32, -0.5, 1.0, -1.0, 0.0] {
        let text = reading(reach, amount);
        let back = parse(reach, Fader::Linear, &text).expect("a reading parses");
        assert!(
            (back - amount).abs() < 1e-6,
            "{amount} reads {text:?} and parses to {back}"
        );
    }
    // And a positive reach that is not a mirror, the case the negative half was written for.
    let reach = Reach::new(7.0, PERCENT).below(3.0);
    for amount in [0.5_f32, -0.5, -1.0] {
        let text = reading(reach, amount);
        let back = parse(reach, Fader::Linear, &text).expect("a reading parses");
        assert!(
            (back - amount).abs() < 1e-2,
            "{amount} reads {text:?} and parses to {back}"
        );
    }
}
