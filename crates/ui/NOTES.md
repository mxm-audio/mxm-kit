# NOTES.md — crates/ui

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples. AGENTS.md is the contract; this file is the reference it links to.

Each section below keeps the wording of the AGENTS.md section it came from (as of 2026-10-06), grouped by topic. A reference to a section *above* or *below* means one in this file.

*Since the split (2026-10-06):* `plans/…` cited here is in the private archive; a `plugins/<plugin>/…`, `crates/<plugin>-dsp/…` or `apps/<plugin>-standalone` path is in that product's own repository, and a rule cited from `plugins/AGENTS.md` is in full in [`docs/plugin-conventions.md`](../../docs/plugin-conventions.md).

## The modules

**Foundation modules:** `theme` (§5 tokens, both themes, measured
contrast), `typography` (§6 scale), `space` (§4.1 grid), `visual` (software-native telemetry
canvas, trace, meter and marker geometry), `control` (§7.1–7.3 knob, slider, segmented control,
toggle), `shell` (§3.1–3.3 app bar, view bar, module card, level meter, and columns of cards that end level) and `browser` (§3.1's preset browser opened out: three panes and
the bank actions, rows in and actions out). `tree` is a card body as data, measured and drawn from one description (the `new-layout` prototype). `flow` lays out wrapping rows; `paging` measures
hidden cards and derives category-first navigation for every plugin editor. `navigation` is the keyboard cursor over that surface—a card, a parameter inside it and the
value—and runs in every collection editor.

## Dependencies and threads

### `egui` and nothing else

No windowing crate, no host crate, no plugin framework. The same controls are drawn by the player
as a pane and by a plugin editor inside a DAW's window, so nothing here may assume it owns a
window, an event loop or a swapchain — and a dependency that pulled in a platform backend would
foreclose native GUI hosting before it was built.

`lib.rs`'s `the_crate_is_windowing_free` test is the mechanical check. It reads the manifest, so it
cannot be satisfied by a transitive dependency arriving under a different name — check what a new
dependency pulls in before adding it.

### `egui_taffy` is not an exception

**`egui_taffy` is a dependency of this crate and is not an exception to *`egui` and nothing else*.**
That rule is about windowing crates, host crates and plugin frameworks — things that would assume
ownership of a window. Taffy is a pure-Rust implementation of CSS flexbox that computes rectangles,
which is what this crate was already doing by hand and getting wrong. `the_crate_is_windowing_free`
is unchanged and still passes.

### Blocking native work leaves the frame — `offthread`

A frame drawn inside a plugin window must never wait on a modal OS loop. A native file dialog pumps
that window's messages while egui-baseview is still inside the frame, the re-entered handler finds
its state borrowed, and the panic lands in a window procedure that cannot unwind — the host aborts.
`offthread::start` runs the work (the caller's own dialog closure; this crate takes no dialog
dependency) on a named thread and requests a repaint when it finishes; `running` disables the
trigger meanwhile; `take` hands the result, once, to an ordinary frame. Results live in the egui
context's temporary memory under the caller's id, so an editor closed before the dialog simply never
collects its pick. Its tests prove the caller returns while the work blocks, a second start does not
stack, a result is collected once, and a cancelled dialog frees the slot.

### A named text style must never panic

`TextStyle::resolve` panics on an unregistered name, and a `Ui` holds a clone of the style it was
built with — so calling `typography::apply` partway through a frame leaves every `Ui` built earlier
resolving against the old map. Use `typography::value_style`, `label_style` and `caption_style`,
which fall back rather than panicking. A panic inside a paint call takes the host down with it.

## Controls and their outcomes

### Controls take a value and return what happened

Every control takes a normalised `0..=1` value and a borrowed description, and returns a
`ControlOutcome`. It never reaches for a host, a plugin, or a parameter object, and it **reports**
gestures rather than sending them — only the caller knows whether it is talking to a CLAP host, a
standalone wrapper or a test.

Real units, ranges and skew stay with whoever owns the parameter. A shared control that reasoned
about a plugin's skew curve would need updating whenever a plugin changed.

The same ownership line applies to a momentary action: shared UI may report that a button was
activated, but it never owns the command path or calls DSP. `docs/MXM_DESIGN_SYSTEM.md` §13 permits
that path only for a genuinely transient non-value action under the plugin contract; the plugin
shell owns delivery, cancellation and proof.

#### Edit kind and externally driven values

`ControlOutcome::reset` distinguishes returning to the default from choosing that same value. The
player uses it to clear a selected step’s lock; inferring reset from equality would collapse two
different acts.

`ParamView::marked` says another owner also sets the parameter, without naming that owner.
`ParamView::modulation` is the quantitative delta: knobs draw an inner arc and sliders draw a line
below the track. Slider marks sit beside the free-flowing name; knob marks overlay the fixed column
so geometry never changes. A segmented control receives `sounding: Option<usize>` and rings the
modulated cell when it differs from the base selection.

While another value sounds, clicking the selected segmented cell must still fire so a step can lock
back to the base. Double-click resets to `default_cell`; `None` preserves inertness for callers with
no default. Mark dots use the surrounding mark color, including playhead inversion.

**A selection one past the last cell is *nothing selected*** — a row of buttons that sets another
control and lights only while that control sits on a button's value (mxm-chorus-06's I, II and
I + II over a free Rate; design system §7.3). No cell is lit, and an arrow enters the row at the end
it moves away from: Right or Up at the first cell, Left or Down at the last
(`an_arrow_enters_a_row_with_nothing_selected_at_its_near_end`).

### Two drag idioms, and they are not a contradiction

A **knob is relational**: drag up to increase, wherever the pointer is. A **slider is positional**:
the handle goes where you point, because that is what a slider means and what direct manipulation
requires. `Shift` refines both.

A control moves the way its shape says it moves. `slider_vertical` is the full-height form for a
parameter beside a primary visualization: minimum is at the bottom, maximum at the top, pointer
position is absolute, Shift keeps fine adjustment, and label/value/text entry retain the ordinary
slider contract. Width and height are supplied by the caller because only that layout knows which
visualization the control must match.

**A knob's drag accumulates against an anchor, not against the value it reads back.** `DragAnchor`
adds total pointer travel to the drag-start value. CLAP parameter delivery is asynchronous with the
editor repaint clock, so accumulating frame deltas against readback would lose or duplicate motion
while the host catches up.

`a_late_applying_host_does_not_change_where_a_drag_ends` holds it, and holds it the right way: it
compares a lagged host against an instant one rather than against a computed distance, because egui
eats the first few pixels of a press before calling it a drag and a hard-coded threshold would
break the next time egui tuned it. What must hold is that the lag costs nothing. A slider needs
none of this — it is positional, so it reads the pointer, not the parameter.

### Nothing changes size under the pointer

Every control allocates geometry from its `Size` tier **before** reading hover, focus or drag state;
interaction changes only color and stroke width.

### The mark slot is allocated whether or not there is a mark

`mark_slot` always reserves `SPACE_2` and paints only when `marked`. Knob marks are painted rather
than laid out, and live route bars reserve their area at every level. **Reserve the space, paint the
state** so interaction cannot shift neighboring controls.

### A remove is the mark alone, never a framed button

`remove_mark` draws the cross unfilled and unbordered. There is no framed form: every remove in the
collection sits at the end of an already framed item — a layer chip, a route row's slider — and
there a second frame reads as a sibling item rather than as an action on the preceding one. The
owner ruled that twice, first on the sampler's chips and again on the route rows, and the framed
`remove_button` was deleted on the second rather than kept for a standalone case that does not
exist.

**The cross is inset by a third of the square**, which is what makes it read as a mark with air
around it rather than as an X filling a box.

**Losing the frame costs nothing in pointer target.** The caller passes the row's height and the
whole square stays clickable; only the paint changes. `shown: false` draws nothing while still
allocating the square, because a row whose crosses appeared as items loaded would reflow under the
pointer and would measure a different card floor depending on what happened to be loaded.

**Neither form spells `✕`.** The editor's font has no glyph for U+2715 and it drew as `?`. A control
whose meaning is a codepoint is a control that depends on a font it does not ship.

### `control::remove_mark` is a parameter control wherever it clears a parameter

A cross that clears a boolean whose *visibility is its state* — the routing rows, where a row exists
because its pair is present and a toggle reading `On` beside it restated the row and cost twenty
points of width doing it. Where it clears a parameter it registers with the keyboard cursor, takes
the focus ring, carries `marked` and names itself for the accessibility tree exactly as `toggle`
does: it is `toggle`'s shape with the label removed rather than a new interaction. **`label` is
what it removes** — *"Remove LFO from Cutoff"* — not a bare "Remove", because that is the name a
screen reader reads out and the only handle a UI test has on which of a stack's rows it found.

Its other consumer, the sampler's layer chips, clears an asset rather than a parameter and passes
`marked: false`; there is nothing there for a host to automate and so nothing for the dot to
report.

The mark is **painted, not typed** — two strokes rather than a `×` glyph — so it owes nothing to the
bundled font's coverage or optical centre and stays crisp from 75 % to 200 %.

## Widths and sizes

### A segmented cell is as wide as its widest option, and no wider

A segmented control debug-asserts **2–6 options**: §7.3's five, and six for a range, which is always
buttons (the owner, 2026-09-27) — mxm-mono-00's 64' to 2'. More needs a menu.

Painted segment text is not clipped, so the width floor must include the widest option plus
`SPACE_3` on both sides, not merely the pointer target. A control whose options
cannot fit **overflows its card** instead of painting outside itself. That failure is visible,
measurable, and points at the caller's card, which is the thing that should be wider.
`segment_min_width` exposes the number, so a caller that must declare its own minimum width can ask
rather than render and guess. `a_segment_is_never_narrower_than_its_word` holds it.

