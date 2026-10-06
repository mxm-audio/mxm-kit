# 2. Topologies

[← fundamentals](01-fundamentals.md) · [index](README.md) · [next: nonlinearity →](03-nonlinearity.md)

All code here is linear. Nonlinearity is [chapter 3](03-nonlinearity.md) — but note as you read
where each structure *offers a node* to put it, because that is the real difference between them.

---

## 2.0 Shared helpers

```rust
use std::f32::consts::PI;

/// Flush denormals. Required on every recursive state — see 04-efficiency.md §4.6.
#[inline(always)]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}

/// Prewarped integrator gain. `tan` is expensive; 04-efficiency.md §4.4 replaces it.
#[inline]
pub fn prewarp(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let ratio = (cutoff_hz / sample_rate).clamp(1.0e-5, 0.45);
    (PI * ratio).tan()
}
```

`0.45` is the Nyquist clamp from [01](01-fundamentals.md) §1.3. `1e-5` keeps `g` away from exact
zero so `1/(1+g)` stays well conditioned and a 0 Hz cutoff request degrades to "very closed" rather
than "division by something tiny".

---

## 2.1 The one-pole (TPT)

The atom. Six flops, one state, exact tuning at any cutoff.

```rust
/// One-pole TPT lowpass. `g` is stored pre-resolved as G = g/(1+g).
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    big_g: f32,
    s: f32,
}

impl OnePole {
    pub fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        let g = prewarp(cutoff_hz, sample_rate);
        self.big_g = g / (1.0 + g);
    }

    /// Set directly from a prewarped `g` — for when several stages share one `tan`.
    pub fn set_g(&mut self, g: f32) {
        self.big_g = g / (1.0 + g);
    }

    #[inline]
    pub fn lowpass(&mut self, x: f32) -> f32 {
        let v = self.big_g * (x - self.s);
        let y = v + self.s;
        self.s = flush(y + v);
        y
    }

    #[inline]
    pub fn highpass(&mut self, x: f32) -> f32 {
        x - self.lowpass(x)
    }

    /// First-order allpass — the phaser building block (§2.8).
    #[inline]
    pub fn allpass(&mut self, x: f32) -> f32 {
        2.0 * self.lowpass(x) - x
    }

    pub fn reset(&mut self) {
        self.s = 0.0;
    }
}
```

**Uses.** DC blocker (highpass at 5–20 Hz), the non-resonant HPF in front of a Roland voice, tilt
and shelf shaping, the smoothing element on a control signal, and the four rungs of a ladder.

**Shelving for free.** `x + (A−1)·lowpass(x)` is a low shelf of gain `A`; the highpass version is a
high shelf. Cheaper and better behaved under modulation than a shelving biquad.

---

## 2.2 The 2-pole TPT state-variable filter

The workhorse. Two integrators, one 2×2 solve collapsed into three precomputed coefficients, and
*every* second-order response available simultaneously from the same three internal signals. This is
Andy Simper's linear trapezoidal SVF, which is the same filter Zavalishin derives as the TPT SVF.

```rust
#[derive(Debug, Clone, Copy)]
pub struct SvfOut {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// `q` is the usual Q: 0.5 = critically damped, 0.707 = Butterworth, large = resonant.
    pub fn set(&mut self, cutoff_hz: f32, q: f32, sample_rate: f32) {
        let g = prewarp(cutoff_hz, sample_rate);
        let k = 1.0 / q.max(0.025);
        self.k = k;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    #[inline]
    pub fn process(&mut self, v0: f32) -> SvfOut {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = flush(2.0 * v1 - self.ic1);
        self.ic2 = flush(2.0 * v2 - self.ic2);
        SvfOut { lp: v2, bp: v1, hp: v0 - self.k * v1 - v2 }
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }
}
```

Per sample: 5 multiplies, 7 adds, no division, no branch. The division lives in `set`.

### The output mixer

Everything second-order is `m0·v0 + m1·v1 + m2·v2`. Store `(m0, m1, m2)` and the filter mode costs
nothing at runtime — no branch in the sample loop, and you can *interpolate* between modes, which is
a genuinely useful synth control ("morph" from LP through BP to HP).

