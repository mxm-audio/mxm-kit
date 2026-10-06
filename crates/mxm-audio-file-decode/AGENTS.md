# AGENTS.md — crates/mxm-audio-file-decode

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

Reads audio files — WAV, AIFF, FLAC, ALAC, MP3, AAC in M4A and Ogg Vorbis — through symphonia, for
every plugin and app that imports audio. Its sibling [`../mxm-audio-file`](../mxm-audio-file/AGENTS.md)
writes them. Plan: `plans/plan-mxm-audio-file.md` (`plans/plan-mxm-audio-file.md` in the private archive) (revision 6,
approved 2026-09-15).

**What it reads:**

| Container | Extensions | Codecs | Measured (2026-09-15) |
|---|---|---|---|
| WAV (RIFF) | `wav`, `wave` | PCM, integer and float | 16- and 24-bit integer, 32-bit float; a LIST chunk before the audio |
| AIFF, AIFF-C | `aif`, `aiff`, `aifc` | PCM | 16- and 24-bit integer; an ID3 chunk |
| FLAC | `flac` | FLAC | flacenc and ffmpeg files, 1,000- to 4,608-frame blocks |
| MP4 / M4A | `m4a`, `mp4` | ALAC, AAC-LC | plain, faststart and fragmented — **from a seekable source only** |
| Ogg | `ogg`, `oga` | Vorbis | mono to 6 channels, 8 to 96 kHz, streams cut or offset away from zero |
| MPEG audio | `mp3` | Layer III | MPEG-1, 2 and 2.5; CBR, VBR and ABR; ID3v2 and ID3v1 tags |

**Not read:** Opus (the owner's ruling), CAF and ADPCM (rated Good by symphonia), RF64 (no symphonia
reader), a chained Ogg, and MP4 from a source that cannot seek. The extension list is `EXTENSIONS`.

# Ownership

| Path | Scope |
|---|---|
| `src/lib.rs` | `decode_file`, `decode`, `Limits`, `AtLimit`, `Keep`, `Decoded`, `Codec`, `Container`, `Error`, `EXTENSIONS`, `NOTICE`, panic containment and the bounds |
| `src/tests.rs` | Round trips through the encoder, limits, channels, refusals (a NaN in a dropped channel, a skipped FLAC frame, the truncation, Vorbis span, MP3 length and MP3 gapless rules) and the byte-mutation sweep |
| `tests/fixtures.rs` | MP3 with and without a LAME tag, AAC, ALAC, Vorbis, a damaged three-second Ogg and two re-timed copies of it, a lost frame in a tagged MP3, Opus and CAF, from the committed fixtures |
| `fixtures/` | Those fixtures and the commands that made them. **Our own work** — see below |
| `NOTICE.md` | The MPL-2.0 notice compiled into every binary that links this crate |

**Consumers (2026-09-15):** `mxm-creative-sampler` (normal dependency), and as a test-only
dev-dependency `mxm-player` and the DSP crates whose listening demos read their files back.
`mxm-fx-convolution` has not adopted it yet; the editor work that blocked it landed on `main` (`fda4f94`).

Does **not** own: any consumer's length limit, channel choice, folding, quantisation, state, or what
it does with container metadata. The sampler's `smpl` loop reading stays in the sampler.

# Local Contracts

## A second crate, not a feature — MPL-2.0 stays out of export-only graphs

symphonia is MPL-2.0, accepted in shipped plugins by the owner on 2026-09-15, used unmodified. Cargo
unifies a feature across every package in one build and `nice_plug_xtask` bundles several packages in
one `cargo build`, so a decode feature on the encoder would leak into whatever shared the build. **A
crate absent from a package's normal graph cannot be linked into it.** Only a consumer that decodes
takes this crate. **Never vendor, patch or modify symphonia**; work around a defect outside it or
report it upstream.

## The notice travels in the binary

`NOTICE` is `include_str!("../NOTICE.md")`: the licence, the exact crate versions and where the source
is. A bundle is copied alone (`plugins/AGENTS.md`, factory presets), so a notice beside it would be
lost. **When the symphonia version or feature list changes, update `NOTICE.md` in the same commit** —
the crate list is `cargo tree -e normal -p mxm-audio-file-decode | grep symphonia`.

**A `const` nothing references is stripped, so `decode()` references it.** Measured 2026-09-15: the
first migrated sampler bundle (11,095,552 bytes) carried symphonia's code — `symphonia-bundle-mp3`
six times — and **not one line of `NOTICE.md`**. `decode()` is the entry every import passes through,
and `std::hint::black_box(NOTICE)` there keeps the text: the rebuilt bundle (11,096,576 bytes) holds
`Mozilla Public License`, the source URL and the header once each. **Do not remove that line** until a
consumer's editor references `NOTICE` itself. **The visible notice is still pending**: the app-bar view
the plan assigns to `mxm-preset` is not built (the editor work it waited on has landed), so today the
binary carries the notice and no screen shows it.

