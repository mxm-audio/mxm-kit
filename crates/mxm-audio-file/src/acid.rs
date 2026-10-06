//! The `acid` chunk, appended to a finished WAV — **moved unchanged from the player's
//! `sequencer/wav.rs`**, the one piece of container writing this crate owns rather than delegates
//! (`plans/plan-mxm-audio-file.md` §3.1, in the private archive).
//!
//! `hound` writes the audio; the chunk is appended afterwards and the RIFF size fixed up, because
//! `hound` has no API for arbitrary chunks and reimplementing a WAV writer to add twenty-four bytes
//! would be the wrong trade.
//!
//! # The chunk is a bonus, and is treated as one
//!
//! `acid` carries tempo, beat count and a loop flag, and DAWs that understand it tempo-match
//! automatically. **How widely it is honoured is unverified** — that claim cannot be checked from
//! this machine, so the design does not depend on it:
//!
//! - the **file name** is what carries tempo for a person, and is what the design relies on;
//! - the WAV **stays valid and plays correctly if the chunk is wrong**, because a reader that does
//!   not recognise `acid` skips it by length like any other unknown chunk.

/// The ACID chunk's fixed payload size.
const ACID_PAYLOAD: u32 = 24;

/// Flags: bit 0 clear means "not a one-shot", bit 1 set means "root note is set" — we set neither
/// beyond the loop bit, because a sequence loop is exactly what this is.
const ACID_LOOPING: u32 = 0x1;

/// Appends an `acid` chunk to a finished RIFF/WAVE file and repairs the RIFF size.
///
/// Returns the new bytes. The input must be a complete WAV as `hound` writes it.
pub fn with_acid_chunk(mut wav: Vec<u8>, tempo: f64, beats: u32) -> Result<Vec<u8>, String> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err("the rendered audio is not a RIFF/WAVE file".to_owned());
    }

    let mut chunk = Vec::with_capacity(ACID_PAYLOAD as usize + 8);
    chunk.extend_from_slice(b"acid");
    chunk.extend_from_slice(&ACID_PAYLOAD.to_le_bytes());
    // file type: looping, tempo present.
    chunk.extend_from_slice(&ACID_LOOPING.to_le_bytes());
    // root note (unused), and two reserved fields.
    chunk.extend_from_slice(&0u16.to_le_bytes());
    chunk.extend_from_slice(&0u16.to_le_bytes());
    chunk.extend_from_slice(&0f32.to_le_bytes());
    // beats in the file.
    chunk.extend_from_slice(&beats.to_le_bytes());
    // meter: denominator then numerator, 4/4.
    chunk.extend_from_slice(&4u16.to_le_bytes());
    chunk.extend_from_slice(&4u16.to_le_bytes());
    // tempo, in BPM.
    chunk.extend_from_slice(&(tempo as f32).to_le_bytes());

    debug_assert_eq!(chunk.len(), ACID_PAYLOAD as usize + 8);
    wav.extend_from_slice(&chunk);

    // RIFF's size field counts everything after it. A wrong size here would break the whole file
    // for every reader, which is exactly what "a bad chunk must never damage the audio" forbids.
    let riff_size = (wav.len() - 8) as u32;
    wav[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Ok(wav)
}

/// Walks a RIFF file's chunks, returning `(id, payload length)` for each.
///
/// Used by the tests to check the file the way a reader that has never heard of `acid` would: by
/// following declared lengths.
pub fn chunks(wav: &[u8]) -> Result<Vec<([u8; 4], u32)>, String> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".to_owned());
    }
    let declared = u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize;
    if declared + 8 != wav.len() {
        return Err(format!(
            "the RIFF size field says {declared} but the file holds {}",
            wav.len() - 8
        ));
    }

    let mut found = Vec::new();
    let mut at = 12usize;
    while at + 8 <= wav.len() {
        let mut id = [0u8; 4];
        id.copy_from_slice(&wav[at..at + 4]);
        let size = u32::from_le_bytes(wav[at + 4..at + 8].try_into().unwrap());
        found.push((id, size));
        // Chunks are word-aligned: an odd payload is followed by a pad byte.
        at += 8 + size as usize + (size as usize % 2);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but structurally valid WAV, as `hound` would leave it.
    fn a_wav() -> Vec<u8> {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&0u32.to_le_bytes()); // fixed below
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 16]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&8u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 8]);
        let size = (wav.len() - 8) as u32;
        wav[4..8].copy_from_slice(&size.to_le_bytes());
        wav
    }

    #[test]
    fn the_file_stays_walkable_by_a_reader_that_never_heard_of_acid() {
        // The property that matters if the chunk is wrong or unsupported: every other chunk is
        // still reachable by following declared lengths.
        let with = with_acid_chunk(a_wav(), 120.0, 8).unwrap();
        let found = chunks(&with).expect("the file must still parse");
        let ids: Vec<String> = found
            .iter()
            .map(|(id, _)| String::from_utf8_lossy(id).to_string())
            .collect();
        assert_eq!(ids, vec!["fmt ", "data", "acid"]);
    }

    #[test]
    fn the_riff_size_is_repaired() {
        // Getting this wrong breaks the file for every reader, not just for acid-aware ones.
        let with = with_acid_chunk(a_wav(), 120.0, 8).unwrap();
        let declared = u32::from_le_bytes(with[4..8].try_into().unwrap()) as usize;
        assert_eq!(declared + 8, with.len());
    }

    #[test]
    fn the_chunk_is_the_documented_size() {
        let before = a_wav().len();
        let after = with_acid_chunk(a_wav(), 120.0, 8).unwrap().len();
        assert_eq!(after - before, ACID_PAYLOAD as usize + 8);
    }

    #[test]
    fn the_payload_carries_the_tempo_and_beat_count() {
        let with = with_acid_chunk(a_wav(), 137.5, 8).unwrap();
        let at = with
            .windows(4)
            .position(|w| w == b"acid")
            .expect("an acid chunk")
            + 8;
        let beats = u32::from_le_bytes(with[at + 12..at + 16].try_into().unwrap());
        let tempo = f32::from_le_bytes(with[at + 20..at + 24].try_into().unwrap());
        assert_eq!(beats, 8);
        assert!((tempo - 137.5).abs() < 0.01, "tempo was {tempo}");
    }

    #[test]
    fn something_that_is_not_a_wav_is_refused() {
        assert!(with_acid_chunk(b"not a wav".to_vec(), 120.0, 8).is_err());
    }
}
