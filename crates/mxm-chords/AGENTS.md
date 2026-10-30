# AGENTS.md — crates/mxm-chords

Parent: [`../../AGENTS.md`](../../AGENTS.md).

# Purpose

**Chord theory for a tracker**: the qualities the tutti format names, a chord typed and spelled,
notes played together named, a chord voiced with voice leading, and a chord stepped through a
key's chords or made simpler and richer. newDAWn's chord patterns (its
`plans/plan-tracker-chords.md`) and MXM Tracker, the web tracker in WebAssembly, both read their
chords through it, so the two can't drift: an owner-approved exception to evidence-first
extraction (the owner, 2026-10-29: "A crate in mxm-kit"). No dependency but `serde`, behind the
`serde` feature; MSRV **1.87**; it builds for `wasm32-unknown-unknown`.

# Ownership

Owns `src/`, `tests/` and `Cargo.toml`.
- `src/lib.rs`: `Quality`, `Chord`, `Key`; `parse`, `quality_of` (the typed spellings' table),
  `spell`, `pitch_class_name`; `name`; `upper_tones`, `voice`, `movement` and the ranges
  (`BASS_RANGE`, `VOICING_RANGE`, `UPPER_VOICES`); `tonic_chord`, `next_in_key`, `transpose`,
  `simpler`, `richer`; the `serde` strings.
- `tests/chords.rs`: the theory; `tests/serde.rs`: the strings, with the feature.

**Not here:** a host's types (its notes, events, keys or settings), how a chord is shown or which
step a key takes: a host converts its key to a tonic and a twelve-bit mask, and decides flats.

# Local Contracts

- **`parse(spell(chord, flats)) == chord`** for every root, known quality and bass, either way
  (`every_chord_spelled_reads_back_as_itself`). A spelling added to `quality_of` reads, never
  writes: `spell` writes `Quality::short` alone.
- **`Quality::KNOWN` is tutti's twenty in its order**, which `name` prefers by after root
  position and fewer tones; an `Other` keeps its text and is never voiced, named or stepped (each
  answers `None`).
- **A voicing is the bass in `BASS_RANGE`, then at most `UPPER_VOICES` upper voices, close, within
  `VOICING_RANGE`**; with more tones, the fifth goes first, then the eleventh under a thirteenth,
  then the ninth. With a chord before, the inversion and octave least far by `movement`, ties to
  the lower; without one, root position from the range's floor. The ranges are constants until a
  host has voicing settings of its own.
- **`next_in_key` always ends**: the next degree's chord of the chord's size built of the key's
  thirds, in root position; a degree that makes no known quality passed; a semitone when none
  does or the key is chromatic.
- **The ladders are `simpler`'s table**, the root and bass kept; a quality at an end stays.

# Work Guidance

- A new quality is tutti's first: its string, its place in `KNOWN`, its intervals, its short form,
  its typed spellings and its rungs, each table's test growing with it.

# Verification

```bash
cargo test -p mxm-chords --features serde
cargo +1.87.0 test -p mxm-chords --features serde
cargo clippy -p mxm-chords --all-targets --features serde -- -D warnings
cargo build -p mxm-chords --target wasm32-unknown-unknown --features serde
```

# Child DOX Index

None.
