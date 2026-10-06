# Modulation — a working reference

The third leg of the voice. [`../filters/`](../filters/README.md) covers what shapes the sound and
[`../oscillators/`](../oscillators/README.md) covers what makes it; this covers what *moves* it.

It exists because `crates/mxm-mono-01-dsp/src/lfo.rs` and `envelope.rs` are shipped, tested code with no
durable documentation, and because the third topic here — parameter smoothing — is where a lot of
otherwise good plugins audibly fail and is currently folklore in an AGENTS.md with no numbers behind
it.

The through-line: **a modulation source is an oscillator whose output you never hear, which changes
what you have to worry about and not whether you have to worry.** An LFO's aliasing does not reach
your ears directly; it reaches them through whatever the LFO is modulating. That indirection is the
whole subject.

## Read in this order

| File | What it covers |
|---|---|
| [01-lfos.md](01-lfos.md) | Rate ranges, shapes, whether an LFO needs band-limiting and what the shipped one costs by not having it, free-run against retrigger, sample and hold |
| [02-envelopes.md](02-envelopes.md) | Exponential against linear against RC, why the shipped attack cannot click, overshoot targets, and what "vintage Digital / RC" switches actually switch |
| [03-smoothing-and-events.md](03-smoothing-and-events.md) | Zipper noise measured, smoothing time constants, control rate against audio rate, and why block length is capped |
| [04-glide-and-portamento.md](04-glide-and-portamento.md) | Fixed time against fixed rate, why a time constant is not a duration, why fixed time makes the effect tempo-dependent, and what a slide flag actually flags |
| `research:modulation/buchla-208-control-sources.md` (research repository) | **Machine-specific appendix:** the original 208/218's linear AHD envelope, ramp-and-pulse pulser, five-stage sequencer, four held random voltages, complement inverter, envelope detector and pressure keyboard |

## Status of the numbers

Measured figures come from `crates/dsp-lab/examples/mod_spike.rs`:

```bash
cargo run -p dsp-lab --release --example mod_spike
```

The run these documents quote is recorded verbatim in
[`measurements-run.txt`](measurements-run.txt). Conditions: 44.1 kHz, 65536-point FFT,
exactly-periodic rates so no window function is needed, AMD Ryzen Threadripper 3970X, `rustc`
1.98.0, release.

**One important limit.** Sections 1–3 of that program measure the **shipped** `lfo.rs` and
`envelope.rs` through their public API. Section 4 does not, and cannot: smoothing belongs to the
plugin — [`../../plugins/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/AGENTS.md) owns parameters, ranges and smoothing,
and [`../../crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) explicitly disclaims
them — and the DSP crate has no dependencies, so an example inside it cannot reach the shipped path.
**§4 measures a generic one-pole model.** Every number from it is labelled as such where it appears,
and it is evidence about the technique, never validation of mxm-mono-01. Closing that properly means
measuring through the player, which hosts the CLAP and captures audio; it is recorded as an open
item in [03-smoothing-and-events.md](03-smoothing-and-events.md#34-what-is-not-measured-here).

## The three things worth knowing up front

1. **An LFO does alias, and it does not matter much — but "not much" is a number.** The shipped
   square LFO measures **−32.5 dB** of aliasing in its own output at 30 Hz. Measured where it
   actually lands — modulating a 1 kHz tone — that arrives **68 dB** below the signal. The claim in
   `lfo.rs`'s module docs survives measurement, and now has a figure attached.

2. **Shape matters far more than rate.** A triangle LFO measures −93.7 dB against a square's
   −32.5 dB at the same rate: **61 dB**, because a triangle has a corner where a square has a step.
   That is [`../oscillators/01-fundamentals.md`](../oscillators/01-fundamentals.md)'s
   discontinuity-order rule, unchanged, applied to a signal nobody listens to.

3. **The shipped envelope cannot click, by construction.** `coef()` clamps every stage time to
   `MIN_TIME_S = 1 ms`, so a zero-length attack is unreachable and the measured high-frequency
   splatter is flat at −64 dB from 0 ms to 20 ms of requested attack. Protection by clamp, not by
   luck — worth knowing before someone "optimises" the clamp away.

4. **`glide` is a time constant, and the control calls it a time.** The shipped lag covers 63% of
   the interval in the displayed number and takes about **seven** of them to arrive within a cent.
   Chapter 4 has the arithmetic and the consequence: fixed-time glide is why a slide sounds like a
   different effect at 90 BPM and at 160. **None of chapter 4 is measured** — `mod_spike` does not
   reach glide, which is recorded there as the open gap.
