# 9. Wavetable oscillators

[Chapter 2 §2.2](02-antialiasing.md#22-wavetables-and-mipmaps) treated wavetables as one
antialiasing method among several and gave them one row in the comparison table. That row hides
every decision that actually matters. This chapter takes the method apart.

A wavetable oscillator has exactly two quality knobs, and they are independent:

- **How you read between the stored points** sets the aliasing floor.
- **How many tables you store** sets how much treble you lose.

Almost every wavetable oscillator that sounds wrong is wrong in one of those two ways, and
measuring only one of them is how it gets shipped. All figures below are from
[`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §8 at 44.1 kHz.

---

## 9.1 The shape of the thing

One period of a waveform in a table, a phase accumulator, and a read:

```rust
impl Osc for WtOsc<'_> {
    fn next(&mut self) -> f64 {
        let x = self.phase * self.bank.len as f64;
        let y = /* read the table at fractional position x */;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}
```

The phasor is the same one as [chapter 1](01-fundamentals.md#11-the-phasor). Everything specific to
wavetables is in `read`, and in what the table contains.

**The table must already be band-limited**, because nothing downstream will do it for you. A table
holding a mathematically perfect sawtooth is a table full of aliasing waiting to happen: read it at
any pitch and every harmonic it contains above Nyquist folds. So tables are built by additive
synthesis with a harmonic count chosen for the pitch that will read them — and since pitch varies,
you need more than one table.

One constraint that is easy to miss and that our first version of the harness got wrong: **a table
of `L` points cannot hold more than `L/2 − 1` harmonics.** Build a 64-point table by summing 68
harmonics and the table itself aliases while it is being built. The measurement then blames the
interpolator for the table's own folding:

```rust
// A table cannot hold a harmonic above its own Nyquist: an L-point
// table carries L/2 - 1 of them. Without this clamp a short table
// aliases while it is being *built*, and the resulting measurement
// blames the interpolator for the table's own folding.
let harmonics = (((fs / 2.0) / hz) as usize)
    .max(1)
    .min(len / 2 - 1);
```

## 9.2 Interpolation is the alias floor

Measured with 2048-point tables, one table per octave, reading only the table that is safe for the
pitch — so the *only* difference between these three blocks is the read:

| `f0` | drop-sample | linear | 4-point cubic |
|---|---|---|---|
| 55.9 Hz | −38.9 dB | −62.7 dB | −75.6 dB |
| 220.0 Hz | −44.9 dB | −80.9 dB | −106.8 dB |
| 880.8 Hz | −50.8 dB | −98.5 dB | −136.4 dB |
| 3520.0 Hz | −56.6 dB | −115.8 dB | −165.6 dB |

Three things fall out of this table.

**Drop-sample interpolation is not an option.** −38.9 dB at the bottom of the keyboard is worse
than the two-point PolyBLEP sawtooth of [chapter 2](02-antialiasing.md), which needs no table at
all. If the read is a truncation, the wavetable has bought nothing.

**Linear is respectable and cubic is transparent.** −62.7 dB against −75.6 dB at 55.9 Hz, and the
gap widens with pitch: 50 dB apart by 3.5 kHz.

**Every method here improves with pitch, which is the opposite of everything else in this
reference.** A correction-function oscillator degrades as `f0` rises because more harmonics fold. A
wavetable's error is interpolation error, which depends on how far apart the read positions are —
and at high pitch the table is being read in big strides across a table that holds very few
harmonics, so there is almost nothing left to interpolate wrongly. This is why wavetables are the
right answer specifically at the top of the keyboard.

## 9.3 Table length trades against interpolation order

At 220 Hz, one table per octave, varying only the length:

| length | drop | linear | cubic | harmonics >3 dB low | bank | build time |
|---|---|---|---|---|---|---|
| 64 | −23.3 dB | −45.5 dB | −60.6 dB | 65 | 5 kB | 0 ms |
| 256 | −26.9 dB | −44.4 dB | −50.0 dB | 22 | 22 kB | 2 ms |
| 1024 | −38.8 dB | −68.8 dB | −88.5 dB | 22 | 88 kB | 16 ms |
| 4096 | −50.9 dB | −92.8 dB | −124.9 dB | 22 | 352 kB | 88 ms |
| 16384 | −63.2 dB | −115.1 dB | −161.4 dB | 22 | 1408 kB | 350 ms |

**The scaling is clean and worth memorising.** Every quadrupling of table length buys

- **12 dB** with drop-sample,
- **24 dB** with linear,
- **36 dB** with 4-point cubic.

(Measured: linear 1024 → 4096 is −68.8 → −92.8, and 4096 → 16384 is −92.8 → −115.1. Cubic:
−88.5 → −124.9 → −161.4.)

That is the whole engineering trade in one line. **Doubling the interpolation order is worth about
as much as sixteen times the table length**, and costs a few multiply-adds instead of memory.
Bristow-Johnson made exactly this point in 1996 — "if linear interpolation (or worse yet,
drop-sample interpolation) is used, a larger wavetable is required to restrain interpolation error
than if a more legitimate method of fractional sample interpolation is being used" — and the
measured numbers put a size on it.

**The 64-point row is a trap worth understanding.** It beats the 256-point row on aliasing
(−45.5 dB against −44.4 dB) while being obviously worse. It wins because it *cannot hold* the
harmonics: clamped to 31, it is simply duller, and 65 of the 90 harmonics that belong below
0.45·fs are more than 3 dB down. An alias column on its own would have recommended it.

## 9.4 Bank resolution is the treble

The second knob. With linear interpolation and 2048-point tables, varying how many tables cover an
octave and whether the oscillator blends toward the brighter neighbour:

| `f0` | tables/octave | crossfade | alias | worst harmonic error | harmonics >3 dB low | bank |
|---|---|---|---|---|---|---|
| 55.9 Hz | 1 | no | −62.7 dB | −276 dB (absent) | 80 | 176 kB |
| 220.0 Hz | 1 | no | −80.9 dB | −289 dB (absent) | 22 | 176 kB |
| 55.9 Hz | 1 | **yes** | −40.7 dB | −6.57 dB | 80 | 176 kB |
| 220.0 Hz | 1 | **yes** | −33.3 dB | −5.40 dB | 22 | 176 kB |
| 55.9 Hz | **2** | no | −58.0 dB | −0.87 dB | **0** | 336 kB |
| 220.0 Hz | **2** | no | −76.3 dB | −0.06 dB | **0** | 336 kB |
| 55.9 Hz | 4 | no | −58.0 dB | −0.87 dB | 0 | 656 kB |
| 220.0 Hz | 4 | no | −76.3 dB | −0.06 dB | 0 | 656 kB |
| 55.9 Hz | 12 | no | −58.0 dB | −0.87 dB | 0 | 1952 kB |
| 220.0 Hz | 12 | no | −76.3 dB | −0.06 dB | 0 | 1952 kB |

Three findings, and the second one contradicts common practice.

**One table per octave costs an octave of treble.** A note at the bottom of its band reads a table
built for a pitch up to twice as high, holding half as many harmonics. At 55.9 Hz that is 80 of
394 harmonics simply absent — everything above about 17.6 kHz.

**Crossfading toward the brighter table is a bad trade.** It does restore the missing harmonics —
worst-case error goes from *absent* to −6.6 dB — but it imports the brighter table's aliasing along
with them, and the measured price is **40 to 47 dB**: −80.9 dB becomes −33.3 dB at 220 Hz. That is
worse than a bare two-point PolyBLEP. Crossfading between mip levels is a technique borrowed from
graphics, where the artefact it prevents (visible level popping) is worse than the one it causes.
In audio the trade runs the other way, because a level step between tables one semitone apart is
inaudible and 40 dB of aliasing is not.

**Two tables per octave is the whole answer, and four is waste.** With two, nothing below 0.45·fs
is more than 3 dB low and the worst harmonic error is 0.87 dB, for 336 kB. Four and twelve tables
per octave measure *identically* — same alias figure, same harmonic error, same zero missing
harmonics — at two and six times the memory. There is nothing left to buy.

The mechanism is simple once seen: the safe table for a pitch holds `fs/(2·f_band_top)` harmonics,
and with two bands per octave `f_band_top ≤ f0·√2`, so the table always holds at least
`fs/(2·√2·f0)` = 71% of the harmonics that fit. Everything above 0.45·fs is outside the measurement
and inaudible anyway.

## 9.5 Crossfading between *different* waveforms is a different problem

Everything above is about crossfading between mip levels of the *same* waveform. Crossfading
between two genuinely different wavetables — the wavetable-position axis that the format is named
for — is the interesting case, and it has its own failure mode.

Bristow-Johnson's warning is the one to remember: adjacent wavetables must be **phase-locked**. If
two tables have similar spectra but their partials are at different phases, crossfading them
produces "an unintended null when the crossfade is half complete" — the two copies partially cancel,
the sound dips in the middle of the morph, and it reads as a badly-made table rather than as a
badly-made crossfade.

The fix is in the analysis, not the playback: rotate each table so its partials are phase-aligned
with its neighbours before storing them. It costs nothing at runtime and cannot be added later.

## 9.6 What a wavetable actually costs

Measured at 440 Hz, mean of five runs, against the 0.54 ns/sample bare counter of
[chapter 2](02-antialiasing.md#211-the-comparison-table):

| read | ns/sample | vs a bare counter |
|---|---|---|
| drop-sample | 6.12 | 11× |
| linear | 7.91 | 15× |
| 4-point cubic | 13.59 | 25× |
| cubic + mip crossfade | 25.25 | 47× |

Plus construction: 176 kB and about 40 ms for a 2048-point, one-per-octave bank; 1.4 MB and 350 ms
at 16384 points. Construction is a plugin-load cost — additive synthesis of a hundred-odd tables
has no business anywhere near `process()`.

**These are the least reliable numbers in this reference, and the reason is worth stating.** The
per-sample work is a handful of multiply-adds; the cost is dominated by whether the table is in
cache. The same code measured 4.2 ns in one build environment and 7.9 ns in another, and within a
single session the run-to-run spread on these rows reached 50%. A wavetable oscillator's real cost
depends on how many voices are reading how many tables and what else is competing for L2 — which
means it is a *system* property, not a property of the oscillator, and it cannot be benchmarked in
isolation with any honesty. Budget for the memory traffic, not for the arithmetic.

Note also that the crossfade row costs 1.9× the plain cubic read, because it is two reads. Since
§9.4 already showed that mip crossfading is a 40 dB mistake, this is a cost paid for a downgrade.

## 9.7 What wavetables are actually for

Everything measured above uses a sawtooth, because that makes it comparable with the rest of this
reference. That is not what wavetables are for, and judging them on a sawtooth is unfair in both
directions.

**Against:** for a classic analog waveform, a correction function wins outright. A two-point
PolyBLEP is one line of arithmetic, no memory, no build step, no mip selection, and it handles
**pulse-width modulation** — a continuum of waveforms — for free. A wavetable pulse needs either a
table axis for width (memory × the number of width steps) or two sawtooth reads and a subtraction,
which is the PolyBLEP structure with extra steps.

**For:** a wavetable can hold *any* waveform, including ones with no closed form — a single cycle
extracted from a recording, a shape drawn by hand, a frame from a spectral analysis. That is the
whole point, and no correction function can do it, because there is no discontinuity to correct
when you do not know where the discontinuities are. Chapter 11's resynthesis engines are, in this
sense, wavetable oscillators whose tables are recomputed continuously.

**The honest summary:** wavetables are a *storage* technique that happens to be band-limited, not
an antialiasing technique that happens to use storage. Reach for them when the waveform is data.

## 9.8 Consequences for this repository

mxm-mono-01 does not use wavetables and should not: it has one sawtooth, one pulse with PWM, and a
sub — three waveforms, all with closed forms, all cheaper and better served by the shipped
PolyBLEP ([07-rust-recipes.md §7.5](07-rust-recipes.md#75-what-not-to-do)).

If a future MXM instrument does need them, the measured defaults are:

- **4-point cubic** interpolation, always. It is 5.7 ns/sample more than linear and worth 13 to
  50 dB.
- **2048-point tables, two per octave.** −106 dB with cubic at 220 Hz, nothing missing below
  0.45·fs, 336 kB per waveform.
- **No mip crossfading.** Spend the memory on the extra band instead; it is better on both axes.
- **Phase-lock adjacent waveform tables** at build time, or morphing will null.
- **Build the bank once, share it across voices**, and budget the cache traffic rather than the
  arithmetic.

## 9.9 Vector synthesis and wave morphing

§9.5 warned that crossfading two wavetables that are not phase-locked produces "an unintended null
when the crossfade is half complete". That warning is the whole engineering content of **vector
synthesis** — the Prophet VS, the Korg Wavestation, and the wavetable-position axis of every modern
soft synth.

The idea is a joystick: four (or more) waveforms at the corners, a position that mixes between them,
and an envelope that moves the position over time. Sequential's VS made the joystick physical;
Waldorf's wavetables made the axis one-dimensional and long.

Everything that makes it work or fail is in §9.5, so this section is mostly a pointer, with two
additions.

**Amplitude crossfading is not spectral morphing.** Mixing table A and table B at 50/50 gives you the
*sum* of two spectra, not a spectrum halfway between them. If A has a partial at 3 kHz and B does
not, the 50% position has that partial at half amplitude — it does not move, shift or transform. A
true morph would interpolate the partials' *frequencies* as well as their amplitudes, which is a
partial-domain operation and belongs to [chapter 11](11-additive-resynthesis.md), not here. Most
"morphing" wavetable synths crossfade, and the distinction is audible on anything with strong
inharmonic content.

**The null is worst where the tables are most similar.** Two tables with unrelated spectra crossfade
without much cancellation, because their partials rarely coincide. Two tables that are near-copies
at different phases cancel almost completely at the midpoint. So the failure appears exactly where a
designer expects the smoothest result — adjacent frames of an analysed sound — which is why
Bristow-Johnson's phase-locking step is a build-time requirement and not an optional polish.

---

Next: [10-granular.md](10-granular.md) — the other way to make an oscillator out of stored sound.
