//! Chord theory for a tracker: the qualities the tutti format names, a chord typed and spelled,
//! notes played together named, a chord voiced with voice leading, and a chord stepped through a
//! key's chords or made simpler and richer.
//!
//! newDAWn's chord patterns (its `plans/plan-tracker-chords.md`) and MXM Tracker, the web tracker,
//! both read their chords through this crate, as both read their keys through `mxm-keys`, so the
//! two can't drift (the owner, 2026-10-29). It knows pitch classes and MIDI pitches and nothing of
//! either program's types: **a key is a tonic and a scale as a twelve-bit mask** ([`Key`]), and
//! **spelling takes whether to write flats**, which the caller decides.
//!
//! # What is here
//!
//! - [`Quality`]: tutti's twenty, each with its tones above the root ([`Quality::intervals`]), and
//!   [`Quality::Other`] for one this crate doesn't know, kept as written and never voiced.
//! - [`parse`] and [`spell`]: a chord as text, `Am7/C`, read in the format's spelling or a common
//!   one (`m`, `-`, `M7`, `Δ7`, `ø`, `°`), and written in its short form; `parse(spell(c)) == c`
//!   for a chord of a known quality.
//! - [`name`]: the chord notes played together make, if one does.
//! - [`voice`]: the notes to play, the bass in [`BASS_RANGE`] and at most [`UPPER_VOICES`] upper
//!   voices close within [`VOICING_RANGE`], moving least from the chord before ([`movement`]).
//! - [`next_in_key`], [`simpler`], [`richer`] and [`transpose`]: the steps a tracker's chord cell
//!   takes.
//!
//! With the `serde` feature a [`Quality`] reads and writes the format's strings.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// A pitch class: 0 is C, 11 is B.
pub type PitchClass = u8;

/// A chord's quality: tutti's twenty (tutti 1.0, 6.7), and any other kept as written.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    /// `maj`: 1 3 5.
    Maj,
    /// `min`: 1 ♭3 5.
    Min,
    /// `dim`: 1 ♭3 ♭5.
    Dim,
    /// `aug`: 1 3 ♯5.
    Aug,
    /// `maj7`: 1 3 5 7.
    Maj7,
    /// `min7`: 1 ♭3 5 ♭7.
    Min7,
    /// `7`: 1 3 5 ♭7.
    Dom7,
    /// `min7b5`: 1 ♭3 ♭5 ♭7.
    Min7b5,
    /// `dim7`: 1 ♭3 ♭5 ♭♭7.
    Dim7,
    /// `sus2`: 1 2 5.
    Sus2,
    /// `sus4`: 1 4 5.
    Sus4,
    /// `6`: 1 3 5 6.
    Six,
    /// `min6`: 1 ♭3 5 6.
    Min6,
    /// `9`: 1 3 5 ♭7 9.
    Nine,
    /// `maj9`: 1 3 5 7 9.
    Maj9,
    /// `min9`: 1 ♭3 5 ♭7 9.
    Min9,
    /// `add9`: 1 3 5 9.
    Add9,
    /// `11`: 1 5 ♭7 9 11.
    Eleven,
    /// `13`: 1 3 5 ♭7 9 13.
    Thirteen,
    /// `5`: 1 5.
    Five,
    /// A quality this crate doesn't know, as written: a chord of one keeps its notes, and is never
    /// voiced, named or stepped.
    Other(String),
}

impl Quality {
    /// The twenty this crate knows, in tutti's order, which [`name`] prefers by.
    pub const KNOWN: [Quality; 20] = [
        Quality::Maj,
        Quality::Min,
        Quality::Dim,
        Quality::Aug,
        Quality::Maj7,
        Quality::Min7,
        Quality::Dom7,
        Quality::Min7b5,
        Quality::Dim7,
        Quality::Sus2,
        Quality::Sus4,
        Quality::Six,
        Quality::Min6,
        Quality::Nine,
        Quality::Maj9,
        Quality::Min9,
        Quality::Add9,
        Quality::Eleven,
        Quality::Thirteen,
        Quality::Five,
    ];