**That floor is also the width** (the owner, 2026-09-23: *"the same size as the largest one must be,
but as small as possible"*). Every cell is `segment_cell_width` — the widest option's floor — and
the row no longer stretches into what it is handed; the width caps several editors had grown to
stop the stretch went with it. **Controls that stand together share one cell**: `shared_cell_width`
measures the stack and `segmented_with_cell` takes it explicitly, or `segmented_stack` scopes it so
the collection's binding (`mxm_preset::binding`) needs no new argument. The scope ends with its closure.
`a_segmented_control_is_as_wide_as_its_widest_option_and_no_wider`,
`a_stack_of_segmented_controls_shares_the_widest_cell` and
`a_scoped_stack_shares_its_width_and_ends_with_its_scope` hold it.

### A toggle is its label and a pad either side

Toggle labels are painted unclipped, so the width is the label with `SPACE_3` either side — and
**no wider** (the owner, 2026-09-23): the old stretch for short labels is gone, and so is
the leading ●/○ mark, because *"the pronounced border is enough"* — the fill and a border that
doubles in width when on carry the state without resting on hue. The width does not depend on
`marked`: the modulation dot sits in the corner padding (`MARK_INSET`), so a toggle never resizes
under the pointer. Toggles that stand together share the widest label's width — `shared_toggle_width`
with `toggle_sized`, or `toggle_stack` as a scope. `toggle_min_width` exposes the number for a layout
that places a toggle in a column. `toggle_compact` is the dense inventory-row form: a square at the pointer floor with one
short visible mark, a thin resting border and a stronger active border; its separate full label owns
the tooltip and accessibility name. It is not permission to abbreviate ordinary standalone toggles.
`a_toggle_is_never_narrower_than_its_label`,
`a_toggle_is_as_small_as_its_label_and_marking_it_moves_nothing`,
`a_stack_of_toggles_shares_the_widest_label` and
`a_compact_toggle_is_square_at_the_pointer_floor` hold them.

### A toggle's painted label and its name are separable

`toggle_labelled` paints one string and reports another to `WidgetInfo` and the tooltip, and
`toggle_min_width` measures the painted one; `toggle` is it with the two the same. This is
`ParamView`'s `label`/`name` split one control down, and it exists for the same reason: where the
card already supplies the prefix, the button can say `Trigger` while the parameter stays
`Step 3 trigger` for host automation, a screen reader and a test that looks a control up by name.

**The width follows the painted label**, which is the whole point — five `Step N trigger`
switches side by side are wider than the one-card minimum and five saying `Trigger` are not. Shortening a name
instead would have renamed the parameter, and shrinking the type to fit is forbidden by design
system §4.2, so the split is the only move that was available.

### Density has cited floors

The pointer floor is 24 × 24 points, the WCAG 2.2 AA Target Size (Minimum), SC 2.5.8; the
collection draws its controls at `MIN_TARGET`, above that floor. Design-system §4.2 keeps 40 × 40
as the preferred target for controls reached while performing. Density comes from
compact geometry without shrinking the type scale.

**One knob diameter, three semantic tiers, one column.** `Size::diameter` returns
`KNOB_DIAMETER`; the tier decides whether the value is always visible. **Every knob in every editor
stands in `control::knob_column`** — its diameter and `SPACE_5`, at least `KNOB_COLUMN_MIN`,
mxm-mono-08's (the owner's standardisation, 2026-09-24) — and **a row of knobs is
`tree::knob_row`**: equal columns capped at the sum of theirs and the gaps between them, so a card
with room draws each exactly at its column, shrinking only as far as the widest knob allows; a
knob in it declares no column of its own. **A knob that shows its value holds its widest reading
whole** (`knob_size`, and `knob` when its view names `widest`): the value is one line and
truncates, so a column holding only its longest word printed `0.060 s/o…` at a card's floor.
Editors had set the column five ways, and one stretched its rows across the card. A name or
reading wider than the column still widens its own (the tree's knob leaf). Consumers use these
rather than copying the numbers.

### An envelope and a mixer are a row of faders

**An envelope and a mixer are a row of faders** (the owner, 2026-09-25: *"ADSRs are typically
sliders"*). `tree::fader_row` sets `tree::fader`s side by side in equal columns as wide as the widest,
each `control::FADER_HEIGHT` tall, so a row reads as one set of levels. An envelope's faders paint
*A*, *D*, *S*, *R* (*"It is a convention"*); the full name stays in the tooltip, the host and
AccessKit.

## The app bar

### The app bar exposes optional preset slots without burdening simple callers

**The app bar's save is "Save preset", and it always asks for a name** (owner, 2026-09-22). It
used to read "Save" and branch invisibly: with a factory preset loaded — which is what every
instrument opens on — it silently became Save As, so a button labelled Save opened a dialog and
wrote a copy. The owner asked what it saved, twice. Now there is one path and the **name decides**:
keep the prefilled name and the naming row's own "A preset of that name exists / Replace it"
confirms the overwrite; change it and a new preset is written. `Save as…` left the `…` menu with
that change, because it had become the same click through a second door.

`AppBar::show_with` exposes §3.1 slots 2–4 for preset navigation and actions. `show` delegates to it
for callers without presets, so both entry points share one layout without requiring empty closures.

**The right-hand group is laid out first.** Filling left to right first would let a long preset name
push the utility menu off the bar — the interface moving because of what a preset happens to be
called. For the same reason the name itself is a **fixed-width** control: it is the widest thing in
the bar and the one whose content changes most.

**Every control in the bar is one height.** `action_size` fixes height while allowing label-driven
width; `the_preset_controls_do_not_make_the_app_bar_taller` guards the 44-point bar.

`preset_browser` knows nothing about presets. A row is a name, a word for where it came from, and
`Some(reason)` when it cannot be chosen — a browser that knew about parameters or files would be a
browser only one instrument could use. A row that cannot be loaded is **shown, disabled, carrying its
reason**, because omitting it would tell somebody their preset had vanished; and `step_preset` skips
those rows, because an arrow that stops on one appears to do nothing. `PresetState::save_disabled`
is a temporary transaction gate distinct from read-only ownership: it disables Save and Save As
without disabling navigation or Init, so an opt-in caller can supersede pending work while preventing
a half-applied patch from being saved.

### The bar's selectors are shared, and they are the caret form

`shell::theme_control` and `zoom_control` both use `control::selector_inline`, so scale and theme
share the collection’s caret form instead of local combo-box variants.

### The app bar compacts itself

**The app bar compacts itself** (design system §3.1): `AppBar::show_with` chooses a
`shell::BarStep` from the widths it measured on the last frame, publishes it while its closures run
(`shell::bar_step`), and `zoom_control`, `theme_control` and `preset_browser` read it — so every
editor with a preset row compacts with no code of its own, and a control outside a bar is never
compact. **Only a bar whose last frame drew `preset_browser`'s `…` menu compacts**: `AppBar::show`,
with no presets, keeps its theme and zoom. Below the last step the preset group is clipped to the
room the right-hand group leaves, cut off at its left so the `…` menu stays whole
(`compact_bar_tests`). **The measured width counts the gap between the groups as `SPACE_5` and the
item spacing**, because `add_space` comes on top of it; counting `SPACE_5` alone chose a step one
spacing too wide near every boundary, and the `…` came out cut short. The test holds the menu
inside its group's clip, not merely left of the right-hand group.

**A product's own bar actions are the last step** (`BarStep::ProductActionsInMenu`, owner,
2026-09-26). A product draws them through `shell::product_actions` with a list of `MenuItem`s:
while the bar has room its closure draws them and their width is recorded like any piece; at the
last step the closure is skipped and the `…` menu lists the items — rows in, actions out, as the
preset browser works — and `take_product_action` hands the product the one chosen, for it to act on
through its own binding. A product that registered the drawn form as a keyboard-cursor bar card
calls `navigation::bar_card_absent` when it is not drawn, or the cursor keeps last frame's
rectangle. `the_products_own_actions_move_into_the_menu_last` holds it. **Every editor's minimum
window holds the bar at that step** — design system §4.3 — and
`mxm_plugin_test::opening_size::bar_holds_from_the_minimum` measures it.

**A scroll list keeps its content clear of its bar** (`shell::scroll_list`). egui's scroll bar
floats over the content, which gets the whole width, so the Parameters views' last column ran
under it; the list is inset on the right by the bar at its widest
(`a_scroll_lists_content_stays_clear_of_its_bar`). Every editor's Parameters view uses it.

### A control in the app bar is laid out along it, not across it

`slider` stacks name/value over the track, as §7.1 requires in a card. The one-row app bar instead
uses `slider_inline` so its controls share one height and baseline.

**A stacked slider's value never draws over its name** (the owner, 2026-09-24). Right-aligned into
the space the name left, it was pinned to the column's edge regardless, so a name and reading wider
than the column painted on top of each other inside the card, where no width check sees it. When
they do not fit, the value follows the name and runs past the edge — visible, and found by a card's
floor measurement — and `slider_size` states a line wide enough for the name, the widest reading
and the spacing between them, so at its floor a slider never needs to. The room is the widest of
the current and `widest` readings, so a value changing length does not flip the choice mid-drag.
Piloted on mxm-mono-08, then every editor's (`plans/plan-editor-standard.md` R1).

`slider_inline` is the same control turned: name, track, value, one line, at `MIN_TARGET`.
`slider_vertical` turns the track instead: name above, formatted value below, with the same edit,
gesture, reset, text-entry, focus and tooltip semantics.

**It states its direction and bounds its region.** The app bar’s right group lays out right-to-left;
the inline slider must allocate its measured text/track width, then establish left-to-right order
inside that region. Otherwise its labels reverse or it consumes the remaining bar.
**Its value room is the widest reading, not the current one.** Because the group is right-to-left,
a region sized to the current value moves the track under a dragging pointer whenever the text
width changes (`0.0 dB` → `-12.3 dB`), which the owner saw as jitter (2026-09-18). A binding passes
`ParamView::widest`, normally `control::widest_value(|n| param.format(n))`, and the slider reserves
that width. Leaving it empty keeps the old current-text measurement, so only app-bar callers need it.
**Everything about the interaction is the stacked one's** — the same `slider_edit`, `paint_slider`,
text entry and tooltip — because only the axis differs and a second interaction path is how two
sliders start behaving differently. Its `width` is the **track alone**, not the whole control: the
name and value sit beside it now rather than over it.

A row that mixes heights has the same fix wherever it appears: set `ui.spacing_mut().interact_size.y`
once for the row, as `browser.rs` does, and let everything in it settle to the same target. Settle
it **upward** — 28 is what the collection draws, §4.2's floor is 24, and shrinking a control to
match a smaller neighbour spends a pointer target to fix an alignment.

## The preset browser

### The preset browser is three panes, and the name in the bar is its door

The preset name reports `PresetAction::browse`; `browser::panes` opens **banks, categories and
presets with search**, followed by *Import…*, *Export…*, *New bank…*, *Open folder* and *Close*. Rows in, actions out, as everywhere in this crate: a pane row is a
label with a count, a preset row is a name with a detail beside it (*Factory · Bass*) and a star,
and what the person did is a `BrowserAction` the caller — `mxm-preset`, which knows the files —
acts on. A `picker` puts a question with a few answers over the list (*replace this bank?*, which
file to import) and the list waits.

**Every row is painted and named.** A pane row and a preset row are `MIN_TARGET` tall and painted
so the name, the detail and the star sit where they are told, which means they have no name until
`widget_info` gives them one — and `every_row_and_action_is_in_the_accessibility_tree` holds that
each has, because a browser an AI cannot read is a browser nobody can debug from the CLI. A broken
preset is shown, disabled, carrying its reason on hover, and a click on it reports nothing
(`clicking_a_category_and_a_preset_report_their_rows`).

**A filter pane is a fifth of the width, held between the narrowest a bank name reads at and the
widest that leaves the list room** (`pane_width`, held by
`a_pane_is_a_fifth_of_the_width_within_bounds`), so in a narrow editor the two panes narrow first
and the list keeps its room. The caller places the overlay with
`browser::overlay_rect`: the panel's width less a gutter, from under the bar to the panel's bottom
less the same, so the bar — the name, the arrows, Save, the modified marker — stays in sight while
the browser is up (§3.1: *preset access must always remain clear*). **The keyboard lives in the
search box while the browser is open**: it keeps focus, with egui's own arrow-key focus travel
locked out of it (`set_focus_lock_filter`), so typing searches, and the arrows are the browser's: **left and
right move the keyboard between the three panes** (the focused pane's title lights and its selected
row is ringed), **up and down move within the focused pane** — stepping the presets, with the list
scrolling to the selection, or choosing the next bank or category — and **Enter is done**: the
caller loads the first match if nothing is, and closes. Escape closes. The keys are read and
consumed before the search box is drawn, or that box would also move its caret on left and right. Without the lock every arrow press also walked the focus
ring onto a row or a button, where Enter would then have clicked whatever it landed on.

## Cards, groups and levelling

### A group is the card's own frame, one level down

`shell::group` — a hairline on `border`, `RADIUS`, `SPACE_3` of padding, and **no fill**.
`ModuleCard`'s outline with no header and no bypass, for the clusters inside a card that each read
as one thing: a modulation destination with its meter, its amount and its source. It implements §4.2’s nested boundary.

**A group is a boundary, not a surface.** It has no fill; `surface_2` remains available to distinguish
controls inside it.

**It sets its body's `item_spacing.y` to `SPACE_2`, and that is the point of it.** A card body
inherits egui's default 3.0 — off the four-point grid §4.1 requires, and so nearly equal to the
space between one cluster and the next that mxm-mono-08's routing column read as one
undifferentiated list; the owner's word was *indecipherable*. A group's ladder is 4 inside against
8 plus a border outside, which is §4.1's proximity rule made mechanical. The **caller** owns the
space above a group, because only the caller knows whether anything precedes it.

`a_group_is_an_unfilled_hairline_panel_that_tightens_its_body` holds the absent fill, the hairline
and the rhythm. **Recorded, not resolved:** §4.2 asks for a card radius of 8 against a nested radius of 4,
while `RADIUS` is 4 and every card already uses it. Both are drawn at 4 today. Changing the card
radius repaints every plugin and is its own change.

### A card body's rhythm is `SPACE_3`, and a group's is `SPACE_2`

`ModuleCard::show` sets `item_spacing.y = SPACE_3` for its body. egui’s default 3.0 is off §4.1’s
four-point grid and does not separate adjacent controls clearly.

That gives the ladder §4.1 asks for, in one place each: **4** inside a `group`, **8** between a
card's own children, **8 plus a border** between one group and the next. A consumer that needs
something else says so locally; nobody needs to remember a number to get the default right.

### Centre what is in a column, do not leave it where it lands

Mixed-height rows center their cells independently, and a knob is centered inside its full-width
column rather than left at the allocation origin.

**A label centred over nothing is worse than no label**: it looks like an answer. So the knob is
allocated as a band the column's width and centred inside it.

`a_knob_sits_in_the_middle_of_its_column_like_its_label_does` reads the **painted arc** rather than
the layout — `paint_knob` centres its arc on the rect it is handed, so where the arc ended up is
where the knob is, which is the thing a person sees and the thing that was wrong. Without the
centring it reports the knob 12 px off in a 72 px column.

### Columns of cards end on one line — for the views that are not a flow

§3.3: cards laid out side by side share a bottom edge. In a reflowing panel that falls out of
`flow::cards` below; where a view instead deals a **fixed number of columns**, `shell::level_columns`
is the mechanism, and
`shell::Level` is the same measurement for a row of cards that are told their widths instead of
dealt by `Ui::columns`. Immediate mode cannot know the tallest column
before the others are drawn, so each column's natural height is measured this frame and the
difference handed to the caller the next, as a `deficit` the caller **shares equally among the
column's cards, inside them** — `shell::grow(ui, share)` at each card's foot, or a display that is
strictly better bigger, as mxm-mono-03's and mxm-poly-06's filter curves are. Inside the cards, so it
is their bottoms that line up rather than the empty space below them; shared, because putting a column’s whole deficit in its last card creates a visibly empty card.

**Spend it with `grow`, never `Ui::add_space`.** The cursor already sits one item spacing below
the last widget, and `add_space` commits that spacing to the card along with the amount — and only
when it is called — so a padded card came out six points taller than its padding and an unpadded one
not at all. The leveller subtracts only the padding, so a padded column measured six points taller
than it was, and where two columns were within six points of each other the tallest flipped every
frame: mxm-mono-01's columns at 900 px, caught by its own paint tests the moment the padding was
shared among cards. `grow` extends the card's rectangle and nothing else, so zero grows it by zero.

**What is remembered is the natural height, the deficit subtracted back out.** Store the padded
height and the correction feeds on itself: the short column reads as tall, the deficit vanishes, and
the two alternate forever. `the_levelling_does_not_feed_on_itself` holds that at twelve frames
against three. `columns_of_cards_end_on_one_line` reads where the columns actually ended, which is
also what `levelled(ctx, id)` hands a consumer's test — every instrument asserts its own columns from
it, and measures its frame's height from it (`plugins/AGENTS.md`, the editor contract).

`level_columns` remains shared shell geometry for fixed-column consumers; paged musician surfaces
use `flow` instead.

## Flow and the layout tree

### Cards in wrapping rows — `flow`

`flow::cards` is the collection's reflowing panel layout: cards in rows, every card in a row
starting and ending on one line, a row as tall as its tallest card. It is the flow's **single
scroll owner on both axes**: later rows scroll vertically, and at increased zoom a card floor wider
than the remaining logical viewport scrolls horizontally. Callers must not wrap it in a second
`ScrollArea`. `flow::pack` decides the row breaks; `flex_wrap` cannot, because it wraps single items
and has no idea that two cards belong together.

### A card body as data — `tree`

The owner, 2026-09-24: *"all these cards have easily calculatable minimum and maximum sizes.
Because we know all the components on them. And the ones that are not visible have simple rules
for their sizes."* A body written as drawing code has no structure until it runs, so its size could
only be found by running it — typed floors policed by squeezing, and a hidden context for heights.
`tree::Node` describes the body instead, and **one tree is both measured (`min_width`, `height`)
and drawn (`show`)**, so the two cannot disagree. Every plugin card is one
(`plans/plan-layout-tree.md`).

- **A leaf's size is its control's own function, from the fonts and style alone**, beside the
  control in `control.rs` (*Sizes without drawing*): knobs in any column, sliders (quiet or not,
  vertical), segmented controls above, beside a knob and at a shared cell, wave grids labelled,
  unlabelled, marked and beside a knob, compact and picture toggles, both selector styles, the
  remove mark, and the plain egui buttons, checkboxes, combo boxes and sliders that two effects
  use unwrapped. A filling leaf takes the width offered; every other leaf is drawn at its own.
- **Containers have one rule each**: rows (a gap; a `Height::Row` child takes the row's height),
  `Columns` (`ui.columns` as data — a cap that shrinks to the widest child, or stretched across the
  width), `Grid` and `Wrap` (as many per line as the width holds), stacks, groups (`GROUP_INSET`,
  `SPACE_2` inside), `Share` (every toggle, or every segment cell, in a subtree at one width —
  `toggle_stack` and `segmented_stack` as data), `Reserve` (draws one child, sized to the largest of
  its alternatives), `Disclosure` (both headers), `Pad`, `Beside` (onto a knob's circle or a
  segmented control's cell line), `Center`, `Disabled`.
- **Two heights: natural and reserved.** `height` is what the tree shows now; `reserved_height` has
  every disclosure open and every `Reserve` at its largest. A card is sized to the reserved height
  and draws the natural one at its top, so what is not shown leaves its room at the card's foot and
  nothing outside the card moves when it appears. A disclosure is drawn as the tree was built — a
  click shows from the next frame, which the header asks for — and a collapsing header's body opens
  over egui's animation time, its natural height following the openness, clipped as egui clips it.
- **A composite states its own size** as `Kind::Custom` — `mxm_modulation_params::ui::stack_size`,
  and every plugin visual, stated beside its drawing. `shell::card_floor` and `card_height` add a
  card's chrome; `tree::card_floor` is a card's content floor.
- **A size function sits beside its control, and a test draws the control and compares**
  (`tree::tests`). A change to one without the other fails there, once, rather than as a card that
  overflows somewhere.
- **Heights are measured in the `Ui` the body is drawn in**: some rules read the item spacing, and
  a card body's is `SPACE_3` — `shell::body_ui` gives one. **Measure in a context whose fonts have
  settled**: the weighted cuts bind on the third pass.
- **Widget ids survive reflow**: a leaf's `Ui` is salted with its key, never its position.
- **Known, and not the tree's**: a knob whose name wraps paints it 0.56 points above its name box —
  two wrapped Body rows are 32.0 tall and the box is two row heights, 31.44. The size reports the
  box; `mxm_plugin_test::tree_checks` tolerates three quarters of a point. Fixing it moves every
  knob in the collection, so it is its own change. **Paint may also reach past a leaf's room into a
  gap**, as it always has — a slider's handle at its ends (up to 4 points), a visual's stroke or
  marker; the checks forbid paint over another leaf's room or outside the card, not into a gap.

## Paging

### Paging is a separate layer above flow

`paging` owns the pure category-first planner and selection state; `paging::editor` integrates
every plugin editor. Order is Performance → Modulators → Sequencers → Generators → Tone →
Effects, then authored order within a category. Stable `Key`s belong to callers, not current array
positions. `Category::from_request` explicitly maps developer CC 0–5; Parameters is 127 and never
merges or appears in musician navigation. These are not controller-page indices. Use full names.

- Whole categories merge first; an oversized category splits into contiguous kind runs, preferred
  groups, then individual cards. Groups must be disjoint, contiguous and same-category. Invalid
  metadata returns an error rather than dropping, duplicating or reordering cards.
- **A whole category never costs a page** (the owner, 2026-09-24, on mxm-para-07). `partition` also
  fills each page in order from preferred groups and single cards, splitting a group too tall for a
  page, and takes that partition only when it has fewer pages. Category-first alone had put
  Performance's three cards on a page by themselves once the Modulators fitted a page whole.
  `a_whole_category_never_costs_a_page` fails against category-first alone, and
  `categories_stay_whole_where_filling_saves_nothing` holds the other half.
- Heights are outer heights at the candidate row's actual outer widths. `flow::row_widths`
  uses the renderer's own Taffy styles. **`paging::editor::show` takes each card as a
  `crate::tree`** and fulfils `Outcome::Pending` from the trees — a card's chrome plus its tree's
  reserved height at the body width — so nothing is drawn to learn a size, hidden cards included.
  While an interaction holds the page its partition is kept, but its rows are still drawn at this
  frame's trees' heights at the widths drawn — never a planner sample, which a tree that changed
  under the gesture would leave stale or missing (`a_held_page_draws_its_cards_at_this_frame_s_trees`).
  The trees are rebuilt every frame and the heights with them, so there is no revision and nothing to
  invalidate: a disclosure, a selected slot or a warning line re-plans at once. Consumers still
  snapshot destructive telemetry before `show`, because painting reads it.
- A one-page solve uses the full workspace and no bar. A multi-page solve reserves a bar height
  that may only increase within that solve, since *more* tabs can have *shorter* labels and fewer
  rows. Exhausted navigation and failure to converge are explicit errors, not loops.
- `State` anchors selection to a card through splits/merges and chooses a surviving neighbour on
  removal. It defers replacement while interaction is owned; incomplete measurement keeps the
  current page set but invalidates stale deferred work. Hysteresis accepts a conservative merge
  candidate only when it saves pages; the caller supplies real-budget fit evidence. Inventory
  changes must not disappear into the dead band. Integration uses one `SPACE_4` of spare height
  for a merge: a chosen UX margin, not a measured physical constant. Native resize feel still
  requires inspection. `hold` covers browser/naming/search/exact entry before developer requests;
  pointer-down, release and popups also defer replacement and queued navigation.
- `Scroll::None` bypasses the scroll container for a proved-fitting page. Indivisible overflow is
  explicitly marked and needs `Scroll::Both`; paging cannot make a tall/wide card smaller.
- **A page that has to scroll anyway takes what fits beside its tall card** (the owner, 2026-09-28,
  on mxm-mono-00 in a short window: two tabs each carried one oscillator). When the page being
  filled already overflows its height, the next unit joins it if every row still fits the width and
  **it adds no row** (`Solver::rides_along`): it sits beside the tall card, so the page scrolls no
  further than its tallest card needs wherever it went, and the second oscillator costs no tab. A
  first build asked that the page grow no taller at all, and mono-00's Oscillator 2 — a little
  taller than Oscillator 1 — still took one. A unit that would add a row still starts a page; at
  1690 × 540 mono-00 is three pages, each a whole category, where it was four.
  `a_scrolling_page_takes_what_fits_beside_its_tall_card` holds it (falsified with the rule off and
  with the no-taller rule);
  the tiny-window review uses a window too narrow, and the compact-navigation review cards too
  wide, for two to share a row, so they still walk one card per page.
  Existing `cards`/`Options::default` scrolling is unchanged.

`ViewBar` cells share the widest label’s measured text/pointer floor and wrap in reading order.
`geometry` reports the exact rows, height and over-wide-cell condition for a paged caller;
`show_paged` draws nothing for zero/one page. Existing `show` retains its singleton until editor
adoption. Both publish accessible selected labels and support pointer/keyboard activation.
`paging::editor` falls back to Previous / Page / Next when wrapped navigation exhausts the budget.
Card bodies get a stable card-keyed, top-down Ui; only the selected page registers visible controls.
Reserve a row's body floor **before** drawing: egui 0.36's `set_min_height` is cursor-relative,
so reserving after drawing adds the requested height again. A row's cards all take its height —
the tallest card's reserved height — and each tree draws its natural height at the top.

`tests/paging.rs` covers ordering, grouping, missing width-qualified measurements, feedback,
selection, deferred gestures, hysteresis, overflow and actual rendered width/height readback.
`tests/view_bar.rs` covers both themes, wrapping, painted labels, allocation and pointer/keyboard
navigation. `tests/paged_editor.rs` exercises the integrated renderer, held/queued navigation, and
tree cards laid out at the heights their trees state — re-planning when a tree grows. Plugin tests run
`mxm_plugin_test::paging_checks` for real-card reachability, category order, floors/caps, per-page
bounds and aligned rows at 1×/2× with a fixed simulated physical budget. These are headless proofs,
**not** native interaction or physical DPI gates.

**Scroll reachability is the painted union, not the viewport width.** Card-keyed `new_child`
Uis do not allocate in the parent; the renderer allocates their complete union to the scroll area.
Hysteresis retains a partition only when its navigation mode matches and its reserved height still
covers the rendered labels after font/style changes. Compact navigation budgets and paints the same
text-measured, wrapping cells, with `MIN_TARGET`-high buttons and fixed-width monospace page numbers;
changing pages cannot move the buttons. Its complete height, including the trailing gap, is reserved.

`tests/pager_review.rs` runs by default: real horizontal wheel movement to the far edge, compact/
wrapped height sweeps, and compact pointer/keyboard navigation through twelve pages in both themes
at narrow widths. `plans/plan-dynamic-view-paging.md` §11 owns the review evidence; these headless
tests do not establish native-window quality.

## What a resize costs

### What a resize costs, and the two knobs that are not on by default

Resize work has two independent layout costs below. Headless frame timings do not establish native
window FPS; floating-resize host wake behavior belongs to the forks' `PATCHES.md`
([mxm-audio/nice-plug](https://github.com/mxm-audio/nice-plug),
[mxm-audio/egui-baseview](https://github.com/mxm-audio/egui-baseview); the monorepo's
`vendor/AGENTS.md` before the split).

1. **Every resize frame is laid out and drawn twice.** `egui_taffy` recomputes the tree and calls
   `Context::request_discard` whenever `taffy.dirty(root) || last_size != root_rect.size()`
   (`egui_taffy-0.14.0/src/lib.rs:632`). The root rect is `Ui::available_rect_before_wrap`, so a
   drag changes it every frame and every frame is discarded.
2. **Every card is drawn whether it is on screen or not.** `leaf` calls the body closure for all of
   them, every pass. Immediate mode has no culling unless you write it — which is what
   `ScrollArea::show_rows` exists for. A narrow window wraps ten cards into many rows and shows two.

[`flow::Options`] answers each, and **both are off by default, so every editor ships today’s
behaviour.** Turn one on with a measurement in hand, never on the reasoning above alone:

- **`quantum`** rounds the rect the layout is computed in *down* to a multiple of n points, so a
  smooth drag crosses a boundary every n points instead of every frame. The cards lag the window
  edge by up to one quantum and snap when it is crossed.
- **`cull`** skips a card that is wholly outside the viewport, reusing the height it was last
  measured at. **It is guarded on that width**, because a height is valid for only one width;
  `culling_does_not_move_a_card` guards the invariant. That guard is also why culling does
  nothing for a *horizontal* drag on its own — the width moves every frame, so nothing is reusable.
  **Pair it with `quantum` and the two compose**, because between boundaries the width is still.
- **`scroll: Both`** is a third option and is not a performance one: it buys reach to a card wider
  than the viewport at high zoom, and costs passes because a scrollbar changes the rect Taffy
  compares against.

`tests/flow_resize_bench.rs` measures all of them and prints passes and card-draws per frame. It is
`#[ignore]`d — a measurement, not an assertion — and needs no window, GPU or plugin:

```bash
cargo test -p mxm-ui --release --test flow_resize_bench -- --ignored --nocapture
```

**`level_columns` is not gone and is not deprecated.** It still serves the views that are not a
flow in the player; plugin musician surfaces now use paging. What it cannot do is a reflowing panel: it has a **column
count**, and it stretches a card to a *column's* height rather than a *row's*, which produces a card
with nothing in it the moment one column is much shorter than its neighbour.

**The trap it records is carried over unchanged.** A card stretches to its row and must not feed
that stretched height back into its own measurement: what is remembered is the natural height, the
growth subtracted back out. `flow` does the same arithmetic `Level` does, for the same reason.

**Give `cards` a ceiling.** A row's children share its width, so a row holding one card stretches
that card across the whole panel; a card much wider than the controls in it is empty space wearing a
border.

## The keyboard cursor

### The keyboard cursor is a card, a parameter and a value — and its map is drawn, not declared

`navigation` is tracker-style editing with the owner's physical hierarchy
(`plans/plan-keyboard-editing.md`). **`Shift`+arrows move module/card to module/card,
`Command`+arrows move between parameters inside one, and bare arrows set the value** — `Command`
being `Ctrl` on Windows and Linux, `Cmd` on macOS. The higher modifier selects the higher level.

- **Left/right is fine and up/down is coarse**, matching the M8 orientation. The plan §1 names the
  Dirtywave manual pages; the manual gives no increments, so the ratio is ours.
- **The parameter-to-card map is a by-product of painting.** A control calls `navigation::mark` as
  it draws, inside a `navigation::card` scope the paging renderer opens and a `navigation::at`
  scope the plugin's binding opens. There is no authored table: it would drift when disclosures or
  paging change. The registry is therefore always exactly what is on screen. A segmented
  parameter unions all its cells into one spot and retains every cell's focus id.
  **`navigation::aside` closes the scope** for an editor-only control drawn inside a parameter's —
  a picker that chooses which parameter the control beside it edits. Without it the picker
  registers as a second cell of that parameter and answers the same bare arrow, so one press both
  steps the menu and moves the value.
- **`navigation::paged` is how an editor drives it, and the order is the plan's.** It reads the
  last frame's `paging::editor::report`, flattens the plan's category-first card order across
  pages, hands `run` the exact visible rectangles, requests a card the cursor reached on another
  page and sets the outline. Editors must not derive a second order from raw authored cards.
  **It asks the caller for no card list**: a spot is only
  registered inside a `card` scope, which only `paging::editor::show` opens, and `show` stores its
  report at the end of that same frame — so a non-empty registry implies a report, and without one
  the cursor has nothing to move.
- **An app-bar parameter is a bar card.** Design system §3.1 puts the master output in the app bar,
  above the paging renderer, where the report cannot see it. The editor draws it inside
  `navigation::bar_card(ui, key, …)`, which opens the card scope and records the painted rectangle,
  and drives the cursor with `navigation::paged_with_bar(ctx, state, inert, &[key])` instead of
  `paged`. Bar cards come first in the cursor's sequence, use last frame's geometry and are never
  requested as a page. The key must not collide with a paging key. Every instrument uses it for
  its master output (`plugins/AGENTS.md`); `paged` is `paged_with_bar` with no bar cards.
- **Card direction comes from the paging renderer's exact rectangles.** Every arrow stays in its
  painted half-plane; horizontal movement prefers the same row, and vertical movement takes the
  adjacent row before a better-aligned later one. At a page edge the paging plan's category-first
  card order is the bridge to the next page. Raw authored order and unions of inner control rects
  are not card geometry.
- **The cursor drives egui's focus, but the cursor target remains the keyboard authority.** Custom
  painted controls can lose native egui focus between frames; value editing must still reach the
  parameter the visible cursor names. `EventFilter`'s arrow fields are `true`: in egui that means
  exclusive to the focused widget, so one press is not also spent on egui's focus travel. `Tab`
  onto any cell whose response retains focus moves the cursor there.
- **The pointer moves the cursor: the parameter clicked or dragged is the next arrow's target**
  (owner, 2026-09-23: *"When I click on a parameter, or move it with the mouse it should
  immediately be possible to edit that parameter with the arrow keys"*). egui 0.36 focuses no
  painted control on a click or a drag — only `TextEdit` and `DragValue` ask for focus — so a cursor
  that followed focus alone stayed on the previous parameter, hidden, and the next arrow edited
  that one. `navigation::mark` therefore takes the control's `Response` and records whether this
  frame pressed or clicked it (`Spot::pointed`, OR-ed across a segmented control's cells); `run`
  settles on a pointed spot **before its `inert` return**, so a press followed by an inert frame is
  not lost, and hands the control egui focus once nothing else holds the keyboard. Read from the
  widget's own interaction, never a hit test of last frame's rectangles: a press on a popup over a
  knob is the popup's. Three edges:
  - **`remove_mark` never claims** (`mark_unclaimed`): its press deletes the row it sits on. It
    stays a cursor target operated by `Enter`/`Space`, and answers no arrow.
  - **A pointer frame skips the focus-follow.** egui surrenders focus on a click but not on a
    drag, so the knob the cursor left can still hold focus while another is dragged, and following
    it would undo the pointer's move.
  - **Focus that could not be handed over is owed** (`State::owed`) and requested on the first
    frame that is not inert. egui sets the arrow lock only on a widget that already had focus last
    frame, so the lock arrives one frame after the focus, as it does after a `Command`+arrow move.
- **A focused text field or any open popup makes the cursor inert**, exactly as the editor's own
  `inert` does: no reveal, no consumed `Shift`/`Command` arrows, no target. A long selector's search
  field edits its text with those keys, and any open list walks its rows. It reads
  `Context::text_edit_focused()` and `egui::Popup::is_any_open` — **not `Context::any_popup_open`**,
  which is built from the popups drawn so far in this pass, and `run` runs before anything is drawn.
- **`Alt` is a finer layer of the value tier** (the owner, 2026-09-24). Under the cursor an
  unmodified **or `Alt`** arrow edits the value, and `Press::finer` says which. In each layer up/down
  is the larger step: 10 % and 1 % without `Alt`, 1 % and 0.1 % with it — a pitch's law makes those
  an octave, a semitone, ten cents and a cent. A segmented control, a toggle and a selector take an
  `Alt` arrow as a bare one, because an option list has nothing finer than the adjacent option.
  `an_alt_arrow_is_the_finer_layer_under_the_cursor` holds it.
- **How far a press moves is the parameter's, not this crate's.** `control::Steps` carries four
  normalised magnitudes the plugin computes from its own parameter each frame, and
  `Steps::DEFAULT` is 1 % fine and 10 % coarse for a control whose owner supplies none; under `Alt`
  the fall-back is fine for up/down and a tenth of fine for left/right. An owner
  with a law that depends on where a press starts — a semitone on a skewed hertz range, a step onto
  the whole semitone a readout shows — hands the control a `control::NextValue` through
  `ParamView::stepping_by`, asked once per press from any value. The plugin's binding implements
  it over `mxm_preset::StepLaw`; this crate still knows no parameter.
- **Presses apply one at a time, in the order pressed.** `take_arrows` walks the event queue, as
  `navigation::requested` does; `consume_key` returned presses grouped by key, which is harmless
  for fixed magnitudes and wrong the moment a step depends on its start (Down-then-Up at the top of
  a range must end at the top). Segmented controls read their arrows the same way.
- **A held continuous edit is one host gesture, anchored as a drag is.** The first matching event
  begins it, repeats only emit values, and release ends it. All matching events in a frame apply,
  each from where the one before landed. **The gesture's memory carries the value its last press
  sent**, and later-frame repeats chain from it rather than from a readback the host may not have
  applied yet — `DragAnchor`'s reasoning, and like it the anchor dies with the gesture. Behaviour
  across separate gestures under a late host is pre-existing and out of scope: every discrete edit
  starts from readback, and hosts apply within a buffer, faster than a person moves between them.
- **A toggle under the cursor answers the bare and `Alt` arrows** as a two-cell segmented control: Right/Up
  on, Left/Down off, `Home`/`End`. Only under the cursor (`keyboard_target`), so the player's panes
  and a disclosure toggle, which is never a cursor target, are unchanged.
- **A selector is a stepped parameter drawn as a menu**, and answers the arrows on `segmented`'s
  terms — the adjacent option, `Home`/`End`, `Command`+`Backspace` — but never while its own popup
  is up, where egui's list owns the keyboard (after the search field, in a list long enough to
  have one). It marks the caret button. Without this the cursor
  could not reach a routing surface at all: mxm-mono-08 draws its whole source list as selectors,
  and mxm-mono-00 did the same for its retired fifteen patch-bay rows before those became routes.
- **`navigation::running` keeps a cardless surface safe.** Where no cursor runs, `control`’s
  bare-arrow editing remains active. An editor calls `navigation::stop` on a cardless surface
  such as Parameters; merely hiding the outline leaves an invisible stale cursor consuming that
  surface's arrows.
- **`consume_key` ignores an extra `Shift` or `Alt`.** A `Modifiers::NONE` pattern therefore also
  matches `Shift`+arrow, so the most specific modifier must be tested first. Getting this backwards
  turns every parameter move into a card move, and nothing fails loudly.
- **The card cursor is painted, never laid out**, outside the card's rectangle exactly as the focus
  ring is drawn outside a control's — a cursor that occupied space would move every card beside it
  the moment it arrived.
- **Where the cursor is and whether it is drawn are separate facts.** `navigation::shown` withholds
  the initial ring until keyboard navigation begins while retaining the first-control position,
  matching design-system §11’s `:focus-visible` behavior. `run` **conceals on any pointer press** — before its `inert` return, because
  a click into the preset browser is still somebody reaching for the mouse — and **reveals on any
  key that operates the cursor**, after that return, because while the browser is open or a value is
  being typed the arrows are that surface's. The revealing set is §11's table minus `Escape`, which
  cancels rather than navigates. The gesture is *peeked, never consumed*: `requested` and `control`
  both still need those events.

  Two consequences worth stating, because each is a thing a later change could quietly undo:

  - **The gate lives in `outline` and in `control::focus_ring`, not at the call sites.** There are
    editors reaching `outline` through `paged` and `paged_with_bar` — and a dozen
    `focus_ring` calls. A rule enforced per caller is one new editor away from being forgotten.
  - **`focus_ring`'s gate is `running`-conditional.** Where no cursor runs — the player, the
    cardless developer Parameters surface — `Tab` is the only way focus arrives at all, and hiding
    its ring would leave those surfaces with no keyboard indication whatsoever.

  **Hidden is not lost, and that is the whole point of the split.** A knob turned with the mouse
  becomes the cursor's parameter (see the pointer rule above), so the next arrow edits *that* value
  and reveals the cursor already sitting on it; a click on empty space leaves the cursor where it
  was. `no_border_until_the_keyboard_is_used_and_none_after_a_click` asserts the opening frame,
  the reveal, the click on nothing, the surviving position and the return;
  `escape_is_not_a_reveal` pins the one key in §11's table that is not this interface.

`crates/ui`'s own tests prove directional card/parameter movement, page-edge fallback, batched key
presses, multi-cell registration, the reveal rule above, the pointer rule and its three edges, the
text-field and popup suspension, and — through real controls — a clicked knob taking the next
arrow, a remove never selecting, same-frame presses chaining in order, a held key's anchor under a
host that applies nothing, and a toggle's arrows; `plugins/mxm-mono-01/tests/keyboard_editing.rs`
proves the half that only exists once a real panel has painted — category-first card order, text
and waveform segmented editing, the step law and its musical laws (a clicked cutoff steps an
octave, a dragged tune a cent, the bend range reaches its end, two batched presses are two
octaves, a held key under a lagging host one cent per repeat), one gesture across key repeats, the
cardless Parameters surface, and that `Space` and the preset browser's arrows are left alone. Every other editor carries the
shared coverage check in `mxm_plugin_test::keyboard_checks`, which is a check of the *plugin's*
scopes rather than of this crate. Headless all of them; **the feel is not proved by any**, and is
judged in `apps/mxm-mono-01-standalone`.

## Reach

`reach` came from newDAWn (2026-10-07), where the owner ruled that **everything on screen is
reachable and editable by the keyboard**, and moved here so the collection's editors can share it
when they convert to the keyboard language. Rather than a registry each control writes into, as
`navigation`'s cards and marks are, it reads the AccessKit tree egui builds every frame (enabled by
`State::begin`), through a plugin's `output_hook`: every widget's bounds, role, label and the
actions it takes. The arrows go to the nearest widget that way (one overlapping the cursor across
the way counts as in line, however wide), OPEN asks for `Click` (or `Focus`, from the next frame's
input, for a text field), VALUE asks for `Increment`/`Decrement`, ten for COARSE, unless the widget
reads its own steps with `reach::edit`. A host names its views with any key and gives each the
`Ui::unique_id` its widgets are drawn in; an open popup takes the keys first. egui closes a popup only
on a pointer click, so a press in a menu closes it unless another menu opened on top. Labels,
containers, scroll bars and unlabelled painted areas (canvases with cursors of their own) are left
out.

## The keyboard language, piloted

Where `pilot` is on (mxm-mono-08, from 2026-10-07), `navigation::run` reads the keys through the
`mxm-keys` engine instead of §11's modifiers, before any control is drawn: a bare arrow goes to the
next parameter inside the card (`Step::Any`; ← → only along the row), and to each cell of a
segmented control, which takes egui's focus so OPEN (Enter, left in egui's queue) chooses it (the
owner, 2026-10-07: the first pilot's arrows crossed cards and rows, and stepping across a control's
buttons "fits what I see on the screen"), COARSE + an arrow is the old card step, VIEW + an arrow
moves between the cards and the bars `navigation::bar` recorded (the view bar, then the app bar),
where `reach` walks their widgets. VALUE's presses, OUT's keep, BACK's cancel and DELETE's reset are
published as `ValueKeys` for the one control the cursor is on, which takes them in
`control::language_edit` (or `segmented_keyboard`, one cell a press): one host gesture until it is
kept or cancelled, chaining each press from where the last one landed, as a held arrow did. Enter,
Escape, Home, End and every chord with `Command` or `Alt` stay in egui's queue for the controls.
`mxm-plugin-test`'s coverage check presses the language's keys where the editor pilots it.

**Rolled out on 2026-10-08** (the owner: "Roll out the keyboard language"): the pilot's checks
were deleted, so every editor's cursor reads the language. BACK alone stopped revealing the cursor,
as `Escape` never did under §11 (`escape_is_not_a_reveal`). The old `Alt` + up/down layer (ten cents
on a pitch) has no key in the language. Design system §11's keyboard text before the rollout, as it
was written:

> Keyboard behavior (the table is §11 as it binds the editors today; **the keyboard language
> replaces it**, the owner decided on 2026-10-07, keys first in newDAWn and the collection, and is
> piloted on mxm-mono-08 through `mxm_ui::pilot`: the arrows go parameter to parameter inside the
> card (← → along its row, stopping at either end), stopping on each cell of a segmented control,
> whose cell OPEN chooses; COARSE + arrows card to card, VALUE + arrows change the value in the steps below — FINE the fine one,
> COARSE and MUSICAL the coarse, MICRO the finer — as one gesture that OUT keeps and BACK cancels,
> DELETE resets to the default, and VIEW + arrows move between the cards and the bars above them,
> never out of the window, which is the window manager's; in the default keymap W is VALUE, S
> COARSE, D FINE, F MICRO, A MUSICAL, C VIEW and Tab OUT):
>
> | Key | Behavior |
> |---|---|
> | `Shift` + arrows | Move the cursor from module/card to module/card |
> | `Command` + arrows | Move between the parameters inside the selected card |
> | `↑` `↓` | Adjust the selected parameter — **coarse**: 10 % of its travel, or an octave |
> | `←` `→` | Adjust the selected parameter — **fine**: 1 %, or a semitone |
> | `Alt` + `↑` `↓` | Adjust the selected parameter — **finer coarse**: 1 %, or ten cents |
> | `Alt` + `←` `→` | Adjust the selected parameter — **finest**: 0.1 %, or a cent |
> | `Command` + `Backspace` | Return the selected parameter to its default |
> | `Tab` / `Shift+Tab` | Move focus forward/back |
> | `Home` / `End` | Minimum / maximum where safe |
> | `Enter` | Activate or begin value entry |
> | `Escape` | Cancel edit or close transient UI |
> | `Ctrl/Cmd+Z` | Undo |
> | `Ctrl/Cmd+Shift+Z` | Redo |
>
> **The modifier height mirrors the selection level.** `Shift`, the higher key, moves the highest
> level: modules/cards. `Command` moves parameters within one, and an unmodified arrow changes the
> lowest level: the value. This is the owner's live-use correction to the first cursor build; it takes
> precedence over that build's M8 modifier grammar. `Command` is `Ctrl` on Windows and Linux and `Cmd`
> on macOS.
>
> **Left/right is the fine axis and up/down the coarse one.** That is the Dirtywave M8's orientation,
> taken deliberately: the tracker is where this interaction comes from and a musician who knows one
> should not have to learn the other backwards. **`Alt` is a finer layer of both, and in each layer
> up/down is the larger step** (owner, 2026-09-24: *"Some of the sliders are getting small, so it is
> important that there are enough fine control with the arrows"*; of `Alt` with up/down, *"Make it
> make meaning"*). `Alt` is `Option` on macOS.
>
> How far one press moves is a share of the control's **travel** — 10 %, 1 %, and under `Alt` 1 % and
> 0.1 % — snapped onto the parameter's own grid, so a skewed range keeps its skew: a press near 20 Hz
> moves a few hertz and one near 20 kHz moves hundreds. **A pitch moves musically**: an octave, a
> semitone, and under `Alt` ten cents and a cent (owner: *"octave, semitone, cent is the range"*), to
> the next whole one in the direction pressed. **A press never moves less than one of the parameter's
> own steps**: on an option list or a whole-semitone tune, every layer reaches the adjacent value.

## Selectors and menus

### A selector is one row, and §7.4 finally has a shared one

`control::selector` and `selector_beside` — the pairing `segmented`/`segmented_beside` already uses,
for the same reason.

A bounded parameter selector is **one row**: the name on the left and the value on the right between the shared
drawn carets, `‹ Off ›`, flat at rest and taking the theme's hover/focus treatment under the
pointer. The caret form is canonical for every bounded parameter enum; filled/bordered fields are
for unbounded or searchable lists such as presets. A column of selectors reads as a list of settings
rather than as a stack of widgets. The rules it keeps are the crate's own—the row is `MIN_TARGET`
tall whatever the text measures, interaction changes treatment rather than geometry, and the
accessible name is *name: value* because the button's own text is only the value and a screen reader
would hear a word with nothing to attach it to.

It also carries the marks a stepped control owes: the accent dot when something else is sounding,
and double-click to the default, the same gesture a knob answers with the same meaning. Reselecting
the base while another cell sounds must report an edit, as segmented controls do.

`selector_grouped` adds non-selectable headings to the open menu and changes nothing else. Its group
slice is parallel to the options; adjacent equal names share one heading, while an empty group
intentionally draws no heading for a standalone utility option such as Off. A heading uses the
semibold Heading style, visibly larger and heavier than an option so it cannot be mistaken for a
model. Headings never become keyboard cells or alter indices, and search draws only headings that
still have a matching option.
The drum machine commissioned it for voice families over a sparse permanent ID domain; the plugin
still supplies only available options, so ordinary arrows/Home/End step sounding choices rather
than reservations.

`selector_caption` is the same caret implementation in caption typography, for a standalone routing
input that belongs to no particular control. Its **button**, not merely the surrounding row, is
`MIN_TARGET` tall. The modulation mark is painted without moving the value or changing the menu's
auto-ID. A consumer must key its routing scope by the permanent target, not by its source text or
current row placement.

`selector_sublabel` is **the** §7.4 form for a source directly beneath its owning control: only the
resolved source, in caption typography and secondary ink, between drawn carets and sized to its
content rather than the column. Bare text does not advertise a menu, while fill or full-column
width overstates it. The carets sit in the button’s padding so the word remains centered between
them. It never wraps: `TextWrapMode::Extend`, because egui's default breaks mid-word and a
source in a knob's column came out as *Oscillato* over *r 2*. A word that will not fit
overflows, which points at the column as the thing to widen.

`slider_with_source` draws the selector **on the amount's name line** instead of beneath the
track, and the caller passes the label with its trailing source word removed: *"Clock period
from"* plus the selector's *Key*. `name` stays whole for `WidgetInfo` and the tooltip. It is a
second entry point rather than an argument on `slider`, the same shape as
`segmented`/`segmented_beside`. The hit area remains `MIN_TARGET` high;
text may wrap at spaces within the control's column. Default's long option label remains in the
menu/accessibility name but may resolve to a compact display value. The modulation mark is painted
without changing layout or popup identity. **Nothing uses this today.** Mono-00 had it for all fifteen matrix
inputs and both PWM sources, and mono-08 for its nine CV and four pulse destinations; both lost it
when their routing conversions replaced the picker with a list of the routes actually in force, so
a destination now draws one row per route rather than one row plus a menu. The form is kept because
it is still the §7.4 answer for a control whose source genuinely varies, not because a consumer
remains. **`selector_sublabel` has no consumer either** since mono-00's two PWM source menus became
routes (its modulation plan's Rev 3, 2026-09-22), and the consumer tests that covered both forms —
pointer selection, themes, hover geometry, resize, keyboard opening and cancellation, host updates,
reset, placement beneath the amount — went with the menus they drove. A consumer that takes either
form up again owes it that coverage.

**The menu stays inside the window, and a long one searches (§7.4).** Every caret form opens the
same list, `selector_menu`:

- **Height.** The rows sit in a vertical `ScrollArea` capped at the larger of the room above and
  below the button, less the popup frame and the search field. egui already tries below and then
  above against `Context::content_rect`, so that cap is what lets one side fit. The cap is the
  minimum scrolled height as well as the maximum, because a `ScrollArea` otherwise measures against
  the popup's size from the last frame and a menu that opened short could never grow.
- **Selection.** For the first two frames — egui's sizing pass and the first visible one — the
  selected row is scrolled to the middle, unanimated. Set a scroll target only *inside* that
  `ScrollArea`: one set after it ends is taken by the scroll area around the selector and scrolls
  the page.
- **Search.** Above `control::SEARCH_ABOVE` = **24** options — above the longest list any editor
  draws, mxm-creative-sampler's 17 Generate recipes — a search field heads the list, accessible as
  *<name> search*, and takes the keyboard when the menu opens. Typing filters by case-insensitive
  substring; `Enter` chooses the first match and closes (with nothing typed it only closes); `↓`
  hands the keyboard to the first row, where egui's focus travel walks the list; `Escape` closes as
  egui closes any menu. The query survives closing and reopening that selector so repeated catalogue
  assignments stay narrowed; its full-size, cross-shaped **Clear <name> search** action clears it,
  restores every row and keeps the menu open.
- **Identity.** The popup is `Popup::menu` on the button's response — `MenuButton::ui`'s own two
  steps, taken apart for the button's rect and id — so its id is unchanged.

A list that fits is laid out exactly as the plain menu was
(`a_short_selector_menu_is_laid_out_as_a_plain_menu`).
`a_long_selector_menu_opens_toward_the_room_and_stays_inside_a_small_window`,
`a_long_selector_menu_opens_on_its_selection_with_the_keyboard_in_its_search`,
`grouped_selector_headings_do_not_become_values_or_change_sparse_option_indices`,
`typing_narrows_a_long_menu_and_a_filtered_row_sets_its_own_index`,
`enter_and_escape_close_without_discarding_the_search_and_clear_restores_the_list` and
`a_short_selector_menu_has_no_search_field` hold the rest, headless.

`ParamView::labelled` may omit a repeated module prefix inside a named card/section (§7.1), never
rename a parameter according to its selected source. `name` remains canonical for tooltips and
accessibility. Knobs and sliders publish Slider roles and their full names, even when their visible
labels omit a prefix. Defaults use `label == name`, so other consumers need no migration.

## The knob grid

### A control beside a knob sits on the knob's grid

A knob's column is a fixed grid (next section but one): a name box `NAME_LINES` tall with the name on
its **bottom** line, `SPACE_2`, the circle. A segmented control is *label, spacing, cells*. Laid out
plainly beside a knob in a top-aligned row, it starts where the knob's box starts — and measured on a
1:1 render, at `Standard`, with the Body line at 14.9 px:

| | knob | switch beside it | off by |
|---|---|---|---|
| label top | 22.9 | 8.0 | 14.9 — one Body line |
| content centre, words | 65.9 | 43.0 | 22.9 |
| content centre, wave picker | 65.9 | 51.0 | 14.9 |

**A control beside a knob is therefore laid out on that knob’s grid**: `beside_a_knob` drops a separate
label onto the knob's name line—`(NAME_LINES - 1)` Body lines of space above it—and drops separate
cells onto the circle's centre—half of (knob diameter minus content height) below it. Both spacers
come from the knob's own geometry, so the next size tier or a change to `NAME_LINES` moves them with
it. A one-row selector is the deliberate exception to the second spacer: its name and selected value
are one sentence, so `selector_beside` aligns that whole row to the **bottom line of the knob's name
box**. This keeps the route baseline fixed beside both one- and two-line amount labels; lowering it
toward the circle made the text jump by label length. **The parent row must be top-aligned**
(`horizontal_top`): the knob's grid is built from the top down, and a row that centres its children
moves each by half its own height, which undoes this by a different amount for every pair.

`toggle_beside` is the same grid rule for a single on/off button. It carries no separate label line,
so it drops the complete button until its centre meets the knob circle. A caller still owns parameter
binding and the button's words. **`tree::switch_beside_knob`** is the one layout for a switch beside
a knob in a tree: on the circle's line, **`tree::switch_gap` from the knob's circle** — the knob
column's margin, 18 px, so with room the switch stands against the column — never inside the
column. The row it follows its knob in reads the column's drawn width, so the distance is the same
at a card's floor, where the column is narrower (the owner, 2026-09-25: *"too far from the knob it
controls ... half that distance"*; it was 34 px, the margin and two `SPACE_3`s). It must be the row child straight after its knob, or after the knob row whose last knob it
belongs to. Tempo sync's quarter note is its canonical consumer.

`segmented_beside` is the entry point for words — **a second one rather than a changed one**, as with
`AppBar::show_with`, because most segmented controls stand on a row of their own and passing `None`
to gain an alignment they do not use is ceremony; both funnel into `segmented_impl`. The wave picker
already took `beside`, so there it became an `Option`, and **`None` means a row of its own** and
adds no beside-a-knob lift.

`a_switch_beside_a_knob_sits_on_the_knobs_grid` renders the row and reads painted labels and content
at every tier for words and shapes; `a_switch_on_a_row_of_its_own_starts_at_its_label` guards the
`None` case. Arithmetic-only tests are insufficient because font metrics decide alignment.

### A knob is told how wide its column is, and must not ask

`ui.available_width()` is the column width **only inside `Ui::columns`**. In a plain row it is
everything left, so a knob that asked took the whole line and pushed the waveform selector clean off
the card. `knob` takes `column` as a parameter: `knob_row` passes the real column width, and a knob
sharing a row with something else passes its own diameter. Asking cost a working layout; being told
cannot.

### A knob's column is a fixed grid, name and value included

Three boxes, and only the middle one was ever fixed:

| | |
|---|---|
| name | `NAME_LINES` lines tall, centred, whatever the name is |
| knob | the tier's diameter |
| value | **one** line tall, centred, whatever the value says |

Name and value text must not size the column: longer readings or wrapped names would move controls
relative to their neighbors. `fixed_label` allocates from line height and line count, never from a
galley. A value is one line and **truncates** where it does not fit: extending kept the height
fixed but pushed `min_rect` out, so a card grew sideways as the knob turned. The ellipsis is a
backstop, not a layout — the column holds the widest reading whole (*Density has cited floors*), and
`tree_checks::card` fails on any painted text a card cut off. A three-line name overflows its box
rather than moving the row, which is the right way round: the layout holds and the name is visibly
too long.

**Not `Ui::put`.** It lays its widget out `centered_and_justified`, and a *justified* wrapped label
stretches every line but the last to fill the width — *Pulse width* came out as `P u l s e` over
`width`. Justification is for paragraphs. `fixed_label` builds its own `UiBuilder`, bottom-up for a
name so a one-line name sits against its knob instead of floating in a box sized for two.

Arithmetic cannot verify font-driven geometry. `column_height` measures a real knob through
`egui_kittest`, with the UI constrained to the knob’s actual column width; an unconstrained test
would hide wrapping.

## Text and typography

### Inter is bundled in three cuts

`assets/inter/` holds Regular, Medium and SemiBold with the OFL license beside them.
`FONT_IS_BUNDLED` is true. Three static cuts are used because egui has no variable-weight axis;
`family_for` maps a requested weight to a cut and rounds **down** between them so a scale adjustment
cannot quietly embolden the interface.

**`apply` names a cut only from the second frame on.** `Context::set_fonts` takes effect at the
next pass, but every consumer calls `apply` from inside a frame, and epaint's answer to an unbound
family is a panic rather than a fallback — it took out four of `mxm-preset`'s single-frame browser
tests. The first frame installs the cuts and leaves the styles on `Proportional`, which is bound and
is Inter Regular from then on anyway.

**The weights are written by a begin-pass hook, because `apply` is called once.** Every editor
calls it from `on_create`, never per frame, so the two-step dance that keeps `set_fonts` safe would
otherwise leave the whole collection on Regular for ever. `apply` installs the cuts, writes the
unweighted map, and registers a hook that writes the weighted one on the second pass — counting
passes rather than asking `Fonts`, which would mean taking that lock from inside a begin-pass hook.

**Card floors are computed from the fonts, not chosen.** Each card's tree states its narrowest in
Inter's metrics (*A card body as data*), so a changed font changes every floor with nothing to
re-measure — but check row packing, because a wider floor can make a view taller.

### Never break a word — and a preference is not a guarantee

§6 of the design system is normative: **text wraps at spaces or it does not wrap.** `Resonan` over
`ce`, `Key` over `tracki` over `ng`. A reader has to reassemble the word before they can read it,
which is the one thing a label must never ask.

**egui will not do this for you.** `break_anywhere` is false everywhere in this crate, and that only
makes egui *prefer* a word boundary: `RowBreakCandidates::get` falls back through dash, punctuation
and finally `any` when no word fits the line. So the guarantee has to come from width.
`fixed_label` takes `max(given width, longest_word(text))`, and
`a_label_never_breaks_a_word_however_narrow_its_box_is` holds it at 8, 24, 48 and 72 px — absurdly
narrow on purpose, because the rule has to hold at any width and not at the one the filter card
happens to give it today. Without the floor it fails immediately, splitting `Resonance` into nine
single letters.

**And never justify.** `Ui::put` lays its widget out `centered_and_justified`; a justified wrapped
label stretches every line but the last, so *Pulse width* came out as `P u l s e` over `width`.
Build the `UiBuilder` rather than reaching for the convenience.

## Accessibility names

### A painted control has no name until you give it one

`allocate_exact_size` returns a `Response` with no semantics. `on_hover_text` is a **tooltip**, not a
label — it reaches a pointer and nothing else. So every control here that paints its own content has
to call `widget_info` with the name, or it is invisible to a screen reader and unfindable by a test.
Both text and waveform segmented controls go through `segment_semantics`; a painted word or picture
is not an accessibility name. **Each cell hovers its own sentence**: every segmented function takes
`details: &[&str]`, one per option — `Label: Option`, then what that option does (the owner,
2026-09-27) — and `debug_details` asserts one non-empty sentence per option.
`each_cell_of_a_segmented_control_hovers_its_own_sentence` hovers each cell and finds only its own
sentence (falsified by a shared sentence and by none). Tests that are not about the text pass
`described(options)`.

### A control may paint less than it announces

**A control may paint less than it announces.** `segmented_named`, `segmented_waves_named` and
`selector_named` take a painted label and the parameter's own name separately, as `toggle_labelled` and
`ParamView::labelled` already did: the card paints *Mode* on a card titled *LPG 1*, and every cell
still announces *LPG 1 mode* (`a_named_segmented_control_announces_its_full_name_not_its_painted_one`).
`ParamView::quiet_label` paints a slider's name in the caption style and secondary ink, for a row
under a title that already says what it moves.

### A painted shell control is still a named control

`ViewBar` registers each painted tab as an accessible radio-style control whose selected state
follows the current view, so semantic order, current state and keyboard focus match the visible bar.

**It wraps rather than shrinks.** The bar was once kept to one row by sharing width down to the
pointer floor and falling back to two-letter initials; it packs into rows now and reports
`ViewBarGeometry { rows, height, overflow }` instead, so an over-wide word is *reported* rather than
squeezed below its floor and a caller decides what to do about it.
`four_view_navigation_at_the_narrowest_editor_keeps_every_view_reachable_and_named` holds the
requirement that drove the original — four views inside the logical width (the test's `WIDTH`) a
narrow host window leaves at 200 %, every one of them a full target with its name intact —
against whichever layout answers it, by asserting `overflow` is false rather than asserting a row
count.

## Waveforms

### A waveform is drawn, not spelled

`Wave` and `segmented_waves` are the shape vocabulary — triangle, square, a pulse, the two ramps,
sample and hold, and a sine (added for mxm-mono-00's LFOs, whose first shape it is) — as **polylines in a unit box**, painted in the ink colour the segment hands them.

**Why it is here and not in a plugin.** Every synth in the collection names the same handful of
shapes, and a square wave is the same picture in an LFO, an oscillator, and whatever MXM-303 needs.
It is part of the segmented control's rendering, which is already in the exempt set.

**Why a polyline and not a glyph or an image.** It scales to any cell, takes the theme's colour, and
cannot go missing the way a character outside the font does — a failure this repository has already
had once, in the shared-envelope label.

**A cell is `MIN_TARGET` tall and half again as wide** — the size a cycle wants to be drawn, not a
minimum to grow from. Words made every cell as wide as the longest of them, which for *Ramp down* is
most of a panel; that is what forced the shapes onto their own row, below the knob they belong to.
§11's pointer floor is the floor and the control does not go under it: smaller than a word is not
the same as small.

**The gaps are fixed too, at `SPACE_2`** — never the leftover width shared out, which is the same
stretch the owner ruled out for every button (2026-09-23, *"as small as possible"*).

**Six or more are balanced rows of no more than five** (`wave_columns`): six are two rows of three,
the owner's choice for the six-shape LFOs. The declared exception to §7.3's 2–5 rule is for pictures
only; the arrows walk the options in order and the grid is one control
(`six_waveforms_are_two_rows_of_three_and_one_control`). **`segmented_waves_unlabelled`** paints no label line, for a grid whose card already says what it
is and cannot spare the height (mxm-drum-machine's LFO rows: the line in each of three rows cost the
opening page its second row of cards); every cell keeps its full name
(`an_unlabelled_wave_grid_saves_its_label_line_and_keeps_its_names`). **`segmented_waves_marked`** paints a short
mark beside each picture, for options that differ in more than their shape (mxm-mono-01's sub
octave). **`toggle_wave`** is the on/off form, for shapes that combine: a wave cell with a toggle's
states and no mark, named for its parameter (`a_picture_switch_is_named_and_toggles`). The shared
vocabulary gained `Spike` (mxm-mono-08), `Noise`, `SmoothRandom` and `Trigger` (one narrow pulse
standing from a low rest — mxm-mono-08's per-step trigger switch, `Spike` upside down), and three
envelopes for mxm-classic-verb's Decay shape: `Decay` (an onset and an exponential fall), `Gated`
(a flat block cut dead at three quarters) and `Swell` (`u²` rising to the same cut), each the law
its DSP applies;
and for mxm-fx-convolution's tail laws, the envelopes they multiply a response by: `Level` (one
throughout), `Fade` (`(1 − x)²`) and `Rise` (`x²`), with `Gated` for its Gate. `QuarterNote`'s
button is square at a wave cell's height (`toggle_wave_size_of`, the tree's `Kind::SyncToggle`), the
note painted at a wave cell's proportions in its centre; every other picture keeps the wave cell.
`QuarterNote` is
*tempo sync*, on every synced control (`plans/plan-tempo-sync-controls.md`): a stem stroked like every picture and a notehead **filled**
(`Wave::fill`), the one picture with a filled part. A shape the list
lacks is added here, never drawn in a plugin.

**`beside` names the knob the row shares a line with, or `None` for a row of its own.** The row is
then laid out on that knob's grid — *A control beside a knob sits on the knob's grid*, above, has the
geometry and the measurement.

**The names do not go away.** They are the hover text and the accessible name. A picture on screen,
and a word for everything that cannot see it.

Two failures worth a test each, because neither shows up in a screenshot review: two shapes that draw
the same path, and a ramp without its reset — rising and falling ramps are one diagonal read in two
directions, and a still picture has no direction.

## The design system, theme and tokens

### A rule is piloted on one editor before the collection takes it

`pilot` holds the rules being tried on one editor first (the owner, 2026-09-24: *"You are testing
the rules on all the synths when we dont even know if the mono/08 is alright. It is too soon."*).
The rule is written once, in the shared code, and draws only where an editor has called
`pilot::enable` — mxm-mono-08 does, from its card dispatch — so every other editor draws exactly as
before. When the owner approves, roll a rule out by deleting its `pilot::on` check; do not copy a
piloted rule into a plugin. `pilot.rs`'s module comment lists what is piloted now: nothing, since
the keyboard language rolled out on 2026-10-08 (the first three rules did on 2026-09-24).

### Only the design-system foundation is exempt from evidence-before-sharing

Design system §13 exempts the shell, theme tokens, typography and basic parameter controls from the
usual wait-for-a-second-consumer rule, because they define the collection. **Anything beyond that
set still waits until two instruments need it.** A widget built for one consumer will have the wrong
API for the second.

### The design system is normative

`docs/MXM_DESIGN_SYSTEM.md` is not advice. The two rules most easily got wrong:

- **Explicitly non-hardware (§2).** No fake knobs, switches, LEDs, textures or embossed labels, and
  specifically **no layout copied from the panel of the instrument that inspired the DSP**. Controls
  are organised by task and signal flow.
- **Both themes are first-class.** Dark and light get equal design and test coverage, from semantic
  tokens. Contrast is *measured*, never eyeballed.
- **Source-panel controls remain directly visible (§3.3).** A disclosure is not a place to hide a
  control merely because it is adjusted less often.
- **`shell::vertical_level_meter` is the full-height visualization companion.** It owns a −60 to
  0 dBFS display scale and a distinct CLIP cell. Unreached level is a neutral track; vivid
  green/yellow/red appears only where the measured signal has reached. Signal lights use their own
  high-saturation palette rather than the muted semantic text colours; in particular, light-theme
  `warning` is brown for text contrast and is not meter yellow. Fixed zone position, threshold
  dividers and the peak cap repeat the reading without relying on red/green
  discrimination. A current clip is full danger colour while an older unacknowledged latch is
  deliberately subdued, so another event still has a visible onset. The consumer owns atomics,
  peak hold/release smoothing, channel labels, click acknowledgement and the latch itself. Nothing
  feeds back into DSP.
- **Bounded parameter selectors use the shared caret form (§7.4).** Bare dropdown values and local
  menu affordances are not collection variants.


Also mandatory: Inter Variable bundled under OFL, independent 75–200% editor zoom,
scroll-wheel editing off by default, tooltips and direct text entry on every parameter control.
Design-system §4.2 requires every plugin editor to fit a quarter-4K physical window; shared
geometry must support that budget without reducing the typography or pointer floors. Compact
controls are an existing per-consumer density choice, not a smaller collection-wide default.
`space::REFERENCE` is a starting size, not a mandatory frame. There is no shared `MINIMUM` token:
resize floors and their tests belong to each editor's measured cards (§4.3).

### The theme control is here, and so is the one file that remembers it

`shell::theme_control` is §3.1 slot 5 and §10 — Dark, Light and System, switching immediately —
and it lives here for the reason `AppBar` does: each product arranges the shared pieces itself.
Plugin editors and the player draw this widget in the same slot at the left end of the bar's
right-hand group. It is a `ComboBox` carrying the current choice
as its label because that is the shape the zoom control beside it already uses for *pick one of
these*.

**Storage is the exception to "this crate knows nothing about files".** `theme::store` writes
`<config>/mxm/editor.json`, one file for every MXM editor, because a person who chose dark chose it
for the collection and not for one instrument. The alternative was plugin state, and §10 forbids a
preset carrying the theme. `serde_json` and `dirs` came in for it — a serialiser and a path lookup,
neither of which assumes a window, an event loop or a plugin framework, which is what the rule at
the top of `Cargo.toml` is actually about.

Two callers, two stores, on purpose: an editor calls `shell::editor_theme_control`, which writes
that file; the player calls `shell::theme_control` and keeps its choice in its own settings, beside
everything else it remembers. `theme::preference` reads `MXM_EDITOR_THEME` first, then the file,
then Light — and the environment is never written back, so a screenshot run cannot change what
somebody chose.

### Consumers depend on the tokens, not the values

Never hard-code a colour, radius or spacing in a consumer. If a consumer needs a value this crate
does not expose, add the token here rather than the literal there. For the rare control that needs
a token `theme::style` cannot deliver through `Visuals`, `theme::tokens(ctx)` resolves the active
theme's set — it exists because egui's selected-button path swaps fill and text but never the
border, so a consumer wanting the full selected treatment must paint the accent border itself.

## Verification detail

### What the named tests hold, and what is manual

`a_toggle_is_never_narrower_than_its_label`,
`a_group_is_an_unfilled_hairline_panel_that_tightens_its_body`, and
`a_caret_selector_beside_a_knob_aligns_with_the_knobs_name_line` hold the easy-to-regress geometry
above.

The substantive check is the design system's **§15 QA gate** (structure, themes and accessibility,
interaction, audio safety and performance, originality), which is manual. Theme contrast is
unit-tested; instrument harnesses can additionally test actual theme/zoom operation and view-tab
semantics, but the whole visual gate is not automated.
