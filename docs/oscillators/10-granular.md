# 10. Granular oscillators

Granular synthesis is usually filed under sound design rather than oscillator design, and the
quotation marks in "granular oscillator" are doing real work: a cloud of grains scattered over a
sample is not an oscillator in any useful sense.

But there is a strict subset that *is* one. Fire grains at a regular rate, and the rate becomes the
pitch; the contents of the grain become the timbre; the grain's envelope becomes a formant. That is
**pulsar synthesis**, and it is the same idea as **FOF** (Rodet's formant-wave-function synthesis,
the engine behind CHANT) and **VOSIM** (Kaegi and Tempelaars' sine-squared pulse trains) — three
names for a periodic train of short windowed bursts, arrived at independently for speech synthesis
and for composition.

This chapter is about that subset, plus the parts of asynchronous granular that a realtime
implementation has to get right regardless. Figures from
[`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §9 at 44.1 kHz.

---

## 10.1 The structure

```rust
impl Osc for GrainTrain {
    fn next(&mut self) -> f64 {
        let period = self.fs / self.rate;
        while self.n >= self.next_onset {
            /* spawn a grain at self.next_onset */
            self.next_onset += period;
        }
        /* sum every live grain: envelope(u) * carrier(t) */
    }
}
```

Four parameters and every one of them is a design decision with a measurable cost:

| Parameter | What it controls | Where it goes wrong |
|---|---|---|
| Grain **rate** | perceived pitch (synchronous) or density (asynchronous) | onset quantisation — §10.3 |
| Grain **duration** | overlap, and the width of the formant | pool exhaustion — §10.5 |
| Grain **envelope** | the formant's shape, and the aliasing | discontinuities — §10.2 |
| Grain **contents** | the timbre | ordinary oscillator problems |

## 10.2 The envelope is the antialiasing decision

A grain is a window multiplied by a carrier. Multiplying in time is convolving in frequency, so the
output spectrum is the carrier's spectrum smeared by the window's. A window with a discontinuity
has a spectrum that decays at 6 dB/octave — and now so does the grain, all the way past Nyquist.

Measured on a synchronous train: 199.9 Hz grain rate, 20 ms grains (four overlapping), 1500 Hz
carrier. `alias` is the exactly-periodic analysis of
[chapter 1](01-fundamentals.md#15-the-measurement-method); `vs reference` is the total deviation
from an 8×-oversampled rendering of the same train.

| envelope | alias | vs 8× reference |
|---|---|---|
| rectangular | −24.4 dB | −47.4 dB |
| Hann | −126.6 dB | −125.3 dB |
| Tukey, 25% taper | −114.7 dB | −123.1 dB |
| Gaussian, σ = 0.5 | −36.7 dB | −59.8 dB |
| FOF (2% cosine rise, exponential decay) | −83.8 dB | −100.8 dB |

**Rectangular against Hann is 78 dB.** No other decision in this chapter is worth as much. A
rectangular grain is two step discontinuities per grain; at four grains overlapping and 200 grains
per second that is 1600 steps per second, none of them band-limited.

**The Gaussian result is the interesting one**, because a Gaussian is the smoothest window there
is and it still measures 65 dB worse than Hann. The reason is truncation: a Gaussian never reaches
zero, so cutting it off at the grain boundary leaves a step. At σ = 0.5 that step is about 1.8% of
full scale — small, and still worth 65 dB. **Windows are judged by their endpoints, not by their
smoothness.**

The same reasoning explains FOF's −83.8 dB: its exponential decay is truncated when the grain ends,
so it has one small step per grain rather than Hann's none. Olsen, Smith and Abel note the same
mechanism in the earlier vocal-synthesis literature — resonant filter approaches were "prone to
audible artifacts being present due to the sharp discontinuity at the start of the exponential
decay". If a FOF-shaped grain is what you want, either let it decay far enough to be inaudible
before truncating, or apply a short fade — and if it must be truncated hard, that step is a
first-derivative-and-signal discontinuity that a
[BLEP or BLAMP](02-antialiasing.md#28-blamp-corners-rather-than-steps) correction can clean up.

## 10.3 Onset placement matters more than everything else

This is the finding I did not expect, and it is the largest single number in this chapter.

A grain's onset almost never lands on a sample boundary. The obvious implementation rounds it to
the nearest sample. Measured, on the same train, both against the same fractional-onset 8×
reference:

| envelope | onsets | alias | vs reference |
|---|---|---|---|
| Hann | fractional | −126.6 dB | −125.3 dB |
| Hann | **rounded to the sample grid** | **+18.6 dB** | **−54.2 dB** |
| Tukey 25% | fractional | −114.7 dB | −123.1 dB |
| Tukey 25% | **rounded to the sample grid** | **−14.7 dB** | **−54.1 dB** |

**Rounding onsets to the sample grid costs 71 dB.** It converts a train that is essentially
perfect into one whose error is 54 dB down — worse than a trivial modulo-counter sawtooth.

Two things are happening, and it is worth separating them:

- **The train stops being periodic.** Rounding each onset by a different fraction of a sample
  modulates the grain period by up to ±0.5 sample. At a 220-sample period that is ±0.2% of jitter,
  landing as inharmonic sidebands around every partial. That is the `alias` column.
- **The grains lose phase coherence with each other.** Each grain's carrier phase is measured from
  its own onset, so shifting the onset shifts the carrier. With four grains overlapping, partial
  cancellation between mistimed copies produces amplitude error at the harmonics themselves — the
  `vs reference` column, and the reason the `alias` figure alone (+18.6 dB) looks absurd: the
  fundamental it is measured against has been largely cancelled.

The fix costs nothing:

```rust
self.spawn(at.max(0.0));   // `at` is fractional
...
start: if self.quantise { at.round() } else { at },
```

Keep the onset as a fractional sample position, and compute both the envelope position and the
carrier phase from `t = n - start`, which is then also fractional. No interpolation, no resampling —
just refusing to round. **A granular engine that quantises onsets has thrown away more quality
than any choice of envelope can recover.**

## 10.4 Overlap-add and the constant-overlap-add property

If grains fire regularly and overlap, their envelopes sum. Whether that sum is *flat* is the
constant-overlap-add (COLA) property, and getting it wrong produces a periodic amplitude wobble at
the grain rate — which, at audio grain rates, is not a wobble but a whole extra set of sidebands.

Measured as peak-to-trough variation of the envelope sum, at hops of `duration/overlap`:

| envelope | ovl 1 | ovl 2 | ovl 3 | ovl 4 | ovl 8 |
|---|---|---|---|---|---|
| rectangular | 0.00 dB | 0.00 dB | 0.00 dB | 0.00 dB | 0.00 dB |
| Hann | *gaps* | **0.00 dB** | **0.00 dB** | **0.00 dB** | **0.00 dB** |
| Tukey 25% | *gaps* | 6.02 dB | 3.52 dB | 2.50 dB | 0.00 dB |
| Gaussian σ=0.5 | 17.37 dB | 0.58 dB | 0.42 dB | 0.24 dB | 0.06 dB |

- **Hann is exactly flat at 50% overlap and every integer overlap above it.** This is the classic
  result and the reason Hann is the default in every granular instrument. Exactly flat: the
  measurement reports 0.00 dB, not "small".
- **Rectangular is flat at every overlap** — and aliases catastrophically (§10.2). Flatness and
  band-limiting are different properties and a window can have either without the other.
- **Tukey is not COLA at 50%**, by 6 dB, which is a large and often-unnoticed error. It becomes
  flat at 8× overlap.
- **Gaussian never reaches flat**, only asymptotically: 0.58 dB at 2×, 0.06 dB at 8×.
- **At overlap 1, Hann and Tukey leave actual silence between grains.** The sum reaches zero. If a
  granular patch sounds like it is stuttering, check the overlap before blaming anything else.

## 10.5 Asynchronous clouds: statistics, not waveforms

Scatter the onsets and the output stops being periodic, so none of the spectral machinery in this
reference applies. What can be measured is the statistics, and those are what govern gain staging
and voice allocation.

20 ms Hann grains, onsets jittered by half a grain period, pool of 64:

| grains/s | grains sounding | RMS | peak | crest factor | dropped |
|---|---|---|---|---|---|
| 10 | 0.2 | 0.173 | 0.999 | 15.2 dB | 0 |
| 50 | 0.9 | 0.413 | 1.332 | 10.2 dB | 0 |
| 200 | 3.8 | 0.849 | 2.243 | 8.4 dB | 0 |
| 1000 | 19.9 | 1.823 | 5.826 | 10.1 dB | 0 |
| 5000 | 63.3 | 1.934 | 7.257 | 11.5 dB | **900** |

**Live grains = density × duration.** 1000 grains/s at 20 ms gives 19.9 sounding, 200 gives 3.8.
The relationship is exact enough to size a pool from the parameter ranges rather than by
experiment, which matters because the pool has to be preallocated
([`../../crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) forbids allocating in
the audio path).

