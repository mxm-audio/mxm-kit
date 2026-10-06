# AGENTS.md — crates/mxm-audio-file-decode

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

Reads audio files — WAV, AIFF, FLAC, ALAC, MP3, AAC in M4A and Ogg Vorbis — through symphonia, for
every plugin and app that imports audio. Its sibling [`../mxm-audio-file`](../mxm-audio-file/AGENTS.md)
writes them. Plan: `plans/plan-mxm-audio-file.md` in the private archive (revision 6, approved
2026-09-15).

**What it reads** (what each was measured on: [NOTES.md § What was measured per format](NOTES.md#what-was-measured-per-format)):

| Container | Extensions | Codecs |
|---|---|---|
| WAV (RIFF) | `wav`, `wave` | PCM, integer and float |
| AIFF, AIFF-C | `aif`, `aiff`, `aifc` | PCM |
| FLAC | `flac` | FLAC |
| MP4 / M4A | `m4a`, `mp4` | ALAC, AAC-LC — **from a seekable source only** |
| Ogg | `ogg`, `oga` | Vorbis |
| MPEG audio | `mp3` | Layer III |

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

Consumers: `mxm-creative-sampler` (normal dependency); `mxm-player` and DSP crates' listening demos
as a test-only dev-dependency ([NOTES.md § Consumers](NOTES.md#consumers)).

Does **not** own: any consumer's length limit, channel choice, folding, quantisation, state, or what
it does with container metadata. The sampler's `smpl` loop reading stays in the sampler.

# Local Contracts

## A second crate, not a feature — MPL-2.0 stays out of export-only graphs

- symphonia is MPL-2.0, accepted in shipped plugins by the owner (2026-09-15), used unmodified.
  **Never vendor, patch or modify symphonia**; work around a defect outside it or report it upstream.
- Only a consumer that decodes takes this crate. A decode feature on the encoder would leak through
  Cargo's feature unification into every package `nice_plug_xtask` builds together
  ([NOTES.md § Why a second crate](NOTES.md#why-a-second-crate-and-not-a-feature)).

## The notice travels in the binary

- `NOTICE` is `include_str!("../NOTICE.md")`: the licence, the exact crate versions and where the
  source is. A bundle is copied alone (`plugins/AGENTS.md`, factory presets; in full,
  [`docs/plugin-conventions.md`](../../docs/plugin-conventions.md#a-preset-is-parameter-values-and-every-instrument-stores-them-the-same-way)), so a notice beside it
  would be lost.
- **When the symphonia version or feature list changes, update `NOTICE.md` in the same commit** —
  the crate list is `cargo tree -e normal -p mxm-audio-file-decode | grep symphonia`.
- **`decode()` holds `std::hint::black_box(NOTICE)`**, because a `const` nothing references is
  stripped. **Do not remove that line** until a consumer's editor references `NOTICE` itself
  ([NOTES.md § Why `decode()` references the notice](NOTES.md#why-decode-references-the-notice)).
- The visible notice is still pending: the binary carries it and no screen shows it yet.

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

Detectable damage only: a flipped PCM sample, or one in an MP3 frame without a CRC, decodes as the
changed audio ([NOTES.md § Detectable damage](NOTES.md#detectable-damage-measured)).

## Skipped audio is found, because symphonia's readers skip silently

symphonia's Ogg reader drops a page whose checksum fails and its FLAC and MP3 readers resync past a
bad frame, all without an error. Two checks close it
([NOTES.md § How skipped audio was found](NOTES.md#how-skipped-audio-was-found-and-the-checks-in-full)):

- **Packet timestamps must be continuous**: a packet starts where the previous one ended, by duration
  or by duration plus trims. Under `Stop`, a gap after the limit is reached means more existed, not
  damage.
- **A file is held to the length it declares**: lossless containers, Vorbis (its end granule less its
  start timestamp and its delay), and an MP3 whose LAME tag and Xing count bound it — **a tagged MP3
  to exactly its length**. On a seekable, sized source an undeclared Vorbis length is refused too.
  The length checks apply at exactly the caller's limit, and **an exact length binds under `Stop`
  too**. An untagged MP3 and AAC stay exempt: their lengths are estimates.

## Float is reported, from the codec id as much as the sample format

`Decoded::float` says whether the file stores floating-point samples. **symphonia's RIFF readers set
no `sample_format`**: a float WAV or AIFF is known only from its `CODEC_ID_PCM_F32*` or `F64*` codec
([NOTES.md § How float reporting was found](NOTES.md#how-float-reporting-was-found)).

## No policy, and integer scale is `2^(bits−1)`

Interleaved `f32` for the kept channels, first N in order. No clamping, folding or quantisation.
symphonia converts integer PCM by dividing by `2^(bits−1)` (32,768 for 16-bit, 8,388,608 for 24-bit),
the rule the sampler's hound import already used, so a WAV reproduces its canonical state.

## Gapless is per file, and is reported

`Decoded::gapless` says whether encoder delay and padding were removed, or there were none — **per
file, not per codec**. An MP3 is gapless when a LAME tag declares its delay and its padding is zero
or bounded by a Xing frame count; Vorbis is trimmed from its stream; AAC never is
([NOTES.md § Gapless](NOTES.md#gapless-what-symphonia-trims-and-the-measurements)).

## The fixtures are our own work

One synthesised stereo sweep, rendered by ffmpeg with the commands in `fixtures/README.md`; no
recording or third-party audio. This section and that README are the root release checklist's media
declaration. **A sweep, not a tone**: a steady sine matches itself a period late
([NOTES.md § Why the fixtures are a sweep](NOTES.md#why-the-fixtures-are-a-sweep)).

# Work Guidance

- Enabling a format means checking its rating in symphonia's README first, adding a fixture made from
  the same sweep, and updating `EXTENSIONS`, `NOTICE.md` and the fixture README.
- Opus import, if the owner ever wants it: [NOTES.md § Opus, if ever](NOTES.md#opus-if-ever).
- **Known limits, dropped by the owner (2026-09-15) — not deferred, nothing to pursue:** an untagged
  VBR MP3 is shortened by symphonia undetected, and a FLAC cut with `ffmpeg -c copy` stays refused as
  truncated ([NOTES.md § Known limits](NOTES.md#known-limits-the-owner-dropped)).
- **No `let` chains** — stable from 1.88; this crate builds at 1.87.

# Verification

```bash
cargo test -p mxm-audio-file-decode
cargo clippy -p mxm-audio-file-decode --all-targets
CARGO_TARGET_DIR=target/msrv-1.87 cargo +1.87.0 test -p mxm-audio-file-decode
cargo tree -e normal -p mxm-player | grep -c symphonia   # in mxm-player; prints 0: the player takes the encoder only
cargo xtask bundle mxm-creative-sampler --release        # in mxm-creative-sampler, as is the next line
grep -a -c "Mozilla Public License" target/bundled/mxm-creative-sampler.clap   # 1: the notice survived linking
```

Last recorded run: [NOTES.md § Verification status](NOTES.md#verification-status).

# Child DOX Index

No child AGENTS.md files.
