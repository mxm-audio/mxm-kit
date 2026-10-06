//! Reads audio files for the collection through symphonia: WAV, AIFF, FLAC, ALAC, MP3, AAC in M4A and
//! Ogg Vorbis.
//!
//! | Container | Extensions | Codecs read |
//! |---|---|---|
//! | WAV (RIFF) | `wav`, `wave` | PCM, integer and float |
//! | AIFF, AIFF-C | `aif`, `aiff`, `aifc` | PCM |
//! | FLAC | `flac` | FLAC |
//! | MP4 / M4A | `m4a`, `mp4` | ALAC, AAC-LC — from a seekable source only |
//! | Ogg | `ogg`, `oga` | Vorbis |
//! | MPEG audio | `mp3` | MP3 (MPEG-1, 2 and 2.5 Layer III) |
//!
//! **Refused:** Opus (owner's ruling, 2026-09-15), CAF and ADPCM (rated Good by symphonia), RF64, a
//! chained Ogg. The crate's `NOTES.md` carries the measured detail for each row.
//!
//! **A separate crate from the encoder, not a feature of it.** symphonia is MPL-2.0; Cargo unifies a
//! feature across every package in one build, and `nice_plug_xtask` bundles several packages in one
//! `cargo build`, so a feature would carry the decoder into whatever shared that build. A crate
//! outside a package's dependency graph cannot be linked into it under any grouping.
//! `plans/plan-mxm-audio-file.md` §3.1, in the private archive.
//!
//! **What it returns.** Interleaved `f32` at the file's own rate, for the channels the caller keeps,
//! plus what the file said about itself: rate, channel count, bit depth, codec, container, the frame
//! count the container declared and whether audio existed beyond the caller's limit. **No policy**:
//! no clamping, mixing, normalisation or quantisation. Integer PCM is scaled by `2^(bits−1)` —
//! symphonia's conversion, and the rule the collection's WAV import already used.
//!
//! **What it refuses** (§3.2): a file that is not audio, a codec it does not decode (Opus among them,
//! by the owner's ruling of 2026-09-15), damage the file lets a reader detect, a non-finite sample, a
//! track over the caller's source-channel ceiling or declared packet budget, and — when the caller says
//! so — audio longer than its frame limit. A panic inside a dependency becomes a refusal too.
//!
//! **Detectable damage, not all damage.** A header or packet symphonia reports as malformed, a lossless
//! file shorter than it declares, and **audio a reader skipped**: symphonia's Ogg and FLAC readers step
//! over a page or frame whose checksum fails and carry on, so packet timestamps are checked for a gap.
//! A flipped sample in PCM, or in an MP3 frame without a CRC, carries nothing that could detect it, and
//! decodes as the changed audio.
//!
//! **What it does not contain.** A file built to exhaust memory *inside* symphonia: an encoded packet
//! is allocated at the size the container declares, before this crate sees it, and allocation failure
//! aborts rather than unwinding. Only a separate decoding process would catch that.

use std::fmt;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use symphonia::core::audio::sample::SampleFormat;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_AAC, CODEC_ID_ALAC, CODEC_ID_FLAC, CODEC_ID_MP3, CODEC_ID_OPUS, CODEC_ID_VORBIS,
};
use symphonia::core::errors::Error as Symphonia;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Every extension a file this crate decodes may carry, lower case, without the dot.
///
/// **One list.** A plugin's drop acceptance and its Browse filter both derive from this, so neither
/// spells `wav` on its own again. An extension is a hint for a dialog; the decoder judges a file by
/// its content, so a mislabelled file is still read for what it is.
pub const EXTENSIONS: &[&str] = &[
    "wav", "wave", "aif", "aiff", "aifc", "flac", "mp3", "m4a", "mp4", "ogg", "oga",
];

/// The MPL-2.0 notice for the symphonia code this crate links: the licence, the exact crate versions
/// and where their source is. **Compiled in**, because a bundle is copied alone — a file beside a
/// `.clap` is lost on the documented install — so whatever links the decoder carries its notice.
pub const NOTICE: &str = include_str!("../NOTICE.md");

/// The most source channels a track may declare unless the caller says otherwise. Chosen, not
/// measured: comfortably above 7.1.4 and third-order ambisonics (16), far below `u16::MAX`.
pub const DEFAULT_MAX_SOURCE_CHANNELS: usize = 32;

