# 1. Fundamentals

[← index](README.md) · [next: topologies →](02-topologies.md)

---

## 1.1 What a musical filter has to do

A mastering EQ and a synth VCF are barely the same object. The VCF has requirements an EQ never
faces:

| Requirement | Consequence for the design |
|---|---|
| Cutoff sweeps 20 Hz → 20 kHz, ten octaves | Coefficients recomputed constantly and cheaply; numerical conditioning must hold at both ends |
| Cutoff modulated at audio rate (filter FM) | Per-sample coefficient update; the structure must not blow up when `g` jumps |
| Resonance up to and past self-oscillation | The loop gain crosses 1; only a nonlinearity keeps output finite |
| Driven hard on purpose | Saturation is part of the sound, not a failure mode |
| Runs 8–16× per voice, polyphonic | ~50–200 ops/sample budget, no allocation, no branches that mispredict |
| Must *sound* right | Bass droop, resonance colour, drive interaction — none of which appear in a magnitude plot |

The last row is the hard one. Everything measurable can be matched by many structures; the sound
lives in the parts usually discarded as "non-ideal".

## 1.2 The analog prototype

Start with an RC lowpass. Kirchhoff gives

```
    dy/dt = ωc · (x − y)
```

which in the Laplace domain is `H(s) = 1 / (1 + s/ωc)`, and normalising `s` to units of `ωc`,

```
    H(s) = 1 / (1 + s)
```

That is the entire building block. **Every classic synth filter is integrators plus feedback.** A
one-pole is one integrator in a unit-gain loop; a state-variable filter is two integrators in a loop
with damping; a ladder is four integrators in a loop with a gain `k`.

Two quantities describe a pole pair:

- **Cutoff `ωc`** — where the pole sits, in radians/s. Musically this is a *pitch*, so it must be
  mapped exponentially; a linear cutoff control is unplayable.
- **Q (or damping `k = 1/Q`)** — how close the poles get to the imaginary axis. At `Q = 0.5` the
  poles are real and coincident (critically damped, no peak); `Q = 1/√2 ≈ 0.707` is maximally flat
  (Butterworth); `Q → ∞` puts the poles *on* the axis and the filter oscillates forever.

Zeros matter less in synth filters but they do appear: a notch is a pair of zeros on the axis, an
allpass mirrors the poles into the right half-plane as zeros, and the diode ladder has a zero nobody
asked for (§2.5).

### Cascades and slope

Cascading `n` identical one-poles gives `1/(1+s)ⁿ` — `6n` dB/octave in the stopband. It also gives a
*soft* corner: the −3 dB point of a 4-pole cascade sits well below the nominal `ωc`
(at `ωc·√(2^(1/4)−1) ≈ 0.435·ωc`), and the passband sags long before the corner. That soft knee is
part of why cascaded designs sound different from a Butterworth 4-pole of the same slope, and it is
why "24 dB/oct" tells you almost nothing about a filter's tone.

Wrapping a feedback gain `k` around that cascade turns the sag into a resonant peak. That single
move — cascade plus global feedback — is the Moog ladder, the SSM2040, the CEM3320, the Roland
IR3109 and a dozen others.

## 1.3 Discretisation: the choice that determines everything

You have a differential equation and need a difference equation. The integrator `y = ∫x dt` can be
approximated three ways:

| Method | Update | Behaviour |
|---|---|---|
| Forward (explicit) Euler | `y[n] = y[n−1] + T·x[n−1]` | Maps part of the stable half-plane *outside* the unit circle near Nyquist. High-Q filters go unstable. Cheap and wrong. |
| Backward (implicit) Euler | `y[n] = y[n−1] + T·x[n]` | Unconditionally stable but over-damped: resonance smears, Q is lost at high cutoff. |
| Trapezoidal | `y[n] = y[n−1] + (T/2)(x[n] + x[n−1])` | Maps the imaginary axis onto the unit circle exactly. This *is* the bilinear transform. |

Trapezoidal integration is the right default and the rest of this document assumes it. Its one cost
is that it is **implicit**: `y[n]` depends on `x[n]`, and when the integrator sits inside a feedback
loop, `x[n]` depends on `y[n]`. That is the delay-free loop, and §1.6 resolves it.

### Bilinear transform and frequency warping

Substituting `s ← (2/T)·(1−z⁻¹)/(1+z⁻¹)` maps the entire `s`-plane imaginary axis onto the unit
circle — infinite analog frequency onto Nyquist. The compression is nonlinear:

```
    ω_digital = (2/T) · tan(ω_analog · T / 2)
```

Read backwards: to place a digital pole at frequency `fc`, use the analog design value

