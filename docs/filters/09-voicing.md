# 9. One editable voicing model for every family

[← sources](08-sources.md) · [index](README.md) · deep-dives: machines/ (`research:filters/machines/README.md`)

The Prophet deep-dive `research:filters/machines/ssm2040-cem3320-prophet.md` §12
built an advanced panel for one filter family: twelve controls, a set of named models, and "save as
your own". This chapter generalises it so **every** family in
machines/ (`research:filters/machines/README.md`) is editable the same way, behind the same panel.

Doing that needs exactly one hard problem solved, and the rest is bookkeeping.

---

## 9.1 The hard problem: resonance does not mean the same thing twice

The families' native resonance parameters are not remotely comparable:

| Family | Native parameter | Value at which it sings |
|---|---|---|
| Moog ladder (`research:filters/machines/moog-transistor-ladder.md`) | loop gain `k` | **4.00** |
| gm-C 4-pole (`research:filters/machines/ssm2040-cem3320-prophet.md`) | loop gain `k` | **4.00** |
| Diode ladder (TB-303) (`research:filters/machines/tb303-diode-ladder.md`) | loop gain `k` | **18.34** |
| Diode ladder (EMS) | loop gain `k` | **10.08** |
| MS-20 / Steiner / SVF (`research:filters/machines/korg35-ms20.md`) | damping `1/Q` | **0** (damping removed) |
| CS-80 / SEM (`research:filters/machines/yamaha-cs80.md`) | damping `1/Q` | **never** — the floor stops it |

A single resonance knob wired straight to the native parameter would put the singing point in a
completely different place on every model, and in one case nowhere at all. Any "advanced" panel that
lets a user switch families would be unusable.

### The fix: normalise on the family's own threshold

Introduce a **normalised resonance** `r`, where **`r = 1.0` is this family's own threshold**,
whatever that numerically is. Each family converts `r` to its native parameter; the voicing never
sees native units.

```rust
pub trait Voiceable {
    /// Convert a **normalised** resonance — where `1.0` is this family's own
    /// oscillation threshold — into whatever native parameter it wants.
    ///
    /// A ladder returns `4·r`; a TB-303 diode ladder returns `18.34·r`; a
    /// damping-resonance filter returns a damping that reaches its floor at 1.
    fn native_resonance(&self, r: f32) -> f32;

    /// Frequency of self-oscillation relative to the nominal cutoff. 1.0 for
    /// almost everything; 1.1955 for a TB-303 diode ladder.
    fn osc_ratio(&self) -> f32 { 1.0 }

    /// Can this family oscillate at all? `false` for damping-with-a-floor.
    fn can_oscillate(&self) -> bool { true }

    fn set_native(&mut self, cutoff_hz: f32, native: f32, sample_rate: f32);
    fn process_one(&mut self, x: f32) -> f32;
    fn reset_all(&mut self);
}
```

The voicing then maps the knob to `r` exactly as the Prophet chapter did — two linear segments
hinged on `self_osc_point` — but in normalised units:

```rust
/// Knob to **normalised** resonance: 1.0 is the family's own threshold.
#[inline]
pub fn resonance(&self, knob: f32) -> f32 {
    let c = knob.clamp(0.0, 1.0);
    let p = self.self_osc_point.clamp(0.05, 0.99);
    if c <= p {
        c / p
    } else {
        1.0 + (self.max_resonance - 1.0) * (c - p) / (1.0 - p)
    }
}
```

**Measured (§9.6): asking for `self_osc_point = 0.80` makes the Moog ladder sing at knob 0.798, the
TB-303 diode ladder at 0.802, the gm-C 4-pole at 0.798 and a damped 2-pole at 0.800** — with native
thresholds of 4.00, 18.34, 4.00 and 0.00 respectively.

One knob, one meaning, six families.

---

## 9.2 Two properties that are easy to forget

Normalising resonance is necessary but not sufficient. Two more things must live in the core, and
both are invisible until something breaks.

### `osc_ratio` — a self-oscillating filter does not always sing at its cutoff

The diode ladder (`research:filters/machines/tb303-diode-ladder.md` §2) oscillates at
**1.1955 × cutoff**, a minor third sharp. Everything else here sings at 1.000 ×.

