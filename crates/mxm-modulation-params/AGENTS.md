# AGENTS.md — crates/mxm-modulation-params

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The parameter and interface half of the collection's modulation routing:
`plans/plan-modulation-routing.md` (in the private archive) §4.2 and §4.3. It owns the shape of a
route's two parameters, the reading of them into the arrays
[`mxm-modulation`](../mxm-modulation/AGENTS.md) wants, and the interface for a target's routes —
including **the gesture that removes one**.

# Ownership

## A second crate, not a feature

It names nice-plug and egui, so it sits at **1.95**; the routing half must stay at **1.87** so every
`*-dsp` crate can take it without inheriting the GUI floor. **Never turn the split into a feature
gate**: Cargo unifies features across a build graph
([NOTES.md § Why a second crate](NOTES.md#why-this-is-a-second-crate-and-not-a-feature)).

## What a plugin still declares for itself

Its own `#[derive(Params)]` structs. The derive needs concrete fields and every instrument's source
list is its own, so a generated one is not on offer — `mxm-mono-08` already builds 120 permanent
routing ids from two small reusable structs and one `#[nested(id_prefix = …)]` per destination, and
that is the shape. **What is shared is everything around those fields**, which is what this crate is.

# Local Contracts

## Removing a source from a target is one parameter write

[`remove`] clears that pair's presence and **deliberately does not touch the amount**. Three things
follow, and all three are the point:

- **Re-adding restores the depth the player last set** — design system §8.7's *"removing an
  assignment is undoable"*, with no undo stack to maintain.
- **The player reads it as a step lock, not a preset load.** One distinct parameter id in a batch is
  somebody sequencing; two or more is a patch arriving (`docs/adding-an-instrument.md` gotcha 4). A
  removal that also zeroed the amount would be two, and would rewrite the sequence patch.
- **The DSP ignores an absent pair's amount entirely**, which is `mxm-modulation`'s contract, so the
  dormant depth cannot contribute anything while the route is gone.

[`add`] is the same gesture in reverse and is likewise one write.

## Rows come from parameter values, never from editor state

A present pair is a row; the sources not yet present are what `‹ modulate ›` offers. So a preset
fully determines what the panel shows, and there is no editor-only routing state — which is what
`mxm-mono-08`'s temp-data picker is, and what makes its layout cache need an explicit invalidation
that this does not.

## A row ends in a remove, not a switch

- The row ends in `mxm_ui::control::remove_mark`, a cross — never a toggle reading `On` (the owner:
  *it is implied that it is on when visible*).
- It is still a parameter control: it registers with the keyboard cursor, takes a focus ring, carries
  `marked`, and is named for the accessibility tree — *"Remove LFO from Cutoff"*.
- **Unframed, and bottom-aligned to the slider's track**, not centred on the row: the cross is
  allocated the track's `MIN_TARGET` square. Why: [NOTES.md § Why a row ends in a remove](NOTES.md#why-a-row-ends-in-a-remove-not-a-switch).

## The target's line titles the box, and each row names its source

The owner's rules ([NOTES.md § The box title and the row names](NOTES.md#the-box-title-and-the-row-names-in-full)):

- **A target with nothing routed draws no group at all** — one line, no border.
- **The target's own line, *"Cutoff ‹ modulate ›"*, is the one title of the box**, in the body style
  and primary ink; each row reads only its source, in the caption style and secondary ink
  (`ParamView::quiet_label`). With every source routed, the line keeps the name without the menu.
  The canonical *"Cutoff from LFO"* stays each row's accessible name, tooltip and host name.
- **The target names the menu**, on the selector's own label.
- **A row's track holds `TRACK_MIN`**, and **a row's reading never draws over its source**
  (`mxm_ui::control::slider`).

## `stack` sizes itself from the `ui` it is given

- It takes no width. The row reserves `control::REMOVE_SIZE` before handing the rest to the slider,
  and caps the slider's `Ui` as well as its track.
- **A consumer must measure that** from the painted shapes' own bounding rects, with every route
  revealed — never `min_rect`, which is clamped to the rect it was given.
  `plugins/mxm-mono-01/src/editor.rs`'s `every_card_fits_its_floor_with_every_route_revealed` (in
  mxm-mono-01) is the pattern.
- **`stack_size` states that size without drawing** (`mxm_ui::tree`): its narrowest is the widest row
  *any* source could draw at its widest reading, inside the group's inset, or the target's line if
  wider; its height is what the patch draws now. Every plugin's `tree_checks` hold both against the
  drawing. Detail: [NOTES.md § How `stack` sizes itself](NOTES.md#how-stack-sizes-itself-and-how-a-consumer-measures-it).

## A target has two names: the one it is called and the one the card paints

- `stack` takes both. **`target`** is canonical — the parameter's name, the host's automation list,
  the accessibility tree. **`panel`** is what is drawn, and may drop a prefix the card already
  carries (design system §7.1's rule for `Bound::panel_label`). The short name is the box's title
  ([NOTES.md § The two names](NOTES.md#the-two-names-of-a-target-and-what-the-long-one-cost)).
- **Both name what the target moves** — *Pitch*, *Cutoff*, *Amplitude* — never *Modulator* or a
  jack's name, because the name is on every row (`plugins/AGENTS.md`, *Declaring the target list*;
  [`docs/plugin-conventions.md`](../../docs/plugin-conventions.md#declaring-the-target-list--what-mxm-mono-00-had-to-discover-twice)).
- **It is not an alias.** It never changes with the source, and it never becomes the name anything
  reads back. A caller with nothing to drop passes the same string twice, which is what three of the
  four consumers do.

## The source is a label, not a menu

A target has one slot per source, so a row *is* its source and there is nothing to re-point. Rows
read *"Cutoff from LFO"*. This is the difference that removes the enum-normalisation hazard: no
routing menu exists to grow, so a later source appends new parameters rather than re-pointing every
stored value.

## An amount is signed and drawn bipolar

Its centre is *no modulation*, so a half-filled track would read as "half on" when it means "off" —
`mxm_preset::binding`'s `Bound::bipolar` records that as a shipped defect once already.

## An amount keeps its own step unless its plugin declares a law

A row's keyboard step is the amount's own (`ErasedParam::stepping`) through `stack`. A plugin
whose target reads as a pitch passes `stack_with_law` a law per route — `mxm-mono-08`'s pitch
routes, `StepLaw::Interval` at the same reach its reading multiplies by (owner, 2026-09-23) — and
the row hands it to the slider as its `NextValue`. Per route, because a reach is a *(target,
source)* pair's; only the plugin knows it, since the reading is its own formatter. Every other
instrument keeps `stack` and the owner's earlier exclusion of route amounts from the laws.

## A route's reading is formatted once — `reading`

- **What a route reads is what it delivers, in its target's unit** — `+12.00 st`, `−4.00 oct`,
  `+20 %/oct` for a Key route. `reading` is the one formatter, its parse and its parameter
  (`amount_param`), for every instrument; never a per-plugin copy
  ([NOTES.md § Why a route's reading is formatted once](NOTES.md#why-a-routes-reading-is-formatted-once)).
- The plugin supplies the `Reach` (its scale table, frame unit and sources' peaks), and
  `Reach::below` where a machine's law is not a mirror.
- **A unit carries its own separator** — `" st"`, `" oct"`, `" %"`, and `"x"` for a multiplier
  (`TIMES`), which reads as the scan-speed knob it moves does.
- **An amount starts at zero** — `amount_param_at` is for the one exception, a machine route whose
  depth was never a control and so has no zero to inherit (`mxm-mono-03`'s accent outputs).
- **The fader follows the offer** (`Fader::for_offer`): a one-sided pair gets only its live half;
  **square-law travel only for a machine pitch column wider than ±24 semitones**; every other amount
  is linear. `tests/readings.rs` round-trips every fader through the host's conversion.

## A route's reading never prints a negative zero

- **Every route's reading formats its number through [`signed`]**, never through `{:+.N}` directly:
  `-0` is not idempotent through the host's conversion, and `clap-validator`'s `param-conversions`
  fails only when its random values land either side of zero, so one clean run proves nothing.
- A factory file that stored such a zero's text is regenerated with it.
- **A consumer proves its own readings with a round trip through the host's conversion at amounts
  either side of zero**, not only at the round numbers
  ([NOTES.md § How the negative zero was found](NOTES.md#how-the-negative-zero-was-found)).

# Work Guidance

- **Do not add a per-route modifier.** A gesture scaling a route — *the LFO through the wheel* — is a
  multiplier module (`mxm-modulation`'s `product`), not a third parameter on every pair. Adding one
  would double the automation surface for something two instruments want.
- **Keep `stack`'s rows derived.** The moment a row's visibility comes from anything but a parameter,
  a preset stops determining the panel and the layout cache needs telling.

# Verification

```bash
cargo test -p mxm-modulation-params
cargo clippy -p mxm-modulation-params --all-targets
```

**What is not covered:** anything drawn. The interface here is judged by eye under design system §15
and by the owner at the pilot's gate, not by these tests.

# Child DOX Index

No child AGENTS.md files.
