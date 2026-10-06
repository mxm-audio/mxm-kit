# 6. Testing and measurement

[← seminal machines](05-seminal-machines.md) · [index](README.md) · [next: Rust recipes →](07-rust-recipes.md)

> Audio bugs are not compile errors. A change that builds is not a change that works.
> — [`AGENTS.md`](../../AGENTS.md)

Everything in this chapter runs as `cargo test` with no host, because `crates/<plugin>-dsp` is
framework-free by design. Four of these properties — tuning, oscillation threshold, THD and
aliasing — regress silently under refactoring, so they belong in CI, not in a notebook.

> **These were run.** The `Svf` and `Ladder` from [chapter 2](02-topologies.md) were compiled with
> the tests below and pass: −3.01 dB at cutoff from 50 Hz to 18 kHz, oscillation threshold within
> 0.1 of `k = 4.00` at 44.1/48/96 kHz across four cutoffs, exact silence after decay, DC gain
> `1/(1+k)`, `HP1` pole mixing measuring as a highpass, survival of 1 kHz square-wave cutoff
> modulation, and `tan_approx` within 1e-4 relative. Three of the tests as first written were
> *wrong* rather than the filters — the specific traps are flagged in comments below, because each
> one produces a failure that looks like a broken filter.

---

## 6.1 A measurement toolkit

**This is compiled code now, not a block to copy.** It lives in
[`crates/mxm-measure`](../../crates/mxm-measure/AGENTS.md), it has tests that score every function
against a closed form, and every crate that measures sound takes it as a dev-dependency.

The block that used to be printed here was copied by hand into a dozen test modules and drifted: the
same `magnitude_at` ended up computing three different physical quantities under one name, and two
crates' oscillator tests were measuring tuning with a ruler nine times coarser than the accuracy they
asserted. A fenced toolkit nobody compiles is a toolkit that cannot be tested.

| Was in this block | Now |
|---|---|
| `magnitude_at` | `spectrum::component_amplitude` — an amplitude, full scale. The `2|X|/N` printed here was the correct convention; the half-scaled copies were not |
| `cycles_len` | `spectrum::periodic_length` |
| `peak`, `rms` | `level::peak`, `level::rms` |
| `db` | `convert::amplitude_db` for amplitudes, `convert::power_db` for energies — the block conflated them, and its `1e-12` floor is now the **caller's**, because what counts as silence differs between a filter test and a reverb tail |
| `render` | not needed: a closure over a system goes straight to `spectrum::transfer_gain` |

What is *not* there, deliberately, is anything that decides whether a number is acceptable. The four
tests below keep their own thresholds, and §6.4 is why.

---

## 6.2 The four tests every filter gets

