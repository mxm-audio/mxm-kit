# AGENTS.md — crates/ui

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

`mxm-ui` — the shared interface foundation for the whole collection. Implements
[`docs/MXM_DESIGN_SYSTEM.md`](../../docs/MXM_DESIGN_SYSTEM.md): semantic theme tokens with
Dark/Light resolution, bundled typography, the app-bar / view-bar / module-card shell, and the basic
parameter controls. Modules: `theme`, `typography`, `space`, `visual`, `control`, `shell`,
`browser`, `tree` (a card body as data), `flow`, `paging`, `navigation` (the keyboard cursor in
every collection editor) and `reach` (the cursor over every widget, newDAWn's so far) — [NOTES.md § The modules](NOTES.md#the-modules). Each rule below has its
reasoning, history, measurements and holding tests in [NOTES.md](NOTES.md), under the same topic.

# Ownership

Owns `src/`, `Cargo.toml`, and every visual decision the design system delegates to code: geometry,
visual state, focus treatment, interaction conventions, theme token resolution, typography.
Does **not** own labels, parameter bindings, or data. Plugins and the player supply those.

# Local Contracts

## Dependencies and threads — [NOTES.md § Dependencies and threads](NOTES.md#dependencies-and-threads)

- **`egui` and nothing else** but `mxm-keys` (no dependencies): no windowing crate, host crate or plugin framework; nothing may
  assume it owns a window, an event loop or a swapchain. `lib.rs`'s `the_crate_is_windowing_free`
  reads the manifest — check what a new dependency pulls in before adding it. `egui_taffy` (pure
  flexbox rectangles) and `serde_json`/`dirs` (the theme file) fit the rule: none assumes a window.
- **Blocking native work leaves the frame** (`offthread`): a frame inside a plugin window never
  waits on a modal OS loop. `offthread::start` runs the caller's closure on a named thread,
  `running` disables the trigger, `take` hands the result once to an ordinary frame.
- **A named text style must never panic**: use `typography::value_style`, `label_style` and
  `caption_style`, which fall back; a panic inside a paint call takes the host down with it.

## Controls — [NOTES.md § Controls and their outcomes](NOTES.md#controls-and-their-outcomes)

- **A control takes a normalised `0..=1` value and a borrowed description and returns a
  `ControlOutcome`**: it reports gestures, never reaches for a host, plugin or parameter; units,
  ranges and skew stay with the parameter's owner. A momentary action reports activation only — the
  plugin shell owns the command path (§13); shared UI never calls DSP.
- `ControlOutcome::reset` is not inferred from equality. `ParamView::marked` = another owner sets it
  too; `ParamView::modulation` = the delta; a segmented control rings its `sounding` cell, a click
  on the selected cell still fires, double-click resets to `default_cell`, and a selection one past
  the last cell is *nothing selected*.
- **A knob is relational, a slider positional; `Shift` refines both.** A knob's drag accumulates
  against `DragAnchor`, never against readback (CLAP delivery is asynchronous).
- **Nothing changes size under the pointer**: geometry comes from the `Size` tier before hover,
  focus or drag is read; interaction changes only color and stroke width. **Reserve the space,
  paint the state** — `mark_slot` always reserves `SPACE_2`.
- **The remove mark is the cross alone** (`control::remove_mark`): unframed, two painted strokes
  (never a `✕`/`×` glyph), the whole square clickable, allocated even when `shown: false`. Where it
  clears a parameter it is a parameter control (cursor, focus ring, `marked`) whose `label` is what
  it removes (*"Remove LFO from Cutoff"*).

## Widths and sizes — [NOTES.md § Widths and sizes](NOTES.md#widths-and-sizes)

- **A segmented control** has 2–6 options. Every cell is `segment_cell_width` (widest option plus
  `SPACE_3` each side) and no wider; one that cannot fit overflows its card. Controls together share
  a cell (`shared_cell_width` + `segmented_with_cell`, or `segmented_stack`).
- **A toggle** is its painted label plus `SPACE_3` either side and no wider, with no ●/○ mark, and
  its width never depends on `marked`. Share widths with `shared_toggle_width`/`toggle_stack`.
  `toggle_labelled` paints one string and names another; `toggle_compact` is for dense rows only.
- **Pointer floor 24 × 24** (WCAG 2.2 SC 2.5.8); controls draw at `MIN_TARGET`; density never
  shrinks the type scale. A mixed-height row sets `interact_size.y` once and settles **upward**.
- **One knob diameter.** Every knob stands in `control::knob_column`, a row of knobs is
  `tree::knob_row`, and **a knob that shows its value holds its widest reading whole** (`knob_size`,
  `widest`). Consumers use these rather than copying numbers. An envelope is a `tree::fader_row`.

## The app bar and the preset browser — [NOTES.md § The app bar](NOTES.md#the-app-bar), [§ The preset browser](NOTES.md#the-preset-browser)

- **"Save preset" always asks for a name**; the name decides overwrite or new. `AppBar::show`
  delegates to `show_with` (§3.1 slots 2–4). The right-hand group is laid out first, the preset
  name is fixed-width, every bar control is one height (`action_size`).
- **The bar compacts itself** through `shell::BarStep`, measured last frame, counting the gap
  between groups as `SPACE_5` **plus** item spacing; only a bar that drew the `…` menu compacts. At
  the last step `shell::product_actions` move into the `…` menu; a product that registered them as
  a bar card calls `navigation::bar_card_absent`. Every editor's minimum window holds that step.
- `slider_inline` shares the stacked slider's whole interaction and reserves `ParamView::widest`
  so its track never moves under a drag. **A stacked slider's value never draws over its name.**
  `shell::scroll_list` keeps content clear of its scroll bar.
- `preset_browser` knows nothing about presets: an unchoosable row is shown disabled with its
  reason and skipped by `step_preset`. `PresetState::save_disabled` gates Save and Save As only.
- `browser::panes`: rows in, `BrowserAction` out; every painted row gets `widget_info`;
  `browser::overlay_rect` keeps the bar in sight. **The keyboard lives in the search box**
  (`set_focus_lock_filter`): left/right between panes, up/down within one, Enter done, Escape
  closes — keys read and consumed before the box is drawn.

## Cards, flow and the layout tree — [NOTES.md § Cards, groups and levelling](NOTES.md#cards-groups-and-levelling), [§ Flow and the layout tree](NOTES.md#flow-and-the-layout-tree)

- `shell::group`: hairline, `RADIUS`, `SPACE_3` padding, **no fill**, body spacing `SPACE_2`; the
  caller owns the space above it. A card body's rhythm is `SPACE_3`. The §4.2 radius conflict (8 vs
  4) is recorded, not resolved. A knob is centred in its full-width column.
