# 1. LFOs

An LFO is an oscillator with two differences: it runs below the audio band, and nobody hears its
output. The first difference is cosmetic. The second changes what you have to care about — but not,
as this chapter measures, whether you have to care.

Figures from `crates/dsp-lab/examples/mod_spike.rs` §1–§2 at 44.1 kHz.

---

## 1.1 What the shipped one is

`crates/mxm-mono-01-dsp/src/lfo.rs`: five shapes, all bipolar in `[-1, 1]`, sharing the oscillator's
`Phasor`.

| Shape | Notes |
|---|---|
| Triangle | Starts at 0 and rises. The default, and the one that sounds like vibrato |
| Square | 50% duty, `+1` at phase zero |
| Saw up / saw down | Ramps across the cycle |
| Random | Sample and hold: a new uniform value at each wrap, held in between |

Rate range 0.05–30 Hz, matching the SH-101's specification of 0.1–30 Hz at the top and going a
little lower at the bottom (`research:oscillators/05-machines.md` §5.1).

Two design decisions are worth pulling out of the source, because both are deliberate and neither is
obvious.

**It sets the increment directly rather than going through `set_freq`:**

```rust
// Set the increment directly: `Phasor::set_freq` clamps to a musical
// minimum of 8 Hz, which would be far too fast for an LFO.
self.phasor.set_inc((rate_hz.max(0.0) / sample_rate).min(0.45));
```

The audio phasor's 8 Hz floor exists to keep a musical oscillator in range; an LFO needs to go three
orders of magnitude below that. Sharing the phasor and bypassing its clamp is the right call, and it
is the kind of thing that gets "fixed" by someone tidying up.

**It free-runs.** The phase is not reset by note-on, matching the hardware. The module docs are
careful about the scope of that claim — it does not mean the LFO keeps advancing while the plugin is
suspended, only that a note-on does not restart it. See §1.4.

## 1.2 The claim in the source, tested

`lfo.rs` states its own position on band-limiting:

> No band limiting. At a 30 Hz ceiling the square and ramp shapes do alias, but an LFO is a
> modulation source rather than something you hear directly, and band limiting it would round off
> the very edges that make a square LFO useful.

That is three assertions: it aliases, it does not matter, and fixing it would cost something. The
first two are measurable.

**It aliases.** Measured as energy off the LFO's own harmonic grid:

| shape | 7.40 Hz | 15.48 Hz | 30.28 Hz |
|---|---|---|---|
| Triangle | −111.9 dB | −102.4 dB | −93.7 dB |
| **Square** | −38.7 dB | −35.5 dB | **−32.5 dB** |
| Saw up | −36.9 dB | −33.7 dB | −30.8 dB |
| Random | *not applicable — sample and hold has no harmonic structure to be off* |

So yes: a 30 Hz square LFO carries −32.5 dB of aliasing. In an audio oscillator that would be
appalling — worse than a trivial modulo-counter sawtooth
([`../oscillators/02-antialiasing.md §2.11`](../oscillators/02-antialiasing.md#211-the-comparison-table)
measures that at −28.1 dB, and this is in the same league).

**Shape matters more than rate.** Triangle to square at the same rate is **61 dB**. That is the
discontinuity-order rule from
[`../oscillators/01-fundamentals.md §1.2`](../oscillators/01-fundamentals.md#12-what-the-classic-waveforms-actually-are)
applied to a control signal: a square has a step and decays at 6 dB/octave; a triangle has only a
corner and decays at 12. Rate, by contrast, buys about 3 dB per doubling.

## 1.3 Where it actually lands

The second assertion — that it does not matter — needs measuring where the LFO's output goes, not
where it is. Multiplying a 1 kHz sine by the LFO puts everything the LFO contains into the audio
band, where it can be measured against the carrier:

| shape | rate | alias energy in the audio band |
|---|---|---|
| Triangle | 7.40 Hz | −120.0 dB |
| Triangle | 30.28 Hz | −101.8 dB |
| **Square** | 7.40 Hz | −74.1 dB |
| **Square** | 30.28 Hz | **−68.2 dB** |
| Saw up | 30.28 Hz | −70.5 dB |

**The worst case is 68 dB below the signal.** The source's claim survives.

Why the improvement over the LFO's own −32.5 dB: the modulated signal has far more wanted energy
than the LFO alone did — a carrier plus a full set of sidebands — so the same absolute alias energy
is a much smaller fraction of it. That is a property of the measurement as much as of the ear, and
it is worth being precise about: the aliased components have not gone anywhere. They are the same
components, sitting at the same absolute level, next to a much louder signal.

Which is exactly the argument for why it does not matter, stated honestly. **An LFO's aliasing is
attenuated by the modulation depth and buried under the thing being modulated.** Drive the depth to
100% on something quiet, or use the LFO to modulate something with very little of its own content,
and the margin shrinks.

## 1.4 Free-run, retrigger and phase

Three behaviours, and instruments differ:

- **Free-running** — the LFO ignores note-on. Every note catches the modulation at a different
  point, which is what makes a slow filter sweep feel like a performance rather than a preset. The
  SH-101 does this and so does ours.
- **Retriggered** — note-on resets the phase, so every note is identical. Necessary when the LFO is
  doing something rhythmic that must line up with the note.
- **One-shot** — the LFO runs one cycle and stops, which makes it an envelope with a shape.

The trap is the same one
[`../oscillators/01-fundamentals.md §1.1`](../oscillators/01-fundamentals.md#11-the-phasor) warns
about for audio oscillators: **resetting phase mid-signal is a discontinuity.** For an audio
oscillator it is a click. For an LFO it is a step in whatever is being modulated — a jump in cutoff,
in pitch, in width — and it will be as audible as the modulation depth makes it. A retriggered LFO
needs either a smoothed destination (see [03-smoothing-and-events.md](03-smoothing-and-events.md))
or a shape that starts at zero.

Note the shipped triangle "starts at 0 and rises" for exactly this reason: if it is ever retriggered,
the jump is zero.

## 1.5 Sample and hold

The `Random` shape draws a new uniform value at each cycle wrap and holds it. Two things to get
right, and the shipped one gets both:

**It must be deterministic.** The generator is seeded from a fixed constant and re-seeded on
`reset()`, so a rendered take is bit-identical when replayed — a contract in
[`../../crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md), and the thing that makes
a golden-audio test possible at all.

**Its aliasing is not measurable and not meaningful.** A held random value is a step function with
no harmonic structure, so "energy off the harmonic grid" has nothing to compare against — the table
in §1.2 says *not applicable* rather than printing a number, which is the honest output. What it
does have is a broadband spectrum with the same 6 dB/octave step decay as a square, so the same
caution applies at high rates and high depths.

## 1.6 What this says for the collection

- **Leave the LFO unbanded.** Measured at 68 dB down where it lands, and band-limiting would round
  the edges that make a square LFO useful. The source's comment was right and now has evidence
  under it.
- **Reach for triangle when depth is extreme.** It is 61 dB cleaner for free, and at high depth on a
  quiet destination that margin is the one that runs out first.
- **Keep the direct `set_inc` call**, and keep the comment explaining why it bypasses `set_freq`.
- **If a retrigger mode is ever added**, pair it with either a zero-starting shape or a smoothed
  destination, because the reset is a step into whatever it modulates.
- **Do not make the sample-and-hold source unseeded.** It would silently break golden-audio testing.

---

Next: [02-envelopes.md](02-envelopes.md)
