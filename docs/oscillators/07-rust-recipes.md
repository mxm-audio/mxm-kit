# 7. Rust recipes, and the verdict for mxm-mono-01

What the shipped oscillator does, what it measurably costs, what to change, and what is still open.

---

## 7.1 What mxm-mono-01 ships

[`crates/mxm-mono-01-dsp/src/oscillator.rs`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/src/oscillator.rs), in one
paragraph: two free-running phasors, a two-point PolyBLEP sawtooth and pulse on the first, a
band-limited pulse on the second at an exactly divided increment for the sub, a xorshift noise
source, a four-source mixer with 0.5 of headroom, and a 15 Hz one-pole DC blocker after the sum.
No tables, no oversampling, no latency, no allocation, no state beyond two phases and the blocker.

Measured behaviour, all at 44.1 kHz from
[`examples/osc_spike.rs`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/examples/osc_spike.rs):

| Property | Measured |
|---|---|
| Sawtooth alias-to-signal | −44.2 dB at 55.9 Hz, −35.4 dB at 440.8 Hz, −25.5 dB at 7040 Hz |
| Pulse alias-to-signal at 440 Hz | −36.9 dB at 50% width, −29.3 dB at 5% |
| Sub alias-to-signal | −43.2 dB (1 oct), −45.9 dB (2 oct square), −44.7 dB (2 oct pulse) |
| High-frequency droop | −3 dB from ~16 kHz, −6.22 dB at 0.45·fs |
| Cost | 1.58 ns/sample for the sawtooth path, against 0.54 ns for a bare counter (mean of five runs) |
| Perceptually alias-free to | `f0 = 2135 Hz` (published NMR figure for the same algorithm) |
| DC offset after the blocker | below 3e-8 across the whole width range |
| Tuning | within 1 cent at 55–4000 Hz and four sample rates (existing unit test) |

## 7.2 The verdict: keep the two-point PolyBLEP

**Recommendation: no change to the sawtooth algorithm.** The reasoning, in order of weight:

1. **It is clean through the range the instrument is for.** mxm-mono-01 is a monophonic bass and lead
   synth ([`../briefs/mxm-mono-01.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/docs/briefs/mxm-mono-01.md)). Its musical range sits below the
   2135 Hz at which a two-point PolyBLEP stops being perceptually alias-free, and the piano's top
   note is 4186 Hz.
2. **The upgrade costs treble, and treble is what this instrument is short of.** The four-point
   version buys 10 dB of alias suppression and gives back 6 dB more high-frequency droop, moving
   the −3 dB corner from ~16 kHz to ~12 kHz. On a single-oscillator synth with a four-pole lowpass
   already removing the top, spending brightness to fix inaudible aliasing is the wrong direction.
3. **The cost argument is not an argument.** 1.58 ns against 1.70 ns per sample is 0.12 ns, which
   is barely outside the 4% run-to-run spread of the measurement itself. In a monophonic voice at
   48 kHz it is under 0.001% of a core. Neither number should influence the decision, in either
   direction — which is worth saying plainly, because "PolyBLEP is the cheap one" is the usual
   reason given for choosing it, and it is not a real reason.
4. **Zero latency is worth something.** The four-point version needs a two-sample output delay, and
   a latency that has to be reported to the host, kept consistent between the main oscillator and
   the sub, and accounted for if a second oscillator is ever added, is a design cost out of
   proportion to 10 dB of inaudible aliasing.

**What would change the verdict:** a second instrument in the collection with a wider pitch range, a
brighter architecture, or several oscillators whose aliases accumulate. At that point revisit with
[§7.4](#74-if-the-answer-ever-changes-the-hybrid).

## 7.3 The things that are worth changing

Small, concrete, and independent of §7.2.

### 7.3.1 The high-frequency droop is undocumented in the code

`oscillator.rs` explains what PolyBLEP is and why the sub gets its own phasor, and says nothing
about the fact that the shipped saw is 3 dB down from about 16 kHz and 6.2 dB down at 0.45·fs. That
is a real spectral property of the output, it is measurable, and someone comparing mxm-mono-01's saw
against a reference will find it and wonder whether it is a bug.

**Action:** record the measured droop as a comment in `oscillator.rs` next to `poly_blep`, with the
number and the reason, in the style the crate's AGENTS.md asks for.

### 7.3.2 The pulse-width collapse at high pitch deserves a comment too

`clamp_pulse_width` explains the mechanism — two corrections must not overlap — but not the audible
consequence: measured, the width range narrows from about 2 kHz and has collapsed to a fixed square
by 16 kHz. On the 2' range that is inside the playable region.

**Action:** add the measured table from
[03-waveshapes.md §3.3](03-waveshapes.md#33-the-two-edge-problem) as a comment, so the behaviour is
a documented property rather than a surprise.

### 7.3.3 The PWM asymmetry is a deviation from the hardware, undeclared

The SH-101 specification sheet gives pulse width modulation as 50% → 0%: one direction only.
`voice.rs` applies it as `pulse_width ± depth · 0.45`, symmetric about the manual width setting.

This is defensible — the collection's naming and design rules ([`../../AGENTS.md`](../../AGENTS.md))
are explicit that hardware architecture inspires the DSP and does not dictate the interface — but
it is currently an undeclared difference rather than a decision.

**Action:** record it in `voice.rs` as a deliberate departure with the reason, or change it. Do not
leave it as an accident.

### 7.3.4 Consider a post-oscillator equaliser, if brightness is ever the complaint

Välimäki, Pekonen and Nam give optimised coefficients for a fixed two-tap symmetric FIR

```
H(z) = b0 + b1·z⁻¹ + b0·z⁻²
```

that restores PolyBLEP-corrected harmonics to within 1 dB of ideal. It is frequency-independent,
so it goes on the mixer output once, not per source — three multiply-adds for the whole oscillator
section, roughly a 20% increase on the section's total cost, and it recovers the 6 dB.

**Not implemented, not measured here.** It is the cheapest available answer if the instrument ever
sounds dull next to a reference, and it should be measured before being believed.

## 7.4 If the answer ever changes: the hybrid

If a future instrument does need the four-point residual, the version worth building is not a
straight swap. It is:

- **Always run the two-sample delay**, whichever residual order is active, so latency is constant
  and switching order is not a discontinuity.
- **Choose the order by `dt`**: two-point below roughly `f0 = 2 kHz`, four-point above. Below the
  crossover the two-point version's aliasing is already inaudible and its droop is smaller; above
  it the four-point's 10 dB matters more than its droop, because there is very little left up there
  to lose.
- **Keep both residuals in the same function** so the mapping of `d` to samples is written once.
  Getting `d` backwards is the single most likely bug — it costs about 30 dB and looks like a
  working oscillator.

The measured four-point implementation is in `osc_spike.rs` as `Blep4` and is the reference for
the plumbing, including the ring buffer and the priming of the delay. It has not been written in
crate style, and the crate does not use it.

## 7.5 What not to do

Recorded because each of these is a plausible-sounding idea that the measurements rule out.

**Do not oversample the oscillator.** Measured, 8× oversampling with a 513-tap decimator lands 6 dB
*worse* than a four-point polynomial correction at roughly 3000× the cost. Oversampling attenuates
aliases after they exist; correction functions stop them existing.

**Do not build the sub from a divider.** The hardware uses a flip-flop because a flip-flop is two
cents. A digital divider clocked from a sampled edge produces sample-quantised transitions —
exactly the thing the rest of the file works to avoid — and it costs the same as an independent
phasor, which measures −43 dB.

**Do not reset the phase on note-on.** It is a click, it is a burst of aliasing no correction knows
about, and the hardware does not do it either. There is a unit test guarding this; do not weaken it.

**Do not switch to DPW or EPTR to save operations.** They are 6 dB worse by energy and more than an
octave worse by NMR, and on this machine EPTR measured 1.8× *slower* than DPW2 despite a lower
operation count — the branches are not free. The whole family lives inside a 1.3 ns spread that
does not matter in a monosynth.

**Do not add wavetables for this instrument.** They win the alias column by 40 dB and lose PWM,
which is a continuum of waveforms rather than one, and 176 kB per waveform set is a poor trade for
inaudible improvement.

## 7.6 Open gaps

The honest list, in the style of [`../filters/07-rust-recipes.md`](../filters/07-rust-recipes.md)
§7.5.

- **Aliasing under modulation is unmeasured.** Every number in this reference is for a steady tone,
  because the exactly-periodic method requires one. Vibrato, glide, envelope-to-pitch and audio-rate
  PWM all move `dt` between samples, which is outside the derivation of every correction function
  here. The method would be to render at 8×, decimate, and compare against a 1× rendering of the
  same modulation. Nothing in the repo does this.
- **The two-tap equaliser of §7.3.4 is neither implemented nor measured.** The coefficients are
  published; whether they behave as claimed on our residual is not verified.
- **No listening test.** Every quality claim here is either an energy measurement made by us or an
  NMR figure taken from a paper. Neither is a listener.
- **No hardware reference.** Nothing was recorded, so no claim in `research:oscillators/05-machines.md`
  is verified against a real instrument, and there is no measurement of how far mxm-mono-01's saw is
  from an SH-101's.
- **The sub's phase relationship to the main oscillator is arbitrary**, where the hardware's is
  rigid. Inaudible in this instrument, and it would not be in a design where the two are meant to
  cancel.
- **Aliasing is measured on the sawtooth in depth and on the pulse at one pitch.** The pulse's
  behaviour across the sample-rate range, and the sub's, are covered by inference rather than
  measurement.
- **Inverse-FFT additive synthesis is unmeasured.** Rodet and Depalle report roughly 15× against an
  oscillator bank; [11-additive-resynthesis.md §11.2](11-additive-resynthesis.md#112-three-ways-to-make-a-sine-measured)
  leans on that figure without checking it, and the inference about how Harmor affords its partial
  counts rests on it.
- **The wavetable cost figures are cache-bound and not reproducible in isolation.** The same code
  measured 4.2 ns and 7.9 ns per sample in two builds. See
  [09-wavetable.md §9.6](09-wavetable.md#96-what-a-wavetable-actually-costs).
- **Two granular questions are open**: whether a BLEP correction on a hard-truncated FOF grain
  recovers the 25 dB between it and a Hann grain, and whether grain *stealing* sounds better than
  dropping when a pool saturates. Neither is implemented.
- **Every cost figure in this reference except the granular ones is from a retired machine.**
  §9e and §9f were re-measured on the current development machine and recorded separately
  (`measurements-run-13900k.txt`); the same machine runs the older cost rows about **2x faster**, so
  the absolute nanoseconds in chapters 2, 3, 9 and 11–17 are stale even though their ratios are not.
  A full re-run on one machine, and a pass updating every cost column from it, is outstanding. It is
  a day's mechanical work and nobody should quote across the two files until it is done.
- **§9f measured grain cost and not grain quality.** The recommendation to move from a sixteen-tap
  computed kernel to an eight-tap polyphase table rests on tap count and on §15.9's measured 30 dB
  between linear and cubic, not on a measurement of the eight-tap kernel's own alias floor. That
  measurement is one row in the spike and has not been made. Until it is, "cheaper and no worse" is
  half-measured.
- **No vectorised grain kernel is measured.** Eight grains to an AVX2 vector is the only route from
  [10-granular.md §10.6.1](10-granular.md#1061-a-sample-reading-grain-is-a-different-animal-and-it-need-not-be)'s
  ~10 ns to the ~2 ns a GR-MEGA-class grain budget needs, and it requires either `unsafe` or a
  dependency. Neither DSP crate may take one today, so the figure is an extrapolation and is
  labelled as one wherever it appears.
- **The phase vocoder is now measured, but only where the metrics work.**
  [11-additive-resynthesis.md §11.5](11-additive-resynthesis.md#115-the-phase-vocoder-and-where-it-sits)
  measures round-trip fidelity, tonal pitch stability and noise flatness under stretch. What it
  cannot measure is the artefact people actually object to: a phase vocoder copies analysis
  magnitudes verbatim, so every magnitude-domain metric is biased toward reporting success, and
  "phasiness" is a loss of coherence *between the bins of one partial*. A metric for inter-bin
  coherence, and a transient-alignment measure, would close this. **Phase locking is also still
  unimplemented**, so what Laroche and Dolson's fix is worth remains unquantified — plain
  propagation already measures 0.0 cents on a steady tone, so the gain would have to be looked for
  on gliding partials and transients.
- **Two sampler questions are open**: whether the segmented G.711 companding law differs audibly
  from the analytic µ-law measured in [14-samplers.md §14.2](14-samplers.md#142-companding-is-why-an-8-bit-machine-sounds-good),
  and what a realistic 1980s reconstruction filter — a few poles rather than 96 dB — leaves of the
  zero-order-hold images in §14.4. Both are cheap to add to the spike.
- **`poly_blep_at` in the spike duplicates the shipped residual.** `oscillator.rs` keeps
  `poly_blep` private and exposes it only through `saw` and `pulse`; §12's phase-distortion
  experiment needs to apply it to a discontinuity neither of those knows about, so the polynomial is
  transcribed into the spike. If the shipped residual ever changes, that copy has to change with it.
  Exporting `poly_blep` would remove the hazard and widen the crate's public surface; neither has
  been decided.
- **Phase-modulating a band-limited oscillator is unmeasured.** Every correction function here
  assumes a constant `dt`, and [12-fm.md §12.8](12-fm.md#128-fm-is-not-only-an-oscillator) argues
  from that assumption — not from a measurement — that PM and PolyBLEP do not compose. Rendering a
  phase-modulated PolyBLEP sawtooth against an oversampled reference would settle it in an hour.
- **The resonator amplitude correction is derived, not applied.** The magic-circle oscillator's
  frequency-dependent amplitude is measured
  ([§11.2](11-additive-resynthesis.md#112-three-ways-to-make-a-sine-measured)); no compensation
  formula has been fitted or checked.

---

Back to [README.md](README.md) · sources: [08-sources.md](08-sources.md)
