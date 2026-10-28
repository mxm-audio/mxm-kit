# AGENTS.md — crates/mxm-preset

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

`mxm-preset` is the collection’s preset system: file format and category, on-disk library,
favourites, three-state loaded identity and app-bar controls. It serves every instrument and each
effect that qualifies under the parent preset rule.

The rules — what a preset is, how it is applied, where it lives, what a favourite is — are stated in
each plugin repository's `plugins/AGENTS.md`, *A preset is parameter values* (in full,
[`docs/plugin-conventions.md`](../../docs/plugin-conventions.md#a-preset-is-parameter-values-and-every-instrument-stores-them-the-same-way)), and implemented here.
This doc holds what is specific to the crate: its one trait, what stays with a plugin, and the
crate-specific contracts. The full text behind each rule is in [NOTES.md](NOTES.md).

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own any instrument's id, parameters or sounds — those
arrive through [`Instrument`](#an-instrument-is-four-core-answers-with-instance-setting-sparse-default-factory-preserved-and-durable-content-seams) — and does not own the drawing of the
browser: `crates/ui`'s `preset_browser` paints the rows and reports what a person did; this crate
knows what the rows are and what the actions mean. `ErasedParam`, the step laws and the
collection's one binding are [`mxm-params`](../mxm-params/AGENTS.md)'; this crate depends on it for
`ErasedParam` and re-exports nothing of it.

# Local Contracts

## An instrument is four core answers, with instance-setting, sparse-default, factory-preserved and durable-content seams

`Instrument` is implemented on a plugin's `Params` struct, in its `src/preset.rs`, and asks for:
the permanent `clap_id`; its `version()`, the plugin's `Plugin::VERSION`; `parameters()` as
`(id, &dyn ErasedParam)` **in declaration order**, the order files are written and Init writes in;
`identity()`, the `#[persist = "preset"]`ed `RwLock<PresetIdentity>` that keeps the loaded
preset's name and baseline with the patch; and `factory_files()`, the factory set as `(file name,
json)`, which by default reads the folder installed for that version (`factory_dir`:
`mxm/<clap id>/factory/<version>/` under the local data directory, one per version, the owner's
ruling of 2026-10-28) in its `order.txt`'s order (`read_factory_dir`), so a plugin implements it
only in a test, which reads its repository's folder, never the machine's. `clap` is what a plugin
gives nice-plug's preset capability: `PRESETS` (this format's metadata for a host's index, the
folder by id and version) and `Loader`, which loads a file as the browser loads an entry, factory
content only inside that folder. Everything takes `&dyn Instrument`, and a
`&MxmXxxParams` coerces, so a plugin's editor passes its params and nothing else.

The seams ([NOTES.md § The instrument's seams in full](NOTES.md#the-instruments-seams-in-full)):

- **A switch added after states were saved**: the plugin's `Plugin::filter_state` calls
  `add_switches_off(state, ids)` — first, if it may clear a malformed state — restoring each missing
  switch Off and extending a loaded preset's baseline with it at normalised zero.
- **Factory presets found at run time** (`Found`): the plugin scans and hands the list to
  `Library::with_found`; they list after the installed set as `Origin::Factory`, each `Entry` carrying
  its folder as `group`. **The library never looks anywhere itself.** An effect is an instrument
  here: four parameters or eighty, the crate does not know the difference.
- **Factory-preserved patch parameters** (`is_factory_preserved`, `mxm-fx-delay`'s Mix): consulted
  only on trusted `Origin::Factory` provenance from the installed folder, never a JSON claim;
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

The rules, all the crate's, not a plugin's ([NOTES.md § The rules in full](NOTES.md#the-rules-in-full)):

- **Factory preservation requires trusted provenance.** Only an entry from the installed factory folder, as `Origin::Factory`,
  may omit an id named by `is_factory_preserved`; everything else reports the omission. Captures and
  Init stay complete except for explicit sparse defaults.
- **Prepared before the gestures, committed after them.** `apply_preset_state` runs before any
  parameter write, so a rejected payload costs no gesture and is not marked loaded;
  `commit_preset_state` runs once after the whole burst, even for a recipe with no content. Generated
  Init follows the same order when `init_preset_state` is supplied.
- **`patch_replaced` follows every successful load and every Init**, a deferred one when it is
  acknowledged, and never a refused one. It is for what an instrument keeps outside the patch but
  owes to it: MXM Drum Machine's typed slot names go back to the ones it makes up. A synchronous
  load or Init calls it as the gestures are emitted, before a host may apply them and before the
  identity is installed; a deferred one at `acknowledge`, after both. So it never reads the
  parameters or the identity.
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

**Stored text is held here, for every plugin** (`stale_factory_texts`, `plans/plan-mxm-params.md`
P2). Each factory file's `text` must be the plugin's own formatting of the `v` beside it; `text` is
never loaded, so nothing else notices it going stale (D17: 134 route readings in `mxm-mono-00`). It
was one plugin's test; every plugin's preset tests now assert the list is empty, and a file that
does not parse is reported rather than skipped. **Each file is read in its own state**: its values
are applied first, callbacks and all, because a reading may depend on another parameter
(`mxm-mono-08`'s Mod frequency reads in the range Mod high selects). A generator writes its texts
the same way, through `refresh_texts`; formatting each value against a default instrument is what
left three mono-08 files storing low-range readings beside Mod high on.

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

*Since the split (2026-10-06):* the last line was the monorepo's; each of those packages is in its
own repository now, so run `cargo test -p <plugin>` in each, against this change.

# Child DOX Index

No child AGENTS.md files.
