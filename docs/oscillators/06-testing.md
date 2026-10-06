# 6. Testing oscillators

Everything in this reference regresses silently. A sign error in a correction function, a `dt` read
one sample late, a phase reset creeping into note-on — none of them fail to compile, none of them
crash, and none of them are obvious in a waveform display. They are obvious in a measurement.

This chapter is what to assert, how to measure it without the measurement lying to you, and what is
already asserted in
[`crates/mxm-mono-01-dsp/src/oscillator.rs`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/src/oscillator.rs).

---

## 6.1 What is currently asserted

Twelve tests in `oscillator.rs`, in rough order of what they would catch:

| Test | Catches |
|---|---|
| `saw_frequency_is_accurate_to_a_cent` | tuning error at four sample rates and five pitches |
| `sub_is_exactly_one_or_two_octaves_below` | a sub that divides by the wrong amount, or drifts |
| `sub_phase_is_locked_to_the_main_oscillator` | a sub recomputed from frequency instead of increment |
| `sawtooth_aliasing_stays_far_below_the_trivial_waveform` | band limiting broken, bypassed, or sign-flipped |
| `pulse_aliasing_holds_up_at_narrow_widths` | the second edge's correction skipped or misplaced |
| `dc_offset_stays_near_zero_across_the_pulse_width_range` | a missing or misconfigured DC blocker |
| `dc_blocker_passes_the_low_bass_it_is_supposed_to` | a DC blocker set so high it thins the bass |
| `output_is_bounded_and_finite_at_extreme_pitch` | NaN, inf, or unbounded output from absurd input |
| `phase_increment_never_exceeds_the_nyquist_limit` | a missing frequency clamp |
| `pulse_width_clamp_keeps_blep_corrections_apart` | overlapping corrections at high pitch |
| `changing_frequency_never_resets_the_phase` | a phase reset introduced into retuning |
| `silence_when_all_levels_are_zero` | a source leaking through at zero level |

Plus the standing crate requirement from
[`../../crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) that silence in gives
*exactly* zero out and `reset()` leaves no tail.

## 6.2 Measuring aliasing in a unit test

> **The mechanics below are compiled now.**
> [`crates/mxm-measure`](../../crates/mxm-measure/AGENTS.md) carries them:
> `spectrum::periods_for` and `stimulus::periodic_sine` for the exactly-periodic choice,
> `spectrum::harmonic_split` and `spectrum::alias_to_signal_db` for the partition, and
> `stimulus::additive_saw` and `stimulus::trivial_saw` as the two controls §6.3 insists on. What the
> crate deliberately does *not* carry is any threshold — §6.4 owns those, and they stay in the test
> that argues for them. Read this section for *why* the method works; take the code from the crate.

The hard part is that a naive spectrum measurement can be wrong by 30 dB in either direction, and
both directions are dangerous: too optimistic and the test passes a broken oscillator, too
pessimistic and it fails a correct one.

Three traps, and the way around all three at once.

**Trap 1: spectral leakage.** An arbitrary frequency in an arbitrary window is not periodic in it,
so energy smears across bins. Without a window this buries everything below about −13 dB; with a
window it buries everything below the window's sidelobe level, which for a 4-term Blackman-Harris
is −92 dB. Either way, a measurement floor arrives long before the interesting numbers do.

**Trap 2: separating alias from signal.** If harmonic and alias bins are not exactly known, you are
peak-picking with a threshold, and the threshold is now a parameter of your test result.

**Trap 3: startup transients.** Every algorithm has state and every startup transient is broadband.
A DPW oscillator started with a zeroed memory emits one sample of `1/(4T)` — at 220 Hz that is
+26 dB relative to the waveform, and it lands in exactly the bins the alias measurement reads.

All three go away with one choice: **make the waveform exactly periodic in the analysis window.**

```
N  = a power of two
P  = an odd number of periods in that window
f0 = P · fs / N
```

Then no window function is needed, harmonic `k` lands exactly on bin `k·P`, its alias lands exactly
on bin `k·P mod N`, and because `gcd(P, N) = 1` those two sets are disjoint. Alias energy is the
sum over every bin that is not a multiple of `P`. No estimation anywhere.

Two details make it exact in practice rather than in principle:

- **Set the increment, not the frequency.** `P / N` with `N` a power of two is a dyadic rational
  and therefore exact in binary floating point; `f0 / fs` computed from a rounded `f0` is not, and
  the residual error accumulates into phase drift across 65536 samples, which reintroduces leakage.
  `Phasor::set_inc` exists for exactly this.
- **Discard a settling prefix.** The signal repeats every `N` samples whatever the starting phase,
  so dropping the first few thousand samples costs nothing and removes trap 3 entirely.

The crate's implementation, kept small enough to read:

```rust
fn alias_to_signal_db(x: &[f64], periods: usize) -> f64 {
    let n = x.len();
    let half = n / 2;
    let (mut wanted, mut alias) = (0.0f64, 0.0f64);
    for bin in 1..half {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in x.iter().enumerate() {
            let ang = -2.0 * std::f64::consts::PI * bin as f64 * i as f64 / n as f64;
            re += v * ang.cos();
            im += v * ang.sin();
        }
        let power = re * re + im * im;
        if bin % periods == 0 {
            wanted += power;
        } else {
            alias += power;
        }
    }
    10.0 * (alias / wanted.max(1e-30)).max(1e-30).log10()
}
```

A direct DFT, not an FFT. `N = 2048` in the test, so it is four million operations — fast enough in
a debug build, and worth the clarity in a file whose other readers will be trying to decide whether
to trust the number.

## 6.3 Validate the ruler inside the test

The single most valuable line in an aliasing test is the one that measures something known to be
clean:

```rust
let floor = alias_to_signal_db(&ideal, PERIODS);
assert!(
    floor < -100.0,
    "the analysis is not trustworthy: an ideal saw measured {floor:.1} dB"
);
```

`ideal` is an additively synthesised sawtooth with exactly the harmonics that fit below Nyquist. It
has no aliasing by construction, so whatever the analysis reports for it is the analysis's own
floor. If someone later changes the window, the frequency selection or the bin partitioning in a
way that breaks the method, this assertion fails first and says so, instead of the real assertion
failing mysteriously or — much worse — passing for the wrong reason.

The same test also measures the *trivial* waveform, which establishes the other end of the range:

```rust
assert!(
    shipped_db < trivial_db - 10.0,
    "band limiting bought only {:.1} dB over the trivial waveform \
     (shipped {shipped_db:.1} dB, trivial {trivial_db:.1} dB)",
    trivial_db - shipped_db
);
```

This is the assertion that catches the failure mode that matters most: **band limiting silently not
happening.** An absolute threshold can be passed by an oscillator that is quietly generating a
trivial waveform at a low enough pitch. A relative one cannot.

## 6.4 Choosing thresholds

Thresholds should be far enough from the measured value that normal drift does not trip them, and
far enough from the failure mode that a real break does. For the sawtooth test, the reference
measurements from
[`examples/osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) at the neighbouring
frequency of 440.8 Hz are −35.4 dB for the shipped PolyBLEP and −19.1 dB for the trivial waveform.
The test asserts below −30 dB, and at least 10 dB better than trivial. Both have roughly 5 dB of
headroom against the real value and roughly 10 dB of margin against the failure.

