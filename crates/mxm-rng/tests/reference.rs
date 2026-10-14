//! **Each generator against the copies it replaced.** The expected bits were printed by those copies
//! themselves, pasted verbatim from the collection's source on 2026-10-14 before any was removed;
//! each test computes the same draws the way its consumer now does. A failure here means a
//! consumer's stream moved, and with it a golden digest or a recorded render.

use mxm_rng::{Lcg32, SplitMix64, Xorshift32, Xorshift64, Xorshift64Star, map};

const GOLDEN_64: u64 = 0x9E37_79B9_7F4A_7C15;

fn bits32(v: impl IntoIterator<Item = f32>) -> Vec<u32> {
    v.into_iter().map(f32::to_bits).collect()
}

fn bits64(v: impl IntoIterator<Item = f64>) -> Vec<u64> {
    v.into_iter().map(f64::to_bits).collect()
}

/// The synths' and the bucket delay's `Rng` (seven byte-identical copies and mono-pr1's).
#[test]
fn xorshift32_bipolar_matches_the_synth_copies() {
    let mut zero = Xorshift32::new(0);
    assert_eq!(
        bits32((0..4).map(|_| zero.bipolar_f32())),
        [0xbebbcee8, 0x3f405caa, 0xbd08ce20, 0xbf7cf8b0],
        "a zero seed becomes the golden-ratio constant"
    );
    let mut seeded = Xorshift32::new(0x1234_5678);
    assert_eq!(
        bits32((0..4).map(|_| seeded.bipolar_f32())),
        [0x3d730b40, 0xbf5549b8, 0xbedf7c30, 0x3c59d600]
    );
}

/// mxm-mono-08's `unipolar`.
#[test]
fn xorshift32_unit_matches_mono_08() {
    let mut zero = Xorshift32::new(0);
    assert_eq!(
        bits32((0..4).map(|_| zero.unit_f32())),
        [0x3ea2188c, 0x3f602e55, 0x3ef7731e, 0x3bc1d400]
    );
    let mut seeded = Xorshift32::new(0x1234_5678);
    assert_eq!(
        bits32((0..4).map(|_| seeded.unit_f32())),
        [0x3f07985a, 0x3daad920, 0x3e9041e8, 0x3f01b3ac]
    );
}

/// mxm-fx-oldify's `transport::Rng`: its own seeding rule and f64 law over the generator.
#[test]
fn xorshift32_words_match_fx_oldify() {
    let oldify = |seed: u32| {
        let mut rng = Xorshift32::from_state(seed.wrapping_mul(2_654_435_761).max(1));
        bits64((0..4).map(move |_| f64::from(rng.next_u32()) / f64::from(u32::MAX) * 2.0 - 1.0))
    };
    assert_eq!(
        oldify(0),
        [
            0xbfeffef7f7bffef8,
            0xbfeefdfe7fbefdfe,
            0x3fcdcca8c59dcca8,
            0xbfeb6a99ac3b6a9a
        ]
    );
    assert_eq!(
        oldify(0x1234_5678),
        [
            0x3feffef8073ffef8,
            0x3fef3f07783f3f08,
            0x3fc7446a40974468,
            0xbfd23fb83cd23fb8
        ]
    );
}

/// The drum machine's 55 noise and metal noise, the convolution's starter room, the shimmer's
/// jitter: `map::unit_f32_word`.
#[test]
fn the_word_law_matches_its_four_copies() {
    let mut noise = Xorshift32::from_state(0x55d3_1a7b);
    assert_eq!(
        bits32((0..4).map(|_| map::unit_f32_word(noise.next_u32()) * 2.0 - 1.0)),
        [0x3f4a2090, 0xbeb93d08, 0x3d71f860, 0x3d1582a0]
    );
    let mut jitter = Xorshift32::from_state(0x6d2b_79f5);
    assert_eq!(
        bits32((0..4).map(|_| map::unit_f32_word(jitter.next_u32()))),
        [0x3e815d8e, 0x3f11e00c, 0x3f1c0fe1, 0x3ecae1ed]
    );
}

/// The drum machine's jump tables are built from `step`.
#[test]
fn step_matches_the_jump_tables_recurrences() {
    assert_eq!(Xorshift32::step(0x1234_5678), 0x8798_5aa5);
    assert_eq!(
        Xorshift64::step(0x2545_F491_4F6C_DD1D),
        0x7f6c_280b_eaa8_e3e7
    );
}