| Mode | `m0` | `m1` | `m2` |
|---|---|---|---|
| Lowpass | 0 | 0 | 1 |
| Bandpass | 0 | 1 | 0 |
| Bandpass (unity peak gain) | 0 | `k` | 0 |
| Highpass | 1 | `−k` | −1 |
| Notch | 1 | `−k` | 0 |
| Peak | 1 | `−k` | −2 |
| Allpass | 1 | `−2k` | 0 |
| Bell, gain `A` | 1 | `k·(A²−1)` | 0 |
| Low shelf, gain `A` | 1 | `k·(A−1)` | `A²−1` |
| High shelf, gain `A` | `A²` | `k·(1−A)·A` | `1−A²` |

with `A = 10^(dB/40)`. For the bell use `k = 1/(Q·A)`; for the shelves scale `g` by `1/√A` (low) or
`√A` (high) before computing `a1..a3`. Source: Simper, *Linear Trapezoidal Integrated State Variable
Filter With Low Noise Optimisation* — verify against the paper before shipping shelves, the LP/BP/HP
rows are the ones this document has checked.

### Why the SVF is the default for anything that is not a ladder

- All responses at once, so multimode costs three multiplies.
- Audio-rate cutoff modulation is fine: no delay in the loop, state is physical.
- Numerically excellent from DC to the clamp — Simper's "low noise optimisation" is precisely about
  keeping `ic1`/`ic2` well scaled.
- One reciprocal per coefficient update, shared by all outputs.

### Higher orders

Cascade SVFs with per-section Q from the standard tables: a 4-pole Butterworth is two sections with
`Q = 0.5412` and `Q = 1.3065` at the same cutoff. But **do not** build a synth's resonant 4-pole
this way — see §2.3. Cascaded-Butterworth resonance sounds like an EQ, not a VCF.

---

## 2.3 The transistor ladder

Four one-poles in series, global feedback `k` from the last stage to the first, with the feedback
**subtracted**:

```
        ┌──────────────── × k ◄─────────────────────────────┐
        ▼ −                                                 │
 x ───►(+)──► LP ──► LP ──► LP ──► LP ──┬──────────────────►│ y
              y1     y2     y3     y4   └──► output
```

Idealised transfer function (all four stages identical, buffered):

```
    H(s) = 1 / ((1 + s)⁴ + k)
```

Everything musical about the ladder is in that equation:

- **Poles.** At `k = 0` all four sit at `(−1, 0)`. As `k` rises they migrate outward along an "X"
  at 45°, centred on `(−1, 0)`. At **`k = 4`** the rightmost pair reaches the imaginary axis and the
  filter self-oscillates. This is why "resonance = 4" is the magic number in every ladder
  implementation, and why a correct implementation must *measure* its threshold at 4.00 (this repo
  does: `crates/mxm-mono-01-dsp/src/filter.rs`).
- **Bass droop.** `H(0) = 1/(1+k)`. At `k = 4` that is `−14 dB`. The passband sags because the
  poles have moved far apart and the response "droops" between them. Players know it as the ladder
  losing its bottom end as resonance comes up, and it is a *diagnostic*: open the cutoff right up,
  play a low tone, raise resonance — if the level drops, it is a ladder-type filter.
- **The same equation is almost universal.** Stinchcombe's analysis notes that four cascaded
  buffered one-poles with global feedback also describes filters built on the CEM3320, SSM2040,
  SSM2044, and any four-OTA design — which includes Roland's IR3109 (SH-101, Juno-6/60/106,
  Jupiter-8) and the Prophet-5. **Linearly, a Minimoog and a Juno-106 are the same filter.** They do
  not sound the same. The difference is entirely in chapter 3.

### Linear ZDF solve

With `G = g/(1+g)` and each stage `yᵢ = G·xᵢ + (1−G)·sᵢ`, write `Sᵢ = (1−G)·sᵢ` and unroll:

```
    y4 = G⁴·u + G³·S₁ + G²·S₂ + G·S₃ + S₄        where u = x − k·y4
```

so

