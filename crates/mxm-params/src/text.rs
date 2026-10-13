//! The collection's value text: the readings a parameter prints and the parsers that read what a
//! person types, one copy where plugins carried the same one (`plans/plan-mxm-params.md`).
//!
//! **Nothing here is a new policy.** Each function is a reading the collection already printed,
//! moved or fixed, and every plugin's text is held to the host's round trip by
//! `mxm_plugin_test::text_checks`. A plugin whose reading is its own keeps it in its `params.rs`.

use std::sync::Arc;

/// Formats a parameter's plain value for display: nice-plug's `with_value_to_string` argument.
pub type ValueToString = Arc<dyn Fn(f32) -> String + Send + Sync>;
/// Parses typed text to a plain value, `None` when it cannot be read: `with_string_to_value`'s.
pub type StringToValue = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

/// `text` without a negative zero: a reading of nothing but zeros (and a decimal point) loses its
/// minus sign, and every other reading comes back as it went in.
///
/// `format!("{:.0}", -0.004)` prints `-0`. A host parses that to zero and prints `0`, so the text
/// is not idempotent through the host's conversion, and `clap-validator`'s `param-conversions`
/// fails whenever its values land in the sliver either side of zero (D6 and D7 in
/// `plans/plan-sibling-audit.md`).
#[must_use]
pub fn without_negative_zero(text: String) -> String {
    match text.strip_prefix('-') {
        Some(digits) if digits.bytes().all(|b| b == b'0' || b == b'.') => digits.to_owned(),
        _ => text,
    }
}

/// A value of `[0, 1]` or `[-1, 1]` as a whole percentage with no unit — nice-plug's
/// `formatters::v2s_f32_percentage(0)`, character for character — **except that a reading that
/// rounds to zero prints `0`, never `-0`**. It reads back through nice-plug's
/// `formatters::s2v_f32_percentage`, the parser it was paired with.
///
/// nice-plug's prints `-0` for a bipolar value a hair below zero, which is why every per-slot
/// deviation in `mxm-drum-machine` and `mxm-model-drums` failed the host's round trip there.
#[must_use]
pub fn whole_percentage() -> ValueToString {
    Arc::new(|value| without_negative_zero(format!("{:.0}", value * 100.0)))
}

/// A route's or a signed control's reading of `value`, signed, to `places` decimals — **never a
/// negative zero**.
///
/// `format!("{:+.0}", -0.004)` prints `-0`. A host parses that back to zero and prints `+0`, so the
/// text is not idempotent through the host's conversion, and `clap-validator`'s `param-conversions`
/// fails whenever its random values land in the sliver either side of zero — which is why one clean
/// run proves nothing. **Only a reading that shows zero changes**: every other value prints exactly
/// as `{:+.N}` does. A stored reading of that zero does change, so a factory file that kept one is
/// regenerated with it. Moved from `mxm-modulation-params`, whose route readings were its first users.
#[must_use]
pub fn signed(value: f32, places: usize) -> String {
    let text = format!("{value:+.places$}");
    match text.strip_prefix('-') {
        Some(digits) if digits.bytes().all(|b| b == b'0' || b == b'.') => format!("+{digits}"),
        _ => text,
    }
}

/// A value of `[0, 1]` or `[-1, 1]` as a whole percent, `{:.0} %`, never `-0 %`. Fourteen plugins
/// carried it: eleven as a plain `{:.0} %`, three (`mxm-mono-00`, `-02`, `-08`) already without the
/// negative zero, which the eleven now take too (`plans/plan-mxm-params.md` P3). Reads back through
/// [`parse_percent`].
#[must_use]
pub fn percent() -> ValueToString {
    Arc::new(|v| format!("{} %", without_negative_zero(format!("{:.0}", v * 100.0))))
}

/// A typed percentage back to `[0, 1]` or `[-1, 1]`: `50`, `50 %` and `50%` all read 0.5. Fifteen
/// plugins carried it.
#[must_use]
pub fn parse_percent() -> StringToValue {
    Arc::new(|text| {
        text.trim()
            .trim_end_matches('%')
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| v / 100.0)
    })
}

/// Seconds as whole milliseconds, `{:.0} ms`, below the second the reading rounds to, and as
/// hundredths of a second, `{:.2} s`, from it. **The unit is chosen from the rounded milliseconds**,
/// not the raw value: a raw `< 1.0` printed `1000 ms` for a value that parses to one second and reads
/// back `1.00 s` (D6). Eight plugins carried it. Reads back through [`parse_time`].
#[must_use]
pub fn milliseconds_then_seconds() -> ValueToString {
    Arc::new(|s| {
        if (s * 1_000.0).round() >= 1_000.0 {
            format!("{s:.2} s")
        } else {
            format!("{:.0} ms", s * 1000.0)
        }
    })
}

/// Seconds as whole milliseconds, `{:.0} ms`, however long — for a time that never reaches a second
/// worth reading in seconds. Two plugins carried it.
#[must_use]
pub fn whole_milliseconds() -> ValueToString {
    Arc::new(|seconds| format!("{:.0} ms", seconds * 1_000.0))
}

/// A typed time back to seconds: `250 ms` and `0.25 s` read the same, a bare number is milliseconds,
/// and case does not matter. Eight plugins carried it.
#[must_use]
pub fn parse_time() -> StringToValue {
    Arc::new(|text| {
        let t = text.trim().to_lowercase();
        let (number, scale) = if let Some(rest) = t.strip_suffix("ms") {
            (rest, 0.001)
        } else if let Some(rest) = t.strip_suffix('s') {
            (rest, 1.0)
        } else {
            (t.as_str(), 0.001)
        };
        number.trim().parse::<f32>().ok().map(|v| v * scale)
    })
}

