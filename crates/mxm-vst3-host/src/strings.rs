//! Text and identifiers across the VST3 boundary: UTF-16 `String128`s, NUL-terminated `char8`
//! arrays, and class IDs.

use std::ffi::c_char;
use vst3::Steinberg::TUID;
use vst3::Steinberg::Vst::String128;

/// The text of a UTF-16 buffer, up to its first NUL.
pub(crate) fn from_utf16(units: &[u16]) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// `text` as a NUL-terminated `String128`, cut short to fit.
pub(crate) fn to_string128(text: &str) -> String128 {
    let mut out = [0u16; 128];
    for (slot, unit) in out.iter_mut().take(127).zip(text.encode_utf16()) {
        *slot = unit;
    }
    out
}

/// The text of a `char8` array, up to its first NUL; invalid UTF-8 is replaced.
pub(crate) fn from_char8(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A class ID as the 32 hex digits the SDK prints (`FUID::print`), the same on every platform.
///
/// The SDK stores the four 32-bit words of an ID in a platform's byte order: on Windows the first
/// two in COM's GUID layout, elsewhere big-endian throughout (`vst3::uid`). Reading them back
/// the same way gives an ID that a project saved on one platform finds on another.
pub(crate) fn class_id_text(cid: &TUID) -> String {
    let b = cid.map(|c| c as u8);
    let words: [u32; 4] = if cfg!(target_os = "windows") {
        [
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            u32::from_be_bytes([b[5], b[4], b[7], b[6]]),
            u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
            u32::from_be_bytes([b[12], b[13], b[14], b[15]]),
        ]
    } else {
        [
            u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
            u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
            u32::from_be_bytes([b[12], b[13], b[14], b[15]]),
        ]
    };
    format!(
        "{:08X}{:08X}{:08X}{:08X}",
        words[0], words[1], words[2], words[3]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_id_reads_back_as_the_words_it_was_declared_with() {
        let cid = vst3::uid(0x0123_4567, 0x89AB_CDEF, 0xFEDC_BA98, 0x7654_3210);
        assert_eq!(class_id_text(&cid), "0123456789ABCDEFFEDCBA9876543210");
    }

    #[test]
    fn utf16_round_trips_and_stops_at_nul() {
        let text = to_string128("Wow depth");
        assert_eq!(from_utf16(&text), "Wow depth");
        let long = "x".repeat(300);
        assert_eq!(from_utf16(&to_string128(&long)).len(), 127);
    }
}