**RMS grows as the square root of density**, as independent sources do: ×5 in density is ×2.24 in
RMS (0.173 → 0.413 → 0.849 → 1.823 against predictions of 0.387, 0.826, 1.900). So **grain density
is not a level control and must not be wired to one** — doubling the density adds 3 dB, and a patch
that sweeps density from 10 to 1000 grains/s gets 20 dB louder on the way.

**Peak grows faster than RMS at first, then slower.** Crest factor falls from 15.2 dB (sparse:
isolated grains, mostly silence) to 8.4 dB (a few overlapping) and then rises again toward the
~12 dB of Gaussian noise as the sum approaches a normal distribution. The uncomfortable consequence
for a limiter: the *quietest* setting has the highest peaks.

**Beyond the pool, grains are dropped silently.** At 5000/s the pool of 64 saturates at 63.3 live
and 900 grains never sound. The measurement counts them; a real engine must too, because the
audible symptom — a cloud that stops thickening and starts sounding wrong — is otherwise
unattributable. Count drops, expose the count, and choose the pool from `max_density ×
max_duration`.

## 10.6 What granular costs

Mean of five runs on the first machine (see [README](README.md)), against the 0.54 ns/sample
bare counter:

| variant | 1 live | 4 live | 16 live | 48 live |
|---|---|---|---|---|
| scan all 64 pool slots | 57.4 ns | 103.7 ns | 301.8 ns | 819.5 ns |
| compact live list | 20.5 ns | 69.6 ns | 270.0 ns | 811.3 ns |
| compact + table carrier | 19.4 ns | 67.1 ns | 240.6 ns | 737.4 ns |