    /// Its tones in semitones above the root, ascending, a ninth and beyond past the octave
    /// (`9` is 14); none for an [`Other`](Quality::Other).
    pub fn intervals(&self) -> &'static [u8] {
        match self {
            Quality::Maj => &[0, 4, 7],
            Quality::Min => &[0, 3, 7],
            Quality::Dim => &[0, 3, 6],
            Quality::Aug => &[0, 4, 8],
            Quality::Maj7 => &[0, 4, 7, 11],
            Quality::Min7 => &[0, 3, 7, 10],
            Quality::Dom7 => &[0, 4, 7, 10],
            Quality::Min7b5 => &[0, 3, 6, 10],
            Quality::Dim7 => &[0, 3, 6, 9],
            Quality::Sus2 => &[0, 2, 7],
            Quality::Sus4 => &[0, 5, 7],
            Quality::Six => &[0, 4, 7, 9],
            Quality::Min6 => &[0, 3, 7, 9],
            Quality::Nine => &[0, 4, 7, 10, 14],
            Quality::Maj9 => &[0, 4, 7, 11, 14],
            Quality::Min9 => &[0, 3, 7, 10, 14],
            Quality::Add9 => &[0, 4, 7, 14],
            Quality::Eleven => &[0, 7, 10, 14, 17],
            Quality::Thirteen => &[0, 4, 7, 10, 14, 21],
            Quality::Five => &[0, 7],
            Quality::Other(_) => &[],
        }
    }

    /// Its name in the tutti format (`maj7`, `min7b5`, `7`); an [`Other`](Quality::Other)'s as
    /// written.
    pub fn tutti(&self) -> &str {
        match self {
            Quality::Maj => "maj",
            Quality::Min => "min",
            Quality::Dim => "dim",
            Quality::Aug => "aug",
            Quality::Maj7 => "maj7",
            Quality::Min7 => "min7",
            Quality::Dom7 => "7",
            Quality::Min7b5 => "min7b5",
            Quality::Dim7 => "dim7",
            Quality::Sus2 => "sus2",
            Quality::Sus4 => "sus4",
            Quality::Six => "6",
            Quality::Min6 => "min6",
            Quality::Nine => "9",
            Quality::Maj9 => "maj9",
            Quality::Min9 => "min9",
            Quality::Add9 => "add9",
            Quality::Eleven => "11",
            Quality::Thirteen => "13",
            Quality::Five => "5",
            Quality::Other(text) => text,
        }
    }

    /// The quality the format's `text` names: one of the twenty, else an
    /// [`Other`](Quality::Other) of it.
    pub fn from_tutti(text: &str) -> Quality {
        Quality::KNOWN
            .iter()
            .find(|quality| quality.tutti() == text)
            .cloned()
            .unwrap_or_else(|| Quality::Other(text.to_owned()))
    }

    /// Its short form in a chord's name, after the root: `` for `maj`, `m7` for `min7`.
    pub fn short(&self) -> &str {
        match self {
            Quality::Maj => "",
            Quality::Min => "m",
            Quality::Min7 => "m7",
            Quality::Min7b5 => "m7b5",
            Quality::Min6 => "m6",
            Quality::Min9 => "m9",
            other => other.tutti(),
        }
    }

    /// One of the twenty, not an [`Other`](Quality::Other).
    pub fn is_known(&self) -> bool {
        !matches!(self, Quality::Other(_))
    }

    /// The quality whose tones are exactly `intervals`, ascending from 0, if one is.
    fn of_intervals(intervals: &[u8]) -> Option<Quality> {
        Quality::KNOWN
            .iter()
            .find(|quality| quality.intervals() == intervals)
            .cloned()
    }
}

/// A chord: its root, its quality, and a slash chord's bass when it isn't the root.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// 0 to 11.
    pub root: PitchClass,
    /// What it is above its root.
    pub quality: Quality,
    /// A slash chord's bass, 0 to 11; `None` for the root.
    pub bass: Option<PitchClass>,
}

impl Chord {
    /// A chord in root position.
    pub fn new(root: PitchClass, quality: Quality) -> Self {
        Self {
            root: root % 12,
            quality,
            bass: None,
        }
    }

    /// Its pitch classes, in the order of its quality's intervals, each once.
    pub fn tones(&self) -> Vec<PitchClass> {
        let mut tones = Vec::new();
        for interval in self.quality.intervals() {
            let tone = (self.root + interval) % 12;
            if !tones.contains(&tone) {
                tones.push(tone);
            }
        }
        tones
    }

    /// The pitch class it has in the bass: its slash bass, else its root.
    pub fn lowest(&self) -> PitchClass {
        self.bass.unwrap_or(self.root)
    }
}

/// A key: its tonic and its scale, bit `n` of `scale` set for the note `n` semitones above the
/// tonic, bit 0 always. Every bit set is no key: the chromatic scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    /// 0 to 11.
    pub tonic: PitchClass,
    /// The scale's notes as bits above the tonic.
    pub scale: u16,
}