```rust
#[test]
fn silence_in_gives_exact_silence_out() {
    let mut f = Svf::default();
    f.set(1_000.0, 0.707, 48_000.0);
    // Excite, then let it settle, then assert *exact* zero — this is the denormal
    // flush test as much as anything.
    for _ in 0..64 { f.process(1.0); }
    for _ in 0..48_000 { f.process(0.0); }
    for _ in 0..1_000 {
        assert_eq!(f.process(0.0).lp, 0.0);
    }
}

#[test]
fn no_nan_or_inf_under_a_parameter_sweep() {
    for &sr in &[44_100.0f32, 48_000.0, 96_000.0, 192_000.0] {
        for fc_i in 0..64 {
            // Exponential sweep, 20 Hz to just under the clamp.
            let fc = 20.0 * (0.45 * sr / 20.0).powf(fc_i as f32 / 63.0);
            for q_i in 0..16 {
                let q = 0.5 * 40.0f32.powf(q_i as f32 / 15.0);
                let mut f = Svf::default();
                f.set(fc, q, sr);
                for i in 0..4096 {
                    let x = if i % 512 == 0 { 1.0 } else { 0.0 };
                    let out = f.process(x);
                    assert!(out.lp.is_finite() && out.bp.is_finite() && out.hp.is_finite(),
                        "non-finite at sr={sr} fc={fc} q={q}");
                }
            }
        }
    }
}

#[test]
fn output_is_bounded() {
    // Only meaningful for a filter with a *nonlinear* loop and a stated bound —
    // a linear ladder past k = 4 diverges by design, and should. If you cannot
    // state a bound, you do not have an argument, you have a hope.
    // `mxm_mono_01_dsp::filter::OUTPUT_BOUND` is this repo's stated bound.
    let mut f = NonlinearLadder::new();
    let mut worst = 0.0f32;
    for i in 0..480_000 {
        // Overdriven square-ish input, resonance past the oscillation threshold.
        let x = ((i as f32 * 0.01).sin() * 4.0).clamp(-1.0, 1.0);
        worst = worst.max(f.process(x, 1_000.0, 1.0, 48_000.0).abs());
    }
    assert!(worst < OUTPUT_BOUND, "peak {worst} exceeds stated bound {OUTPUT_BOUND}");
}

#[test]
fn reset_leaves_no_tail() {
    let mut f = Svf::default();
    f.set(500.0, 8.0, 48_000.0);
    for _ in 0..1000 { f.process(1.0); }
    f.reset();
    for _ in 0..100 {
        assert_eq!(f.process(0.0).lp, 0.0);
    }
}
```

---

## 6.3 Tuning: is the cutoff where you said it is?

The test that catches a missing prewarp instantly.

```rust
#[test]
fn butterworth_cutoff_is_minus_three_db() {
    let sr = 48_000.0;
    for &fc in &[50.0f32, 200.0, 1_000.0, 4_000.0, 8_000.0, 12_000.0, 18_000.0] {
        let n = cycles_len(fc, sr, 1 << 16);
        let mut f = Svf::default();
        f.set(fc, std::f32::consts::FRAC_1_SQRT_2, sr);
        // Let the transient pass, then measure a whole number of cycles.
        for i in 0..4096 {
            f.process((2.0 * PI * fc * i as f32 / sr).sin());
        }
        let out: Vec<f32> = (4096..4096 + n)
            .map(|i| f.process((2.0 * PI * fc * i as f32 / sr).sin()).lp)
            .collect();
        let g = db(magnitude_at(&out, fc, sr));
        assert!((g + 3.01).abs() < 0.3, "fc={fc}: {g} dB, expected -3.01");
    }
}
```

The key line is the highest frequency. Without prewarping, `fc = 12 kHz` at 48 kHz comes out well
over an octave low and this assertion fails by many dB. **Run it at 44.1, 48, 96 and 192 kHz** — a
filter that is only correct at one sample rate is a filter with a hard-coded constant in it.

For a ladder, the equivalent test is the *resonant peak* frequency at high `k`: sweep sine
frequencies around the nominal cutoff, find the maximum, assert it lands within 1% of `fc`.

---

## 6.4 Self-oscillation threshold

The number that tells you whether your loop is really resolved. For an ideal ladder it must be
`k = 4.00`, at every cutoff and every sample rate.