/// The largest packet a track may declare, in frames, unless the caller says otherwise. Chosen, not
/// measured: FLAC's own block-size field tops out at 65,535, and this leaves room for PCM readers.
pub const DEFAULT_MAX_PACKET_FRAMES: u64 = 1 << 20;

/// What reaching the frame limit means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtLimit {
    /// The file is refused — the sampler's rule for a small-sample instrument.
    Refuse,
    /// Decoding stops at the limit and [`Decoded::more_existed`] says whether audio went on — the
    /// convolution plugin's ten-second truncation, which fades only when it did.
    Stop,
}

/// Which source channels are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    /// The first `n` channels, or all of them when there are fewer.
    First(usize),
    /// Every channel, refusing a file with more than `n`.
    AllUpTo(usize),
}

/// The caller's bounds on one import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The most frames kept.
    pub max_frames: usize,
    pub at_limit: AtLimit,
    pub keep: Keep,
    /// A track declaring more source channels is refused before any packet decodes.
    pub max_source_channels: usize,
    /// A track declaring a larger packet is refused before any packet decodes.
    pub max_packet_frames: u64,
}

impl Limits {
    /// A frame limit, what reaching it means and which channels are kept, with the default ceilings.
    pub fn new(max_frames: usize, at_limit: AtLimit, keep: Keep) -> Self {
        Self {
            max_frames,
            at_limit,
            keep,
            max_source_channels: DEFAULT_MAX_SOURCE_CHANNELS,
            max_packet_frames: DEFAULT_MAX_PACKET_FRAMES,
        }
    }
}

/// The codec the audio was stored with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Pcm,
    Flac,
    Alac,
    Mp3,
    Aac,
    Vorbis,
}

impl Codec {
    /// Whether the stored audio lost information when it was encoded.
    pub fn is_lossy(self) -> bool {
        matches!(self, Codec::Mp3 | Codec::Aac | Codec::Vorbis)
    }
}

/// The container the audio was stored in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    /// RIFF WAVE. A consumer may read the file's own chunk list afterwards — the sampler reads `smpl`.
    Wav,
    Aiff,
    Flac,
    /// ISO base media: M4A, MP4.
    Mp4,
    Ogg,
    /// A bare MPEG audio stream.
    Mp3,
    /// A container symphonia named that this list does not.
    Other(&'static str),
}

/// A decoded file.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    /// Interleaved samples for the kept channels.
    pub interleaved: Vec<f32>,
    /// Channels kept and interleaved.
    pub channels: usize,
    /// Channels the file holds.
    pub source_channels: usize,
    pub sample_rate: u32,
    /// Bits per decoded sample, where the codec states it.
    pub bits_per_sample: Option<u32>,
    /// The file stores floating-point samples. Integer and float at the same width are otherwise
    /// indistinguishable in the result — and the player's export promises float.
    pub float: bool,
    pub codec: Codec,
    /// The decoded audio has had its encoder delay and padding removed, or had none to remove.
    ///
    /// **Per file, not per codec.** symphonia trims an MP3 only when a LAME tag declares the delay, so an
    /// MP3 without one keeps its priming; its AAC decoder never trims ("Gapless: No" in its README).
    /// Vorbis declares its delay in the stream, and lossless codecs have nothing to trim.
    pub gapless: bool,
    pub container: Container,
    /// The playable frame count the container declared, where it declared one. This, not the kept
    /// count, is what a consumer reading container metadata compares loop points against.
    pub declared_frames: Option<u64>,
    /// Decoding stopped at the frame limit and audio went on past it. Only [`AtLimit::Stop`] sets it.
    pub more_existed: bool,
}

impl Decoded {
    /// Frames kept.
    pub fn frames(&self) -> usize {
        self.interleaved.len() / self.channels.max(1)
    }
}

