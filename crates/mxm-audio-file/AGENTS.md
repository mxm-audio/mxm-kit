# AGENTS.md — crates/mxm-audio-file

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

Writes audio files — WAV, AIFF and FLAC — for every plugin, app, offline tool and listening harness
in the collection. Its sibling [`../mxm-audio-file-decode`](../mxm-audio-file-decode/AGENTS.md) reads
them. Plan: `plans/plan-mxm-audio-file.md` in the private archive (revision 6, approved
2026-09-15).

**Established encoders, not ours.** `hound` writes WAV, `aifc` writes AIFF, `flacenc` writes FLAC.
This crate owns only what those leave to their caller: `f32` to integer, refusing a buffer that is
not audio, reporting what clipped, and writing atomically.

**What it writes:**

| `Target` | File | Samples | Limits |
|---|---|---|---|
| `WavFloat32` | WAV | 32-bit IEEE float | a frame ≤ 65,535 bytes, a byte rate within 32 bits, ≤ 4 GiB of samples |
| `Wav(Bits::Sixteen)`, `Wav(Bits::TwentyFour)` | WAV | 16- or 24-bit integer | as above |
| `Aiff(Bits::Sixteen)`, `Aiff(Bits::TwentyFour)` | AIFF (not AIFF-C) | 16- or 24-bit integer, big-endian | ≤ 32,767 channels; `aifc` refuses a FORM over 4 GiB |
| `Flac(Bits::Sixteen)`, `Flac(Bits::TwentyFour)` | FLAC | 16- or 24-bit integer, lossless | ≤ 8 channels, a rate below 1,048,576 Hz |

Also the `acid` chunk on a finished WAV (`acid::with_acid_chunk`). **Not written:** MP3, AAC, Vorbis and
Opus — lossy export is out by the owner's ruling (2026-09-15) — and ALAC, which no crate the collection
uses writes.

# Ownership

| Path | Scope |
|---|---|
| `src/lib.rs` | `Target`, `Bits`, `quantise`, `encode`, `write`, `write_atomic`, the three encoder adapters, the WAV size bound, the FLAC STREAMINFO correction, and the byte-level tests |
| `src/acid.rs` | The `acid` chunk append, **moved unchanged from the player's `sequencer/wav.rs`**, with its tests |

Does **not** own: decoding (the sibling crate), gain, normalisation, dither, channel folding, or any
consumer's file naming.

# Local Contracts

## No MPL code, and no decoder

Nothing here links symphonia. That is what lets the player and the DSP harnesses take this crate
without taking MPL-2.0 code into their graphs, under any build grouping. **Never add a decoding
dependency here**, even as a dev-dependency a consumer could see; round-trip tests live in the decode
crate, which dev-depends on this one.

## Quantisation is a published definition

`quantise` clamps to `-1.0..=1.0`, scales by `2^(bits−1) − 1` and rounds to nearest. **At 16 bits it
is exactly the rule `mxm_measure::wav::quantise` published before that module retired here**, in
`f32`, so the harnesses that moved here write the bytes they wrote before; `sixteen_bit_quantisation_is_mxm_measures_definition` executes that equivalence.
24 bits is computed in `f64`.

## Every target refuses a non-finite sample; integer targets report clipping

A NaN written as silence or an infinity written as full scale is a plausible file over a DSP failure,
so `encode` refuses the whole buffer and names the sample. **Clipping is reported, never silent**:
`Encoded::clipped` counts samples outside full scale for WAV, AIFF and FLAC at 16 or 24 bits, and is
zero for 32-bit float WAV — which is why the player exports float.

## Writes are atomic

`write` and `write_atomic` write to `<path>.tmp` and rename it over `<path>`, so a failure never leaves
a half-written file looking finished, and a failed write leaves the previous file untouched.

## A WAV's 32-bit sizes are checked before hound sees them

hound 3.5.1 computes the byte rate and counts data bytes in unchecked `u32` (`write.rs`), so a buffer
over 4 GiB, or an absurd rate × channel count, panicked under overflow checks and in a release build
returned a file whose header sizes had wrapped. **Found by code review round 1 (2026-09-15)**; the
player's export and session dump fed hound the same way before this crate existed. `check_shape`
refuses, as `Error::Shape`: a frame over 65,535 bytes, a byte rate over 32 bits, and more sample data
than `u32::MAX − WAV_OVERHEAD_BYTES` (1,024: hound's 68-byte extensible header plus room for an appended
chunk such as `acid`). AIFF needs no bound here: `aifc` refuses an oversized FORM itself.

## One deliberate exception: the `acid` chunk

hound cannot write arbitrary chunks and no crate writes `acid`: `riff` 2.0.0 is generic, so the
24-byte payload would still be ours, and `bwavfile` would replace hound. The player's tested append
therefore moved here unchanged. Swapping working code for a new dependency is the reinvention the
owner ruled out. **This is the only container writing the crate does itself.**

## flacenc's STREAMINFO is corrected after encoding

flacenc 0.5.1 counts the short final block in STREAMINFO's minimum block size. The FLAC format
excludes the last block, and the reference encoder writes min = max for a fixed-blocksize stream.
symphonia 0.6.1 reads min ≠ max as variable-blocksize and rejects every frame-numbered header, so it
**could not open any flacenc file whose length was not a multiple of the block size** — 256, 5,000
and 9,001 frames were refused, 4,096 opened, and ffmpeg decoded all of them. `flac()` sets both
fields to the block size through flacenc's own public `set_block_sizes`. Neither crate is patched.
**No upstream report** — dropped by the owner (2026-09-15). Keep the correction while flacenc is pinned,
and on any flacenc upgrade re-run the decode crate's round trips before removing it.

## hound writes the canonical 44-byte header for 16-bit mono and stereo

hound chooses WAVE_FORMAT_EXTENSIBLE only above two channels or sixteen bits, so a 16-bit mono or
stereo WAV is the plain PCM layout `mxm-measure` wrote; `a_sixteen_bit_stereo_wav_has_the_canonical_pcm_header`
holds it field by field.

# Work Guidance

- A new target is one encoder crate and one `Target` variant. A lossy target (Vorbis, Opus, MP3) is
  **not** added here: every candidate bundles C, and the plan puts each in an encode crate of its own
  so 1.87 encode-only consumers never build it. MP3 through LAME is LGPL — a licensing ruling, not a
  convenience. The owner ruled lossy export out for now (2026-09-15).
- **No `let` chains.** They are stable from Rust 1.88; this crate builds at 1.87, and the 1.87 run
  caught one in `write_atomic` that stable accepted silently.

# Verification

```bash
cargo test -p mxm-audio-file
cargo clippy -p mxm-audio-file --all-targets
CARGO_TARGET_DIR=target/msrv-1.87 cargo +1.87.0 test -p mxm-audio-file   # MSRV is verified, not asserted
cargo test -p mxm-audio-file-decode    # the round trips through every target live there
```

**Status (2026-09-15, Windows):** 16 tests pass on stable and on 1.87; clippy is silent; the round trips in the
decode crate pass for every target. Linux and macOS are not verified here.
*Since the split (2026-10-06):* CI runs this crate's tests on Windows, macOS and Linux on every `v*`
tag; otherwise Linux and macOS are checked later, together.

# Child DOX Index

No child AGENTS.md files.
