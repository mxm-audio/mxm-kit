# AGENTS.md — crates/mxm-params

Parent: [`../../AGENTS.md`](../../AGENTS.md). The detail behind the rules: [NOTES.md](NOTES.md).

# Purpose

What a parameter is to the collection's controls and presets, how an editor binds one, and how it
reads: `ErasedParam` and the keyboard's step laws (`erased`), the collection's one parameter binding
(`binding`) and its value text (`text`). Every instrument and effect uses all three, through its own
`binding` module, its `preset.rs` and its `params.rs`.

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own a plugin's parameters, ranges or descriptions, nor the
drawing of a control, which is `crates/ui`'s. `mxm-preset` depends on it for `ErasedParam`;
`mxm-modulation-params` depends on it for `ErasedParam`, `StepLaw` and `signed`, and on nothing of the
preset system.

**One path.** Neither shared crate re-exports anything here; each plugin's `binding` module is
`pub use mxm_params::binding::*;`, the plugin's local facade.

# Local Contracts

## `ErasedParam` is a parameter as a control and a preset see it

The slice of a nice-plug parameter a control or preset needs, as a trait object. Plugin bindings
re-export it, and `mxm-preset`'s `Instrument::parameters` hands it out. It cannot live in `crates/ui`, whose contract excludes plugin frameworks.

**`steps()` is a cell count, not a step size, and using it as one is a trap**: a `with_step_size`
parameter reports itself continuous. Move a value with **`stepping()`**: 1 % fine and 10 % coarse,
snapped onto the parameter's own grid and floored at one of its steps
([NOTES.md § `steps()` and `stepping()`](NOTES.md#steps-and-stepping-in-full)).

## The collection's one binding — `binding`

**Every editor binds its parameters through `binding`**: a plugin's `binding` module is
`pub use mxm_params::binding::*;` and keeps only what is its own (mxm-creative-sampler's
`layer_label`). Tempo sync's `sync_picture` and `synced_widest` live here too, which is why this crate
depends on `mxm-tempo` ([NOTES.md § The binding](NOTES.md#the-binding-its-history-and-the-laws-in-full)).

- **`Bound::apply` is the one place a control's gesture becomes host automation**: begin before any
  set, end after the last; a committed text entry is one gesture, a stepped control one
  instantaneous write, and `set_together` several parameters as one.
- **`Bound::panel` is the painted label** (§7.1); the parameter's own name stays the accessible and
  tooltip name (`labelled`, `unlabelled`, `painted`, and a stepped control's `_named` form).
- **`Escape` cancels an open text entry** in every drawer. **`Bound::stepped` outranks the keyboard
  law**, and the finer layer (MICRO) does not go under it. Its tests hold all four.

**`StepLaw` and `step_from` are the musical laws.** `step_from(normalised, press, law)` answers where
one press lands from **any** value; **every law has the finer layer** (MICRO, `Press::finer`), and
gives a coarser finer step when `coarse` is set as well, which no key asks for. The laws,
in plain units and clamped to the range: `Own` (`stepping()` at the start value), `Semitones` (±1 and
±12 from the whole semitone shown), `Cents` (exactly ±1 and ±10, no grid), `Hertz` (×/÷ 2^(1/12) and
×/÷ 2), `Interval { octaves_per_unit }` (to the **next** whole semitone or octave) and
`Voltage { octaves_per_unit, fine }` (coarse to the next whole semitone, fine by exactly `fine`).
**← → snap** (`Press::snap`, newDAWn's owner, 2026-10-09): every law goes to the next line of the
size on a fixed lattice, `snap_from` — `Own` 10 %, 1 % and 0.1 % of the travel; `Semitones` 12, 1
and 0.01 semitones from 0; `Cents` 10, 1 and 0.1 cents; `Hertz` the octave, semitone and cent lines
of A440; `Interval` its own lattice, already snapping; `Voltage` whole semitones coarse and
multiples of `fine` and a tenth of it. Never less than one of the parameter's own steps.
**Declared by the owner per parameter, never guessed.** The plain⇄normalised conversion is nice-plug's
`ParamPtr::preview_plain`/`preview_normalized`, this crate's only `unsafe`; `normalised_of` clamps
first. `erased.rs`'s tests pin each law. The laws in full:
[NOTES.md § The binding](NOTES.md#the-binding-its-history-and-the-laws-in-full).

## `text` holds readings the collection already prints

Never a new policy: each function is one the plugins already printed, moved or fixed, and a plugin
whose reading is its own keeps it ([NOTES.md § The value text in full](NOTES.md#the-value-text-in-full)).

- **A group moves only when its copies are identical**, proved by a before-and-after census of
  every plugin's text.
- **No reading prints a negative zero** (`without_negative_zero`, and `signed` for a reading that
  carries its sign): `-0` parses to zero and reads back `0`, and `clap-validator` fails in the sliver.
- **A unit switch is chosen from the rounded reading**, never the raw value (D6).
- `whole_percentage` is nice-plug's `v2s_f32_percentage(0)` without the negative zero.
- `ValueToString` and `StringToValue` are the formatter types; a plugin imports them.
- Every plugin's text is held to the host's round trip by `mxm_plugin_test::text_checks`.

# Work Guidance

- A change here reaches every instrument and effect: run every plugin's tests after this crate's.
- MSRV 1.95: the binding draws with egui. No DSP crate needs anything here.

# Verification

```bash
cargo test -p mxm-params
cargo clippy -p mxm-params --all-targets -- -D warnings
```

# Child DOX Index

None.
