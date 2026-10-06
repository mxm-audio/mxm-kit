# AGENTS.md — crates/mxm-preset

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

`mxm-preset` is the collection’s preset system: file format and category, on-disk library,
favourites, three-state loaded identity and app-bar controls. It serves every instrument and each
effect that qualifies under the parent preset rule.

The rules — what a preset is, how it is applied, where it lives, what a favourite is — are stated in
[`plugins/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/AGENTS.md), *A preset is parameter values*, and implemented here.
This doc holds what is specific to the crate: its one trait, what stays with a plugin, and the
crate-specific contracts. The full text behind each rule is in [NOTES.md](NOTES.md).

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own any instrument's id, parameters or sounds — those
arrive through [`Instrument`](#an-instrument-is-four-core-answers-with-instance-setting-sparse-default-factory-preserved-and-durable-content-seams) — and does not own the drawing of the
browser: `crates/ui`'s `preset_browser` paints the rows and reports what a person did; this crate
knows what the rows are and what the actions mean.

# Local Contracts

## An instrument is four core answers, with instance-setting, sparse-default, factory-preserved and durable-content seams

`Instrument` is implemented on a plugin's `Params` struct, in its `src/preset.rs`, and asks for:
the permanent `clap_id`; `parameters()` as `(id, &dyn ErasedParam)` **in declaration order**, the
order files are written and Init writes in; `identity()`, the `#[persist = "preset"]`ed
`RwLock<PresetIdentity>` that keeps the loaded preset's name and baseline with the patch; and
`factory_files()`, the compiled-in `(name, json)` set. Everything takes `&dyn Instrument`, and a
`&MxmXxxParams` coerces, so a plugin's editor passes its params and nothing else.

The seams ([NOTES.md § The instrument's seams in full](NOTES.md#the-instruments-seams-in-full)):

- **A switch added after states were saved**: the plugin's `Plugin::filter_state` calls
  `add_switches_off(state, ids)` — first, if it may clear a malformed state — restoring each missing
  switch Off and extending a loaded preset's baseline with it at normalised zero.
- **Factory presets found at run time** (`Found`): the plugin scans and hands the list to
  `Library::with_found`; they list after the compiled set as `Origin::Factory`, each `Entry` carrying
  its folder as `group`. **The library never looks anywhere itself.** An effect is an instrument
  here: four parameters or eighty, the crate does not know the difference.
- **Factory-preserved patch parameters** (`is_factory_preserved`, `mxm-fx-delay`'s Mix): consulted
  only on trusted `Origin::Factory` provenance from the compiled-in entry, never a JSON claim;
  independent of `is_source_owned`.
- **Synchronous durable content** (`mxm-creative-sampler`, `mxm-classic-verb`, `mxm-fx-curve`): the
  instrument supplies captured versioned JSON content, a cheap deterministic fingerprint,
  whole-payload preflight and application, the commit, and `is_source_owned`. User saves carry the
  payload in the format's reserved `state` field; **`state = null` is a factory recipe that preserves
  current content**. `init_preset_state` (default empty) makes Init restore a default model. The
  default methods reject an unexpected `state`; parameter-only plugins serialize exactly as before.
- **Deferred** (`DeferredPresetTransaction`, `mxm-fx-convolution`): a separate, strictly opt-in API
  (`preset_row_deferred`, `overlays_deferred`); Save and Save As are disabled while Pending. Existing
  `preset_row`, `overlays`, `apply_preset_checked` and `init_patch` never dispatch through it and stay
  synchronous.

The rules, all the crate's, not a plugin's ([NOTES.md § The six rules in full](NOTES.md#the-six-rules-in-full)):

- **Factory preservation requires trusted provenance.** Only an entry compiled as `Origin::Factory`
  may omit an id named by `is_factory_preserved`; everything else reports the omission. Captures and
  Init stay complete except for explicit sparse defaults.
- **Prepared before the gestures, committed after them.** `apply_preset_state` runs before any
  parameter write, so a rejected payload costs no gesture and is not marked loaded;
  `commit_preset_state` runs once after the whole burst, even for a recipe with no content. Generated
  Init follows the same order when `init_preset_state` is supplied.
- **A preset writes source-owned parameters only when it brings the source they describe.**
  `Preset::resolve` skips them whenever `state` is null; Init skips them too.
- **Instance settings are host state, never preset sound.** `is_instance_setting(id)` parameters are
  excluded from capture/save, apply, Init, missing-parameter reports, the loaded identity baseline and
  dirty comparison — deferred included. No preset writes an instance setting.
- **An omitted parameter preserves the current value unless the instrument names one explicit
  default migration** (`default_missing_legacy_parameter`). `omit_default_parameter` and
  `omit_captured_parameter` require the same id in that bridge. Unknown ids remain errors.
- **Deferred means no early visibility, and acknowledgement is not a new snapshot.** Pending,
  rejected, cancelled and superseded work emits no gestures and changes no identity. Ready emits only
  targets whose unmodulated base bits — and `parameter_edit_revision`, when present — are unchanged
  since begin. `acknowledge` installs the baseline and fingerprint retained at begin and publication,
  never a recapture. Init clears identity; requests and cancellation are monotonic; a published
  predecessor survives a cancelled or rejected successor; editor close cancels unpublished work by id.
- **Synchronous gestures are not synchronous parameter application.** A host may apply queued
  gestures later, so a preset load builds its identity baseline **before emission** from the
  canonical intended targets, never from live atomics read after it. Saves still snapshot the live
  patch.

## The tests here are the rules; a plugin's are its sounds

Shared tests prove each rule once, against `TestInstrument` (`lib.rs`’s `testing` module). A
plugin's `preset.rs` tests only **its** sounds: factory designs and regeneration, shipped files, every
factory preset resolving every parameter and audible, Init first, the user root under its own id.
**A test that would pass unchanged in every plugin belongs here**
([NOTES.md § The shared tests](NOTES.md#the-shared-tests-in-full)).

## A sound is saved with its category

- `Category` is one word from a **fixed, collection-wide list** (`Category::ALL`, *Uncategorised*
  last), **on the preset, not on a bank**. Save As asks for name and category; Rename and Save keep it.
- A missing or unknown word (`#[serde(other)]`) reads as *Uncategorised*; **the schema stays at 1** —
  a version moves only when an older build would read something *wrong*.
- **Every factory set carries a category per sound** (`every_factory_preset_has_a_category` or an
  equivalent). **A found preset is filed under its folder** (`group`) instead
  ([NOTES.md § Categories](NOTES.md#categories-in-full)).

## A bank is a directory here and one file on the wire

- `Origin::Bank(name)` lives in `presets/<dir_name(name)>/` with a `bank.json`; *My presets*
  (`Origin::User`) is always first in `Library::banks()`. Save, Rename, Delete and *exists* take the
  origin; **a factory preset refuses every one**. The directory's name wins over the file's.
- **On the wire a bank is one file**, `<name>.mxmbank.json`, in `banks/` beside `presets/`.
  `import_bank` refuses a wrong file whole, and an existing name with `ImportRefused::Exists` unless
  told to replace — **a person is asked, never silently overwritten**.
- **A folder, not a required dialog** (`open_folder`, with an arm for all three platforms).
  `Library::at(root)` has a banks folder only when the root is called `presets`.
- **Favourites are keyed by origin and name** (`favourite_key`)
  ([NOTES.md § Banks](NOTES.md#banks-in-full)).

## The browser knows the banks; the panes do not

- `ui::overlays` draws the browser (`mxm-ui`'s `browser::panes` in an `egui::Area`) and the naming
  row. **`PresetUi::visible` is the one list** the browser and the bar's arrows step through.
- *Export…* packs the presets shown **except those found in a folder** (`Entry::found`). *Save As*
  goes to the loaded preset's bank, else the bank being looked at, else the user's own.
- The name in the bar and CC 117 (`PresetUi::set_browser_open`) open it; a click on the bar does not
  close it. **Left and right move between the panes, up and down within one, Enter is done**
  ([NOTES.md § The browser](NOTES.md#the-browser-in-full)).

## `ErasedParam` lives here

The slice of a nice-plug parameter a control or preset needs, as a trait object. Plugin bindings
re-export it. It cannot live in `crates/ui`, whose contract excludes plugin frameworks.

**`steps()` is a cell count, not a step size, and using it as one is a trap**: a `with_step_size`
parameter reports itself continuous. Move a value with **`stepping()`**: 1 % fine and 10 % coarse,
snapped onto the parameter's own grid and floored at one of its steps
([NOTES.md § `steps()` and `stepping()`](NOTES.md#steps-and-stepping-in-full)).

## The collection's one binding — `binding`

**Every editor binds its parameters through `binding`**: a plugin's `binding` module is
`pub use mxm_preset::binding::*;` and keeps only what is its own (mxm-creative-sampler's
`layer_label`). Tempo sync's `sync_picture` and `synced_widest` live here too, which is why this crate
depends on `mxm-tempo` ([NOTES.md § The binding](NOTES.md#the-binding-its-history-and-the-laws-in-full)).

- **`Bound::apply` is the one place a control's gesture becomes host automation**: begin before any
  set, end after the last; a committed text entry is one gesture, a stepped control one
  instantaneous write, and `set_together` several parameters as one.
- **`Bound::panel` is the painted label** (§7.1); the parameter's own name stays the accessible and
  tooltip name (`labelled`, `unlabelled`, `painted`, and a stepped control's `_named` form).
- **`Escape` cancels an open text entry** in every drawer. **`Bound::stepped` outranks the keyboard
  law**, and one `Alt` does not go under. Its tests hold all four.

**`StepLaw` and `step_from` are the musical laws.** `step_from(normalised, press, law)` answers where
one press lands from **any** value; **every law has an `Alt` layer** (`Press::finer`). The laws,
in plain units and clamped to the range: `Own` (`stepping()` at the start value), `Semitones` (±1 and
±12 from the whole semitone shown), `Cents` (exactly ±1 and ±10, no grid), `Hertz` (×/÷ 2^(1/12) and
×/÷ 2), `Interval { octaves_per_unit }` (to the **next** whole semitone or octave) and
`Voltage { octaves_per_unit, fine }` (coarse to the next whole semitone, fine by exactly `fine`).
**Declared by the owner per parameter, never guessed.** The plain⇄normalised conversion is nice-plug's
`ParamPtr::preview_plain`/`preview_normalized`, this crate's only `unsafe`; `normalised_of` clamps
first. `erased.rs`'s tests pin each law. The laws in full:
[NOTES.md § The binding](NOTES.md#the-binding-its-history-and-the-laws-in-full).

## The controls know about files; the widget does not

`ui::PresetUi`'s listing is rebuilt on change, never per frame; **`holds_the_keyboard`** suspends
paging and cursor handling together; `ui::naming_row` floats under the bar, never in the flow.
`ui::apply_preset` is the strict origin-less synchronous path and **reports what it could not do
rather than dropping it**; `ui::init_patch` writes every default that is neither source-owned nor an
instance setting as one gesture. `DeferredPresetUi` is only an adapter: the plugin owns workers,
publication and acknowledgement. Tests that need a bar lay one out in `egui_kittest` and read the
accessibility tree ([NOTES.md § The controls](NOTES.md#the-controls-in-full)).

# Work Guidance

- A change here reaches every instrument and effect: run every plugin's tests after `cargo test -p
  mxm-preset`, not only this crate's.
- egui is pinned at `=0.36.1` and the MSRV is 1.95 for the controls; the format and the library
  alone would stand at 1.87, and are not split out until something framework-free needs them.
- Visual §15 review remains manual. A native file dialog over the banks folder is an owner decision,
  not an implied missing implementation.

# Verification

```bash
cargo test -p mxm-preset
cargo clippy -p mxm-preset --all-targets
cargo test -p mxm-mono-00 -p mxm-mono-01 -p mxm-mono-02 -p mxm-mono-03 -p mxm-mono-08 -p mxm-mono-pr1 -p mxm-poly-06 -p mxm-para-07 -p mxm-chorus-06 -p mxm-folded-spring -p mxm-bucket-delay -p mxm-shimmer -p mxm-classic-verb -p mxm-grain-fx -p mxm-fx-convolution -p mxm-creative-sampler -p mxm-drum-machine
```

# Child DOX Index

No child AGENTS.md files.
