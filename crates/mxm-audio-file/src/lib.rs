//! Writes audio files for the collection: WAV, AIFF and FLAC.
//!
//! | [`Target`] | File | Samples |
//! |---|---|---|
//! | `WavFloat32` | WAV | 32-bit IEEE float |
//! | `Wav(Bits)` | WAV | 16- or 24-bit integer |
//! | `Aiff(Bits)` | AIFF (not AIFF-C) | 16- or 24-bit integer, big-endian |
//! | `Flac(Bits)` | FLAC | 16- or 24-bit integer, lossless |
//!
//! Plus the [`acid`] chunk on a finished WAV. **Not written:** MP3, AAC, ALAC, Vorbis, Opus.
//!
//! **Established encoders, not ours.** WAV goes through `hound`, AIFF through `aifc`, FLAC through
//! `flacenc`. This crate owns only what those crates leave to their caller: turning `f32` into the
//! integers a target stores, refusing a buffer that is not audio, reporting what clipped, and
//! writing the file so a crash never leaves a half-written one looking finished.
//! `plans/plan-mxm-audio-file.md` §3.1–3.2, in the private archive.
//!
//! **No policy.** No gain, no normalisation, no dither, no channel folding. A caller that wants
//! headroom scales before calling, where the argument for the amount can be read — the stance
//! `mxm-measure`'s writer took, and the one its migrated callers already follow.
//!
//! **One deliberate exception to "not ours": the `acid` chunk** ([`acid`]). No crate writes it, so
//! the player's tested append moved here unchanged rather than being replaced by a dependency.

use std::fmt;
use std::io::{self, Cursor};
use std::path::Path;

pub mod acid;

/// What a file stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// WAV, 32-bit IEEE float. Nothing clips on the way out, which is why the player exports it.
    WavFloat32,
    /// WAV, signed integer PCM.
    Wav(Bits),
    /// AIFF (not AIFF-C), big-endian signed integer PCM.
    Aiff(Bits),
    /// FLAC, lossless.
    Flac(Bits),
}

/// The integer widths an integer target stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bits {
    Sixteen,
    TwentyFour,
}

impl Bits {
    /// Bits per sample.
    pub fn count(self) -> u16 {
        match self {
            Bits::Sixteen => 16,
            Bits::TwentyFour => 24,
        }
    }

    /// The code a full-scale sample maps to: `2^(bits−1) − 1`.
    fn full_scale(self) -> f64 {
        match self {
            Bits::Sixteen => 32_767.0,
            Bits::TwentyFour => 8_388_607.0,
        }
    }
}

/// What a successful encode did to the audio it was given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Encoded {
    /// Samples outside `-1.0..=1.0` that an integer target limited to full scale. Always zero for
    /// [`Target::WavFloat32`]. **Reported, never silent**: the player chose float so an
    /// un-normalised render could not clip, and an integer target must say when one did.
    pub clipped: usize,
}

/// Why nothing was written.
#[derive(Debug)]
pub enum Error {
    /// The buffer, channel count or rate cannot describe a file of that target.
    Shape(String),
    /// A sample that is not a number. A NaN written as silence or an infinity written as full scale
    /// is a plausible file over a DSP failure, so the whole buffer is refused.
    NonFinite { sample: usize },
    /// The encoder refused the data.
    Encoder(String),
    /// Writing the file failed; the previous file at that path, if any, is untouched.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Shape(what) => write!(f, "{what}"),
            Error::NonFinite { sample } => write!(f, "sample {sample} is not a number"),
            Error::Encoder(what) => write!(f, "the encoder refused the audio: {what}"),
            Error::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Io(error)
    }
}

