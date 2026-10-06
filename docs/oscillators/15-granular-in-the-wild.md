# 15. Granular in the wild

[Chapter 10](10-granular.md) measured what a grain engine does. This chapter is about what people
have actually built with one: a survey of the granular instruments worth studying, what each exposes,
and where they genuinely differ.

The short version, which the rest of the chapter argues: **the parameter set has converged almost
completely, and the interesting differences are in three places** — how many grains the design is
willing to pay for, whether grains are scheduled by the note or by a clock of their own, and what
happens to the grains after they are made.

**Evidence standard.** Nothing here was measured, disassembled or reverse-engineered. Product claims
are from vendor documentation or, where noted, from secondary coverage, and they are authoritative
about *what* each does and mostly silent about *how*. The one exception is Clouds, whose firmware is
public. Measured figures are our own, from [chapter 10](10-granular.md) and from
[`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §14.

---

## 15.1 The parameter set everyone arrived at

Line the products up and the same seven controls appear under different names:

| What it does | Typical names |
|---|---|
| Where in the source the grain reads | position, scan, file position |
| How long the grain lasts | size, duration, length, grain size |
| How often grains fire | density, rate, number, grains |
| How the grain's level is shaped | shape, window, texture, envelope |
| How fast the grain reads its source | pitch, transpose, ratio |
| How much of all the above is randomised per grain | spray, variation, randomise, jitter |
| Where it lands in the stereo field | spread, pan, random pan |

That convergence is not fashion. Each one corresponds to a term in the same equation — a windowed
read from a buffer, scheduled at some rate — and [chapter 10](10-granular.md) measured what each is
worth. Position and size set the formant structure; density sets the level (as √density, §10.5); the
window sets the aliasing (78 dB between rectangular and Hann, §10.2); and the randomisation is what
keeps a synchronous train from sounding like a static tone.

What differs between products is not which knobs exist. It is the numbers behind them.

## 15.2 Grain budget is the design decision

The single most revealing spec, and the one nobody puts on the box:

| Instrument | Per voice | Voices | Total |
|---|---|---|---|
| Granulator III, Classic mode | **2 per stereo channel** | per note | — |
| Granulator III, Cloud mode | up to 20 per channel | per note | — |
| Steinberg Padshop 2 | **8** per grain oscillator | — | — |
| Mutable Instruments Clouds | — | — | up to 16 |
| Dawesome Novum (secondary) | **6 layers**, not grains | — | — |
| Audio Damage Quanta 2 | up to 100 | per voice | — |
| Tasty Chips GR-1 | 128 | 16 | **1 000+** |
| Tasty Chips GR-MEGA | 128 | 20 × 4 layers = 80 | **10 000** |

That is a range of about five thousand to one, for instruments that are all "granular". It is a
statement about what each thinks granular synthesis is *for*.

**The rows are not commensurable and that is part of the finding.** Each vendor picks a different
denominator — per voice, per channel, per note, per oscillator, or total — and Novum's six are
*layers*, a different architecture counted a different way, listed here because leaving it out would
imply a figure it does not publish. Most of the field publishes nothing at all: Pigments exposes a
grain "Limit" control without documenting its maximum, and Portal and Emergence give no number in
any vendor material found. **Where a spec is absent it is absent, not small.**

**Software and hardware are on different scales, and it is not the processor.** Every software
figure here is between 2 and 100; every hardware figure is between 1 000 and 10 000. Our own
measurement says a properly built stereo grain costs about 10 ns, so **one desktop core would run
Padshop's eight grains about two hundred and fifty times over**. These are design theses, not
budgets. The maximalists went to a box for the seven-inch screen and the knob per parameter, not
because a laptop could not do the arithmetic.

Henke's Classic mode with two grains is not a limitation, it is the thesis: two overlapping reads
crossfading through a sample, with everything expressive coming from how they are modulated. The
GR-MEGA's ten thousand is the opposite thesis — granular as a *cloud*, where individual grains are
not meant to be heard at all.

**Read the two Tasty Chips rows against each other and one of them is short.** 128 × 16 is 2 048, but
the GR-1's own specification says 128 per voice "which can add up to a total of 1000+ grains
simultaneously" — half its nominal ceiling, because the machine runs out of processor before it runs
out of voices. The GR-MEGA's 10 000 against 80 × 128 = 10 240 is not short. **The successor did not
buy more grains per voice; it bought the ability to actually reach them**, and that is the more
interesting thing a spec sheet has ever accidentally disclosed about a granular instrument.

Our measurements price both. At about **10 ns per live stereo grain**
([§10.6.1](10-granular.md#1061-a-sample-reading-grain-is-a-different-animal-and-it-need-not-be)) one
core of a current desktop, scalar and in safe Rust, runs **about 2 000 grains at 48 kHz** — twice the
GR-1's total, on one core rather than four, and a fifth of the GR-MEGA's. At the quarter-core a
plugin can honestly claim inside a DAW it is about 500.

**The GR-MEGA's number is reachable and it is not reachable scalar.** Ten thousand grains at 48 kHz
is 480 million grain-samples a second. On Pi-class silicon — four Cortex-A cores somewhere between
1.2 and 2 GHz, which is the GR-1's documented architecture and an assumption about its successor's —
that is **10 to 17 core-cycles per grain-sample with all four cores spent on nothing else**. Our
cheapest measured grain, a linear read from a mono source with a tabled window, uses about 30 cycles
on one x86 core scalar. Four-wide NEON closes that gap and a little more, and the GR-1's
specification says in as many words that it runs "optimized Neon SIMD code". **They are not doing a
different technique; they are vectorising it and we are not.** That is an extrapolation from our
numbers and their published ones, not a measurement of theirs.

So the honest position for a plugin: **GR-1 class is a scalar single-core problem and already
solved by the arithmetic; GR-MEGA class needs either a whole core and SIMD, or more cores than a
plugin should take from its host without being asked.** SIMD across eight grains to an AVX2 vector
is the route, and it costs `#![forbid(unsafe_code)]` in `mxm-grain-fx-dsp` or the zero-dependency
claim in both DSP manifests. Priced here, deliberately not taken.

## 15.3 Tasty Chips GR-1, GR-2, GR-MEGA

The hardware lineage, and the maximalist end of the design space.

The **GR-1** established the format: 16 voices, up to 128 grains per voice and 1 000+ in total, a big
screen showing the sample with the grain cloud scattered over it, and one knob per parameter. Its
interface argument is that granular synthesis is a *spatial* activity — you are pointing at a region
of a recording, not dialling a number. It is the one machine here whose processor is on the record:
a quad-core ARM Cortex-A on Raspberry Pi architecture "running optimized Neon SIMD code", with
32-bit float algorithms behind 16-bit converters at 106 dB SNR. Mono samples only.

The **GR-MEGA** extends that into a workstation. From vendor and press material: five engines —
stereo granular, granular *slice*, spectral synthesis by phase vocoding, tape scratch, and a
conventional sampler; four LFOs, four envelopes and four step-sequencer modulators into a matrix of
20 sources onto 100 destinations; four simultaneous effects; a 64-step, seven-note sequencer;
Eurorack gate and CV; multitimbral layers each with their own sequencer; and stereo sample support,
which the GR-1 lacked. Its published numbers: **128 grains per voice, 20 voices per layer across four
layers, 10 000 grains in total**, 48 kHz, a 32-bit float mix, and a phase vocoder limited to three
voices. Samples run to 60 minutes, or 5m50s in granular "because of polyphony". Secondary coverage
reports the firmware is written in Rust.

**The grain total is dated, and the date is the point.** Vendor material carries 5 000 for earlier
units and 10 000 "from October '25 onwards" — the same instrument, the same engine, twice the cloud,
which says the ceiling was the board rather than the design.

Three things are worth extracting.

**The grain budget is the headline spec and it is the one that moved.** Per-voice count did not
change between GR-1 and GR-MEGA — both say 128 — and
[§15.2](#152-grain-budget-is-the-design-decision) reads the totals to show why that is the less
interesting half.

**Granular and spectral shipped side by side**, exactly as they do in Padshop (§15.7). That is not
redundancy — [chapter 11](11-additive-resynthesis.md) explains why. A phase vocoder stretches tonal
material cleanly and smears transients; a grain cloud keeps transients as events and smears pitch.
An instrument that offers both is admitting that neither wins. The vocoder's three-voice limit
against granular's eighty is the cost difference between the two stated as a spec.

**The modulation count is the real feature.** 20 sources onto 100 destinations is what makes a grain
cloud sound alive rather than static, and [chapter 10](10-granular.md) says why: a synchronous train
with fixed parameters is a periodic waveform with a formant, and nothing more. All the motion comes
from modulating the scheduling.

## 15.4 Mutable Instruments Clouds — the one we can be sure about

Clouds is the Eurorack granular processor that defined the module category, and it is the only
instrument in this chapter whose internals are not guesswork: Emilie Gillet open-sourced the
hardware and firmware under an MIT licence, which is why the "Clouds derivative" is now its own
genre — Supercell, µClouds, Parasites, Superparasites.

Its published control set: **position, size and pitch** as the grain controls, **density** governing
how many grains are superimposed — up to 16 — independently of grain size, and **texture**, which
morphs the grain envelope "from sharp rectangular edges to a smooth bell curve", with a diffusion
network available to "dissolve the edges of grains into blurry textures".

That texture control is worth pausing on, because it is our
[§10.2](10-granular.md#102-the-envelope-is-the-antialiasing-decision) finding exposed as a knob. The
measured distance between a rectangular and a Hann window on a synchronous train is **78 dB of
aliasing**. Clouds puts that continuum on the front panel and calls it timbre — which it is: the
"gritty" end of the texture knob is, in significant part, folded aliasing, and the "smooth" end is
its absence. The module is honest about this in a way a spec sheet cannot be.

The **blend** parameter bundles four post-processing amounts — dry/wet, random panning, feedback and
reverb — onto one control. That is the third of §15's three differentiators in one knob: what
happens *after* the grains. A grain cloud with no diffusion sounds like a grain cloud; with reverb
and random panning it sounds like a place.

## 15.5 Robert Henke's Granulator III

The minimalist thesis, and the most carefully argued design of the set. From Henke's own
documentation:

**Three playback modes.** *Classic* — "based on Granulator II, and provides the most versatile and
expressive engine" — creates **two overlapping grains per stereo channel**. *Loop* is "much closer
to a 'normal' sampler with crossfading sample playback and random position modulation". *Cloud*
runs "up to 20 non-synced individual grains" for "thick monophonic textures".

**Grain size from 2 ms to 2 seconds**, and Henke calls it "the most prominent parameter" — at the
short end producing "noisy or high-pitched tonal timbres", at the long end simply playing the
sample. That range spans the whole of [§10.7](10-granular.md#107-granular-as-an-oscillator-specifically):
at 2 ms the grain rate *is* the pitch and the grain is a formant; at 2 seconds it is a sampler.

**Grain shape morphs across a family**, "from grains with a sharp attack and exponential decay to
smooth crossfading to exponential attack and abrupt end". Compare
[§10.2](10-granular.md#102-the-envelope-is-the-antialiasing-decision) and
[§13.5](13-phase-distortion.md#135-the-resonant-waveforms-are-grains): a sharp attack is a step,
an abrupt end is a step, and the smooth middle of that morph is the clean setting. The two ends are
the interesting-sounding ones for the same reason the CZ's resonant waveforms are.

**Parameters are latched per grain.** Henke: effects are calculated "at the beginning of each new
grain", which "allows quite complex rhythmical textures" from modest modulation. This is a real
implementation decision with an audible signature — modulation becomes quantised to grain onsets, so
an LFO on grain size produces stepped rather than continuous change. It is also, incidentally, the
right way to build it: per-grain latching is what lets a grain be rendered as a unit.

**Variation** randomises "the start position of *each individual grain*, its size, amplitude, or
pitch". Henke's justification is the honest one: "static looping timbres can become more alive".

**Scan** is "a ramp that starts when a new note is triggered", which is how time-stretch and
pitch-shift fall out of a grain engine: move the read position slowly while the grains keep firing
at the note rate.

**Two cascaded state-variable filters**, nine modes, and a detail worth stealing: the filter's key
tracking defaults to **100%**, "for this reason, it follows the spectrum of the sample when
transposed". A sampler's brightness moves with the transposition; a fixed filter cutoff fights it.

Plus MPE across slide, pressure and pitch bend; audio-rate LFO modes including a 1:1 ratio mode; and
real-time audio capture into the device.

## 15.6 Arturia Pigments

Granular as one engine among several, inside a synth that is otherwise conventional.

Pigments' sample/granular engine exposes grain **rate** (how often new grains are generated), grain
**envelope shape**, and grain **length** as its three primary controls, with position, pitch and the
usual randomisation underneath; samples are loaded from a browser or imported (WAV/AIFF, 16 or
24-bit, 44.1–192 kHz), with multiple slots for mapping.

The interesting thing about Pigments is not the grain engine, which is ordinary, but the context. It
sits in the same instrument as wavetable, virtual-analog and physical-modelling engines, all feeding
the same filters, effects and — critically — the same modulation system. That is a different bet
from Clouds or the GR-1: granular as *a colour available to a synth* rather than as the instrument's
identity. If the survey has a lesson for someone building one engine among many, it is that the
grain scheduler needs the same modulation depth as everything else or it will sound static next to
the wavetable engine.

## 15.7 Steinberg Padshop 2

The clearest statement of the granular-versus-spectral trade in a shipping product: Padshop 2 runs
**two co-existing oscillator types**, a grain oscillator and a spectral oscillator, and lets you use
either or both.

The grain oscillator is described in Steinberg's own terms as playing "only short portions of a
sample… in any order", with envelopes applied to the grains "to avoid discontinuities in the
playback and to minimize artifacts" — which is
[§10.2](10-granular.md#102-the-envelope-is-the-antialiasing-decision) stated as a design
requirement. Its parameters include grain **number**, **duration** with a key-follow amount, and
**position**. **Number tops out at eight** — one of the few published grain ceilings in software,
and the second-smallest in [§15.2](#152-grain-budget-is-the-design-decision) after Granulator III's
two. One oscillator generates up to eight grain streams with independent parameters, which is the
Classic-mode thesis with more voices rather than the cloud thesis with fewer.

The spectral oscillator "analyzes the spectrum of the loaded sample… the progression of the
frequencies, amplitudes, and phases from the sample start to the end" and can then "play any sample
at any speed, at any position, in either direction and at any pitch". That is exactly the parametric
representation of [chapter 11](11-additive-resynthesis.md), with the same consequence measured
there: time and pitch become independent parameters because the analysis holds them separately.
Its **purity** and **inharmonicity** controls operate on the analysed partials, which is the kind of
manipulation only a partial-domain representation allows — the same class of operation as Harmor's
prism and blur.

Putting both in one instrument is the correct engineering answer to a problem with no single winner.

## 15.8 The rest of the field, briefly

- **Audio Damage Quanta 2** — a dedicated granular synth with up to 100 grains per voice, two
  virtual-analog oscillators that can be granularised or passed through clean, dual multimode
  filters, four envelopes, MPE and MTS-ESP microtuning. The VA oscillators are the notable idea:
  granulating a *synthesised* source rather than a recording, which removes the sample-management
  problem entirely.
- **Output Portal** — granular as an *effect*, on whatever is already on the track, with a stated
  goal of results "more closely related to the original input" than granular processing usually is.
  A different product category from everything above, and the one most people meet first.
- **Make Noise Morphagene** — the Eurorack tape-and-grain hybrid, framed around splicing rather than
  clouds.
- **Dawesome Novum** — six *layers* rather than a grain count, every sample divided across them.
  A different architecture, and a reminder that "how many grains" is not always a question the
  design admits: what Novum publishes is how many independently manipulable copies of the source it
  keeps, not how many windowed reads are in the air.
- **Inear Display Emergence**, and a long tail of dedicated granular instruments. No published
  grain figure for any of them.
- **PaulStretch** — not granular at all, but always mentioned alongside: extreme time-stretching by
  spectral means, and the clearest demonstration of the failure mode
  [§11.7](11-additive-resynthesis.md#117-where-additive-resynthesis-fails) describes, since
  stretching noise by 50× is what turns it into the sound the program is famous for.

## 15.9 What our measurements say about their feature lists

Reading the survey back against [chapter 10](10-granular.md), five things stand out.

**The window control is an aliasing control.** Clouds' texture, Granulator's shape, Padshop's grain
envelope — all of them morph between a window with steps at its ends and one without. Measured
distance: 78 dB. When these instruments sound "gritty" at one end of that control, a large part of
what is being heard is folded aliasing, deliberately offered.

**Density is not a level control, but it is wired to one in the user's head.** RMS grows as
**√density** ([§10.5](10-granular.md#105-asynchronous-clouds-statistics-not-waveforms)), so sweeping
density from sparse to thick adds up to 20 dB. Every instrument here has that control and none of
them can avoid the physics; if you build one, compensate by `1/√density` or the texture sweep is
also a volume sweep.

**Grain count sets the cost and nothing else sets it — measured, not assumed.** Cost per live grain
is flat to within the run-to-run spread from 4 grains to 128
([§10.6.1](10-granular.md#1061-a-sample-reading-grain-is-a-different-animal-and-it-need-not-be)):
8.73 to 9.27 ns over a factor of 32. A 2-grain design and a 10 000-grain design are the same code
with a different number in it, and a factor of five thousand in CPU. What *is* negotiable is the
price of one grain, and that varies by 8x between the shipped engines in this repository alone.

**Fractional grain onsets are worth 71 dB** ([§10.3](10-granular.md#103-onset-placement-matters-more-than-everything-else)),
and no product documents whether it does this. It is the single largest quality difference available
in a grain engine and it is invisible from a feature list — which is as good an explanation as any
for why two granular plugins with identical parameter sets do not sound equally good.

**Grain pitch is sampler transposition, one grain at a time.** Every instrument in this chapter
offers per-grain pitch. Measured, on a synchronous train reading a recording that carries every
harmonic below 19.8 kHz:

| grain pitch ratio | linear interpolation | 4-point cubic |
|---|---|---|
| 0.25× | −158.6 dB | −158.5 dB |
| 0.5× | −86.2 dB | −115.7 dB |
| 1.0× | −89.2 dB | −118.9 dB |
| **2.0×** | **−22.7 dB** | **−22.7 dB** |
| **4.0×** | **−17.6 dB** | **−17.6 dB** |

The same cliff as [§14.5](14-samplers.md#145-transposition-the-problem-no-interpolator-can-solve),
to the same decimal — transposing a grain up folds the source's harmonics, the grain window does
nothing to hide it, and interpolation quality stops mattering entirely above unity. Below unity,
cubic is worth about 30 dB over linear.

So a granular instrument needs the same band-limiting machinery a sampler does, applied per grain.
An engine that pitches grains up without it is generating exactly the artefact that
[chapter 2](02-antialiasing.md) exists to prevent, once per grain, hundreds of times a second.

## 15.10 If you were building one

The survey and the measurements agree on a fairly specific starting design:

- **Pick a grain budget deliberately, and let it define the instrument.** Two grains and deep
  modulation is a coherent product; 128 grains and shallow modulation is not.
- **Fractional onsets, always.** 71 dB, free, invisible on a feature list.
- **Hann by default, with a morph toward hard-edged windows offered as timbre** — because that is
  what the successful products do, and because the morph is genuinely musical even though one end
  of it is aliasing.
- **Latch parameters per grain**, as Granulator III does. It makes grains renderable as units and
  turns modulation into rhythm.
- **Band-limit grain pitch**, per grain, using the sample mipmap of
  [§9.4](09-wavetable.md#94-bank-resolution-is-the-treble). This is the one place a granular engine
  can be straightforwardly better than its competitors. It is a quality argument and only a quality
  argument: playback rate itself measures free, 8.74 to 8.82 ns from 0.5x to 4x
  ([§10.6.1](10-granular.md#1061-a-sample-reading-grain-is-a-different-animal-and-it-need-not-be)),
  so the mipmap costs memory and buys the 22.7 dB above, and nothing else moves.
- **Table the interpolation kernel rather than computing it per read.** 6.8x, the largest number on
  the cost side of chapter 10, and the difference between 2 000 grains a core and 300.
- **Compensate density for level** by `1/√density`.
- **Spend at least as much on what comes after the grains as on the grains** — diffusion, random
  panning, filters with key tracking, reverb. Clouds puts four of them on one knob; Granulator III
  ships two cascaded SVFs with 100% key tracking by default.
- **If the goal is time-stretching rather than texture, build the spectral engine too.** Padshop and
  the GR-MEGA both ship both, and [chapter 11](11-additive-resynthesis.md) measured why neither wins
  alone.

## 15.11 Consequences for this repository

The collection has two granular engines: `mxm-grain-fx`, a processor on a live capture buffer, and
`mxm-creative-sampler`'s Grain reader over a stored sample. Both landed after this chapter was
written, and neither was written against it — which is most of why
[§10.6.2](10-granular.md#1062-what-the-collection-actually-pays) had two engines eight times apart per
grain to report.

[§15.10](#1510-if-you-were-building-one) is the spec, and reading both engines against it has now
closed two of its entries and left one:

- **Per-read kernels: closed.** The sampler's sixteen windowed-sinc coefficients are a 512-phase
  polyphase table at unity rate, which took its grain from 93 ns to 28.8 and its pool from 128 to
  256. Both engines table their window too.
- **§15.9's below-unity 30 dB: closed.** `mxm-grain-fx` interpolated linearly and now reads
  four-point, measured at 40 dB of error improvement at 750 Hz and 27 dB at 3 kHz for 2.64 ns per
  grain. The sampler already read sixteen tabled taps.
- **The mipmap: still outstanding, and it is the one that needs it.** Neither engine mipmaps its
  source, so both still pay §15.9's 22.7 dB at 2× and 17.6 at 4× — the source's own harmonics
  folding, which no interpolator repairs and only a band-limited source does.
  [10-granular.md §10.6.1](10-granular.md#1061-a-sample-reading-grain-is-a-different-animal-and-it-need-not-be)
  withdrew the cost argument against it; what is left is memory and a plan.

The grain-budget question this chapter exists to frame now has an answer for a plugin: **GR-1 class
on one scalar core, GR-MEGA class only with SIMD or more cores than a plugin should take.**
[§15.2](#152-grain-budget-is-the-design-decision) shows the arithmetic. Neither engine's pool is
anywhere near either figure today, and **neither is held there by the processor any more** — the
sampler's 256 grains cost 31.9% of a core and grain-fx's 64 cost 4%. The sampler's 256 are one
budget shared across voices rather than sixteen fixed pools, so a single held note can spend all of
them; that redistribution is also what took it from 35% to 31.9%.

---

Back to [README.md](README.md) · sources: [08-sources.md](08-sources.md)