/// Why a file was not imported.
#[derive(Debug)]
pub enum Error {
    /// The file could not be read.
    Io(io::Error),
    /// No format this crate reads recognised the content.
    NotAudio,
    /// The container was recognised but its codec is not decoded here.
    UnsupportedCodec(String),
    /// A header or packet was malformed, audio was skipped, or a lossless file is shorter than it
    /// declares: damage the file lets a reader detect.
    Damaged(String),
    /// Audio longer than the frame limit under [`AtLimit::Refuse`].
    TooLong { max_frames: usize },
    /// More source channels than the caller's ceiling or [`Keep::AllUpTo`] allow.
    TooManyChannels { found: usize, max: usize },
    /// A declared packet larger than the caller's budget.
    PacketTooLarge { declared: u64, max: u64 },
    /// A decoded sample that is not a number.
    NonFinite,
    /// The file holds no audio frames.
    Empty,
    /// The kept audio could not be allocated.
    OutOfMemory,
    /// A dependency panicked while reading this file.
    Panicked,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(error) => write!(f, "could not read the file: {error}"),
            Error::NotAudio => write!(f, "not an audio file this build can read"),
            Error::UnsupportedCodec(codec) => write!(f, "unsupported codec: {codec}"),
            Error::Damaged(what) => write!(f, "the audio is damaged: {what}"),
            Error::TooLong { max_frames } => {
                write!(f, "too long: at most {max_frames} frames are accepted")
            }
            Error::TooManyChannels { found, max } => {
                write!(f, "{found} channels; at most {max} are accepted")
            }
            Error::PacketTooLarge { declared, max } => {
                write!(
                    f,
                    "declares {declared}-frame packets; at most {max} are accepted"
                )
            }
            Error::NonFinite => write!(f, "the audio contains a sample that is not a number"),
            Error::Empty => write!(f, "the file contains no audio"),
            Error::OutOfMemory => write!(f, "not enough memory to hold the audio"),
            Error::Panicked => write!(f, "the decoder failed on this file"),
        }
    }
}

impl std::error::Error for Error {}

/// Decode the file at `path` within `limits`. Off the audio thread, always: this allocates and reads.
pub fn decode_file(path: impl AsRef<Path>, limits: &Limits) -> Result<Decoded, Error> {
    let path = path.as_ref();
    let file = std::fs::File::open(path).map_err(Error::Io)?;
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_owned);
    decode(Box::new(file), extension.as_deref(), limits)
}

/// Decode audio from any seekable source — a file, or bytes in a `Cursor`.
pub fn decode(
    source: Box<dyn symphonia::core::io::MediaSource>,
    extension_hint: Option<&str>,
    limits: &Limits,
) -> Result<Decoded, Error> {
    // **The notice stays in every binary that can decode.** A `const` nothing references is stripped at
    // link time: the first migrated sampler bundle (11,095,552 bytes, 2026-09-15) carried symphonia's
    // code and not one line of `NOTICE.md`. Every import passes through here, so this reference keeps
    // the licence and source location in the artefact until an editor's notice view references it too.
    std::hint::black_box(NOTICE);
    // **A panic is a refusal.** symphonia rates the components enabled here Great or Excellent, and
    // Great is not a promise that no stream panics. This needs panics to unwind; no shipped profile
    // sets `panic = "abort"`.
    catch_unwind(AssertUnwindSafe(|| {
        decode_inner(source, extension_hint, limits)
    }))
    .unwrap_or(Err(Error::Panicked))
}

