//! What the decoder promises, proved through the collection's own encoder where a file can be written
//! in Rust, and against committed fixtures where it cannot (`tests/fixtures.rs`).

use super::*;
use mxm_audio_file::{Bits, Target, encode};
use std::io::Cursor;

fn from_bytes(bytes: Vec<u8>, extension: &str, limits: &Limits) -> Result<Decoded, Error> {
    decode(Box::new(Cursor::new(bytes)), Some(extension), limits)
}

fn roomy(keep: Keep) -> Limits {
    Limits::new(1_000_000, AtLimit::Refuse, keep)
}

/// A deterministic two-tone signal, with a sample exactly at each full scale.
fn signal(frames: usize, channels: u16) -> Vec<f32> {
    let mut out: Vec<f32> = (0..frames * usize::from(channels))
        .map(|i| {
            let channel = i % usize::from(channels);
            ((i / usize::from(channels)) as f32 * (0.011 + 0.007 * channel as f32)).sin() * 0.8
        })
        .collect();
    out[0] = 1.0;
    out[1] = -1.0;
    out
}

/// What an integer file decodes to: the quantised code over `2^(bits−1)`, symphonia's scale.
fn as_decoded(sample: f32, bits: Bits) -> f32 {
    let code = mxm_audio_file::quantise(sample, bits).expect("finite");
    match bits {
        Bits::Sixteen => code as f32 / 32_768.0,
        Bits::TwentyFour => code as f32 / 8_388_608.0,
    }
}

#[test]
fn every_lossless_target_round_trips_exactly() {
    let channels = 2;
    let source = signal(9_001, channels);
    let cases = [
        (Target::WavFloat32, "wav", Container::Wav),
        (Target::Wav(Bits::Sixteen), "wav", Container::Wav),
        (Target::Wav(Bits::TwentyFour), "wav", Container::Wav),
        (Target::Aiff(Bits::Sixteen), "aiff", Container::Aiff),
        (Target::Aiff(Bits::TwentyFour), "aif", Container::Aiff),
        (Target::Flac(Bits::Sixteen), "flac", Container::Flac),
        (Target::Flac(Bits::TwentyFour), "flac", Container::Flac),
    ];
    for (target, extension, container) in cases {
        let (bytes, _) = encode(&source, channels, 44_100, target).expect("encodes");
        let decoded = from_bytes(bytes, extension, &roomy(Keep::AllUpTo(2)))
            .unwrap_or_else(|error| panic!("{target:?} does not decode: {error}"));
        assert_eq!(decoded.container, container, "{target:?}");
        assert_eq!(decoded.sample_rate, 44_100, "{target:?}");
        assert_eq!(decoded.float, target == Target::WavFloat32, "{target:?}");
        assert_eq!(decoded.channels, 2, "{target:?}");
        assert_eq!(decoded.frames(), 9_001, "{target:?}");
        assert!(!decoded.more_existed, "{target:?}");
        assert!(!decoded.codec.is_lossy() && decoded.gapless, "{target:?}");
        let expected: Vec<f32> = match target {
            Target::WavFloat32 => source.clone(),
            Target::Wav(bits) | Target::Aiff(bits) | Target::Flac(bits) => {
                source.iter().map(|&s| as_decoded(s, bits)).collect()
            }
        };
        assert_eq!(decoded.interleaved, expected, "{target:?} moved a sample");
    }
}

#[test]
fn a_file_is_judged_by_its_content_not_its_extension() {
    let (bytes, _) =
        encode(&signal(256, 1), 1, 48_000, Target::Flac(Bits::Sixteen)).expect("encodes");
    let decoded = from_bytes(bytes, "wav", &roomy(Keep::First(2))).expect("a FLAC named .wav");
    assert_eq!(decoded.container, Container::Flac);
    assert_eq!(decoded.codec, Codec::Flac);

    let text = b"This is a text file wearing a .wav extension, long enough to probe.".repeat(8);
    assert!(matches!(
        from_bytes(text, "wav", &roomy(Keep::First(2))),
        Err(Error::NotAudio)
    ));
    assert!(from_bytes(Vec::new(), "wav", &roomy(Keep::First(2))).is_err());
}