**Do not set the threshold at the measured value.** A test that asserts `< -35.0` against a
measurement of −35.4 dB will fail on a different rounding mode, a different optimisation level, or
a harmless refactor, and the response to a test that fails for no reason is to loosen it, which
removes the test.

## 6.5 What a unit test cannot do

Three things need the example rather than the test suite:

- **Comparisons between algorithms.** A test asserts a property of the shipped code. Deciding
  whether to *change* the shipped code needs the whole table from
  [02-antialiasing.md §2.11](02-antialiasing.md#211-the-comparison-table), which is the example's
  job.
- **Cost.** Timing in a unit test is a flake generator. `cargo test` runs in debug by default,
  where every relative cost in this reference is wrong.
- **Does it sound like a synthesizer.** `cargo run -p mxm-mono-01-dsp --example mono_01_render_demo` writes a
  WAV, and there is no substitute.

## 6.6 Measuring something with no closed form

The exactly-periodic method needs a signal whose ideal spectrum you can name. A grain train, a
resynthesis engine, an FM pair or a wavetable read with an unusual interpolator does not have one.

It is worth being precise about what the method still does handle, because it is easy to talk
yourself out of a working measurement. **Periodicity does not hide aliasing.** An alias of harmonic
`k` lands at `|k·f0 − j·fs|`, which is a multiple of `f0` only when `fs` is an integer multiple of
`f0` — and §6.2's frequency choice deliberately makes `fs/f0 = N/P` non-integer. So aliases land off
the harmonic grid and the alias column separates them, for a grain train and an FM operator as much
as for a sawtooth. Measured: a rectangular-windowed grain train reports −24.4 dB there, and an FM
pair at index 64 reports −1.3 dB.

What the alias column cannot do is judge whether the *wanted* harmonics came out at the right
level, because for these algorithms you cannot write down what the right level is. A grain train's
harmonic amplitudes are the grain's Fourier transform sampled at multiples of `f0`, not `1/k`. That
is the gap the reference fills, along with the case where the output is not exactly periodic at all.

The general answer is to compare against a heavily oversampled rendering of the same algorithm:

```rust
fn oversampled_reference(
    make: impl Fn(f64) -> Box<dyn Osc>,
    fs: f64,
    factor: usize,
    n: usize,
) -> Vec<f64>
```

render at `factor` times the rate, decimate with a long windowed-sinc filter, and compare bin by
bin. Errors of every kind — folded aliasing, interpolation error, a wrong harmonic level — show up
together, which is the point. Two cautions:

- **The method has a floor, and it is not the FFT's.** The reference goes through a decimation
  filter with its own passband ripple and stopband leakage, and comparing against it can therefore
  never report better than about **−115 dB** on this harness. An unmodulated sine measures −115 dB
  against its own 8× reference. Read anything at that value as "no measurable error", never as a
  number, and always measure a known-clean control to find where the floor currently sits.
- **The reference has its own aliasing**, `factor` times further down. At 8× that is enough for
  anything measuring worse than about −100 dB and not enough for anything better.
- **The comparison must be between the same algorithm at two rates**, not between an algorithm and
  an analytic ideal. Otherwise the measurement includes whatever the algorithm gets wrong on
  purpose.

## 6.7 The tests worth adding that are not there yet

Recorded honestly, since the crate's AGENTS.md asks for the ones that regress silently and these do:

- **Aliasing at more than one pitch.** The current test measures one frequency. The worst case is
  the top of the range, where the shipped code measures −25.5 dB.
- **Aliasing under modulation.** No test covers a moving `dt`, and the exactly-periodic method
  cannot: it needs a steady tone. The technique would be to render the same modulated signal at
  8× and at 1×, decimate the former, and compare. Not implemented.
- **A `reset()` leaves no tail test for the oscillator specifically.** The crate-level requirement
  exists; the oscillator's own coverage of it is indirect, through `silence_when_all_levels_are_zero`.
- **Sample-rate sweep on the pulse.** The saw is tested at four rates; the pulse and its width
  clamp are tested at one.

---

Next: [07-rust-recipes.md](07-rust-recipes.md) — the verdict for mxm-mono-01.
