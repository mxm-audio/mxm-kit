# 8. Sources

Annotated, in the order they are useful rather than alphabetically. "Primary" means the paper or
document is the origin of the technique or the measurement; "secondary" means someone else's
account of a circuit, which is how most hardware information in `research:oscillators/05-machines.md`
had to be obtained.

---

## Papers

### <a id="jasa2012"></a>Välimäki, Pekonen and Nam — "Perceptually informed synthesis of bandlimited classical waveforms using integrated polynomial interpolation"

*Journal of the Acoustical Society of America* 131(1), pp. 974–986, January 2012.
[PDF](https://mac.kaist.ac.kr/pubs/ValimakiPeknenNam-jasa2012.pdf) ·
[publisher](https://pubs.aip.org/asa/jasa/article-abstract/131/1/974/822958/)

**The single most useful paper in this reference, and the one to read first.** It derives the
PolyBLEP family properly — the ideal BLEP is the sine integral, and integrated Lagrange or B-spline
interpolation approximates it — and gives the residual polynomials for orders 1 through 4 in tables
you can transcribe directly.

Use it for:

- **Table VIII**, the highest perceptually alias-free fundamental per method, quoted in full in
  [01-fundamentals.md §1.4](01-fundamentals.md#14-audibility-why-the-energy-number-is-the-wrong-number).
  It is the only comparison that answers "is this good enough" rather than "is this better".
- **Table VII**, the third-order B-spline residual, which is the four-point PolyBLEP implemented in
  our spike.
- **Table IV**, coefficients for a two-tap equaliser that undoes the high-frequency droop. Unverified
  by us; see [07-rust-recipes.md §7.3.4](07-rust-recipes.md#734-consider-a-post-oscillator-equaliser-if-brightness-is-ever-the-complaint).
- The finding that **B-spline beats Lagrange at equal order**, and that **even orders are
  preferable** because odd ones need extra control logic and cost as much as the next even one.

Also worth reading for its methodology: it explains why it uses NMR, and why it dropped PEAQ after
getting results it could not defend.

### <a id="spm2007"></a>Välimäki and Huovilainen — "Antialiasing oscillators in subtractive synthesis"

*IEEE Signal Processing Magazine* 24(2), pp. 116–125, 2007.
[Aalto record](https://research.aalto.fi/en/publications/antialiasing-oscillators-in-subtractive-synthesis/)

The survey, and where the name *PolyBLEP* is introduced — as a variation of BLEP that needs no
table because the residual has a closed-form polynomial. Sorts the field into bandlimited,
quasi-bandlimited and alias-reducing methods. Read it for orientation; read the JASA paper above
for the details.

### <a id="dpw"></a>Välimäki — "Discrete-time synthesis of the sawtooth waveform with reduced aliasing"

*IEEE Signal Processing Letters* 12(3), pp. 214–217, March 2005.
[IEEE](https://ieeexplore.ieee.org/document/1395943/)

The DPW method: sample a parabola, difference it, scale. Reports a 10 dB SNR improvement over
trivial sampling for the basic version and 15 dB for an oversampled-and-decimated variant. Our
independent measurement of DPW2 puts it 10.2 dB below the trivial waveform at 55.9 Hz, which is a
satisfying agreement given the different metrics.

### <a id="eptr"></a>Ambrits and Bank — "Improved polynomial transition regions algorithm for alias-suppressed signal synthesis"

*SMC 2013*. [PDF](https://home.mit.bme.hu/~bank/publist/smc13.pdf)

Derives EPTR, removes PTR's half-sample offset, and claims roughly 30% fewer operations than PTR
for identical output. The derivation is short and worth doing yourself; both branches collapse to
`(p₀ ∓ 1)(T − 1)/T`.

Two things we took from it beyond the algorithm: the observation that an `N`th-order DPW differs
from the trivial waveform in only `N−1` samples per period, which is the insight the whole PTR
family rests on; and the note that an asymmetric triangle with a one-sample transition can safely
replace a sawtooth, costing slight high-frequency attenuation and *reducing* aliasing — which is a
cheap way to model a finite reset.

Our measurement disagrees with its performance claim on modern hardware, for reasons the paper is
not responsible for: see [02-antialiasing.md §2.7](02-antialiasing.md#27-dpw-ptr-and-eptr).

### <a id="ptr"></a>Kleimola and Välimäki — "Reducing aliasing from synthetic audio signals using polynomial transition regions"

*IEEE Signal Processing Letters* 19(2), pp. 67–70, February 2012.

The PTR method itself. Cited here through the EPTR paper's account of it; the original was not
obtained in full.

### <a id="blamp"></a>Esqueda, Välimäki and Bilbao — "Rounding corners with BLAMP"

*DAFx-16*, Brno. [PDF](https://www.dafx.de/paper-archive/2016/dafxpapers/18-DAFx-16_paper_33-PN.pdf)

The band-limited *ramp*, for discontinuities in the first derivative rather than the signal:
triangle waves, hard clipping, half- and full-wave rectification. Reports up to 50 dB of alias
reduction and about 20 dB of SNR improvement, and that it beats oversampling for the same job.

Its Table 1 gives the four-point B-spline basis, its first integral (the four-point polyBLEP) and
its second (the polyBLAMP). We used the polyBLEP row as an independent check on the JASA paper's
Table VII — they agree exactly, which is worth knowing because both print coefficients whose signs
are easy to transcribe wrongly.

### <a id="minblep"></a>Brandt — "Hard sync without aliasing"

*ICMC 2001*, Havana. [PDF](http://www.cs.cmu.edu/~eli/papers/icmc01-hardsync.pdf)

Introduces minBLEP: the minimum-phase band-limited step, obtained by integrating a minimum-phase
windowed sinc, so the correction is causal and needs no lookahead. Written around hard sync, which
is the case that forces the issue, and still the standard reference for it. Also a clear, short
account of why naive sync sounds the way it does.

### <a id="blit"></a>Stilson and Smith — "Alias-free digital synthesis of classic analog waveforms"

*ICMC 1996*. The BLIT method: band-limit the derivative, then integrate. Cited here through the
later surveys rather than read in full. Historically the start of the modern line of work, and
superseded in practice by BLEP for the reasons in
[02-antialiasing.md §2.3](02-antialiasing.md#23-blit-band-limited-impulse-train).

### <a id="moogsaw"></a>Pekonen, Lazzarini, Timoney, Kleimola and Välimäki — "Discrete-time modelling of the Moog sawtooth oscillator waveform"

*EURASIP Journal on Advances in Signal Processing*, 2011.
[PDF](https://mural.maynoothuniversity.ie/id/eprint/4185/1/VL_Discrete-time.pdf)

**The only primary measurement of real analog oscillator waveforms used in this reference.** 47
recorded sawtooths from 86 Hz to 8.3 kHz, a phase-distortion model fitted to them, and the finding
that the rising ramp resembles a sinusoid more than a line. Also gives a cheaper alternative — a
first-order IIR post-equaliser matching the analog spectral envelope — which composes with any
antialiasing method.

Incidentally the clearest statement of the fractional-delay convention that the whole PolyBLEP
family depends on: `d` is the delay from the discontinuity to the sample following it.

### <a id="wt101"></a>Bristow-Johnson — "Wavetable Synthesis 101, A Fundamental Perspective"

*AES Convention 101*, paper 4400, November 1996.
[PDF](https://www.musicdsp.org/en/latest/_downloads/e2efef95ecbb4921981cc2f7658569d3/Wavetable-101.pdf)

The wavetable reference. Mostly about extracting tables from recorded tones, which is not what
[09-wavetable.md](09-wavetable.md) measures, but two of its points are load-bearing there:

- **Interpolation order trades against table length.** "If linear interpolation (or worse yet,
  drop-sample interpolation) is used, a larger wavetable is required to restrain interpolation
  error than if a more legitimate method of fractional sample interpolation is being used." Our
  measurements put a number on the exchange rate: 12, 24 and 36 dB per quadrupling.
- **Adjacent tables must be phase-locked** before they are crossfaded, or "an unintended null will
  occur when the crossfade is half complete". This one cannot be fixed at playback time.

### <a id="microsound"></a>Roads — *Microsound*

MIT Press, 2001. The standard text on granular and sub-granular synthesis, and the source for the
taxonomy — pulsar, glisson, trainlet, wavelet — used in [10-granular.md](10-granular.md). The claim
this reference leans on is the qualitative one: the grain envelope is the key to all granular
techniques, and a sharper attack produces a broader bandwidth. Cited from secondary summaries; the
book itself was not consulted directly.

### <a id="fof"></a>Rodet — "Time-Domain Formant-Wave-Function Synthesis"

*Computer Music Journal* 8(3), pp. 9–14, 1984, with Rodet, Potard and Barrière, "The CHANT Project"
in the same issue. [CHANT paper PDF](https://www.ee.columbia.edu/~dpwe/papers/RodetPB84-CHANT.pdf)

FOF: a periodic train of short excitations, each a sinusoid under an exponentially decaying
envelope, overlap-added. Each burst contributes one formant to the spectrum. The direct ancestor of
"granular synthesis used as an oscillator", and the reason [10-granular.md §10.7](10-granular.md#107-granular-as-an-oscillator-specifically)
treats grain rate as pitch and grain contents as formant.

### <a id="fofosc"></a>Olsen, Smith and Abel — "A hybrid filter–wavetable oscillator technique for formant-wave-function synthesis"

*SMC 2016*. [PDF](https://ccrma.stanford.edu/~mjolsen/pdfs/smc2016_MOlsenFOF.pdf)

A modern FOF implementation using a second-order filter plus a wavetable oscillator per burst,
triggered by an impulse train. Useful here for two asides rather than its main proposal: that
time-domain FOF needs an overlap-add scheme and an arbitrary cutoff point for each exponentially
decaying burst, and that earlier resonant-filter vocal synthesis was "prone to audible artifacts
being present due to the sharp discontinuity at the start of the exponential decay" — which is the
mechanism [10-granular.md §10.2](10-granular.md#102-the-envelope-is-the-antialiasing-decision)
measures at 25 dB.

### <a id="vosim"></a>Kaegi and Tempelaars — "VOSIM — A New Sound Synthesis System"

*Journal of the Audio Engineering Society* 26(6), pp. 419–425, June 1978.
[PDF](https://kaegi.nl/werner/userfiles/downloads/vosim-system.pdf)

Repeating tone bursts of variable pulse duration and variable delay, built from sine-squared
pulses. The other independent invention of pulsar synthesis, from the speech side.

### <a id="mq"></a>McAulay and Quatieri — "Speech Analysis/Synthesis Based on a Sinusoidal Representation"

*IEEE Transactions on Acoustics, Speech and Signal Processing* 34(4), 1986.

The origin of sinusoidal modelling and of birth-death partial tracking: how to follow partials that
appear, vanish and drift between analysis frames. The half of resynthesis that
[11-additive-resynthesis.md](11-additive-resynthesis.md) explicitly does not model.

### <a id="sms"></a>Serra and Smith — "Spectral Modeling Synthesis"

*Computer Music Journal* 14(4), 1990. Sound as deterministic partials **plus** a stochastic
residual. The standard answer to the question that sinks naive additive resynthesis: what do you do
with the part of the sound that is not sinusoids? See
[11-additive-resynthesis.md §11.7](11-additive-resynthesis.md#117-where-additive-resynthesis-fails).

### <a id="ifft"></a>Rodet and Depalle — "Spectral Envelopes and Inverse FFT Synthesis"

*AES 93rd Convention*, 1992. [AES record](https://www.aes.org/e-lib/browse.cfm?elib=6740)

Additive synthesis by writing each partial as a kernel into a spectrum and taking one inverse FFT
per block, rather than running an oscillator per partial. Reported cost reduction "on the order of
15". Not implemented or verified here; the inference about how Harmor affords its partial counts
rests on this figure.

### <a id="pvoc"></a>Laroche and Dolson — "Improved Phase Vocoder Time-Scale Modification of Audio"

*IEEE Transactions on Speech and Audio Processing* 7(3), pp. 323–332, May 1999.
[PDF](https://citeseerx.ist.psu.edu/document?repid=rep1&type=pdf&doi=8312d42cab3f14152d8e6406a9c0463737b6aa45)

Identity phase locking, and the clearest statement of what "phasiness" is and where it comes from.
The relevant contrast for [11-additive-resynthesis.md §11.4](11-additive-resynthesis.md#114-why-it-stretches-so-cleanly):
this is the sophisticated version of the stitching approach, and our overlap-add baseline is
deliberately the naive one.

### <a id="chowning"></a>Chowning — "The Synthesis of Complex Audio Spectra by Means of Frequency Modulation"

*Journal of the Audio Engineering Society* 21(7), pp. 526–534, September 1973.
[PDF](https://web.uvic.ca/~aschloss/course_mat/MU307/MUS307_MATERIALS/Chowning_FM.pdf)

The paper that started it. Sidebands at `fc ± k·fm` with Bessel amplitudes `J_k(I)`, bandwidth
growing with index as energy is "stolen" from the carrier, and the carrier-to-modulator ratio
deciding harmonicity.

The part most often skipped and most worth reading is the treatment of reflected components:
sidebands that fall below zero "reflect around 0 Hz and 'mix' with the components in the positive
domain", and because odd-order Bessel coefficients are negative they can subtract rather than add.
[12-fm.md §12.1](12-fm.md#121-what-the-knob-does) leans on this.

### <a id="dx7"></a>Shirriff — Yamaha DX7 reverse-engineering series

[righto.com, 2021](http://www.righto.com/2021/12/yamaha-dx7-reverse-engineering-part-iii.html)

Die-photo-level reverse engineering of the DX7's sound chips. The relevant detail for
[12-fm.md §12.7](12-fm.md#127-cost) is the log-sine ROM: 1024 14-bit values covering a quarter
cycle, delta-encoded into 5344 bits, stored as *negated logarithms* so that applying an envelope is
an addition rather than a multiplication, with a pair of exponential ROMs converting back. A good
antidote to assuming the classic instruments implemented the textbook algorithm.

### <a id="modfm"></a>Lazzarini and Timoney — "Theory and Practice of Modified Frequency Modulation Synthesis"

*Journal of the Audio Engineering Society* 58(6), pp. 459–471, 2010.
[PDF](https://mural.maynoothuniversity.ie/id/eprint/4697/1/JAES_V58_6_PG459hirez.pdf)

ModFM: replace the Bessel functions with *modified* Bessel functions, which lack the oscillating
factor and are always non-negative. The practical payoff is a spectrum that evolves smoothly with
the index instead of dropping partials where `J_k(I)` crosses zero. Also covers a phase-synchronous
variant for resonant and formant synthesis. Cited, not implemented.

### <a id="vps"></a>Kleimola, Lazzarini, Välimäki and Timoney — "Vector Phaseshaping Synthesis"

*DAFx-11*. [PDF](https://mural.maynoothuniversity.ie/id/eprint/4096/1/vps_dafx11.pdf)

The paper to read for phase distortion, even though its subject is the generalisation. §2 states
classic PD cleanly — a piecewise-linear phase-shaping function with one breakpoint at `(d, 0.5)`
applied to a trivial sawtooth phase — which is the formulation
[13-phase-distortion.md](13-phase-distortion.md) uses.

Three things taken from it beyond the formulation: that PD "can be seen as a type of complex-wave
phase modulation"; that phaseshaping is better behaved than waveshaping, because "the use of
non-smooth shaping functions does not necessarily imply the presence of audible aliasing (which is
more or less inevitable in waveshaping)" and no gain scaling is needed; and the VPS extension
itself, where the breakpoint becomes a two-dimensional vector, `v` beyond 1 produces formants, and
non-integral `2v − 1` produces aliasing from the incomplete final cycle — with a suppression
algorithm that renders the incomplete segments as whole half- or full-cycle sinusoids.

### <a id="adaptivepd"></a>Lazzarini, Timoney, Pekonen and Välimäki — "Adaptive Phase Distortion Synthesis"

*DAFx-09*. [PDF](https://www.dafx.de/paper-archive/2009/papers/paper_12.pdf)

Phase distortion applied to arbitrary input signals rather than a stored cosine, implemented with a
coefficient-modulated first-order allpass filter. PD as an effect rather than an oscillator. Cited,
not implemented.

### <a id="casiopd"></a>Casio phase distortion, secondary accounts

[Wikipedia: phase distortion synthesis](https://en.wikipedia.org/wiki/Phase_distortion_synthesis)
and [Casio CZ synthesizers](https://en.wikipedia.org/wiki/Casio_CZ_synthesizers).

Secondary, and the only accessible description of what the CZ hardware actually did: phase
transforms "assembled from piecewise linear functions under binary logic control" with "sharp knees
(and for some transforms, even sudden jumps)"; DCW as the degree of distortion; eight waveforms
including three resonant ones; and the resonant shapes built from "sine waves at the resonant
frequency, synchronised and windowed at the fundamental frequency", with the patent figure showing
a resonance counter hard-reset by the base counter.

The article's own note that "some aliasing is still present due to discontinuities in the function's
derivatives" is what
[13-phase-distortion.md §13.2](13-phase-distortion.md#132-the-trick-there-is-no-discontinuity)
takes apart and measures.

## Hardware documentation

### <a id="buchla-easel-directive"></a>Allen Strange — *Programming and Metaprogramming in the Electro-Organism*

[2013 second edition PDF](https://synthfool.com/docs/Buchla/PaMtEO.pdf) of the 1974 Music
Easel operating directive. Near-primary evidence for the original oscillator controls, nominal
55–1760 Hz complex-oscillator range, 0.17–55 Hz modulation-oscillator low range, separate waveform
crossfade and timbre functions, and AM/FM/balanced modes. The revision warning matters: later
208e/218e patch-chart artwork was laid over the older text, so the diagrams are not one untouched
1974 panel record.

### <a id="buchla-208-schematics"></a>Historic Buchla Model 208 page and 1973 schematics

[Fluxmonkey archive](https://fluxmonkey.com/historicBuchla/208-programsource.htm). The primary
circuit evidence for Cards 5–9 and 11: modulator, modulation oscillator, the three complex-oscillator
cards, and the AC-coupled Gate 2 monitor route. Use the dated drawings for topology and components;
do not infer one universal production revision from their circulation.

### <a id="brown-buchla-208-original"></a>Dave Brown — original 1973/74 Model 208 examination

[ModularSynthesis](https://modularsynthesis.com/buchla/208/buchla_208.htm). Card photographs,
scope traces and measurements from one restored original. The source for the approximately
38–3500 Hz complex-oscillator and 0.13–70 Hz modulation-oscillator ranges, the negative narrow
pulse, low-rate waveform deformation, and the dual-core interpretation of the early timbre path.
These are one-unit measurements, not guaranteed endpoints for every 208.

### <a id="brown-buchla-208v2"></a>Dave Brown — 208 V2 circuit, build and repair notes

[ModularSynthesis](https://modularsynthesis.com/roman/buchla208v2/208spss.htm). Detailed card-level
tracing of the two complex-oscillator cores, tracking loop, Card 5 balance procedure, Card 11
coupling values, calibration and common repair modifications. Strong circuit-reading evidence, but
the subject is a reconstruction; every modification must stay labelled rather than becoming an
original-hardware default.

### <a id="verbos-easel-oscillator"></a>Mark Verbos — “Music Easel Oscillator”

[Buchla Tech](http://buchlatech.blogspot.com/2008/10/music-easel-oscillator.html). Specialist
account of the waveform crossfade, timbre function and oscillator lineage. Its early/later revision
description conflicts with the terminology used for Brown's examined card set; that conflict is
preserved in `research:oscillators/buchla-208-oscillators.md`, not silently resolved.

### <a id="lanterman-easel-balanced-modulator"></a>Aaron Lanterman — Music Easel balanced-modulator adaptation

[Georgia Tech](https://lanterman.ece.gatech.edu/buchla/boards/mea_balmod/). A traced adaptation of
Card 5 that makes the dry-to-balanced-product crossfade and related vactrol-controlled FM path
legible. Use it to understand signal decomposition; adaptation substitutions are not evidence of
the original component population.

### Roland SH-101 Service Manual, November 1982

[Internet Archive](https://archive.org/stream/roland_Roland_SH101_Service_Manual/Roland_SH101_Service_Manual_djvu.txt)

**Primary.** The parts list settles what the SH-101's oscillator actually is (IC13 = CEM3340), what
the sub is built from (IC17 = MB84013B dual D flip-flop), and what the rest of the voice uses
(IR3109, BA662A). The specification sheet gives the PWM range as 50% → 0%, the LFO range as
0.1–30 Hz, tune as ±50 cents, and the envelope times. The adjustment procedures show that pitch is
set through a D/A converter with its own tune, width and linearity trims.

Read the parts list and the specification sheet; the rest is service procedure.

### CEM3340 documentation

Secondary, and the place where sources conflict:

- [Electric Druid — CEM3340 VCO designs](https://electricdruid.net/cem3340-vco-voltage-controlled-oscillator-designs/) —
  the most practical account: output levels per waveform, the PWM comparator, the high-frequency
  tracking pin, supply constraints, and why designers added hysteresis to the pulse comparator.
  Describes a ramp core with the triangle derived.
- [Synth DIY Wiki — CEM3340](https://sdiy.info/wiki/CEM3340) — feature summary and history.
- xtal-flux's "3340 VCO operation" — describes a *triangle* core with saw and square derived. Was
  unreachable at the time of writing and is cited from search-result excerpts only, which is why
  `research:oscillators/05-machines.md` §5.2 records the conflict rather than
  resolving it.

### AMSynths AM8110 build notes

[amsynths.co.uk](https://amsynths.co.uk/home/products/oscillators/am8110-sh-101-vco/)

Secondary, and specifically a *recreation* rather than the original — its own core is a CEM3340 or
AS3340. Useful for one detail stated about the original arrangement: the sub-oscillator flip-flops
are clocked from the buffered and level-shifted **sawtooth**, not from the pulse output.

### Roland Juno DCO analyses

Secondary, three independent accounts that agree on every point used here:
[Electric Druid](https://electricdruid.net/roland-juno-dcos/),
[Thea Flowers' DCO write-up](https://blog.thea.codes/the-design-of-the-juno-dco/), and
[Sequence 15's Juno-106 circuit analysis](http://sequence15.blogspot.com/2008/02/analyzing-juno-106-dco-circuit.html).
Between them: crystal-clocked countdown timer triggering a capacitor discharge, D flip-flop
sub-octave, pulse waveshaped from the sawtooth.

### TB-303 oscillator

Secondary and least verified of anything cited here:
[tinyloops' TB-303 VCO page](https://www.tinyloops.com/tb303/sound_vco.html) and a
[modular synthesis textbook chapter](https://olney.ai/ct-modular-book/tb-303.html). The
pitch-dependent duty cycle — about 45% at high pitches, about 71% at the lowest — comes from the
former and has not been independently checked.

### Harmor documentation

- [Image-Line's Harmor manual page](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/plugins/Harmor.htm)
  — vendor documentation, so authoritative on *what the controls do* and silent on how. The source
  for the 516-partials-per-unison-voice figure, the three per-partial datasets, the gain and pitch
  planes, harmonic phase mapping, prism, blur, the harmonizer, and the CPU notes.
- Malmgren, *The Harmor Synths Creation Manual* (2017), [PDF](https://www.malmgren.nl/Harmor.pdf) —
  third-party, and more forthcoming about the architecture than the official manual. The source for
  the resynthesis-versus-image-synthesis distinction, the one-way conversion, the "internal
  higher-resolution copy" description, and the stretching claim quoted in
  [11-additive-resynthesis.md §11.6](11-additive-resynthesis.md#116-harmor).

Neither is a description of the algorithm. Everything in §11.5 beyond what these two documents state
is labelled as inference where it appears.

### Sampler documentation

All secondary. No sampler was measured and no plugin was analysed.

- [TAL-Sampler product page](https://tal-software.com/products/tal-sampler) — vendor documentation.
  The source for the DAC mode list (Emu II, AM6070, S1000, Sample Hold, Linear, Clean), the "steep
  96 dB low-pass reconstruction filter (Emu II and AM6070 F)", the zero-delay-feedback filter
  claims, the AHDSR "vintage Digital / RC mode", and the architecture statement that the engine
  "really down-samples the sample to the desired sampling frequency, then processes the data
  depending on the chosen DAC and up-samples it to the desired pitch". Authoritative about what it
  does, silent about how.
- **AM6070 datasheet**, via [Electric Druid's copy](https://electricdruid.net/wp-content/uploads/2018/06/AM6070-uLaw-DAC.pdf)
  — a scanned PDF with no extractable text, so its figures here come from secondary summaries:
  8-bit companding to the µ-255 law, 72 dB output dynamic range, "12-bit accuracy and resolution
  around zero", monotonic across the range, 300 ns settling.
- **E-mu Emulator II** — 1984, 8-bit companding, 27.7 kHz, SSM2045 four-pole resonant analog
  filters after the converter. From general secondary sources; not verified against a service
  manual, unlike the SH-101 material in `research:oscillators/05-machines.md`.

The µ-law implementation measured in [14-samplers.md](14-samplers.md) is the **analytic** law, not
the piecewise-linear segmented approximation of G.711 that real companding hardware uses. The two
track closely; whether they differ audibly is untested.

### Granular instruments

All vendor documentation or press coverage, cited in
[15-granular-in-the-wild.md](15-granular-in-the-wild.md). Authoritative about feature sets, silent
about implementation — with one exception.

- [Robert Henke's Granulator page](https://roberthenke.com/technology/granulator3.html) — the
  designer's own documentation, and the most technically forthcoming of the set: grain counts per
  mode, the 2 ms–2 s size range, the grain-shape morph, per-grain parameter latching, the Scan ramp,
  and why the filter's key tracking defaults to 100%.
- [Mutable Instruments Clouds documentation](https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/)
  and its [open source page](https://pichenettes.github.io/mutable-instruments-documentation/modules/clouds/open_source/)
  — **the only instrument in that chapter whose firmware is public** (MIT). Position, size, pitch,
  density up to 16 grains, the texture control morphing the envelope "from sharp rectangular edges
  to a smooth bell curve", the diffusion network, and the four-in-one blend parameter.
- [Tasty Chips GR-1](https://www.tastychips.nl/product/gr-1-granular-synthesizer/) and
  [GR-MEGA](https://www.tastychips.nl/product/gr-mega-black/) product pages — the grain and voice
  specifications §15.2 and §15.3 quote: 128 grains per voice on both, 16 voices and "1000+ grains
  simultaneously" on the GR-1, and 20 voices per layer across four layers with a stated total of
  10 000 on the GR-MEGA, at 48 kHz with a 32-bit float mix. The GR-1 page is also where its quad-core
  ARM Cortex-A on Raspberry Pi architecture "running optimized Neon SIMD code" is stated. The
  GR-MEGA's 5 000/10 000 split by manufacturing date, and the report that its firmware is written in
  Rust, are **secondary coverage** rather than the vendor's own words.
- [Steinberg Padshop documentation](https://www.steinberg.help/r/padshop/2.3/en) — the grain and
  spectral oscillators, the statement that the spectral engine can "play any sample at any speed, at
  any position, in either direction and at any pitch", and that one grain oscillator generates **up
  to eight** grain streams with independent parameters. Confirmed in
  [Sound On Sound's Padshop 2 grain-oscillator workshop](https://www.soundonsound.com/techniques/cubase-padshop-2-grain-oscillator):
  "The Number setting can be adjusted to a maximum of eight."
- [Sound On Sound's Dawesome Novum review](https://www.soundonsound.com/reviews/dawesome-novum) —
  **secondary**, and the source for the six-layer figure in §15.2. Novum counts layers rather than
  grains, so the row is listed with that qualification rather than converted.
- [Arturia Pigments](https://www.arturia.com/products/software-instruments/pigments/overview) and
  [Audio Damage Quanta 2](https://www.audiodamage.com/products/ad055-quanta-2) — feature pages.
  Quanta 2's "up to 100 grains per voice" comes from its own page; Pigments exposes a grain **Limit**
  control whose maximum Arturia does not document, which is why §15.2 records an absence rather than
  a number.

## Within this repository

- [`../filters/README.md`](../filters/README.md) — the companion volume, and the model this one
  follows. Its chapter 3 on nonlinearity and oversampling is directly relevant: it measured that
  oversampling a nonlinear *filter* buys harmonic accuracy rather than alias reduction, which is
  the same lesson as [02-antialiasing.md §2.9](02-antialiasing.md#29-oversampling) reached from
  the other side.
- [`../../crates/dsp-lab/examples/osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs)
  — every in-repo DSP measurement in this reference.
- [`../../crates/mxm-mono-01-dsp/src/oscillator.rs`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/src/oscillator.rs) — the
  shipped implementation and its tests.