**Keep the live grains packed.** Scanning a fixed pool costs 2.8× more than a compact list when one
grain is sounding, and converges as the pool fills. Since sparse is the normal case — most granular
patches run a handful of grains — the compact list is close to free quality.

**The marginal cost of a grain is about 16 ns** (from 20.5 ns at one grain to 811 ns at 48). Most
of it is the carrier.

**Replacing `sin()` with a table read saves 5 to 10%, not an order of magnitude.** 19.4 ns against
20.5 ns at one grain, 737 against 811 at 48. This contradicts the standard folklore, and the
explanation is that the folklore is thirty years old: a table read on a modern machine is two
memory accesses and a multiply-add, and `sin()` is no longer the catastrophe it was in 1995. The
first version of this measurement used `rem_euclid` to wrap the table phase and made the table look
*slower* than `sin()` — a division hiding inside a "cheap" lookup. `x - x.floor()` fixed it. **If a
lookup table is not measurably faster than the function it replaces, it is not an optimisation,
it is a lookup table.**

For scale: 20 live grains at 48 kHz costs roughly 300 ns/sample, or about 1.5% of one core — so a
granular *voice* is roughly a hundred times a subtractive one. That is the price of the technique,
and it is why granular instruments have voice limits that subtractive ones do not.

### 10.6.1 A sample-reading grain is a different animal, and it need not be