impl Key {
    /// Every note: no key.
    pub const CHROMATIC: u16 = 0x0FFF;
    /// The major scale.
    pub const MAJOR: u16 = 0b1010_1011_0101;
    /// The natural minor scale.
    pub const MINOR: u16 = 0b0101_1010_1101;

    /// A key, its tonic taken to a pitch class and its scale to twelve bits with the tonic in it.
    pub fn new(tonic: PitchClass, scale: u16) -> Self {
        Self {
            tonic: tonic % 12,
            scale: (scale | 1) & Self::CHROMATIC,
        }
    }

    /// Whether it holds the pitch class.
    pub fn contains(self, pitch_class: PitchClass) -> bool {
        let above = (pitch_class % 12 + 12 - self.tonic) % 12;
        self.scale & (1 << above) != 0
    }

    /// Whether it's no key: every note in it.
    pub fn is_chromatic(self) -> bool {
        self.scale & Self::CHROMATIC == Self::CHROMATIC
    }

    /// Its notes as pitch classes, from the tonic up.
    fn degrees(self) -> Vec<PitchClass> {
        (0..12u8)
            .filter(|above| self.scale & (1 << above) != 0)
            .map(|above| (self.tonic + above) % 12)
            .collect()
    }
}

const SHARPS: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const FLATS: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// A pitch class's name, with sharps or with flats.
pub fn pitch_class_name(pitch_class: PitchClass, flats: bool) -> &'static str {
    let names = if flats { &FLATS } else { &SHARPS };
    names[usize::from(pitch_class % 12)]
}

/// A chord's name, its quality in the short form, with sharps or flats: `Am7/C`, `Bbmaj7`,
/// `F#m7b5`. An [`Other`](Quality::Other) quality is written as it is.
pub fn spell(chord: &Chord, flats: bool) -> String {
    let mut text = String::from(pitch_class_name(chord.root, flats));
    text.push_str(chord.quality.short());
    if let Some(bass) = chord.bass {
        text.push('/');
        text.push_str(pitch_class_name(bass, flats));
    }
    text
}

/// A chord as typed: a root (`A` to `G`, then `#` or `b`, or `♯` or `♭`), a quality in the
/// format's spelling or a common one (the table in [`quality_of`]), and `/` with a bass. None for
/// anything else; the text is read whole, as it is.
pub fn parse(text: &str) -> Option<Chord> {
    let (root, rest) = read_pitch_class(text)?;
    let (quality, bass) = match rest.rsplit_once('/') {
        Some((quality, bass)) => {
            let (bass, after) = read_pitch_class(bass)?;
            if !after.is_empty() {
                return None;
            }
            (quality, Some(bass))
        }
        None => (rest, None),
    };
    Some(Chord {
        root,
        quality: quality_of(quality)?,
        bass,
    })
}

/// A quality as typed after a root, in the format's spelling or a common one:
///
/// | Quality | Typed |
/// |---|---|
/// | `maj` | (nothing), `maj`, `M` |
/// | `min` | `min`, `m`, `-` |
/// | `dim` | `dim`, `°`, `o` |
/// | `aug` | `aug`, `+` |
/// | `maj7` | `maj7`, `M7`, `Δ7`, `Δ` |
/// | `min7` | `min7`, `m7`, `-7` |
/// | `7` | `7` |
/// | `min7b5` | `min7b5`, `m7b5`, `-7b5`, `ø`, `ø7` |
/// | `dim7` | `dim7`, `°7`, `o7` |
/// | `sus2` | `sus2` |
/// | `sus4` | `sus4`, `sus` |
/// | `6` | `6`, `M6` |
/// | `min6` | `min6`, `m6`, `-6` |
/// | `9` | `9` |
/// | `maj9` | `maj9`, `M9`, `Δ9` |
/// | `min9` | `min9`, `m9`, `-9` |
/// | `add9` | `add9`, `add2` |
/// | `11`, `13`, `5` | as written |
pub fn quality_of(text: &str) -> Option<Quality> {
    Some(match text {
        "" | "maj" | "M" => Quality::Maj,
        "min" | "m" | "-" => Quality::Min,
        "dim" | "°" | "o" => Quality::Dim,
        "aug" | "+" => Quality::Aug,
        "maj7" | "M7" | "Δ7" | "Δ" => Quality::Maj7,
        "min7" | "m7" | "-7" => Quality::Min7,
        "7" => Quality::Dom7,
        "min7b5" | "m7b5" | "-7b5" | "ø" | "ø7" => Quality::Min7b5,
        "dim7" | "°7" | "o7" => Quality::Dim7,
        "sus2" => Quality::Sus2,
        "sus4" | "sus" => Quality::Sus4,
        "6" | "M6" => Quality::Six,
        "min6" | "m6" | "-6" => Quality::Min6,
        "9" => Quality::Nine,
        "maj9" | "M9" | "Δ9" => Quality::Maj9,
        "min9" | "m9" | "-9" => Quality::Min9,
        "add9" | "add2" => Quality::Add9,
        "11" => Quality::Eleven,
        "13" => Quality::Thirteen,
        "5" => Quality::Five,
        _ => return None,
    })
}

