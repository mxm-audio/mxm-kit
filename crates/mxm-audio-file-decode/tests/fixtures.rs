//! The formats no Rust crate here can write — MP3, AAC and ALAC in M4A, Ogg Vorbis — decoded from
//! committed fixtures. Every fixture is our own work: one synthesised stereo sweep, rendered by
//! ffmpeg with the commands in `fixtures/README.md`. No third-party audio. **A sweep because offsets
//! are measured here**: a steady tone matches itself one period late, which is how the first set
//! reported a 2,205-frame "lag" at a correlation of 1.000000.

use mxm_audio_file_decode::{
    AtLimit, Codec, Container, Decoded, Error, Keep, Limits, decode, decode_file,
};
use std::io::Cursor;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn limits() -> Limits {
    Limits::new(1_000_000, AtLimit::Refuse, Keep::AllUpTo(2))
}

fn open(name: &str) -> Decoded {
    decode_file(fixture(name), &limits()).unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// Left channel of an interleaved stereo buffer.
fn left(decoded: &Decoded) -> Vec<f32> {
    decoded
        .interleaved
        .chunks_exact(2)
        .map(|frame| frame[0])
        .collect()
}

/// The lag, within ±`reach` frames, at which `candidate` best matches `reference`, and the normalised
/// correlation there. Positive lag means `candidate` starts late.
fn best_lag(reference: &[f32], candidate: &[f32], reach: isize) -> (isize, f64) {
    let span = reference
        .len()
        .min(candidate.len())
        .saturating_sub(reach.unsigned_abs() * 2);
    let mut best = (0, f64::MIN);
    for lag in -reach..=reach {
        let (mut dot, mut rr, mut cc) = (0.0f64, 0.0f64, 0.0f64);
        for (n, &r) in reference
            .iter()
            .enumerate()
            .take(span)
            .skip(reach.unsigned_abs())
        {
            let Some(&c) = candidate.get((n as isize + lag) as usize) else {
                continue;
            };
            let r = f64::from(r);
            let c = f64::from(c);
            dot += r * c;
            rr += r * r;
            cc += c * c;
        }
        let correlation = dot / (rr.sqrt() * cc.sqrt()).max(f64::MIN_POSITIVE);
        if correlation > best.1 {
            best = (lag, correlation);
        }
    }
    best
}

#[test]
fn the_reference_wav_is_what_the_fixtures_were_made_from() {
    let reference = open("reference-s16.wav");
    assert_eq!(reference.container, Container::Wav);
    assert_eq!(reference.codec, Codec::Pcm);
    assert_eq!((reference.sample_rate, reference.channels), (44_100, 2));
    assert_eq!(reference.frames(), 22_050);
    assert_eq!(reference.declared_frames, Some(22_050));
}

#[test]
fn alac_in_m4a_decodes_to_exactly_the_reference() {
    let reference = open("reference-s16.wav");
    let alac = open("tone-alac.m4a");
    assert_eq!(alac.container, Container::Mp4);
    assert_eq!(alac.codec, Codec::Alac);
    assert!(!alac.codec.is_lossy());
    assert_eq!(alac.sample_rate, 44_100);
    assert_eq!(alac.frames(), reference.frames());
    assert_eq!(
        alac.interleaved, reference.interleaved,
        "lossless means every sample"
    );
}

#[test]
fn mp3_with_an_id3v2_tag_decodes_gapless_at_zero_offset() {
    let reference = open("reference-s16.wav");
    let mp3 = open("tone.mp3");
    assert_eq!(mp3.container, Container::Mp3);
    assert_eq!(mp3.codec, Codec::Mp3);
    assert!(mp3.codec.is_lossy() && mp3.gapless);
    let (lag, correlation) = best_lag(&left(&reference), &left(&mp3), 3_000);
    eprintln!(
        "mp3: {} frames, lag {lag}, correlation {correlation:.6}",
        mp3.frames()
    );
    assert_eq!(lag, 0, "encoder delay was not trimmed");
    assert!(correlation > 0.999, "correlation {correlation}");
}

#[test]
fn an_mp3_without_a_lame_tag_keeps_its_priming_and_says_it_is_not_gapless() {
    // The same encode as `tone.mp3` without its Xing/LAME header: nothing declares the encoder delay, so
    // symphonia trims nothing. **Gapless is a property of the file, not of MP3.**
    let reference = open("reference-s16.wav");
    let mp3 = open("tone-untagged.mp3");
    assert_eq!(mp3.codec, Codec::Mp3);
    assert!(!mp3.gapless);
    let (lag, correlation) = best_lag(&left(&reference), &left(&mp3), 3_000);
    eprintln!(
        "untagged mp3: {} frames, lag {lag}, correlation {correlation:.6}",
        mp3.frames()
    );
    assert!(lag > 0, "the priming was trimmed after all: lag {lag}");
    assert!(correlation > 0.999, "correlation {correlation}");
    assert!(
        mp3.frames() >= reference.frames() + lag as usize,
        "the whole signal follows the priming"
    );
}

#[test]
fn ogg_vorbis_decodes_gapless_at_zero_offset() {
    let reference = open("reference-s16.wav");
    let vorbis = open("tone.ogg");
    assert_eq!(vorbis.container, Container::Ogg);
    assert_eq!(vorbis.codec, Codec::Vorbis);
    assert!(vorbis.codec.is_lossy() && vorbis.gapless);
    let (lag, correlation) = best_lag(&left(&reference), &left(&vorbis), 3_000);
    eprintln!(
        "vorbis: {} frames, lag {lag}, correlation {correlation:.6}",
        vorbis.frames()
    );
    assert_eq!(lag, 0, "encoder delay was not trimmed");
    assert!(correlation > 0.999, "correlation {correlation}");
}

#[test]
fn a_damaged_ogg_page_is_refused_not_skipped() {
    // Code review round 1 (2026-09-15). symphonia's Ogg reader drops a page whose checksum fails and
    // carries on: a damaged middle page returned a second less audio as a success, and a damaged page
    // before the last ended the stream early with no gap. `tone.ogg` holds a single audio page, so
    // losing it leaves nothing — which is why the byte-mutation sweep never saw either.
    let bytes = std::fs::read(fixture("tone-3s.ogg")).expect("the fixture exists");
    let intact = decode(Box::new(Cursor::new(bytes.clone())), Some("ogg"), &limits())
        .expect("the intact file decodes");
    assert_eq!(intact.frames(), 132_300);
    assert_eq!(intact.declared_frames, Some(132_300));

    let pages: Vec<usize> = bytes
        .windows(4)
        .enumerate()
        .filter(|(_, window)| *window == b"OggS")
        .map(|(at, _)| at)
        .collect();
    assert!(pages.len() >= 5, "several audio pages: {pages:?}");
    // One byte in the middle of each audio page after the first: page 0 is the identification header,
    // page 1 the comment and setup headers, page 2 the first audio.
    for page in 3..pages.len() {
        let end = pages.get(page + 1).copied().unwrap_or(bytes.len());
        let mut damaged = bytes.clone();
        damaged[(pages[page] + end) / 2] ^= 0x5A;
        match decode(Box::new(Cursor::new(damaged)), Some("ogg"), &limits()) {
            Err(Error::Damaged(_)) => {}
            other => panic!(
                "page {page} damaged decoded as {:?}",
                other.map(|d| d.frames())
            ),
        }
    }
}

#[test]
fn a_vorbis_stream_that_does_not_start_at_zero_is_whole() {
    // Code review round 2 (2026-09-15). `tone-3s.ogg`'s own pages, rewritten by ffmpeg `-c copy`:
    // `-output_ts_offset 2` starts at 88,072 and ends at granule 220,500, and was refused as truncated;
    // `-ss 1` starts at -44,228 and ends at 88,200, and reported 88,200 as its length. Same packets, so
    // each is exactly the original's 132,300 frames.
    for name in ["tone-3s.ogg", "tone-3s-offset.ogg", "tone-3s-cut.ogg"] {
        let decoded = open(name);
        assert_eq!(decoded.frames(), 132_300, "{name}");
        assert_eq!(decoded.declared_frames, Some(132_300), "{name}");
    }
    // A capped decode reports the same playable length.
    let capped = decode_file(
        fixture("tone-3s-offset.ogg"),
        &Limits::new(44_100, AtLimit::Stop, Keep::AllUpTo(2)),
    )
    .expect("stops at the limit");
    assert!(capped.more_existed);
    assert_eq!(capped.declared_frames, Some(132_300));
}

/// Where each MPEG-1 Layer III frame starts, past any ID3v2 tag.
fn mpeg_frames(bytes: &[u8]) -> Vec<usize> {
    const KBPS: [usize; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const RATES: [usize; 4] = [44_100, 48_000, 32_000, 0];
    let mut at = 0;
    if bytes.starts_with(b"ID3") {
        at = 10
            + (usize::from(bytes[6]) << 21
                | usize::from(bytes[7]) << 14
                | usize::from(bytes[8]) << 7
                | usize::from(bytes[9]));
    }
    let mut frames = Vec::new();
    while at + 4 <= bytes.len() {
        if bytes[at] == 0xFF && bytes[at + 1] & 0xFE == 0xFA {
            let kbps = KBPS[usize::from(bytes[at + 2] >> 4)];
            let rate = RATES[usize::from((bytes[at + 2] >> 2) & 3)];
            if kbps > 0 && rate > 0 {
                frames.push(at);
                at += 144_000 * kbps / rate + usize::from((bytes[at + 2] >> 1) & 1);
                continue;
            }
        }
        at += 1;
    }
    frames
}

#[test]
fn a_frame_lost_from_a_tagged_mp3_is_refused() {
    // Code review round 2 (2026-09-15). symphonia's MP3 reader resyncs past a broken frame header and
    // keeps the timestamps contiguous. Breaking the sync of `tone.mp3`'s frames 15, 17 or 21 decoded
    // 21,935 of 22,050 frames as `Ok`, and one break decoded longer. Its LAME tag and Xing count bind the
    // length exactly, so no broken frame may decode to a different length.
    let bytes = std::fs::read(fixture("tone.mp3")).expect("the fixture exists");
    let whole = open("tone.mp3");
    let frames = mpeg_frames(&bytes);
    assert!(frames.len() > 20, "{} frames", frames.len());
    let mut refused = 0;
    // Frame 0 is the Xing/LAME frame. Breaking frame 1 moves the declaration with the audio — the length
    // decoded and the length declared change together — so no length rule can see it. Asserted as
    // exactly that, so a change in symphonia shows here.
    for (index, &at) in frames.iter().enumerate().skip(1) {
        let mut damaged = bytes.clone();
        damaged[at] = 0x00;
        match decode(Box::new(Cursor::new(damaged)), Some("mp3"), &limits()) {
            Err(_) => refused += 1,
            Ok(decoded) if index == 1 => {
                assert_eq!(decoded.declared_frames, Some(decoded.frames() as u64));
            }
            Ok(decoded) => assert_eq!(
                decoded.frames(),
                whole.frames(),
                "frame {index} broken decoded to another length"
            ),
        }
    }
    assert!(refused > 0);
}

#[test]
fn aac_in_m4a_decodes_and_says_it_is_not_gapless() {
    // **Recorded, not asserted to be zero** (plan §5): symphonia's ISO/MP4 demuxer and AAC decoder
    // are "Gapless: No", so the encoder's priming is still in the audio.
    let reference = open("reference-s16.wav");
    let aac = open("tone-aac.m4a");
    assert_eq!(aac.container, Container::Mp4);
    assert_eq!(aac.codec, Codec::Aac);
    assert!(aac.codec.is_lossy() && !aac.gapless);
    let (lag, correlation) = best_lag(&left(&reference), &left(&aac), 3_000);
    eprintln!(
        "aac: {} frames, lag {lag}, correlation {correlation:.6}",
        aac.frames()
    );
    assert!(
        correlation > 0.99,
        "the audio is the reference, wherever it starts: {correlation}"
    );
}

#[test]
fn opus_is_refused_as_an_unsupported_codec() {
    // The owner's ruling of 2026-09-15: no Opus import.
    match decode_file(fixture("tone.opus"), &limits()) {
        Err(Error::UnsupportedCodec(codec)) => assert!(codec.contains("Opus"), "{codec}"),
        other => panic!("Opus was not refused as a codec: {other:?}"),
    }
}

#[test]
fn caf_is_not_read_because_symphonia_rates_it_good() {
    assert!(matches!(
        decode_file(fixture("tone.caf"), &limits()),
        Err(Error::NotAudio)
    ));
}

#[test]
fn a_mutated_compressed_file_yields_a_result_or_a_refusal_and_never_escapes() {
    let mut runs = 0usize;
    let mut panicked = 0usize;
    for name in ["tone.mp3", "tone-aac.m4a", "tone-alac.m4a", "tone.ogg"] {
        let bytes = std::fs::read(fixture(name)).expect("the fixture exists");
        let extension = name.rsplit('.').next();
        let step = (bytes.len() / 61).max(1);
        for at in (0..bytes.len()).step_by(step) {
            for value in [0x00u8, 0xFF, 0x55] {
                let mut mutated = bytes.clone();
                mutated[at] = value;
                runs += 1;
                if matches!(
                    decode(Box::new(Cursor::new(mutated)), extension, &limits()),
                    Err(Error::Panicked)
                ) {
                    panicked += 1;
                }
            }
            runs += 1;
            if matches!(
                decode(
                    Box::new(Cursor::new(bytes[..at].to_vec())),
                    extension,
                    &limits()
                ),
                Err(Error::Panicked)
            ) {
                panicked += 1;
            }
        }
    }
    eprintln!("{runs} mutated compressed decodes, {panicked} contained panics");
    assert!(runs > 800);
}