- **Columns of cards end on one line** (§3.3, `shell::level_columns`/`Level`): the deficit is shared
  equally inside a column's cards with `shell::grow` — **never `Ui::add_space`** — and the natural
  height is remembered, never the padded one.
- **`flow::cards` is the single scroll owner on both axes** — never wrap it in a second
  `ScrollArea`; give it a ceiling. `flow::Options`' `quantum` and `cull` stay **off by default**
  unless a measurement says otherwise.
- **A card body as data — `tree`**: one `tree::Node` is both measured and drawn; every plugin card
  is one. A leaf's size function sits beside its control (*Sizes without drawing*) and `tree::tests`
  compares the two — change both together. A card is sized to `reserved_height` and draws the
  natural height at its top. Measure in the body's `Ui` (`shell::body_ui`) after fonts settle; salt
  a leaf's `Ui` with its key. Paint may reach into a gap, never over another leaf or off the card.

## Paging — [NOTES.md § Paging](NOTES.md#paging)

- Order is Performance → Modulators → Sequencers → Generators → Tone → Effects, then authored order;
  `Key`s are the caller's. `Category::from_request` maps developer CC 0–5; Parameters is 127 and
  never merges or appears in musician navigation.
- Groups are disjoint, contiguous and same-category; invalid metadata is an error, never dropped,
  duplicated or reordered. A whole category never costs a page; a page that must scroll anyway
  takes a unit that adds no row. `Scroll::None` only for a proved-fitting page.
- `paging::editor::show` sizes cards from their trees; nothing is drawn to learn a size. Snapshot
  destructive telemetry before `show`. Exhausted navigation and non-convergence are errors, not
  loops; `State` defers replacement while interaction is owned (`hold`).
- **Reserve a row's body floor before drawing** — egui 0.36's `set_min_height` is cursor-relative.
  `ViewBar` wraps rather than shrinks and reports `ViewBarGeometry`.

## The keyboard cursor — [NOTES.md § The keyboard cursor](NOTES.md#the-keyboard-cursor), [§ The keyboard language](NOTES.md#the-keyboard-language), [§ Reach](NOTES.md#reach)

- **`reach` walks every widget egui describes to screen readers** (the owner, 2026-10-07; the editors
  convert to it, keys first in newDAWn and the collection): nothing registers; a painted control
  names itself or stays out; a value in its own units reads `reach::edit`; menus `reach::context_menu`.
- **The cursor reads the keyboard language** (`navigation/language.rs`, design system §11; piloted on
  mxm-mono-08, rolled out 2026-10-08): arrows parameter to parameter, COARSE + arrows card to card,
  VALUE + arrows the value (fine, COARSE coarse, MICRO the finer layer), one gesture OUT keeps and
  BACK cancels, DELETE the default, VIEW + arrows the bars. BACK alone never reveals the cursor
  (`escape_is_not_a_reveal`). **BACK during a mouse drag cancels it** (the owner, 2026-10-08, as
  newDAWn): `drag` marks it before the keys are read, and a knob or slider whose drag stops
  cancelled puts back the value it began at (`back_during_a_drag_puts_the_knob_back`); an editor's
  own dragged widget asks `drag::cancelled` when its drag stops.
