# 8. Sources

[← Rust recipes](07-rust-recipes.md) · [index](README.md)

Annotated: what each one is actually good for, so you know which to open.

---

## Start here

**Vadim Zavalishin — *The Art of VA Filter Design*** (Native Instruments, free)
[discodsp.net/VAFilterDesign_2.1.0.pdf](https://www.discodsp.net/VAFilterDesign_2.1.0.pdf) ·
[NI 1.1.1](https://www.native-instruments.com/fileadmin/ni_media/downloads/pdf/VAFilterDesign_1.1.1.pdf)

The book. Derives everything in [chapter 1](01-fundamentals.md) from first principles: trapezoidal
integration, the bilinear transform, TPT, zero-delay feedback, and then nonlinear versions of all of
it. Chapters on the ladder, the SVF, Sallen–Key and the diode ladder. If you read one thing, read
this. Dense but complete.

**Andy Simper (Cytomic) — *Linear Trapezoidal Integrated State Variable Filter With Low Noise
Optimisation***
[cytomic.com/files/dsp/SvfLinearTrapOptimised.pdf](https://www.cytomic.com/files/dsp/SvfLinearTrapOptimised.pdf) ·
[all Cytomic papers](https://cytomic.com/technical-papers/)

The SVF in [§2.2](02-topologies.md), with the derivation and the full set of output-mixing
coefficients including bell and shelf. Short, practical, immediately implementable. The
[KVR thread](https://www.kvraudio.com/forum/viewtopic.php?t=474128) and
[FredAntonCorvest/Common-DSP](https://github.com/FredAntonCorvest/Common-DSP) carry working C++ ports.

---

## The gm-C family (SSM2040 / CEM3320)

**Sound Semiconductor — SSI2140 datasheet**
[soundsemiconductor.com](https://www.soundsemiconductor.com/downloads/ssi2140datasheet.pdf)

The best datasheet in this bibliography, and the authority on the SSM2040 core — written under its
original designer, Dave Rossum. The four-pole application circuit, the 10k:200 input network, the
phase accounting, "loop gain needs to be exactly 4.0", the three named Q-compensation schemes, and
−18 mV/octave. Read it even if you never touch the chip: it is a filter-design tutorial with a part
number on it.

**Electric Druid — CEM3320 filter designs**
[electricdruid.net](https://electricdruid.net/cem3320-filter-designs/) — per-instrument circuits for
the Pro-One, OB-Xa (both 4-pole and 2-pole) and Elka Synthex, with component values.

Full annotated list in
`research:filters/machines/ssm2040-cem3320-prophet.md` §14.

## The ladder, in detail

**Tim Stinchcombe — *Analysis of the Moog Transistor Ladder and Derivative Filters***
[timstinchcombe.co.uk/synth/Moog_ladder_tf.pdf](https://www.timstinchcombe.co.uk/synth/Moog_ladder_tf.pdf)

The definitive circuit analysis. Where [§2.3](02-topologies.md) and [§2.5](02-topologies.md) get
their numbers: `H(s) = 1/((1+s)⁴+k)`, the 45° pole X, oscillation at `k = 4`, the ~14 dB droop, and
transfer functions for five real diode-ladder variants (TB-303, EMS, Doepfer A-102, Roland 100,
Minisonic) with their pole locations and their much higher oscillation thresholds (`k ≈ 18.4` for
the 303-type, `≈ 9.8` for the EMS type). Also settles the "18 dB vs 24 dB" argument properly.

**Antti Huovilainen — *Non-Linear Digital Implementation of the Moog Ladder Filter*** (DAFx-04)
[dafx.de/paper-archive/2004/P_061.PDF](https://dafx.de/paper-archive/2004/P_061.PDF)

The five-`tanh` nonlinear ladder: four in the stages, one in the feedback. The reference for
per-stage saturation ([§3.2](03-nonlinearity.md) node [C]). Requires ≥2× oversampling. See also
[ddiakopoulos/MoogLadders](https://github.com/ddiakopoulos/MoogLadders) for several models
side by side in C++.

**Stilson & Smith — *Analyzing the Moog VCF with Considerations for Digital Implementation***
(CCRMA, 1996). The paper that started digital ladder modelling; the origin of the `k = 4` result
and of the one-sample-delay-in-the-feedback approach that [§3.3(a)](03-nonlinearity.md) advises
against.

**Raph Levien — *A matrix approach to the Moog ladder filter***
[levien.com/ladder.pdf](http://www.levien.com/ladder.pdf) — a different formulation, useful if you
want the whole ladder as one linear-algebra step.

**D'Angelo & Välimäki — *Generalized Moog Ladder Filter, Parts I & II*** (IEEE/TASLP). Explicit
nonlinear model with a novel delay-free-loop method. Where to go if [§3.3(d)](03-nonlinearity.md) is
what you want.

**Paschou et al. — *Modeling and Measuring a Moog Voltage-Controlled Filter*** (Aalto)
[acris.aalto.fi PDF](https://acris.aalto.fi/ws/portalfiles/portal/27503975/ELEC_Paschou2017_Moog_VCF.pdf) —
measurements of a real unit, which is the part most papers skip.

---

## Nonlinearity and aliasing

**Parker, Zavalishin & Le Bivic — *Reducing the aliasing of nonlinear waveshaping using
continuous-time convolution*** (DAFx-16)
[dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf](https://dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf)

The original ADAA paper. Formalised in Bilbao, Esqueda, Parker & Välimäki, *Antiderivative
Antialiasing for Memoryless Nonlinearities*, IEEE SPL 24(7), 2017.

**Jatin Chowdhury — *Practical Considerations for Antiderivative Anti-Aliasing***
[medium post](https://jatinchowdhury18.medium.com/practical-considerations-for-antiderivative-anti-aliasing-d5847167f510) ·
[code](https://github.com/jatinchowdhury18/ADAA)

The caveats [§3.7](03-nonlinearity.md) lists — half-sample delay, ill-conditioning, what happens in
a loop — laid out by someone who shipped it.

**Albertini, Bernardini & Sarti — *Antiderivative Antialiasing for Stateful Systems***
[researchgate](https://www.researchgate.net/publication/338093154_Antiderivative_Antialiasing_for_Stateful_Systems) —
the extension to filters with memory.

**"Cheap non-linear zero-delay filters"**, KVR
[t=349859](https://www.kvraudio.com/forum/viewtopic.php?t=349859) — the long thread where mystran's
fixed-pivot linearisation ([§3.3b](03-nonlinearity.md)) is developed and argued over. Slow reading,
high value; this is where the practical folklore lives.

**Medine — *Newton–Raphson Solution of Nonlinear Delay-Free Loop Filter Networks*** (IEEE/TASLP
2019) [ieeexplore](https://ieeexplore.ieee.org/abstract/document/8744571/) — when Newton is and is
not applicable, which most implementations assume rather than check.

---

## Other topologies

**Will Pirkle — application notes**, free:
[Virtual Analog Filter Implementation and Comparisons](https://forum.audulus.com/uploads/default/original/2X/8/82ea1a4ef055962ff4a50bcdf5e21f7aa895bebd.pdf) ·
[AN-7: Korg35 Highpass v2](http://www.willpirkle.com/Downloads/Korg35HPFAppNote_V2.pdf) ·
AN-6: Diode Ladder. Plus *Designing Software Synthesizer Plug-ins in C++*. Pirkle is the most
implementation-oriented of the sources: block diagrams and code, less derivation.

**Tim Stinchcombe — *A Study of the Korg MS10 & MS20 Filters***
[PDF](https://www.timstinchcombe.co.uk/synth/MS20_study.pdf) — both revisions, the Korg-35 and the
LM13600 versions, with the clipping-diode detail that gives Rev 1 its scream.

<a id="parker-dangelo-buchla-lpg"></a>
**Parker & D'Angelo — *A Digital Model of the Buchla Lowpass-Gate*** (DAFx-13)
[PDF](https://dafx.de/paper-archive/2013/papers/44.dafx2013_submission_56.pdf) — the vactrol, which
is the actual subject ([§2.8](02-topologies.md), [§5.8](05-seminal-machines.md)). Its circuit and
12 ms/250 ms timing constants define the behavioural family model in the deep-dive; they are not a
measurement of the Music Easel's Cards 10/11.

<a id="buchla-208-lpg-schematics"></a>
**Buchla Model 208 Cards 10 and 11, 1973** —
[Historic Buchla 208 archive](https://fluxmonkey.com/historicBuchla/208-programsource.htm). Primary
schematic evidence for the Easel's two specific gate circuits, switched modes, optical control,
preamp and routing. These drawings, rather than the 292-derived behavioural constants above, are
the electrical boundary for a 208 reconstruction.

<a id="lanterman-208-lpg"></a>
**Aaron Lanterman — *Adaptation of the Low Pass Gate from the Music Easel***
[lantertronics.blogspot.com](http://lantertronics.blogspot.com/2019/10/adaptation-of-low-pass-gate-from-music.html) —
a legible circuit-level cross-check for the Card 10/11 mode connections, four off-board 120 kΩ
resistors, and the disputed 50 kΩ schematic annotation versus a recommended 10 kΩ linear slider.
It is an adaptation, so its substitutions are not attributed to the original 208.

**Werner et al. — *Virtual Analog Modeling of Audio Circuitry Using Wave Digital Filters***
(Stanford PhD) [purl.stanford.edu/jy057cz8322](https://purl.stanford.edu/jy057cz8322) ·
[DAFx-19 WDF tutorial](https://dafx2019.bcu.ac.uk/programme/mon/tutorial-werner) ·
[chowdsp_wdf](https://arxiv.org/pdf/2210.12554)

The general-purpose alternative to hand-derived TPT: model the circuit, not the block diagram. More
faithful, much more expensive, and overkill for a VCF you are designing rather than reproducing.
Worth knowing exists. The nodal DK method is the other member of this family.

**Improving the Chamberlin Digital State Variable Filter**
[arxiv 2111.05592](https://arxiv.org/pdf/2111.05592) — if you want the cheap old structure but
better behaved.

---

## Circuits and instruments

- **Electric Druid — Roland filter designs with the IR3109/AS3109**
  [electricdruid.net](https://electricdruid.net/roland-filter-designs-with-the-ir3109-or-as3109/) —
  how Roland wired the same chip differently in each instrument.
- **Roland SH-2 Service Notes** (15 October 1979; [synfo.nl mirror](https://www.synfo.nl/servicemanuals/Roland/ROLAND_SH-2_SERVICE_NOTES.pdf))
  — page 7 is the VCF/VCA board: four BA662s with JFET buffers, the 1 k / 1 k input attenuators, the
  resonance path's 15 k → back-to-back diodes → 2.2 k → pot → 5.6 k, the cutoff summer's resistors,
  the thermistor; pages 9–10 the calibration targets (oscillation onset at 7–9 on the resonance
  scale, 20 kHz at the top, below 50 Hz at the bottom). Image-only scan; what defeated the reading
  is listed in `research:instruments/sh-2.md` §11. The primary source for `research:filters/machines/ba662-sh-2.md`.
- **AMSynths — All about the BA662 chip** [amsynths.co.uk](https://amsynths.co.uk/2018/01/07/all-about-the-ba662-chip/)
  — what the OTA is, the nine colour-dot transconductance grades and why a filter needs four from
  one band, the BA662A's lower offset, which instruments used it and why the IR3109 replaced it.
- **AMSynths — AM8102 SH02 VCF** [amsynths.co.uk](https://amsynths.co.uk/2022/03/26/am8102-sh02-vcf/)
  — from building a clone of the SH-2's filter: the capacitor-value discrepancy in the service
  notes, self-oscillation from 10 Hz to 20 kHz, "no Q compensation" — and the level claim that
  conflicts with a plain four-pole's droop, recorded in the deep-dive's §9.
- **AMSynths — All about the IR3109** [amsynths.co.uk](https://amsynths.co.uk/2022/04/06/all-about-the-ir3109-chip/)
- **SH-101 lowpass filter thread**, Gearspace
  [thread](https://gearspace.com/threads/sh-101-low-pass-filter-thoughts.697847/) — the back-to-back
  diodes in the SH-101's feedback loop ([§5.2](05-seminal-machines.md)).
- **Oberheim Matrix-12 owner's manual, "More About Filter Poles"**
  [ManualsLib p.114](https://www.manualslib.com/manual/803278/Oberheim-Matrix-12.html?page=114) —
  the 15 modes described by the people who built them. Also
  [MATRIXSYNTH's list](https://www.matrixsynth.com/2016/07/15-filter-types-of-oberheim-matrix.html).
- **Old Crow's CS-80 tour** [cs80.com/tour.html](https://www.cs80.com/tour.html) and the
  [MOD WIGGLER CS-80 filter thread](https://modwiggler.com/forum/viewtopic.php?t=40363) — the
  damping-based, non-self-oscillating, frequency-dependent Q ([§5.7](05-seminal-machines.md)).
- **Reverb — *A Guide to Synth Filter Types***
  [reverb.com/news](https://reverb.com/news/a-guide-to-synth-filter-types-ladders-steiner-parkers-and-more) —
  readable overview of the families and their instruments.
- **Eddy Bergman's build series** [eddybergman.com](https://www.eddybergman.com/) — schematics and
  breadboards for the TB-303 VCF, the MS-20 filter, the Steiner-Parker and the Yamaha CS filter.
  Useful when you want to know what is actually in the box.
- **R. A. Moog, US Patent 3,475,623** (1969) — the transistor ladder patent that pushed everyone
  else toward diode ladders, Sallen–Key and state-variable designs.

---

## Efficiency

- **Short Note on Costs of Floating Point Operations on current x86-64 Architectures**
  [arxiv 1506.03997](https://arxiv.org/pdf/1506.03997) — actual measured denormal, underflow and
  division costs.
- **EarLevel Engineering — Floating point denormals**
  [earlevel.com](https://www.earlevel.com/main/2019/04/19/floating-point-denormals/) and
  [the digital state variable filter](https://www.earlevel.com/main/2003/03/02/the-digital-state-variable-filter/).
- **Modern synth architecture: SIMD + multithreading**, KVR
  [t=586455](https://www.kvraudio.com/forum/viewtopic.php?t=586455) — voice-parallel SIMD in
  practice, including why gather/scatter usually is not worth it.
- **hiir** (Laurent de Soras) — the reference polyphase IIR oversampling library; its coefficient
  designer is the right way to generate halfband coefficients rather than copying them.

---

## Reference implementations worth reading

- [ddiakopoulos/MoogLadders](https://github.com/ddiakopoulos/MoogLadders) — Stilson, Huovilainen,
  Simplified, Improved, Krajeski, Microtracker, Oberheim, RK simulation, all in one repo. The single
  best way to hear the difference between models.
- [FredAntonCorvest/Common-DSP](https://github.com/FredAntonCorvest/Common-DSP) — Simper's SVF.
- [Faust `vaeffects` library](https://faustlibraries.grame.fr/libs/vaeffects/) and
  [`aanl`](https://faustlibraries.grame.fr/libs/aanl/) — VA filters and antialiased nonlinearities,
  readable as specifications even if you never run Faust.
- [Csound `diode_ladder`](https://csound.com/docs/manual/diode_ladder.html) — Pirkle's diode ladder,
  documented and shipping.
- [musicdsp.org filter archive](https://www.musicdsp.org/en/latest/Filters/) — historical, often
  wrong, occasionally exactly what you need. Verify anything you take from it.

---

## Where the field is going

Neural and differentiable methods are now credible for *reproducing a specific unit* you have
measured, rather than designing one:

- [Comparative Study of State-based Neural Networks for Virtual Analog Audio Effects Modeling](https://arxiv.org/abs/2405.04124)
  (2024) — state-space models and linear recurrent units vs LSTM.
- [Differentiable Black-box and Gray-box Modeling of Nonlinear Audio Effects](https://arxiv.org/pdf/2502.14405)
  (2025).
- State Trajectory Networks — an MLP as a state-space model, trained on internal node voltages.

For a plugin you are *designing*, these are the wrong tool: they cannot be reasoned about, cannot be
proven bounded, are expensive per sample, and give you no knobs that mean anything. They are the
right tool when you have a specific piece of hardware on the bench and a deadline.

---

[← Rust recipes](07-rust-recipes.md) · [index](README.md)
