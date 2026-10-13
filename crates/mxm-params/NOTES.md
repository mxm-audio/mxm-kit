# NOTES.md — crates/mxm-params

The detail behind [AGENTS.md](AGENTS.md): the reasoning, measurements and history of the rules
that are still in force.

## `steps()` and `stepping()` in full

**`steps()` is a cell count, not a step size, and using it as one is a trap.**
`FloatParam::step_count` is unconditionally `None` in nice-plug, so a parameter declared
`with_step_size` reports itself continuous — a keyboard or controller step built on `steps()` walks
such a parameter straight off its own grid, silently. `stepping()` is the one to move a value with:
it returns the four normalised magnitudes one keyboard press moves: **1 % of the travel fine and
10 % coarse** (the owner, 2026-09-24), each snapped onto the parameter's own grid through
nice-plug's conversion — the declared step size, the enum and bool cell counts, the range's skew —
and floored at one of its steps (`next_step`/`previous_step`). The keyboard cursor in `crates/ui`'s `navigation` is its consumer,
and the arithmetic is here rather than there because only this crate may name nice-plug.

## The binding: its history and the laws in full

**Tempo sync's two helpers live here** (`plans/plan-tempo-sync-controls.md`): `sync_picture`, the
quarter note with the one `SYNC_DESCRIPTION`, and `synced_widest`, a syncable control's widest reading
over its free values and its span's division labels. They are why this crate depends on `mxm-tempo`.