#[test]
fn refuse_at_the_limit_refuses_one_frame_over_and_accepts_exactly_the_limit() {
    let (bytes, _) =
        encode(&signal(1_000, 1), 1, 48_000, Target::Wav(Bits::Sixteen)).expect("encodes");
    let at = |max| Limits::new(max, AtLimit::Refuse, Keep::First(2));
    let exact =
        from_bytes(bytes.clone(), "wav", &at(1_000)).expect("exactly the limit is accepted");
    assert_eq!(exact.frames(), 1_000);
    assert!(!exact.more_existed);
    assert!(matches!(
        from_bytes(bytes, "wav", &at(999)),
        Err(Error::TooLong { max_frames: 999 })
    ));
}

#[test]
fn stop_at_the_limit_keeps_the_limit_and_says_whether_more_existed() {
    let source = signal(1_000, 2);
    let (bytes, _) = encode(&source, 2, 48_000, Target::WavFloat32).expect("encodes");
    let at = |max| Limits::new(max, AtLimit::Stop, Keep::AllUpTo(2));

    let cut = from_bytes(bytes.clone(), "wav", &at(999)).expect("one frame over stops");
    assert_eq!(cut.frames(), 999);
    assert!(cut.more_existed, "a frame went past the limit");
    assert_eq!(&cut.interleaved[..], &source[..999 * 2]);

    let whole = from_bytes(bytes, "wav", &at(1_000)).expect("exactly the limit");
    assert_eq!(whole.frames(), 1_000);
    assert!(!whole.more_existed, "nothing went past an exact limit");
}

#[test]
fn first_n_keeps_the_leading_channels_in_order_and_mono_stays_mono() {
    let channels = 4;
    let source = signal(512, channels);
    let (bytes, _) =
        encode(&source, channels, 48_000, Target::Wav(Bits::TwentyFour)).expect("encodes");
    let decoded = from_bytes(bytes, "wav", &roomy(Keep::First(2))).expect("decodes");
    assert_eq!(decoded.source_channels, 4);
    assert_eq!(decoded.channels, 2);
    let expected: Vec<f32> = source
        .chunks_exact(4)
        .flat_map(|frame| [frame[0], frame[1]])
        .map(|s| as_decoded(s, Bits::TwentyFour))
        .collect();
    assert_eq!(decoded.interleaved, expected);

    let (mono, _) = encode(&signal(64, 1), 1, 48_000, Target::Wav(Bits::Sixteen)).expect("encodes");
    let decoded = from_bytes(mono, "wav", &roomy(Keep::First(2))).expect("decodes");
    assert_eq!(
        (decoded.source_channels, decoded.channels),
        (1, 1),
        "no folding happens here"
    );
}

#[test]
fn the_channel_ceiling_and_all_up_to_refuse_before_decoding() {
    let (bytes, _) =
        encode(&signal(64, 4), 4, 48_000, Target::Wav(Bits::Sixteen)).expect("encodes");
    assert!(matches!(
        from_bytes(bytes.clone(), "wav", &roomy(Keep::AllUpTo(2))),
        Err(Error::TooManyChannels { found: 4, max: 2 })
    ));
    let mut tight = roomy(Keep::First(2));
    tight.max_source_channels = 3;
    assert!(matches!(
        from_bytes(bytes, "wav", &tight),
        Err(Error::TooManyChannels { found: 4, max: 3 })
    ));
}

#[test]
fn a_non_finite_float_sample_refuses_the_file() {
    // hound writes whatever bits it is given, so a NaN can be planted where the encoder refuses one.
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut bytes = Vec::new();
    {
        let mut writer = hound::WavWriter::new(Cursor::new(&mut bytes), spec).expect("writer");
        for n in 0..64 {
            writer
                .write_sample(if n == 40 { f32::NAN } else { 0.25 })
                .expect("sample");
        }
        writer.finalize().expect("finalize");
    }
    assert!(matches!(
        from_bytes(bytes, "wav", &roomy(Keep::First(2))),
        Err(Error::NonFinite)
    ));
}

