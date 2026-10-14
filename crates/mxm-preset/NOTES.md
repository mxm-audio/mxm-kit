# NOTES.md — crates/mxm-preset

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

*Since the split (2026-10-06):* `plans/…` cited here is in the private archive; a `plugins/<plugin>/…`, `crates/<plugin>-dsp/…` or `apps/<plugin>-standalone` path is in that product's own repository, and a rule cited from `plugins/AGENTS.md` is in full in [`docs/plugin-conventions.md`](../../docs/plugin-conventions.md).

## The instrument's seams in full

**A switch added after states were saved** has a state-side seam beside the file-side
`default_missing_legacy_parameter`: `add_switches_off(state, ids)`, called from a plugin's
`Plugin::filter_state`, restores each missing switch Off and extends a loaded preset's persisted
baseline with it at normalised zero (the collection persists the identity under `"preset"`). A plugin
whose `filter_state` may clear a malformed state calls it first, so the clearing stays a no-op
(`an_older_state_gains_an_added_switch_off_and_its_preset_stays_clean`; the tempo syncs are its first
use, `plugins/AGENTS.md`).

**Factory presets can also be found at run time** (`Found`, the owner, 2026-09-28:
mxm-fx-convolution's impulse files are its factory set). The plugin scans where it keeps them and
hands the list to `Library::with_found`; `list()` puts them after the compiled set as
`Origin::Factory`, each `Entry` carrying the folder it came from as `group`. The library never looks
anywhere itself — the same injected-root rule as the user presets' — so a test hands it a list, or
none. **An effect is an
instrument here**: four parameters or eighty, the crate does not know the difference.

`mxm-fx-delay` opts into **factory-preserved patch parameters**: its compiled-in factory recipes
leave Mix at the live insert/send balance. The answer defaults empty and is consulted only when the
library/apply path supplies trusted `Origin::Factory`; factory status is provenance from the
compiled-in entry, never a JSON claim. Origin-less direct calls and user/bank presets remain strict,
and capture plus Init remain complete. This is independent of `is_source_owned`: Mix describes the
patch, not durable source content, and the source seam is keyed to payload presence rather than
origin. Both synchronous and deferred library paths carry trusted origin into resolution.

`mxm-creative-sampler`, `mxm-classic-verb` and `mxm-fx-curve` opt into synchronous durable content.
The sampler carries audio layers. The reverb carries a fitted space or an explicit absence,
kilobytes and never audio. FX Curve carries its complete authored stage model and additionally
supplies `init_preset_state`, so generated Init restores its default model; consumers that preserve
loaded source leave that answer empty. The default methods remain parameter-only and reject an
unexpected `state`. A durable-content instrument supplies: captured versioned JSON content; a cheap
deterministic fingerprint for dirty comparison; whole-payload preflight and application; the commit
that makes prepared content audible; and `is_source_owned`, the small set of ids that describe loaded
source rather than the patch around it. User saves carry the payload in the format's reserved `state`
field; `state = null` is a factory recipe that preserves current content. Existing parameter-only
plugins serialize exactly as before.

`mxm-fx-convolution` is the durable-content consumer that opts into
`DeferredPresetTransaction`. Its immutable source makes a recipe-owned model an explicit
`MergeWithCommittedSource` overlay, and preparing its FIR cannot block the editor. The transaction
captures targets and an identity outcome, returns monotonic work to the consumer, and stays Pending
until that consumer reports Ready or Rejected. `preset_row_deferred` and `overlays_deferred` are the
strictly opt-in production UI route: selection/arrows/Init return a `DeferredPresetUiRequest`, Save
and Save As are disabled while its adapter says Pending — including a naming row opened before the
request began — and newer selection remains available to supersede older work. This is a separate
API: existing `preset_row`, `overlays`,
`apply_preset_checked` and `init_patch` do not dispatch through it and remain byte-for-behavior
synchronous.

## The rules in full

These rules follow from those seams, and all are the crate's, not a plugin's:

- **Factory preservation requires trusted provenance.** Only a library entry compiled as
  `Origin::Factory` may omit an id named by `is_factory_preserved`; origin-less calls, writable
  user/bank files and consumers that do not opt in report the omission. This does not weaken the
  rule that captures and Init are complete except for an instrument's explicit sparse defaults.
- **Prepared before the gestures, committed after them.** `apply_preset_state` runs before any
  parameter write, so a rejected payload costs no gesture and is not marked loaded;
  `commit_preset_state` runs once after the whole burst, so new audio is never rendered through the
  patch it is replacing. It is called even for a recipe that carried no content, so an instrument
  holding something staged is not left holding it. Parameter-only instruments implement neither.
  The temporal barrier itself is the plugin's — the sampler's `AssetBank` or FX Curve's staged
  prepared engine — and this is the order it needs. Generated Init follows the same order when a
  consumer supplies `init_preset_state`; the default-empty answer preserves the established
  parameter-only or source-preserving Init behavior.
- **A preset writes source-owned parameters only when it brings the source they describe.** A root
  note and a region are measurements of one recording; `Preset::resolve` skips them whenever
  `state` is null, so a factory recipe cannot crop or detune whatever the player has loaded, while
  a user preset that embeds its audio does place them. Init skips them for the same reason.
- **Instance settings are host state, never preset sound.** `is_instance_setting(id)` declares
  parameters such as per-part audio destinations and external MIDI channels. They remain ordinary
  parameters in a DAW project, but capture/save, apply, Init, missing-parameter reports, the loaded
  identity baseline and dirty comparison all exclude them. A preset file written before or after
  the declaration therefore loads without rewiring the instance, and moving only one of these
  settings never marks the loaded sound modified. The deferred transaction follows the same
  rule: its Init writes and its loaded baseline omit them. This is deliberately distinct from
  `is_source_owned`: embedded-source user presets may write source-owned values; no preset writes an
  instance setting.
- **An omitted parameter preserves the current value unless the instrument names one explicit
  default migration.** `default_missing_legacy_parameter` makes the resolver write that parameter's
  own default and report no incomplete-file warning. Its first use was compatibility for parameters
  added after shipped preset files. `omit_default_parameter` is the sparse-default counterpart;
  `omit_captured_parameter` may additionally omit a value made irrelevant by compound patch state.
  Both require the same id in the missing-default bridge so loading actively clears prior state. The
  drum machine uses the pair to omit every unassigned route and to store an assigned zero route as
  presence alone. Unknown ids remain errors.
- **Deferred means no early visibility, and acknowledgement is not a new snapshot.** Pending,
  rejected, cancelled and superseded work emits no gestures and changes no identity. Ready emits only
  targets whose base value has not changed since begin, then calls one infallible pre-reserved
  publication hook. Deferred consumers expose an optional monotonic
  `Instrument::parameter_edit_revision`; publication always requires the unmodulated bits captured at
  begin to remain unchanged and, when a revision is present, requires that revision to remain
  unchanged too. The value guard closes the interval between the separate begin-time reads; the
  revision skips the target after an observed away-and-back edit, while the `None` default preserves
  value-only comparison for existing adapters. The consumer owns a genuinely base-only revision
  source: convolution samples unmodulated bits at
  process blocks, editor frames and transaction begin/publication because nice-plug's value callback
  also fires for modulation. Its explicit bound is an away-and-back base edit wholly between two
  observation points. A loaded target retains its intended canonical parameter baseline from begin and
  its durable fingerprint from that publication; `acknowledge` installs those retained values rather
  than recapturing the live patch, so edits during preparation or between publication and callback
  acknowledgement remain truthfully Modified. `ErasedParam::canonical_normalised` defaults to the
  identity for existing continuous adapters and only nice-plug's blanket implementation adds its
  plain/normalized snapping, keeping this opt-in extension source-compatible. Init clears identity.
  Newer unpublished requests and stale cancellation are monotonic. A published predecessor moves to
  its own awaiting slot when a successor begins; that successor cannot publish over the predecessor's
  callback slot, and cancelling or rejecting it cannot remove the predecessor's retained identity.
  Editor close cancels unpublished work by id, while an already published request retains its callback
  acknowledgement outside the editor's lifetime.
- **`patch_replaced` follows every replacement, whatever it carried, and never reads the patch**
  (2026-10-14). MXM Drum Machine names each slot, and a name typed over one belongs to the drum in
  it: when a kit load or Init changes the drums, the names go back to the ones it makes up (the
  owner, 2026-10-14). The bar and the browser load inside this crate and report nothing back, and
  `commit_preset_state` is no signal: Init calls it only when it carries content of its own. So
  `patch_replaced` is called once for every successful load, every Init and every acknowledged
  deferred transaction, and for no refused load. A synchronous load or Init calls it as the
  gestures are emitted: a host may apply them later (below), and a load installs its identity after
  it returns. A deferred transaction calls it at `acknowledge`, after the callback confirmed
  publication and after the identity is installed. Since the two orders differ, it acts only on
  what the instrument holds outside the patch, never on live parameters or the identity. Tests:
  `the_patch_is_replaced_by_every_load_and_init_and_no_refused_load`,
  `the_bar_and_the_browser_replace_the_patch`, and the deferred Init's acknowledgement.

**Synchronous gestures are not synchronous parameter application.** A CLAP host may queue every
GUI-authored begin/value/end and apply it only in a later process or `params.flush()` call. An
ordinary preset load therefore builds its identity baseline before emission: current canonical
values for parameters the preset preserves, and each written parameter's canonical intended target.
Reading the live atomics immediately after emission captures the patch being replaced and marks the
preset Modified when the queued values arrive. Saves still snapshot the live patch. The queued-host
regression applies the writes later and requires the loaded identity to become Clean.

## The shared tests in full

Shared transaction tests inject a base edit between value and revision capture so matching revision
alone cannot overwrite it, separately prove observed away-and-back edits, and require a published
predecessor to survive a cancelled successor while blocking that successor's publication. Shared
tests cover
favourites, the on-disk library, identity and format against a three-parameter
`TestInstrument` (`lib.rs`’s `testing` module), so each rule is proved once. What stays in a plugin's `preset.rs` is about **its** sounds: the
factory designs and their regeneration, the shipped files matching the design, every factory preset
parsing and resolving every parameter (including declared sparse defaults) and being audible, the list
beginning with Init, and the user root being under its own id. A test that would pass unchanged in
every plugin belongs here.

## Categories in full

`Category` is one word from a **fixed, collection-wide list** (`Category::ALL`, *Uncategorised*
last) — the owner's rule (2026-09-04) that a sound is categorised when it is saved, on every
instrument. The Save As row asks for the name and the category together; Rename keeps the preset's
own; Save keeps the loaded one's. **On the preset, not on a bank**, because a person looking for a
bass wants every bass they own in one list, whichever bank it came from. A file without the field —
every preset written before it existed — reads as *Uncategorised*, and so does a word this build
does not know (`#[serde(other)]`); **the schema stays at 1**, because neither direction is wrong,
only poorer, and a version moves when an older build would read something *wrong*. **Every factory
set carries a category per sound** — in its design tuple, mxm-mono-01's fifty included — and each
plugin holds it — `every_factory_preset_has_a_category`, or the equivalent assertion inside a
wider per-file test, as mxm-mono-08's `every_file_is_complete_distinct_categorised_and_owned` does: *Uncategorised* is for files
written before the field existed, never for a sound the collection ships.

**A found preset is filed under its folder** instead: the categories pane lists each `group` after
the `Category` rows, in name order, a group's row shows exactly its presets, and a grouped preset is
not counted under its own `Category` as well — a hundred rooms do not all land in *FX* too. The row
detail reads *Factory · Halls*. Save As still asks for a `Category`: a preset of your own is filed
the ordinary way. `found_presets_are_factory_presets_filed_under_their_folders` holds it.

## Banks in full

`Origin::Bank(name)` is a preset in `presets/<dir_name(name)>/`, a directory beside the user's own
files with a `bank.json` (`BankInfo`: name, author, comment) naming it; the user's own presets are
the bank called *My presets*, `Origin::User`, always first in `Library::banks()`. Save, Rename,
Delete and *exists* take the origin and go to that directory; a factory preset refuses every one of
them. A directory whose `bank.json` cannot be read is still a bank — its presets are somebody's —
under the directory's own name, carrying the problem; and the directory's name wins over the file's
when they disagree, because `dir_for` finds the presets from the name.

**On the wire a bank is one file**, `<name>.mxmbank.json` (`BankFile`: the bank's facts and every
preset inside it), in the **banks folder** `banks/` beside `presets/`. `import_bank` unpacks one
into a directory, refused whole when it is not a bank, not this schema or not this plugin's, and
refused with `ImportRefused::Exists` when a bank of that name is already here unless told to
replace — a person is asked, never silently overwritten; a preset inside the file that is for
another plugin is skipped. `export_bank` packs whatever presets the caller hands it, which is what
makes *save a category* the same act as *export a bank*; nothing is written for an empty list.

**A folder, not a required dialog.** A native file dialog cannot be driven from the player’s CLI,
and its Linux portal path cannot be verified from this machine. The sampler takes that
cross-platform dependency for source acquisition, but it does not make a bank dialog CLI-drivable.
So bank files are dropped into and picked up from the banks folder, `open_folder`
shows it in the platform's file manager with an arm for all three platforms, and a dialog may come
later as a convenience over the same folder. `Library::at(root)` has a banks folder only when the
root is called `presets`, as the real layout is; a sandbox that wants one names its root so.

**Favourites are keyed by origin and name** (`favourite_key`), because two banks may each carry a
*Sub bass* and starring one must not star the other — and a plain name in the index, from before
banks, still stars a factory or user preset (`is_favourite`): nobody's stars go out for a format
change.

## The browser in full

`ui::overlays` draws what floats under the bar: the browser when it is open, and the naming row.
The browser is `mxm-ui`'s `browser::panes` in an `egui::Area` over the view area, fed from
`PresetUi`'s filters — a bank or every bank, a category or the starred or every category, and a
search — and `PresetUi::visible` is the one list both the browser and **the bar's arrows** step
through, so with a bank and a category chosen previous and next mean something again. The panes
count what the *other* pane's choice leaves: the banks pane counts under the category filter, the
categories pane within the selected bank, and a category with nothing in it is not listed.

The bank actions are the library's: *Import…* lists the bank files in the banks folder and unpacks
the one chosen, asking before replacing a bank of the same name; *Export…* opens the naming row and
packs **the presets shown** — the filter is the bank, which is what makes *save a category* one
act — **except presets found in a folder** (`Entry::found`), which name a file there rather than
carry it: they are left out and the notice says how many, and an export of nothing else writes
nothing and says to share the folder instead; *New bank…* makes an empty one and looks at it, so *Save As* then puts a sound in it (*Save
As* goes to the loaded preset's bank, else the bank being looked at, else the user's own); *Open
folder* shows the banks folder. What went right or wrong shows along the browser's foot while it is
open, and floats under the bar when it is closed.

**The name in the bar opens it, and so does the developer channel**: CC 117 through
`PresetUi::set_browser_open`, so a script can look at the browser without a mouse. A click on the
content around the browser closes it; a click on the bar above does not, so the bar's own controls
keep working while it is up. **Left and right move the keyboard between the panes; up and down move within the focused one** —
choosing the next bank or category, or stepping the filtered list, which loads as it goes so
stepping is auditioning — and **Enter is done**: the browser closes with what is loaded, or with the first
match when nothing from the list is — type a name, press Enter (the owner's ask, 2026-09-04). `the_browser_opens_from_the_name_and_a_category_narrows_the_list` holds
the door, the panes and the filter from the accessibility tree.

## The controls in full

`ui::PresetUi` holds the library, its listing (rebuilt when something changes it, never per frame),
favourites, the open Save As or Rename, and the last problem. **`holds_the_keyboard` answers whether
one of those surfaces owns the keys** and suspends paging and cursor handling together. `ui::preset_row` fills §3.1's patch
slots through `mxm-ui`'s browser and acts on what comes back; `ui::naming_row` floats the Save As
and Rename row under the bar, never in the flow, because the fixed layout was measured without it;
`ui::apply_preset` is the strict origin-less place a synchronous preset becomes bracketed
parameter writes, and reports what it could not do rather than dropping it — two of the five copies
had dropped it. The browser's synchronous and deferred library paths supply the entry's trusted
origin; only that provenance may accept a factory-preserved omission. Application preflights and
prepares opt-in synchronous durable state before emitting any parameter gesture, and
commits it after the last one; a rejected payload is reported without writes and is not marked
loaded. `ui::init_patch` writes every default that is neither source-owned nor an instance setting
as a complete gesture; parameter-only consumers own neither. `transaction::DeferredPresetTransaction` is the explicit asynchronous sibling;
it does not alter either synchronous function. `DeferredPresetUi` is only an adapter of request
callbacks and pending state; the plugin still owns workers, publication and acknowledgement. Tests
that need a bar lay one out in `egui_kittest` and read the accessibility tree, including the oracle
that pending disables Save while selection and Init still produce deferred requests.
