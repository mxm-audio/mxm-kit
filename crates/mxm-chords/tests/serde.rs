//! With the `serde` feature, a quality reads and writes the tutti format's strings.

#![cfg(feature = "serde")]

use mxm_chords::Quality;

#[test]
fn a_quality_is_the_formats_string() {
    for quality in Quality::KNOWN {
        let text = serde_json::to_string(&quality).unwrap();
        assert_eq!(text, format!("\"{}\"", quality.tutti()));
        assert_eq!(serde_json::from_str::<Quality>(&text).unwrap(), quality);
    }
    let other: Quality = serde_json::from_str("\"7#11\"").unwrap();
    assert_eq!(other, Quality::Other("7#11".into()));
    assert_eq!(serde_json::to_string(&other).unwrap(), "\"7#11\"");
}