/// A note's letter and accidental at the start of `text`, and what follows.
fn read_pitch_class(text: &str) -> Option<(PitchClass, &str)> {
    let mut chars = text.chars();
    let letter = match chars.next()? {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, rest) = if let Some(rest) = rest.strip_prefix('#').or(rest.strip_prefix('♯')) {
        (1, rest)
    } else if let Some(rest) = rest.strip_prefix('b').or(rest.strip_prefix('♭')) {
        (11, rest)
    } else {
        (0, rest)
    };
    Some(((letter + shift) % 12, rest))
}

/// The chord notes played together make: a root and a quality whose tones are exactly their
/// pitch classes, the lowest note the bass (a slash where it isn't the root). Root position is
/// preferred, then fewer tones, then [`Quality::KNOWN`]'s order. None when no chord fits.
pub fn name(pitches: &[u8]) -> Option<Chord> {
    let bass = pitches.iter().min()? % 12;
    let mut classes: Vec<PitchClass> = pitches.iter().map(|pitch| pitch % 12).collect();
    classes.sort_unstable();
    classes.dedup();
    let mut best: Option<((bool, usize, usize), Chord)> = None;
    for root in 0..12u8 {
        for (order, quality) in Quality::KNOWN.iter().enumerate() {
            let chord = Chord::new(root, quality.clone());
            let mut tones = chord.tones();
            tones.sort_unstable();
            if tones != classes {
                continue;
            }
            let rank = (root != bass, quality.intervals().len(), order);
            if best.as_ref().is_none_or(|(kept, _)| rank < *kept) {
                let bass = (root != bass).then_some(bass);
                best = Some((rank, Chord { bass, ..chord }));
            }
        }
    }
    best.map(|(_, chord)| chord)
}

/// The bass's range, a pitch each end: the chord's lowest pitch class in this octave.
pub const BASS_RANGE: (u8, u8) = (36, 47);
/// The upper voices' range, a pitch each end.
pub const VOICING_RANGE: (u8, u8) = (48, 79);
/// The most upper voices a chord is given.
pub const UPPER_VOICES: usize = 4;

/// The pitch classes of a chord's upper voices: its tones, and, with more than
/// [`UPPER_VOICES`], the fifth left out first, then the eleventh under a thirteenth, then the
/// ninth; the third or a sus tone and a seventh always stay.
pub fn upper_tones(chord: &Chord) -> Vec<PitchClass> {
    let mut intervals: Vec<u8> = chord.quality.intervals().to_vec();
    for drop in [7, 17, 14] {
        if intervals.len() <= UPPER_VOICES {
            break;
        }
        if drop == 17 && !intervals.contains(&21) {
            continue;
        }
        intervals.retain(|interval| *interval != drop);
    }
    let mut tones = Vec::new();
    for interval in intervals {
        let tone = (chord.root + interval) % 12;
        if !tones.contains(&tone) {
            tones.push(tone);
        }
    }
    tones
}

/// The notes to play for a chord, ascending: its lowest pitch class in [`BASS_RANGE`], then its
/// upper voices ([`upper_tones`]) close, within [`VOICING_RANGE`]. With the chord before it
/// (`previous`, its notes, the lowest its bass), the inversion and octave of the upper voices
/// that move least from its upper voices ([`movement`]), ties to the lower; without one, root
/// position from the range's floor. None for an [`Other`](Quality::Other) quality.
pub fn voice(chord: &Chord, previous: Option<&[u8]>) -> Option<Vec<u8>> {
    if !chord.quality.is_known() {
        return None;
    }
    let bass = BASS_RANGE.0 + (chord.lowest() + 12 - BASS_RANGE.0 % 12) % 12;
    let tones = upper_tones(chord);
    let before: Vec<u8> = previous
        .map(|notes| {
            let mut notes = notes.to_vec();
            notes.sort_unstable();
            if notes.len() > 1 {
                notes.remove(0);
            }
            notes
        })
        .unwrap_or_default();
    let upper = if before.is_empty() {
        let floor = lowest_at_least(chord.root, VOICING_RANGE.0);
        close(&tones, tones_index(&tones, chord.root), floor)
    } else {
        candidates(&tones)
            .into_iter()
            .min_by_key(|upper| (movement(upper, &before), upper[0], upper.clone()))
            .expect("a chord's close voicings fit the range")
    };
    let mut notes = Vec::with_capacity(upper.len() + 1);
    notes.push(bass);
    notes.extend(upper);
    Some(notes)
}

