# 17. Waveshaping, folding and ADAA

Take a sine and bend it. That is the other way to build a spectrum — not by adding partials
([chapter 11](11-additive-resynthesis.md)) or by band-limiting a discontinuity
([chapter 2](02-antialiasing.md)), but by pushing a clean signal through a nonlinearity and letting
the harmonics fall out.

It is the West Coast tradition — Buchla's timbre section, Serge's wave multipliers — and it is also
every saturator, every clipper and every distortion in the collection's future effects chain. It has
the worst aliasing story in this reference, and the most interesting fix.

Figures from [`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §19 at 44.1 kHz.

---

## 17.1 Why it aliases so badly

A waveshaper is a memoryless function `y = f(x)`. Feed it a sine and the output is periodic at the
same frequency, so its spectrum is harmonics of the input — but *how many* harmonics depends
entirely on `f`.

- A polynomial of order `n` produces exactly `n` harmonics. Bounded, predictable, finite.
- A saturator like `tanh` produces infinitely many, decaying at a rate set by the drive.
- A **folder** — a function that turns back on itself — produces a great many, and more the harder
  you drive it.

That last case is what makes folding a sound-design tool and an antialiasing problem at once. The
knee of a folder is a corner, and corners have 12 dB/octave spectra that extend forever; each
additional fold adds another corner.

None of it can be corrected by the methods of [chapter 2](02-antialiasing.md), because there is no
*known* discontinuity to place a residual at — the fold happens wherever the input signal happens to
cross the threshold, which changes every sample.

## 17.2 The measured picture

A 2 kHz sine — high enough that the harmonics run past Nyquist, which is the only regime where any
of this matters — through three shapers. `vs ref` is deviation from a 16× rendering of the same
shaper, which is the fidelity companion: ADAA attenuates the *wanted* harmonics too, and a table
with only an alias column would reward that.

| shaper | method | alias | vs reference | ns/sample |
|---|---|---|---|---|
| `tanh`, drive 20 | trivial | −19.3 dB | −19.3 dB | 25.3 |
| | **first-order ADAA** | **−28.1 dB** | −25.5 dB | 44.0 |
| | 2× oversampling | −35.1 dB | −35.1 dB | 58.4 |
| | 4× oversampling | −65.1 dB | −65.1 dB | 106.0 |
| | 8× oversampling | −116.5 dB | −125.2 dB | 201.5 |
| sine folder, drive 8 | trivial | −8.4 dB | −6.0 dB | 20.5 |
| | first-order ADAA | −12.9 dB | −10.6 dB | 25.0 |
| | **2× oversampling** | **−99.2 dB** | −151.0 dB | 40.4 |
| | 4× oversampling | −99.2 dB | −177.2 dB | 85.4 |
| | 8× oversampling | −99.2 dB | −202.2 dB | 201.0 |
| Chebyshev 5th | trivial | −250.2 dB | −105.7 dB | 16.3 |
| | first-order ADAA | −243.8 dB | **−16.7 dB** | 22.0 |
| | 8× oversampling | −250.2 dB | −205.6 dB | 126.5 |

Four things, and two of them are surprises.

**Trivial waveshaping is catastrophic.** −19.3 dB for a driven saturator; **−8.4 dB for a driven
folder** — more aliased energy than a trivial modulo-counter sawtooth manages
([chapter 2](02-antialiasing.md#211-the-comparison-table): −19.1 dB at 440 Hz). A hard-driven folder
at 2 kHz is more alias than signal in places.

**Oversampling works here, unlike for classic waveforms.** Chapter 2 measured 6 dB per doubling for
a sawtooth; here `tanh` gets 15.8 dB from the first doubling, 30 dB from the second and 51 dB from
the third. The reason is the same one [chapter 12 §12.4](12-fm.md#124-oversampling-is-the-right-answer-here--and-only-here)
gives for FM: the spectrum being folded decays fast, so moving the fold point up the spectrum lands
it in territory that is already empty.

**The sine folder is FM in disguise, and that explains its 91 dB.** `sin(A·sin(ωt))` is phase
modulation — literally the equation from [chapter 12 §12.1](12-fm.md#121-what-the-knob-does). Its
sidebands are Bessel functions, which collapse super-exponentially past the modulation index, so a
single doubling of the sample rate moves the fold point past everything and the figure jumps from
−8.4 dB to −99.2 dB. It then stops improving, because there is nothing left to gain. Anyone who has
noticed that a sine folder and an FM operator sound related is hearing the same mathematics.

**ADAA buys 4–9 dB, not the 40 the marketing implies.** First-order antiderivative antialiasing
improves `tanh` by 8.8 dB and the folder by 4.5 dB, for about 1.2–1.7× the cost. That is a real
improvement at a real bargain — but it is *one order*. Compare 2× oversampling: more improvement,
more cost, and for the folder vastly more improvement.

## 17.3 How ADAA works, and why it is only worth one order

The trick is elegant enough to state in one line. Instead of evaluating `f` at the sample point,
evaluate the *average* of `f` over the interval the signal covered during that sample — and the
average of a function over an interval is a difference of its antiderivative:

```rust
let f = self.shaper.antiderivative(x);
let dx = x - self.prev_x;
let out = if dx.abs() > 1e-6 {
    (f - self.prev_f) / dx
} else {
    self.shaper.f(0.5 * (x + self.prev_x))
};
```

That is the whole method: one antiderivative evaluation, one subtract, one divide. It is a box
filter applied in the *signal* domain rather than the time domain, which is why it costs nothing
like an oversampler.

Two caveats the code makes visible.

**It needs a tolerance.** When the signal barely moves — at a peak, or on a held DC value — `dx → 0`
and the difference quotient is `0/0`. Every ADAA implementation needs the midpoint fallback above,
and the threshold is a tuning parameter nobody documents.

**It needs an antiderivative you can write down.** `tanh` → `ln(cosh)`. A sine folder →
another cosine. A polynomial → a polynomial. A lookup table or a conditional waveshaper → you have
to integrate it piecewise yourself, and each piece needs its boundaries handled. This is why ADAA
shows up in analytically-defined saturators and rarely in complicated folders.

## 17.4 ADAA is not free where there is nothing to fix

The Chebyshev row is the cautionary one, and it is the reason the fidelity companion column exists.

A fifth-order Chebyshev polynomial of a 2 kHz sine produces exactly five harmonics, the highest at
10 kHz — comfortably below Nyquist. **It cannot alias**, and the measurement agrees: −250.2 dB,
which is the floor.

Apply ADAA anyway and the alias figure does not improve — but the deviation from the reference goes
from −105.7 dB to **−16.7 dB**. ADAA has changed the sound substantially while fixing nothing,
because its box filter attenuates the wanted harmonics along with the imaginary ones.

Sweeping drive on the folder shows both regimes in one table:

| drive | trivial | with ADAA | ADAA gain |
|---|---|---|---|
| 1 | −218.7 dB | −225.4 dB | 6.7 dB |
| 2 | −139.3 dB | −146.0 dB | 6.6 dB |
| 4 | −66.1 dB | −72.2 dB | 6.1 dB |
| 8 | −8.4 dB | −12.9 dB | 4.5 dB |
| 16 | **+3.9 dB** | −8.2 dB | 12.1 dB |
| 32 | **+10.8 dB** | −6.5 dB | 17.3 dB |

At drive 16 and above the trivial folder produces **more aliased energy than signal** — the positive
figures — and ADAA claws back 12 to 17 dB. At drive 1 to 4 there is nothing to fix and the
"improvement" is measurement floor.

So the rule is: **ADAA where the nonlinearity is actually generating out-of-band content, and
nowhere else.** A saturator sitting at unity gain with a clean input is not helped by it and is
measurably dulled.

## 17.5 What to reach for

Putting §17.2 together, by shaper family:

| If the shaper is… | reach for | because |
|---|---|---|
| A polynomial of known order | **nothing** | It is band-limited by construction; check `n·f0 < fs/2` and stop |
| A saturator, moderate drive | **ADAA, first order** | 8.8 dB for 1.7× cost, and its antiderivative is a one-liner |
| A saturator, extreme drive | **oversampling, 4× or more** | ADAA's single order runs out; oversampling keeps scaling |
| A folder | **2× oversampling** | 91 dB from one doubling, because folding is phase modulation and its spectrum collapses past the index |
| A table or conditional shaper | **oversampling** | There is no antiderivative to write down |

And the general point, which is the one worth carrying out of this chapter: **the right antialiasing
technique depends on the shape of the spectrum you are folding, not on the technique's reputation.**
Chapter 2 measured oversampling as the worst deal available for a sawtooth. Here it is the best one
available for a folder. Same tool, opposite verdicts, because a sawtooth's spectrum decays at
6 dB/octave forever and a folder's collapses super-exponentially past a knee.

## 17.6 Consequences for this repository

mxm-mono-01 has no waveshaper. Its filter has saturators inside the feedback loop, and those are covered
by [`../filters/03-nonlinearity.md`](../filters/03-nonlinearity.md), which measured something
consistent with this chapter from the other end: a lowpass filter attenuates its own generated
harmonics, so a nonlinearity *inside* a filter aliases far less than the same nonlinearity in the
open.

For any future MXM instrument with a shaper, folder or distortion:

- **Check whether it can alias at all before defending against it.** A bounded-order polynomial
  cannot; the Chebyshev row cost nothing to measure and would have cost a chapter of wrong advice.
- **Budget ADAA at one order and 4–9 dB.** It is excellent value and it is not a solution to a
  hard-driven folder.
- **A folder wants 2× oversampling and nothing more** — measured 91 dB for the first doubling and
  0.0 dB for the next two.
- **Keep the antiderivative next to the shaper** in the code, so the two cannot drift apart. The
  `Shaper` enum in the spike pairs `f` and `antiderivative` on the same type for exactly that reason.

---

Back to [README.md](README.md) · sources: [08-sources.md](08-sources.md)
