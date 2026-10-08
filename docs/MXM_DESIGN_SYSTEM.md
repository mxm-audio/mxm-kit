# MXM Synth Collection Interface Design System

**Version:** 1.0  
**Status:** Normative for every MXM synthesizer  
**Applies to:** Plugin editors, preset browsers, dialogs, and shared UI components

The MXM Synth Collection is a family of modern software instruments. Its interfaces should feel calm,
fast, and native to a screen—not like photographs of hardware. A user should be able to understand
the signal path at a glance, make a precise change without visual friction, and move between MXM
instruments without relearning the interface.

In this document, **MUST**, **SHOULD**, and **MAY** indicate requirement strength.

---

## 1. Design statement

> **Sound first. Structure made visible. Nothing ornamental.**

The collection takes high-level inspiration from Arturia Pigments' successful software-native
qualities: clear hierarchy, colour-coded modulation, direct visual feedback, progressive disclosure,
and dedicated light and dark themes. It MUST NOT copy Pigments' exact layout, palette, components,
icons, artwork, wording, or trade dress. All MXM assets and component designs must be original.

### Five principles

1. **Instrument, not illustration** — every visible element supports understanding, navigation, or
   sound design.
2. **Simple first, depth on demand** — the common path is immediately available; software-added
   advanced controls may appear through tabs, expansion, or contextual detail. A control reproduced
   from the source instrument's physical panel is part of the instrument, not optional depth.
3. **Show the signal** — waveforms, envelopes, routing, and modulation should explain what the DSP is
   doing rather than merely decorate the editor.
4. **Color has grammar** — accent and modulation colors communicate identity and relationships. They
   are never confetti.
5. **One collection, distinct instruments** — components and interaction remain consistent while
   each synthesizer gets one restrained identity accent and the views its architecture needs.

---

## 2. Explicitly non-hardware

MXM interfaces MUST NOT use:

- Wood, brushed metal, leather, plastic grain, panel wear, reflections, or photographic textures.
- Screws, rack ears, ventilation slots, seams, handles, or other fake construction details.
- Photorealistic knobs, switches, jacks, patch cables, LEDs, or embossed/engraved labels.
- A layout copied from the physical panel of the instrument that inspired the DSP.
- A decorative piano keyboard unless on-screen note input is a real, tested feature.
- Vintage manufacturer logos, model typography, control captions, or recognizable trade dress.
- Perspective, simulated depth, or shadows intended to make controls appear physical.

Hardware architecture MAY inform the sound engine. It does not dictate the interface. Controls are
organized by user task and signal flow, not by the position they occupied on an original panel.

A useful test: **if a visual element could be removed without reducing meaning or usability, remove
it.**

---

## 3. Collection anatomy

Every instrument uses the same high-level shell.

```text
┌──────────────────────────────────────────────────────────────────────┐
│ Product / preset                         utility actions / output     │  App bar
├──────────────────────────────────────────────────────────────────────┤
│ Play       Synth       Mod       FX       [instrument-specific]      │  View bar
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│                    task-oriented workspace                           │  Main view
│                                                                      │
├──────────────────────────────────────────────────────────────────────┤
│ contextual modulation sources / macro controls                      │  Context rail
└──────────────────────────────────────────────────────────────────────┘
```

### 3.1 App bar

The app bar is persistent and MUST contain, in this order where practical:

1. MXM wordmark and product name.
2. Previous preset, preset name/browser trigger, and next preset.
3. Favorite and save actions when presets are supported — including **Init**, which returns every
   parameter to its default and belongs beside the patch actions rather than in the utility menu.
   It is a patch action; that is where a person looks for it.
4. Undo and redo.
5. Global utility menu, theme control, and help.
6. Master output control and a compact level/clip indicator.

Do not turn the app bar into a second parameter panel. Rare actions belong in the utility menu.

**A narrow window compacts the bar one step at a time** (the owner, 2026-09-24), taking only as
many steps as its width needs, each chosen from widths the bar measured while drawing: first the
theme and zoom controls move into the `…` menu; then the preset name narrows; then the product name
hides, the wordmark staying; then the favourite and *Save preset* move into the `…` menu; last, a
product's own bar actions move into it (`mxm_ui::shell::product_actions` — the drum machine's
Resample and export, owner, 2026-09-26). Below the last step the preset group is cut off at its left,
never drawn over the right-hand group, so its `…` menu stays whole (`mxm_ui::shell::BarStep`) — and
§4.3's minimum window is wide enough that it always can be. **A bar with no `…` menu never compacts** — a bar with no
presets keeps its theme and zoom at any width, having nowhere else to put them.

**The preset name in the bar opens the browser**, an overlay over the view area with the bar left in
sight: three panes — banks, categories, presets with a search box — and the bank actions along its
foot. Not a dropdown: a flat list does not survive a shared bank. Left and right move the keyboard between
the panes and up and down within one — the next bank, the next category, or the next preset, loading
as it goes; Enter is done and closes it; so do Escape, the name,
and a click on the content around it.

**A preset is saved under a name and a category**, asked for together in the Save As row: one word
from the collection's fixed list, on the preset itself, so a browser can find a sound by what it is
across every bank it may end up in. Rename keeps the category; Save keeps the loaded one's.

**Favorites are per-person state and live beside the presets, never inside one.** A preset carrying
a `favorite` field would arrive on somebody else's machine already starred, and a factory preset —
compiled in and immutable — could never be starred at all.

**Loaded, modified and no preset are three states, not two.** "Modified" claims something is loaded;
after Init, or on a fresh instance, nothing is. Show the name, the name marked, or *no preset*.
Modified must show through more than hue (§7.2), and it is a **comparison against what was loaded**
rather than a flag something sets — so returning a parameter to its loaded value clears the marker,
and a change made by host automation while the editor was closed still shows the moment anybody
looks.

### 3.2 Views

**A generated parameter list is not a view a musician needs a tab for.** Where an editor's own
interface reaches every control, the complete list stays implemented and reachable — a host's
automation surface and the developer channel both want it — but it does not take a tab in front
of the person playing the instrument every day (owner, 2026-09-07).

**Musician pages are derived from available width and height, not authored view assignments.**
The category order is **Performance → Modulators → Sequencers → Generators → Tone → Effects**.
Each indivisible card has one primary category, a stable identity, a kind and honest width bounds.
Preserve authored order inside each category; mixed-purpose cards keep their bodies intact and
record their primary-category choice in the brief. User zoom is never derived from window size.

Merge whole categories while they fit; split an oversized category through kind runs, preferred
same-category groups, then individual cards. **A whole category never costs a page** (owner,
2026-09-24): where filling each page in order, from preferred groups and single cards, makes fewer
pages than keeping categories whole, the pages fill, and a page carries every category it holds in
its label (*Performance + Modulators*, *Modulators + Generators + Tone*). Otherwise categories stay
whole. Page count is not capped at five. A single page has
no navigation bar. Full category names are the initial vocabulary; do not invent abbreviations.

Measure hidden cards at their candidate widths without input, host gestures, transient actions or
destructive telemetry reads. Preserve card-local widget IDs across page moves. Reserve disclosures
open, and invalidate measurements for changed fonts, scale, content and editor-only captions.
Defer re-paging and navigation until active gestures (including release), text entry and popups
finish. Use a conservative merge dead band rather than flickering across a boundary.

