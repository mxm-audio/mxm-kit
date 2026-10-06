# 11. Additive synthesis and resynthesis

Every other chapter builds a waveform and then works to keep its bandwidth under control. This one
inverts the problem: build the sound out of sine partials, one per harmonic, and bandwidth is never
in question because nothing above Nyquist is ever generated.

The price is that you now need hundreds of oscillators per note, and a way to know what amplitude
each of them should have. The first is an engineering problem with measured answers. The second is
*analysis*, and it is what turns additive synthesis into **resynthesis** — the technique behind
FL Studio's **Harmor**, which is the specific thing this chapter was written to explain.

Figures from [`osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs) §10 at 44.1 kHz.

---

## 11.1 A bank of partials

```rust
impl Osc for PartialBank {
    fn next(&mut self) -> f64 {
        let mut acc = 0.0;
        for p in self.partials.iter_mut() {
            acc += p.amp * sine(p.phase);
            p.phase += p.inc;
            if p.phase >= 1.0 { p.phase -= 1.0; }
        }
        acc
    }
}
```

That is the whole engine. Its properties are the mirror image of everything else in this reference:

- **No aliasing, by construction.** A partial above Nyquist is simply not created. There is no
  correction function, no oversampling, no table selection, nothing to get wrong.
- **No waveform.** There is no "sawtooth" in the code — a sawtooth is what you get when the
  amplitudes happen to be `1/k`. Every spectral shape costs the same.
- **Perfect independence.** Each partial's frequency, amplitude and phase can be modulated
  separately, at audio rate, without any interaction. Filters, formant shifts and inharmonic
  detuning become arithmetic on a list of numbers rather than signal processing.
- **Cost proportional to partial count**, which for a bass note is hundreds.

## 11.2 Three ways to make a sine, measured

The per-partial oscillator is the entire cost of the technique, so it is worth measuring properly.
Three implementations, all producing the same partial:

| implementation | junk (everything not the wanted bin) |
|---|---|
| `f64::sin` of a phase accumulator | −247.6 dB |
| 2048-point table, linear interpolation | −129.1 dB |
| magic-circle resonator | −247.0 dB |

and their cost, as a whole bank, with the right-hand column being one voice's share of a single
core at 48 kHz:

| partials | `sin` | table | resonator | core % (table) |
|---|---|---|---|---|
| 16 | 130.8 ns | 66.7 ns | 12.4 ns | 0.3% |
| 64 | 550.3 ns | 233.2 ns | 45.0 ns | 1.1% |
| 256 | 1939.3 ns | 923.3 ns | 185.4 ns | 4.4% |
| 516 | 3821.8 ns | 1919.0 ns | 372.4 ns | 9.2% |

Per partial, per sample: **7.4 ns with `sin`, 3.7 ns with a table, 0.72 ns with a resonator.**

The resonator is the magic-circle (coupled-form) oscillator — two state variables, two multiplies,
two adds, no transcendental and no memory traffic:

```rust
PartialKind::Resonator => {
    for p in self.partials.iter_mut() {
        p.x += p.eps * p.y;
        p.y -= p.eps * p.x;
        acc += p.amp * p.x;
    }
}
```

Five times cheaper than a table read and as clean as `sin` — but it has two catches, and the second
one is not in the folklore.

**It cannot retune cheaply.** `eps = 2·sin(π·f/fs)` is fixed at construction. Changing a partial's
frequency means a `sin` per partial, which is the cost you were avoiding. For fixed harmonic
frequencies this is free; for anything gliding it is not.

**Its amplitude depends on frequency.** Measured, running each partial for 60 seconds:

| frequency | amplitude drift over 60 s | actual peak (unit input) |
|---|---|---|
| 55 Hz | 0.0000 dB | 1.000008 |
| 440 Hz | −0.0000 dB | 1.000491 |
| 4 kHz | 0.0000 dB | 1.042013 |
| 15 kHz | 0.0000 dB | 2.077246 |

The drift column is the good news and it is genuinely good: **zero measurable drift over 60
seconds** at every frequency, because the magic circle is a symplectic integrator — it conserves an
elliptical invariant exactly, even in floating point. It will not decay or blow up, ever, which is
exactly what a recursive oscillator is usually bad at.

The peak column is the trap. The invariant it conserves is an *ellipse*, not a circle, and the
ellipse's eccentricity grows with `eps`. A partial at 15 kHz comes out **6.35 dB too loud**; at
4 kHz, 0.36 dB; at 440 Hz, 0.004 dB. So a resonator bank needs a per-partial amplitude correction
that a `sin` bank does not — trivial to apply once known, and a silent high-frequency tilt if not.

**On inverse-FFT synthesis.** For very large partial counts the standard answer is not a bank of
oscillators at all: write each partial as a windowed kernel directly into a spectrum and take one
inverse FFT per block. Rodet and Depalle introduced it in 1992 and report a cost reduction "on the
order of 15" against oscillators. We have not implemented or measured it; it is the obvious next
step for anything past a few hundred partials, and it is recorded as an open gap in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

## 11.3 From synthesis to resynthesis

An additive bank needs an amplitude for every partial at every moment. Get those numbers from a
*recording* rather than from a patch, and you have resynthesis.

The analysis is the hard half. Ours is the easy case deliberately — known pitch, exactly harmonic —
so it is four lines:

```rust
for k in 1..=partials {
    let w = 2.0 * PI * f0 * k as f64 / fs;
    // ... correlate the frame against cos(w n) and sin(w n) ...
    amps.push(2.0 * (re * re + im * im).sqrt() / win as f64);
}
```

and playback is an interpolated read of those frames:

```rust
/// Amplitudes at an arbitrary time in samples, linearly interpolated between
/// frames. Reading this at `t / stretch` is the whole of time-stretching.
fn at(&self, t: f64, out: &mut [f64]) { ... }
```

The real problem has three parts that this does not model, and every one of them is a research
area: estimating the fundamental, **tracking** partials as they appear, vanish, and drift in
frequency, and representing the part of the sound that is not sinusoidal at all. The canonical
references are McAulay and Quatieri's sinusoidal analysis/synthesis (1986), which introduced
birth-death partial tracking, and Serra and Smith's **spectral modelling synthesis** (1990), which
adds the second half of the answer: model the sound as deterministic partials **plus** a stochastic
residual, because a flute's breath and a snare's crack are not sums of sinusoids and never will be.

## 11.4 Why it stretches so cleanly

This is the property that makes resynthesis interesting, and it follows from one fact: **in an
additive representation, time and frequency are separate parameters.** The partial frequencies live
in one array and their amplitudes-over-time in another. Reading the amplitude array at half speed
stretches the sound; it cannot change the pitch, because it never touches the frequencies.

Contrast the **frame-based** family — overlap-add, WSOLA, and the phase vocoder — which all work by
cutting the signal into frames and reassembling them at a different spacing. Being frame-based is
the property that matters here, not which domain the frames are processed in: overlap-add and WSOLA
work on time-domain frames and the phase vocoder on their spectra, and all three are stitching.
Stitching leaves seams: phase discontinuities between frames, the characteristic "phasiness" that
Laroche and Dolson's phase-locking work (1999) exists to reduce, and transient smearing.

Measured, on a 220 Hz harmonic tone whose partial levels sweep (so the analysis has something to
track), stretched by additive resynthesis and by naive overlap-add:

| stretch | method | output `f0` | pitch error | envelope ripple |
|---|---|---|---|---|
| 0.5× | additive resynthesis | 220.10 Hz | **+0.44 cents** | 1.29 dB |
| 0.5× | naive overlap-add | 224.76 Hz | +36.69 cents | 2.07 dB |
| 1.0× | additive resynthesis | 220.04 Hz | −0.06 cents | 1.38 dB |
| 1.0× | naive overlap-add | 220.04 Hz | −0.06 cents | 1.40 dB |
| 2.0× | additive resynthesis | 220.04 Hz | **−0.00 cents** | 1.49 dB |
| 2.0× | naive overlap-add | 239.22 Hz | +144.68 cents | 6.29 dB |
| 8.0× | additive resynthesis | 220.04 Hz | **+0.00 cents** | 1.07 dB |
| 8.0× | naive overlap-add | 199.79 Hz | −167.15 cents | 4.67 dB |

The source's own envelope ripple over the same measurement is 1.40 dB — the brightness sweep moves
the level, so that is the floor, not zero.

Read it as three results:

1. **At 1× the two methods are identical**, which is the control: the harness is not rigged.
2. **Additive resynthesis holds pitch to under half a cent at every stretch factor**, including 8×,
   and adds no amplitude ripple beyond what the source already had. There is no "stretch quality"
   parameter because there is nothing being stretched — the oscillators run at their own frequencies
   and only the control data moves.
3. **Naive overlap-add is destroyed by a factor of two**: 145 cents sharp at 2×, 167 cents flat at
   8×, with 4–6 dB of amplitude wobble. This is the un-phase-locked baseline, not a modern phase
   vocoder — WSOLA and phase-locking exist precisely to fix this and get much closer — but it shows
   what the stitching approach is fighting, and why the additive route has nothing to fight.

Cost: our 24-partial resynthesis read measures **137 ns/sample**, about 5.7 ns per partial —
the table-read cost of §11.2 plus the per-sample interpolation of the analysis frames.

## 11.5 The phase vocoder, and where it sits

The obvious question, given §11.4: a phase vocoder also analyses into the frequency domain, also
resynthesises, and is also used for time-stretching. Is it the same thing?

**Not quite, and the difference is exactly one thing: where the frequencies live.**

A phase vocoder takes overlapping frames, FFTs each one, and keeps a magnitude and a phase for every
**bin** — a fixed grid of frequency slots decided by the transform size, not by the sound. To stretch,
it resynthesises with a different hop, and to stop each bin's sinusoid from jumping it propagates
the phase forward by that bin's *instantaneous* frequency, estimated from how much the phase advanced
between the analysis frames. Then it inverse-FFTs and overlap-adds.

An additive engine keeps a list of **partials** — each with its own frequency, wherever that
frequency happens to be — and drives an oscillator per partial.

So the compact statement is: **a phase vocoder is additive resynthesis with the partials nailed to
bin centres and the oscillator bank implemented as an inverse FFT.** That is not a metaphor. The
inverse-FFT synthesis of §11.2, which Rodet and Depalle introduced to make large oscillator banks
affordable, is the phase vocoder's synthesis half driven by tracked partials instead of by bins.
The two techniques share their machinery and differ in what they put through it.

Three consequences follow, and they are the reason the distinction is worth keeping.

**Resolution against tracking.** A bin grid cannot separate two partials that fall inside one bin,
and a partial that glides across bins hands its phase-advance estimate from one bin to the next —
which is why vibrato, glissandi and heavy detuning are where phase vocoders struggle. A partial
tracker has no grid and no such limit. Its price is that the tracking is the hard part: partials
appear, vanish and cross, which is the birth-and-death problem McAulay and Quatieri solved and which
§11.3's toy analysis carefully does not attempt.

**Phasiness has a specific cause.** A real partial does not occupy one bin, it occupies several, and
their phases must stay in a fixed relationship or the partial's waveform smears in time. Standard
phase propagation advances every bin independently, so those relationships drift apart — heard as a
loss of transient definition and a hollow, chorused quality. Laroche and Dolson's fix is **phase
locking**: find the peak bins, and slave the phases of the bins around each peak to the peak's own.
That is a *vertical* coherence repair, complementing the *horizontal* frame-to-frame propagation.
An additive engine has neither problem, because a partial is one oscillator and there is nothing to
keep in step.

**Both are still frame-based, and that is what §11.4 was really contrasting.** A phase vocoder's
frames are what smear transients. An additive engine has frames too — §11.3's analysis has 253 of
them — but they carry only amplitudes and frequencies, which are interpolated between; the
oscillators themselves run continuously and are never restarted. Nothing is cut and nothing is
rejoined.

### Measured

A phase vocoder is cheap to write once the FFT is there, so rather than leave §11.4's claim about it
unsupported, here is one: 1024-point window, Hann on both sides, 256-sample synthesis hop, with three
choices of what to do with phase — carry it through unchanged (frequency-domain overlap-add),
propagate it from the estimated instantaneous frequency (the actual phase vocoder), or replace it
with a new random value every frame (what PaulStretch does).

**Does the transform survive a round trip?** Stretch 1.0, so nothing has to be invented:

| source | error against the input |
|---|---|
| harmonic tone | −239.3 dB |
| noise | −216.8 dB |

**It is an identity, noise included.** That is the first half of the answer to "can a phase vocoder
reproduce noise": unmodified, it reproduces *everything* exactly, because the STFT is invertible and
there is no model in the way. This is the sharpest contrast with sinusoidal modelling, which is lossy
by construction — a partial list cannot represent noise at all, which is why
[§11.7](#117-where-additive-resynthesis-fails) needs Serra and Smith's stochastic residual.

**Stretching a harmonic tone 8×:**

| phase mode | output `f0` | cents off |
|---|---|---|
| keep (frequency-domain overlap-add) | 199.73 Hz | −167.7 |
| **propagate (the phase vocoder)** | **220.04 Hz** | **−0.0** |
| randomise (PaulStretch-style) | 176.97 Hz | −377.1 |

Two things fall out. The "keep" row lands at −167.7 cents, which is
[§11.4](#114-why-it-stretches-so-cleanly)'s naive time-domain overlap-add result of −167.15 to within
a cent — the same failure seen from the other domain, and a useful check that both implementations
are measuring the same thing. And **a properly propagated phase vocoder holds the pitch exactly**,
matching the additive engine's 0.00 cents. So §11.4's table should not be read as "what a phase
vocoder does": it is what the crude end of the family does, and the sophisticated end is not
measurably worse than additive resynthesis on a steady tone.

(The randomise row's −377 cents is not a transposition. Random phase preserves the magnitude
spectrum but destroys coherence between frames, so peak-picking a pitch from the result is
meaningless. The number is reported only to say that the metric has stopped working, which is itself
the finding.)

**Stretching noise 8×**, measured by spectral flatness from 200 Hz to 8 kHz — near 0 dB is noise,
strongly negative is energy collected into peaks — and by how much the output repeats itself at the
synthesis hop:

| phase mode | flatness | vs source | repeats at hop |
|---|---|---|---|
| source | −2.50 dB | — | 0.0053 |
| keep | −67.83 dB | −65.32 | 0.0110 |
| **propagate** | **−4.00 dB** | **−1.50** | 0.0293 |
| randomise | −2.72 dB | −0.22 | 0.0095 |

and across stretch factors, with standard propagation:

| stretch | noise flatness | tone, cents off |
|---|---|---|
| 1× | −2.48 dB | 0.0 |
| 2× | −3.51 dB | 0.0 |
| 4× | −3.18 dB | −0.0 |
| 8× | −4.00 dB | −0.0 |
| 16× | −5.54 dB | −0.0 |

**Noise survives better than the folklore predicts.** A stretched-by-eight noise band is 1.5 dB less
flat than the source, and 3 dB less at 16× — a gradual degradation, not the collapse into tones the
usual description implies. The reason is worth understanding: the phase *advance* is re-estimated
from the analysis phase **every frame**, and for noise that estimate is different every time. The
propagated phase therefore performs a random walk rather than locking into a steady sinusoid, and it
is the frames' magnitudes that carry the noise character — which the vocoder copies verbatim.

The "keep" row shows what a collapse actually looks like when it happens: −65 dB of flatness, the
output comb-filtered into near-tonality by overlap-adding almost-identical analysis frames at
mismatched phases. It is a broken configuration rather than a design anyone ships, included as the
control that shows the metric can detect the failure it is looking for.

**Randomised phase preserves noise best** — 0.2 dB from the source — and destroys everything tonal.
That is the PaulStretch trade in two numbers, and the reason it is the tool people reach for on pads
and textures and never on a melody.

### What this measurement does not establish

A phase vocoder copies the analysis magnitudes into the synthesis frames unchanged, so **any
magnitude-domain measure is structurally biased toward saying the output is fine.** Spectral flatness
is such a measure. The temporal check for frame-rate repetition found nothing gross either
(0.029 against the source's 0.005), but neither metric addresses the artefact people actually
complain about — the "watery", "phasey" quality, which is a loss of coherence *between the bins of
one partial* and a misalignment of transients, not a change in the magnitude spectrum.

So the honest conclusion is narrower than the table looks: **the spectrum survives, and steady noise
is one of the easier cases for a phase vocoder rather than one of the hard ones.** The hard cases are
transients and gliding partials, for the reasons given above. A metric that captures inter-bin
coherence — or a listening test — would be needed to say more, and is recorded in
[07-rust-recipes.md §7.6](07-rust-recipes.md#76-open-gaps).

**One implementation warning**, since it cost an hour: the **synthesis** hop is the fixed one and the
analysis hop is derived from it, not the other way round. With a fixed analysis hop, a stretch of 8
places the synthesis frames eight hops apart, so for any hop above `n_fft/8` the output frames do not
overlap — or even touch — and the window-sum normalisation divides the gaps between them by nearly
zero. It measured as a 309-cent pitch error and a plausible-looking table before it was found.

Where the phase vocoder turns up elsewhere in this reference: it is what most DAW time-stretching
is, it is the "spectral synthesis (phase vocoding)" engine in the Tasty Chips GR-MEGA and the
spectral oscillator in Padshop 2 ([15-granular-in-the-wild.md](15-granular-in-the-wild.md)), it is
what PaulStretch pushes to its limit, and it is the "legacy algorithms" that Harmor's documentation
is describing when it talks about sound "cut in analysis frames… stitched together as good as
possible" in §11.6 below.

## 11.6 Harmor

With the above in hand, Harmor is legible. What follows is drawn from Image-Line's own manual and
from a third-party manual; **Image-Line has not published the algorithm**, so the mechanism claims
are labelled as inference where they are inference.

**The engine is additive.** Up to **516 partials per note, per unison voice**, each with its own
frequency, amplitude and phase. The manual is explicit that the synth works on "a table of frequency
and amplitude data" rather than on an audio stream — which is why its filters, phaser, prism and
harmonizer are *arithmetic on partials* rather than DSP on a signal. That is the architectural
choice everything else follows from:

| Harmor control | What it is, in partial terms |
|---|---|
| Filters (LP/BP/HP, custom shapes) | a gain curve evaluated per partial |
| Phaser | a comb-shaped gain curve over partial index |
| Prism | shifting partial frequencies off their harmonic positions — harmonic becomes inharmonic |
| Harmonizer | copying the partial set to a shifted position and mixing |
| Blur | smearing the partial data horizontally (time) or vertically (frequency) |
| Harmonic level envelopes | the amplitude array itself |

Two of those are worth dwelling on, because they are things a conventional synth *cannot* do at
any price. **Prism** moves partials off the harmonic grid — there is no filter that turns a
harmonic spectrum into an inharmonic one. **Harmonic blur** smears energy across partial index —
there is no time-domain operation that blurs the spectrum without also blurring the waveform. Both
are trivial in an additive representation and impossible outside one. The manual notes harmonic
blur is "one of the biggest CPU eaters", which fits: vertical smearing touches every partial.

**The IMG section is the resynthesis half**, and it has two modes that are not the same thing:

- **Resynthesis mode** keeps the analysed sound at full detail *including phase*. The third-party
  manual describes it as keeping "an internal higher-resolution copy of the sound".
- **Image synthesis mode** represents the sound as two images — a **gain plane** (pixel brightness
  is partial level, black is silent) and a **pitch plane** (vertical position is partial frequency,
  horizontal is time) — which you can edit in any image editor.

The conversion is **one-way**: resynthesis can be converted to image synthesis, and not back,
because the phase data is discarded. That single detail explains most of what users report about
the two modes, and it is the clearest statement anywhere of what Harmor's resynthesis actually
stores.

**On the stretching**, the third-party manual puts the argument in the same terms as §11.4:

> There exist various legacy algorithms to preserve an audio signal's pitch when playing it faster
> or slower. The sound is cut in analysis frames where each snippet is superimposed and then
> stitched together as good as possible. In Harmor all such legacy imperfections are removed because
> the additive engine has full control over the sound at any speed.

That is exactly the mechanism our §11.4 measurement isolates: no frames are being stitched, so the
artefacts of stitching cannot occur. The controls are consistent with it — **image time offset**
relocates the playback position, **image fine/course speed** change the rate — and the frequency
axis has its own separate control, **image formant shift**, which is the additive representation's
other free lunch: moving the spectral envelope without moving the pitch.

**Inference, clearly labelled.** Our numbers say a bank of 516 table-read oscillators costs 9.2% of
a core. Harmor allows 516 partials *per unison voice*, so a modest patch — four notes, eight unison
voices — is 16,512 partials, which extrapolates to roughly three cores with table oscillators and
around half a core with resonators. Since Harmor plainly runs such patches on one core, it is
almost certainly not using a straightforward oscillator bank. The published technique that fits is
**inverse-FFT synthesis** (§11.2), whose reported ~15× advantage lands the same patch comfortably
inside a fraction of a core. We have not verified this and Image-Line has not documented it: it is
an inference from a cost extrapolation, and it should be treated as one.

The manual's own CPU guidance is consistent with an engine whose cost scales with partial count:
partial count is reducible from 516 down to **12**, lower notes cost more (more partials fit below
Nyquist), and unison multiplies everything.

## 11.7 Where additive resynthesis fails

The technique's weaknesses are as structural as its strengths, and the marketing rarely lists them.

**Noise is not sinusoids.** A partial-based model represents breath, bow scrape, cymbal wash and
consonants as a dense thicket of short-lived partials, which is both expensive and wrong-sounding —
stretch it and the noise becomes a chorus of tones, the characteristic "metallic" or "watery"
resynthesis artefact. Serra and Smith's deterministic-plus-stochastic split is the standard
answer: model the tonal part additively, model the residual as filtered noise, and stretch the
noise's *envelope* rather than its waveform.

**Transients are the opposite problem.** A drum hit is broadband and instantaneous — every partial
starting at once with aligned phase. Any analysis with a window long enough to resolve partials is
too long to localise the attack, so stretching smears it. This is the time-frequency uncertainty
principle, not an implementation flaw, and no representation escapes it; the practical fixes
(transient detection, and passing transients through unstretched) are all special cases bolted on.

**Pitch has to be found first.** Our measurement assumes a known fundamental. Real material needs
pitch tracking, and everything downstream inherits its errors. Polyphonic or inharmonic material
makes "the fundamental" meaningless, which is why resynthesis tools are at their best on single
sustained notes and at their worst on a mixed loop.

**Phase is a choice you have to make.** With partial amplitudes but no phases, the resynthesis must
invent them — and the invented phases determine whether the waveform looks anything like the
original, whether unison voices cancel, and whether transients survive. Harmor's answer is a
**harmonic phase mapping** shared across timbres to prevent cancellation, and keeping the analysed
phase in resynthesis mode. The general answer in the literature is phase locking of the kind
Laroche and Dolson describe.

## 11.8 Consequences for this repository

mxm-mono-01 is a one-oscillator subtractive monosynth and none of this applies to it. Recorded because
the collection may one day want an instrument that does apply, and because these numbers took a
day to obtain:

- **Additive is affordable at synth scale and not at resynthesis scale.** 64 partials is 1.1% of a
  core; 516 is 9.2%; a unison patch of 16,000 partials is not an oscillator-bank problem at all.
- **Use resonators for fixed frequencies and table reads for anything that glides**, and remember
  the resonator's frequency-dependent amplitude (+6.35 dB at 15 kHz) needs correcting.
- **If partial counts go past a few hundred, stop and implement inverse-FFT synthesis** rather than
  optimising the oscillator loop.
- **The reason to reach for additive at all is not aliasing** — chapters 2 and 9 solve that far more
  cheaply. It is the operations that only exist in the partial domain: independent time and pitch,
  formant shift, inharmonic warping, spectral blur.
- **Anything claiming artefact-free stretching is claiming a parametric representation.** Ask what
  it does with noise and transients; that is where the bodies are.

---

Back to [README.md](README.md) · sources: [08-sources.md](08-sources.md)