```
    y4 = (G⁴·x + Sfb) / (1 + k·G⁴),     Sfb = G³S₁ + G²S₂ + G·S₃ + S₄
```

One division per sample (or precompute `1/(1+k·G⁴)` in `set`). Then recover `u` and run the four
stages forward normally.

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct Ladder {
    big_g: f32,     // G = g/(1+g)
    k: f32,         // resonance feedback, 0..=4 (and a little past)
    inv_den: f32,   // 1 / (1 + k·G⁴)
    g4: f32,        // G⁴
    s: [f32; 4],
}

impl Ladder {
    /// `k` is ladder feedback: 0 = no resonance, 4 = self-oscillation threshold.
    pub fn set(&mut self, cutoff_hz: f32, k: f32, sample_rate: f32) {
        let g = prewarp(cutoff_hz, sample_rate);
        self.big_g = g / (1.0 + g);
        self.k = k;
        let g2 = self.big_g * self.big_g;
        self.g4 = g2 * g2;
        self.inv_den = 1.0 / (1.0 + k * self.g4);
    }

    /// Returns the five ladder taps: `[u, y1, y2, y3, y4]`, where `u` is the signal
    /// entering the first pole (input minus feedback). Feed them to `mix` (§2.6).
    #[inline]
    pub fn process(&mut self, x: f32) -> [f32; 5] {
        let g = self.big_g;
        let one_minus = 1.0 - g;

        // Contribution of the stored states to y4, with no input.
        let big_s = [
            one_minus * self.s[0],
            one_minus * self.s[1],
            one_minus * self.s[2],
            one_minus * self.s[3],
        ];
        let g2 = g * g;
        let g3 = g2 * g;
        let s_fb = g3 * big_s[0] + g2 * big_s[1] + g * big_s[2] + big_s[3];

        // Resolve the delay-free loop.
        let y4 = (self.g4 * x + s_fb) * self.inv_den;
        let u = x - self.k * y4;

        // Run the stages forward, updating state.
        let mut taps = [u, 0.0, 0.0, 0.0, 0.0];
        let mut v = u;
        for i in 0..4 {
            let d = g * (v - self.s[i]);
            v = d + self.s[i];
            self.s[i] = flush(v + d);
            taps[i + 1] = v;
        }
        taps
    }

    pub fn reset(&mut self) {
        self.s = [0.0; 4];
    }
}
```

`taps[4]` should equal the `y4` solved above to within rounding — a good invariant to assert in a
debug test.

### The gain-compensation question

Because `H(0) = 1/(1+k)`, many designs add `(1 + c·k)` makeup gain at the input. **Think before
doing this.** Compensating fully removes the droop and with it the ladder's identity. What real
hardware did:

- Moog: nothing. The bass goes.
- Some Moog derivatives and modern clones: a dual-gang pot that raises input level with resonance.
- Practical middle ground: compensate *partially*, e.g. `1 + 0.5·k`, and expose it. Or compensate
  fully and put the character back with input drive, which is more musical because it saturates.

---

## 2.4 Sallen–Key — and why "linear equivalence" is a trap

The equal-component Sallen–Key lowpass with buffer gain `K`:

```
    H(s) = K / (s² + (3 − K)·s + 1)