Every grain above is a windowed **sine**. Every grain in a granular *instrument* is a windowed read
from a stored **sample**, and the difference is an interpolator — the one thing §10.6's figures do
not contain. Spike §9f measures that grain directly: `f32` planar source, one second of it, stereo
unless marked, grains scattered over it, 20 ms long, 32 live.

Five-run means on the **second machine** (Intel i9-13900K; the tables above are the Threadripper —
see [README](README.md), and compare ratios, not nanoseconds, between the two). `per grain` is
`(ns/sample − baseline) / live grains`; `@100%` and `@25%` are grains affordable at 48 kHz in that
much of one core. Run-to-run spread is 1.2 to 6.4%.

The ladder starts at the shape `mxm-creative-sampler-dsp` ships — a sixteen-tap windowed sinc whose
coefficients are computed per read, `rem_euclid` per tap for the wrap, a `cos()` window, and a pool
scan — and removes one cost at a time:

| variant | per grain | @100% | @25% |
|---|---|---|---|
| sinc16 computed, wrapped, `cos()` window, pool scan | 69.01 ns | 302 | 75 |
| + padded source, no `rem_euclid` per tap | 59.75 ns | 349 | 87 |
| + window table | 59.18 ns | 352 | 88 |
| + compact live list | 59.98 ns | 347 | 87 |
| + block render | 57.39 ns | 363 | 91 |
| → 8-tap sinc from a 512-phase polyphase table | **10.09 ns** | **2 064** | **516** |
| → 4-point cubic Hermite | 8.87 ns | 2 348 | 587 |
| → linear | 5.97 ns | 3 492 | 873 |
| → cubic, mono source | 6.72 ns | 3 102 | 776 |
| → linear, mono source | 5.54 ns | 3 763 | 941 |

**A sample-reading grain, built properly, costs what a windowed sine costs.** 8.87 ns for a stereo
cubic read against 9.42 ns for §10.6's sine carrier, measured in the same run on the same machine.
The 16 ns budget above was never wrong about the *technique*. Reading a sample is not what makes a
grain expensive.

