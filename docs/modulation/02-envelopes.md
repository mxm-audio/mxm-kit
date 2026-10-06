# 2. Envelopes

An envelope is a curve and a state machine. Almost every audible difference between one instrument's
envelopes and another's comes from the curve; almost every *bug* comes from the state machine.

Figures from `crates/dsp-lab/examples/mod_spike.rs` §3.

---

## 2.1 Three curves

**Linear.** Level moves by a constant amount per sample. Simple, and wrong for amplitude: a linear
fade sounds like it hangs at the top and then drops away, because loudness is roughly logarithmic.
Its one virtue is that it reaches its target in a known, exact number of samples.

**Exponential / RC.** Level moves a constant *fraction* of the remaining distance per sample:

```rust
self.level = target + (self.level - target) * coef;
```

This is a one-pole lowpass, which is what a resistor charging a capacitor does — hence "RC". It is
what every analog envelope generator produces, and it is what the shipped `Adsr` uses. Its
characteristic is that it approaches the target asymptotically and never arrives, which is a problem
the next section is about.

**Curve-shaped.** A power law or lookup applied on top, so the designer can dial between linear and
exponential. This is what a "vintage Digital / RC" switch such as TAL-Sampler's
([`../oscillators/14-samplers.md §14.1`](../oscillators/14-samplers.md#141-what-the-plugin-says-it-does))
selects: *Digital* is the linear or near-linear curve early digital samplers used because it was
cheap, *RC* is the analog one. The two sound distinctly different on percussive material and nearly
identical on pads, which is why the switch exists rather than a knob.

## 2.2 The asymptote problem, and the overshoot fix

An exponential attack never reaches 1.0. Left alone, the envelope stays in its attack stage forever
and never advances to decay.

The standard fix is to aim past the target and stop early, which is what the shipped code does:

```rust
const ATTACK_OVERSHOOT: f32 = 0.2;
const ATTACK_TAUS: f32 = 1.791_759_5; // ln(1.2 / 0.2)
...
Stage::Attack => {
    let target = 1.0 + ATTACK_OVERSHOOT;
    self.level = target + (self.level - target) * self.attack_coef;
    if self.level >= 1.0 {
        self.level = 1.0;
        self.stage = Stage::Decay;
    }
}
```

Aim at 1.2, stop at 1.0. The `ATTACK_TAUS` constant is `ln(1.2 / 0.2)` — the number of time
constants needed to travel from 0 to 1.0 when heading for 1.2 — so that the *requested* attack time
is the time actually taken to reach full level, not the time constant of a curve that never gets
there. Without that correction an "attack time" parameter means something like a third of what it
says.

Decay and release use `DECAY_TAUS = ln(100)`, i.e. they are considered finished when they are within
1% of the target, with `ZERO_THRESHOLD = 1e-4` (−80 dB) as the stage-exit test.

**This is the detail to copy.** A great many envelope implementations either never leave the attack
stage, or leave it at an arbitrary threshold that makes the time parameter a lie. Which convention
you pick matters less than making the parameter mean what it says.

## 2.3 Why the shipped envelope cannot click

A zero-length attack is a step, and a step on a sounding oscillator is broadband. Measured as
high-frequency energy above 4 kHz on a gated 1 kHz sine — where a smoothly-gated tone has none:

| requested attack | click energy above 4 kHz | peak |
|---|---|---|
| 0.0 ms | −64.0 dB | 1.000 |
| 0.5 ms | −64.0 dB | 1.000 |
| 1.0 ms | −64.0 dB | 1.000 |
| 5.0 ms | −63.6 dB | 1.000 |
| 20.0 ms | −63.5 dB | 1.000 |

**Flat.** A zero-millisecond attack measures identically to a twenty-millisecond one, which is not
what an unprotected envelope does.

The reason is one line:

```rust
fn coef(time_s: f32, taus: f32, sample_rate: f32) -> f32 {
    let t = time_s.max(MIN_TIME_S);
    ...
}
```

with `MIN_TIME_S = 0.001`. **Every stage time is clamped to a millisecond minimum**, so a zero-length
attack is unreachable through the API at all. At 44.1 kHz a 1 ms attack is 44 samples, which is slow
enough that the resulting transient has no meaningful energy above 4 kHz.

Two things follow. First, the parameter layer's 5 ms attack floor
(`time_param("Attack", 0.005, 8.0)`) is a *second* line of defence, not the only one — the DSP
protects itself even if a parameter range is widened later. Second, and this is why the measurement
is worth keeping: **that clamp is load-bearing and looks like a rounding detail.** Anyone
"simplifying" `coef` by removing the `max` reintroduces the click, and nothing else in the code
would stop them.

The peak column is the other half: no stage overshoots 1.000, so the 1.2 target in §2.2 never
escapes into the output.

## 2.4 The state machine, and what breaks it

The parts that are easy to get wrong, all visible in `envelope.rs`:

**Retrigger from a non-zero level.** Triggering while the envelope is still sounding must continue
from the current level rather than jumping to zero — jumping is a step, and §2.3 is about steps. The
shipped `trigger()` sets the stage without resetting the level, which is the correct behaviour and
is what makes legato work.

**Sustain tracked, not stepped.** If the sustain parameter moves while the envelope is holding, the
level has to glide to the new value:

```rust
Stage::Sustain => {
    // Track sustain changes smoothly rather than stepping to them.
    self.level = sustain + (self.level - sustain) * self.decay_coef;
}
```

Without that, automating sustain produces exactly the zipper noise
[03-smoothing-and-events.md](03-smoothing-and-events.md) measures.

**Knowing when it is finished.** `tail_samples()` exists so the voice can be freed when the release
has decayed below audibility rather than when it reaches exactly zero — which, being exponential, it
never does. Get this wrong in the other direction and voices are stolen while still sounding.

**Exact silence.** `ZERO_THRESHOLD` at −80 dB and the crate's denormal flushing together mean a
released envelope produces *exactly* zero rather than a decaying denormal, which is both a CPU
concern and what makes "silence in gives exactly zero out" testable.

## 2.5 One envelope, two destinations

Worth recording because it is mxm-mono-01's defining constraint and it is an envelope-design decision,
not an oscillator one: the instrument has **one** ADSR driving both the filter and the amplifier, as
the SH-101 does. The design brief calls it "the most common source of confusion for anyone expecting
two envelopes".

The consequence for this chapter: the decay parameter shapes both timbre and articulation at once,
so its curve matters twice over. A linear decay that sounds acceptable on the amplifier will sound
wrong on the filter, and vice versa — which is an argument for the exponential curve being the right
default even before the analog-modelling argument.

## 2.6 What this says for the collection

- **Exponential by default**, with a corrected `taus` so the time parameter means what it says.
- **Keep `MIN_TIME_S`.** It is the only thing preventing a click, and it does not look like it.
- **Track sustain, do not step to it**, and do the same for any parameter an envelope reads while
  running.
- **If a curve switch is ever added**, model it on the Digital/RC distinction rather than a
  free-floating "shape" knob: two named, meaningful positions beat a continuum nobody can aim.
- **Do not reset level on retrigger.** Continue from where it is, or legato clicks.

---

Next: [03-smoothing-and-events.md](03-smoothing-and-events.md)