/// How an `f32` becomes an integer sample — **this crate's published definition**, not a tolerance.
///
/// Clamped to `-1.0..=1.0`, scaled by `2^(bits−1) − 1` and rounded to nearest, so `+1.0` is the
/// largest positive code and `-1.0` one above the smallest. At 16 bits this is exactly
/// the rule `mxm_measure::wav::quantise` published before it retired here, so the harnesses that
/// moved here write the bytes they wrote before.
/// `None` for a non-finite sample, which has no encoding.
pub fn quantise(sample: f32, bits: Bits) -> Option<i32> {
    if !sample.is_finite() {
        return None;
    }
    Some(match bits {
        // f32 arithmetic, as `mxm-measure` does it: f64 could round a half-code case differently.
        Bits::Sixteen => i32::from((sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16),
        Bits::TwentyFour => (f64::from(sample.clamp(-1.0, 1.0)) * bits.full_scale()).round() as i32,
    })
}

/// Encode interleaved samples into the bytes of a whole file.
pub fn encode(
    interleaved: &[f32],
    channels: u16,
    rate: u32,
    target: Target,
) -> Result<(Vec<u8>, Encoded), Error> {
    check_shape(interleaved, channels, rate, target)?;
    if let Some(sample) = interleaved.iter().position(|s| !s.is_finite()) {
        return Err(Error::NonFinite { sample });
    }
    match target {
        Target::WavFloat32 => Ok((wav_float(interleaved, channels, rate)?, Encoded::default())),
        Target::Wav(bits) => {
            let (samples, encoded) = integers(interleaved, bits);
            Ok((wav_int(&samples, channels, rate, bits)?, encoded))
        }
        Target::Aiff(bits) => {
            let (samples, encoded) = integers(interleaved, bits);
            Ok((aiff(&samples, channels, rate, bits)?, encoded))
        }
        Target::Flac(bits) => {
            let (samples, encoded) = integers(interleaved, bits);
            Ok((flac(&samples, channels, rate, bits)?, encoded))
        }
    }
}

/// Encode and write a file at `path`, atomically.
///
/// The bytes go to a temporary file beside `path` and are renamed over it, so a failure or a crash
/// never leaves a half-written file looking finished — the rule the player's export already had.
/// A missing parent directory is created.
pub fn write(
    path: impl AsRef<Path>,
    interleaved: &[f32],
    channels: u16,
    rate: u32,
    target: Target,
) -> Result<Encoded, Error> {
    let (bytes, encoded) = encode(interleaved, channels, rate, target)?;
    write_atomic(path, &bytes)?;
    Ok(encoded)
}

/// Write finished file bytes at `path` through a temporary file and a rename.
///
/// Public for a caller that changes the bytes between [`encode`] and the disk — the player appends
/// an [`acid`] chunk there.
pub fn write_atomic(path: impl AsRef<Path>, bytes: &[u8]) -> io::Result<()> {
    let path = path.as_ref();
    // Nested rather than a `let` chain: chains are stable from Rust 1.88, and this crate builds at 1.87.
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

fn check_shape(interleaved: &[f32], channels: u16, rate: u32, target: Target) -> Result<(), Error> {
    let shape = |what: &str| Err(Error::Shape(what.to_owned()));
    if channels == 0 {
        return shape("an audio file needs at least one channel");
    }
    if rate == 0 {
        return shape("an audio file needs a sample rate");
    }
    if !interleaved.len().is_multiple_of(usize::from(channels)) {
        return shape("the buffer does not hold whole frames for that channel count");
    }
    match target {
        Target::WavFloat32 => wav_bounds(interleaved.len(), channels, rate, 4),
        Target::Wav(bits) => wav_bounds(interleaved.len(), channels, rate, bits.count() / 8),
        // AIFF stores the channel count as a signed 16-bit field. Its FORM size is checked by `aifc`,
        // which refuses a stream over 32 bits (`SizeTooLarge`).
        Target::Aiff(_) if channels > i16::MAX as u16 => {
            shape("AIFF holds at most 32,767 channels")
        }
        // FLAC's frame header has three bits for the channel assignment and twenty for the rate.
        Target::Flac(_) if channels > 8 => shape("FLAC holds at most eight channels"),
        Target::Flac(_) if rate >= 1 << 20 => shape("FLAC holds rates below 1,048,576 Hz"),
        _ => Ok(()),
    }
}

/// Bytes a WAV may spend outside its sample data: hound's largest header (68 bytes, for
/// WAVE_FORMAT_EXTENSIBLE) and room for chunks a caller appends to the finished file — the player's
/// 32-byte [`acid`] chunk among them. A margin, not a measurement of any one file.
const WAV_OVERHEAD_BYTES: u64 = 1_024;

/// **A WAV's sizes are 32-bit, and hound does not check them.** hound 3.5.1 multiplies the byte rate and
/// counts data bytes in unchecked `u32` (`write.rs`), so an oversized buffer panics under overflow checks
/// and, in a release build, returns a file whose header sizes have wrapped. Refused here instead, from
/// the shape alone: `samples` is the interleaved sample count.
fn wav_bounds(
    samples: usize,
    channels: u16,
    rate: u32,
    bytes_per_sample: u16,
) -> Result<(), Error> {
    let block_align = u64::from(channels) * u64::from(bytes_per_sample);
    if block_align > u64::from(u16::MAX) {
        return Err(Error::Shape(
            "a WAV frame holds at most 65,535 bytes: too many channels".to_owned(),
        ));
    }
    if u64::from(rate) * block_align > u64::from(u32::MAX) {
        return Err(Error::Shape(
            "a WAV's byte rate must fit 32 bits: the sample rate is too high".to_owned(),
        ));
    }
    let data = (samples as u64).checked_mul(u64::from(bytes_per_sample));
    if data.is_none_or(|data| data > u64::from(u32::MAX) - WAV_OVERHEAD_BYTES) {
        return Err(Error::Shape(
            "a WAV holds at most 4 GiB of sample data".to_owned(),
        ));
    }
    Ok(())
}

/// Quantise a finite buffer, counting what was limited. Callers have already refused non-finite.
fn integers(interleaved: &[f32], bits: Bits) -> (Vec<i32>, Encoded) {
    let clipped = interleaved.iter().filter(|s| s.abs() > 1.0).count();
    let samples = interleaved
        .iter()
        .map(|&s| quantise(s, bits).expect("non-finite samples were refused before quantising"))
        .collect();
    (samples, Encoded { clipped })
}

fn hound_error(error: hound::Error) -> Error {
    match error {
        hound::Error::IoError(error) => Error::Io(error),
        other => Error::Encoder(other.to_string()),
    }
}

fn wav_float(interleaved: &[f32], channels: u16, rate: u32) -> Result<Vec<u8>, Error> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut bytes = Vec::new();
    let mut writer = hound::WavWriter::new(Cursor::new(&mut bytes), spec).map_err(hound_error)?;
    for &sample in interleaved {
        writer.write_sample(sample).map_err(hound_error)?;
    }
    writer.finalize().map_err(hound_error)?;
    Ok(bytes)
}

fn wav_int(samples: &[i32], channels: u16, rate: u32, bits: Bits) -> Result<Vec<u8>, Error> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: bits.count(),
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = Vec::new();
    let mut writer = hound::WavWriter::new(Cursor::new(&mut bytes), spec).map_err(hound_error)?;
    for &sample in samples {
        match bits {
            Bits::Sixteen => writer.write_sample(sample as i16),
            Bits::TwentyFour => writer.write_sample(sample),
        }
        .map_err(hound_error)?;
    }
    writer.finalize().map_err(hound_error)?;
    Ok(bytes)
}