/// The player's sequencer: `seed | 1`, a bounded draw from the high half, then `unit_f64`.
#[test]
fn xorshift64_matches_the_players_sequencer() {
    for (seed, below, unit) in [
        (
            0u64,
            0,
            [
                0x3fb00041060c0110,
                0x3fe363d085edd0c4,
                0x3feeaa9ea06aabb0,
                0x3fe0c183f6120b32,
            ],
        ),
        (
            0xB10C,
            6,
            [
                0x3fef88317c28e21b,
                0x3fdf0b5b21704292,
                0x3fe1c0a58add2e32,
                0x3feedd9741ae78f5,
            ],
        ),
    ] {
        let mut rng = Xorshift64::from_state(seed | 1);
        assert_eq!(rng.next_u32() % 7, below);
        assert_eq!(bits64((0..4).map(|_| rng.unit_f64())), unit);
    }
}

/// The drum engines' LFO and per-hit draws: the raw words and `map::unit_f32_closed`.
#[test]
fn xorshift64_and_the_closed_law_match_the_drum_engines() {
    let mut rng = Xorshift64::from_state(0x2545_F491_4F6C_DD1D);
    let words: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
    assert_eq!(
        words,
        [
            0x7f6c280beaa8e3e7,
            0xe47119871cf9abe0,
            0x35174a4158b8a0b7,
            0x62ce1ffad85b1c36
        ]
    );
    assert_eq!(
        bits32(
            words
                .iter()
                .map(|&w| map::unit_f32_closed((w >> 32) as u32))
        ),
        [0x3efed851, 0x3f64711a, 0x3e545d29, 0x3ec59c3f]
    );
}

/// mxm-fx-delay's random modulation: `seed.max(1)` and the closed law, bipolar.
#[test]
fn the_closed_law_matches_fx_delay() {
    // `modulation::Rng` seeds with `seed.max(1)`; `random_signed` runs on a nonzero state, which
    // the same rule leaves alone.
    let delay = |seed: u64| {
        let mut rng = Xorshift64::from_state(seed.max(1));
        bits32((0..4).map(move |_| map::unit_f32_closed(rng.next_u32()) * 2.0 - 1.0))
    };
    assert_eq!(delay(0), [0xbf800000, 0xbf5fff7e, 0x3e58f428, 0x3f6aa9ec]);
    assert_eq!(delay(7), [0xbf800000, 0xbdffe388, 0x3f02b040, 0xbd690d90]);
    assert_eq!(
        delay(0x1234_5678_9abc_def0),
        [0x3f7d001c, 0xbf663078, 0xbef88126, 0xbe186a84]
    );
}

/// mxm-classic-verb's test streams: `(seed + 1) × golden`, zero moved to one, then words and
/// `bipolar_f64`.
#[test]
fn xorshift64_matches_classic_verbs_test_streams() {
    let seeded = |seed: u64| {
        let state = seed.wrapping_add(1).wrapping_mul(GOLDEN_64);
        Xorshift64::from_state(if state == 0 { 1 } else { state })
    };
    for (seed, words, bipolar) in [
        (
            0u64,
            [0xdc1b77ae0bf34dad, 0x64f0eeb9026e6076],
            [
                0xbfa3e0c5b869be80,
                0xbfe3e83ebcf25c8e,
                0xbfe4c53a47d78eae,
                0x3fc7101dce4e7bf8,
            ],
        ),
        (
            4,
            [0xbe6f263aaff8bef1, 0x1c59458f94ced3cc],
            [
                0x3fee1c688b063f0e,
                0xbfd0582328fc1904,
                0x3fd93051953bcdd8,
                0xbfc4b37ed7f576a8,
            ],
        ),
    ] {
        let mut rng = seeded(seed);
        assert_eq!([rng.next_u64(), rng.next_u64()], words);
        assert_eq!(bits64((0..4).map(|_| rng.bipolar_f64())), bipolar);
    }
}

/// mxm-grain-fx's `Rng`: `unit_f32`, then `bipolar_f32`, from its fixed seed.
#[test]
fn xorshift64_star_matches_grain_fx() {
    let mut rng = Xorshift64Star::from_state(0x2545_F491_4F6C_DD1D);
    assert_eq!(
        bits32((0..4).map(|_| rng.unit_f32())),
        [0x3f2d5db6, 0x3f3edaf6, 0x3d21fe00, 0x3f1706f4]
    );
    assert_eq!(
        bits32((0..4).map(|_| rng.bipolar_f32())),
        [0x3f7be786, 0xbedf8670, 0xbf3e8f86, 0xbd75d980]
    );
}

