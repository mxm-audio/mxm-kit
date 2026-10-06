# The MXM Control Map

**Normative.** One MIDI controller edits every instrument in the collection, and a knob means the
same thing on all of them.

The data lives in two files: [`control-map.json`](../crates/mxm-control-map/control-map.json) in
`crates/mxm-control-map` is the collection standard, and each instrument ships its own
`control-map.json` beside its bundle. That crate also holds the schema both files are read with.

---

## 1. The contract

Sixteen knobs.

- **Eight fixed knobs that never change meaning.** No paging, no modes. These are the controls a
  player reaches for without looking.
- **Eight paged knobs** that reach everything else. A page holds the same role in the same slot on
  every instrument, whether or not that instrument has it.

The paged half is contextual by design — "knob 3" means something different on a different page, and
the player always shows which page is active. The fixed half is what makes the promise
unconditional where it matters most.

## 2. The fixed knobs

The MMA Sound Controllers, so a GM2-aware controller works with **no configuration at all**.

| CC | MIDI 1.0 / GM2 meaning | Role |
|---|---|---|
| 74 | Brightness | `filter.cutoff` |
| 71 | Timbre / Harmonic Intensity | `filter.resonance` |
| 73 | Attack Time | `amp_env.attack` |
| 75 | Decay Time | `amp_env.decay` |
| 72 | Release Time | `amp_env.release` |
| 5 | Portamento Time | `voice.glide` |
| 76 | Vibrato Rate | `lfo1.rate` |
| 77 | Vibrato Depth | `lfo1.to_pitch` |

**CC 14–21 are aliases for the same eight roles, in the same order.** This is the factory MIDI
template of NI's Komplete Kontrol M32 and A-series (knob page 1), so those controllers also work
with no configuration: knob 1 is cutoff, knob 8 is vibrato depth. The aliases sit in the block MIDI
1.0 leaves undefined or general-purpose (14-15 undefined, 16-19 General Purpose 1-4, 20-21
undefined), so they collide with nothing standard. An alias is an ordinary `fixed` entry pointing at
an already-claimed role - the map is keyed by CC, so nothing special holds them together.

A happy accident worth knowing: the M32's knob page 3 is CC 102-109, which is exactly the paged
bank below - a stock M32 reaches the whole collection standard from its factory template.

## 3. The paged bank

