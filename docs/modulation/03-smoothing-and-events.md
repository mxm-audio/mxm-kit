# 3. Smoothing, control rate and events

The least glamorous chapter in either reference, and the one whose absence is most audible. A
parameter that jumps produces a step; a step is broadband; and a plugin that steps its parameters
sounds cheap in a way no amount of oscillator work repairs.

**Scope warning, stated once and meant.** The measurements here come from a **generic one-pole
model**, not from the shipped path. Smoothing belongs to the plugin —
`plugins/AGENTS.md` ([`plugin-conventions.md`](../plugin-conventions.md#smooth-signals-not-coefficients)) owns parameters, ranges and smoothing, and
[`../../crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) explicitly disclaims them
— and the DSP crate has no dependencies, so an example inside it cannot reach
`plugins/mxm-mono-01/src/params.rs`. These numbers are evidence about the *technique*. They are not
validation of mxm-mono-01. See [§3.4](#34-what-is-not-measured-here).

---

## 3.1 Zipper noise, measured

A control stepping between two gain values every 10 ms, applied to a 1 kHz sine, measured as energy
above 4 kHz relative to the carrier — where a smoothly-varying gain has none:

| smoothing | energy above 4 kHz | audible step |
|---|---|---|
| none | **−28.5 dB** | yes |
| 0.2 ms | −45.6 dB | yes |
| 1.0 ms | −59.1 dB | no |
| 5.0 ms | −71.6 dB | no |
| 20.0 ms | −84.5 dB | no |

**An unsmoothed parameter puts broadband junk 28.5 dB below the signal.** That is not a subtlety; it
is roughly the level of the aliasing a trivial sawtooth produces
([`../oscillators/02-antialiasing.md §2.11`](../oscillators/02-antialiasing.md#211-the-comparison-table)),
and it appears every time a user moves a knob or a host sends automation.

The scaling is about **14 dB per decade** of smoothing time, and the knee for this test is around
1 ms — below that the step is still audible as a click, above it the broadband component is
inaudible under the signal.

Note what the measure deliberately excludes. The gain steps produce legitimate sidebands close to
the carrier at multiples of 100 Hz; those are the *modulation*, not the defect. Measuring far from
the carrier separates the click from the thing that was asked for — the same reason
[02-envelopes.md §2.3](02-envelopes.md#23-why-the-shipped-envelope-cannot-click) measures above
4 kHz rather than everywhere.

## 3.2 How much smoothing, and of what

**1 to 20 ms is the useful range.** Below 1 ms the step survives; above about 20 ms the parameter
starts to feel disconnected from the control, and fast automation gets smeared into something the
user did not draw.

But the right answer is not one number, because parameters differ in kind:

| Parameter kind | Smoothing | Why |
|---|---|---|
| Gain, cutoff, resonance, pan, PWM | **Yes**, 5–20 ms | Continuously audible; a step is a click |
| Envelope times, glide time | **No** | They set state-machine behaviour. Smoothing them makes the timing impossible to reason about |
| Enum-like switches (filter mode, LFO shape, sub octave) | **No** — crossfade or accept the click | There is no meaningful value between "square" and "triangle" |
| Anything sample-accurate by contract (note pitch at note-on) | **No** | It must land exactly when the event says |

The second row is a rule the collection already follows and states in
`plugins/mxm-mono-01/src/params.rs`:

> **Values that are coefficients rather than signals are not smoothed.** The envelope times and the
> glide time set state-machine behaviour; smoothing them would make the timing impossible to reason
> about.

That is the distinction worth generalising: **smooth signals, not coefficients.** A parameter whose
value is added to or multiplied by the audio needs smoothing. A parameter that configures how
something behaves does not, and smoothing it produces a control that lies about what it is doing.

## 3.3 Control rate, audio rate and block splitting

A plugin's parameters arrive at block boundaries; its audio is per-sample. Three strategies:

**Per-block.** Read parameters once per buffer, hold them for the block. Cheapest, and the source of
zipper noise at exactly the buffer rate — which is why the artefact is worse at large buffer sizes
and can vanish when a developer tests at 64 samples.

**Per-sample smoothing.** Read at block start, glide toward it every sample. §3.1's numbers. This is
what the collection does.

**Block splitting on events.** Cut the buffer at each event so a note-on or a parameter change lands
on the exact sample the host asked for. Necessary for note timing; the crate's realtime rules
require it, with one addition:

> Cap internal blocks: `min(next_event, host_end, start + MAX_BLOCK)`. Splitting only on events lets
> an event-free buffer become one arbitrarily long block.

The cap is the part people miss. Splitting only at events sounds sufficient until a host sends a
4096-sample buffer with no events in it, at which point any per-block work — coefficient updates,
modulation recalculation — happens once for 93 ms of audio, and a filter sweep becomes a staircase
with one step. **The cap converts a correctness property into a bounded one**, and it is why
modulation stays smooth in a buffer that contains no events at all.

## 3.4 What is not measured here

Restating the scope warning as a specific gap rather than a caveat:

- **The shipped smoothing is unmeasured.** §3.1 is a one-pole model. mxm-mono-01 uses `nice-plug`'s
  `SmoothingStyle` — `Logarithmic(20.0)` on gains and cutoffs, `Linear` elsewhere — and how those
  behave under real automation has never been measured, only reasoned about.
- **The right way to close it** is through the player, which already hosts the CLAP and captures
  audio in its tests. That is a cross-product measurement and a piece of work in its own right.
- **Block splitting is asserted, not measured.** The cap is in the contract and in the code; nobody
  has measured what a 4096-sample event-free block actually does to a cutoff sweep with and without
  it.
- **Nothing here is A/B tested against a listener.** "Audible step" in §3.1's table is a judgement
  from the numbers and the mechanism, not from an experiment.

## 3.5 What this says for the collection

- **Smooth signals, not coefficients.** The rule is already in `params.rs`; it belongs in
  `plugins/AGENTS.md` where it applies to every future plugin (since the split it is there, in
  [`plugin-conventions.md`](../plugin-conventions.md#smooth-signals-not-coefficients)).
- **5–20 ms on continuous parameters.** Measured 28.5 dB of broadband junk with none, 71.6 dB down
  at 5 ms.
- **Keep the block cap**, and keep the comment explaining it — like the envelope's `MIN_TIME_S`, it
  looks like a detail and is load-bearing.
- **Measure the shipped path through the player** before quoting any of §3.1's numbers as mxm-mono-01's.

---

Back to [README.md](README.md)