#[test]
fn a_non_finite_sample_in_a_channel_the_caller_drops_still_refuses_the_file() {
    // Three channels, NaN only in the third; `First(2)` never returns it, and the file is still refused.
    let spec = hound::WavSpec {
        channels: 3,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut bytes = Vec::new();
    {
        let mut writer = hound::WavWriter::new(Cursor::new(&mut bytes), spec).expect("writer");
        for n in 0..64 * 3 {
            writer
                .write_sample(if n == 40 * 3 + 2 { f32::NAN } else { 0.25 })
                .expect("sample");
        }
        writer.finalize().expect("finalize");
    }
    assert!(matches!(
        from_bytes(bytes, "wav", &roomy(Keep::First(2))),
        Err(Error::NonFinite)
    ));
}

#[test]
fn audio_a_reader_skips_is_refused_even_when_the_limit_stops_decoding() {
    // symphonia's FLAC reader resyncs past a frame whose checksum fails and says nothing. Under `Refuse`
    // the declared length catches the shortfall; under `Stop` it cannot, because decoding ends before the
    // end of the file. The packet timestamps jump where the frame went missing, in both.
    let source = signal(20_000, 2);
    let (bytes, _) = encode(&source, 2, 48_000, Target::Flac(Bits::Sixteen)).expect("encodes");
    let stop = Limits::new(15_000, AtLimit::Stop, Keep::AllUpTo(2));
    let intact =
        from_bytes(bytes.clone(), "flac", &stop).expect("the intact file stops at the limit");
    assert_eq!(intact.frames(), 15_000);
    assert!(intact.more_existed);

    let mut damaged = bytes;
    let middle = damaged.len() / 2;
    damaged[middle] ^= 0x5A;
    for limits in [stop, roomy(Keep::AllUpTo(2))] {
        match from_bytes(damaged.clone(), "flac", &limits) {
            Err(Error::Damaged(what)) => assert!(what.contains("missing"), "{what}"),
            other => panic!(
                "{:?}: a skipped frame decoded as {:?}",
                limits.at_limit,
                other.map(|d| (d.frames(), d.more_existed))
            ),
        }
    }
}

#[test]
fn a_lossless_file_short_of_its_declaration_is_refused_even_at_exactly_the_limit() {
    // No real reader was found that ends cleanly short of its declaration — a WAV declaring more frames
    // than it holds errors with an unexpected end of file — so the rule is proved on its own.
    use LengthRule::{Estimate, Exact, NotShorter};
    assert!(matches!(
        check_complete(Some(101), 100, NotShorter, false, false),
        Err(Error::Damaged(_))
    ));
    assert!(check_complete(Some(100), 100, NotShorter, false, false).is_ok());
    assert!(
        check_complete(Some(101), 100, NotShorter, true, false).is_ok(),
        "the limit stopped it"
    );
    assert!(
        check_complete(Some(101), 100, Estimate, false, false).is_ok(),
        "an estimated length binds nothing"
    );
    assert!(check_complete(None, 100, NotShorter, false, false).is_ok());
    // A tagged MP3 is bound both ways.
    assert!(check_complete(Some(100), 100, Exact, false, false).is_ok());
    assert!(check_complete(Some(101), 100, Exact, false, false).is_err());
    assert!(check_complete(Some(99), 100, Exact, false, false).is_err());
    // More past the caller's limit excuses a shortfall, never audio beyond the declaration.
    assert!(check_complete(Some(99), 100, Exact, true, false).is_err());
    assert!(
        check_complete(Some(100), 100, Exact, true, false).is_err(),
        "the whole declared length kept, and more existed"
    );
    assert!(check_complete(Some(101), 100, Exact, true, false).is_ok());
}

#[test]
fn a_vorbis_stream_is_held_to_its_length_where_the_length_can_be_read() {
    // An Ogg granule position is exact, so a short Vorbis stream is truncated like a lossless one.
    assert!(matches!(
        check_complete(Some(101), 100, LengthRule::NotShorter, false, true),
        Err(Error::Damaged(_))
    ));
    // No length from a source whose end the reader could read means the end was damaged.
    assert!(matches!(
        check_complete(None, 100, LengthRule::NotShorter, false, true),
        Err(Error::Damaged(_))
    ));
    // A source that cannot seek never declares one, and that is not damage; nor is a stopped decode.
    assert!(check_complete(None, 100, LengthRule::NotShorter, false, false).is_ok());
    assert!(check_complete(None, 100, LengthRule::NotShorter, true, true).is_ok());
}

#[test]
fn a_vorbis_span_is_its_end_less_its_start_and_its_delay() {
    // The five streams measured in code review round 2, each against the frames it decoded.
    assert_eq!(vorbis_span(220_500, -128, Some(128)), Some(220_500));
    assert_eq!(vorbis_span(220_500, 44_608, Some(1_024)), Some(174_868));
    assert_eq!(vorbis_span(308_700, 88_072, Some(128)), Some(220_500));
    assert_eq!(vorbis_span(163_170, -12_722, Some(1_024)), Some(174_868));
    assert_eq!(vorbis_span(88_200, -44_228, Some(128)), Some(132_300));
    assert_eq!(
        vorbis_span(100, 200, None),
        None,
        "an end before the start is no length"
    );
}

#[test]
fn an_mp3_is_gapless_only_when_both_its_ends_can_be_trimmed() {
    assert!(mp3_gapless(Some(1_105), Some(731), Some(220_500)));
    assert!(
        mp3_gapless(Some(1_105), Some(0), None),
        "no padding to trim"
    );
    assert!(mp3_gapless(Some(1_105), None, None));
    assert!(
        !mp3_gapless(Some(1_105), Some(731), None),
        "padding with no frame count to trim it against stays in"
    );
    assert!(!mp3_gapless(None, None, Some(24_192)), "no LAME tag");
}

#[test]
fn a_truncated_lossless_file_is_refused_rather_than_shortened() {
    for (target, extension) in [
        (Target::Wav(Bits::Sixteen), "wav"),
        (Target::Aiff(Bits::Sixteen), "aiff"),
        (Target::Flac(Bits::Sixteen), "flac"),
    ] {
        let (mut bytes, _) = encode(&signal(20_000, 2), 2, 48_000, target).expect("encodes");
        bytes.truncate(bytes.len() * 2 / 3);
        let result = from_bytes(bytes, extension, &roomy(Keep::First(2)));
        assert!(
            result.is_err(),
            "{target:?}: a file cut to two thirds decoded as {:?} frames",
            result.map(|d| d.frames())
        );
    }
}

#[test]
fn a_mutated_file_yields_a_result_or_a_refusal_and_never_escapes() {
    // Byte mutations and truncations across each Rust-writable container. Reaching the assertions
    // at all is the property: a panic that escaped `decode` would fail the test harness instead.
    let mut panicked = 0usize;
    let mut runs = 0usize;
    for (target, extension) in [
        (Target::Wav(Bits::Sixteen), "wav"),
        (Target::Aiff(Bits::TwentyFour), "aiff"),
        (Target::Flac(Bits::Sixteen), "flac"),
    ] {
        let (bytes, _) = encode(&signal(4_096, 2), 2, 44_100, target).expect("encodes");
        let step = (bytes.len() / 97).max(1);
        for at in (0..bytes.len()).step_by(step) {
            for value in [0x00u8, 0xFF, 0x7F, 0x80] {
                let mut mutated = bytes.clone();
                mutated[at] = value;
                runs += 1;
                if matches!(
                    from_bytes(mutated, extension, &roomy(Keep::First(2))),
                    Err(Error::Panicked)
                ) {
                    panicked += 1;
                }
            }
            let truncated = bytes[..at].to_vec();
            runs += 1;
            if matches!(
                from_bytes(truncated, extension, &roomy(Keep::First(2))),
                Err(Error::Panicked)
            ) {
                panicked += 1;
            }
        }
    }
    eprintln!("{runs} mutated decodes, {panicked} contained panics");
    assert!(runs > 1_000);
}

#[test]
fn the_extension_list_names_every_container_this_crate_reads() {
    for extension in ["wav", "aiff", "aif", "flac", "mp3", "m4a", "ogg"] {
        assert!(EXTENSIONS.contains(&extension), "{extension} is missing");
    }
    assert!(EXTENSIONS.iter().all(|e| {
        e.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }));
    assert!(
        !EXTENSIONS.contains(&"opus"),
        "Opus is out by the owner's ruling"
    );
    assert!(
        !EXTENSIONS.contains(&"caf"),
        "CAF is rated Good, which may panic"
    );
}

#[test]
fn the_notice_names_the_licence_the_version_and_where_the_source_is() {
    assert!(NOTICE.contains("MPL-2.0"));
    assert!(NOTICE.contains("0.6.1"));
    assert!(NOTICE.contains("https://github.com/pdeljanov/Symphonia"));
}