Any feature that treats the filter as an oscillator — a tuning test, a drone, a "filter tracks the
keyboard" mode — has to ask the family, not assume:

```rust
pub fn oscillation_hz(&self, cutoff_hz: f32) -> f32 {
    cutoff_hz * self.family.osc_ratio()
}
```

Measured: for a nominal 440 Hz cutoff, the ladder sings at **440.0 Hz** and the diode ladder at
**526.0 Hz**.

### `can_oscillate` — "never" is a legitimate answer

The CS-80 (`research:filters/machines/yamaha-cs80.md`) and the SEM cannot self-oscillate by design, and that is the
point: it frees the whole resonance range for musically useful settings. Expressed in this schema it
is a **damping floor**, one number:

```rust
fn native_resonance(&self, r: f32) -> f32 {
    // r = 1 means "at threshold", i.e. zero damping — clamped by the floor.
    (self.max_damping * (1.0 - r.clamp(0.0, 1.0))).max(self.damping_floor)
}
fn can_oscillate(&self) -> bool { self.damping_floor <= 0.0 }
```

Measured: with the floor set, the filter never sings at any knob position or `max_resonance`
setting, and still decays to exact zero at maximum resonance. The editor should grey out
`self_osc_point` when `can_oscillate()` is false rather than showing a control that does nothing.

---

## 9.3 The two-layer schema

**Core voicing** is what every family shares. **Family voicing** is what only that family has.
Trying to flatten the two produces a struct with twenty mostly-inapplicable fields.

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoreVoicing {
    // --- tuning ---
    pub cutoff_span_octaves: f32,
    pub cutoff_offset_semitones: f32,
    /// 0 = fixed cutoff, 1 = follows pitch exactly.
    pub key_track: f32,

    // --- resonance ---
    /// Knob position at which the filter reaches **its own** threshold.
    /// Ignored by families that cannot oscillate.
    pub self_osc_point: f32,
    /// Normalised resonance at the top of the knob. 1.0 stops exactly at
    /// threshold; more pushes past it.
    pub max_resonance: f32,

    // --- drive ---
    pub drive_trim_db: f32,

    // --- output ---
    pub output_highpass_hz: f32,
    pub output_gain_db: f32,
}
```

| Layer | Controls | Why there |
|---|---|---|
| **Core** | cutoff span, cutoff offset, key tracking, self-oscillation point, max resonance, drive trim, output highpass, output gain | Every family has a cutoff, a resonance knob, an input level and an output |
| **Family** | nonlinearity placement and curve; pole set; mode/tap vector; Q-compensation scheme; vactrol times; slew limit; input mix | These have no meaning outside their family. A pole set is nonsense for a 2-pole; a vactrol is nonsense for anything but a gate |

What belongs in which layer is decided by one question: **would a user switching families expect
this setting to carry over?** Cutoff offset, yes. Diode knee, no.

### The family layer, per machine

| Family | Its own controls | Deep-dive |
|---|---|---|
| Moog ladder | saturation placement (feedback / per-stage), cell drive, thermal drift | `research:filters/machines/moog-transistor-ladder.md` §6 |
| gm-C 4-pole | cell drive, cell bias, feedback saturation and knee, Q-comp mode and amount | `research:filters/machines/ssm2040-cem3320-prophet.md` §12 |
| Diode ladder | pole set (TB-303 / EMS), feedback drive | `research:filters/machines/tb303-diode-ladder.md` §5 |
| MS-20 | diode placement (forward / feedback), gain and drop, damping mapping (`3−K` / `2−K`) | `research:filters/machines/korg35-ms20.md` §5 |
| Xpander | the 15-mode mix vector, per-mode makeup gain | `research:filters/machines/oberheim-sem-xpander.md` §5 |
| CS-80 | damping floor, damping per octave, extra pole | `research:filters/machines/yamaha-cs80.md` §4 |
| Buchla gate | vactrol rise / fall / speed-up, panel mode | `research:filters/machines/buchla-lowpass-gate.md` §5 |
| Steiner | three input levels | `research:filters/machines/steiner-parker.md` §4 |
| Polivoks | slew limit | `research:filters/machines/polivoks.md` §4 |

---

## 9.4 The shared wrapper

```rust
#[derive(Debug, Clone, Copy)]
pub struct Voiced<F: Voiceable> {
    pub family: F,
    pub core: CoreVoicing,
    hp_g: f32,
    hp_s: f32,
    drive_gain: f32,
    out_gain: f32,
}