/// Hertz as tenths, `{:.1} Hz`, below the kilohertz the reading rounds to, and as tenths of a
/// kilohertz, `{:.1} kHz`, from it — the unit chosen from the rounded tenths (D6). Two plugins
/// carried it.
#[must_use]
pub fn tenths_of_hertz_then_kilohertz() -> ValueToString {
    Arc::new(|hz| {
        if (hz * 10.0).round() >= 10_000.0 {
            format!("{:.1} kHz", hz / 1_000.0)
        } else {
            format!("{hz:.1} Hz")
        }
    })
}

/// Hertz as whole hertz, `{:.0} Hz`, below the kilohertz the reading rounds to, and as hundredths of
/// a kilohertz, `{:.2} kHz`, from it. **The unit is chosen from the rounded display value**:
/// otherwise 999.9 Hz formats as `1000 Hz`, parses to exactly 1000, and then formats as `1.00 kHz`.
/// Two plugins carried it.
#[must_use]
pub fn whole_hertz_then_kilohertz() -> ValueToString {
    Arc::new(|value| {
        if value.round() >= 1_000.0 {
            format!("{:.2} kHz", value / 1_000.0)
        } else {
            format!("{value:.0} Hz")
        }
    })
}

/// A filter cutoff: tenths of a hertz below a kilohertz, whole hertz across the bucket from 1000 to
/// 1050 Hz, where tenths of a kilohertz would still read `1.0 kHz`, and tenths of a kilohertz above.
/// Two plugins carried it.
#[must_use]
pub fn cutoff_hertz() -> ValueToString {
    Arc::new(|value| {
        let rounded_hz = value.round();
        if (1_000.0..=1_050.0).contains(&rounded_hz) {
            format!("{rounded_hz:.0} Hz")
        } else if value < 1_000.0 {
            format!("{value:.1} Hz")
        } else {
            format!("{:.1} kHz", value / 1_000.0)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::formatters;

    #[test]
    fn only_a_reading_of_zeros_loses_its_minus_sign() {
        assert_eq!(without_negative_zero("-0".into()), "0");
        assert_eq!(without_negative_zero("-0.00".into()), "0.00");
        assert_eq!(without_negative_zero("-0.01".into()), "-0.01");
        assert_eq!(without_negative_zero("-1".into()), "-1");
        assert_eq!(without_negative_zero("0".into()), "0");
        assert_eq!(without_negative_zero("10".into()), "10");
    }

    /// **The same text as nice-plug's, except a negative zero.** Every value either side of zero at
    /// each decade, every rounding edge of a whole percent and a fine sweep of the whole range.
    #[test]
    fn whole_percentage_is_nice_plugs_reading_without_a_negative_zero() {
        let ours = whole_percentage();
        let theirs = formatters::v2s_f32_percentage(0);
        let mut values: Vec<f32> = (-20_000..=20_000).map(|i| i as f32 / 20_000.0).collect();
        for decade in [1e-9_f32, 1e-7, 1e-5, 1e-3] {
            values.extend([decade, -decade, 4.9 * decade, -4.9 * decade]);
        }
        values.extend((-100..=100).flat_map(|p| {
            let edge = (p as f32 + 0.5) / 100.0;
            [edge.next_down(), edge, edge.next_up()]
        }));
        values.extend([0.0, -0.0]);
        for value in values {
            let theirs = theirs(value);
            let expected = if theirs == "-0" {
                "0".to_owned()
            } else {
                theirs
            };
            assert_eq!(ours(value), expected, "at {value}");
        }
        assert_eq!(ours(-0.004), "0");
        assert_eq!(ours(-0.006), "-1");
    }

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

    #[test]
    fn percent_reads_whole_percents_and_no_negative_zero() {
        let percent = percent();
        assert_eq!(percent(0.5), "50 %");
        assert_eq!(percent(-0.004), "0 %");
        assert_eq!(percent(-0.006), "-1 %");
        let parse = parse_percent();
        assert_eq!(parse("50 %"), Some(0.5));
        assert_eq!(parse(" 50% "), Some(0.5));
        assert_eq!(parse("-25"), Some(-0.25));
        assert_eq!(parse("abc"), None);
    }

    #[test]
    fn a_time_switches_to_seconds_where_its_milliseconds_round_to_a_thousand() {
        let time = milliseconds_then_seconds();
        assert_eq!(time(0.9994), "999 ms");
        assert_eq!(time(0.9995), "1.00 s");
        assert_eq!(time(1.234), "1.23 s");
        let parse = parse_time();
        assert_eq!(parse("250 ms"), Some(0.25));
        assert_eq!(parse("0.25 S"), Some(0.25));
        assert_eq!(parse("250"), Some(0.25));
        assert_eq!(parse("abc"), None);
        assert_eq!(whole_milliseconds()(1.5), "1500 ms");
    }

    #[test]
    fn a_frequency_switches_to_kilohertz_where_its_reading_rounds_to_one() {
        assert_eq!(tenths_of_hertz_then_kilohertz()(999.94), "999.9 Hz");
        assert_eq!(tenths_of_hertz_then_kilohertz()(999.95), "1.0 kHz");
        assert_eq!(whole_hertz_then_kilohertz()(999.4), "999 Hz");
        assert_eq!(whole_hertz_then_kilohertz()(999.5), "1.00 kHz");
        assert_eq!(cutoff_hertz()(999.4), "999.4 Hz");
        assert_eq!(cutoff_hertz()(1_020.0), "1020 Hz");
        assert_eq!(cutoff_hertz()(1_050.6), "1.1 kHz");
    }
}
