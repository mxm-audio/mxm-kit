# 4. Analog character

Chapters 1–3 assumed the target was the mathematical waveform. A real VCO does not produce one, and
the differences are not small or subtle. This chapter is about what they are, which of them are
worth modelling, and which are folklore.

Evidence note, since this is the chapter most prone to confident invention: **no hardware was
measured for this repository.** Claims below are attributed to a datasheet, a service manual, a
published measurement, or a secondary source, and where sources disagree the disagreement is
recorded rather than resolved by preference.

---

## 4.1 The reset is not instantaneous

The single largest difference between a real sawtooth and a digital one.

An analog saw core charges a capacitor with a constant current and discharges it through a switch
when a comparator fires. The discharge takes time — a service-manual figure for a healthy
oscillator's reset is on the order of a microsecond — and the comparator itself has propagation
delay. So the "vertical" edge is a steep but finite slope, and the top of the ramp is rounded where
the comparator begins to turn on.

The best published evidence on how much this matters is Pekonen, Lazzarini, Timoney, Kleimola and
Välimäki's study of a Moog oscillator ([08-sources.md](08-sources.md#moogsaw)). Measuring recorded
waveforms, they report that **the rising part of the period is not linear at all** — it "resembles
more a sinusoid than a linear function" — and that the spectral envelope departs visibly from the
textbook −6 dB/octave.

Their model is phase distortion: run a quarter-cosine through a phase-shaping function with one
parameter `P`, fitted per fundamental. Fitting `P` against 47 recorded waveforms from 86 Hz to
8.3 kHz, they find `P` close to 1 at low fundamentals and falling as `f0` rises, well approximated
by a first-order polynomial in `f0` below 4 kHz.

Two things worth extracting from that:

- **The deviation is frequency dependent.** A real VCO's waveform *shape* changes with pitch,
  because the fixed reset time is a growing fraction of a shrinking period. Any model that applies
  a fixed waveshape at all pitches is missing the mechanism, not just the parameters.
- **The deviation helps with aliasing.** The authors note that a slower reset means less
  high-frequency content, so a faithful model of the rounded reset aliases *less* than an idealised
  sharp one. Analog character and alias suppression point the same way here, which is unusual.

A cheap approximation that captures the first-order effect without a phase-distortion model: treat
the reset as a ramp of finite slope rather than a step, i.e. correct it with a BLAMP-shaped
transition of one sample's duration instead of a BLEP. Ambrits and Bank make exactly this point in
passing — an asymmetric triangle whose transition lasts one sample can safely replace a sawtooth,
costs a slight high-frequency attenuation, and *reduces* aliasing because two samples get corrected
instead of one.

## 4.2 The waveforms are not the same amplitude, and not derived the way you assume

On a CEM3340, the datasheet-level facts are that the sawtooth swings 2/3 of the positive supply,
the triangle 1/3, and the pulse the full supply minus about 1.5 V. Designs using the chip need
different mixer resistors per waveform to compensate — the SH-101 included.

That is a real, audible thing that a digital implementation gets wrong by default: if your saw,
pulse and sub all leave the oscillator at ±1, your mixer balance is not the hardware's mixer
balance, and the patches that made the original famous do not transfer. Level is part of the
architecture, not a normalisation detail.

**Sources conflict on the 3340's internal core.** Electric Druid's write-up describes a ramp core
with the triangle derived from it; a circuit analysis on xtal-flux describes a triangle core —
capacitor charged *and discharged* at constant current — with saw and pulse derived from the
triangle. The chip's provision of both hard and soft sync inputs is weak evidence for the triangle
core, since soft sync classically works by reversing the charge direction, which only exists on a
triangle core. We have not resolved it, and it matters only for modelling sync behaviour; for saw
and pulse output the two descriptions predict the same thing.