impl<F: Voiceable> Voiced<F> {
    /// Knob positions in 0..=1, plus the note's offset from the key-track centre.
    pub fn set(&mut self, cutoff_knob: f32, res_knob: f32, base_hz: f32, note_oct: f32, sr: f32) {
        let hz = self.core.cutoff_hz(cutoff_knob, base_hz, note_oct).clamp(10.0, 0.45 * sr);
        let r = self.core.resonance(res_knob);
        let native = self.family.native_resonance(r);
        self.family.set_native(hz, native, sr);
        let g = prewarp(self.core.output_highpass_hz.max(1.0), sr);
        self.hp_g = g / (1.0 + g);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.family.process_one(x * self.drive_gain);
        let v = self.hp_g * (y - self.hp_s);
        let lp = v + self.hp_s;
        self.hp_s = flush(lp + v);
        (y - lp) * self.out_gain
    }
}
```

Generic over the family, so it monomorphises and there is no `dyn` in the audio path
([04 §4.9](04-efficiency.md)). Cutoff and resonance arrive as **knob positions**, never as Hz or
loop gain — the mapping is the voicing's job and nothing upstream should know the native units.

Modulation still sums in octaves and clamps once
([07 §7.2](07-rust-recipes.md#72-parameter-mapping)); `key_track` is one more octave term.

---

## 9.5 Putting it in a plugin

The parameter/state split is already a contract in
[`plugins/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/AGENTS.md), and it applies unchanged:

| | Automatable parameter | Persisted state |
|---|---|---|
| Cutoff, Resonance, Drive | **yes** | |
| Family selector, and the model selector within it | **yes** — stepped | |
| Core voicing (8 controls) | | **yes** |
| Family voicing | | **yes** |

**Panel layout follows the schema.** The advanced panel has one group per core section — Tuning,
Resonance, Drive, Output — plus a family section whose contents change with the selected family.
That is organisation by task and signal flow, which is what
[`docs/MXM_DESIGN_SYSTEM.md`](../MXM_DESIGN_SYSTEM.md) §2 asks for anyway.

**Switching family is a bigger event than switching model.** The core voicing carries over; the
family voicing cannot. Either reset the family section to that family's default, or keep a
per-family remembered set — but do not silently drop the user's edits.

**Bound the ranges in the DSP, not only in the editor.** §9.6 hammers every family through three
extreme voicings with full-range cutoff modulation at 97 % resonance: all stay finite and all reach
exact silence, but peaks range from **0.5 to 140**. A user model is user-generated input.

---

## 9.6 Measured results

89 tests, all passing. These are the ones specific to this chapter.

### One knob position, six families

Bisecting the knob position at which each family begins to oscillate:

| Asked (`self_osc_point`) | Moog ladder | Diode ladder | gm-C 4-pole | Damped 2-pole |
|---|---|---|---|---|
| 0.65 | 0.649 | 0.653 | 0.649 | 0.650 |
| 0.80 | 0.798 | 0.802 | 0.798 | 0.800 |
| 0.92 | 0.918 | 0.921 | 0.918 | 0.920 |
| *native threshold* | *4.00* | *18.34* | *4.00* | *0.00 damping* |

Worst error **0.004** of knob travel, across families whose native thresholds differ by 4.6× and one
that runs the opposite way.

### A damping floor forbids oscillation

With the CS-style floor at 0.16, the native resonance at the top of the knob is **0.160** — the
floor, exactly — and the filter never sings at any setting, including `max_resonance = 1.5`. It
still decays to exact zero at maximum resonance.

### Cutoff offset is semitones everywhere

| Asked | Moog | gm-C | Steiner |
|---|---|---|---|
| −7.0 st | −7.07 | −7.07 | −6.98 |
| +5.0 st | +5.02 | +5.02 | +4.99 |