```rust
/// Does the filter sustain oscillation from a small impulse?
fn oscillates(fc: f32, k: f32, sr: f32) -> bool {
    let mut f = Ladder::default();
    f.set(fc, k, sr);
    let settle = (0.25 * sr) as usize;
    // Kick it, let transients pass, then compare energy across two later windows.
    for i in 0..settle {
        let x = if i == 0 { 1.0 } else { 0.0 };
        f.process(x);
    }
    let a = rms(&render(4096, |_| f.process(0.0)[4]));
    for _ in 0..settle { f.process(0.0); }
    let b = rms(&render(4096, |_| f.process(0.0)[4]));
    // A *linear* ladder past threshold diverges rather than settling to a level,
    // so non-finite means "oscillating". Without this branch the NaN comparison
    // below is false and the bisection concludes the filter never oscillates —
    // which is exactly the bug this test is supposed to catch in the filter.
    // Keep the settle window short for the same reason: 1 s of exponential
    // growth at k = 6 overflows f32 long before you measure it.
    if !b.is_finite() {
        return true;
    }
    b > a * 0.5 && b > 1.0e-4
}

#[test]
fn oscillation_threshold_is_four_everywhere() {
    for &sr in &[44_100.0f32, 48_000.0, 96_000.0, 192_000.0] {
        for &fc in &[100.0f32, 500.0, 2_000.0, 8_000.0] {
            // Bisect on k.
            let (mut lo, mut hi) = (0.0f32, 6.0f32);
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if oscillates(fc, mid, sr) { hi = mid } else { lo = mid }
            }
            assert!((hi - 4.0).abs() < 0.1, "sr={sr} fc={fc}: threshold {hi}");
        }
    }
}
```

A threshold that moves with `fc` is the signature of a delayed feedback path (§3.3a). A threshold
that moves with sample rate is the signature of a missing prewarp or a hard-coded coefficient. This
repo's measured spread — 2.66 to 3.96 with a delayed loop, 3.97 to 4.00 with it solved — is exactly
what this test produces.

---

## 6.4b The droop is a testable invariant

`H(0) = 1/(1+k)` (§2.3) is a number, so assert it.

```rust
#[test]
fn ladder_droops_at_dc_with_resonance() {
    let sr = 48_000.0f32;
    let mut f = Ladder::default();
    // k must be *below* threshold. At k = 4 the poles sit on the imaginary axis,
    // the step response rings forever and there is no settled value to measure —
    // a mistake that reads as a broken filter and is a broken test.
    f.set(4_000.0, 2.0, sr);
    let mut y = 0.0;
    for _ in 0..48_000 {
        y = f.process(1.0)[4];
    }
    assert!((y - 1.0 / 3.0).abs() < 0.01, "DC gain {y}, expected 0.333");
}
```

If this test *passes at unity gain*, someone has added resonance compensation. That may be
intentional (§2.3) — but it should be a decision with a comment, not a surprise.

## 6.5 Distortion and aliasing

```rust
/// THD of a filter driven by a sine: energy in harmonics 2..=H over energy at f0.
pub fn thd(out: &[f32], f0: f32, sr: f32, harmonics: usize) -> f32 {
    let fund = magnitude_at(out, f0, sr);
    let mut sum = 0.0f64;
    for h in 2..=harmonics {
        let f = f0 * h as f32;
        if f < 0.5 * sr {
            let m = magnitude_at(out, f, sr) as f64;
            sum += m * m;
        }
    }
    (sum.sqrt() / fund.max(1.0e-9) as f64) as f32
}

/// Aliasing: drive with a sine whose harmonics fold, then measure everything that is
/// NOT at a harmonic of f0. Any energy there arrived by folding.
#[test]
fn aliasing_stays_below_target_at_the_worst_preset() {
    let sr = 48_000.0f32;
    let f0 = 1_237.0; // deliberately not a nice ratio to sr
    let n = cycles_len(f0, sr, 1 << 17);

    let mut f = Ladder::default();
    f.set(16_000.0, 3.9, sr); // worst case: cutoff open, resonance high
    let out: Vec<f32> = (0..n)
        .map(|i| f.process(4.0 * (2.0 * PI * f0 * i as f32 / sr).sin())[4])
        .collect();

    let harmonics: Vec<f32> = (1..)
        .map(|h| f0 * h as f32)
        .take_while(|&f| f < 0.5 * sr)
        .collect();

    // Probe a grid of frequencies; flag any bin far from every harmonic.
    let mut worst = 0.0f32;
    for i in 1..500 {
        let probe = i as f32 * 0.5 * sr / 500.0;
        let near = harmonics.iter().any(|&h| (h - probe).abs() < 40.0);
        if !near {
            worst = worst.max(magnitude_at(&out, probe, sr));
        }
    }
    let fund = magnitude_at(&out, f0, sr);
    let rel = db(worst / fund);
    assert!(rel < -60.0, "aliasing at {rel} dBc");
}
```

