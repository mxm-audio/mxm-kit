# NOTES.md — crates/mxm-modulation-params

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

*Since the split (2026-10-06):* `plans/…` cited here is in the private archive; a `plugins/<plugin>/…`, `crates/<plugin>-dsp/…` or `apps/<plugin>-standalone` path is in that product's own repository, and a rule cited from `plugins/AGENTS.md` is in full in [`docs/plugin-conventions.md`](../../docs/plugin-conventions.md).

## Why this is a second crate and not a feature

It names nice-plug and egui, so it sits at **1.95**; the routing half must stay at **1.87** so every
`*-dsp` crate can take it without inheriting the GUI floor. A feature gate would not have held that
line, because Cargo unifies features across a build graph — `docs/adding-an-instrument.md` gotcha 10
records what that already cost here once, when a standalone harness's feature put cpal and a libjack
link into a shipped `.clap`. Two crates is the cheap answer to a trap the repository has already
sprung.

## Why a row ends in a remove, not a switch

`mxm_ui::control::remove_mark` — a cross. The first version put a toggle reading `On` there, and
the owner's answer was the whole argument: *it is implied that it is on when visible*. A row exists
**because** its pair is present, so a switch beside it restated the row and spent twenty more points
of width doing it — enough, on mxm-mono-01's first conversion, to break 1920 into two rows of cards.

It is still a parameter control and must stay one: it clears a presence a host can automate, so it
registers with the keyboard cursor, takes a focus ring, carries `marked`, and is named for the
accessibility tree — *"Remove LFO from Cutoff"*.

**Unframed, and aligned to the track rather than to the row.** The cross follows a slider, which is
already framed, so framing the cross too made it read as a second item instead of as an action on
the slider before it — the owner's ruling, and the same one the sampler's layer chips got. It is
also bottom-aligned: a slider stacks its name/value line over its track, so the row is taller than
the cross, and centring the cross — which is what `ui.horizontal` does — floated it half a name
line above the track it belongs to. The track is the last `MIN_TARGET` of the slider's column and
the cross is allocated the same square, so aligning their bottoms puts the two boxes on each other.

## The box title and the row names, in full

The owner's rules, each of which removes something rather than adding it:

- **A target with nothing routed draws no group at all.** An empty bordered box is a frame around
  nothing; it reads as a control that failed to load, not as an invitation. An unrouted target costs
  one line and no border.
- **The target's own line, *"Cutoff ‹ modulate ›"*, is the one title of the box** its routes are
  drawn in (2026-09-24: *"the dropdown title and dropdown is promoted to the box title"*), in the
  body style and primary ink. Each row inside reads only its source, in the caption style and
  secondary ink (`ParamView::quiet_label`), so the title is the stronger word and the source the
  lesser. The owner had found the old weights *"backwards"*. With every source routed, the line
  keeps the name without the menu. The canonical *"Cutoff from LFO"* stays each row's accessible
  name, tooltip and host name.
- **The target names the menu**, on the selector's own label. A card carrying two stacks otherwise
  offers two identical unlabelled menus, and there is nothing on screen that says which control
  either one reaches until after it is used.
- **A row's track holds `TRACK_MIN`** (six pointer targets, 168 points), so a card that hugs its
  content cannot shrink until a depth is too short to aim. **A row's reading never draws over its
  source** (`mxm_ui::control::slider`).

These were piloted on mxm-mono-08 and rolled out to every editor on 2026-09-24
(`plans/plan-editor-standard.md` R1). Before that, every other editor drew *"Cutoff from LFO"* on
each row, with the target's line as a caption beneath the box. Those long names set the width of
every card that carried a route, and ran into the reading beside them.

## How `stack` sizes itself, and how a consumer measures it

It takes no width. A caller cannot know the group's own margin, and a row that guesses it runs its
removal button past the card's border — which is exactly what the first version did. The row
reserves `control::REMOVE_SIZE` before handing the rest to the slider, and caps the slider's `Ui` as
well as its track, because a slider lays its value out right-aligned across the whole width its
`Ui` has rather than across the number it was passed.

