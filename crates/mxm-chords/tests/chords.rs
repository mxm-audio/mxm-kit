//! The chord theory's tests (newDAWn's `plans/plan-tracker-chords.md`, *Tests*): every quality's
//! tones, the names read and written, notes named, voicing and voice leading, a key's chords and
//! the ladders.

use mxm_chords::{
    BASS_RANGE, Chord, Key, Quality, UPPER_VOICES, VOICING_RANGE, movement, name, next_in_key,
    parse, richer, simpler, spell, transpose, upper_tones, voice,
};

fn chord(text: &str) -> Chord {
    parse(text).unwrap_or_else(|| panic!("{text} reads as a chord"))
}

#[test]
fn every_quality_has_its_tones() {
    let tones: Vec<(&str, Vec<u8>)> = Quality::KNOWN
        .iter()
        .map(|quality| (quality.tutti(), quality.intervals().to_vec()))
        .collect();
    let expected: Vec<(&str, Vec<u8>)> = vec![
        ("maj", vec![0, 4, 7]),
        ("min", vec![0, 3, 7]),
        ("dim", vec![0, 3, 6]),
        ("aug", vec![0, 4, 8]),
        ("maj7", vec![0, 4, 7, 11]),
        ("min7", vec![0, 3, 7, 10]),
        ("7", vec![0, 4, 7, 10]),
        ("min7b5", vec![0, 3, 6, 10]),
        ("dim7", vec![0, 3, 6, 9]),
        ("sus2", vec![0, 2, 7]),
        ("sus4", vec![0, 5, 7]),
        ("6", vec![0, 4, 7, 9]),
        ("min6", vec![0, 3, 7, 9]),
        ("9", vec![0, 4, 7, 10, 14]),
        ("maj9", vec![0, 4, 7, 11, 14]),
        ("min9", vec![0, 3, 7, 10, 14]),
        ("add9", vec![0, 4, 7, 14]),
        ("11", vec![0, 7, 10, 14, 17]),
        ("13", vec![0, 4, 7, 10, 14, 21]),
        ("5", vec![0, 7]),
    ];
    assert_eq!(tones, expected);
    assert_eq!(Quality::from_tutti("7#11"), Quality::Other("7#11".into()));
    assert!(Quality::Other("7#11".into()).intervals().is_empty());
}

#[test]
fn every_chord_spelled_reads_back_as_itself() {
    for root in 0..12 {
        for quality in Quality::KNOWN {
            for bass in std::iter::once(None).chain((0..12).map(Some)) {
                let chord = Chord {
                    root,
                    quality: quality.clone(),
                    bass,
                };
                for flats in [false, true] {
                    let text = spell(&chord, flats);
                    assert_eq!(parse(&text), Some(chord.clone()), "{text}");
                }
            }
        }
    }
    assert_eq!(spell(&chord("Am7/C"), false), "Am7/C");
    assert_eq!(spell(&chord("A#maj7"), true), "Bbmaj7");
    assert_eq!(spell(&chord("Gbm7b5"), false), "F#m7b5");
}

#[test]
fn the_common_spellings_read_and_anything_else_does_not() {
    for (typed, spelled) in [
        ("CΔ7", "Cmaj7"),
        ("CΔ", "Cmaj7"),
        ("CM7", "Cmaj7"),
        ("C-7", "Cm7"),
        ("C-", "Cm"),
        ("Cø", "Cm7b5"),
        ("C°", "Cdim"),
        ("C°7", "Cdim7"),
        ("C+", "Caug"),
        ("Csus", "Csus4"),
        ("Cadd2", "Cadd9"),
        ("C♯m", "C#m"),
        ("B♭7", "A#7"),
        ("Cmin7b5", "Cm7b5"),
        ("Cmaj", "C"),
        ("C/E", "C/E"),
    ] {
        assert_eq!(spell(&chord(typed), false), spelled, "{typed}");
    }
    for refused in ["", "Xyz", "H7", "c", "C/", "C/H", "Cfoo", "C7/E7", " C"] {
        assert_eq!(parse(refused), None, "{refused:?}");
    }
}

#[test]
fn notes_played_together_are_named_root_position_first() {
    for quality in Quality::KNOWN {
        let chord = Chord::new(0, quality.clone());
        let notes: Vec<u8> = quality.intervals().iter().map(|i| 48 + i).collect();
        assert_eq!(name(&notes), Some(chord), "{}", quality.tutti());
    }
    // Inversions with no root-position reading: the lowest note is the bass.
    assert_eq!(name(&[52, 55, 60]), Some(chord("C/E")));
    assert_eq!(name(&[43, 48, 52]), Some(chord("C/G")));
    // C E G A has a root-position reading, C6, preferred to Am7/C.
    assert_eq!(name(&[48, 52, 55, 57]), Some(chord("C6")));
    assert_eq!(name(&[45, 48, 52, 55]), Some(chord("Am7")));
    // A cluster, a single note and nothing are no chord.
    assert_eq!(name(&[60, 61, 62]), None);
    assert_eq!(name(&[60]), None);
    assert_eq!(name(&[]), None);
}