fn decode_inner(
    source: Box<dyn symphonia::core::io::MediaSource>,
    extension_hint: Option<&str>,
    limits: &Limits,
) -> Result<Decoded, Error> {
    // A seekable source of known size is one symphonia's Ogg reader reads the stream's end from.
    let length_readable = source.is_seekable() && source.byte_len().is_some();
    let stream = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension_hint {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| match error {
            Symphonia::IoError(error) if error.kind() != io::ErrorKind::UnexpectedEof => {
                Error::Io(error)
            }
            _ => Error::NotAudio,
        })?;
    let container = container(format.format_info().short_name);

    let track = format
        .default_track(TrackType::Audio)
        .ok_or(Error::NotAudio)?;
    let track_id = track.id;
    // symphonia's `num_frames` is where the stream **ends**: a frame count only for a stream starting at
    // zero. `delay` and `padding` come from a LAME tag (MP3) or the stream header (Vorbis).
    let declared_end = track.num_frames;
    let start_ts = track.start_ts.get();
    let delay = track.delay;
    let padding = track.padding;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| Error::UnsupportedCodec("not an audio codec".to_owned()))?
        .clone();

    if params.codec == CODEC_ID_OPUS {
        return Err(Error::UnsupportedCodec("Opus".to_owned()));
    }
    // **Refused before any packet decodes**, where the track declares enough to decide.
    if let Some(declared) = params.channels.as_ref().map(|c| c.count()) {
        check_channels(declared, limits)?;
    }
    // No `let` chains in this crate: they are stable from Rust 1.88, and it builds at 1.87.
    if let Some(declared) = params.max_frames_per_packet {
        if declared > limits.max_packet_frames {
            return Err(Error::PacketTooLarge {
                declared,
                max: limits.max_packet_frames,
            });
        }
    }
    let sample_rate = params
        .sample_rate
        .filter(|&rate| rate > 0)
        .ok_or_else(|| Error::Damaged("no sample rate".to_owned()))?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|error| match error {
            Symphonia::Unsupported(what) => Error::UnsupportedCodec(what.to_owned()),
            other => Error::Damaged(other.to_string()),
        })?;
    // Every decoder this build can make that is not one of the named codecs is the PCM crate's.
    let codec = match params.codec {
        id if id == CODEC_ID_MP3 => Codec::Mp3,
        id if id == CODEC_ID_AAC => Codec::Aac,
        id if id == CODEC_ID_VORBIS => Codec::Vorbis,
        id if id == CODEC_ID_FLAC => Codec::Flac,
        id if id == CODEC_ID_ALAC => Codec::Alac,
        _ => Codec::Pcm,
    };
    let gapless = match codec {
        Codec::Aac => false,
        Codec::Mp3 => mp3_gapless(delay, padding, declared_end),
        _ => true,
    };
    let declared_frames = match codec {
        Codec::Vorbis => declared_end.and_then(|end| vorbis_span(end, start_ts, delay)),
        _ => declared_end,
    };
    let length_rule = match codec {
        // A LAME delay with a Xing frame count is exact: every clean tagged MP3 measured decodes to it.
        Codec::Mp3 if delay.is_some() && declared_end.is_some() => LengthRule::Exact,
        Codec::Mp3 | Codec::Aac => LengthRule::Estimate,
        _ => LengthRule::NotShorter,
    };

    let mut interleaved: Vec<f32> = Vec::new();
    let mut scratch: Vec<f32> = Vec::new();
    let mut layout: Option<(usize, usize)> = None; // (source channels, kept channels)
    let mut kept_frames = 0usize;
    let mut more_existed = false;
    // Where the next packet should start: by its duration, and counting its trims as well.
    let mut expected_start: Option<(i64, i64)> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(error) => return Err(damaged(error)),
        };
        if packet.track_id != track_id {
            continue;
        }
        // **Skipped audio is damage.** symphonia's Ogg reader drops a page whose checksum fails and its
        // FLAC reader resyncs past a bad frame, both without an error, so the next packet starts later
        // than the last one ended. Readers disagree on whether `dur` includes trimmed frames — an MP3
        // priming packet's does, a Vorbis one's does not — so either account of the end is continuous.
        // Measured 2026-09-15 on WAV, AIFF, FLAC, ALAC, AAC, MP3 and Vorbis at two rates and lengths:
        // no gap in any clean file; a damaged Ogg and FLAC jumped by exactly the frames they lost.
        let start = packet.pts.get();
        if let Some((by_duration, with_trims)) = expected_start {
            if start != by_duration && start != with_trims {
                if limits.at_limit == AtLimit::Stop && kept_frames == limits.max_frames {
                    // The gap lies past everything kept: the kept audio is whole, and more existed.
                    more_existed = true;
                    break;
                }
                return Err(Error::Damaged(format!(
                    "audio is missing: the stream jumps from {by_duration} to {start}"
                )));
            }
        }
        let end = start.checked_add_unsigned(packet.dur.get());
        let trims = packet.trim_start.get().checked_add(packet.trim_end.get());
        expected_start = match (end, trims) {
            (Some(end), Some(trims)) => end.checked_add_unsigned(trims).map(|with| (end, with)),
            _ => None,
        };
        if expected_start.is_none() {
            return Err(Error::Damaged(
                "a packet timestamp is out of range".to_owned(),
            ));
        }
        let buffer = decoder.decode(&packet).map_err(damaged)?;
        let frames = buffer.frames();
        if frames == 0 {
            continue;
        }
        let source = buffer.spec().channels().count();
        let (source_channels, kept) = match layout {
            Some((known, kept)) if known == source => (known, kept),
            Some(_) => return Err(Error::Damaged("the channel layout changed".to_owned())),
            None => {
                // A track that did not declare its channels is checked on its first buffer.
                let kept = check_channels(source, limits)?;
                layout = Some((source, kept));
                (source, kept)
            }
        };

        if kept_frames == limits.max_frames {
            match limits.at_limit {
                AtLimit::Refuse => {
                    return Err(Error::TooLong {
                        max_frames: limits.max_frames,
                    });
                }
                AtLimit::Stop => {
                    more_existed = true;
                    break;
                }
            }
        }
        let room = limits.max_frames - kept_frames;
        let take = if frames > room {
            if limits.at_limit == AtLimit::Refuse {
                return Err(Error::TooLong {
                    max_frames: limits.max_frames,
                });
            }
            more_existed = true;
            room
        } else {
            frames
        };

        let values = buffer.samples_interleaved();
        scratch.clear();
        scratch
            .try_reserve_exact(values)
            .map_err(|_| Error::OutOfMemory)?;
        scratch.resize(values, 0.0);
        buffer.copy_to_slice_interleaved(&mut scratch[..]);
        interleaved
            .try_reserve(take * kept)
            .map_err(|_| Error::OutOfMemory)?;
        for frame in scratch.chunks_exact(source_channels).take(take) {
            // Every source channel is checked, kept or not: a non-finite sample anywhere in the audio
            // read refuses the file.
            if frame.iter().any(|sample| !sample.is_finite()) {
                return Err(Error::NonFinite);
            }
            interleaved.extend_from_slice(&frame[..kept]);
        }
        kept_frames += take;
        if more_existed {
            break;
        }
    }

    let Some((source_channels, channels)) = layout else {
        return Err(Error::Empty);
    };
    if kept_frames == 0 {
        return Err(Error::Empty);
    }
    check_complete(
        declared_frames,
        kept_frames,
        length_rule,
        more_existed,
        codec == Codec::Vorbis && length_readable,
    )?;
    Ok(Decoded {
        interleaved,
        channels,
        source_channels,
        sample_rate,
        bits_per_sample: params.bits_per_sample,
        float: is_float(&params),
        codec,
        gapless,
        container,
        declared_frames,
        more_existed,
    })
}

