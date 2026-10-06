# 5. Seminal machines, and what to take from them

[← efficiency](04-efficiency.md) · [index](README.md) · [next: testing →](06-testing.md)

A note on why there is any variety at all: **Bob Moog patented the transistor ladder in 1969**
(US 3,475,623). Everyone else had to design around it. Diode ladders, Sallen–Key filters, state
variable filters and OTA designs all exist in synthesizers partly because a patent lawyer said so.
The diversity of filter character in 1970s synths is a side effect of intellectual property law.

---

## 5.0 The map

| Instrument | Topology | Poles | Self-osc | Signature |
|---|---|---|---|---|
| Minimoog / Model D | Transistor ladder | 4 | Yes, `k = 4` | Fat, bass droops with resonance, clean sine oscillation |
| ARP 2600 (early 4012) | Transistor ladder | 4 | Yes, `k = 4` | Moog-like; replaced after Moog's visit |
| ARP 2600 (later 4072) | 4-pole OTA cascade | 4 | Yes, `k = 4` | **Same transfer function, same droop** — deep-dive (`research:filters/machines/arp-4072.md`) |
| EMS VCS3 / Synthi | Diode ladder, 3 diodes | 4 | `k ≈ 9.8` | Wooly then vicious; distorts before it sings |
| Steiner-Parker Synthacon | Sallen–Key, diode VCRs | 2 | Yes | Multimode via three *inputs*; passband holds to 0.01 dB across the resonance range — deep-dive (`research:filters/machines/steiner-parker.md`) |
| Korg MS-20 Rev 1 | Sallen–Key, Korg-35 IC | 2-pole LP, **1-pole HP** | Yes, `k₁k₂ = 3` | Diodes in the *forward* path: everything distorts |
| Korg MS-20 Rev 2 | Buffered OTA cascade, **not** Sallen–Key | 2-pole LP, **1-pole HP** | Yes, `k₁k₂ = 2` | Diodes in the *feedback* path: only the corner distorts |
| Roland SH-101 | IR3109 quad OTA | 4, **no HPF** | Yes | Diode clamp in the feedback path; raunchy at high resonance |
| Roland Juno-6/60/106 | IR3109 quad OTA | 4 + non-resonant HP | Yes | Q compensation keeps the body; the 106's HPF position 0 *boosts* bass |
| Roland Jupiter-8 | IR3109 quad OTA | 4, **12/24 dB switch** | Yes | Pole mixing in hardware, 1981 |
| Roland Jupiter-6 | IR3109 as **two 2-pole SVFs** | 2+2, LP/HP/BP | **No** | Same chip, completely different filter |
| Roland TB-303 | Diode ladder, 1 diode | 4 (reads as 18 dB) | Barely | Squelch; the envelope circuitry is half the sound |
| Oberheim SEM | State variable | 2 | No (by design) | Continuous LP–notch–HP sweep; smooth, vocal |
| Oberheim Xpander / Matrix-12 | Ladder + pole mixing | 1–4 | Yes | 15 modes, resonant in all of them |
| Prophet-5 Rev 1/2 | SSM2040 gm-C, external Q VCA | 4 | Yes, `k = 4` | Softer, "organic", partly from instability |
| Prophet-5 Rev 3 | CEM3320 gm-C, **on-chip** Q VCA | 4 | Yes, `k = 4` | Flatter, more forward, more stable, slightly noisier |
| Sequential Pro-One | CEM3320, 150 pF caps | 4 | Yes | Feedback tapped after a ×3.4 buffer — even oscillation across the sweep |
| Yamaha CS-80 | Two 2-pole SVFs (IG00156) | 2 HP + 2 LP | **No** | Damping-based Q, frequency-dependent Q, "dripping wet" |
| Buchla 291/292 and 208 LPGs | Vactrol lowpass + VCA | 2 | No | Struck-object decay; the 208's Cards 10/11 remain a distinct circuit boundary |
| Polivoks | **Slew-limited** state variable | 2 | Yes | Distortion depends on amplitude x frequency; even *and* odd harmonics — deep-dive (`research:filters/machines/polivoks.md`) |

---

## 5.1 Minimoog — the reference point

> **→ Full deep-dive: `research:filters/machines/moog-transistor-ladder.md`** —
> where the `tanh` comes from, thermal drift in cents per degree, and a measurement of what
> per-stage saturation actually does (−40 semitones of level-dependent cutoff).

