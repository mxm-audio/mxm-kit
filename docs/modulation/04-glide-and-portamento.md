# 4. Glide and portamento

The pitch lag between one note and the next. A one-pole filter in the pitch path, which makes it the
same arithmetic as [§3's](03-smoothing-and-events.md) parameter smoothing — applied on purpose and
loudly, rather than incidentally and quietly.

**Evidence in this chapter.** The shipped behaviour is read from
`crates/mxm-mono-01-dsp/src/voice.rs` and confirmed by the tests named beside each claim; nothing
here comes from `mod_spike`, which does not measure glide. The TB-303's ≈60 ms figure is
**published, not measured here** — see [§4.6](#46-sources). Every arrival percentage and residual
below is **derived**: arithmetic on the exponential, not a measurement of any instrument.

---

## 4.1 Fixed time or fixed rate

Two mechanisms wear the same knob, and they behave differently everywhere except at one interval.

**Fixed rate** — the pitch travels at *n* semitones per second, so a wide interval takes
proportionally longer than a narrow one. Every glide has the same *slope*.

**Fixed time** — the pitch approaches the target with a time constant that does not depend on the
interval, so an octave and a semitone take the same time. Every glide has the same *duration*.

Analog portamento is almost always fixed time, because the circuit that produces it is an RC lag in
the control-voltage path: a resistor and a capacitor between the keyboard's CV and the VCO's input.
The capacitor charges toward the new voltage exponentially, and an exponential's time constant is a
property of the circuit, not of how far it has to go.

That gives the shape everyone recognises: **most of the movement happens fast, then it eases in.**
A linear ramp arrives with a corner; an exponential arrives asymptotically and never quite gets
there at all.

## 4.2 A time constant is not a duration, and the difference is audible

The one-pole lag is

```text
y[n] = target + (y[n-1] - target) · exp(-1 / (τ · fs))
```

τ is the **time constant**, not the time to arrival. After τ the pitch has covered 63.2% of the
interval; the rest of the journey is asymptotic and, strictly, never finishes.

| Elapsed | Covered |
|---|---|
| 1 τ | 63.21% |
| 2 τ | 86.47% |
| 3 τ | 95.02% |
| 5 τ | 99.33% |

**Derived**, from `1 − e^(−k)`.

The consequence a control has to survive: for an interval of 1200 cents and τ = 60 ms, reaching
within one cent — which is the point a listener would call *arrived* — takes **τ · ln(1200) ≈ 425
ms**, seven time constants. A control reading "60 ms" that takes the better part of half a second to
land is not lying, but it is not answering the question a player is asking either.

| Within | Time, τ = 60 ms | In τ |
|---|---|---|
| 100 cents | 149 ms | 2.48 |
| 10 cents | 287 ms | 4.79 |
| 1 cent | 425 ms | 7.09 |

**Derived**: `t = τ · ln(1200 / cents)`.

## 4.3 Fixed time is why the effect changes character with tempo

This is the part that gets re-derived every time, so here are the numbers.

A 16th note lasts `60000 / (BPM · 4)` ms. The glide's time constant does not move with it. So the
fraction of the interval a slide actually covers before the next step arrives falls as the tempo
rises, and with it the pitch error still outstanding at the boundary:

| Tempo | 16th | Covered, τ = 60 ms | Left over on an octave jump |
|---|---|---|---|
| 90 BPM | 166.7 ms | 93.8% | 0.75 st (75 cents) |
| 120 BPM | 125.0 ms | 87.5% | 1.49 st (149 cents) |
| 140 BPM | 107.1 ms | 83.2% | 2.01 st (201 cents) |
| 160 BPM | 93.8 ms | 79.0% | 2.52 st (252 cents) |
| 180 BPM | 83.3 ms | 75.1% | 2.99 st (299 cents) |

**Derived**, all of it. τ = 60 ms is the published TB-303 figure; the arithmetic is
`1 − e^(−d/τ)` and `12 · e^(−d/τ)`.

Read the right-hand column as the thing that matters. At a slow tempo a slide is **a quick blip at
the front of the note** and the step still spends most of its length in tune. At 160 BPM the same
circuit leaves the note **a minor third flat** at the moment the next step begins — and if that step
also slides, the lag starts its next approach from wherever it happened to be, toward a target that
has already moved.

**Chain several and the line stops landing anywhere.** It wanders between targets, permanently in
transit, and the pitches written in the pattern become attractors the sound leans toward rather than
notes it plays. That drifting quality is a large part of what people mean by the 303 sound, and it
is not a modelled behaviour — it falls out of a fixed time constant meeting a step shorter than
itself.

**A tempo-relative glide time destroys this**, which is why it is a feature and not a fix. Scaling τ
with the step length makes every tempo cover the same fraction, so the right-hand column becomes a
constant and the instrument sounds the same at 90 and 180. Some software emulations offer it. It is
a legitimate control to have; it is not the machine.

## 4.4 Slide is a property of the transition, not of the step

The flag says nothing about what happens *inside* the step carrying it. It says what happens at the
boundary **out of** it.

With slide set on step 5, and step 6 holding a different note:

1. Step 5 plays at its own correct pitch, for its whole duration. Nothing moves.
2. At the boundary into step 6, the pitch begins travelling from step 5's note toward step 6's.
3. Step 6 **begins at roughly step 5's pitch** and slides into its own.

So the audible glide lives at the front of the *following* note. Getting this backwards — treating
the flag as "this step glides" and bending the pitch within step 5 — produces a pattern that sounds
approximately right in isolation and wrong the moment the sequence is compared against the original,
because every slide lands one step early.

**And slide makes the pair legato.** The gate stays open across the boundary, so the second note does
not retrigger the amplitude or filter envelopes. Two slid notes sound like **one continuous note that
changes pitch**: no new attack, no new filter sweep, no accent on the second one.

That second half is not a detail hanging off the first. It is half of what the effect *is* — a slide
with the gate retriggering would be two notes with a pitch bend between them, which is a different
and much more ordinary sound.

## 4.5 What mxm-mono-01 already does

**It is the same mechanism, in shipped code.** `Voice::process` runs

```rust
// crates/mxm-mono-01-dsp/src/voice.rs
let coef = (-1.0 / (p.glide_time_s * fs)).exp();
self.glide_offset = flush((self.glide_offset + (self.glide_target - target)) * coef);
```

which is §4.2's one-pole exactly, with `glide_time_s` as **τ**, and the pitch is
`target + glide_offset`. So every number in §4.2 and §4.3 describes this instrument as well as the
hardware, at whatever τ the `glide` parameter is set to.

**The lag is carried as the remaining distance, not as the pitch.** The formula above, written as
`y = target + (y − target) · coef` in `f32`, rounds every step's decrement to the pitch's ulp and
stops moving once a step is under half of it: at τ = 0.5 s and 48 kHz it came to rest 5 cents short
of middle C, at 2 s 37 cents, twice that at 96 kHz — until the next note. Four instruments shipped
that form until 2026-09-26. The distance keeps full precision down to zero, so the pitch lands.

Four properties worth having written down, because none of them is visible from the control:

- **`glide` is a time constant, and the interface calls it a time.** `params.rs` formats it with
  `v2s_time()` over a 0–2 s skewed range, and the code comment beside it — *"Not smoothed: this is a
  time constant, not a signal"* — is currently the only place the distinction is recorded. A reading
  of 500 ms means 63% of the way in 500 ms.
- **Zero means jump.** `if p.glide_time_s <= 0.0` snaps, rather than running the lag with a
  degenerate coefficient. That is what makes glide an *amount* whose zero is genuinely off, per
  `plugins/AGENTS.md`'s init-patch contract, and it is why the parameter defaults to 0.0.
- **The first note of a phrase does not glide.** The distance is zeroed rather than lagged, and the
  pitch snaps to its target, range switch included, so a patch with a long glide does not open with a swoop up from whatever the previous
  phrase left behind. `first_note_does_not_glide_from_a_stale_pitch` holds it.
- **Legato is already separable from glide, but it is global.** `Retrigger::Legato` — the default —
  restarts the envelope only on the first note of a phrase, which is the gate behaviour §4.4
  describes. mxm-mono-01 exposes it as one parameter for the whole patch. **The 303 makes the same
  choice per step**, and that difference is the entire design question below.

## 4.6 Sources

- **τ ≈ 60 ms for the TB-303's slide** — published, widely reported, and consistent with the RC lag
  in the CV path. **Not measured here, and no hardware was available to measure.** Anyone modelling
  this should treat it as a starting point to be checked against a recording, not as a constant to
  build a test around.
- The gate-held-open behaviour and its consequence for envelope retriggering are described
  consistently across emulations and service documentation; also unmeasured here.
- `research:filters/machines/tb303-diode-ladder.md` §4 lists
  slide and gate behaviour as one of four things the filter does not account for. This chapter is
  where that one lands.

---

## 4.7 Consequences for this repo

1. **The player has both halves of slide, and there is no third step flag — decided and built.**
   `Pattern` carries `tied`, and a tie is *exactly* §4.4's gate behaviour: `Runtime` emits Sound
   then Release at the same frame and `emit_legato_joint` resolves the release first, so the
   instrument sees the new note arrive while the old one is still held. Glide is separately a
   sequenceable parameter — `voice.glide` fills a control-map role and locks are sent as
   `PARAM_MOD`. **A 303 slide is a tied step carrying notes plus a glide lock**, with no new step
   state at all.

   The two questions this section once deferred are settled in
   [`../../apps/mxm-player/AGENTS.md`](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/AGENTS.md): a slide is not presented
   as one flag over the two mechanisms — the step model calls a note-carrying tie a **slide** and
   the glide lock rides it like any per-step lock — and a slide to the *same* pitch is allowed,
   emitting the same legato joint, which this collection's instruments sound as a hold and a
   third-party CLAP is free to treat audibly.

2. **The owner's "glide as part of the synth not the sequencer" is the right call, and §4.4 is
   why.** The pitch lag is a property of the voice — an RC in the CV path, which is downstream of
   whatever is generating notes. What belongs to the sequencer is only the per-transition *decision*
   to engage it. Putting the lag itself in the sequencer would make it unavailable to a player using
   a MIDI keyboard, which the hardware's own signal path does not do.

3. **A per-step legato override is the thing mxm-mono-01 does not have.** `Retrigger` is one
   parameter for the patch. Per-step slide needs the envelope's retrigger decision taken per
   transition, and the player already carries a per-step flag that means exactly that. Whichever
   instrument gets there first should check whether `Retrigger` wants to become locked-per-step
   rather than gaining a parallel mechanism.

4. **If `glide` is ever relabelled, say what the number means.** A control reading a duration that
   is really a time constant is a documentation problem today and a support problem later. Either
   name it as a time constant, or map the control so the displayed number is a stated arrival
   criterion — τ · ln(1200) for "within a cent", from §4.2 — and say which was chosen. Do not
   change the range without deciding this: the two are one decision, in the same shape as the
   calibrated-cutoff trap in
   `research:filters/machines/tb303-diode-ladder.md` §10.5.

5. **A tempo-synced glide option is a real feature and must not be the default.** §4.3 is the whole
   character of the effect; a build that scaled τ with the step length by default would sound
   correct at one tempo and wrong everywhere else, with nothing on screen to explain it.

6. **Nothing here is measured, and the gap is worth closing.** `mod_spike` does not touch glide, and
   `Voice`'s lag is reachable through the DSP crate's public API — so a §5 in that program could
   measure arrival time against τ, confirm the first-note snap, and pin the zero-means-jump branch,
   at the same evidence standard as the LFO and envelope sections. Until someone does, every number
   in this chapter is derived arithmetic or a published figure, and it says so.