## Only components symphonia rates Great or Excellent

symphonia's "Good" means "some streams may panic, error, or produce audible glitches". Enabled: `wav
aiff isomp4 ogg pcm flac alac aac mp3 vorbis`. **CAF and ADPCM are Good and stay out** until upstream
re-rates them. `id3v2` is not needed: an MP3 with an ID3v2.3 tag in front probes and decodes.

## What is bounded, and what is not

- **The caller's frame limit binds while decoding**, never from a header. `AtLimit::Refuse` refuses one
  frame over; `AtLimit::Stop` keeps the limit and sets `more_existed` only when audio went past it.
- **A source-channel ceiling and a declared packet budget refuse before any packet decodes**, where
  the track declares them; an undeclared channel count is checked on the first buffer.
  `DEFAULT_MAX_SOURCE_CHANNELS` (32) and `DEFAULT_MAX_PACKET_FRAMES` (2²⁰) are chosen, not measured.
- **Kept memory is kept frames × kept channels plus one decoded packet**, allocated fallibly.
- **Not bounded: allocations inside symphonia.** An encoded packet is a `Box<[u8]>` sized by the
  container, and `FormatOptions` has no packet limit. Allocation failure aborts rather than unwinding.
  Only a separate decoding process would contain a file built to exhaust memory there.

## What is refused

Not audio; an unsupported codec (Opus, by the owner's ruling); **damage the file lets a reader
detect** — a header or packet symphonia reports malformed, audio a reader skipped, a file shorter than
it declares; a non-finite sample in **any** source channel, kept or not; too many channels; too long
under `Refuse`; empty. A panic inside a dependency is caught and becomes `Error::Panicked`, which needs
panics to unwind: **no shipped profile may set `panic = "abort"`.**

**Detectable, not all.** A flipped sample in PCM, or in an MP3 frame without a CRC, carries nothing
that could detect it and decodes as the changed audio. Measured 2026-09-15, one byte flipped at 59
points of a 5 s file: WAV 59 decoded changed; MP3 42 changed, 14 identical, 3 refused.

## Skipped audio is found, because symphonia's readers skip silently

symphonia 0.6.1's Ogg reader logs and drops a page whose checksum fails (`symphonia-format-ogg`
`demuxer.rs` `read_page`, `logical.rs`), and its FLAC reader resyncs past a bad frame; neither returns
an error. **Found by code review round 1 (2026-09-15):** one flipped byte returned a 5 s Ogg Vorbis
file a second short as `Ok` for 36 of 59 flips. The committed 0.5 s fixture holds one audio page, so
the mutation sweep, which asserted only "no panic", could not show it. Two checks close it:

- **Packet timestamps must be continuous.** A packet must start where the previous one ended, by
  duration or by duration plus trims — readers disagree on whether `dur` counts trimmed frames (an MP3
  priming packet's does, a Vorbis one's does not). Under `Stop`, a gap after the limit is reached
  means more existed, not damage.
- **A file is held to the length it declares:** lossless containers, Vorbis, and an MP3 whose LAME
  tag and Xing count bound it. **symphonia reports where a stream ends, not how long it is**: a Vorbis
  length is its end granule less its start timestamp and its delay, and until code review round 2 a
  stream cut or offset away from zero was refused as truncated. When the Ogg page before last is
  damaged the reader never reads the last page, the stream ends with no gap, and **the length comes
  back undeclared** — so on a seekable, sized source an undeclared Vorbis length is refused too.
- **A tagged MP3 is held to exactly its length.** symphonia's MP3 reader resyncs past a broken frame
  header and keeps the timestamps contiguous: breaking one frame's sync at a time decoded 9 of 193, 19
  of 210 and 5 of 128 files short as `Ok`, and a few longer. **Undetectable:** breaking the first audio
  frame moves the declaration with the audio — decoded and declared length change together.
- The length checks apply at exactly the caller's limit as well, and **an exact length binds under
  `Stop` too**: more kept than declared, or the declared length kept with more still to come, refuses
  wherever decoding stopped (code review round 3). An untagged MP3 and AAC stay exempt:
  their lengths are estimates.

**Measured after the fix:** the same 59 flips were refused 59 of 59 in Ogg and in FLAC. No clean file
was refused: 36 ffmpeg files (24-bit and float WAV, AIFF, FLAC, ALAC, AAC, MP3 tagged and untagged,
Vorbis; 44.1 and 48 kHz; 1.37 and 5 s), every fixture, and seven more Oggs (0.02 s, mono, 8 and 96 kHz,
lowest quality, piped, a 3,000-byte tag). A chained Ogg is refused as "the stream changed format".
After round 2's fixes, the same sweep, plus 13 more formats read both seekable and through a source
that cannot seek, cut and re-timed Oggs, and `Stop`/`Refuse` at half, exactly and one frame short of
every file's length: results in `.agent-loop/20260915-audio-file-code-review/response-02.md`.

## Float is reported, from the codec id as much as the sample format

`Decoded::float` says whether the file stores floating-point samples — integer and float at the same
width are otherwise indistinguishable in the result, and the player's export promises float.
**symphonia's RIFF readers set no `sample_format`**: a float WAV or AIFF is a `CODEC_ID_PCM_F32*` or
`F64*` codec and nothing else says so. The first implementation read only the sample format and
decoded a float WAV as `float: false`; the round-trip test caught it.

## No policy, and integer scale is `2^(bits−1)`

Interleaved `f32` for the kept channels, first N in order. No clamping, folding or quantisation.
symphonia converts integer PCM by dividing by `2^(bits−1)` (32,768 for 16-bit, 8,388,608 for 24-bit),
the rule the sampler's hound import already used, so a WAV reproduces its canonical state.

## Gapless is per file, and is reported

`Decoded::gapless` says whether encoder delay and padding were removed, or there were none. symphonia
trims an MP3's start **only when a LAME tag declares the delay** and its end padding **only against a
Xing frame count**, trims Vorbis from its stream, and never trims AAC. So an MP3 is gapless when its
delay is declared and its padding is zero or bounded. **Per file, not per codec:** code review round 1 found the first API,
`Codec::is_gapless`, calling every MP3 gapless. **Measured on the fixtures (2026-09-15):** tagged MP3
and Vorbis decode to exactly 22,050 frames at zero offset against the reference (correlation 1.000000
and 0.999736); the same MP3 without its header decodes 24,192 frames starting **1,105 frames late**
(1.000000); AAC decodes 23,552 frames starting **1,024 frames late** (0.999839) — the encoders'
priming, left in.

## The fixtures are our own work

One synthesised stereo sweep, rendered by ffmpeg 8.1.1 with the commands in `fixtures/README.md`. No
recording or third-party audio. The root release checklist's media check finds these files; this
section and that README are its declaration. **A sweep, not a tone**: a steady sine matches itself a
period late, and the first fixture set reported a false 2,205-frame lag at a correlation of 1.000000.

# Work Guidance

- Enabling a format means checking its rating in symphonia's README first, adding a fixture made from
  the same sweep, and updating `EXTENSIONS`, `NOTICE.md` and the fixture README.
- Opus import, if the owner ever wants it, is `symphonia-adapter-libopus`: C, CMake on all three
  platforms, Rust 1.89, and outside the panic containment.
- **Known limits, dropped by the owner (2026-09-15) — not deferred, nothing to pursue:**
  - **An untagged VBR MP3 is shortened by symphonia**, and nothing here detects it. With no Xing or
    LAME header its reader estimates the length from the first frame's bitrate and trims the end to
    match: a 5 s file decoded 194,688 of the 222,336 frames ffprobe counts, as `Ok`. No upstream report,
    no refusal. The fixtures stay CBR so it does not confound them.
  - **A FLAC cut with `ffmpeg -c copy` stays refused as truncated**: its STREAMINFO keeps the original
    total (220,500) while its frames start at sample 55,296 and hold 165,204. Accepting it by first-frame
    timestamp would also accept a FLAC whose first frame was skipped as damage.
- **No `let` chains** — stable from 1.88; this crate builds at 1.87.

# Verification

```bash
cargo test -p mxm-audio-file-decode
cargo clippy -p mxm-audio-file-decode --all-targets
CARGO_TARGET_DIR=target/msrv-1.87 cargo +1.87.0 test -p mxm-audio-file-decode
cargo tree -e normal -p mxm-player | grep -c symphonia   # prints 0: the player takes the encoder only
cargo xtask bundle mxm-creative-sampler --release
grep -a -c "Mozilla Public License" target/bundled/mxm-creative-sampler.clap   # 1: the notice survived linking
```

**Status (2026-09-15, Windows):** 17 unit tests and 12 fixture tests pass on stable and on 1.87; clippy
is silent.
The mutation sweeps ran 1,470 decodes of mutated WAV, AIFF and FLAC and 992 of mutated MP3, AAC, ALAC
and Vorbis, with **no panic** in either. Linux and macOS are not verified here.

# Child DOX Index

No child AGENTS.md files.