Set the threshold from a listening test, then hold it. `−60 dBc` is a reasonable bar for a synth
filter at a hostile preset; `−80` needs oversampling or ADAA; `−40` will be audible as a shimmer
that does not track pitch.

**Do the pitch-tracking check by ear too:** play the same preset up a chromatic scale. Real
harmonics move up with the note; aliases move *down*. If you hear something descending while you
play ascending, that is folding.

---

## 6.6 Time-varying stability

The test that catches everything §1.7 warns about.

```rust
#[test]
fn survives_audio_rate_cutoff_modulation() {
    let sr = 48_000.0f32;
    let mut f = Ladder::default();
    for i in 0..(10 * sr as usize) {
        // Full-range square-wave cutoff modulation at 1 kHz: the most hostile
        // thing a modulation matrix can do.
        let fc = if (i / 24) % 2 == 0 { 30.0 } else { 0.44 * sr };
        f.set(fc, 3.95, sr);
        let x = (2.0 * PI * 110.0 * i as f32 / sr).sin();
        let y = f.process(x)[4];
        assert!(y.is_finite() && y.abs() < 32.0, "blew up at sample {i}: {y}");
    }
}
```

Variants worth adding: a sine sweep of cutoff, resonance jumping between 0 and maximum, and sample
rate changes mid-render (which should be impossible in a host but is a cheap way to catch state that
depends on `fs` and is not recomputed).

---

## 6.7 Denormal / silence-cost test

Not a correctness test, a performance one — but it belongs here because the symptom is so
counter-intuitive.

```rust
#[test]
fn silence_is_not_slower_than_signal() {
    use std::time::Instant;
    let sr = 48_000.0f32;
    let mut f = Ladder::default();
    f.set(200.0, 3.0, sr);

    let t0 = Instant::now();
    for i in 0..1_000_000 {
        std::hint::black_box(f.process((2.0 * PI * 110.0 * i as f32 / sr).sin()));
    }
    let loud = t0.elapsed();

    let t1 = Instant::now();
    for _ in 0..1_000_000 {
        std::hint::black_box(f.process(0.0));
    }
    let quiet = t1.elapsed();

    assert!(quiet < loud * 2, "silence {quiet:?} vs signal {loud:?} — denormals?");
}
```

Mark it `#[ignore]` in CI if the runners are noisy; run it locally when touching state handling.

---

## 6.8 Things worth eyeballing, not asserting

Some properties are easier to inspect than to encode. Render them to a file from an `examples/`
binary (`crates/mxm-mono-01-dsp/examples/` already has this pattern) and look:

- **Magnitude response family** — sweep `k` from 0 to threshold, plot all the curves together. The
  ladder droop should be visible, and the peak should stay at `fc`.
- **Impulse response** at high resonance — should be a decaying sine at `fc`, not a decaying
  something-else.
- **Pole trajectories**, if you can compute them — the 45° X for a transistor ladder is a striking
  confirmation you built the right thing.
- **A sweep with the resonance up, recorded as audio.** Zipper noise, stepping and clicks are
  instantly obvious by ear and nearly invisible in a plot.

---

## 6.9 The full-system checks this repo already mandates

- `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `cargo fmt --all`
- `cargo xtask bundle <plugin> --release` — plain `cargo build` does not produce a loadable plugin
- `clap-validator validate "target/bundled/<Bundle Name>.clap"`
- **A debug build run** — `assert_process_allocs` only fires in debug, so a clean release validator
  run proves nothing about allocation
- A real DAW at a small buffer size, with automation on cutoff and resonance

Say which of these actually ran. "It builds" is not a result.

---

[← seminal machines](05-seminal-machines.md) · [index](README.md) · [next: Rust recipes →](07-rust-recipes.md)