#[test]
fn a_voicing_keeps_its_ranges_and_drops_the_fifth_first() {
    for root in 0..12 {
        for quality in Quality::KNOWN {
            for bass in [None, Some((root + 4) % 12)] {
                let chord = Chord {
                    root,
                    quality: quality.clone(),
                    bass,
                };
                let notes = voice(&chord, None).unwrap();
                assert!(
                    (BASS_RANGE.0..=BASS_RANGE.1).contains(&notes[0]),
                    "{notes:?}"
                );
                assert_eq!(notes[0] % 12, chord.lowest());
                let upper = &notes[1..];
                assert!(upper.len() <= UPPER_VOICES, "{notes:?}");
                assert!(upper.windows(2).all(|pair| pair[0] < pair[1]), "{notes:?}");
                assert!(
                    upper
                        .iter()
                        .all(|pitch| (VOICING_RANGE.0..=VOICING_RANGE.1).contains(pitch)),
                    "{notes:?}"
                );
                assert_eq!(upper[0] % 12, root, "root position from the floor");
            }
        }
    }
    assert_eq!(upper_tones(&chord("C9")), [0, 4, 10, 2], "the fifth goes");
    assert_eq!(upper_tones(&chord("C13")), [0, 4, 10, 9], "then the ninth");
    assert_eq!(
        upper_tones(&chord("C11")),
        [0, 10, 2, 5],
        "the eleventh stays"
    );
    assert_eq!(
        voice(&Chord::new(0, Quality::Other("x".into())), None),
        None
    );
}

/// Every close voicing of a chord, as `voice` chooses among, to check it chose the least moving.
fn least_movement(chord: &Chord, before: &[u8]) -> u32 {
    let tones = upper_tones(chord);
    let mut best = u32::MAX;
    for start in 0..tones.len() {
        for floor in VOICING_RANGE.0..=VOICING_RANGE.1 {
            if floor % 12 != tones[start] {
                continue;
            }
            let mut notes = vec![floor];
            let mut ok = true;
            for step in 1..tones.len() {
                let first = tones[0];
                let mut circle = tones.clone();
                circle.sort_by_key(|t| (t + 12 - first) % 12);
                let at = circle.iter().position(|t| *t == tones[start]).unwrap();
                let tone = circle[(at + step) % circle.len()];
                let last = *notes.last().unwrap();
                let next = last + 1 + (tone + 24 - (last + 1) % 12) % 12;
                ok &= next <= VOICING_RANGE.1;
                notes.push(next);
            }
            if ok {
                best = best.min(movement(&notes, &before[1..]));
            }
        }
    }
    best
}

#[test]
fn voice_leading_moves_each_voice_least() {
    // C, Am, F, G: no upper voice more than a fourth from the one before it.
    let mut before = voice(&chord("C"), None).unwrap();
    for next in ["Am", "F", "G", "C"] {
        let notes = voice(&chord(next), Some(&before)).unwrap();
        for pitch in &notes[1..] {
            let nearest = before[1..]
                .iter()
                .map(|p| p.abs_diff(*pitch))
                .min()
                .unwrap();
            assert!(nearest <= 5, "{next}: {notes:?} after {before:?}");
        }
        assert_eq!(
            movement(&notes[1..], &before[1..]),
            least_movement(&chord(next), &before),
            "{next}"
        );
        before = notes;
    }
    // C, G7, C: a voice added, then dropped, measured alike.
    let c = voice(&chord("C"), None).unwrap();
    let g7 = voice(&chord("G7"), Some(&c)).unwrap();
    assert_eq!(g7.len(), 5);
    assert_eq!(
        movement(&g7[1..], &c[1..]),
        least_movement(&chord("G7"), &c)
    );
    let back = voice(&chord("C"), Some(&g7)).unwrap();
    assert_eq!(back.len(), 4);
    assert_eq!(
        movement(&back[1..], &g7[1..]),
        least_movement(&chord("C"), &g7)
    );
    // The measure: each new voice to its nearest old one (1, 1, 0), and each old one to its
    // nearest new one (1, 2, 1, 0).
    let distances: u32 = [1, 1, 0, 1, 2, 1, 0].iter().sum();
    assert_eq!(movement(&[60, 64, 67], &[59, 62, 65, 67]), distances);
}

fn walk(start: &str, key: Key, up: bool, steps: usize) -> Vec<String> {
    let mut chord = chord(start);
    let mut names = vec![spell(&chord, false)];
    for _ in 0..steps {
        chord = next_in_key(&chord, key, up).unwrap();
        names.push(spell(&chord, false));
    }
    names
}