/// How far a file's declared length binds the audio decoded from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LengthRule {
    /// An estimate: an MP3 without a LAME tag and Xing count, and AAC.
    Estimate,
    /// Exact, and a reader only ever loses audio: lossless containers and Vorbis.
    NotShorter,
    /// Exact both ways: an MP3 whose LAME tag and Xing count bound it. A lost frame keeps the timestamps
    /// contiguous (symphonia's MP3 reader resyncs silently) and a false sync can decode garbage as audio.
    Exact,
}

/// **A truncated file is refused, not shortened.** Ending short of an exact declared length without the
/// caller's limit having stopped decoding means audio was lost — including a file that happens to end
/// at exactly the caller's limit.
///
/// **Vorbis is held to its length too.** Its length is the last Ogg page's granule position, exact to
/// the frame, and symphonia reads it whenever the source is seekable and sized. Code review round 1
/// (2026-09-15): when the page before last fails its checksum, the reader drops it, never reads the
/// last page, and ends the stream early with no gap in the timestamps — and the length comes back
/// undeclared. So where the length is `required` and undeclared, that is damage.
///
/// **A tagged MP3 is held to exactly its length.** Code review round 2: breaking one frame's sync byte
/// at a time decoded 9 of 193, 19 of 210 and 5 of 128 files short as `Ok`, and a few longer.
fn check_complete(
    declared: Option<u64>,
    kept_frames: usize,
    rule: LengthRule,
    more_existed: bool,
    required: bool,
) -> Result<(), Error> {
    let kept = kept_frames as u64;
    // **Audio beyond an exact declaration is damage wherever decoding stopped** (code review round 3):
    // more kept than declared, or the declared length kept with more still to come. Only a shortfall
    // needs the end of the file to have been reached.
    if let (Some(declared), LengthRule::Exact) = (declared, rule) {
        if kept > declared {
            return Err(Error::Damaged(format!(
                "{kept} frames decoded where {declared} are declared"
            )));
        }
        if more_existed && kept == declared {
            return Err(Error::Damaged(format!(
                "audio continues past the {declared} frames declared"
            )));
        }
    }
    if more_existed {
        return Ok(());
    }
    match (declared, rule) {
        (None, _) if required => Err(Error::Damaged(
            "the stream's end could not be read, so its length is unknown".to_owned(),
        )),
        (Some(declared), LengthRule::NotShorter | LengthRule::Exact) if kept < declared => Err(
            Error::Damaged(format!("truncated: {kept} of {declared} frames")),
        ),
        _ => Ok(()),
    }
}