```
    g = tan(π · fc / fs)
```

This `g` is the single most important number in VA filter design. It is the per-sample integrator
gain, it goes to infinity at Nyquist (correct — the pole must reach the top of the unit circle), and
any structure using `2π·fc/fs` or `2·sin(π·fc/fs)` instead is an approximation to it that fails at
high cutoff.

Concretely, at `fs = 48 kHz`, `fc = 8 kHz`: `2π·fc/fs = 1.047` but `tan(π·fc/fs) = 0.577`. The
un-prewarped version puts the cutoff nearly an octave off. At `fc = 12 kHz` it is off by a fifth.
This is a *tuning* error, and on a self-oscillating filter it is immediately audible against an
oscillator.

**Practical bound.** `tan` blows up at Nyquist. Clamp `fc` to ~0.45·fs (this repo uses
`NYQUIST_FRACTION = 0.45`) and, if using a rational approximation to `tan`, make sure the
approximant's pole sits at `π/2` exactly — [04-efficiency.md](04-efficiency.md) §4.4.

### Impulse invariance, and why not to use it here

Impulse invariance samples the analog impulse response directly. It preserves time-domain shape and
does *not* warp frequency — but it aliases the analog response around Nyquist, which for a lowpass
with a long stopband tail is tolerable and for a resonant filter is not. It also offers no clean
place to insert nonlinearities. Ignore it for VCF work; it earns its keep in reverb and physical
modelling.

## 1.4 Direct forms, and why synth filters avoid them

The textbook route: design the analog filter, bilinear-transform the whole transfer function,
implement the resulting biquad in Direct Form I or Transposed Direct Form II with RBJ cookbook
coefficients. For an EQ band this is exactly right. For a VCF it is a trap.

- **Time-varying coefficients.** Direct-form states hold delayed *inputs and outputs*, not physical
  quantities. Change coefficients between samples and the stored history belongs to a different
  filter; the output jumps. Transposed DF-II is the worst offender under fast modulation — its state
  is a partial sum computed with the *old* coefficients. DF-I is the most forgiving because its
  state is raw signal history. TPT (§1.6) beats all of them because its state is a capacitor
  voltage, physically continuous across a coefficient change.
- **No place for nonlinearity.** In the real circuit, saturation happens at specific nodes. In a
  direct form those nodes have been algebraically eliminated. You can only clip the input or the
  output, both of which sound wrong (§3.2).
- **Coefficient conditioning.** At `fc/fs = 0.001` in `f32`, a direct-form biquad's `a1` approaches
  `−2` and `a2` approaches `1`; the pole radius lives in the last few bits. Cutoff resolution goes
  coarse and the filter can stick or go unstable. State-variable and TPT structures encode cutoff in
  `g` itself and stay well conditioned down to DC.
- **Coefficient cost.** Recomputing a biquad from `fc`/`Q` needs `sin`, `cos` and a division per
  update. TPT needs one `tan` and one division, and the SVF form shares that division across all
  outputs.

Direct forms remain the right answer for fixed or slowly-changing EQ inside a plugin — output
shelving, DC blockers, oversampling filters. Just not for the VCF.

## 1.5 The Chamberlin SVF: the useful wrong answer

The classic digital state variable filter:

```
    hp = x − lp − q1·bp
    bp = f·hp + bp
    lp = f·bp + lp
```

with `f = 2·sin(π·fc/fs)` and `q1 = 1/Q`. It is beautiful: three multiplies, all four responses at
once, states that mean something physically.

It is also **forward-Euler in a loop with a one-sample delay**, and it has two hard limits:

- Stability requires roughly `fc < fs/6`, and less as Q rises. Above that it blows up.
- `f = 2·sin(...)` is not `tan(...)`, so cutoff is mistuned, increasingly so with frequency, and the
  resonant peak drifts away from the nominal cutoff.

The fix is the same fix as everywhere else: trapezoidal integration and an algebraically resolved
loop. The result (§2.2) costs barely more and has none of the limits. Treat the Chamberlin SVF as a
historical stepping stone — or as a deliberate lo-fi flavour, since its high-cutoff misbehaviour is
itself a recognisable early-digital sound.

## 1.6 TPT / ZDF: the modern default

**Topology-Preserving Transform** (Zavalishin) means: discretise the *integrators* with the bilinear
transform, keep the block diagram of the analog circuit intact, and solve the resulting instantaneous
(delay-free) loop algebraically. "Zero-delay feedback" names the same idea after its most visible
consequence. The name is a slight lie — you cannot remove all delay from a recursive system — but
the delays end up on integrator *states*, where they belong, instead of across a feedback path where
they detune the loop.

