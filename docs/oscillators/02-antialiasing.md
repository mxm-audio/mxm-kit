# 2. Antialiasing methods

Every family, what each one actually does, what it costs, and the measured comparison.

All Rust in this chapter is quoted from
[`crates/dsp-lab/examples/osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) or
[`crates/mxm-mono-01-dsp/src/oscillator.rs`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/src/oscillator.rs). It compiles,
it runs, and the numbers in §2.11 come from running it.

---

## 2.0 The three strategies

Välimäki and Huovilainen's 2007 survey ([08-sources.md](08-sources.md#spm2007)) sorts oscillator
algorithms into bandlimited, quasi-bandlimited, and alias-reducing. A more useful cut for someone
about to write code is by *where the bandwidth limit is imposed*:

| Strategy | Idea | Family |
|---|---|---|
| **Build only what fits** | Never generate a partial above Nyquist in the first place | additive, wavetable/mipmap |
| **Repair the discontinuity** | Generate trivially, then add a correction around each transition that is the difference between the ideal band-limited edge and the trivial one | BLIT, BLEP, minBLEP, PolyBLEP, BLAMP |
| **Change the spectral tilt** | Generate a smoother waveform whose aliasing is inherently lower, then differentiate to restore the spectrum | DPW, PTR, EPTR |

and one that sits outside the taxonomy:

| **Run faster and filter** | Generate trivially at `M·fs`, lowpass, decimate | oversampling |

The repair strategy dominates modern practice because its cost is proportional to the number of
*discontinuities*, not to the number of samples — a saw at 100 Hz and 48 kHz pays for two corrected
samples every 480, and the other 478 are a bare counter.

## 2.1 Additive synthesis

Sum the harmonics that fit:

```
y[n] = (2/π) · Σ_{k=1}^{K} (−1)^(k+1) · sin(2π k f0 n / fs) / k,   K = floor(fs / 2f0)
```

Exactly correct by construction, and useless in a realtime voice: `K` is 400 at the bottom of a
keyboard, so this is 400 sine evaluations per sample, and `K` changes as pitch moves, which makes
every pitch change a discontinuity in the harmonic count.

It is nonetheless indispensable in two roles, both of which we use:

- **As the reference** the measurement chain is validated against ([01-fundamentals.md §1.5](01-fundamentals.md#15-the-measurement-method)).
- **As the table generator** for the wavetable method below.

## 2.2 Wavetables and mipmaps

Precompute one period of a band-limited waveform into a table, read it back with interpolation.
Aliasing is then bounded by two things only: how many harmonics the table holds, and the error of
the interpolator.

The table cannot hold `fs/2f0` harmonics for every `f0`, so you build several — one per octave is
the convention, hence "mipmap" — and select by pitch. Our harness builds them additively:

```rust
fn new(inc: f64, fs: f64) -> Self {
    let mut tables = Vec::new();
    let mut hz = Self::BASE_HZ;
    while hz < fs / 2.0 {
        let harmonics = (((fs / 2.0) / hz) as usize).max(1);
        // ... additive synthesis of one period into a LEN-point table ...
        tables.push(t);
        hz *= 2.0;
    }
    // The table one octave above the note: it never holds a harmonic above
    // Nyquist for any pitch that reads it.
    let octaves = ((inc * fs) / Self::BASE_HZ).max(1.0).log2().ceil() as usize;
    let table = octaves.min(tables.len() - 1);
    ...
}
```

**Measured** (2048-point tables, linear interpolation, 44.1 kHz): −62.7 dB alias-to-signal at
55.9 Hz falling to −123.0 dB at 7 kHz. Against the best correction-function result in this chapter
that is 8 dB better at the bottom of the keyboard and 85 dB better at the top — and the gap widens
with pitch, where every other method degrades. It is why sample-playback instruments do not have an
aliasing problem.

The costs are all somewhere other than the alias column:

- **Memory.** 11 tables × 2048 × 8 bytes = 176 kB in the harness, per waveform. Per *waveform*
  matters: PWM is a continuum of waveforms, so a pulse either needs a table axis for width or has
  to be built from two sawtooth table reads.
- **Startup.** 42 ms to build the set, measured. Fine at plugin load, unacceptable inside
  `process()`, and a reason the tables must be `const`-generated or built once and shared.
- **Missing treble.** This is the one people forget. With naive one-table-per-octave selection, a
  note at the bottom of its octave band reads a table built for a pitch up to twice as high, which
  holds only half as many harmonics. Measured: at 55.9 Hz, **80 of the 394 harmonics below
  0.45·fs are more than 3 dB low** — everything above ~17.6 kHz is simply absent. The fixes are
  more tables per octave, or crossfading between two adjacent tables, and both cost more memory or
  more reads.
- **Interpolation error.** Linear interpolation between table points is a lowpass with error that
  grows with `inc`; it is the reason the 55.9 Hz row is 60 dB worse than the 7 kHz row rather than
  equal to it.

**Verdict for a virtual-analog voice:** excellent if the waveform set is fixed and small, awkward
the moment PWM, sync or waveform morphing enters, and never the simplest thing to get right.

The row in §2.11 is one point in a large design space, and the interesting decisions — interpolation
order, table length, how many tables per octave, whether to crossfade between them — are taken apart
with measurements in [09-wavetable.md](09-wavetable.md).

## 2.3 BLIT: band-limited impulse train

Stilson and Smith's 1996 approach ([08-sources.md](08-sources.md#blit)). The *derivative* of a
sawtooth is an impulse train plus a DC offset; an impulse train is easy to band-limit in closed
form (it is a sum of cosines, expressible as a ratio of sines — the Dirichlet kernel). So: generate
a band-limited impulse train, subtract the DC, integrate.

It works, and it has two structural problems that pushed the field toward BLEP:

- **The integrator is a leaky feedback path.** Any DC error, and there is always DC error,
  accumulates into a drifting offset. A leaky integrator fixes the drift and introduces a
  frequency-dependent amplitude and phase error at the low end.
- **The closed-form kernel needs a division by `sin`** that is ill-conditioned near the impulse
  centre, so practical implementations use tables anyway, at which point BLEP is strictly better.

The one measured comparison worth carrying: Välimäki, Pekonen and Nam report a BLIT using a
third-order B-spline as its impulse reaches 4593 Hz of perceptually clean fundamental, against
4591 Hz for the *same-order* B-spline PolyBLEP — a dead heat — but the BLEP needs no integrator and
no DC correction. **Integrating before synthesis beats integrating after it.**

## 2.4 BLEP: band-limited step

Instead of correcting the derivative, correct the signal. The ideal band-limited step is the sine
integral

```
h_BLEP(t) = 1/2 + (1/π)·Si(π fs t)
```

and the **BLEP residual** is `h_BLEP(t) − u(t)`, the difference between it and the trivial step —
a small, decaying, two-sided ripple centred on the discontinuity. Add the residual, scaled by the
height of the jump, to the samples around each transition and the trivial waveform becomes
approximately band-limited.

The classic implementation stores a windowed, oversampled residual in a table and reads it with
the fractional position of the discontinuity. That is straightforward and it is why "BLEP table"
appears in so many codebases. The measured cost of that choice is unflattering
([01-fundamentals.md §1.4](01-fundamentals.md#14-audibility-why-the-energy-number-is-the-wrong-number)):
a 4-sample table BLEP is clean only to 358 Hz, and you need **32 samples of table** to match what a
two-point polynomial does with two.

The reason is windowing. Truncating the residual to `K` samples reintroduces a discontinuity at the
truncation point, and windowing to avoid that smears the correction. Polynomials that are exactly
zero outside their support have no such problem.

There is also a hard limit nobody mentions until it bites: **when two discontinuities are closer
than `K` samples, their corrections overlap.** For a saw, that means the table method caps out at
`f0 = fs/K`. For a pulse it happens far sooner, because a narrow pulse puts its two edges close
together at any pitch. See [03-waveshapes.md §3.3](03-waveshapes.md#33-the-two-edge-problem).

## 2.5 minBLEP

Brandt's 2001 refinement ([08-sources.md](08-sources.md#minblep)). The BLEP residual is two-sided —
it corrects samples *before* the discontinuity, which you have not yet been told about. In an
offline or block-lookahead context that is fine; in a per-sample loop it means latency.

minBLEP takes the minimum-phase version of the windowed band-limited step, obtained by cepstral
factorisation. All of its energy is concentrated at the start, so the correction is causal: it only
touches samples *after* the transition, and no lookahead is needed.

Brandt's motivation was hard sync, where discontinuities land at arbitrary times set by another
oscillator and lookahead is genuinely awkward. It is still the standard technique for sync — see
[03-waveshapes.md §3.6](03-waveshapes.md#36-hard-sync).

The costs: the table has to be precomputed offline (cepstral factorisation is not something to do
at plugin load), minimum phase means the correction is not symmetric so its phase response is not
flat, and it is still a table with all of §2.4's sizing problems.

## 2.6 PolyBLEP

Replace the tabulated residual with a low-order polynomial. This is the method mxm-mono-01 ships, and
for most instruments it is the right default.

The derivation, from Välimäki, Pekonen and Nam: take a polynomial interpolation kernel — Lagrange
or B-spline — as a stand-in for the sinc, integrate it to get an approximate band-limited step,
subtract the unit step, and you have a residual that is a piecewise polynomial in the fractional
delay `d`, exactly zero outside its support.

`d` is the fraction of a sample by which the first sample *after* the discontinuity trails it. All
spans of a given residual take the same `d`; it is a property of where the edge fell, not of which
sample is being corrected. (Pekonen et al. state this convention explicitly for their four-point
filter: "`d` is the fractional delay from the discontinuity to the sample following it".)

### 2.6.1 Two-point (integrated linear interpolation)

The residual is two quadratic pieces:

| Span | Residual |
|---|---|
| `[−T, 0]` — the sample before the edge | `d²/2` |
| `[0, T]` — the sample after the edge | `−d²/2 + d − 1/2` |

Our shipped implementation folds the factor-of-two step height in, so it returns twice the residual
and the caller subtracts:

```rust
#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
pub fn saw(p: &Phasor) -> f32 {
    2.0 * p.phase() - 1.0 - poly_blep(p.phase(), p.inc())
}
```

Two comparisons and, on the ~2 samples per period that need it, a handful of arithmetic. There is
no table, no state, and no latency.

**Measured**: −44.2 dB at 55.9 Hz to −25.5 dB at 7 kHz, at 1.58 ns/sample against the trivial
counter's 0.54 ns. Perceptually clean to `f0 = 2135 Hz` by the published NMR analysis.

### 2.6.2 Four-point (third-order B-spline)

Two samples each side. From the same paper's Table VII, cross-checked against the polyBLAMP residual
in Esqueda, Välimäki and Bilbao's DAFx-16 paper, which differentiates to exactly these four
polynomials — a useful independent check, since both papers print the coefficients with signs that
are easy to transcribe wrongly:

```rust
#[inline]
fn blep4(d: f64) -> [f64; 4] {
    let d2 = d * d;
    let d3 = d2 * d;
    let d4 = d2 * d2;
    [
        d4 / 24.0,
        -d4 / 8.0 + d3 / 6.0 + d2 / 4.0 + d / 6.0 + 1.0 / 24.0,
        d4 / 8.0 - d3 / 3.0 + 2.0 * d / 3.0 - 1.0 / 2.0,
        -d4 / 24.0 + d3 / 6.0 - d2 / 4.0 + d / 6.0 - 1.0 / 24.0,
    ]
}
```

Sanity checks that catch a wrong transcription immediately: at `d → 0` the residual is
`[0, 1/24, −1/2, −1/24]`, and `−2 × (−1/2) = +1` added to the trivial value of `−1` puts the sample
exactly at the midpoint of the step, which is what a band-limited step does when the edge lands on
a sample. At `d → 1` the pattern shifts by one sample and the middle term flips sign by exactly 1 —
the trivial step itself moving across the sample.

The implementation cost is not the polynomial, it is the plumbing: correcting samples *before* the
edge means a two-sample output delay and a small ring buffer.

```rust
fn step(&mut self) {
    let v = 2.0 * self.phase - 1.0;
    self.phase += self.inc;
    if self.phase >= 1.0 {
        self.phase -= 1.0;
        let r = blep4(self.phase / self.inc);
        // The trivial sawtooth jumps by -2 at the wrap.
        let h = -2.0;
        // Spans [-2T,-T], [-T,0], [0,T], [T,2T] are samples i-1, i, i+1, i+2.
        let i = self.i;
        self.corr[(i + Self::RING - 1) % Self::RING] += h * r[0];
        self.corr[i % Self::RING] += h * r[1];
        self.corr[(i + 1) % Self::RING] += h * r[2];
        self.corr[(i + 2) % Self::RING] += h * r[3];
    }
    self.triv[self.i % Self::RING] = v;
    self.i += 1;
}
```

**Measured**: −54.2 dB at 55.9 Hz to −38.5 dB at 7 kHz — a consistent **10 dB better than the
two-point version** — at 1.70 ns/sample against 1.58 ns, and two samples of latency.

### 2.6.3 Which order, and Lagrange or B-spline

Two findings from the literature, both worth obeying:

- **B-spline beats Lagrange at the same order**, because B-spline kernels are built from repeated
  convolution of a rectangle, so their spectra decay as `sinc^(N+1)` and the correction rings less.
  Measured by the authors as 4591 Hz vs 3236 Hz at order 3, and 7845 Hz vs 5134 Hz at order 4.
- **Prefer even orders.** An odd-order PolyBLEP needs extra control logic to choose which
  polynomial applies depending on which side of a sample the edge fell, and ends up at least as
  expensive as the next even order, which is strictly better.

### 2.6.4 The droop, and the equaliser

Every PolyBLEP attenuates the top of the spectrum, because the polynomial kernel is a worse lowpass
than a sinc and it rolls off before Nyquist. This is the part of the method that alias-only
comparisons hide, so, measured at 44.1 kHz on our own code:

| Method | Max harmonic error below 0.45·fs | Harmonics >3 dB low | Where the 3 dB point falls |
|---|---|---|---|
| Trivial | 0.00 dB | 0 | — |
| DPW2 / EPTR | −3.11 dB | 6 of 394 | ~21.7 kHz |
| **PolyBLEP2 (shipped)** | **−6.22 dB** | **104 of 394** | **~16.2 kHz** |
| PolyBLEP4 | −12.44 dB | 176 of 394 | ~12.2 kHz |

(at `f0 = 55.9 Hz`, where there are enough harmonics for the numbers to mean something; the 3 dB
frequency is stable across fundamentals — 15.9 kHz for PolyBLEP2 at 440 Hz, 16.1 kHz at 220 Hz.)

So a four-point PolyBLEP buys 10 dB of alias suppression by giving up an extra 6 dB at the top of
the spectrum and moving the corner down 4 kHz. Whether that is a good trade depends entirely on
whether a lowpass filter is about to remove that region anyway — which, in a subtractive
synthesizer, it usually is.

The fix, if you want both: a fixed two-tap FIR equaliser after the oscillator. The JASA paper gives
optimised coefficients per method (its Table IV) for

```
H(z) = b0 + b1·z⁻¹ + b0·z⁻²
```

and reports harmonic levels within 1 dB of ideal afterwards. It is frequency-independent, so it is
three multiply-adds per sample for the whole oscillator section, applied once to the mix rather
than per source. **We have not implemented or measured this**; it is recorded as an open item in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

## 2.7 DPW, PTR and EPTR

A different idea, and a genuinely elegant one. Integrating a signal tilts its spectrum by
−6 dB/octave; differentiating tilts it back. So: integrate the sawtooth analytically *before*
sampling — the integral of a ramp is a parabola — sample the parabola, which aliases far less
because its spectrum falls at 12 dB/octave, then difference the samples to restore the tilt.

```rust
fn next(&mut self) -> f64 {
    let tri = 2.0 * self.phase - 1.0;
    let par = tri * tri;
    let y = (par - self.prev) * self.scale;   // scale = 1 / (4T)
    self.prev = par;
    self.phase += self.inc;
    if self.phase >= 1.0 { self.phase -= 1.0; }
    y
}
```

Squaring the trivial saw *is* the sampled parabola, so the whole method is: square, difference,
scale by `1/(4T)`. Three operations and one sample of memory.

Higher orders integrate more times and difference more times: an `N`th-order DPW makes the
pre-sampling spectrum fall at `6N` dB/octave. The cost is `N−1` samples of state and an increasingly
ugly `1/T^(N−1)` scaling that loses precision at low `f0`.

**PTR** (Kleimola and Välimäki, 2012) is the observation that an `N`th-order DPW output differs
from the trivial waveform in only `N−1` samples per period; the rest are the trivial value plus a
constant offset. So skip the integrate-and-difference machinery and apply a closed-form correction
polynomial in the transition region, exactly like a BLEP but derived from DPW.

**EPTR** (Ambrits and Bank, 2013) removes PTR's remaining half-sample offset by deriving the
correction from a half-sample-advanced counter, so the linear region needs no addition at all —
the trivial counter *is* the output. Their derivation, for `A = 1`, `p_max = 1`, `p_min = −1`:

```
before the discontinuity (p₀ > 1 − T):   y = p₀ − p₀/T + 1/T − 1
after  the discontinuity (p₀ < −1 + T):  y = p₀ − p₀/T − 1/T + 1
otherwise:                               y = p₀
```

which is worth deriving once yourself, because it drops out of two lines of algebra. Both cases are
`(p₀ ∓ 1)(T − 1)/T`.

```rust
fn next(&mut self) -> f64 {
    let (p, t, r) = (self.counter, self.t, self.inv_t);
    let y = if p > 1.0 - t {
        p - p * r + r - 1.0
    } else if p < -1.0 + t {
        p - p * r - r + 1.0
    } else {
        p
    };
    self.counter += 2.0 * t;
    if self.counter >= 1.0 { self.counter -= 2.0; }
    y
}
```

**Measured, and this is the useful part:** EPTR and DPW2 produce *identical spectra*, to every digit
of every column, across all eight test frequencies. That is the papers' central claim and it
verifies both implementations at once — if either had a sign error they would diverge.

**Also measured, and this one contradicts the paper's headline:** EPTR is not cheaper here. On this
machine, with reciprocals precomputed for both, DPW2 runs at 0.68 ns/sample and EPTR at 1.17 ns —
EPTR is **1.7× slower**, where the operation count predicts it should be ~30% faster. The
explanation is that operation counts do not price branches. DPW2's per-sample work is one multiply,
one subtract and one multiply, entirely branchless and pipelineable; EPTR's is two comparisons
whose outcome the branch predictor cannot know in advance. On a 1980s DSP the operation count was
the cost model. On a superscalar out-of-order core it is not.

This is not a criticism of the papers — their claim is about operation counts and it is correct.
It is a reminder that **an operation-count argument is a hypothesis about performance, not a
measurement of it**, and the repo's rule about measuring rather than expecting
([`../../AGENTS.md`](../../AGENTS.md)) applies to complexity claims from the literature too.

**Quality-wise DPW2/EPTR sit clearly below the BLEP family**: −38.3 dB at 55.9 Hz against
PolyBLEP2's −44.2 dB by energy, and ~900 Hz against 2135 Hz by NMR. Their one real advantage is the
mildest high-frequency droop of any method here (−3.1 dB, corner ~21.7 kHz), which makes them
interesting if brightness matters more than the last 6 dB of alias suppression.

## 2.8 BLAMP: corners rather than steps

A triangle wave has no discontinuity in the signal, only in its slope. Correcting it needs the
*integral* of a BLEP residual — the band-limited ramp, or BLAMP — scaled by the change in slope
rather than the size of a jump.

The four-point polyBLAMP residual from Esqueda, Välimäki and Bilbao:

| Span | Residual |
|---|---|
| `[−2T, −T]` | `d⁵/120` |
| `[−T, 0]` | `−d⁵/40 + d⁴/24 + d³/12 + d²/12 + d/24 + 1/120` |
| `[0, T]` | `d⁵/40 − d⁴/12 + d²/3 − d/2 + 7/30` |
| `[T, 2T]` | `−d⁵/120 + d⁴/24 − d³/12 + d²/12 − d/24 + 1/120` |

The authors report up to 50 dB of alias reduction and about 20 dB of SNR improvement, and note it
is more efficient than oversampling for the same job. Its scope is wider than triangles: any
slope discontinuity, including **hard clipping and rectification**, which is why the technique
matters for waveshapers and not only oscillators.

mxm-mono-01 has no triangle and no oscillator-stage clipper, so we do not use it. It is here because
it is the correct answer to a question that arrives the moment a triangle or a fold does.

## 2.9 Oversampling

Generate the trivial waveform at `M·fs`, lowpass, throw away `M−1` of every `M` samples. It needs
no understanding of the waveform at all, which is its entire appeal.

**Measured**, with a 64·M+1-tap windowed-sinc decimator — deliberately generous, so the filter is
not what limits the result:

| Method | alias @ 55.9 Hz | alias @ 7 kHz | ns/sample | vs trivial |
|---|---|---|---|---|
| trivial + 2× | −35.4 dB | −13.0 dB | 431 | 798× |
| trivial + 4× | −41.5 dB | −19.6 dB | 858 | 1588× |
| trivial + 8× | −47.6 dB | −25.6 dB | 1714 | 3174× |
| PolyBLEP4 | −54.2 dB | −38.5 dB | 1.70 | 3.1× |

Read that table twice. **Eight-times oversampling with a long filter is still 6 dB worse than a
four-point polynomial correction, at roughly three thousand times the cost.** Even allowing that a
production implementation would use a polyphase halfband cascade and be several times cheaper than
this direct-form filter, the conclusion does not move: for *oscillators*, oversampling is the
wrong tool.

The reason is structural. Oversampling attenuates aliases after they exist, and the trivial
sawtooth's aliases are enormous — it starts from −28 dB and each doubling of the rate buys about
6 dB. A correction function prevents most of them from being generated. You cannot filter your way
out of a 6 dB/octave spectrum that extends to infinity.

Note the contrast with [`../filters/`](../filters/README.md), where oversampling a *nonlinear
filter* was measured to buy harmonic accuracy rather than alias reduction. Different problem,
different answer, same lesson: measure what the oversampling is actually buying before paying
for it.

## 2.10 Antiderivative antialiasing, in passing

ADAA — integrate the nonlinearity analytically, then difference the result — is the equivalent
technique for waveshapers, and it is the same mathematical move as DPW: shift the spectral tilt,
sample, shift it back. It is listed here so the family resemblance is on record. It does not apply
to a plain oscillator, which has no input signal to shape. It becomes relevant the moment a
waveshaper, folder or clipper appears in the oscillator section.

## 2.11 The comparison table

All figures measured by
[`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) at 44.1 kHz, `N = 65536`,
AMD Ryzen Threadripper 3970X, rustc 1.98.0 release. `alias` is total aliased energy over total
wanted-harmonic energy; `gone` is how many harmonics below 0.45·fs came out more than 3 dB low.
Spectra come from one run and are bit-identical between runs; the nanosecond column is the mean of
five, whose spread is under 3% for every row except the wavetable's 52%.