What is not in dispute, and is useful: **the pulse is a comparator on the core waveform against the
pulse-width control voltage.** That makes pulse width a direct function of a voltage compared
against a ramp, which is why PWM on these machines is close to linear in the control voltage and
why the width tracks in the way [03-waveshapes.md §3.2](03-waveshapes.md#32-pulse-and-pwm) models.

## 4.3 High-frequency tracking error

Comparator delay is a fixed time, so it eats a growing fraction of the period as pitch rises: the
oscillator runs progressively *flat* at the top of its range. The CEM3340 has a dedicated pin for
this — a current output that compensates for switching delay above roughly 5 kHz, trimmed with a
feedback resistor to the frequency control input.

The existence of a factory trim for it is the interesting part. It means:

1. Real machines are mistuned at the top of their range *by an amount that depends on how well a
   technician set a trimmer in 1983*.
2. A digital oscillator that tracks perfectly across the range is, in this specific respect,
   already unlike every hardware unit ever built.

Whether to model it: probably not as a deliberate detune, since a synth that goes flat above 5 kHz
reads as a bug. Worth knowing when comparing a model against a recording, though — a tuning
mismatch at the top octave may be the *hardware* being wrong.

## 4.4 Drift, and what actually drifts

The word "drift" covers three different mechanisms with different time constants, and conflating
them produces the characteristic bad emulation where everything wobbles at the same rate.

| Mechanism | Time scale | What it sounds like |
|---|---|---|
| Thermal, warm-up | minutes | the instrument slowly going in tune after power-on |
| Thermal, ambient and self-heating | seconds to minutes | slow, correlated pitch wander |
| Supply and bias noise | milliseconds | a low-level frequency jitter, effectively phase noise |

A monophonic synth with a single oscillator has nowhere to *show* slow drift — there is nothing for
it to beat against. On a polysynth with several VCOs, the same drift produces the chorusing that is
half of why those instruments are loved. **This is why "analog drift" as a feature is often
disappointing on a monosynth and transformative on a poly**: the mechanism only becomes audible as
a relationship between oscillators.

The SH-101 case is more specific still. Its pitch CV comes from a CPU-driven D/A converter with its
own tune, width and linearity trims — the service manual's adjustment procedure sets the D/A
*before* the VCO, and warns that the VCO width and tune trims interact. So the *pitch command* is
digital and stable; the drift that remains is in the analog exponential converter and the core.
The oscillator is analog, but the keyboard scaling is not, and it does not have the per-note
scaling error a resistor-ladder keyboard would.

## 4.5 The duty cycle that will not stay put

Not every hardware square wave has a 50% duty cycle, and the deviation is often pitch dependent.

Secondary analyses of the TB-303 report that its square is not generated independently but
waveshaped from the sawtooth core, and that its duty cycle is about **45% at high pitches and about
71% at the lowest** — a swing of more than a quarter of the period across the keyboard. That is
not a defect anyone corrected; it is part of the sound.

If a model of that machine is the goal, it is a much larger effect than anything in
[chapter 2](02-antialiasing.md). Given a plain `pulse()` with a width parameter, it costs one
lookup: make the width a function of `f0` instead of a constant.

## 4.6 DCO versus VCO

The Juno DCOs are the useful contrast case, because they are the same *architecture* as a VCO with
one part replaced.

A Juno DCO charges a capacitor in an op-amp integrator exactly as a VCO does, but the reset is
triggered by a countdown counter clocked from a crystal-derived MHz-range master clock: the CPU
computes a divisor from the desired pitch, the counter counts down, and at zero a transistor
discharges the capacitor. The sub-octave comes from a D flip-flop dividing that same timer output
by two, and the pulse is waveshaped from the sawtooth.

So the *waveform* is analog — a real integrator ramp with a real reset — while the *timing* is
crystal-accurate. That combination explains both halves of the Juno reputation: it never drifts,
and it still does not sound like a digital oscillator.

For a digital model this is a gift, because it says exactly where the character lives. It is not in
pitch instability, which the DCO does not have. It is in the ramp and the reset, which is
[§4.1](#41-the-reset-is-not-instantaneous).

There is one digital-side artefact worth knowing about: because the reset instant is quantised to
the master clock, a DCO's pitch is quantised too. At MHz clock rates against audio periods the
quantisation is far too fine to hear as mistuning, but it is a real, describable difference from a
VCO, and it is the reason DCO designs need no tuning trim at all.

## 4.7 What is worth modelling

An opinionated list, in the order the effort pays off, for a virtual-analog instrument that is an
*homage* rather than a circuit emulation:

1. **Nothing.** Ship the clean band-limited waveforms first and get the filter right. The filter is
   where the character of a subtractive synth mostly lives, and a mediocre filter behind a
   beautifully modelled VCO sounds worse than the reverse.
2. **Mixer levels matched to the hardware's**, if a specific machine is the reference. Costs
   nothing, changes every patch.
3. **Pitch-dependent pulse width**, if the reference machine has it. One line, large effect.
4. **A finite reset** — a one-sample transition instead of an instantaneous one. Cheap, reduces
   aliasing, moves the spectral envelope toward the hardware's.
5. **Per-note pitch randomisation**, small and fixed for the duration of a note, if there are
   several oscillators to beat against each other. On a single-oscillator monosynth, skip it.
6. **A full phase-distortion or circuit model.** Only if the machine *is* the product.

Everything on that list below item 3 is optional. Nothing on it is a substitute for getting
[chapter 2](02-antialiasing.md) right, because aliasing is the one artefact that no amount of
character can excuse.

---

Next: `research:oscillators/05-machines.md` — the specific machines, and what each one is evidence for.