Four buffered one-poles, global feedback, `H(s) = 1/((1+s)⁴+k)`, oscillation at `k = 4`.

**What makes it sound like that:**

- The transistor pairs saturate *inside* every rung ([C] in §3.2), so drive makes it darker and
  thicker rather than brighter and fizzier.
- The feedback subtraction at the input causes the passband droop — `−14 dB` at DC when `k = 4`.
  The bass leaving as resonance arrives is not a flaw, it is the Moog sound.
- The oscillation is remarkably clean, close to a sine, because the limiting is gentle and
  distributed.

**Take:** the topology, the `k = 4` invariant as a *test assertion*, per-stage saturation if you can
afford it, and the droop. Resist the urge to compensate.

---

## 5.2 Roland IR3109 — SH-101, Juno-6/60/106, Jupiter-8/6

> **→ Full deep-dive: `research:filters/machines/ir3109-roland.md`** — this section is the
> summary; that one is the monograph.

A quad OTA in a 16-pin DIP with four buffers and an antilog converter for exponential control. No
on-chip resonance — the feedback loop is external, which is precisely why instruments using the same
chip sound different.

**The SH-101 detail that matters.** The SH-101 uses the same IR3109 as the Juno-60, but puts
**back-to-back diodes to ground in the resonance feedback loop.** They limit self-oscillation
amplitude and make the filter get raunchy as resonance rises. One component pair, one node, and it
is the difference between "Juno" and "101".

This is the cleanest possible demonstration of chapter 3's thesis: identical poles, identical chip,
different nonlinearity placement, different instrument.

**Also Roland, but not the 101:** the Junos and Jupiters put a **non-resonant highpass in front** of
the resonant lowpass (§2.7), and on the Juno-106 its lowest position is a low-shelf *boost*. The
**SH-101 itself has no highpass** — its panel is VCO, Source Mixer, VCF (Frequency, Resonance, Env,
Mod, Kybd), VCA, ENV, and that is all.

**Before the chip: the SH-2 (1979).** The same cascade in four discrete, hand-matched BA662s with
individual JFET buffers, and the same diode clamp in its resonance path — on the SH-2's own
schematic, three years before the 101. That machine's filter is `mxm-mono-02`'s, and it has its own
deep-dive: **`research:filters/machines/ba662-sh-2.md`**.

