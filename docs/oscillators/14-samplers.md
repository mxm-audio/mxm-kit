# 14. Samplers, and what makes a vintage one sound like that

A sampler is the oscillator of [chapter 9](09-wavetable.md) with the constraint removed: the table is
a recording, arbitrarily long, and it is read at whatever rate the note demands. Everything in that
chapter about interpolation still applies. What this chapter is about is everything *else* — the
converter, the sample rate, the reconstruction — because that is where the character of the machines
people miss actually lives.

The occasion for writing it is TAL-Sampler, which sounds unusually good for a plugin that is, on
paper, a sample player. Its feature list explains why, and the mechanisms behind each item are
measurable. That is what follows.

**Scope, stated plainly:** TAL-Sampler was not analysed, disassembled or measured. Its feature
claims below are quoted from the vendor's own documentation, which is authoritative about *what*
it does and silent about *how*. Every number in this chapter is our own measurement of the
underlying mechanism, not of that plugin. Figures from
[`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §13.

---

## 14.1 What the plugin says it does

From TAL's own feature list, the parts that matter here:

- **"Vintage DAC modes (Emu II, AM6070, S1000, Sample Hold, Linear, Clean)"** — not a bit-depth
  control, a converter model.
- **"Steep 96 dB low-pass reconstruction filter (Emu II and AM6070 F)"** — the reconstruction stage
  offered as a separate choice from the converter.
- **"Variable sample rate."**
- The architecture claim, which is the important one: the engine *"really down-samples the sample to
  the desired sampling frequency, then processes the data depending on the chosen DAC and up-samples
  it to the desired pitch"* — as against "simplistic bit-crushing techniques".
- **"Self resonating zero feedback delay filter (24/12/6 dB LP, HP, BP, AP, Notch)"** — a ZDF/TPT
  topology, the family in [`../filters/`](../filters/README.md).
- **"Three AHDSR envelopes with vintage Digital / RC mode"** — envelope *curve* as a switch.

Two of those names are worth decoding before the measurements, because they are the whole point.

**AM6070** is not a bit depth. It is an 8-bit **companding** converter — a real chip, tested to the
µ-255 law, 18-pin DIP, quoted at 72 dB of output dynamic range with "12-bit accuracy and resolution
around zero" and monotonicity guaranteed across the range. Companding means the quantisation steps
are logarithmic: coarse at full scale, fine near silence.

**Emu II** is the E-mu Emulator II of 1984: 8-bit companding, **27.7 kHz** (a secondary figure — the
service manual does not state a sample rate), with SSM2045 four-pole resonant analog filters after
the converter.

**Corrected 2026-09-11:** this chapter previously said the Emulator II is "the machine the AM6070 is
famous from". Its service manual names the part as the **6072** — *"Each output channel consists of
an input latch, (74HCT374) a DAC, (6072) a VCF/VCA (SSM2045) and a switched capacitor filter
(S3528)"* (p. 2-17). The Am6072 and Am6070 are both AMD companding DACs and every character argument
below is unaffected, but the part number was wrong. See `research:instruments/emulator-ii.md` §3.

So the DAC list is not a list of bit depths. It is a list of *distortion characters*, and §14.2
shows they are not remotely the same thing.

## 14.2 Companding is why an 8-bit machine sounds good

Measured: a 440 Hz sine quantised by each converter, at five signal levels, reported as everything
that is not the wanted bin relative to it.

| level | 16-bit linear | 12-bit linear | 8-bit linear | 8-bit µ-law |
|---|---|---|---|---|
| 0 dB | −98.1 dB | −73.8 dB | −49.2 dB | −35.7 dB |
| −6 dB | −92.1 dB | −68.0 dB | −44.1 dB | −38.6 dB |
| −18 dB | −80.1 dB | −56.0 dB | −32.3 dB | −37.5 dB |
| −36 dB | −62.1 dB | −37.9 dB | −15.2 dB | −36.0 dB |
| −54 dB | −44.1 dB | −20.8 dB | *signal gone* | −26.4 dB |

The linear columns confirm the textbook `6.02·bits + 1.76` at full scale — 98.1 dB measured against
98.1 predicted for 16 bits, 49.2 against 49.9 for 8 — which is the check that says the measurement
is sound.

Now read across the rows.

**Linear quantisation degrades one-for-one with level.** Drop the signal 18 dB and the
signal-to-junk ratio drops 18 dB, because the step size is fixed and the signal is using fewer of
them. By −36 dB an 8-bit linear converter is at −15.2 dB — audibly destroyed. By −54 dB **the
signal has disappeared entirely**: the amplitude is smaller than one quantisation step, so every
sample rounds to zero and the output is digital silence. That is not a good number, it is the worst
possible one.

**Companding holds its ratio flat.** The µ-law column sits between −36 and −39 dB across a 30 dB
span of input level, and only starts to fall at −54 dB. It is *worse than 8-bit linear at full
scale* — −35.7 against −49.2 — and 21 dB better at −36 dB, and infinitely better below that.

That trade is the entire reason a 1984 8-bit sampler is remembered fondly while a bit-crusher plugin
set to 8 bits sounds like a toy. **Music spends most of its time well below full scale**, and it is
the decays, the reverb tails and the quiet passages that a linear converter turns to gravel and a
companding one keeps intact. The noise floor of a companding converter *follows the signal* — which
is also why it is not perceived as noise but as texture.

One number worth reconciling, because it trips people up: the AM6070's advertised **72 dB dynamic
range** is the ratio between the largest signal and the smallest resolvable step. Its
**signal-to-noise at any given level is about 36 dB**, which is what we measure. Both are true;
they answer different questions. Companding buys range, not resolution.

## 14.3 The grit is the missing anti-alias filter, not the bit depth

If bit depth is not what makes a vintage sampler sound vintage, what is? Measured: a 220 Hz
sawtooth carrying every harmonic below 19.8 kHz — a full-bandwidth signal arriving at the input —
sampled at each machine's rate, with and without a band-limiting filter in front of the converter.

| machine | rate | harmonics kept | with input filter | without |
|---|---|---|---|---|
| CD / Akai S1000 | 44100 Hz | 90 | −243.2 dB | −243.2 dB |
| **Emulator II** | **27700 Hz** | 56 | −241.8 dB | **−25.2 dB** |
| **SP-1200** | **26040 Hz** | 53 | −241.7 dB | **−24.6 dB** |
| Ensoniq-ish | 22050 Hz | 45 | −240.6 dB | −22.7 dB |
| low fidelity | 15000 Hz | 30 | −240.0 dB | −19.5 dB |

With a proper filter every rate is clean — a 27.7 kHz machine is simply a machine with less treble.
Without one, sampling that source at 27.7 kHz folds **−25 dB of inharmonic material** back into the
audio band.

**That is the vintage sound, and it is aliasing.** Not "warmth", not the converter, not the bit
depth: content that was above half the sampling rate, folded down to somewhere it does not belong.
And it is *program-dependent* — how much you get, and where it lands, depends entirely on what the
source had above the cutoff. That is why it sounds alive and reacts to the material, where a static
bit-crush sounds the same on everything.

It is also why TAL's architecture claim matters more than the DAC names. Down-sampling the sample to
the target rate — with a filter that is realistic rather than ideal — and *then* doing the converter
modelling at that rate, before resampling for pitch, reproduces this mechanism. Quantising to 8 bits
at 44.1 kHz does not, however good the quantiser is, because the folding never happens.

## 14.4 Reconstruction: what "Sample Hold" means

A converter updates its output once per sample and holds it. That zero-order hold has two
consequences, and the second one is much bigger than people expect.

Measured: a tone at 27.7 kHz sample rate, held and examined at four times that rate so the images
are visible.

| tone | images, bare hold | images, with a steep lowpass | in-band droop |
|---|---|---|---|
| 1385 Hz (0.05·fs) | −21.1 dB | −51.7 dB | −0.04 dB |
| 5540 Hz (0.2·fs) | −8.8 dB | −57.1 dB | −0.58 dB |
| 11082 Hz (0.4·fs) | **−1.6 dB** | −48.4 dB | −2.42 dB |

**The droop is the small problem.** `sinc(π f / fs)` costs 0.04 dB at the bottom of the band and
2.42 dB near the top — a gentle treble tilt, and arguably part of the charm.

**The images are the large one.** A bare sample-and-hold leaves a copy of the signal at `fs − f`,
and for a tone at 11 kHz on a 27.7 kHz machine that image sits at **16.7 kHz, only 1.6 dB below the
wanted tone** — inside the audible band, not politely above it. At 5.5 kHz the image is at 22.2 kHz
and 8.8 dB down. This is the entire reason a low-rate converter needs a reconstruction filter, and
the reason a "Sample Hold" mode is a *sound* rather than a defect: those images are extra top-octave
energy that tracks the signal.

The steep filter removes them — 48 to 57 dB down — which is exactly what TAL's separate "Emu II F"
and "AM6070 F" variants with the "steep 96 dB low-pass reconstruction filter" are offering. The
choice between the two is a choice between a machine as it was designed to work and a machine with
its output stage taken off.

## 14.5 Transposition: the problem no interpolator can solve

[Chapter 9 §9.2](09-wavetable.md#92-interpolation-is-the-alias-floor) measured interpolation quality
on a table whose harmonic content was chosen for the pitch. A sampler cannot do that: the recording
has whatever bandwidth it has, and pitching it up moves that content past Nyquist.

Measured: one period of a sawtooth recorded at 220 Hz carrying every harmonic below 19.8 kHz, played
back at other pitches.

| played | ratio | drop-sample | linear | 4-point cubic |
|---|---|---|---|---|
| 55.9 Hz | 0.25× | −49.7 dB | −89.2 dB | −118.9 dB |
| 110 Hz | 0.5× | −49.7 dB | −89.2 dB | −118.9 dB |
| 220 Hz | 1.0× | −49.7 dB | −89.2 dB | −118.9 dB |
| **440.8 Hz** | **2.0×** | **−22.7 dB** | **−22.7 dB** | **−22.7 dB** |
| **880.8 Hz** | **4.0×** | **−17.6 dB** | **−17.6 dB** | **−17.6 dB** |

Two regimes, and the boundary is exactly at unity.

**Playing at or below the source pitch, interpolation is everything** — 69 dB between drop-sample
and cubic, matching chapter 9.

**Playing above it, interpolation is worth nothing at all.** All three interpolators measure
identically to the decimal at 2× and 4×, because the problem is no longer reconstructing between
samples. The recording's own harmonics have moved above Nyquist and folded, and an interpolator's
job is to *reconstruct* information, not to *remove* it. Cubic interpolation of a signal that is
already aliased produces a beautifully interpolated aliased signal.

The fix is the same one chapter 9 reached by another road: band-limit per playback rate. A good
sampler keeps decimated copies of each sample — the mipmap of §9.4 — or runs a properly
band-limiting resampler, so that pitching up removes the content that will not fit before it folds.
Anything that only upgrades the interpolator is solving the wrong half.

## 14.6 Varispeed: the hardware did not resample at all

§14.5 measures the problem a *software* sampler has, and it is worth being explicit that the
machines being emulated did not have it. They made pitch by **changing the converter's clock**, one
DAC per voice, the way a tape or a record changes pitch with speed. There is no interpolator in that
path because there is no resampling in that path: the same stored bytes are simply clocked out
faster or slower.

**The consequence is the one that matters for a converter model.** If the clock makes the pitch,
then the converter's own rate *is* a function of the note. Play an octave down and the virtual
sample rate halves, so the zero-order hold's steps get longer and every image it generates comes
down an octave with the fundamental. The grit tracks the keyboard.

**It is tempting to conclude that such a machine got dirtier as it played lower** — images sliding
down beneath a fixed analogue reconstruction filter — and **the Emulator II was built so that it
does not.** Its service manual puts a **switched-capacitor** filter (S3528) at the end of every
voice, after the DAC and the SSM2045 (p. 2-17). A switched-capacitor filter's cutoff is directly
proportional to its clock, which is the standard way to hold a filter in fixed ratio to a *varying*
sample rate; putting one there is only worth doing if the stage ahead of it changes rate. So the
reconstruction filter tracks the converter, and the machine keeps the same relationship between
signal and images at every pitch. `research:instruments/emulator-ii.md` §5 carries the sourcing and
the one inference it still rests on.

**TAL-Sampler deliberately does not do this**, and says so: the engine down-samples to the chosen
rate, models the DAC *there*, and up-samples for pitch (§14.1). That decouples the converter rate
from the note — a defensible choice for a plugin, since it makes the Sample rate control mean one
fixed thing, but it is not the hardware mechanism.

**This collection's sampler varispeeds, and it was not designed to.** It falls out of counting the
hold grid in *source frames* rather than output samples: one grid cell then lasts `grid / ratio`
output samples, so the effective converter rate is `ratio × host_rate / grid` and moves with the
pitch on its own. Measured by `mxm-creative-sampler-dsp`'s `examples/measure_converter.rs` — a
400 Hz sine, full hold, grid 32, so the converter runs at 1500 Hz at the root:

| played | fundamental | clock | first image | measured there | measured at 1300 Hz |
|---|---|---|---|---|---|
| root | 400 Hz | 1500 Hz | 1100 Hz | **0.06363** | 0.00011 |
| octave down | 200 Hz | **750 Hz** | **550 Hz** | **0.06359** | 0.02693 |

The image moved from 1100 Hz to 550 Hz at the same magnitude — it followed the note exactly. The
1300 Hz energy an octave down is not a fixed clock surviving; it is the *second* image of the 750 Hz
one, at 2 × 750 − 200. A fixed-clock engine would have kept the first image at 1300 Hz and left
550 Hz empty, which is the opposite of what the numbers say.

**Evidence standard.** `research:instruments/emulator-ii.md` was opened on 2026-09-11 to put a
sourced page behind this section, and it reports the split honestly. **Primary**, from the Emulator
II service manual: eight voices, each with its own converter, its own SSM2045 and its own
switched-capacitor output filter. **Secondary**, reported consistently but not quoted from primary
documentation: that pitch is made by varying each voice's DAC clock, and the 27.7 kHz figure itself.
Our own measurement of our own engine is first-hand. Build to the mechanism, but see
`research:instruments/emulator-ii.md` §8 before treating the history as settled — and note that no
page exists for the Fairlight, the Synclavier or any Akai.

## 14.7 So what makes it special

Pulling the measurements together into an answer to the original question.

**It models the converter, not the word length.** A DAC mode list of Emu II / AM6070 / S1000 /
Sample Hold / Linear / Clean is a list of *quantisation laws and output stages*. Companding alone is
worth 21 dB at −36 dBFS against linear 8-bit, in the direction that matters musically, and it
changes the *character* of the noise from gravel to texture.

**It does the vintage processing at the vintage rate.** The order of operations — downsample, then
model the converter, then resample to pitch — is what allows the folding of §14.3 to happen at all.
Bit-crushing at 44.1 kHz cannot produce it. This is the single largest difference between "emulates
a sampler" and "has a bit-crusher".

**It separates the reconstruction filter from the converter.** The images of §14.4 are −1.6 dB down
for a high tone on a 27.7 kHz machine; whether they are there or not is a bigger tonal decision than
the bit depth, and it is offered as its own switch.

**Everything after the converter is a synthesizer.** Self-oscillating ZDF filters, three AHDSR
envelopes with a switchable RC curve, three LFOs, a modulation matrix, four layers. The sample is a
source; the voice around it is where the rest of the sound comes from. The Emulator II worked the
same way — its 8-bit converter fed SSM2045 four-pole resonant analog filters, and half of what
people remember as "the Emulator sound" is those filters.

**And the honest fifth reason:** a great many samplers get §14.5 wrong, and a plugin that
band-limits its transposition properly will sound cleaner than its competitors on every pitched-up
patch, for reasons no feature list mentions.

Nothing in that list is exotic. It is a converter model, an order of operations, a filter choice and
a resampler — each of which is measurable, and each of which most sample players skip.

## 14.8 Consequences for this repository

The MXM collection has no sampler and this proposes none. What is worth keeping if one is ever
built:

- **Band-limit transposition, or nothing else matters.** Measured 69 dB of interpolator quality
  evaporates the moment the ratio passes 1.0. Mipmap the sample or use a decimating resampler.
- **Use 4-point cubic at or below unity**, where it is worth 30 dB over linear for a few
  multiply-adds.
- **If a vintage mode is ever wanted, build it as a rate reduction with a deliberately imperfect
  filter, at the reduced rate** — not as a quantiser at the host rate. That is where the −25 dB of
  program-dependent folding comes from.
- **Companding is a better 8-bit than linear 8-bit**, by 21 dB where music actually lives. If a
  low-resolution mode is on the table, µ-law is the one to implement.
- **Model the reconstruction stage as a separate choice.** Bare zero-order hold at a low rate puts
  images 1.6 dB below the signal inside the audible band; that is a tonal decision, not a bug to
  hide.

Untested and worth measuring before believing: whether the *segmented* G.711 companding law audibly
differs from the analytic one used here, and what a realistic 1980s reconstruction filter — a few
poles, not 96 dB — does to §14.4's images. Both are in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

---

Back to [README.md](README.md) · sources: [08-sources.md](08-sources.md)