/// mxm-model-drums' `White`: `seed.max(1) ^ golden`, then `bipolar_f64`.
#[test]
fn xorshift64_star_matches_model_drums_white() {
    for seed in [0u64, 1] {
        let mut rng = Xorshift64Star::from_state(seed.max(1) ^ GOLDEN_64);
        assert_eq!(
            bits64((0..4).map(|_| rng.bipolar_f64())),
            [
                0xbfebf54c51941c78,
                0xbfe6d1d8b73a586e,
                0x3fdac24b8d525694,
                0xbfe88227ad24d15e
            ]
        );
    }
}

/// mxm-measure's `stimulus::noise`: `new` (zero to golden) and its own f64 law.
#[test]
fn xorshift64_star_matches_mxm_measures_noise() {
    let noise = |seed: u64| {
        let mut rng = Xorshift64Star::new(seed);
        bits32((0..4).map(move |_| {
            let unit = (f64::from(rng.next_u32() >> 8) / 16_777_216.0) * 2.0 - 1.0;
            (unit * 0.5) as f32
        }))
    };
    assert_eq!(noise(0), [0xbee4f89a, 0xbe2ceed0, 0x3e2117cc, 0xbc247d40]);
    assert_eq!(noise(4), [0xbe6c9b4c, 0x3e4796d4, 0xbc87f460, 0xbdedd7e0]);
}

/// mxm-listening's staircase: `seed.max(1)`, then `unit_f64`.
#[test]
fn xorshift64_star_matches_the_staircase() {
    for (seed, unit) in [
        (
            0u64,
            [
                0x3fd1f93392e25b36,
                0x3fe579f4d51c0f2c,
                0x3fe73a21b1fd6e63,
                0x3fd36d06282ec6c0,
            ],
        ),
        (
            9,
            [
                0x3fea32e5394e75b0,
                0x3faf36233c1e3c90,
                0x3fe1817a7318dba8,
                0x3fe1041adc39737e,
            ],
        ),
    ] {
        let mut rng = Xorshift64Star::from_state(seed.max(1));
        assert_eq!(bits64((0..4).map(|_| rng.unit_f64())), unit);
    }
}

/// mxm-creative-sampler's `split_mix` stream, and its stateless `cell_wobble`; the drum machine's
/// `pcm_909::source_noise`.
#[test]
fn split_mix_matches_the_sampler_and_the_909() {
    for (seed, unit) in [
        (0u64, [0x3f6220a8, 0x3edcf13c, 0x3cd88ba0, 0x3f788bb8]),
        (
            0xa5a5_5a5a_c3c3_3c3c,
            [0x3e8c9278, 0x3f30ff03, 0x3e68dcb4, 0x3f369e9b],
        ),
    ] {
        let mut rng = SplitMix64::new(seed);
        assert_eq!(bits32((0..4).map(|_| rng.unit_f32())), unit);
    }
    let wobble =
        |cell: u64| map::unit_f32((SplitMix64::mix(cell.wrapping_mul(GOLDEN_64)) >> 32) as u32);
    assert_eq!(
        bits32((0..4).map(wobble)),
        [0x00000000, 0x3f6220a8, 0x3edcf13c, 0x3cd88ba0]
    );
    let source = |index: u64| map::bipolar_f32((SplitMix64::mix(index ^ GOLDEN_64) >> 32) as u32);
    assert_eq!(
        bits32((0..4).map(source)),
        [0x3f444150, 0x3f49b2e2, 0x3e3ac1a8, 0x3e085168]
    );
}

/// mxm-classic-verb's `shaped::Lcg`: `unit_f32`, and the raw states.
#[test]
fn lcg32_matches_classic_verbs_shaped_path() {
    let mut zero = Lcg32::new(0);
    assert_eq!(
        bits32((0..4).map(|_| zero.unit_f32())),
        [0x3e71bbcc, 0x3e8ea052, 0x3f51ccf6, 0x3f2af953]
    );
    let mut shaped = Lcg32::new(0x6d78_6d31);
    assert_eq!(
        bits32((0..4).map(|_| shaped.unit_f32())),
        [0x3ee1ae08, 0x3de62ed0, 0x3f5f216e, 0x3f2bdf62]
    );
    let mut raw = Lcg32::new(0x6d78_6d31);
    assert_eq!([raw.next_u32(), raw.next_u32()], [0x70d7_04dc, 0x1cc5_da8b]);
}