View-bar cells MUST be uniformly sized to the widest label's measured text and pointer floor.
When the row cannot hold them, wrap into further rows without breaking or squeezing words. Reserve
the complete bar height before laying out workspace content; navigation state must not change its
geometry. Painted tabs expose their names and selection to accessibility, with keyboard activation.
If labels or wrapping consume the content budget, provide compact Previous / Page / Next navigation.
An indivisible card larger than its viewport retains both-axis scrolling; fitting pages do not scroll.
Controller pages and developer category addresses are separate namespaces from these derived pages.

The selected view, preset access, output state, and unsaved-change state must always remain clear.

### 3.3 Module cards

A module card groups one coherent operation, such as an oscillator, filter, envelope, or effect.

- Header: title, enable/bypass, and at most two local actions.
- Body: visualization first when it materially aids editing, then parameters in reading order.
- Footer: optional routing or advanced disclosure; never a decorative strip.
- Disabled modules remain legible at reduced emphasis. Do not hide them or reduce them to an
  unexplained icon.
- Cards use flat surfaces, a one-pixel border, and spacing—not simulated panels—to establish groups.
- **A card body MAY hold nested groups.** Where a card carries several small clusters that each
  read as one thing — a modulation destination with its meter, its amount and its source — a
  cluster is drawn as a group: a one-pixel border on the border token at the nested radius, and
  **no fill**. That is this section's own sentence applied one level down, not a simulated panel:
  no bevel, no gradient, no shadow, no depth. **A group is a boundary, not a surface.** A tint
  costs it twice — it outweighs the controls it contains, and it takes `surface-2` away from the
  inputs that need it to read as inputs. A group has **no header and no bypass**; anything needing
  either is a card. Nest one level only, and use the group when spacing alone has stopped carrying
  the grouping — not to decorate a card whose controls already read as a list.
- Cards laid out side by side **end on one line**. In a fixed editor, dealt into columns, a shorter
  column's cards share the difference equally *inside themselves* — padding at each foot, or a
  display that is strictly better bigger — so it is the cards' bottoms that align, never the empty
  space below them, and no one card carries a column's whole difference.
- **In a reflowing editor the unit is the row, and the rule is simpler**: every card in a row starts
  on the same line and ends on the same line, and the row is exactly as tall as the tallest card in
  it. **A card is never taller than it needs to be unless something beside it is taller.** Stretching
  a card to match something it does not sit beside — a column of cards it merely shares an edge
  with — produces a card with nothing in it, which is the failure this rule exists to prevent.
- **Do not stretch a card to fill a row it is alone in.** Cap a card's width; a card much wider than
  the controls in it is empty space wearing a border.
- **A reproduced source-panel sound or performance control MUST NOT be hidden behind a disclosure**
  (owner, 2026-09-08). Direct visibility is part of preserving the machine's function even though
  its panel geometry and appearance are not. Disclosures are for software additions, diagnostics,
  or contextual detail—not for controls the original player could already reach on its panel.
- A permitted disclosure opens **inside the frame the editor already has**. A fixed editor is sized
  with every disclosure open, so opening one never pushes a control off the window.

---

### 3.4 Block order

**Cards are ordered by the signal chain: what feeds the chain, then the chain, then what it feeds.**
A general rule; an instrument may break it where the machine earns it, and its §14 brief records the
break.

```
[ voice / keyboard ] [ modulation sources ] → [ oscillators ] → [ mixer ] → [ filter ] → [ amplifier ] → [ built-in effect ]
```

#### Order is the only thing a reflowing layout has

A fixed panel has positions: *far left*, *the second row*, *bottom middle*. **A layout that wraps
has none of them.** Where a card lands depends on the window, so the only property that survives a
resize is **the sequence**, and the only ordering principle that holds at every width is the one the
circuit already fixed.

This is why the hardware convention "modulation below the audio row" **cannot be taken and must not
be cited**. It is a statement about position, it is one machine's habit that software later adopted
(`research:interfaces/panel-layout-and-signal-flow.md` §4.2), and in a reflowing layout it decays
into "last in the sequence" — which no instrument surveyed does, and which puts an LFO after the
filter it feeds.

#### What feeds the chain comes before it

- **The voice block is upstream, not a trailing utility.** The keyboard's note runs through
  portamento, key priority and bend and arrives at the oscillators as their pitch CV. It is the head
  of the chain.
- **A modulation source goes before what it feeds.** An LFO reaching the oscillator's pitch and the
  filter's cutoff is generated before either, so it precedes them. Seven of the thirteen panels
  surveyed put it first for exactly this reason.
- **A card is placed by what it is, not by what it is currently patched to.** Routing is state: a
  preset changes it, and a panel that rearranges itself when a preset loads is not a panel. An LFO
  that reaches nothing until it is patched is still an LFO and sits with the other modulation
  sources — `mxm-mono-00`'s LFO-2, whose neighbour LFO-1 is only *default*-wired anyway.

#### The rest of the placements

- **A separate envelope card belongs to Modulators.** An envelope embedded inside an indivisible
  mixed-purpose card stays there under that card's primary category; do not re-cut cards merely
  to satisfy taxonomy.
- **The mixer is a card when several sources are balanced against each other**, and folds into the
  oscillator when the machine has one source with a sub and a noise level.
- **A built-in effect ends the path**, where it sits in the circuit.
- **Parallel same-category branches are preferred groups.** Keep them adjacent and on one row
  while width and height permit; split the group when otherwise a fitting card would scroll.
- **Modulation depth lives at the destination, on every instrument** (the owner, 2026-09-22;
  `plans/plan-modulation-routing.md` decision 1.1, in the private archive). A source is chosen on the card it moves, beneath
  the control it moves, and a depth fader appears for it — §8's stack. **No card chooses where its
  own output goes**: a machine's destination switch is routes into each destination, present in the
  init patch where the switch pointed.

**An Amplifier card that looks thin is not a mistake to pad.** On the machines surveyed the VCA
usually carries a modulation depth or nothing at all, and three of thirteen had no amplifier block.

§2 still binds: this orders *our* blocks by signal flow. It never licenses copying a panel.

---

## 4. Layout and sizing

Component values are **logical pixels** before host DPI scaling. The quarter-4K window budget
in §4.2 is explicitly physical pixels; do not confuse the two.

### 4.1 Grid

| Token | Value | Typical use |
|---|---:|---|
| `space-0` | 0 | Reset |
| `space-1` | 2 | Optical adjustment only |
| `space-2` | 4 | Tight internal spacing |
| `space-3` | 8 | Related controls |
| `space-4` | 12 | Component padding |
| `space-5` | 16 | Card padding / small gutters |
| `space-6` | 24 | Section separation |
| `space-7` | 32 | Major section separation |
| `space-8` | 48 | Sparse display spacing |

Use the 4 px grid for component geometry and the 8 px rhythm for layout. Arbitrary values are
allowed only for optical centering or data visualization.

**Related controls sit closer together than unrelated ones.** A group's internal spacing must be
strictly smaller than the space separating it from what is around it, and by a step on this scale
rather than by a hair. Equal gaps are read as one undifferentiated list however carefully the
controls inside them are named.

This is the rule the system was missing. The table above types `space-3` as "related controls" and
`space-6` as section separation, but nothing said the two had to differ, so a routing column shipped
in which every gap was the UI framework's own default: a card's explanatory caption sat closer to
the control above it than a source selector sat to the slider it belonged to. The owner's word for
the result was *indecipherable*. Grouping is not decoration and it is not achieved by naming things
well; it is achieved by the space between them, and where space alone cannot carry it, by §3.3's
nested group.