```

so `Q = 1/(3 − K)` and it self-oscillates at `K = 3`. Compare the SVF lowpass:
`H(s) = 1/(s² + k·s + 1)` with `k = 1/Q`. **They are the same filter**, with `k = 3 − K`.

This generalises. Any two structures of the same order with the same poles and zeros have the same
linear response, by definition. So:

> Topology choice never changes the linear response. It changes **where the nonlinearity can live**,
> **what the states mean**, and **how the thing behaves when parameters move**.

That is not a disappointment, it is the whole point. The Korg MS-20's Sallen–Key is famous for
screaming, and it screams because in the Rev 1 circuit a pair of diodes clips *inside* the resonance
feedback path, asymmetrically. Run the same poles through an SVF with a clean feedback path and you
get a polite 2-pole. Run them through an SVF with a saturator on `v1` and you are back in business.

Practical consequence: **implement Sallen–Key-flavoured filters as an SVF plus a nonlinearity on the
damping path.** You get better numerics, all the outputs, and the character. Reserve a
circuit-faithful SK model for when you are matching a specific unit and have its schematic.

---

## 2.5 The diode ladder

Replace the ladder's transistors with diodes and the stages stop being buffered. Each rung now loads
the one above it, so the poles interact. Consequences, from Stinchcombe's analysis:

| | Transistor ladder | Diode ladder (1 diode, e.g. TB-303) |
|---|---|---|
| Poles at `k = 0` | all four at `−1` | real and spread: `−3.532, −2.347, −1.000, −0.121` |
| Self-oscillation | `k = 4` | `k ≈ 18.4` (`≈ 9.8` for the 3-diode EMS type) |
| Corner shape | defined knee | much gentler — the origin of the "18 dB/oct" myth |
| Stopband slope | 24 dB/oct | 24 dB/oct (it *is* four poles) |
| Numerator | constant | has a zero |

The near-origin pole at `−0.121` is the interesting one: it means the response is already falling
well below the nominal cutoff, which is exactly the "soft, wooly, then suddenly squelchy" character.
And the far higher feedback needed to oscillate means the signal levels inside the ladder are much
larger before it sings — which is why diode ladders break into distortion *before* they oscillate,
where a transistor ladder oscillates cleanly. That single fact is most of the difference between a
Minimoog bassline and an acid line.

### Practical model

Solving the fully coupled 4×4 delay-free system per sample is possible (it is tridiagonal, so a
Thomas-algorithm sweep) but it is not where the value is. A structural model that captures the
behaviour: give each stage a back-coupling term from the stage below, taken from that stage's
**integrator state** rather than its instantaneous output. The state is the previous sample's
end-of-sample capacitor voltage, so the approximation error is `O(g)` per stage — while the *global*
resonance loop, which controls tuning and the oscillation threshold, is still solved exactly.

```rust
/// Structural diode-ladder model: a transistor ladder plus inter-stage loading.
/// `coupling` ≈ 0.4–0.6 gives the characteristic spread-real-pole behaviour;
/// derive it from the circuit if you are matching a specific unit.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiodeLadder {
    inner: Ladder,
    coupling: f32,
    back: [f32; 3],
}

impl DiodeLadder {
    pub fn set(&mut self, cutoff_hz: f32, k: f32, coupling: f32, sample_rate: f32) {
        // Diode ladders need far more feedback to sing; scale the user's 0..4 range.
        self.inner.set(cutoff_hz, k * 4.6, sample_rate);
        self.coupling = coupling;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        // Inject last sample's downstream voltages as loading on the stages above.
        let load = self.coupling * (self.back[0] + self.back[1] + self.back[2]) / 3.0;
        let taps = self.inner.process(x + load);
        self.back = [taps[2], taps[3], taps[4]];
        taps[4]
    }
}
```

This is deliberately a *model of the behaviour*, not of the schematic, and **there is now a better
one**: `research:filters/machines/tb303-diode-ladder.md` builds the filter directly
from its published **pole positions** (`−3.532, −2.347, −1.000, −0.121` for the TB-303
configuration), which reproduces the oscillation threshold and the offset oscillation frequency with
no free parameters to tune. Prefer that. The sketch above is kept only as the simplest thing that
shows the idea.

---

## 2.6 Pole mixing: 15 filter modes from one ladder

The Oberheim Xpander and Matrix-12 got 15 filter responses out of a single 4-pole VCF by tapping
each rung and summing them with signed coefficients. It is the highest ratio of musical result to
DSP cost in this entire document, and almost nothing in software does it.

**Why it works.** With `L = 1/(1+s)` the tap after `n` poles is `Lⁿ`. Since `s/(1+s) = 1 − L`, any
polynomial in `L` of degree ≤ 4 is reachable as a weighted sum of the taps `[1, L, L², L³, L⁴]`, and
that set includes every lowpass, highpass, bandpass, notch and allpass of order ≤ 4:

```
    HP₁ = 1 − L
    HP₂ = (1 − L)²  = 1 − 2L + L²
    BP₂ = L − L²                      (= L·(1−L))
    BP₄ = L² − 2L³ + L⁴               (= L²·(1−L)²)
    AP₁ = 2L − 1
    AP₃ = −1 + 6L − 12L² + 8L³        (from (2−w)³/w³ with w = 1+s)