/// How far a voicing moves from the one before: each new voice's distance in semitones to the
/// nearest previous one, plus each previous one's to the nearest new one, so chords of different
/// sizes are measured alike.
pub fn movement(new: &[u8], previous: &[u8]) -> u32 {
    let nearest = |pitch: u8, among: &[u8]| {
        among
            .iter()
            .map(|other| u32::from(pitch.abs_diff(*other)))
            .min()
            .unwrap_or(0)
    };
    new.iter()
        .map(|pitch| nearest(*pitch, previous))
        .sum::<u32>()
        + previous
            .iter()
            .map(|pitch| nearest(*pitch, new))
            .sum::<u32>()
}

/// The lowest pitch of `pitch_class` at or above `floor`.
fn lowest_at_least(pitch_class: PitchClass, floor: u8) -> u8 {
    floor + (pitch_class + 12 - floor % 12) % 12
}

fn tones_index(tones: &[PitchClass], tone: PitchClass) -> usize {
    tones.iter().position(|t| *t == tone).unwrap_or(0)
}

/// The tones sorted by where they fall in an octave above the first, then laid close from the one
/// at `start`, it at `pitch` and each next at the next pitch above of its class.
fn close(tones: &[PitchClass], start: usize, pitch: u8) -> Vec<u8> {
    let first = tones[0];
    let mut circle: Vec<PitchClass> = tones.to_vec();
    circle.sort_by_key(|tone| (tone + 12 - first) % 12);
    let at = circle
        .iter()
        .position(|tone| *tone == tones[start])
        .expect("the start is one of the tones");
    let mut notes = vec![pitch];
    for step in 1..circle.len() {
        let tone = circle[(at + step) % circle.len()];
        let last = *notes.last().expect("a note is laid");
        notes.push(lowest_at_least(tone, last + 1));
    }
    notes
}

/// Every close voicing of the tones within [`VOICING_RANGE`]: each inversion at each octave.
fn candidates(tones: &[PitchClass]) -> Vec<Vec<u8>> {
    let mut all = Vec::new();
    for start in 0..tones.len() {
        let mut pitch = lowest_at_least(tones[start], VOICING_RANGE.0);
        while pitch <= VOICING_RANGE.1 {
            let notes = close(tones, start, pitch);
            if notes.last().is_some_and(|top| *top <= VOICING_RANGE.1) {
                all.push(notes);
            }
            pitch += 12;
        }
    }
    all
}

/// The chord on the next degree of the key, up or down, of the chord's size: two tones a fifth
/// (`5`), three a triad, four a seventh, five or more a ninth, built of the key's own thirds (its
/// own fifth for a `5`), in root position. From a root outside the key, the first degree that
/// way. A degree whose notes make no quality this crate knows is passed; when none does, or the
/// key is chromatic, it's a semitone ([`transpose`]). None for an [`Other`](Quality::Other).
pub fn next_in_key(chord: &Chord, key: Key, up: bool) -> Option<Chord> {
    let size = chord.quality.intervals().len();
    if size == 0 {
        return None;
    }
    let semitone = transpose(chord, if up { 1 } else { -1 });
    if key.is_chromatic() {
        return Some(semitone);
    }
    let degrees = key.degrees();
    let count = degrees.len();
    // The first degree strictly that way from the root, going round the octave.
    let distance = |degree: PitchClass| {
        let (from, to) = if up {
            (chord.root, degree)
        } else {
            (degree, chord.root)
        };
        (to + 24 - from - 1) % 12 + 1
    };
    let mut at = (0..count)
        .min_by_key(|index| distance(degrees[*index]))
        .expect("a key holds its tonic");
    for _ in 0..count {
        if let Some(found) = built_on(&degrees, at, size) {
            return Some(found);
        }
        at = if up {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        };
    }
    Some(semitone)
}