/// **A Vorbis stream's playable length: its end granule, less its start and its delay.**
///
/// symphonia gives the end as `num_frames` and the priming packet's timestamp as `start_ts`, which is
/// `-delay` for a stream from zero. Code review round 2 (2026-09-15): a stream that starts elsewhere — an
/// ffmpeg `-c copy` cut, a timestamp offset — was refused as truncated. Measured, `end − start − delay`
/// equals the decoded frames on all five cases: 220,500 − (−128) − 128; a `-copyts` cut 220,500 −
/// 44,608 − 1,024 = 174,868; an offset 308,700 − 88,072 − 128 = 220,500; a plain cut 163,170 − (−12,722) −
/// 1,024 = 174,868; and `tone-3s-cut.ogg` 88,200 − (−44,228) − 128 = 132,300. `end − start` alone is
/// wrong by the delay. A span that is not a count is no length.
fn vorbis_span(end: u64, start_ts: i64, delay: Option<u32>) -> Option<u64> {
    let first = i128::from(start_ts) + i128::from(delay.unwrap_or(0));
    u64::try_from(i128::from(end) - first).ok()
}

/// **An MP3 is gapless when symphonia can remove both ends.** A LAME tag's delay trims the start; the
/// padding is trimmed only against a Xing frame count (`symphonia-bundle-mp3` demuxer), so a LAME tag
/// with padding and no count keeps its padding (code review round 2).
fn mp3_gapless(delay: Option<u32>, padding: Option<u32>, declared_end: Option<u64>) -> bool {
    delay.is_some() && (padding.unwrap_or(0) == 0 || declared_end.is_some())
}

/// The ceiling, then the caller's keep rule. Returns the kept channel count.
fn check_channels(source: usize, limits: &Limits) -> Result<usize, Error> {
    if source == 0 {
        return Err(Error::Damaged("no channels".to_owned()));
    }
    if source > limits.max_source_channels {
        return Err(Error::TooManyChannels {
            found: source,
            max: limits.max_source_channels,
        });
    }
    match limits.keep {
        Keep::First(n) => Ok(source.min(n.max(1))),
        Keep::AllUpTo(n) if source > n => Err(Error::TooManyChannels {
            found: source,
            max: n,
        }),
        Keep::AllUpTo(_) => Ok(source),
    }
}

/// Any error symphonia reports after probing refuses the file.
fn damaged(error: Symphonia) -> Error {
    match error {
        Symphonia::Unsupported(what) => Error::UnsupportedCodec(what.to_owned()),
        Symphonia::ResetRequired => Error::Damaged("the stream changed format".to_owned()),
        other => Error::Damaged(other.to_string()),
    }
}

fn container(short_name: &'static str) -> Container {
    let name = short_name.to_ascii_lowercase();
    if name.contains("wav") || name.contains("riff") {
        Container::Wav
    } else if name.contains("aif") {
        Container::Aiff
    } else if name.contains("flac") {
        Container::Flac
    } else if name.contains("mp4") || name.contains("m4a") {
        Container::Mp4
    } else if name.contains("ogg") {
        Container::Ogg
    } else if name.contains("mp3") || name.contains("mpa") {
        Container::Mp3
    } else {
        Container::Other(short_name)
    }
}

/// Whether the stream stores floating-point samples.
///
/// **The codec id, not only the declared sample format.** symphonia's RIFF readers set no
/// `sample_format`: a float WAV or AIFF is a `CODEC_ID_PCM_F32*` or `F64*` codec and nothing else
/// says so, which is how a float WAV first decoded here as `float: false`.
fn is_float(params: &symphonia::core::codecs::audio::AudioCodecParameters) -> bool {
    use symphonia::core::codecs::audio::well_known::{
        CODEC_ID_PCM_F32BE, CODEC_ID_PCM_F32BE_PLANAR, CODEC_ID_PCM_F32LE,
        CODEC_ID_PCM_F32LE_PLANAR, CODEC_ID_PCM_F64BE, CODEC_ID_PCM_F64BE_PLANAR,
        CODEC_ID_PCM_F64LE, CODEC_ID_PCM_F64LE_PLANAR,
    };
    matches!(
        params.sample_format,
        Some(SampleFormat::F32 | SampleFormat::F64)
    ) || [
        CODEC_ID_PCM_F32LE,
        CODEC_ID_PCM_F32LE_PLANAR,
        CODEC_ID_PCM_F32BE,
        CODEC_ID_PCM_F32BE_PLANAR,
        CODEC_ID_PCM_F64LE,
        CODEC_ID_PCM_F64LE_PLANAR,
        CODEC_ID_PCM_F64BE,
        CODEC_ID_PCM_F64BE_PLANAR,
    ]
    .contains(&params.codec)
}

#[cfg(test)]
mod tests;