```

The resonance feedback is untouched, so **the resonant peak stays at cutoff in every mode** — which
is why the Xpander's highpass and notch modes are resonant and musical rather than the flabby things
you get from a cascade.

```rust
/// Xpander/Matrix-12 style tap weights over `[u, y1, y2, y3, y4]`.
pub type PoleMix = [f32; 5];

pub const LP1: PoleMix = [0.0, 1.0, 0.0, 0.0, 0.0];
pub const LP2: PoleMix = [0.0, 0.0, 1.0, 0.0, 0.0];
pub const LP3: PoleMix = [0.0, 0.0, 0.0, 1.0, 0.0];
pub const LP4: PoleMix = [0.0, 0.0, 0.0, 0.0, 1.0];
pub const HP1: PoleMix = [1.0, -1.0, 0.0, 0.0, 0.0];
pub const HP2: PoleMix = [1.0, -2.0, 1.0, 0.0, 0.0];
pub const HP3: PoleMix = [1.0, -3.0, 3.0, -1.0, 0.0];
pub const BP2: PoleMix = [0.0, 1.0, -1.0, 0.0, 0.0];
pub const BP4: PoleMix = [0.0, 0.0, 1.0, -2.0, 1.0];
pub const NOTCH2: PoleMix = [1.0, -2.0, 2.0, 0.0, 0.0];
pub const AP3: PoleMix = [-1.0, 6.0, -12.0, 8.0, 0.0];
// The four "+ 1-pole lowpass" modes are the same polynomials shifted one tap right.
pub const HP2_LP1: PoleMix = [0.0, 1.0, -2.0, 1.0, 0.0];
pub const HP3_LP1: PoleMix = [0.0, 1.0, -3.0, 3.0, -1.0];
pub const NOTCH2_LP1: PoleMix = [0.0, 1.0, -2.0, 2.0, 0.0];
pub const AP3_LP1: PoleMix = [0.0, -1.0, 6.0, -12.0, 8.0];