**A consumer must measure that.** A `Ui`'s `min_rect` is clamped to the rect it was given, so a
widget placed past the right edge still reads back as fitting; the honest oracle is the painted
shapes' own bounding rects, with every route revealed rather than at the init patch where no row is
drawn at all. `plugins/mxm-mono-01/src/editor.rs`'s
`every_card_fits_its_floor_with_every_route_revealed` is the pattern, and the card floor it produced
is a routing cost stated where it is paid.

**`stack_size` states that size without drawing** (`mxm_ui::tree`). Its narrowest is the widest
row *any* source could draw, present or not, at its widest reading, inside the group's inset —
every route revealed, so adding one never widens the card under the pointer — or the target's line
if that is wider. Its height is what the patch draws now. Both follow the layout `stack_with_law`
draws: a row is named by its source, its reading beside the name, over a `TRACK_MIN` track, and the
target's line opens the group. Every plugin's `tree_checks` hold both against the drawing.

## The two names of a target, and what the long one cost

`stack` takes both. **`target`** is canonical — the parameter's name, the host's automation list,
the accessibility tree. **`panel`** is what is drawn, and it may drop a prefix the card around it
already carries: *"Pitch"* inside a card titled *Oscillator 2*, where the parameter is *"Oscillator 2
pitch"*. That is design system §7.1's rule for `Bound::panel_label`, applied to a routing row.

**The short name is the box's title, and the cost of the long one is measurable.** Before the
rollout each row painted `<panel> from <source>`, and `mxm-mono-00` paid **a hundred points a
card** for the long form — *"Amplifier envelope gate from Oscillator 1 sync"* against *"Gate from
Oscillator 1 sync"* — which across eleven cards pushed its editor's first musician page down to a
single card. Now only the title carries the panel name, but a long one still sets the width of
every card with a stack.

## Why a route's reading is formatted once

**What a route reads is what it delivers, in its target's unit** — `+12.00 st`, `−4.00 oct`,
`+20 %/oct` for a Key route — and `reading` is the one formatter, its parse and its parameter
(`amount_param`), for every instrument (`plans/plan-modulation-standard.md`). Before it, ten
`routes.rs` files carried their own copy, and a unit printed differently on one instrument was a
modulation that read differently there. The plugin supplies the `Reach`, because only it knows its
scale table, its frame unit and its sources' peaks — and, where a machine's law is not a mirror, a
negative half of its own (`Reach::below`: `mxm-mono-02`'s inverted envelope reaches 2.75 octaves
down where it reaches ten up).

**The fader follows the offer** (`Fader::for_offer`): a one-sided pair gets only its live half
(0…+1 or −1…0), and **square-law travel is only for a machine pitch column wider than ±24
semitones**, where a vibrato's cents would otherwise sit in the first sliver of a linear fader;
every other amount is linear. `tests/readings.rs` holds the shared reading to the copies it
replaces and round-trips every fader through the host's conversion.

## How the negative zero was found

**Every route's reading formats its number through [`signed`]**, never through `{:+.N}` directly.
`format!("{:+.0}", -0.004)` prints `-0`; a host parses that to zero and prints `+0`, so the text is
not idempotent through the host's conversion and `clap-validator`'s `param-conversions` fails — but
only when its random values land in the sliver either side of zero, so one clean run proves nothing.
`mxm-mono-03`'s routing conversion found it, and `mxm-mono-00`, `mxm-mono-pr1`, `mxm-mono-02` and
`mxm-poly-06` had the same format. **Only a reading that shows zero changes**: every other value
prints exactly as `{:+.N}` does (`tests/readings.rs`). A factory file that stored such a zero's text
does change, and is regenerated with it: `mxm-mono-02`'s negative full scales had put `-0.00 st` and
`-0 %` into all fifty of its files. A consumer proves its own readings with a round trip through the
host's conversion at amounts either side of zero, not only at the round numbers.