**Take:** for mxm-mono-01 specifically — ladder topology and a diode clamp rather than a plain `tanh` in
the feedback path; **taken, 2026-09-03** (`crates/mxm-mono-01-dsp/src/filter.rs`, measured in the
deep-dive's §11). **Full deep-dive: `research:filters/machines/ir3109-roland.md`** — the
chip, the per-instrument resonance circuits, input- versus output-side Q compensation, the Jupiter-8
slope switch, the Jupiter-6 SVF configuration, working Rust and measured results.

---

## 5.3 TB-303 — a diode ladder and a great deal of context

> **→ Full deep-dive: `research:filters/machines/tb303-diode-ladder.md`** — the pole
> set as the model, the +309-cent oscillation offset, and measurements showing the filter core is
> *cleaner* than a transistor ladder, not dirtier.

Four diode-connected transistors, one diode at the top of the ladder, the bottom capacitor halved.
Because the rungs are not buffered, the poles interact: at `k = 0` they sit at `−3.532, −2.347,
−1.000, −0.121` rather than all at `−1`, and self-oscillation needs `k ≈ 18.4`.

Consequences you can hear:

- The pole near the origin means the response is already sagging well below nominal cutoff — the
  gentle corner that made everyone call it an 18 dB filter. It is genuinely 24 dB/oct once you are
  properly into the stopband.
- Needing `k ≈ 18` to oscillate means the internal signal levels are *enormous* before it sings, so
  it distorts long before it oscillates. Acid squelch is a distorting filter, not a resonating one.

**The honest caveat:** Stinchcombe's own conclusion is that the ancillary envelope circuitry around
the 303's filter — with its complex interactions — probably contributes as much to the legendary
sound as the filter core. The famously snappy accent-driven envelope and the way it interacts with
the VCA is not the filter. Model the whole voice or do not claim to have modelled a 303.

**Take:** if you want dirt, build a filter that *distorts before it oscillates*. The route to that
is unbuffered stage coupling and a high oscillation threshold, not more `tanh`.

---

## 5.4 Korg MS-20 — the screaming Sallen–Key

Two 2-pole sections, highpass then lowpass, both voltage-controlled Sallen–Key. Rev 1 (1978–79)
uses the Korg-35 IC; Rev 2 (1979–83) uses LM13600 OTAs in a two-cascaded-one-pole arrangement.

Rev 1's aggression comes from **forward-path diodes clipping asymmetrically inside the resonance
path**. It is not that the filter is more resonant; it is that the resonance is limited violently
and asymmetrically, which produces even harmonics and a level-dependent operating point.

**Take:** as §2.4 argued, implement the linear part as an SVF and change the *mapping* per
revision. Two corrections to what this section used to say: the **highpass variants are 6 dB/octave,
not 12** (Korg grounded the lowpass input and fed the signal into a lifted capacitor rather than
doing the RC-CR swap), and Rev 2 is **not** a Sallen–Key at all.

**→ Full deep-dive: `research:filters/machines/korg35-ms20.md`** — both transfer functions,
the reverse-saturation trick that gave a Sallen–Key voltage control, the diode knees (8.8 mV versus
667 mV), and a measured listening test that tells the two revisions apart.

---

## 5.5 Oberheim SEM and Xpander — two different great ideas

> **→ Full deep-dive: `research:filters/machines/oberheim-sem-xpander.md`** — all
> fifteen modes verified, the 33.6 dB gain spread between them, and the SEM's mode knob as three
> mixing coefficients.

**SEM (1974):** a 12 dB/oct state variable filter with a continuous knob from lowpass through notch
to highpass. It deliberately does not self-oscillate. This is the argument that a *2-pole* filter
with a good mode sweep can be more musical than a 4-pole with more resonance — the SEM's smooth,
vocal quality is a big part of why it is still cloned.

**Xpander / Matrix-12 (1984):** one 4-pole VCF, every rung tapped, 15 responses from signed sums of
the taps (§2.6). Modes include 1–4 pole lowpass, 1–3 pole highpass, 2- and 4-pole bandpass, 2-pole
notch, 3-pole phase shift, and four "+ 1-pole lowpass" combinations.

**Take:** both, and they compose. Pole mixing on a ladder costs five multiply-adds and is the single
highest-value feature in this document. And interpolating between mix vectors gives the SEM's
continuous mode sweep as a bonus the hardware could not do.

---

## 5.6 Prophet-5 — the same filter, twice, sounding different

> **→ Full deep-dive: `research:filters/machines/ssm2040-cem3320-prophet.md`** —
> the two chips, the gm cell and its 51:1 input attenuator, Q compensation by the manufacturer's own
> names, the Pro-One's variation, working Rust with an arrowhead Newton solve, and measured answers
> on what oversampling actually buys.

Rev 1/2 used the SSM2040; Rev 3 the CEM3320. Both realise the same idealised 4-pole cascade with
global feedback. Players consistently describe Rev 2 as softer, silkier, more three-dimensional with
a fuzzier resonance, and Rev 3 as flatter, more forward, more aggressive, slightly noisier.

The usual explanation is the honest one: **Rev 1/2 get much of their character from instability.**
Tuning drift, imperfect matching, and per-voice variation gave an "organic" quality the more stable
Curtis design lacks. Sequential's own 2020 reissue includes both filter designs and a switch.

**Take:** per-voice component variation and drift (§3.8) are not garnish. They are a substantial
part of what people mean by "analog". Implement them, expose them as one macro control, and default
it to non-zero. The strongest evidence: u-he bought **two Pro-Ones** to model for Repro-1 and had to
ship **two filter models**, a few semitones apart in cutoff and with different resonance behaviour.
Same model, same chip, same circuit.

---

## 5.7 Yamaha CS-80 — resonance without feedback

> **→ Full deep-dive: `research:filters/machines/yamaha-cs80.md`** — the RC bypass that makes
> Q fall as the filter opens, measured at 2.7x across the sweep, and what MXM-80 needs.

Two 12 dB state variable sections per channel, highpass and lowpass, on Yamaha IG00156 gain cells.
The interesting parts:

- **It cannot self-oscillate**, by design: external limiting resistors on the gain cells cap the
  feedback, and resonance is achieved by *reducing damping* rather than by adding a positive
  feedback loop. Damping never reaches zero.
- **Q is frequency-dependent.** The resonance peak height changes with cutoff.
- There is an additional gentle single-pole lowpass in the path.

Together these produce the "dripping wet" resonance that emulations notoriously miss — because
emulations implement resonance as a feedback gain that can reach oscillation, and then wonder why it
sounds wrong.

**Take:** in an SVF, `k = 1/Q` is a *damping* term. Clamp it away from zero (`k ≥ 0.15`, say) and
make it a function of cutoff, and you have the CS-80 behaviour directly — it is not a hack, it is
what the circuit does. A filter that cannot self-oscillate is a legitimate design choice and it
frees the whole top of the resonance control for musically useful settings.

---

## 5.8 Buchla low-pass gates — the filter as a physical object

> **→ Full deep-dive: `research:filters/machines/buchla-lowpass-gate.md`** — the
> vactrol's real time constants, the three panel modes, the struck-object signature measured, and
> the electrical boundary between its 291/292-derived model and the Music Easel's Cards 10/11.

A 2-pole lowpass whose cutoff and amplitude are both driven by the resistance of a vactrol. The
vactrol responds fast to a rising control and very slowly to a falling one, with a tail that can run
into seconds. Hit it with a pulse and you get a bright transient that darkens and fades together —
the acoustic signature of a struck object.

Digital LPGs usually disappoint because they model the *filter* and not the *vactrol*. The published
Parker & D'Angelo DAFx-13 model is all about the photoresistor's nonlinear, hysteretic response. The
deep-dive's measurements of a model built from that evidence keep **9 %** of brightness at −20 dB,
against **96 %** for a VCA. That is family-level behavioural evidence, not proof that its circuit
values or 12 ms/250 ms timing constants describe an original 208. The Easel's two gates are
specific switched circuits on Cards 10/11, and the deep-dive keeps them separate. §2.8 has the
generic sketch.

**Take:** sometimes the character is not in the filter at all but in what drives it. Also: coupling
brightness to amplitude is a general trick — it is what makes any filtered sound read as "acoustic"
rather than "synthetic", and it costs one multiply.

---

## 5.9 Steiner-Parker, EMS, Polivoks — the useful outliers

> Deep-dives: **Steiner-Parker (`research:filters/machines/steiner-parker.md`)** — multimode by three *inputs*, and a
> passband that does not move with resonance. **Polivoks (`research:filters/machines/polivoks.md`)** — filtering by
> slew limiting, measured at 6235x more frequency-dependent than a saturator.

- **Steiner-Parker Synthacon:** a 2-pole Sallen–Key with *three inputs* (LP, HP, BP) instead of
  three outputs — you choose the response by where you inject, and you can inject into more than
  one at once. Positive feedback means resonance does not cost you level, unlike the Moog. Worth
  copying as an interface idea: a mode "blend" that is genuinely a mix, not a crossfade.
- **EMS VCS3:** diode ladder with three diodes at the top, oscillating at `k ≈ 9.8` — halfway
  between the 303 and a transistor ladder. Distorts on the way to oscillating.
- **Polivoks:** built from slew-limited programmable op-amps rather than conventional RC filter
  cores, with the drive character coming from how the resonance circuit interacts with the supply
  rails. The lesson is that **slew limiting is a distinct kind of nonlinearity** — it limits the
  *derivative*, not the amplitude, which produces a different and much more brutal spectrum than any
  waveshaper. If you want an unusual filter voice, a slew limiter in the feedback path is unexplored
  territory in software.

---

## 5.10 The transferable lessons

1. **Same poles ≠ same filter.** Moog, Roland, SSM, Curtis all realise `1/((1+s)⁴+k)`.
2. **One component in one place is a whole instrument's identity** — the SH-101's feedback diodes.
3. **Distorting before oscillating is a design target**, achieved by unbuffered coupling and a high
   oscillation threshold (303, VCS3), not by adding saturation.
4. **Not self-oscillating is a legitimate, musical choice** (SEM, CS-80) and it makes the whole
   resonance control usable.
5. **Pole mixing is free and nobody does it** (Xpander).
6. **Instability is character** (Prophet Rev 2). Model tolerance and drift.
7. **Sometimes the character is upstream of the filter** (303 envelope, Buchla vactrol).
8. **Coupling brightness to amplitude** makes things sound physical.
9. **The filter is part of an architecture** — a non-resonant HPF in front (Roland) or a resonant
   HP in series (MS-20, CS-80) changes the instrument as much as the LP does.

---

[← efficiency](04-efficiency.md) · [index](README.md) · [next: testing →](06-testing.md)