#[inline]
pub fn mix(taps: &[f32; 5], m: &PoleMix) -> f32 {
    taps[0] * m[0] + taps[1] * m[1] + taps[2] * m[2] + taps[3] * m[3] + taps[4] * m[4]
}
```

Five multiply-adds. Two caveats worth knowing:

- **Level.** The high-order alternating-sign mixes (`AP3` especially) have large coefficients and
  can produce a hot output from a modest input. Normalise per mode, or measure and store a gain.
- **Nonlinearity interacts.** With saturators inside the stages, the taps are no longer exactly
  `Lⁿ`, so the cancellations in `HP3` and `AP3` are imperfect and those modes leak some lowpass. In
  practice this sounds *good* — it is what the analog original did too — but do not expect a
  textbook null.

Interpolating between two mix vectors gives a continuously morphing filter, which is a control the
hardware could not offer.

---

## 2.7 Series and parallel arrangements

Not every filter section is a resonant VCF, and the arrangement is part of the instrument's voice.

- **HPF → LPF in series.** The Roland house sound. The Junos and Jupiters put a *non-resonant*
  highpass ahead of the 24 dB lowpass, so you thin the source before it hits the resonant stage —
  quite different from a resonant HP, and much better behaved with self-oscillation. One `OnePole`
  in highpass mode, or a 2-pole non-resonant version, is the whole implementation. The Juno-106's
  version has a twist: its lowest position is a low-shelf *boost*, not a bypass, and that is much of
  why a 106 sounds bigger than a 60. (The **SH-101 has no highpass at all** — a common
  misattribution. See `research:filters/machines/ir3109-roland.md` §5.)
- **HPF and LPF both 2-pole, both resonant.** Yamaha CS-80: two independent 12 dB sections, giving
  a bandpass with independently controllable skirts, which is why CS-80 pads breathe the way they
  do.
- **Parallel bands.** Sum several bandpasses at fixed ratios for a formant/vocal filter (§2.8), or
  two lowpasses slightly detuned in cutoff for a phasey, wide sound.
- **Stereo split.** Two instances with a small cutoff offset (±1–3%) is the cheapest convincing
  "analog width" you can buy. Also the cheapest way to hide a mono filter's dullness.

---

## 2.8 Beyond poles-and-zeros: the other filters in a synth

### Comb and Karplus–Strong

A delay line plus feedback. `y[n] = x[n] + a·y[n − N]` gives peaks every `fs/N` Hz. Fractional delay
(one-sample allpass or Lagrange interpolation) is required for tuning. Put a one-pole lowpass in the
loop and you have Karplus–Strong; put a *filter* in a comb loop and you have the basis of physical
modelling. Cheap, and utterly unlike a resonant lowpass in character.

### Allpass chains — the phaser

A cascade of first-order allpasses, each `(1−s)/(1+s)`, whose cutoffs sweep together, summed with
the dry signal. `N` allpasses give `N/2` notches. The `OnePole::allpass` method above is the entire
building block. A phaser is a filter that only exists as an *interference* pattern — no energy is
removed by the allpasses themselves — which is why it sounds like motion rather than tone shaping.

### Formant / vocal filters

Three to five bandpasses in parallel at vowel formant frequencies, with per-band gains and Qs.
Interpolating the *frequencies* between vowels sounds better than crossfading the outputs. An SVF
per band, `m = (0, k, 0)` for unity peak gain, and you can sweep a vowel space with two macro
controls.

### Lowpass gates

Buchla's 291/292: a lowpass whose cutoff *and* amplitude are controlled together by the resistance
of a vactrol (an LED shining on a photoresistor). The character comes entirely from the vactrol's
asymmetric, sluggish response — it tracks a rising control fast and a falling one slowly, with a
multi-second tail. Percussive material fed through it acquires the "struck object" quality that
makes West Coast synthesis sound like it does.

Modelling it is not a filter problem, it is an envelope problem:

```rust
/// Vactrol-ish control-signal shaper: fast attack, slow and level-dependent release.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vactrol {
    level: f32,
}

impl Vactrol {
    #[inline]
    pub fn process(&mut self, drive: f32, sample_rate: f32) -> f32 {
        // VTL5C3/2 datasheet: about 12 ms rising, 250 ms falling, and *faster*
        // when already open. Getting either of those backwards changes the
        // instrument — see machines/buchla-lowpass-gate.md §2.
        let base = if drive > self.level { 0.012 } else { 0.25 };
        let tau = base * (1.0 - 0.7 * self.level).max(0.05);
        let coeff = 1.0 - (-1.0 / (tau * sample_rate)).exp();
        self.level = flush(self.level + coeff * (drive - self.level));
        self.level
    }
}
```

Drive a `OnePole`'s cutoff *and* a VCA gain from the same `level` and the gate behaviour appears.
Measured, that coupling keeps **9 %** of a note's brightness by −20 dB where a VCA keeps 96 % —
`research:filters/machines/buchla-lowpass-gate.md` §3, which also covers the three
panel modes and why prewarping is impractical here.

---

## 2.9 Choosing

| You want | Use |
|---|---|
| Multimode, morphing, EQ, formants, anything not "the classic 24 dB sound" | TPT SVF (§2.2) |
| The Moog / Roland / Prophet / ARP 4-pole voice | Ladder (§2.3) + nonlinearity in the feedback |
| Screaming 2-pole with aggressive resonance | SVF poles + saturator on the damping path (§2.4) |
| Acid, squelch, dirt before oscillation | Diode ladder (§2.5) |
| 15 modes for the price of 5 multiply-adds | Ladder + pole mixing (§2.6) |
| Non-resonant tone shaping, DC block, shelves | `OnePole` (§2.1) |
| Motion, width, plucks, vowels, struck objects | §2.8 |

---

[← fundamentals](01-fundamentals.md) · [index](README.md) · [next: nonlinearity →](03-nonlinearity.md)