fn aiff(samples: &[i32], channels: u16, rate: u32, bits: Bits) -> Result<Vec<u8>, Error> {
    let aifc_error = |error: aifc::AifcError| match error {
        aifc::AifcError::StdIoError(error) => Error::Io(error),
        other => Error::Encoder(format!("{other:?}")),
    };
    let info = aifc::AifcWriteInfo {
        file_format: aifc::FileFormat::Aiff,
        channels: channels as i16,
        sample_rate: f64::from(rate),
        sample_format: match bits {
            Bits::Sixteen => aifc::SampleFormat::I16,
            Bits::TwentyFour => aifc::SampleFormat::I24,
        },
    };
    let mut stream = Cursor::new(Vec::new());
    {
        let mut writer = aifc::AifcWriter::new(&mut stream, &info).map_err(aifc_error)?;
        match bits {
            Bits::Sixteen => {
                let narrow: Vec<i16> = samples.iter().map(|&s| s as i16).collect();
                writer.write_samples_i16(&narrow)
            }
            Bits::TwentyFour => writer.write_samples_i24(samples),
        }
        .map_err(aifc_error)?;
        writer.finalize().map_err(aifc_error)?;
    }
    Ok(stream.into_inner())
}

fn flac(samples: &[i32], channels: u16, rate: u32, bits: Bits) -> Result<Vec<u8>, Error> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;

    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, error)| Error::Encoder(format!("{error:?}")))?;
    let source = flacenc::source::MemSource::from_samples(
        samples,
        usize::from(channels),
        usize::from(bits.count()),
        rate as usize,
    );
    let mut stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|error| Error::Encoder(format!("{error:?}")))?;
    // **STREAMINFO's minimum block size excludes the last block** (FLAC format, STREAMINFO), and for a
    // fixed-blocksize stream the reference encoder writes it equal to the maximum. flacenc 0.5.1 counts
    // the short final block, so any stream whose length is not a multiple of the block size declares
    // min < max. symphonia 0.6.1 reads min ≠ max as a variable-blocksize stream and rejects every
    // frame-numbered header in it (`symphonia-bundle-flac` `strict_frame_header_check`), so it could
    // not open the file at all — while ffmpeg decoded it. Corrected here, through flacenc's own public
    // setter, rather than by patching either crate. A 4,096-frame file was unaffected; 256, 5,000 and
    // 9,001 frames were refused before this line.
    stream
        .stream_info_mut()
        .set_block_sizes(config.block_size, config.block_size)
        .map_err(|error| Error::Encoder(format!("{error:?}")))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|error| Error::Encoder(format!("{error:?}")))?;
    Ok(sink.as_slice().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frames: usize, channels: u16, amplitude: f32) -> Vec<f32> {
        (0..frames * usize::from(channels))
            .map(|i| (i as f32 * 0.013).sin() * amplitude)
            .collect()
    }

    #[test]
    fn sixteen_bit_quantisation_is_mxm_measures_definition() {
        // The expression `mxm_measure::wav::quantise` published before it retired here, restated so the equivalence the
        // migrated harnesses rely on is executed, not recorded.
        let measure = |s: f32| (s.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        for i in 0..40_001 {
            let sample = i as f32 / 20_000.0 - 1.0 + 1.0e-5 * (i % 7) as f32;
            assert_eq!(
                quantise(sample, Bits::Sixteen),
                Some(i32::from(measure(sample))),
                "sample {sample}"
            );
        }
        assert_eq!(quantise(1.0, Bits::Sixteen), Some(32_767));
        assert_eq!(quantise(-1.0, Bits::Sixteen), Some(-32_767));
        assert_eq!(quantise(2.0, Bits::TwentyFour), Some(8_388_607));
        assert_eq!(quantise(f32::NAN, Bits::Sixteen), None);
        assert_eq!(quantise(f32::INFINITY, Bits::TwentyFour), None);
    }

    #[test]
    fn every_target_refuses_a_non_finite_sample_and_names_it() {
        for target in [
            Target::WavFloat32,
            Target::Wav(Bits::Sixteen),
            Target::Aiff(Bits::TwentyFour),
            Target::Flac(Bits::Sixteen),
        ] {
            let error = encode(&[0.1, f32::NAN, 0.2, 0.3], 2, 48_000, target)
                .expect_err("a NaN must be refused");
            assert!(
                matches!(error, Error::NonFinite { sample: 1 }),
                "{target:?}: {error}"
            );
        }
    }

    #[test]
    fn integer_targets_report_what_clipped_and_float_reports_nothing() {
        let hot = [0.5, 1.5, -2.0, 1.0, -1.0, 0.0];
        for target in [
            Target::Wav(Bits::Sixteen),
            Target::Aiff(Bits::Sixteen),
            Target::Flac(Bits::TwentyFour),
        ] {
            let (_, encoded) = encode(&hot, 1, 44_100, target).expect("encodes");
            assert_eq!(encoded.clipped, 2, "{target:?}: exactly ±1.0 is not a clip");
        }
        let (_, encoded) = encode(&hot, 1, 44_100, Target::WavFloat32).expect("encodes");
        assert_eq!(encoded.clipped, 0);
    }

    #[test]
    fn a_shape_no_file_can_hold_is_refused_before_encoding() {
        assert!(matches!(
            encode(&[0.0; 3], 2, 48_000, Target::WavFloat32),
            Err(Error::Shape(_))
        ));
        assert!(matches!(
            encode(&[0.0; 2], 0, 48_000, Target::WavFloat32),
            Err(Error::Shape(_))
        ));
        assert!(matches!(
            encode(&[0.0; 2], 1, 0, Target::Wav(Bits::Sixteen)),
            Err(Error::Shape(_))
        ));
        assert!(matches!(
            encode(&[0.0; 9], 9, 48_000, Target::Flac(Bits::Sixteen)),
            Err(Error::Shape(_))
        ));
    }

    #[test]
    fn a_wav_too_large_for_its_32_bit_fields_is_refused_before_encoding() {
        // Proved on the shape, without a 4 GiB buffer: the largest data size that leaves room for the
        // header and an appended chunk is accepted, one sample more is refused.
        let room = u64::from(u32::MAX) - WAV_OVERHEAD_BYTES;
        let largest_float = (room / 4) as usize;
        assert!(wav_bounds(largest_float, 2, 48_000, 4).is_ok());
        assert!(matches!(
            wav_bounds(largest_float + 1, 2, 48_000, 4),
            Err(Error::Shape(_))
        ));
        let largest_24 = (room / 3) as usize;
        assert!(wav_bounds(largest_24, 1, 48_000, 3).is_ok());
        assert!(wav_bounds(largest_24 + 1, 1, 48_000, 3).is_err());
        // A byte rate or a frame that cannot be written, through the public API.
        for (channels, rate) in [(2, u32::MAX), (u16::MAX, 48_000)] {
            assert!(matches!(
                encode(&[0.0; 0], channels, rate, Target::WavFloat32),
                Err(Error::Shape(_))
            ));
        }
        assert!(encode(&[0.0; 16], 8, 192_000, Target::WavFloat32).is_ok());
    }

    #[test]
    fn a_sixteen_bit_stereo_wav_has_the_canonical_pcm_header() {
        // The header the retired `mxm_measure::wav::write_pcm16` wrote and its callers' files carry, field by
        // field against the format rather than against a reader: 2 channels, 44.1 kHz, 3 frames.
        let (bytes, _) = encode(&[0.0; 6], 2, 44_100, Target::Wav(Bits::Sixteen)).expect("encodes");
        let data_len = 3 * 2 * 2;
        assert_eq!(bytes.len(), 44 + data_len, "header plus data, nothing else");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[4..8], &((36 + data_len) as u32).to_le_bytes());
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(&bytes[16..20], &16u32.to_le_bytes(), "PCM fmt chunk size");
        assert_eq!(&bytes[20..22], &1u16.to_le_bytes(), "format tag 1 = PCM");
        assert_eq!(&bytes[22..24], &2u16.to_le_bytes());
        assert_eq!(&bytes[24..28], &44_100u32.to_le_bytes());
        assert_eq!(&bytes[28..32], &176_400u32.to_le_bytes());
        assert_eq!(&bytes[32..34], &4u16.to_le_bytes());
        assert_eq!(&bytes[34..36], &16u16.to_le_bytes());
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(&bytes[40..44], &(data_len as u32).to_le_bytes());
    }

    #[test]
    fn a_known_payload_encodes_to_fixed_bytes() {
        // Proved without any reader: full scale, its negative, half scale rounded, zero.
        let (bytes, _) = encode(
            &[1.0, -1.0, 0.5, 0.0],
            1,
            48_000,
            Target::Wav(Bits::Sixteen),
        )
        .expect("encodes");
        assert_eq!(
            &bytes[44..],
            &[0xFF, 0x7F, 0x01, 0x80, 0x00, 0x40, 0x00, 0x00]
        );
    }

    #[test]
    fn aiff_is_plain_aiff_with_big_endian_samples() {
        let (bytes, _) =
            encode(&[0.5, -0.5], 1, 44_100, Target::Aiff(Bits::Sixteen)).expect("encodes");
        assert_eq!(&bytes[0..4], b"FORM");
        assert_eq!(&bytes[8..12], b"AIFF", "AIFF, not AIFF-C");
        let ssnd = bytes
            .windows(4)
            .position(|w| w == b"SSND")
            .expect("a sound data chunk");
        // SSND: id, size, offset, block size, then samples. 0.5 → 16384 = 0x4000, big-endian.
        assert_eq!(&bytes[ssnd + 16..ssnd + 18], &[0x40, 0x00]);
    }

    #[test]
    fn flac_output_is_a_flac_stream() {
        let (bytes, _) = encode(
            &sine(4_096, 2, 0.7),
            2,
            48_000,
            Target::Flac(Bits::TwentyFour),
        )
        .expect("encodes");
        assert_eq!(&bytes[0..4], b"fLaC");
    }

    #[test]
    fn a_float_wav_keeps_samples_bit_exact() {
        let signal = sine(512, 2, 1.3);
        let (bytes, encoded) = encode(&signal, 2, 96_000, Target::WavFloat32).expect("encodes");
        assert_eq!(encoded.clipped, 0);
        let data = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("a data chunk")
            + 8;
        let back: Vec<f32> = bytes[data..]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert_eq!(back, signal);
    }

    #[test]
    fn a_write_that_fails_leaves_the_previous_file_untouched() {
        let dir =
            std::env::temp_dir().join(format!("mxm-audio-file-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        let path = dir.join("take.wav");
        write(&path, &[0.25; 8], 2, 48_000, Target::Wav(Bits::Sixteen)).expect("the first write");
        let before = std::fs::read(&path).expect("read back");
        assert!(write(&path, &[f32::NAN; 8], 2, 48_000, Target::Wav(Bits::Sixteen)).is_err());
        assert_eq!(std::fs::read(&path).expect("still there"), before);
        assert!(
            !dir.join("take.wav.tmp").exists(),
            "no temporary file is left behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