| Method | alias @55.9 Hz | @440.8 Hz | @7040 Hz | h.err | gone @55.9 Hz | ns/sample | latency | memory |
|---|---|---|---|---|---|---|---|---|
| ideal (additive) | −246.0 | −241.6 | −230.1 | 0.00 | 0 | — | 0 | — |
| trivial | −28.1 | −19.1 | −6.8 | 0.00 | 0 | 0.54 | 0 | 0 |
| DPW2 | −38.3 | −29.4 | −18.2 | −3.11 | 6 | 0.68 | 0 | 1 sample |
| EPTR | −38.3 | −29.4 | −18.2 | −3.11 | 6 | 1.17 | 0 | 0 |
| **PolyBLEP2 (shipped)** | **−44.2** | **−35.4** | **−25.5** | **−6.22** | **104** | **1.58** | **0** | **0** |
| PolyBLEP4 (B-spline) | −54.2 | −45.5 | −38.5 | −12.44 | 176 | 1.70 | 2 samples | 8 f64 |
| trivial + 2× OS | −35.4 | −26.4 | −13.0 | −5.84 | 7 | 431 | ~64 samples | 129 taps |
| trivial + 4× OS | −41.5 | −32.5 | −19.6 | −5.84 | 7 | 858 | ~64 samples | 257 taps |
| trivial + 8× OS | −47.6 | −38.5 | −25.6 | −5.84 | 7 | 1714 | ~64 samples | 513 taps |
| wavetable (mipmap) | −62.7 | −89.8 | −123.0 | (missing) | 80 | 7.9 | 0 | 176 kB |