- **The map is drawn, not declared**: `navigation::mark` inside the `card` and `at` scopes;
  `navigation::aside` for an editor-only control. Editors drive it with `navigation::paged` (never
  a second card order), or `paged_with_bar` + `navigation::bar_card` for an app-bar parameter.
- The cursor target is the keyboard authority, not egui focus. The pointer moves it
  (`Spot::pointed`, before `run`'s `inert` return); `remove_mark` never claims; unhanded focus is
  owed. A text field or open popup makes it inert — read `egui::Popup::is_any_open`, **not**
  `Context::any_popup_open`.
- A press's size is the parameter's (`control::Steps`, `ParamView::stepping_by`). Presses apply one
  at a time, in order; an edit is one gesture, each press chaining from the value the last sent.
- **`consume_key` ignores an extra `Shift` or `Alt`**: test the most specific modifier first.
- `navigation::stop` on a cardless surface. The cursor is painted, never laid out; concealed on a
  pointer press, revealed by the language's keys; the gate lives in `outline` and
  `control::focus_ring` (only where a cursor runs), never at call sites.

## Selectors and menus — [NOTES.md § Selectors and menus](NOTES.md#selectors-and-menus)

- **The caret form `‹ Off ›` is canonical for every bounded parameter enum** (§7.4): one
  `MIN_TARGET` row, accessible as *name: value*, with a stepped control's marks and reset; key a
  routing scope by its permanent target. `selector_grouped` headings never become cells.
  `selector_sublabel` and `slider_with_source` have no consumer; one that returns owes the coverage.
- **`selector_menu` stays inside the window**: capped `ScrollArea`, scroll target set only inside
  it, search above `control::SEARCH_ABOVE` options, popup id kept.
- `ParamView::labelled` may drop a repeated prefix (§7.1), never rename by selected source.

## The knob grid — [NOTES.md § The knob grid](NOTES.md#the-knob-grid)

- A knob's column is a fixed grid (`NAME_LINES` name box, `SPACE_2`, circle, one truncating value
  line); text never sizes it. **Not `Ui::put`**: build the `UiBuilder`. A knob is told its `column`
  and never asks `ui.available_width()`.
- A control beside a knob sits on the knob's grid (`beside_a_knob`, `selector_beside`,
  `toggle_beside`, `tree::switch_beside_knob` as the child straight after its knob); the row is
  `horizontal_top`. Prove it by rendering: font metrics decide alignment.

## Text, names and pictures — [NOTES.md § Text and typography](NOTES.md#text-and-typography), [§ Accessibility names](NOTES.md#accessibility-names), [§ Waveforms](NOTES.md#waveforms)

- Inter in three cuts, rounded **down**; `apply` names a cut only from the second frame, weights by
  a begin-pass hook. **Never break a word** (§6): `fixed_label` floors at `longest_word`.
- **A painted control has no name until `widget_info` gives it one**; each segmented cell hovers its
  own sentence (`details`). A control may paint less than it announces.
- **A waveform is drawn, not spelled** (`Wave`); a missing shape is added here, not in a plugin.

## The design system, theme and tokens — [NOTES.md § The design system, theme and tokens](NOTES.md#the-design-system-theme-and-tokens)

- `docs/MXM_DESIGN_SYSTEM.md` is normative: non-hardware (§2), both themes with measured contrast,
  source-panel controls visible (§3.3), 75–200 % zoom, no scroll-wheel editing by default, tooltips
  and text entry on every parameter control, a quarter-4K window (§4.2), no shared `MINIMUM` token.
- `shell::vertical_level_meter` never relies on red/green alone; nothing feeds back into DSP.
- Only the §13 foundation skips evidence-before-sharing. **A new rule is piloted** behind
  `pilot::on`; roll it out by deleting the check, never by copying it into a plugin.
- Editors call `shell::editor_theme_control` (`<config>/mxm/editor.json`), the player
  `shell::theme_control`; `MXM_EDITOR_THEME` overrides and is never written back.
- **Consumers depend on the tokens, not the values**: add a token here, never a literal there.

# Work Guidance

- Read the design system before writing any widget, and check its **§15 QA gate** before calling one done.
- MSRV is 1.95 (egui). Anything depending on this crate inherits that floor.
- egui is pinned at `=0.36.1`. Consumers must not pull a different egui version into the graph.

# Verification

```bash
cargo test -p mxm-ui
cargo clippy -p mxm-ui --all-targets
cargo test -p mxm-ui --release --test flow_resize_bench -- --ignored --nocapture   # a measurement
```

`a_toggle_is_never_narrower_than_its_label`, `a_group_is_an_unfilled_hairline_panel_that_tightens_its_body`
and `a_caret_selector_beside_a_knob_aligns_with_the_knobs_name_line` hold the easy-to-regress
geometry. The substantive check is the design system's **§15 QA gate**, which is manual; headless
tests prove behaviour, not native feel.

# Child DOX Index

No child `AGENTS.md` files.