### Key tracking is octaves

`key_track = 1.0`, one octave up: **640.0 Hz → 1280.0 Hz**, exactly +1200 cents.

### Self-oscillation pitch is per family

For a nominal 440 Hz cutoff: Moog ladder **440.0 Hz**, diode ladder **526.0 Hz** (+309 cents).

### The shared output stage

| Highpass setting | Corner, relative to passband | Effect of the +6 dB output setting |
|---|---|---|
| 5 Hz | −3.12 dB | **+6.00 dB** |
| 40 Hz | −3.12 dB | **+6.00 dB** |

Both measured as *differences*. The absolute passband level is meaningless on its own, because a
ladder droops with resonance and a Sallen–Key does not — which is the whole reason
Q compensation (`research:filters/machines/ir3109-roland.md` §4)
is a family control rather than a core one.

### Every family survives the whole voicing range

Three voicings per family — a narrow conservative one, the default, and a deliberately extreme one —
each hammered for two seconds with full-range cutoff modulation every 32 samples at 97 % resonance,
6 dB of drive trim and up to 12 dB of output gain:

| Family | Peak, narrow → extreme | Time to exact silence |
|---|---|---|
| Moog ladder | 2.0 → **140.2** | 4.00 → 0.50 s |
| Diode ladder | 0.5 → 79.4 | 3.75 → 0.50 s |
| gm-C 4-pole | 4.5 → 134.8 | 3.75 → 0.50 s |
| MS-20 (damped) | 34.9 → 132.2 | 5.00 → 0.50 s |
| CS-80 (floored) | 4.2 → **41.6** | 3.50 → 0.50 s |
| Steiner-Parker | 34.9 → 132.2 | 4.25 → 0.50 s |

All finite, all reaching **exact zero**. Two things worth reading off that table:

- **The extreme voicings are loud.** 140× is a hostile transient no automation curve produces, but
  it is why §9.5 says to clamp in the DSP.
- **The floored family is the quietest at the extreme** (41.6 against 132–140), because it cannot be
  driven past its threshold at all. That is the CS-80's design argument showing up as a number.

*(An earlier version of this test asserted exact silence after a fixed 4-second settle and failed on
one family at 1.9e-18. The filter was fine — it was still crossing the flush threshold. The test now
measures **how long** exact silence takes instead of assuming.)*

---

## 9.7 What it costs — and why this is not one do-everything filter

The obvious worry about a shared voicing layer is that it turns into one general filter that
branches its way through every family, per sample. It does not, and the measurements say so.

**`Voiced<F>` is generic, not dynamic.** Each family is its own type with its own `process`; the
wrapper monomorphises into a separate specialised copy per family. There is no `dyn`, no vtable, and
nothing per-sample that asks "which family am I".

The clearest evidence is that the families cost *different* amounts. If there were a shared
do-everything core underneath, they would converge:

| Family inside the same wrapper | ns/sample |
|---|---|
| `Voiced<SteinerVoiced>` — 2-pole, closed form | **13.05** |
| `Voiced<DampedTwoPole>` — 2-pole, 1 scalar Newton | 68.97 |
| `Voiced<DiodeLadder>` — 4-pole spread, 1 solve | 79.27 |
| `Voiced<Ladder4>` — 4-pole, 1 scalar Newton | 82.26 |
| `Voiced<GmFilter>` — 4-pole, arrowhead 4×4 Newton | **209.95** |

A **16× spread** is what separate implementations look like.

### The wrapper itself is nearly free

Identically configured bare core versus the same core wrapped:

| | Bare | Wrapped | Overhead |
|---|---|---|---|
| Ladder | 81.08 | 81.70 | **+0.63 ns (+0.8 %)** |
| gm-C | 206.38 | 209.38 | **+3.00 ns (+1.5 %)** |

That is one one-pole highpass and two multiplies, which is what the wrapper adds and nothing else.
Cutoff mapping, resonance normalisation and `native_resonance` all run at **coefficient-update
rate**, never per sample.

### Three things the measurement caught