**The kernel is the cost, and the cost is not the tap count — it is computing the coefficients.**
Sixteen computed taps to eight tabled ones is 6.8×, and only half of that is the eight taps that went
away: a tabled eight-tap sits within 14% of a four-point cubic (10.09 against 8.87) on twice the taps.
**§9f measured cost and not quality**, so what that buys is inference from tap count and from
[§15.9](15-granular-in-the-wild.md#159-what-our-measurements-say-about-their-feature-lists)'s
measured 30 dB between linear and cubic below unity; an eight-tap kernel's alias floor has not been
measured here and should be before the choice is called settled. Four `sin_cos` calls, sixteen divisions and a normalising divide are
what the sampler's kernel actually buys per grain per sample. **Compute a kernel's coefficients once,
at build time, or pay for them on every grain on every sample forever.**

**The structural fixes are worth 1.6×, and only once the kernel stops hiding them.** The same five
rows against a cubic kernel:

| variant | per grain | @100% | @25% |
|---|---|---|---|
| cubic, wrapped, `cos()` window, pool scan | 14.08 ns | 1 479 | 370 |
| + padded source, no `rem_euclid` per tap | 9.97 ns | 2 089 | 522 |
| + window table | 9.45 ns | 2 204 | 551 |
| + compact live list | 9.18 ns | 2 268 | 567 |
| + block render | 8.76 ns | 2 379 | 595 |

1.20× against the expensive kernel, 1.61× against the cheap one, from identical edits. **Padding the
source is 1.41× of it on its own** — the taps of one read are contiguous, so a wrap resolved once and
walked beats an integer division per tap, and the placement that makes padding legal is a spawn-time
decision the grain has to make anyway.

**"Keep the live grains packed" needs its condition stated.** The 2.8× at the top of §10.6 is a pool
of 64 with one grain in it. With the pool sized from `max_density × max_duration` as
[§10.5](#105-asynchronous-clouds-statistics-not-waveforms) prescribes, compaction is worth 1.03× at
the cheap kernel and nothing at all at the expensive one — 59.18 → 59.98 ns is inside the run-to-run
spread, and the swap-remove costs about what the skipped slots save. **Compaction pays when the pool
is mostly empty, which is the sparse case, and it is not free when the pool is full.**

**Cost is overlap and nothing else, and this is the measurement that says so.** Cubic at the bottom
of the ladder, against the number of grains alive:

| live grains | 4 | 16 | 64 | 128 |
|---|---|---|---|---|
| per grain | 9.27 ns | 8.88 ns | 8.80 ns | 8.73 ns |

Flat to within the spread over a factor of 32. There is no economy of scale in a grain cloud and no
diseconomy either: `live = density × duration` from §10.5, times the cost of one windowed
interpolated read, is the whole cost model.

**Transposition is free, which removes one of the two arguments for a mipmap.** Cubic at 0.5×, 1×, 2×
and 4× playback rate measures 8.76, 8.80, 8.74 and 8.82 ns. A grain pitched two octaves up strides
four times the memory per output frame and might have been expected to pay for the cache lines; at
this overlap the working set stays inside L2 at every rate and it does not.
[15-granular-in-the-wild.md §15.10](15-granular-in-the-wild.md#1510-if-you-were-building-one) still
asks for the mipmap and should — [§15.9](15-granular-in-the-wild.md#159-what-our-measurements-say-about-their-feature-lists)
measured a 22.7 dB alias floor at 2× that no interpolator repairs — but it is a **quality** argument,
and this measurement withdraws the cost argument that was standing beside it.

### 10.6.2 What the collection actually pays

Both shipped engines, measured through their own public APIs on the second machine, on an idle
machine and not while one was building — which is a caution this repository has already paid for
once:

| engine | per live grain | pool | pool as % of one core |
|---|---|---|---|
| `mxm-grain-fx` (`grain_cost`), two-tap read, computed window | **11.53 ns** | 64 | **3.6%** |
| `mxm-grain-fx`, four-tap read, tabled window | **12.6 ns** | 64 | **4.0%** |
| `mxm-creative-sampler` Grain, as first measured | **93 ns** | 128 | **57%** |
| `mxm-creative-sampler` Grain, after the kernel table | **28.8 ns** | 256 | **35%** |
| `mxm-creative-sampler` Grain, budget shared and run compacted | **28.8 ns** | 256 | **31.9%** |

**Two granular engines in one repository, and they were eight times apart per grain.** They are not
doing different amounts of work for the listener. grain-fx reads a planar buffer with the pan
resolved at spawn; the sampler built a sixteen-tap kernel from scratch on every grain on every
sample.

**grain-fx's `MAX_GRAINS = 64` is a product choice, and by a factor of about 26.** Sixty-three live
grains cost 4.0% of one core, and 12.6 ns/grain buys 1 650 of them at 100%.

**Its window `sin()` was tabled, and the table cost half of what the `sin()` did.** The Hann end of
the morph measured 757.8 ns/frame against the ramp end's 532.9 at the same occupancy — 3.57 of its
11.53 ns, 31%, one transcendental. With 2048 points interpolated, the same gap is **1.97 ns per
grain**, not zero: a lookup into 8 KB is not free while a 1.5 MB capture buffer is already competing
for the cache. **Table a transcendental for about half its price, not all of it** — the sampler's
1.06× on a kernel-dominated grain and this 1.74 ns on a cheap one are the two ends of the same
number.

**And grain-fx spent that saving on its reader.** It interpolated linearly, which
[§15.9](15-granular-in-the-wild.md#159-what-our-measurements-say-about-their-feature-lists) prices at
about 30 dB below a four-point cubic at and under unity rate; it now reads four-point, at **2.64 ns
per grain**. Net, the engine went from 11.98 to 12.6 ns — **both arms measured in one session**,
because the 11.53 above is the same code measured in another one and only a within-session pair may
be subtracted. A 5% cost for 27 dB at 3 kHz, on an engine using 4% of a core, and it is recorded as
a trade rather than as a win.

**The sampler's grain was ten times the technique's floor, and its kernel was most of the ten.**
93 ns against the 9.42 ns a windowed sine costs in the same run, because the sixteen windowed-sinc
coefficients were built from scratch on every grain on every sample.

**That has been taken, and the answer is 28.8 ns** — the row above. What made it possible is that
the sampler's band limit is `1 / max(rate, 1)`, so every read at or below the recorded rate has a
cutoff of exactly 1.0 and shares one kernel; it is tabled at 512 phases and interpolated pairwise,
which also retires the normalising divide, because two rows that each sum to one interpolate to a
row that sums to one. Reads *above* the recorded rate keep the computed kernel, since their cutoff
narrows with pitch and no single table describes it. With the shared window table and Character
resolved once per sample rather than once per grain, the pool went from 128 grains at 57% to **256
at 35%**.

**The remaining 2.9× against the floor is sixteen taps against eight, and it was measured rather
than assumed.** Dropping this reader to eight taps takes its alias rejection at +12 st from
−45.0 dB to **−22.1 dB** and saves three points of a core. The 10 ns floor below is an 8-tap
number, and **8 taps is right for a fixed-cutoff interpolator and wrong for one that narrows with
pitch** — at a 0.5 cutoff, eight taps span about four lobes of the narrowed sinc. Quote the floor as
the technique's, not as a target every reader should hit.

**The budget, then.** On one core of a current desktop at 48 kHz, an honest stereo grain — 8-tap
polyphase sinc, tabled window, padded planar source, block-rendered — costs about **10 ns**, which is
**2 000 grains at 100% of a core and 500 at 25%**. A plugin sharing a DAW with everything else should
plan on the second number.

## 10.7 Granular as an oscillator, specifically

Everything above generalises. The narrow case — a *synchronous* grain train used as a tone
generator — deserves its own summary, because it is the one that competes with the other chapters.

Fire grains at exactly `f0`. The result is periodic with period `1/f0`, so it has a harmonic
spectrum at multiples of `f0`, and the *shape* of that spectrum is the Fourier transform of one
grain. So:

- **Grain rate sets the pitch.** Nothing else does.
- **The carrier frequency inside the grain sets a formant** — a peak in the spectral envelope that
  stays put when the pitch changes. That is exactly what a vowel is, and exactly what FOF was
  built for.
- **The grain's envelope sets the formant's bandwidth.** Roads' summary — sharper attacks produce
  broader bandwidths — is the time-frequency uncertainty principle wearing a synthesis hat. A
  short grain is a wide formant.
- **Overlap sets how many formant copies stack**, and via §10.4 whether the result has an
  amplitude ripple on top.

This gives an oscillator whose spectral envelope is controlled independently of its pitch, using
nothing but a window and a sine. It is worth knowing about even if you never ship one, because it
is the cheapest possible way to get a formant that does not move with the note — the effect a
fixed filter gives, without the filter.

Two known relatives worth naming: **VOSIM** uses a sine-squared pulse and a variable gap between
pulses, and **pulsar synthesis** treats the gap as an explicit parameter, so the duty cycle of the
train becomes a timbre control. Both are the same object with different parameterisations.

## 10.8 Consequences for this repository

The collection ships two grain engines: `mxm-grain-fx`, a granular processor on a live capture
buffer, and `mxm-creative-sampler`'s Grain reader, a cloud over a stored sample inside a sampler
voice. Both were written after this chapter and neither was written against it. These are the
settled defaults and the numbers behind them.

**The quality decisions, unchanged and still the largest numbers here:**

- **Hann envelope by default**, 78 dB better than rectangular and exactly COLA from 50% overlap.
- **Fractional onsets, always.** Rounding to the sample grid costs 71 dB and nothing is gained. A
  block-rendered engine must split its blocks at onsets rather than round them, which is what
  §9f's block variant does and what keeps it comparable with the rest of the ladder.
- **Do not wire density to loudness.** RMS goes as √density; compensate by `1/√density` or the
  patch changes level while it changes texture.
- **Preallocated pool sized from `max_density × max_duration`**, with dropped grains counted rather
  than hidden.

**The cost decisions, all of them measured in [§10.6.1](#1061-a-sample-reading-grain-is-a-different-animal-and-it-need-not-be):**

- **Table the interpolation kernel.** A polyphase table is 6.8× a kernel computed per read, and it
  is the single largest number on the cost side of this chapter. Eight tabled taps cost within 14%
  of a four-point cubic on twice the taps, so **8-tap polyphase is the default kernel**, not cubic
  and not sixteen computed taps — with the caveat that the eight-tap's *quality* is inferred from
  tap count and not yet measured, which §7.6 now records as a gap.
- **Pad the source and place the grain so it stays inside.** 1.41× on its own, and it removes an
  integer division per tap rather than making one cheaper.
- **Table the window, and expect about half the `sin()` back.** Measured across the swap on
  grain-fx: a `sin()` worth 3.57 ns per grain became a 2048-point lookup worth 1.97, because the
  table competes for cache with the audio it is windowing. Still the right move, and still the
  largest saving left on a grain whose kernel is already cheap.
- **Render a grain across a block**, so its state is loaded once per block instead of once per
  sample. 1.05× — the smallest of the four, and the one that also opens the door to vectorising.
- **Compact the live list where the pool runs mostly empty, and not reflexively.** 2.8× sparse,
  1.03× at a correctly sized full pool, nothing at all when the kernel dominates.
- **Budget about 10 ns per live stereo grain** — 2 000 at 100% of one core at 48 kHz, 500 at 25%.
  The older "~16 ns per live grain, and a granular voice costs about a hundred subtractive ones"
  still describes a *sine-carrier* grain and still holds; a sample-reading grain built properly
  costs the same, and one built carelessly costs ten times as much.

**And a grain budget, which is a product decision this chapter can now inform.** Past about twenty
live grains per voice, [§10.5](#105-asynchronous-clouds-statistics-not-waveforms) measured the crest
factor returning to the ~12 dB of Gaussian noise: the cloud has become noise shaped by the source,
and further grains change the level law rather than the texture. So per-voice grain counts above
roughly 100 buy polyphony and layering, not density.
[15-granular-in-the-wild.md §15.2](15-granular-in-the-wild.md#152-grain-budget-is-the-design-decision)
puts the hardware at 128 per voice. **Eight voices at 128 grains is 1 024 grains, which is about half
of one core at 10 ns each** — reachable on a desktop, scalar, in safe Rust, and more total grains
than the GR-1 reaches on four dedicated ARM cores.

**Closed, 2026-09-11: a pool shared across voices beats a fixed one per voice, and costs less.**
`mxm-creative-sampler` held sixteen grains per layer in a fixed array, which provisions every note
for the eight-note worst case and then charges that provisioning to every single note. Sharing one
256-grain budget among the layers actually sounding gives a held note 128 per layer and an
eight-note chord the sixteen it always had — the same ceiling, redistributed — and measured
**31.9%** of a core against 35% for the fixed pool, because a compacted live run walks O(live) over
contiguous memory instead of scanning a fixed array. Stealing was not needed to get there: a layer
simply does not spawn above its share, and live grains always finish.

Untested and worth measuring before believing: whether a BLEP correction on a hard-truncated FOF
grain recovers the 25 dB between it and Hann. It is in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps). Not measured here either: any vectorised
grain kernel. Eight grains to an AVX2 vector is the only route to the GR-MEGA's 10 000, it needs
either `unsafe` or a dependency, and both contradict standing contracts in the two DSP crates — so it
is priced in [§15.2](15-granular-in-the-wild.md#152-grain-budget-is-the-design-decision) and not
taken.

---

See also [15-granular-in-the-wild.md](15-granular-in-the-wild.md), which reads the shipping granular
instruments — the Tasty Chips hardware, Clouds, Granulator III, Pigments, Padshop — against these
measurements.

Next: [11-additive-resynthesis.md](11-additive-resynthesis.md) — what happens when the oscillator
*is* the analysis.