/// The key's own chord: the triad its thirds build on its tonic, as `next_in_key` builds one; a
/// major triad when they make none this crate knows, or the key is chromatic.
pub fn tonic_chord(key: Key) -> Chord {
    if key.is_chromatic() {
        return Chord::new(key.tonic, Quality::Maj);
    }
    built_on(&key.degrees(), 0, 3).unwrap_or_else(|| Chord::new(key.tonic, Quality::Maj))
}

/// The chord of `size` tones the key's thirds build on its degree `at`, if it's one this crate
/// knows.
fn built_on(degrees: &[PitchClass], at: usize, size: usize) -> Option<Chord> {
    let root = degrees[at];
    let steps: &[usize] = match size {
        2 => &[0, 4],
        3 => &[0, 2, 4],
        4 => &[0, 2, 4, 6],
        _ => &[0, 2, 4, 6, 8],
    };
    let intervals: Vec<u8> = steps
        .iter()
        .map(|step| {
            let tone = degrees[(at + step) % degrees.len()];
            let above = (tone + 12 - root) % 12;
            if *step == 8 { above + 12 } else { above }
        })
        .collect();
    Quality::of_intervals(&intervals).map(|quality| Chord::new(root, quality))
}

/// The chord moved by semitones, its quality kept, its bass moved with it.
pub fn transpose(chord: &Chord, semitones: i8) -> Chord {
    let by = |pitch_class: PitchClass| {
        ((i16::from(pitch_class) + i16::from(semitones)).rem_euclid(12)) as u8
    };
    Chord {
        root: by(chord.root),
        quality: chord.quality.clone(),
        bass: chord.bass.map(by),
    }
}

/// A simpler chord on the same root and bass:
///
/// | Quality | Simpler | Richer |
/// |---|---|---|
/// | `5` | `5` | `maj` |
/// | `maj` | `5` | `maj7` |
/// | `maj7` | `maj` | `maj9` |
/// | `maj9` | `maj7` | `maj9` |
/// | `6`, `add9` | `maj` | `maj9` |
/// | `sus2` | `5` | `add9` |
/// | `sus4` | `5` | `11` |
/// | `aug` | `maj` | `aug` |
/// | `min` | `5` | `min7` |
/// | `min7` | `min` | `min9` |
/// | `min6` | `min` | `min9` |
/// | `min9` | `min7` | `min9` |
/// | `7` | `maj` | `9` |
/// | `9` | `7` | `11` |
/// | `11` | `9` | `13` |
/// | `13` | `11` | `13` |
/// | `dim` | `dim` | `dim7` |
/// | `dim7` | `dim` | `dim7` |
/// | `min7b5` | `dim` | `min7b5` |
///
/// A quality at an end stays; an [`Other`](Quality::Other) has neither.
pub fn simpler(chord: &Chord) -> Option<Chord> {
    ladder(chord, true)
}

/// A richer chord on the same root and bass: [`simpler`]'s table, its other column.
pub fn richer(chord: &Chord) -> Option<Chord> {
    ladder(chord, false)
}

fn ladder(chord: &Chord, simpler: bool) -> Option<Chord> {
    use Quality::*;
    let (down, up) = match &chord.quality {
        Five => (Five, Maj),
        Maj => (Five, Maj7),
        Maj7 => (Maj, Maj9),
        Maj9 => (Maj7, Maj9),
        Six | Add9 => (Maj, Maj9),
        Sus2 => (Five, Add9),
        Sus4 => (Five, Eleven),
        Aug => (Maj, Aug),
        Min => (Five, Min7),
        Min7 => (Min, Min9),
        Min6 => (Min, Min9),
        Min9 => (Min7, Min9),
        Dom7 => (Maj, Nine),
        Nine => (Dom7, Eleven),
        Eleven => (Nine, Thirteen),
        Thirteen => (Eleven, Thirteen),
        Dim => (Dim, Dim7),
        Dim7 => (Dim, Dim7),
        Min7b5 => (Dim, Min7b5),
        Other(_) => return None,
    };
    Some(Chord {
        quality: if simpler { down } else { up },
        ..chord.clone()
    })
}

#[cfg(feature = "serde")]
mod serde_impl {
    use super::Quality;

    impl serde::Serialize for Quality {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(self.tutti())
        }
    }

    impl<'de> serde::Deserialize<'de> for Quality {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let text = <String as serde::Deserialize>::deserialize(deserializer)?;
            Ok(Quality::from_tutti(&text))
        }
    }
}