### 4.2 Reference frame

- Starting design size while a panel is being built: **1200 × 760**; smaller useful editors are
  welcome. **The shipped opening size is not chosen at all** — §4.3 derives it from what the panel
  actually draws.
- Collection-wide fit baseline: **1920 × 1080 physical pixels**, one quadrant of a 3840 × 2160 screen.
- Minimum resize width: one usable card, as specified in §4.3; not a mandatory 1920-pixel window.
- Default app bar: **48** high; view bar: **40** high.
- Standard workspace gutter: **16**; compact gutter: **12**.
- Standard card radius: **8**; nested control radius: **4**.
- Minimum pointer target: **24 × 24**; preferred target: **40 × 40**.
- Rectangular controls are drawn at **28** and knobs at **40**. The floor is a floor, not a
  target: the collection drew at the floor exactly for one sitting, and once the interface
  moved to Inter the type filled the boxes. The controls gave, not the type.

**The pointer minimum is 24, and it is a citation rather than a preference** (owner,
2026-09-07). It was 32, a number this system chose for itself, and at 32 every button,
segment, selector and slider track stood a third taller than its content needed, which is
space taken from the parameters a page can hold. 24 × 24 is WCAG 2.2 AA's Target Size
(Minimum), SC 2.5.8. Anything a performer reaches for while playing still wants the
preferred 40, and nothing here licenses shrinking a *transport* or a *macro* to the floor:
the floor is for the dense parameter surfaces, not for the controls a player uses blind.

**Quarter-4K usability is a minimum quality requirement for every plugin editor** (owner,
2026-09-05), not permission to demand more screen space. At 100% editor zoom, each editing page
MUST fit its finite controls and open disclosures within that window budget, including app/view
bars and any floating-window chrome, without clipping, overlap or scrolling to reach ordinary
editing controls. Unbounded preset lists may scroll. Do not shrink typography or pointer targets
below §§6–7 and 11 to pass; improve grouping and density instead.

Measure the default from the rendered contents, with disclosures open, and pin it from above and
below within this budget. Record host/OS DPI scaling and the available logical content rectangle:
physical size is logical size × DPI scale × editor zoom. Do not claim a 1920 × 1080 logical render
fits a physical quadrant at a higher DPI. At larger user-selected zoom, keep the physical window
fixed and retain reachability through reflow/scrolling rather than silently enlarging it. Smaller
windows remain supported through §4.3. Page count is a per-instrument design decision, not a
collection-wide two-page limit. Existing editors must be checked; this rule does not certify them.

The editor MUST support host DPI scaling and SHOULD be resizable from 75% to 200% of the reference
size. Text and hit targets must scale with the editor. Do not use bitmap assets for interface text
or essential control markings.

### 4.3 Responsive behavior

Card floors and ceilings determine rows; available width **and height**, after shell/navigation,
determine pages (§3.2). There are no global column breakpoints or fixed musician view assignments.

Responsive priority is: controls → labels/values → routing → informative visualization → secondary
help. Essential sound controls and every reproduced source-panel control MUST NOT disappear. If
space is constrained, reduce visualization height; only software-added advanced controls may move
behind a labeled disclosure.

#### Every card declares a floor, and it is the larger of two numbers

A reflowing layout has to be told how narrow each card may be, and **one measurement is not enough**:

1. **The width below which it overflows.** A card narrower than its content does not wrap — a knob
   row caps itself and a label never breaks a word — it spills past its own edge. It is computed,
   not measured: every control states its narrowest size, and the card's description adds them up
   (`crates/ui`'s `tree`).
2. **The width below which its controls stop being usable.** A card can stop overflowing purely by
   squeezing its knob columns under the pointer target, which is not a card anybody can use.

**They disagree in both directions**, which is why both are needed, and the floor is the larger.
A floor derived from only one of them is a number the layout will believe and a person will not.

#### A reflowing editor's frame

- The **minimum** is one card wide, at the widest card's floor. Grouping and wrapping preferences
  are things the layout does while it can, never floors on the window: a single column of cards is a
  good layout in a narrow tile. **It is never narrower than the app bar at its last compact step**
  (owner, 2026-09-26): the `…` menu whole — it is the way to the presets, the theme and the zoom once
  the bar has given them up — and nothing in the bar drawn over anything else. Where the bar is the
  wider of the two, it sets the minimum, measured rather than chosen.
- **The default is the quarter-4K budget, hugged** (owner, 2026-09-09), and it is measured rather
  than chosen. Lay the panel out at the budget — the most room an editor may ask for, so it shows as
  many modules as it ever will — then take away the slack between what it drew and the edges of the
  window. That is the whole rule, and it is deliberately a simple one: an opening size is a starting
  point, and anybody who uses an editor for ten minutes finds the size they prefer.
- **The slack is what every page leaves, not the first page** (owner, 2026-09-24: *"The small
  version is not my 1/4 4K hugging rule"*). mxm-para-07 once opened at a third of the budget with
  seven tabs, because its first page happened to be one short category and the window was hugged
  to that. The window is hugged to the largest extent any page draws, and a shrink that would add a
  page has gone too far.
- **The hug leaves a small guard** (`opening_size::GUARD`, R2 of `plans/plan-editor-standard.md` in the private archive):
  hugged to the point, a row that fits with a fraction of a point to spare is split by any hair of
  measurement difference, and the window opens on more pages than it was hugged to.
- **Derive it, do not pick it.** `mxm_plugin_test::opening_size::derive` does the
  layout and the hug; every editor carries a standing check that its constant is still that answer,
  to a gutter's tolerance. A number chosen while building a panel goes stale silently: mxm-mono-01
  opened on two pages with a band of bare canvas under its last row while all six of its cards
  would have fitted.
- The **height** stays within the physical budget. Verify every derived page with disclosures
  open; total unpaged height does not determine the window height. Both-axis scrolling is the
  fallback only for an indivisible overflow, not for otherwise fitting multi-card pages.
- **Nothing in the layout may depend on a position.** See §3.4: a wrapping layout has sequence, not
  places, and any rule phrased as *top*, *bottom* or *the second row* is unexpressible in it.

---

## 5. Color system

Components MUST consume semantic tokens. They MUST NOT embed raw colors locally. Light mode is not
an inverted dark mode; both themes preserve the same hierarchy with separately tuned values.

### 5.1 Neutral and interaction tokens

| Semantic token | Dark | Light | Purpose |
|---|---|---|---|
| `canvas` | `#101216` | `#F2F3F5` | Editor background |
| `surface-1` | `#171A20` | `#FFFFFF` | App bar and primary cards |
| `surface-2` | `#1E222A` | `#E9EBEF` | Nested groups and input tracks |
| `surface-3` | `#272C35` | `#DDE1E6` | Hovered/selected neutral surface |
| `border` | `#343A45` | `#C8CDD5` | Default separators |
| `border-strong` | `#4A5260` | `#AAB1BC` | Active structure |
| `text-primary` | `#F4F5F7` | `#171A1F` | Main labels and values |
| `text-secondary` | `#A9B0BC` | `#5C6470` | Supporting labels |
| `text-disabled` | `#6F7682` | `#8A929D` | Disabled only; not body copy |
| `accent` | `#4CC9D8` | `#247F91` | Default MXM identity/accent |
| `accent-hover` | `#79D7E3` | `#176B7B` | Hover and focus emphasis |
| `focus` | `#92E4ED` | `#0C6575` | Keyboard focus ring |
| `selection` | `#254B54` | `#CBEAF0` | Selected rows/ranges |
| `danger` | `#FF707A` | `#B83A45` | Destructive/error state |
| `warning` | `#F2B84B` | `#8A5A00` | Warning/clip-near state |
| `success` | `#43C59E` | `#147D62` | Valid/saved state |

`text-primary` and `text-secondary` meet WCAG AA contrast against their main surfaces. Disabled text
is intentionally lower contrast and MUST never carry information that is otherwise unavailable.

### 5.2 Modulation colors

Source colors are consistent across every MXM instrument. Dark and light variants represent the
same category.

| Source token | Dark | Light |
|---|---|---|
| `mod-envelope` | `#FF7A82` | `#C64C55` |
| `mod-lfo` | `#56A8FF` | `#2478C7` |
| `mod-key-voice` | `#A98AFF` | `#7750C9` |
| `mod-random` | `#F2B84B` | `#9D6C00` |
| `mod-performance` | `#43C59E` | `#147D62` |

Color never acts alone. A modulation connection also uses a source label or icon, and bipolar
amounts use direction and signed values. Visualization colors MUST remain distinguishable in
common color-vision-deficiency simulations.

### 5.3 Instrument identity accents

Each instrument MAY replace `accent` with one approved identity hue. The accent:

- MUST pass 4.5:1 contrast when used for text and 3:1 for control boundaries.
- SHOULD occupy less than 10% of non-visualization screen area.
- MUST NOT replace fixed modulation or status colors.
- MUST have separately tuned dark- and light-theme values.
- MUST not recreate the signature color arrangement of the referenced hardware.

### 5.4 Color usage rules

- Reserve bright color for active controls, selected navigation, modulation, and live data.
- Inactive controls are neutral; bypassed controls do not retain a saturated fill.
- Do not use gradients on controls. A subtle two-stop data gradient MAY be used inside a spectrum or
  wavetable display if it communicates magnitude or density.
- Do not use pure black or pure white as large backgrounds.
- Clip indication remains visible until acknowledged or until a clearly documented timeout expires.

---

## 6. Typography

Use **Inter** for the complete interface, bundled with its OFL license. It is: `crates/ui/assets/
inter/` carries Regular, Medium and SemiBold with the license beside them, and the early-development
platform fallback this section used to permit is gone.

**Three static cuts, not the variable file**, because egui has no variable axis: a variable font
loads and renders at one instance, which would leave the weight column below decorative. Each cut
is its own family and every style names the weight it asks for. A style needing a weight that is
not shipped needs its file added.

**Tabular numerals stay on a monospace face.** Inter carries them as an OpenType feature and egui
applies no OpenType features, so asking Inter for a value would get proportional digits and the
readout would reflow as it changed — the defect the rule exists to prevent. A visible seam, and the
honest one: a value that jitters under the pointer is a defect, a value in a second face is a
compromise.

| Style | Size / line | Weight | Use |
|---|---:|---:|---|
| `display` | 24 / 30 | 600 | Product or major Play-view value; rare |
| `title` | 16 / 22 | 600 | Dialog and major section titles |
| `heading` | 14 / 19 | 600 | Module titles |
| `body` | 13 / 18 | 500 | Standard labels and text |
| `label` | 12 / 15 | 600 | Parameter labels |
| `caption` | 11 / 14 | 550 | Units and secondary metadata |
| `value` | 12 / 15 | 550, tabular | Parameter values |

The scale went up one step on 2026-09-07, at the same sitting as the controls came down.
Those are the same decision, not two: shrinking a control and shrinking the word beside
it buys density twice and pays for it in legibility twice. A smaller control with a
clearer label is denser **and** easier to read, which is what the reference interfaces
the owner compared against actually do.

Rules:

- Use sentence case: `Filter envelope`, not `FILTER ENVELOPE`.
- Acronyms such as LFO, MIDI, and MPE retain conventional casing.
- Parameter names should be short but not cryptic. Prefer `Cutoff` to `Freq`, and `Resonance` to
  `Res`, where space permits.
- Units are part of the formatted value: `440 Hz`, `−12.0 dB`, `35%`, `1/8`.
- Never condense, stretch, outline, bevel, or glow text.
- **Never break a word.** Text wraps at spaces or it does not wrap. `Resonan` over `ce` and `Key`
  over `tracki` over `ng` are not narrow labels, they are unreadable ones — the reader has to
  reassemble the word before they can read it, which is the one thing a label must never ask.
  This applies to every string in every surface: labels, values, headings, status lines, tooltips.
- **A box is therefore never narrower than the longest word it must hold.** Most UI toolkits treat
  word-boundary wrapping as a *preference* and fall back to breaking mid-word when a word does not
  fit — egui does, and it is the default. Preference is not a guarantee; width is. Where a word
  cannot be made to fit, let it overflow visibly, or shorten the name at the source so every
  surface and host automation agree on it. Do not let the layout mangle it.
- **Never justify.** Justified text stretches the space between letters and words to fill a line, so
  `Pulse width` comes out as `P u l s e` over `width`. It is for paragraphs of prose, and there are
  none here.

**The no-broken-words rule is unbreakable** (owner, 2026-09-07, on finding `Oscillato` over
`r 2` in a source selector). It is not a target, a preference or a default to be overridden
by a container that happens to be narrow: every widget that paints text sets its wrapping
explicitly, because the toolkit's own default breaks mid-word. Where a word cannot fit, the
control overflows and the container is what gets fixed.

---

## 7. Core components

### 7.1 Parameter control

The standard parameter control has a label, an interactive value/control, and a formatted value.
The same parameter uses the same name and unit in every view and in host automation. A visible
label MAY omit a repeated module prefix when its enclosing card or section already names that
module; retain the full name in accessibility and tooltips. Do not invent a second name or name an
assignable amount after its default source: its name must remain true after re-routing.

Parameter hierarchy:

- **Primary:** full-width slider where the range matters; always shows label and value.
- **Standard:** medium slider or a knob; always shows label and value.
- **Compact:** value may appear on hover/focus when density requires it. **Never a tempo-syncable
  rate or time**: an LFO's rate, a sample clock, a delay time always shows its reading — the free
  value, or the note when synced — while it is adjusted (the owner, 2026-09-27), so it is Standard
  at least.

**Every knob in the collection is one diameter** (owner, 2026-09-07). The tiers named
three — 64, 48 and 32, later 52, 36 and 24 — and none of them was carrying the hierarchy:
a knob is a circle with an arc around it, and a small circle with a small arc reads as a
*distant* control rather than a subordinate one, while the row it sits in has to reserve
the tallest tier's height anyway. What ranks a parameter is where it sits, what it is
called, and whether its value is on its face. The tier now says only the last of those.
Sliders keep their range of widths: a slider is a length, so its size is information.

**Every knob stands in one column, and a row of knobs is one row** (owner, 2026-09-24:
standardisation). The column is mxm-mono-08's — the diameter and a gutter, at least the reading
width a name needs (`mxm_ui::control::KNOB_COLUMN_MIN`, `knob_column`) — and a row is equal columns
capped at the sum of theirs and their gaps, never stretched across a card (`mxm_ui::tree::knob_row`).
A knob that shows its value holds its widest reading whole, and a name or reading wider than the
column widens its own column; nothing else does.

Knobs are abstract circular controls: a neutral track, a 270° value arc, and a simple radial marker.
They have no cap texture, lighting, perspective, or fake pointer shadow. Prefer horizontal or
vertical sliders where range and comparison matter more than compactness. A full-height vertical
slider beside a visualization runs from minimum at the bottom to maximum at the top and retains the
same visible name, formatted value, direct entry, reset, fine-edit and host-gesture contracts as the
standard slider.

**An envelope and a mixer are a row of vertical faders** (the owner, 2026-09-25: *"ADSRs are
typically sliders"*, with mixers the prime suspects). The four stages or the sources stand side by
side at one height (`control::FADER_HEIGHT`) in equal columns (`tree::fader_row`), so the levels
compare at a glance. An envelope's faders paint **A, D, S, R** (*"It is a convention"*); the full
names stay in the tooltip, the host and assistive technology.

All continuous controls MUST support:

- Click/touch and drag with a consistent collection-wide direction.
- Fine adjustment with `Shift`.
- Reset to default with double-click.
- Direct text entry from a documented gesture or context action.
- A visible hover, active, automated, and keyboard-focus state.
- Correct host automation begin/change/end gestures.
- A tooltip containing the full name, exact value, and a one-sentence description.

Do not rely on scroll-wheel editing by default; accidental changes while scrolling a view are too
easy. If enabled, it must require focus or a modifier.

### 7.2 Buttons and toggles

**Tempo sync is a quarter note beside its control** (the owner, 2026-09-25), on a **square** button
at a wave cell's height (the owner: *"the button itself should be square. Keep the current
height"*), against its knob's column — the column's margin, 18 px, from the circle (*"too far from
the knob it controls ... half that distance"*). Every time or rate that
can follow the host's tempo has one: on, the control beside it picks a musical division with its own
travel and reads it (`1/8`, `1/4T`, `2 bars`), the way it moves free; off, or with no tempo, it reads
its free value. No separate division control; a transition law (Glide, Snap, Fade) is its own switch.

- Standard heights: 24 compact, 32 standard, 40 prominent.
- Text buttons use verbs (`Save preset`, `Reset modulation`).
- Icon-only buttons are reserved for universally familiar or repeatedly used utility actions and
  require tooltips.
- A toggle shows state through its fill **and** its border, whose width doubles when on — a width
  is not a hue, so the state never rests on colour alone. **No ●/○ mark** (the owner, 2026-09-23:
  *"the pronounced border is enough"*).
- Bypass is not deletion. Destructive actions use confirmation or a recoverable undo.

**Buttons are as small as their content allows, and buttons that stand together share the widest
one's width** (the owner, 2026-09-23: *"Make buttons the same size as the largest one must be, but
as small as possible"*). A toggle is its label with a pad either side and nothing more; a stack or a
row of related toggles takes its widest label's width (`control::shared_toggle_width`,
`toggle_stack`), so the set reads as one. Controls no longer stretch into the space a card hands
them — a switch drawn across a whole card reads as the card's most important control, and several
editors had grown width caps to stop it.

**A toggle is never narrower than its label.** The shared toggle once had a fixed frame and painted
its label unclipped from a fixed inset, so *Complex keyboard tracking* ran straight through its own
border while every rectangle measurement said it fitted. The label sets the frame's width, and a
toggle whose label cannot fit **overflows its card**, as a segmented cell does, which is visible and
points at the card as the thing that should be wider. The modulation dot sits in the corner padding
and never changes the width, so nothing resizes under the pointer when a host starts modulating it.

### 7.3 Segmented control

Use a segmented control for 2–5 mutually exclusive options, such as oscillator mode or filter type.
For more than five options use a menu, searchable chooser, or visual browser. Segments must not
imitate mechanical switches.

**A range is always a segmented control, never a menu or a stepper** (the owner, 2026-09-27: *all
range selectors on all plugins should have buttons, not drop-downs*). An oscillator's footage or
octave is played by jumping between positions, so every position shows. It is the one exception to
the five-option ceiling: mxm-mono-00's six, 64' to 2', are buttons, and the shared control admits
six for it.

**Every option has its own hover sentence** (the owner, 2026-09-27: *Hold, Envelope and Gate do not
do the same, so should not have the same text*). A cell's tooltip is `Label: Option` and under it
what **that** option does — *Hold*: "Always open: the sound drones with no key held." — never one
sentence for the whole row, and never the row's options listed in one line. The shared control
takes one sentence per option and asserts it; a row without them fails in any test that paints it.

**A row of buttons may set another control rather than hold a value of its own** (mxm-chorus-06's
I, II and I + II, which put Rate on the circuit's three rates — the owner, 2026-09-28). A button is
lit only while that control sits on its value, so the two cannot disagree; with the control
anywhere else nothing is lit, and an arrow enters the row at the end it moves away from — Right or
Up at the first cell, Left or Down at the last.

**Where the options are waveforms, draw them — in every plugin, wherever a picture exists** (the
owner, 2026-09-23: *"All the plugins must always use waveform images instead of names where
possible"*). A musician recognises a square wave faster as a picture than as the word, and the cells
then need only the §11 pointer minimum rather than the width of the longest name — which is often
the difference between a control sitting beside the knob it belongs to and being pushed onto its own
row. Drawing is not a licence to drop the name: it stays as the hover text and the accessible name,
and a segment that paints its own content must carry one.

- **The picture must be the shape the DSP makes**: a saw that falls is `RampDown`, noise is `Noise`
  and not sample-and-hold steps, a random LFO that glides is `SmoothRandom`, a reverb's decay envelope is `Decay`, `Gated` or
  `Swell`, and a tail law is the envelope it multiplies by (`Level`, `Fade`, `Rise`, `Gated`). The shared vocabulary is
  `mxm_ui::control::Wave`; a shape it lacks is added there, never drawn locally.
- **Six pictures are two rows of three** — the one declared exception to the 2–5 rule above, for
  waveform pictures only, because a cell is a small picture and a menu would hide every shape but
  one. The arrows walk the options in order across the rows; the grid is one control.
- **Options that differ in more than their shape carry a mark** beside the picture (mxm-mono-01's
  sub oscillator: a square `−1`, a square `−2`, a pulse `−2`).
- **Shapes that combine are picture switches** (`control::toggle_wave`): an on/off per wave, drawn as
  the wave, with the name as its accessible name.
- A list that mixes waveforms with things that are not (a source called *LFO*, *Gate*) keeps its
  words.

**Every cell of a segmented control is as wide as its widest option, and no wider.** The cells are
equal — that is what makes the row one control — and the row does not stretch into the width it is
handed. Segmented controls stacked together share one cell width, the widest any of them needs
(`control::shared_cell_width`, `segmented_stack`), so a stack is even rather than ragged.

**A cell is never narrower than the word in it.** §11's 32-point pointer minimum is a floor, and a
word is usually wider than 32 points; a cell sized to the pointer floor alone paints its label
across its neighbours and outside the control, while the control's own rectangle stays obediently
inside the space it was given. Nothing measures that, so nothing catches it — it was found by
looking at a narrow card. A control whose options cannot fit therefore **overflows its card**, which
is visible, measurable, and points at the card as the thing that should be wider.

**Beside a knob, a segmented control sits on the knob's grid.** Its label is on the knob's name line
and its cells are centred on the knob's circle, in a top-aligned row. A knob's name box is two lines
tall with the name on the second, so a control laid out plainly beside it puts its label a full line
above the knob's name and its cells well above the circle. The shared control does the arithmetic
from the knob's size tier; an editor only says which knob it sits beside.

### 7.4 Menus and selectors

- The selected value is always visible.
- Menus open toward available editor space and remain inside plugin bounds.
- Long lists support type-to-search where the UI framework permits it.
- Preset selection is distinct from parameter selection and receives more visual width.

**A selector is one row: the name on the left, the value on the right.** Not a label on its own line
above a framed box the width of the card — that is two lines and a great deal of furniture to carry
one word, on a card whose job is the controls above it. Flat at rest, taking the hover treatment
under the pointer; a column of selectors then reads as a list of settings rather than as a stack of
widgets.

The row is §11's pointer height whatever the text measures, and **its accessible name is *name:
value***, both halves — the visible text is only the value, and a name alone leaves a screen reader
with a word and nothing to attach it to.

**Beside a knob, that whole selector row aligns to the bottom line of the knob's fixed name box.**
A selector's name and value are one sentence, so unlike §7.3's label-plus-cells control it has no
second row to lower onto the knob circle. This keeps a route selector on one baseline whether its
neighbouring amount name occupies one line or two; vertically centring ad hoc rows makes the route
text jump when the amount wraps.

**A bounded parameter selector uses the collection's canonical caret treatment at rest:**
`‹ Off ›` (owner, 2026-09-08). Bare value text gives no affordance and a generic dropdown triangle
says only that a menu opens; the two flanking carets say that the current value is one of a set.
They are drawn rather than set because no font in the stack is guaranteed to carry a chevron glyph.
The shared `selector`/`selector_beside`/`selector_inline`/`selector_sublabel` implementation owns
that geometry; an editor does not substitute a local dropdown.

A caret-flanked selector is **sized to its own content**: the word, a caret either side, and the
padding between them. It does not stretch to its container—a one-word setting drawn the width of a
card reads as an empty text field. The word is centred between the carets and never runs into one.
It is flat at rest and uses hover/focus treatment under interaction; carets and a filled box would
be one affordance too many. Filled and bordered menu fields remain for unbounded/searchable lists
such as presets, not for a finite parameter enum.

**A source belongs on its amount's name line, not on a line of its own.** A routed amount
is named *"Clock period from Key"*; drawing the source beneath the track spends a whole
row restating a word already in the label. The label drops the trailing source word and the
selector supplies it inline — *"Clock period from ‹ Key ›"* — so the row reads as one
sentence and every destination costs one line less. The full name stays in accessibility
and in the tooltip, which is the bargain §7.1 already strikes for a repeated module prefix.

**Routing uses caption typography and secondary ink.** A standalone input that belongs to no
particular control uses the name/value row above.

**A modulation source selector is the same control on every instrument.** Where a source belongs
to a particular knob, slider or switch, it MUST appear as the **clickable sublabel directly beneath
that control**, showing only the selected source and a small drawn chevron — the shared
`selector_sublabel`, not a name/value row, a framed combo box or a local widget. Two instruments
once drew the same idea two ways, one with the chevron and one without, and the owner could not
tell which was the menu. Do not repeat a separate input-name column or collect these sublabels in
a remote footer. The sublabel remains a full pointer target, with the complete input/value
accessible name, focus/hover feedback and a tooltip. Default may resolve to its actual source in
the closed sublabel, but remains an explicitly distinct option in the menu.

Where a destination sums several sources and the sublabel reveals one at a time, the other
non-zero sources are named in one caption line beneath the sublabel, reading **"Also from
Pressure +30 % · Random 1 −12 %"**. The line is absent when there is nothing else to name; "Active:
none" tells the player nothing and is not written.

**A destination's live meter, amount, sublabel and "Also from" line are one group**, in §3.3's
sense: they are drawn inside one nested group, and the space between two destinations is larger
than the space inside either. A source selector belongs to the amount above it, and a reader must
be able to see which amount that is without counting rows.

### 7.5 Meters and scopes

- Meter updates are smoothed visually and never fed back into DSP.
- Peak and clip states are visually distinct. A full-height vertical peak meter uses the shared −60
  to 0 dBFS scale: green below −12 dBFS, yellow from −12 to −3 dBFS and red above −3 dBFS. Meter
  lights use a dedicated vivid signal palette, not the more muted semantic colours chosen for text;
  yellow must read as yellow rather than amber or brown. Fixed vertical position, visible threshold
  dividers and a peak cap repeat the reading for red-green-colour-blind users instead of relying on
  hue alone. Unreached level remains a neutral theme-appropriate gray; signal colour appears only
  where the measured peak has reached, and a peak hold keeps brief yellow/red excursions readable.
  Clipping occupies a separate per-channel CLIP cell: full danger colour means a current event;
  lower-intensity red means an older unacknowledged latch; clicking acknowledges it.
- Paired input/output rails use short **In** and **Out** headings so a narrow meter never breaks a
  word; accessible names retain the complete signal and channel names.
- Scopes show a neutral zero/reference line and meaningful scale when appropriate.
- Animation pauses or drops to a low update rate when the editor is hidden.
- A visualization must degrade gracefully if realtime data is unavailable.

### 7.6 Tooltips and help

**No help text on the panel** (the owner, 2026-09-27: *there should not be help text on the screen;
mouse hover over a parameter is fair game*). A card carries controls, their names and values, live
readings (*Sounding 440.0 Hz*), status and a display's legend — never a sentence explaining how
something works. That sentence is the control's tooltip (§7.1's one-sentence description, or a
display's hover text); durable explanation belongs in the instrument's documentation. An
explanation that only makes sense for the card as a whole goes on the control it is really about,
or nowhere. This reverses the earlier rule that required operating instructions must not live only
in tooltips.

**Tooltip text is written for the player** (the owner, 2026-09-27: *the text MUST be user focused,
not a leak from internal technical discussions of the synths*): what the control does to the sound
and how to use it, in plain words — *Glide time between notes; zero turns glide off.* Never the
circuit's, the research's or the code's vocabulary (*divider tap*, *master core*, *CV lag*,
*phase-locked register*, *nonlinear clamp*), and never a claim about routing that the stacks
already show. A row of buttons says what each button does, on its own cell (§7.3). **Checked**:
each plugin's `speaks_to_the_player` tests (`mxm_plugin_test::hover_text`) read its editor's
sentences and fail on the vocabulary that
leaked before — the hardware, the machine, the plug-out, the circuit, the original, *CV*, *chip*,
*as fitted*, *exact bypass*, *costs no CPU* (2026-09-27, when every editor's text was rewritten).

**Every editor is clear** (2026-09-27): the survey's captions in mxm-para-07, mono-00, mono-01,
mono-02, mono-03, mono-08, mono-pr1, poly-06, creative-sampler, drum-machine, fx-convolution and
fx-curve are gone, each fact moved to the tooltip or hover text it belongs to. mxm-para-07 and
mxm-mono-08 hold the rule with `no_card_prints_help_text`, which allows only their live readings.

Tooltips appear after a short delay, do not cover the active control where avoidable, and contain
plain language. Advanced parameters MAY include a second sentence describing musical effect.

---

## 8. Modulation language

For instruments that support routable modulation:

1. Every source has a persistent chip using its category color, short name, and source icon.
2. Selecting a source enters assignment mode and highlights valid destinations without recoloring
   the entire interface.
3. Drag-and-drop assignment MAY be supported, but click-source then click-destination MUST be an
   accessible alternative.
4. A destination displays modulation as a colored secondary arc/track around its base value.
5. Bipolar depth displays a center reference and an explicit signed value.
6. Multiple sources are shown as distinct thin arcs or a concise source list; never blend them into
   an unreadable rainbow ring.
7. Removing an assignment is undoable.
8. Live modulation motion is shown inside the control, while the editable base value remains clear.
9. A destination is named for what it moves — *Cutoff from Envelope 1* — and a parameter has one
   destination, however many jacks the hardware gave it. *Modulator*, *Modulator 1* and a jack's
   name are not destination names (`plugins/AGENTS.md`, *Declaring the target list*; since the split,
   [`plugin-conventions.md`](plugin-conventions.md#declaring-the-target-list--what-mxm-mono-00-had-to-discover-twice)).

Simple instruments do not need a modulation matrix merely to match larger products. The language is
shared; feature depth is not mandatory.

---

## 9. Visualization and motion

Animation exists to explain sound, state, or cause and effect.

### Appropriate motion

- Current position moving through an envelope or wavetable.
- Filter response reacting to cutoff/resonance.
- Modulation range and live modulated value.
- Meter attack, release, and peak hold.
- A 100–160 ms transition for view or disclosure changes.

### Inappropriate motion

- Ambient pulsing, particles, animated backgrounds, decorative glow, parallax, or idle movement.
- Springy/bouncy control transitions.
- Animations that continue when the editor is closed.

Motion SHOULD update at 60 fps when inexpensive, but it must yield before audio performance is at
risk. UI telemetry comes from lock-free atomics or a triple buffer—never a mutex read by the audio
thread. Respect the operating system's reduced-motion setting where available and provide a reduced
motion option otherwise.

---

## 10. Themes

Every instrument ships with **Dark**, **Light**, and optionally **System** theme choices.

- Dark and light receive equal design and test coverage.
- Theme switching is immediate and does not reopen the editor.
- Theme preference persists as editor/UI state, not as an automatable audio parameter.
- A preset must sound identical in every theme; no theme value enters DSP state.
- User-authored presets should not silently change the editor theme.
- Logos, icons, plots, focus indicators, disabled states, menus, and dialogs must all consume theme
  tokens. There are no theme-specific one-off assets unless absolutely necessary.

Theme quality is judged by hierarchy, not by whether all colors were mathematically inverted.

---

## 11. Accessibility and input

- Normal text meets WCAG 2.2 AA contrast (4.5:1); large text and essential control boundaries meet
  3:1.
- **Keyboard focus is visible to whoever is using the keyboard**, and follows visual reading order.
  The cursor is always *positioned* — the last control touched, by mouse or by key, so the next
  VALUE press edits rather than arrives — but it is only *painted* once somebody reaches for the
  keyboard. An editor opens with nothing ringed, any pointer press anywhere puts the indication
  away, and the next key that operates the cursor brings it back where it was. This is the
  web's `:focus-visible` rule, and both halves of the cursor follow it together: the card outline
  and the parameter's focus ring. The owner ruled it on the grain-fx panel, 2026-09-11, against an
  earlier reading of this line that painted the cursor from the first frame of every session.
- Controls expose a text value and name to accessibility APIs where the plugin framework supports
  them.
- Color is reinforced by shape, position, text, or iconography.
- Do not encode meaning with animation alone.
- Avoid flashing above three times per second.
- The UI remains usable at 150% and 200% scaling without clipped labels or inaccessible controls.
- Mouse, trackpad, and pen behavior should be equivalent. Touch support is desirable but not a v1
  requirement unless explicitly scoped.

Keyboard behavior is **the keyboard language** (newDAWn's `docs/keyboard.md`; the owner decided
on 2026-10-07 that the editors convert to it, keys first in newDAWn and the collection; piloted on
mxm-mono-08 and rolled out to every editor on 2026-10-08). Its engine is `mxm-keys`, and the keys
named below are the default keymap's: W is VALUE, S COARSE, D FINE, F MICRO, A MUSICAL, C VIEW and
Tab OUT. The keys of a combination can be held together or pressed one after another.

| Key | Behavior |
|---|---|
| arrows | Move the cursor to the next parameter that way inside the card (← → along its row, stopping at either end), and onto each cell of a segmented control |
| COARSE + arrows | Move the cursor from module/card to module/card |
| VIEW + arrows | Move between the cards and the bars above them (the view bar, then the app bar), never out of the window, which is the window manager's |
| VALUE + arrows | Adjust the selected parameter — **fine**: 1 % of its travel, or a semitone |
| VALUE + COARSE (or MUSICAL) + arrows | Adjust the selected parameter — **coarse**: 10 %, or an octave |
| VALUE + MICRO + arrows | Adjust the selected parameter — **finer**: 0.1 %, or a cent |
| OUT | Keep the edit, as one host gesture, and stay on the parameter |
| BACK (`Escape`) | Cancel the edit back to where it began, a mouse drag included, or close transient UI |
| DELETE | Return the selected parameter to its default |
| OPEN (`Enter`) | Choose the segmented cell the cursor is on, or begin value entry; while a verb is armed, keep the edit and open nothing, as in newDAWn |
| `Home` / `End` | Minimum / maximum where safe |
| `Ctrl/Cmd+Z` | Undo |
| `Ctrl/Cmd+Shift+Z` | Redo |
| `F1` | Show or hide the sheet of the keys, named from the keymap in use; `Escape` closes it too |

The edit is **one gesture**, however many presses it takes, and each press starts where the one
before it landed. BACK alone never reveals the cursor: a panel must not light up because somebody
dismissed a menu. Chords with `Command` or `Alt` are left to the controls and the host.

How far one press moves is a share of the control's **travel** — 10 %, 1 % and 0.1 % — snapped
onto the parameter's own grid, so a skewed range keeps its skew: a press near 20 Hz moves a few
hertz and one near 20 kHz moves hundreds. **A pitch moves musically**: an octave, a semitone and a
cent (owner: *"octave, semitone, cent is the range"*), to the next whole one in the direction
pressed. **A press never moves less than one of the parameter's own steps**: on an option list or a
whole-semitone tune, every size reaches the adjacent value.

**Every editor in the collection runs the cursor**, so this table is the mapping, not one of two.
Only where there is no cursor to run — a cardless developer surface — does a focused control edit
its value with the bare arrows, and an editor MUST NOT let one do so on a card surface. A control
that edits a parameter
MUST be reachable by the cursor, whatever widget draws it: a knob, a switch, a segmented control
and a selector are all parameters, and one that paints without joining the cursor's registry is a
control a keyboard player can see and never touch. Movement follows painted geometry: a target MUST lie in the arrow's half-plane; horizontal movement prefers the same visual
row, vertical movement crosses to the adjacent row before a later one, and a page edge continues to
the adjacent card in the paging plan. A multi-cell control is one parameter target, not one target
per cell. A cardless surface has no cursor and MUST NOT retain invisible keyboard ownership from a
previous card surface.

The cursor's own state — selected card and selected parameter — MUST be visible, MUST NOT rely on
hue alone, and MUST NOT occupy layout space, so arriving on a control cannot move anything. That
visible target remains authoritative if a custom-painted control loses native widget focus between
frames. A value edit is one balanced host automation gesture from its first press until it is kept
(OUT, or letting go of a held VALUE) or cancelled (BACK); repeat events update values inside it and
MUST NOT be dropped.

The host owns global shortcuts. Plugin shortcuts MUST work only while the editor has appropriate
focus and MUST not trap DAW transport input unexpectedly.

---

## 12. Icons, artwork, and brand

- Use a single original 1.5 px outline icon set on a 16 or 20 px grid.
- Icons use round joins/caps and minimal interior detail.
- Filled icons are reserved for selected or latched states.
- Prefer text when an icon's meaning is not obvious.
- Product artwork, if present in preset browsing or an about panel, uses abstract geometry derived
  from signal concepts—not images of the original hardware.
- The MXM wordmark stays monochrome. The instrument accent may appear as a small adjacent mark.

The product title format is `MXM` plus the recognizable collection model identifier, following the
repository naming convention. Names and artwork must be checked for trademark confusion before
release.

---

## 13. Shared implementation contract

The canonical implementation belongs in `crates/ui`; plugins consume it instead of restyling local
widgets.

Suggested organization:

```text
crates/ui/src/
├── theme.rs          # semantic tokens and Dark/Light resolution
├── typography.rs     # bundled font registration and text styles
├── icons.rs          # original vector paths
├── layout.rs         # shell, app bar, view bar, cards, responsive helpers
├── controls/         # knobs, sliders, buttons, selectors, tooltips
├── modulation/       # source chips, destination arcs, assignment states
└── visualization/    # meters, envelopes, scopes, response plots
```

Rules:

- Plugin code supplies labels, parameter bindings, formatted values, and domain data.
- Shared UI code owns geometry, visual state, focus treatment, and interaction conventions.
- DSP never reads editor state. Durable sound changes use the plugin parameter system.
- A genuinely transient, non-value action MAY use a plugin-owned realtime command path only when
  the plugin's local contract explains why parameter, preset, state, and automation semantics would
  be false. The path is preallocated, non-blocking, and outside shared UI code; one activation must
  neither coalesce nor duplicate, reset/panic ordering must be defined, and rejected delivery must
  not be silent. The plugin shell consumes the command and invokes the DSP's ordinary event path.
- DSP-to-editor visualization uses atomics or a triple buffer and may drop frames.
- No control implementation may allocate, lock, log, or perform I/O on the audio thread.
- Shared components need visual state examples for default, hover, pressed, focused, automated,
  disabled, and error states in both themes.

A component enters the shared crate only when at least two instruments need it, except for the shell,
theme tokens, typography, and basic parameter controls, which define the collection from day one.

---

## 14. Per-instrument design brief

Before implementing a new MXM instrument, add a one-page UI brief answering:

1. What is the instrument's primary sound-design task?
2. What are the three to five parameters users reach for most?
3. What signal flow must be visible?
4. Which controls belong in Play view?
5. Which **software-added** controls are advanced and how are they disclosed? Confirm that every
   reproduced source-panel sound and performance control remains directly visible.
6. Which primary categories, kinds and stable cards does it contain, and which groups should stay together while space permits?
7. What is its identity accent in both themes, with contrast results?
8. What live visualizations materially improve understanding?
9. What is removed from the source hardware layout, and why?
10. How does each page fit the quarter-4K budget with disclosures open, and behave at smaller sizes
    and 200% zoom in the same physical window? Record the tested DPI scale.

This prevents “same skin, different panel” designs. Collection consistency comes from the system;
each instrument's layout still follows its musical purpose.

---

## 15. Design QA gate

An instrument is not visually complete until all items pass.

### Structure

- [ ] Common app bar and view behavior match the collection.
- [ ] Primary sound task is obvious within five seconds.
- [ ] Signal flow reads left-to-right or top-to-bottom without a manual.
- [ ] Advanced detail does not compete with primary controls.
- [ ] No fake hardware or ornamental panel elements remain.

### Themes and accessibility

- [ ] Dark and light modes have equivalent hierarchy and complete states.
- [ ] Text/control contrast has been measured, not judged by eye.
- [ ] Color-vision-deficiency simulation preserves modulation meaning.
- [ ] Full editor can be operated with keyboard focus where supported.
- [ ] 100%, 150%, and 200% scales have no clipped essential content.

### Interaction

- [ ] Every parameter supports fine edit, reset, exact value entry, and tooltip.
- [ ] Host automation gestures begin and end correctly.
- [ ] Undo/redo covers destructive and modulation-routing actions.
- [ ] Disabled, bypassed, selected, focused, and automated states are distinct.
- [ ] Menus and tooltips remain within plugin bounds.
- [ ] Nothing appears, disappears or moves under the pointer during a drag. Live feedback
      occupies its space at rest and fills in place.

### Audio safety and performance

- [ ] Opening, resizing, animating, and switching themes do not affect audio output.
- [ ] Closing the editor stops or throttles visualization work.
- [ ] UI telemetry never locks or allocates on the audio thread.
- [ ] Typical animation remains smooth without starving the DAW.

### Responsive panels

- [ ] Every editing page fits the **1920 × 1080 physical quarter-4K budget** at 100% editor zoom,
      with disclosures open, standard typography/targets, and no overlapping or clipped controls.
      Record DPI scaling and window chrome; inspect painted contents, not just outer card bounds.
- [ ] At increased zoom the physical test window stays fixed; reflow/scrolling preserves access.
- [ ] A list that can outgrow its space **wraps into columns and then scrolls** — it never clips.
      Reachability is the invariant; simultaneous visibility is a target, not a promise, because a
      generic host cannot bound how many controls a plugin reports.
- [ ] Wrapping stops at a **minimum usable control width**. A slider too narrow to aim at is worse
      than a scrollbar.
- [ ] Ordering stays predictable through reflow: a control must not jump somewhere surprising when
      the window changes by a pixel.
- [ ] Every card's floor is the **larger** of the width below which it overflows and the width below
      which its controls stop being usable (§4.3). One of the two alone is a number the layout
      believes and a person does not.
- [ ] Cards in a row start and end on one line, and **no card is taller than its own content unless
      something beside it is taller** (§3.3).
- [ ] No card is stretched across a row it is alone in.
- [ ] Parallel branches — two oscillators, two envelopes of a pair — are never separated by a wrap
      (§3.4).
- [ ] No rule in the layout is phrased as a **position**. A wrapping layout has sequence, not places
      (§3.4).

### Overlapping states

- [ ] Where two meanings can apply to one element at once — sounding *and* selected, for instance —
      **all three states are distinguishable**, and the third is not merely one of the other two.
- [ ] None of them relies on hue alone. A second channel — a marker, a border, a fill — carries the
      same information.

### Originality

- [ ] No third-party UI assets, icons, screenshots, or copied component geometry are used.
- [ ] The result is recognizably MXM, not a Pigments reskin or a source-hardware replica.
- [ ] Fonts and all bundled assets include compatible licenses.

---

## 16. Inspiration references

These references establish design goals only; they are not implementation assets. Naming what
influenced a decision is useful — it says *which* quality is meant. What is not acceptable is
holding up another product's interface as the thing to match: no screenshots, no mock-ups traced
from one, nothing that turns a reference into a target.

- [Arturia Pigments overview](https://www.arturia.com/products/software-instruments/pigments/overview) — visual sound design, color-coded workflow, informative animation.
- [Arturia Pigments product page](https://www.arturia.com/store/pigments) — graphical representation, modulation workflow, and dark/light themes.

The original MXM tokens, component geometry, icons, layouts, and visualizations defined above are the
source of truth for this collection.