**Every editor binds its parameters through `binding`** (2026-09-24, `plans/plan-editor-standard.md`
R1c; the owner: *"they should all look, feel and work the same"*). It was eighteen per-plugin copies of
`editor/binding.rs`: the effects' within ten lines of each other, the instruments' drifted by up to
five hundred. It lives here because this crate already names nice-plug and draws with `mxm-ui`, and
`crates/ui` may name neither. A plugin's `binding` module is `pub use mxm_params::binding::*;` and keeps
only what is its own (mxm-creative-sampler's `layer_label`).

- **`Bound::apply` is the one place a control's gesture becomes host automation**: begin before any
  set, end after the last, and a committed text entry as one complete gesture. Every stepped control
  (`segmented*`, `selector`, `toggle*`) is one instantaneous write, and `set_together` writes several
  parameters as one.
- **`Bound::panel` is the painted label** (design system §7.1): every drawer paints it, and the
  parameter's own name stays the accessible name and the tooltip's. `labelled` sets it,
  `unlabelled` clears it for a flat Parameters list, `painted` answers what is drawn. A stepped
  control takes one as `panel` in its `_named` form.
- **`Escape` cancels an open text entry** in every drawer — mxm-mono-08's, every editor's since.
- **`Bound::stepped` outranks the keyboard law**: a whole-note grid a drag lands on while the
  parameter holds anything between (the sampler's Root), and the finer layer (MICRO) does not go
  under it.
- Its tests hold the gesture bracketing, the stepped grid, the label split and the group write.

**`StepLaw` and `step_from` are the musical laws** (owner, 2026-09-23: a pitch steps ±1 semitone
and ±1 octave, not 10 % of its range). **Every law has the finer layer, MICRO** (`Press::finer`;
the owner, 2026-09-24: *"octave, semitone, cent is the range"*): `Own` 0.1 %, the pitch laws a cent,
`Cents` a tenth of a cent, `Voltage` a tenth of its fine step — and a finer press that rounds back
onto its start moves one of the parameter's own steps instead. With `coarse` set as well, which no
key asks for, each law gives the finer layer's coarse step: `Own` 1 %, the pitch laws and
`Voltage` ten cents, `Cents` a cent. `step_from(normalised, press, law)` answers where one press
lands from **any** value — not a magnitude at the current one, because a musical step depends on
where it starts and several presses in a frame each start where the last landed. The laws, each in
plain units and clamped to the range:

- **`Own`** — `stepping()`'s law, measured at the value the press starts from.
- **`Semitones`** — `r ± 1` and `r ± 12`, where `r` is `round_ties_even` of the plain value: the
  whole semitone the readout's `{:.0}` shows, so a continuous bend range at 2.37 or 2.5 moves to 3
  or 1 and the display always changes by exactly one semitone or one octave.
- **`Cents`** — exactly ±1 and ±10 cents, with no grid: the knobs read tenths.
- **`Hertz`** — ×/÷ 2^(1/12) and ×/÷ 2. A ratio can neither leave nor reach 0 Hz, so up from 0
  takes the parameter's own step, and a step down landing below the first own step above the
  minimum lands on it — `mxm-fx-convolution`'s Low cut reads Open there.
- **`Interval { octaves_per_unit }`** — a signed pitch depth read in octaves at finer than a
  semitone (`mxm-mono-08`'s pitch routes, owner 2026-09-23). A press goes to the **next** whole
  semitone (fine) or whole octave (coarse) in the direction pressed, so +0.30 oct steps to +0.33
  or +0.25 and to +1.00 or +0.00; from a whole interval it moves exactly one. `octaves_per_unit`
  is the plugin's reach, the number its reading multiplies by.
- **`Voltage { octaves_per_unit, fine }`** — a level that becomes pitch only through a route and
  reads as something else (`mxm-mono-08`'s stage levels, 0–100 %, one octave through a `+1.00 oct`
  route; owner 2026-09-23). Coarse goes to the **next** whole semitone in the direction pressed;
  fine moves exactly `fine` plain units with no grid. So a fine press detunes off a semitone and
  the next coarse press lands back on one: 2 % and then coarse up is one semitone, not one and 2 %.

**← → snap, every law** (newDAWn's owner, 2026-10-09: *"sdf value left right should change in
musical steps"*). ↑ ↓ move by the size as above; ← → (`Press::snap`) go **to** the next line of the
size, on a lattice fixed in the value's own units, so two presses from anywhere land on a round
value: `Own` the 10 %, 1 % and 0.1 % lines of the travel from its start; `Semitones` 12, 1 and
0.01 semitones from 0; `Cents` 10, 1 and 0.1 cents from 0, where ↑ ↓ keep no grid; `Hertz` the
octave, semitone and cent lines of equal temperament at A440, in log2, where ↑ ↓ are ratios, with 0
Hz and the minimum as its step treats them; `Interval` its own lattice, which already snaps, so
← → = ↑ ↓; `Voltage` the whole semitones coarse, and multiples of `fine` and of a tenth of it from
0, where ↑ ↓ fine keep no grid. A line the parameter's own grid can't show moves one of its own
steps, as any press does. `a_snap_goes_to_the_next_line_of_each_laws_lattice` holds it.

**Declared by the owner, never guessed** — each plugin's binding carries a `law` its sections set
per parameter. A unit string cannot tell a cutoff from an LFO rate, and `" st"` also marks the
continuous tunes the owner excluded. The plain⇄normalised conversion is nice-plug's own
`ParamPtr::preview_plain`/`preview_normalized`, which the wrappers use for every parameter type;
it is this crate's only `unsafe`, sound because `as_ptr` borrows the parameter for the call and the
pointer dies with it. `normalised_of` clamps a plain value into the range first, because a skewed
range raises its offset from the minimum to a power and a value below it would come back NaN. The
tests in `erased.rs` pin each law against real `FloatParam`/`IntParam` ranges.

## The value text in full

The plugins carried 80 named formatter and parser functions with 47 distinct bodies when `text`
was made (the architecture audit's O9). The groups identical apart from variable names moved:
percent and its parser (fourteen and fifteen plugins), the millisecond-to-second time and the time
parser (eight each), and four identical pairs. A plugin-wide census — every parameter's text at
every point the shared walk visits, and what typed inputs parse to — was taken before and after the
move, and the two were identical.

Near-copies whose arithmetic differs stay in their plugins until an exhaustive comparison over
each parameter's range proves them equal: two tenth-of-a-millisecond times compute their unit
switch as `s × 10000` and `s × 1000 × 10`, different f32 roundings, and three frequency parsers
accept different suffixes.

D6, the defect a raw-value unit switch causes, was fixed in twelve plugins one copy at a time
before `text` existed. The shared walk that now holds every plugin found three more on joining:
fx-convolution's High cut at its 1 kHz and Open edges, and the drum plugins' `-0` through
nice-plug's `v2s_f32_percentage(0)`, which `whole_percentage` replaces.