Reading it:

- **Cost is not the discriminator among the per-sample methods.** Trivial, DPW2, EPTR, PolyBLEP2
  and PolyBLEP4 all land between 0.54 and 1.70 ns/sample. In a monophonic synth at 48 kHz that is
  the difference between 0.003% and 0.009% of one core. Choosing DPW over PolyBLEP to save
  operations is optimising something that was never the bottleneck.
- **Oversampling and wavetables are the only rows with real costs**, and they are opposite kinds:
  time versus memory. The wavetable row is also the only one whose cost is *memory-bound* rather
  than compute-bound: an earlier build of the same code, in a different cache environment, measured
  it at 4.2 ns rather than 7.9. Treat it as a range, and see
  [09-wavetable.md §9.6](09-wavetable.md#96-what-a-wavetable-actually-costs).
- **The wavetable's alias figure improves with pitch while everything else degrades**, because its
  error is dominated by interpolation at low `inc`, not by folding. If aliasing at the top of the
  keyboard is the specific worry, it is the only method that answers it outright.
- **`gone` is the column that stops this table being a beauty contest.** The two methods with the
  best alias numbers per unit cost — PolyBLEP2 and PolyBLEP4 — are also the two that give away the
  most treble.

## 2.12 Discrete summation formulae

§2.0 sorted the field into three strategies and left one out. Moorer's **discrete summation
formulae** belong to the first — build only what fits — but they build it in closed form rather than
by summing oscillators or reading a table.

The identity is that a geometrically weighted harmonic sum has an exact closed form:

```text
Σ_{k=1..K} a^k sin(kθ)
    = [a sinθ − a^(K+1) sin((K+1)θ) + a^(K+2) sin(Kθ)] / (1 − 2a cosθ + a²)
```

So a waveform containing exactly `K` harmonics and nothing above them costs **O(1) per sample
whatever `K` is** — three sines, one cosine and a divide, whether `K` is 20 or 400. That is the
appeal, and it is a real one: additive synthesis of a 355-harmonic bass note costs 355 oscillators
([11-additive-resynthesis.md §11.2](11-additive-resynthesis.md#112-three-ways-to-make-a-sine-measured)),
and this costs four transcendentals.

Measured, with `a` chosen so the highest harmonic sits 60 dB below the first:

| `f0` | harmonics | alias | spectral tilt | ns/sample |
|---|---|---|---|---|
| 55.9 Hz | 355 | −247.9 dB | −0.39 dB/oct | 32.3 |
| 220.0 Hz | 90 | −248.1 dB | −1.54 dB/oct | 33.7 |
| 880.8 Hz | 22 | −248.3 dB | −6.31 dB/oct | 32.1 |

**It does not alias at all.** −248 dB is the measurement floor: nothing above harmonic `K` is ever
generated, so there is nothing to fold. In that respect it beats every correction function in §2.11
outright.

**Its spectral tilt is the problem.** The harmonic amplitudes fall as `a^k` — geometrically — where
a sawtooth's fall as `1/k`. With `a` set from a fixed "top harmonic at −60 dB" rule, the tilt comes
out at −0.39 dB/octave at the bottom of the keyboard, which is nearly flat and sounds nothing like a
sawtooth, and −6.31 dB/octave two octaves up, which is a sawtooth almost exactly. **The waveform's
shape changes with pitch.** Getting a consistent timbre means choosing `a` per note, or
post-filtering, and neither is free.

**And O(1) is not the same as cheap.** 32 ns/sample against the shipped PolyBLEP sawtooth's 1.58 —
twenty times more — because four transcendentals and a divide is a lot of work to do every sample.
DSF only wins on cost where the alternative is a genuinely large oscillator bank: it beats additive
synthesis somewhere above about ten partials, and loses to a correction function everywhere.

So the honest place for DSF in the taxonomy: **the method to reach for when you need an exactly
band-limited spectrum whose tilt you are willing to control yourself**, and specifically when the
harmonic count is high enough to make additive synthesis expensive. For a classic analog waveform it
is the wrong tool, which is why §2.11's table does not have a row for it — the comparison would be
between a sawtooth and something that is not one.

---

Next: [03-waveshapes.md](03-waveshapes.md) — applying all this to the waveforms an actual synth has.