**Controller pages are not GUI pages.** Resizing/zooming an editor derives its visual pages but
never changes these roles, slots, CLAP remote-control pages or CC 110/111. The opt-in developer
CC 119 instead addresses 0 Performance, 1 Modulators, 2 Sequencers, 3 Generators, 4 Tone,
5 Effects, or 127 Parameters; other values are ignored. It selects the category's first card,
not a visual page number. See `plugins/AGENTS.md` (the rule is
[`plugin-conventions.md`](plugin-conventions.md#a-developer-channel-in-every-editor)'s) for the developer-channel gate.

Slots on **CC 102–109**, page down **110**, page up **111** — from the block the MIDI spec leaves
explicitly undefined, so nothing collides with GM, GM2, or the channel-mode messages.

| # | Section | Page | On mxm-mono-01 |
|---|---|---|---|
| 1 | Oscillator | Osc 1 | 5 of 8 |
| 2 | Oscillator | Osc 2 | **empty** |
| 3 | Mixer | Mixer | 5 of 8 |
| 4 | Filter | Filter | 5 of 8 |
| 5 | Envelope | Amp | 4 of 8 — `amp_env.trigger` appended in slot 5 for `mxm-mono-02` |
| 6 | Envelope | Filter | **empty** — mxm-mono-01 shares one envelope |
| 7 | LFO | LFO 1 | 4 of 8 |
| 8 | Voice | Voice | 5 of 8 on mxm-mono-01; 8 of 8 declared |
| 9 | Effects | Chorus | **empty** — the mono instruments have no effect; `mxm-poly-06` fills one slot, `mxm-mono-00` three more, `mxm-chorus-06` the next three |
| 10 | LFO | LFO 2 | **empty** on `mxm-mono-01`; `mxm-mono-00` fills two slots |
| 11 | Effects | Delay | **empty** on every instrument — `mxm-bucket-delay` established it at 7 of 8; `mxm-fx-delay` fills the reserved Freeze slot |
| 12 | Effects | Shimmer | **empty** on every instrument — the page `mxm-shimmer` needed, 7 of 8 |
| 13 | Effects | Classic verb | **empty** on every instrument — the page `mxm-classic-verb` needed, 7 of 8 |

mxm-mono-01 fills 27 slots and leaves two pages empty. A two-oscillator, two-envelope instrument fills
them, and **nothing moves**. That is the whole point.

**`voice.accent_depth`, `voice.accent` and `voice.slide` were appended to the Voice page's free
slots** for `mxm-mono-03`. Its accent is two things — a depth, and a per-note engagement a sequencer
locks, and one control cannot be both, since a depth knob turned up would accent every note. Slide
is the second of the machine's two per-step buttons and needs no depth at all: the slide time is
fixed in hardware, so the role is a switch. Appending into free slots moved nothing, and the Voice
page is now full.

**`filter.hpf`, `filter_env.polarity` and `fx.chorus` were added for `mxm-poly-06`.** The HPF into
the Filter page's one free slot; the envelope's polarity switch into the Filter-envelope page, which
is otherwise empty on a one-envelope machine and is where *the envelope as applied to the filter*
belongs; and the chorus onto a **new, appended** page 9 — the standard had no effects section, pages
may only append, and it is the page every later built-in effect fills. The saw and pulse switches
needed no new role: they fill `mixer.src1` and `mixer.src2` as mono-01's saw and pulse *levels* do,
since on a machine with switches the switch is the source's level. Since `mxm-poly-06`'s routing
conversion its polarity is the sign of the (Cutoff ← Envelope) route's amount, so it cannot claim
`filter_env.polarity` at all; the role stays for a machine whose polarity is a switch.

**`fx.phaser`, `fx.delay` and `fx.reverb` were appended into page 9's free slots, and `lfo2.rate`
and `lfo2.shape` onto a new, appended page 10, for `mxm-mono-00`** — the first instrument with two
LFOs and the first with more than one effect. Page 10 puts LFO-2's two roles in the slots LFO 1's
rate and shape occupy, so "knob 1 on an LFO page" means rate on both. **Page 9 keeps its `Chorus`
label** although it now holds four effects: the page name is reported to hosts through the
remote-controls extension, so renaming it is a compatibility change of its own and is not made in
passing — it is recorded here as debt for a page-naming policy. `mxm-mono-00` fills no `lfo2.to_*`
or `lfo2.delay` (its second LFO has no depths of its own and no delay), and it deliberately **does
not fill `filter.hpf`** — see §5. **No instrument fills `lfo1.sync` or `lfo2.sync` yet**, although
since 2026-09-25 every LFO rate in the collection has its own tempo sync
(`plans/plan-tempo-sync-controls.md`, in the private archive); mapping them is a control-map change of its own, not made in
passing.

**`amp_env.trigger` was appended into the Amp page's first free slot for `mxm-mono-02`** — the
GATE+TRIG / GATE / LFO switch that decides what fires the envelope. Stepped. The role is shared by
construction: `mxm-mono-01`'s legato / retrigger switch and `mxm-mono-00`'s two three-position
trigger modes are the same control on other machines. It also settles a role that had two
meanings: `mxm-mono-01`'s map fills `voice.note_priority` with its `retrigger` parameter, while
`mxm-poly-06` fills it with a real key-assign mode. **`voice.note_priority` is key assign; trigger
mode lives on `amp_env.trigger`.** mono-01's map is out of step until its own plan moves the row
(`plans/plan-mxm-mono-02.md` §8, in the private archive); no instrument's behaviour changes until then. `mxm-mono-02` leaves
`filter_env.polarity` unfilled: since its routing conversion the envelope's polarity is the sign of
its (Cutoff ← Envelope) route's amount, and the follower is a source of its own. It leaves
`osc1.pwm_source` and `osc2.pwm_source` unfilled too, the source switch having become two routes,
and since 2026-09-27 the two `pwm_depth` roles, when its init patch stopped routing anything to the
pulse width.
`mxm-para-07` leaves the same three unfilled for the same reasons since its own conversion, and
repoints `osc1.pwm_depth`, `osc2.pwm_depth`, `filter.key_track` and `lfo1.to_amp` at the route
amounts its init patch wires, so none of those roles is dead on a fresh instance. `mxm-mono-00`
(its Rev 3, 2026-09-22) repoints `filter.key_track` and `lfo1.to_amp` the same way and leaves the two
`pwm_source` roles, the two `pwm_depth` roles — its init patch routes nothing to a pulse width — and
`lfo1.to_pitch` unfilled: its vibrato reached both oscillators, and one parameter cannot drive two
routes.

**`fx.chorus_rate`, `fx.chorus_depth` and `fx.chorus_mix` were appended into page 9's slots 5–7 for
`mxm-chorus-06`**, the collection's first standalone effect (2026-09-04) — the JUNO-106's chorus
unlocked, whose controls are the quantities the circuit fixes. It **leaves `fx.chorus` unfilled**: that
role is stepped, the built-in's Off / I / II / Both switch, and the standalone has no switch — Mix at
zero is Off — and an existing role's curve is never changed (§5). Rate's curve is `log`, a frequency;
depth and mix are `linear`. Noise is off the controller: a setup control, not a performance one.
The roles entered the standard in the same change as the plugin, so it claims them (§9).

**`fx.reverb_tank` was appended into the Effects page's last free slot for `mxm-folded-spring`**,
the collection's second standalone effect (2026-09-04) — the System-100 103's spring reverb with the
tank as a control. Its Level fills `fx.reverb`, which already existed for the built-in spring and is
the same quantity, so no role was added for it; the tank is a new stepped role of three positions,
and it is a performance control rather than a setup one, which is what earns a slot. **The Effects
page is now full.** The next effect that wants a role of its own needs a page, not a slot, and
adding one is a decision about the controller's geography rather than a line in a file.

**A second Effects page, `Delay`, was added for `mxm-bucket-delay`** (2026-09-06) — the collection's
third standalone effect, and the first built from a device family rather than promoted out of an
instrument. That decision was the owner's, whose ruling was *keep all controls on one page if at all
possible*, and one page was possible because **not every parameter earns a role**:

- **Mix fills the existing `fx.delay`.** It is the same quantity — how much delay is in what you
  hear — and `mxm-folded-spring` set the precedent for not minting a role that already exists. The
  role stays where it is, on the Effects page. That this plugin's control is a *crossfade* where the
  spring's is an added level does not change the role: a role names the quantity, not the law.
- **Seven new roles take the new page**: `fx.delay_time` (log — it is a clock), `fx.delay_feedback`,
  `fx.delay_line` (stepped, four chips), `fx.delay_spread`, `fx.delay_bias`, `fx.delay_wobble` and
  `fx.delay_wobble_rate` (log).
- **The eighth slot was left free, deliberately.** `mxm-fx-delay` later filled it with
  `fx.delay_freeze`, the same captured-recurrence performance act as its panel Freeze. This appends
  one role into reserved space; no existing role or page moves.
- **Eleven of that plugin's nineteen parameters are unmapped**: the six tap faders, Return, Filter,
  Routing, Reverse and Sync. They are the shape you set and store in a preset, not the controls you
  reach for while it plays — and that split is what made one page enough. `Division` was a twelfth
  until 2026-09-06, when `Time` took over selecting the subdivision and the parameter went away, so
  `fx.delay_time` now drives the synced division too.

**`fx.delay_freeze` filled the Delay page's reserved eighth slot for `mxm-fx-delay`**
(2026-09-17), the collection's general-purpose original delay. Its Mix fills `fx.delay`; Time,
Feedback, Offset, Motion and Rate reuse the page's existing quantity roles. Model and Character do
not claim the device-family Line and Bias roles: those are a bucket chip choice and a clock bias,
not generic aliases. Freeze is the only new performed quantity, stepped, and filling the free slot
moves nothing.

**A third Effects page, `Shimmer`, was added for `mxm-shimmer`** (2026-09-08), the collection's
fourth standalone effect and first original pitch-shifted feedback reverb. This addition also made
the until-1.0 correction recorded in §5: `LFO 2`, which existed first, is page 10; `Delay` moves to
page 11; and the new `Shimmer` page appends as page 12. Every shipped product consumes the same
collection file, so the correction applies to all instruments and effects in one pass.

- **Mix fills the existing `fx.reverb`.** It is the same performed quantity — how much reverb is in
  what you hear — and follows the spring and delay precedents rather than minting a duplicate role.
- **Seven new roles take the new page**: `fx.shimmer_amount`, `fx.shimmer_regen`,
  `fx.shimmer_shift`, `fx.shimmer_placement`, `fx.shimmer_reverse`, `fx.shimmer_size` and
  `fx.shimmer_freeze`. Shift remains linear so all semitone positions are reachable; placement,
  reverse and freeze are stepped.
- **The eighth slot is deliberately free.** Diffusion, Low Cut, High Cut and Pre-delay are the stored
  shape of the space rather than performed controls. They remain available on the panel and to host
  automation without displacing a live control.

**A fourth Effects page, `Classic verb`, was appended as page 13 for `mxm-classic-verb`**
(2026-09-14), the collection's everyday algorithmic reverb. The page was decided with the plugin's
panel in `docs/briefs/mxm-classic-verb.md`, and its roles entered the standard with the plugin, so
its map claims them (§9).

- **Mix fills the existing `fx.reverb`**, for the reason the spring and shimmer maps give.
- **Seven new roles take the new page, in the brief's order**: `fx.verb_space` (stepped),
  `fx.verb_decay` (log), `fx.verb_size` (log), `fx.verb_predelay` (log), `fx.verb_bass` and
  `fx.verb_treble` (bipolar), and `fx.verb_shape` (stepped). The two multipliers are bipolar
  because their neutral ×1 is the middle of their travel, and a bipolar curve's exact centre detent
  is how a controller knob puts one back on the space as fitted. The space selector will grow to
  about a hundred positions when its spaces are fitted; a 7-bit controller still reaches each one.
- **The eighth slot is deliberately free.** Diffusion, the modulation pair, Width, the tone pair,
  Early and late and Ducking are shape controls, set once for a space; they stay on the panel and
  automatable without displacing a performed control.

**A fifth Effects page, `Dynamics`, was appended as page 14 for `mxm-fx-curve`** (2026-09-17).
This original effect has one automatable control and a durable authored model rather than a panel of
host parameters.

- **Mix fills the new `fx.dynamics` role.** It means how much of the complete serial curve stack is
  in what the player hears. Reusing delay or reverb Mix would give the physical knob the wrong
  meaning.
- **Seven slots remain deliberately free.** Points, point modes, handles, stage order and detector
  ballistics are saved model state. Turning them into roles would contradict the product's one-
  parameter host contract and make a fixed controller address mean whichever stage is visible.

An empty slot is inert. It is never an error, and never quietly reassigned to a neighbour — a wrong
mapping is worse than no mapping, because it is silent.

## 4. Reserved controllers

Never mappable, always forwarded to the plugin. Enforced in
`control_map::schema::RESERVED_CCS`; a layout claiming one is refused whole.

| CC | Why |
|---|---|
| 120 | All Sound Off — the player's panic and recovery machinery depends on it |
| 123 | All Notes Off — likewise |
| 1 | Mod wheel: **live performance modulation**, which a plugin consumes without writing any parameter. Mapping it would turn a gesture into an edit |

CC 7 (Channel Volume) is deliberately **not** in the map. Hosts and control surfaces already drive
it as a mixer level; an instrument's output gets a page slot instead.

## 5. Local Contracts

### An instrument owns its own mapping

The collection standard — roles, fixed knobs, bank, pages — is compiled into the player from
`crates/mxm-control-map`. **Which
parameters fill those roles ships with the instrument**, as `control-map.json` in the plugin's
crate, staged beside the bundle by `cargo xtask bundle`.

Nobody is obliged to install the whole collection. Someone who downloads one plugin must still get a
working controller layout, and the player must not need to have heard of an instrument released
after it.

### Roles are permanent; pages append and never insert

**Until 1.0, the standard may be rearranged — the owner's ruling, 2026-09-08.** Nothing in this
collection has been released, so no controller template and no saved project depends on this layout
yet, and the permanence below is a promise about the *released* standard rather than a constraint on
reaching it. Until 1.0 a role may move, a page may be reordered and a mistake may be corrected
rather than logged.

**The condition is the whole of it: a change is made across every instrument and effect at once, or
it is not made.** Consistency is what the standard is for, and a layout that is one shape on six
instruments and another on three is worse than either shape. So a rearrangement is a single deliberate
pass over `crates/mxm-control-map/control-map.json`, every plugin's map, and the pinned page-count test — never a
per-plugin convenience.

**At 1.0 this closes** and everything below binds permanently. Two consequences hold today, because
they are mechanical rather than social: appending is still the cheapest change, and the pinned count
still has to move with the pages.

A role name is a compatibility surface. Renaming one breaks every instrument map that uses it, and
every controller template somebody has built.

Pages append for a mechanical reason as well as a social one: nice-plug numbers remote-control pages
positionally (`page_id: self.pages.len()`, `wrapper/clap/context.rs:296-297`), so inserting a
section renumbers every later page.

**Appending a page changes the shipped standard, and the player pins the count.**
`control_map::schema::tests::the_shipped_layout_is_valid` asserts how many pages the compiled
standard has, so a new page is a deliberate act that updates the test with it — page 9 did — and
never a silent one.

### A role's metadata is compatibility surface too

A role's `curve` is compiled into every player that has shipped, and `control_map/curve.rs` maps a
`stepped` role over a continuous 0–1 parameter to **two positions**. So changing an existing role's
curve does not fail on an old player — it loads, and that one knob lies, which is worse than the
whole-map refusal §9 describes. **An existing role's curve is never changed.** An instrument whose
control does not fit a role's declared curve leaves the role unfilled and says so in its map's
comment; a new role with the right curve is the amendment, when a slot exists for it. `mxm-mono-00`'s
continuous HPF against the stepped `filter.hpf` is the case that established this.

### Every page holds eight slots or fewer

nice-plug auto-splits a longer page into `"{name} {n}"` (`context.rs:351-365`), silently renaming a
page the map keys on. Validation refuses a ninth slot rather than letting that happen.

### Parameters are referenced by permanent id, never by display name

A reference is either a nice-plug **string id** (`"cutoff"`), hashed to the CLAP id the host sees,
or a bare number used as a CLAP id directly. Display names stay free to change; two oscillators can
both be called "Tune" without ambiguity.

### The map is data. Adding an instrument never means code

`t5_control_map.rs::a_new_instrument_gets_the_collection_layout_by_adding_a_file_and_nothing_else`
asserts this directly, against an instrument that does not exist and parameters the player has never
seen. If it ever needs a code change, the design has failed.

## 6. Resolution, and the surprise in it

**nice-plug reports parameters to the host as normalised `0.0..=1.0`** — `min_value = 0.0`,
`max_value = step_count.unwrap_or(1)` (`vendor/nice-plug/src/wrapper/clap/wrapper.rs:3455-3459`;
since the split, [`src/wrapper/clap/wrapper.rs:3760-3764`](https://github.com/mxm-audio/nice-plug/blob/main/src/wrapper/clap/wrapper.rs#L3760-L3764) in the nice-plug fork) —
with the plugin's own skew applied inside.

So for every MXM plugin, mapping a controller linearly across that range **inherits the plugin's own
curve**, which is exactly right. A host-side log curve on top would double-apply it.

The curves in the standard are still meaningful: CLAP permits plain ranges, and a third-party plugin
reporting 20–20 000 Hz genuinely needs one, because `clap_param_info` carries no skew hint at all.
One rule covers both — a log curve over a range starting at zero falls back to linear, and the
normalised case is precisely that.

## 7. Resolution is 7-bit, and that is accepted

128 steps. Across a ten-octave cutoff that is roughly one step per semitone, and parameter smoothing
does the rest. 14-bit CC via MSB/LSB pairs is defined only for CC 0–31, so **not** for the Sound
Controllers; NRPN is out of scope.

## 8. Verification

[`apps/mxm-player/tests/t5_control_map.rs`](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/tests/t5_control_map.rs)
in mxm-player, plus unit tests in its `control_map::{curve, takeover,
schema}`. What is actually proven:

- a new instrument gets the layout by adding a file only;
- one broken instrument map does not disable the controller for the others;
- reserved CCs can never be claimed, and an unclaimed CC still reaches the plugin;
- a claimed CC edits the parameter and does **not** also reach the plugin's own CC handling;
- a role an instrument does not have is inert;
- a knob away from its parameter does not jump it, and catches up by crossing;
- a gesture opens once, closes when the knob goes quiet, and closes on every teardown path;
- a malformed reload keeps the map that was working.
- every product's own map holds to the standard it is built against: `cargo xtask bundle` refuses
  to build otherwise (`mxm-xtask`'s `control_maps::check`), which keeps §9's rule below.

## 9. An unknown role rejects the whole map, and that is a defect

`Layout::check_instrument` returns on the **first** role the compiled standard does not declare, and
`load_instrument_map` propagates it — so one unknown role costs an instrument **every** mapping in
its file, not just that entry.

That contradicts §3's own promise that *"an empty slot is inert. It is never an error"*, and it
undoes §5's *"the player must not need to have heard of an instrument released after it"* — which
holds for parameters, but **not for roles**, because the standard is compiled into the player.

Until an unknown role is inert rather than fatal, **an instrument ships a map naming only roles the
standard already declares**. `mxm-mono-03` does exactly that: its `control-map.json` leaves the two
accent roles unclaimed and says so in its own comment, so it keeps a working layout on any player.
Accent stays reachable from the panel, from host automation, and from per-step modulation.
**`mxm-poly-06` does the same** for `filter.hpf` and `fx.chorus` — and the
chorus is the one control a player of that instrument would want on a knob, which is what this
defect now costs. **Two instruments ship restricted maps, and `mxm-mono-00` will be the third** — losing its
second LFO and all three effects on a controller until the fix lands. **The loader fix is overdue**,
and `plans/plan-mxm-poly-06.md` §8 (in the private archive) proposes it as a plan of its own.

**`mxm-mono-02` claims `amp_env.trigger`, the role added with it, and that is not the exception it
looks like.** The restriction above protects players built before a role existed. `amp_env.trigger`
entered the standard in the same change as the instrument, so no player compiles in this
instrument's existence without it; withholding the role would protect nobody and cost the trigger
switch on every player there is. The rule, with its reason: **a map claims every role the standard
declared at or before the instrument's own baseline**, and the restriction the siblings pay is for
roles added *after* theirs. The loader fix stays overdue for those.

`t5_control_map.rs::a_new_instrument_gets_the_collection_layout_by_adding_a_file_and_nothing_else`
does **not** cover this: its fixture names only roles the standard already declares, so it exercises
a new *instrument*, never a new *role*.

## 10. Not done

Deliberately, and none of it blocking:

- **`remote_controls()` export.** CLAP's own extension carries the page taxonomy to a DAW —
  `section_name`, `page_name`, eight parameter ids. It carries **no CC numbers and no navigation**,
  so it exports the semantic layout, not this document's controller bindings. The zero-setup
  guarantee is MXM Player only.
- **Third-party plugins** that ship no map: module-qualified name matching, ambiguity left unmapped.
- **MIDI learn.**
- **Linear depth roles on signed route amounts.** `filter.key_track`, `filter.lfo_amount`,
  `lfo1.to_filter` and `lfo1.to_amp` are `linear`, and six instruments (`mxm-mono-00`, `-01`,
  `-02`, `-pr1`, `mxm-poly-06`, `mxm-para-07`) bind them to route amounts whose zero is the
  **centre** of their 0–1 travel, so the bottom half of the knob inverts the depth. The player
  cannot tell: nice-plug reports every continuous parameter as 0–1 with its own skew inside
  (`vendor/nice-plug/src/wrapper/clap/wrapper.rs:3455-3459`; since the split, lines 3760–3764 of
  the nice-plug fork's). The fix is either a binding option
  that maps from the centre — which an old player's `ParamRef` cannot parse, so it would refuse the
  instrument's whole map — or unfilling the roles. The owner left it as it is, 2026-09-22.