#[test]
fn fine_walks_the_keys_chords_of_the_chords_size() {
    let c_major = Key::new(0, Key::MAJOR);
    assert_eq!(
        walk("C", c_major, true, 7),
        ["C", "Dm", "Em", "F", "G", "Am", "Bdim", "C"]
    );
    assert_eq!(walk("C", c_major, false, 2), ["C", "Bdim", "Am"]);
    assert_eq!(
        walk("Cmaj7", c_major, true, 7),
        [
            "Cmaj7", "Dm7", "Em7", "Fmaj7", "G7", "Am7", "Bm7b5", "Cmaj7"
        ]
    );
    // A fifth: B's isn't perfect, so it's passed.
    assert_eq!(walk("A5", c_major, true, 2), ["A5", "C5", "D5"]);
    // Five tones or more, a ninth: the third and seventh degrees make no ninth this crate knows.
    assert_eq!(walk("C13", c_major, true, 3), ["C13", "Dm9", "Fmaj9", "G9"]);
    // A chord not built of thirds goes to the key's chord of its size.
    assert_eq!(walk("Csus4", c_major, true, 1), ["Csus4", "Dm"]);
    assert_eq!(walk("C6", c_major, true, 1), ["C6", "Dm7"]);
    // From a root outside the key, the first degree that way.
    assert_eq!(walk("C#", c_major, true, 1), ["C#", "Dm"]);
    assert_eq!(walk("C#", c_major, false, 1), ["C#", "C"]);
    // Minor and a mode.
    let a_minor = Key::new(9, Key::MINOR);
    assert_eq!(walk("Am", a_minor, true, 3), ["Am", "Bdim", "C", "Dm"]);
    let d_dorian = Key::new(2, 0b0110_1010_1101);
    assert_eq!(walk("Dm", d_dorian, true, 2), ["Dm", "Em", "F"]);
    assert_eq!(walk("Bb", Key::new(10, Key::MAJOR), true, 1), ["A#", "Cm"]);
    // A scale whose degrees make no quality, and no key: a semitone, the quality kept.
    assert_eq!(walk("C", Key::new(0, 0b111), true, 1), ["C", "C#"]);
    assert_eq!(
        walk("Cm7/G", Key::new(0, Key::CHROMATIC), true, 1),
        ["Cm7/G", "C#m7/G#"]
    );
    assert_eq!(
        next_in_key(&Chord::new(0, Quality::Other("x".into())), c_major, true),
        None
    );
}

#[test]
fn the_ladders_go_simpler_and_richer() {
    let rows = [
        ("5", "5", "maj"),
        ("maj", "5", "maj7"),
        ("maj7", "maj", "maj9"),
        ("maj9", "maj7", "maj9"),
        ("6", "maj", "maj9"),
        ("add9", "maj", "maj9"),
        ("sus2", "5", "add9"),
        ("sus4", "5", "11"),
        ("aug", "maj", "aug"),
        ("min", "5", "min7"),
        ("min7", "min", "min9"),
        ("min6", "min", "min9"),
        ("min9", "min7", "min9"),
        ("7", "maj", "9"),
        ("9", "7", "11"),
        ("11", "9", "13"),
        ("13", "11", "13"),
        ("dim", "dim", "dim7"),
        ("dim7", "dim", "dim7"),
        ("min7b5", "dim", "min7b5"),
    ];
    assert_eq!(rows.len(), Quality::KNOWN.len());
    for (quality, down, up) in rows {
        let chord = Chord {
            root: 9,
            quality: Quality::from_tutti(quality),
            bass: Some(0),
        };
        let step = |chord: Option<Chord>| {
            let chord = chord.unwrap();
            assert_eq!((chord.root, chord.bass), (9, Some(0)), "root and bass kept");
            chord.quality.tutti().to_owned()
        };
        assert_eq!(step(simpler(&chord)), down, "{quality} simpler");
        assert_eq!(step(richer(&chord)), up, "{quality} richer");
    }
    assert_eq!(simpler(&Chord::new(0, Quality::Other("x".into()))), None);
    assert_eq!(transpose(&chord("B/D#"), 1), chord("C/E"));
}

#[test]
fn the_keys_notes() {
    let c_major = Key::new(0, Key::MAJOR);
    let notes: Vec<u8> = (0..12).filter(|pc| c_major.contains(*pc)).collect();
    assert_eq!(notes, [0, 2, 4, 5, 7, 9, 11]);
    let a_minor = Key::new(9, Key::MINOR);
    let notes: Vec<u8> = (0..12).filter(|pc| a_minor.contains(*pc)).collect();
    assert_eq!(notes, [0, 2, 4, 5, 7, 9, 11]);
    assert!(Key::new(3, Key::CHROMATIC).is_chromatic());
}