### The trapezoidal integrator

Write the integrator as gain `g` plus state `s`:

```
    y = g·x + s
    s ← y + g·x          (equivalently  s ← s + 2·g·x)
```

`s` is exactly the capacitor voltage at the end of the previous sample. This is why TPT survives
coefficient modulation: changing `g` does not invalidate `s`.

### Resolving the loop: the one-pole

The analog one-pole feeds `(x − y)` into the integrator:

```
    ┌────────────────────────────────┐
    │            ┌───┐    ┌─────┐    │
 x ─┴─►(+)──────►│ g │───►│  ∫  │────┴──► y
        ▲ −      └───┘    └─────┘
        └────────────────────────────┘
```

Substituting `y = g·(x − y) + s` and solving:

```
    v = G·(x − s),   where G = g/(1+g)
    y = v + s
    s ← y + v
```

Three multiplies, three adds, one state. `G` is computed once per coefficient update. Outputs:

```
    lowpass  = y
    highpass = x − y
    allpass  = 2y − x          (lowpass − highpass)
```

Rust in [02-topologies.md](02-topologies.md) §2.1.

### Why this is worth the algebra

- **Tuning is exact** at every cutoff up to the clamp, because `g` is a tangent and there is no
  extra delay in the loop.
- **Resonance stays where you put it.** With a delayed feedback path both the peak frequency and the
  self-oscillation threshold become functions of `fc/fs`. This repo measured exactly that: `k` at
  threshold drifting from 2.66 to 3.96 across the cutoff range with a delayed loop, versus 3.97–4.00
  with the loop solved (`crates/mxm-mono-01-dsp/src/filter.rs`).
- **Modulation is clean.** State is physical, so per-sample `g` changes do not click.
- **There is a place to put the nonlinearity** — at the node where the circuit actually saturates.
  This is the big one, and [03-nonlinearity.md](03-nonlinearity.md) is entirely about it.

### The general recipe

For any single-loop analog topology:

1. Replace each capacitor with `y = g·x + s`.
2. Write the loop equations, treating every signal as instantaneous.
3. Solve the linear system for the loop variable. One-loop structures need one division; the SVF
   needs a 2×2 solve that still collapses to one reciprocal.
4. Update all states.
5. If a nonlinearity sits in the loop, step 3 is no longer linear — see §3.3.

## 1.7 Time-varying stability

A filter stable for every *fixed* parameter setting can still explode when parameters move. The
mechanism is energy: a coefficient change that rescales a state without rescaling the energy it
represents injects energy, and a resonant loop amplifies it.

Three practical rules:

- **Prefer structures whose state is a physical quantity.** TPT integrator states are capacitor
  voltages; scaling `g` does not change them. Direct-form states are not, and modulation pumps them.
- **Bound the parameter, not just the output.** Clamp `fc` to `[20 Hz, 0.45·fs]` and `k` to a
  measured maximum, and do it inside the DSP where the invariant is needed — not only in the
  parameter layer, which a host automation curve or a modulation sum can drive past.
- **Smooth signals, not timings.** Cutoff and resonance are signals and should be smoothed. Envelope
  times and glide time are coefficients of a state machine and should not — smoothing them makes the
  timing unanalysable. Same rule as [`plugins/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/AGENTS.md).

There is a formal version of this (passivity / energy analysis of time-varying digital filters), but
the practical test is a unit test that modulates cutoff with a full-range square wave at audio rate
for a few seconds and asserts the output stays finite and bounded. [06-testing.md](06-testing.md)
§6.6.

## 1.8 Vocabulary, disambiguated

| Term | Meaning here |
|---|---|
| `g` | Prewarped integrator gain, `tan(π·fc/fs)`. Elsewhere written `wd`, `T`, `F`. |
| `G` | Resolved one-pole feed-forward gain, `g/(1+g)`. |
| `k` | Feedback / damping amount. **Two incompatible conventions.** In an SVF, `k = 1/Q` — damping, higher means *less* resonant. In a ladder, `k` is resonance feedback gain — higher means *more* resonant, oscillating at `k = 4`. Always state which. |
| `s` | Integrator state (in code). Also the Laplace variable (in maths). Context disambiguates. |
| ZDF | Zero-delay feedback — the loop is solved, not delayed. |
| TPT | Topology-preserving transform — ZDF plus keeping the analog block diagram. |
| VA | Virtual analog — modelling analog behaviour digitally, by any method. |
| Pole mixing | Building many responses from linear combinations of one ladder's taps (§2.6). |

---

[← index](README.md) · [next: topologies →](02-topologies.md)