**`#[inline]` on the trait impls is worth 11 ns/sample.** Without it, `Voiced<GmFilter>`'s overhead
was +14.05 ns (+6.8 %) instead of +3.00 ns (+1.5 %) — the trait method was a real call. Within one
crate LLVM often sees through it; **across a crate boundary it cannot**, which is exactly the point
[04 §4.9](04-efficiency.md#49-dispatch-generics-and-the-inner-loop) makes. Every `Voiceable` method
is `#[inline]`.

**Measurement instrumentation had leaked into the audio path.** `GmFilter::process` was delegating
to `process_measured`, which computes a final residual for the deep-dive's convergence tables —
eight extra saturator evaluations per sample, on top of the 24 the three Newton iterations already
do. A third more saturator work, in the hot path, to produce a number nothing was reading. The two
are now separate functions.

**A per-sample enum match costs nothing measurable.** Several family models `match` on a mode enum
inside `process`. Against calling the specialised path directly:

| | ns/sample |
|---|---|
| `Ladder4::process`, matches on placement | 81.08 |
| The same path, called directly | 81.06 |
| **Difference** | **+0.01 ns (+0.0 %)** |

This is a genuine nuance on [04 §4.9](04-efficiency.md#49-dispatch-generics-and-the-inner-loop),
which says to dispatch per block rather than per sample. That rule is about **unpredictable**
branches. A mode enum is constant across a whole buffer, so the predictor gets it right every time
and the cost disappears. Hoisting it is still tidier, but it is not a performance fix and should not
be sold as one.

### The lever that does matter

Newton iterations dominate the 4-pole cost, and they are almost perfectly linear:

| Fixed iterations | ns/sample | Relative |
|---|---|---|
| 1 | 73.82 | 1.00× |
| 2 | 140.60 | 1.90× |
| **3** | **206.82** | **2.80×** |
| 4 | 274.23 | 3.71× |

About **67 ns per iteration** on roughly 7 ns of fixed overhead. The residual measurements in
the gm-C deep-dive `research:filters/machines/ssm2040-cem3320-prophet.md` §10 say **two steps is
enough and three is comfortable** — so going from three to two saves **32 %** of the whole filter,
which is far more than any amount of branch-hoisting.

### In context

One sample period at 48 kHz is 20 833 ns. Sixteen voices of one filter:

| Filter | 16 voices | Fraction of one core |
|---|---|---|
| Steiner 2-pole | 210 ns | **1.0 %** |
| Moog ladder | 1 317 ns | **6.3 %** |
| gm-C 4-pole (3 iterations) | 3 356 ns | **16.1 %** |

Even the most expensive filter here, sixteen times over, is a sixth of one core — and that is before
dropping the third Newton step or vectorising across voices
([04 §4.8](04-efficiency.md#48-simd)). The voicing layer is not where the money goes.

---

## 9.8 Consequences for this repo

1. **This supersedes the Prophet chapter's schema**, which stays as the worked example with its own
   measurements. `research:filters/machines/ssm2040-cem3320-prophet.md` §12
   is now the *family* layer for the gm-C; this chapter is the core.
2. **`Voiceable` is the shared-crate API.** [07 §7.6](07-rust-recipes.md) says to extract a shared
   filter crate when a second plugin needs one. This is what its trait should look like, and the
   normalised-resonance contract is the part that must not be compromised for convenience.
3. **`native_resonance` is the single most important method.** Everything else is bookkeeping. If a
   family's threshold is not measured, its knob is wrong — and
   [`plugins/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/AGENTS.md) already requires mapping resonance around the
   *measured* threshold.
4. **`osc_ratio` must exist even though it is 1.0 almost everywhere.** The one family where it is
   not would otherwise be silently a minor third out.
6. **The efficiency answer is generics, not enums.** §9.7. If the shared crate is ever extracted,
   `Voiceable` must stay a trait with `#[inline]` impls and `Voiced<F>` must stay generic. A
   `Box<dyn Filter>` or a `match` on family inside `process` would undo all of it.
7. **mxm-mono-01 needs only the core layer today.** One family, one model, no family selector — but
   putting the core in place now means the advanced panel is a UI job later, not a DSP job.

---

[← sources](08-sources.md) · [index](README.md) · deep-dives: machines/ (`research:filters/machines/README.md`)
