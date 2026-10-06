# Adding an instrument to the collection

A working route from "I want to build `mxm-<category>-<model-token>`" to a plugin that loads in a DAW,
sequences in MXM Player, and passes the gates — plus the traps that are only learned by walking into
them.

**This is a howto, not a contract.** Every rule here is owned somewhere else, and where the two
disagree the owning `AGENTS.md` wins. The table in [§10](#10-where-each-rule-actually-lives) says
which document owns what. What this file adds is the *order* and the *gotchas*, which live nowhere
as a unit.

*Since the split (2026-10-06):* this guide was written for the monorepo, where every instrument was
a folder. An instrument is now a repository of its own under
[github.com/mxm-audio](https://github.com/mxm-audio), laid out like the existing ones
([mxm-mono-01](https://github.com/mxm-audio/mxm-mono-01) is the usual model): `plugins/<plugin>/`
with its `host-tests/`, `crates/<plugin>-dsp/`, `xtask/`, `bundler.toml`, and the kit's crates as
git dependencies at a tag. A path below is inside that new repository unless it says otherwise;
`apps/mxm-player/…` is in [mxm-player](https://github.com/mxm-audio/mxm-player), `crates/mxm-…`
shared crates and these docs are in this kit, and `plans/…` is in the private archive.

Read the new repository's root `AGENTS.md` and its `plugins/AGENTS.md` (in the monorepo, the root
`AGENTS.md` and `plugins/AGENTS.md`), with the [plugin conventions](plugin-conventions.md) and
[collection rules](collection-rules.md) they link to, before starting. This document assumes them.

---

## 1. What an instrument actually is

Five places, and no more:

| Path | What goes there | Owned by |
|---|---|---|
| `docs/briefs/<plugin>.md` | The §14 design brief — **written first** | [Design system](MXM_DESIGN_SYSTEM.md) §14 (in the monorepo, [`docs/AGENTS.md`](AGENTS.md)) |
| `crates/<plugin>-dsp/` | The whole voice as plain Rust. No framework types, and no runtime dependency but the shared routing, `crates/mxm-modulation` | [`crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) |
| `plugins/<plugin>/` | The nice-plug shell: identity, params, MIDI, presets, editor | The repository's `plugins/AGENTS.md`, contracting [`plugin-conventions.md`](plugin-conventions.md) |
| `plugins/<plugin>/control-map.json` | Which parameters fill the collection's controller roles | [`MXM_CONTROL_MAP.md`](MXM_CONTROL_MAP.md) |
| `bundler.toml` + workspace `members` | One row each | The repository's root `AGENTS.md` |

Nothing goes into `apps/mxm-player` (in [mxm-player](https://github.com/mxm-audio/mxm-player)).
**The player must never need to have heard of your
instrument** — if adding one requires a player change, the design has failed, and
`t5_control_map.rs::a_new_instrument_gets_the_collection_layout_by_adding_a_file_and_nothing_else`
(in mxm-player's `apps/mxm-player/tests/`) is the assertion that says so.

[`apps/mxm-mono-01-standalone`](https://github.com/mxm-audio/mxm-mono-01/blob/main/apps/mxm-mono-01-standalone/AGENTS.md)
in mxm-mono-01 is the one exception worth knowing about: it is a per-plugin harness,
so a second instrument that wants the same convenience gets its own `apps/<plugin>-standalone`.
It is optional. The player opens your editor as a floating window without it.

---

## 2. Decide these before writing code

Each of these is expensive to change later, and two of them are permanent.

**The name.** `mxm-<category>-<model-token>`, all lower case. The token is a short approved
normalisation of the inspiring hardware's model, not a sequence and no longer necessarily its last
two digits. Existing numeric names stay permanent; the first name under the revised rule is
`mxm-mono-pr1`. Intake sends an ambiguous token or collision to the owner instead of inventing a
normalisation. The rail's *Naming* section owns the record (since the split,
[`collection-rules.md`](collection-rules.md#naming)).

**`CLAP_ID` — permanent.** `dk.mxm.<plugin>`, assembled with `concat!` from the `plugin_name!`
macro, never from `CARGO_PKG_NAME`. **It cannot collide with anyone else's** — the namespace is a
domain the project owns and every plugin here lives in this repository (since the split, in one of
the project's own repositories) — so the only thing to get
right is that it is permanent. It has been changed exactly once in this project, while pre-alpha and
undistributed, and that change orphaned the user preset directory, its `favourites.json`, parked
locks in the player's settings, and locks inside saved sequences. There is no second time.

**Parameter `#[id = "…"]` strings — permanent.** Renaming the Rust field is free; renaming the id
breaks every saved project and every `control-map.json` entry pointing at it.

**Which effects, if any, are inside it.** The rule is *no effect that was not on the original
instrument* — the root `AGENTS.md` for the principle (since the split,
[`collection-rules.md`](collection-rules.md#the-goal-and-how-much-licence-a-copy-has)), `plugins/AGENTS.md`
for what it binds you to ([`plugin-conventions.md`](plugin-conventions.md#an-instrument-ships-the-effects-its-original-had-and-no-others)). Answer it in the brief
(Phase 0) with the evidence about the hardware, because it decides the DSP crate's shape before you
write any of it.

**Licence.** GPL-3.0-or-later, the new repository's root `LICENSE`: since the split (2026-10-06)
every product is, and a plugin folder carries no `LICENSE` of its own
([`plugin-conventions.md`](plugin-conventions.md#licensing)). Still decide *before* copying anything
from another project, because after is too late. *In the monorepo:* MIT if all the code was
original, decided before copying anything from a GPL project (VCV Rack is GPL-3.0), and each plugin
folder carried its own `LICENSE`, which is what let a GPL-derived instrument sit beside MIT ones
without relicensing the workspace.

**MSRV.** The DSP crate's one runtime dependency is `mxm-modulation`, which is dependency-free at
**1.87**, so the DSP crate stays there too (1.88 if it uses `let` chains) and states it explicitly. The
plugin crate pulls egui through its editor and is therefore **1.95**. Record both in the root
MSRV table. The editor depends on the DSP; never the reverse, and never let the DSP crate inherit
the GUI floor.

---

## 3. The order of work

The order is not arbitrary. Each phase is verifiable on its own, and doing them out of order means
debugging two layers at once.

### Phase 0 — the brief

`docs/briefs/<plugin>.md`, answering all ten questions of design system §14. **Gating, not
polish**: it decides the section sequence, the surface classification, the identity accent, and the
init patch, and the editor implements it rather than re-deciding it.

The one question people skip is §14.9 — *what is removed from the source hardware layout, and why*.
Answer it beside its twin: **what order the blocks go in**, which
[Phase 5a](#phase-5a--what-order-the-cards-go-in) decides from the audio path and
`research:interfaces/panel-layout-and-signal-flow.md`. Deciding it in the brief is what stops it
being decided by whichever card happened to be written first.
Answer it. "Kept: section sequence and membership, because they are the signal flow. Removed:
appearance, geometry, control style, colour arrangement, typography, trade dress" is
`mxm-mono-01`'s answer and a good template.

**Catalogue the machine's warts here too**, with evidence — the drift, the noise floor, the range
that stops short, the coupling nobody would design on purpose. A copy reproduces them (see the
root `AGENTS.md`; since the split, [`collection-rules.md`](collection-rules.md#the-goal-and-how-much-licence-a-copy-has),
*A copy is warts and all*), and the brief is where they stop being folklore and become
something a test can pin. A wart discovered in Phase 1 and left unwritten is a wart somebody
removes in Phase 2 while tidying.

### Phase 1 — the DSP crate

`crates/<plugin>-dsp`, framework-free, with `mxm-modulation` as its only `[dependencies]` entry.
Plain values and a sample rate in, samples out. **Modulation is the collection's routing from the
first line** — declare the instrument's sources and targets against `mxm-modulation` rather than
wiring a depth per path ([`plugin-conventions.md`](plugin-conventions.md#every-instruments-modulation-is-the-shared-routing),
*Every instrument's modulation is the shared routing*; `crates/mxm-mono-pr1-dsp/src/routing.rs` in
[mxm-mono-pr1](https://github.com/mxm-audio/mxm-mono-pr1/blob/main/crates/mxm-mono-pr1-dsp/src/routing.rs)
is a worked example). Read [`code-review-notes.md`](code-review-notes.md) §7
first: an absent route still owes a cleared source and a snapped smoother. **Declare the target list
by [`plugin-conventions.md`](plugin-conventions.md#declaring-the-target-list--what-mxm-mono-00-had-to-discover-twice)'s
*Declaring the target list*** before the first id is written: one target
per thing that moves, named for it, and no destination switch anywhere — `mxm-mono-00` found each of
those after its ids had shipped.

Write the tests that regress silently, because every one of them has:

- silence in gives **exactly** zero out after decay (this is also the denormal-flush test)
- no NaN or inf across a sample-rate × cutoff × resonance sweep
- output inside a stated `pub const` bound under overdrive
- `reset()` leaves no tail
- any self-oscillation threshold, **measured**, at 44.1 / 48 / 96 / 192 kHz
- aliasing measured against an additive reference computed inside the same test — and where the
  hardware's own artefact is the point, the bound states **the machine's** figure, with the brief's
  evidence beside it, rather than the figure a clean oscillator would reach

**Do not write the rulers.** Add `mxm-measure` to `[dev-dependencies]` — zero dependencies, the same
1.87 floor, and not in your shipped graph — and take the measurement from
[`crates/mxm-measure`](../crates/mxm-measure/AGENTS.md): peak and RMS, decibels and cents, a
component's amplitude, a filter's transfer gain, frequency by interpolated zero crossings, the
additive and trivial aliasing controls, a WAV encoder for the demo, and the exact-silence and
never-NaN observations the list above asks for. Every one is scored against a closed form there.

**What you still write is the argument.** The crate carries no thresholds and never will, so the
bound, its headroom and the reason for it belong in your test — [`oscillators/06-testing.md`](oscillators/06-testing.md) §6.4
says where a threshold comes from. Two crates once asserted one-cent tuning while measuring with a
±9-cent ruler, which is the failure this split exists to prevent: a shared ruler you can trust, and a
threshold you had to justify. If you need a measurement the crate lacks, check its **declined
register** first — the concept may already exist as somebody's local composition, and yours would be
the second consumer that promotes it.

Flush denormals in the DSP itself, on every recursive state. Do not rely on a framework FTZ guard —
it can be a no-op without an opt-in feature, and DSP-level flushing is what preserves exact digital
silence.

Verify: `cargo test -p <plugin>-dsp`, `cargo clippy -p <plugin>-dsp --all-targets`, and a render
example that writes a WAV, because the question *does it sound like a synthesizer* has no unit test.
**Name it for the crate or the machine — `examples/<token>_render_demo.rs`, or `sh2_demo.rs` —
never a bare `render_demo.rs`.** Every example in the workspace links into one flat directory, so a
shared name means a shared output file: cargo runs whichever won the race and a parallel build dies
on a linker error. Root `AGENTS.md` *Naming* has the rule (since the split,
[`collection-rules.md`](collection-rules.md#naming)).

### Phase 2 — the plugin shell and parameters

`plugins/<plugin>/src/lib.rs` + `params.rs`. Copy `mxm-mono-01`'s shape:

- `plugin_name!` macro at the top, then `NAME` and `CLAP_ID` derived from it
- `impl Plugin` with `AUDIO_IO_LAYOUTS` declaring **no main input** for an instrument
- `MIDI_INPUT: MidiConfig::MidiCCs` (see [gotcha 6](#6-declare-midi-input-or-the-players-panic-does-nothing))
- `type Editor` as an **associated type**, and `fn activate` — this is nice-plug, not nih-plug
- `nice_export_clap!(YourPlugin);` at the end
- `crate-type = ["cdylib", "lib"]` in `Cargo.toml`, and **do not** enable nice-plug's `standalone`
  feature here

Parameters: one `#[derive(Params)]` struct behind an `Arc`. Smooth signals, not coefficients. The
routes are `mxm-modulation-params` pairs beside it — `plugins/mxm-mono-pr1/src/routes.rs` in
[mxm-mono-pr1](https://github.com/mxm-audio/mxm-mono-pr1/blob/main/plugins/mxm-mono-pr1/src/routes.rs) is the
shape — and the paths the machine itself wires are present in the init patch.
Store gain as linear gain and format as dB. Sentence-case, non-cryptic names, identical in the
editor and in host automation.

The init patch is a contract, not a preference: **every amount starts at zero, every configuration
starts somewhere musically useful**, and the init patch and the CLAP `default_value`s are one set of
numbers. Glide time is an amount despite being a time; envelope times are configuration despite
being amounts, because the envelope is always running.

Verify: `cargo test -p <plugin>`, and a defaults test that pins the *rule* (every amount is zero)
rather than the taste, so a retune survives and a non-zero depth fails.

### Phase 3 — events and MIDI

Note on / off / choke, pitch bend, CC 1, CC 120, CC 123. Per-channel bend and mod-wheel state.
`process()` splits on events **and** caps internal blocks at a constant — splitting only on events
lets an event-free buffer become one arbitrarily long block.

Return `ProcessStatus::Tail(n)` while releasing and `Normal` when truly idle, and make the tail
reflect *audible* output rather than hidden pre-VCA state. The player's export uses a −90 dBFS
silence threshold on the bar after the pattern to decide whether the sound ended inside the file; a
tail that lies about itself shows up there.

### Phase 4 — presets and Init

**Not optional.** Every instrument ships the whole preset system: the browser in the app bar, a
factory set, the generated Init, favourites, and Save / Save As / Rename / Delete. An instrument
without presets is unfinished, not minimal — this phase has been forgotten once and retrofitted
once, which is why it is spelled out here.

**Nothing is copied.** The system is [`crates/mxm-preset`](../crates/mxm-preset/AGENTS.md): add
`mxm-preset.workspace = true`, give your `Params` a `#[persist = "preset"] pub preset:
RwLock<mxm_preset::PresetIdentity>`, and in `src/preset.rs` implement `mxm_preset::Instrument` —
your `CLAP_ID`, `all_parameters` mapped to `(id, param)`, the identity slot, `FACTORY_FILES` —
beside the factory set and its tests; `mxm-mono-02`'s `preset.rs` is the shape. The editor calls
`mxm_preset::ui::preset_row` in the app bar's patch slot and `overlays` right after the bar — the
browser and the naming row float there — and holds one `PresetUi::new(params)`. The rules the crate holds, so you do not have to:

- two numbers per parameter, and **only `v` is read** — `text` exists so the file diffs
- applying is bracketed parameter writes, never a state blob
- factory presets are compiled in with `include_str!`, because a directory beside a `.clap` is not
  part of it on every platform
- **a factory set is content, and fifty sounds is the floor** (every instrument ships fifty;
  the owner raised the floor from twenty on 2026-09-04). Design them as overrides-on-defaults in the test module's
  `FACTORY_DESIGN`, generate the JSON with the `#[ignore]`d `write_the_factory_presets`, and keep
  the runs-by-default test that compares shipped files against the design — it is what catches a
  forgotten regenerate. The `#[ignore]`d mapping-table facility prints what each normalised value
  means, so the numbers are chosen with eyes open. **Write it before the designs, not after**:
  mxm-mono-08 shipped fifty sounds chosen without one, and they turned out to be one sound fifty
  times
- user presets live under `dirs::config_dir()`, and the root is **injected** so tests cannot write
  into the config directory of whoever ran them
- **Init has no preset file, so it cannot be deleted** — it is generated from the parameter
  defaults, and a test compares them parameter by parameter
- dirty is a comparison against a baseline read back **from the parameters** after applying, never
  from the JSON: a stepped parameter canonicalises what it is given
- **a sound is saved with its category**, chosen in the Save As row beside the name; give every
  factory design one

It was five deliberate copies until 2026-09-04, when banks and categories made the extraction due —
a shared crate is extracted from the evidence of honest copies, not ahead of it. The binding followed
on 2026-09-24, from eighteen copies: a new editor's `editor/binding.rs` is one line,
`pub use mxm_preset::binding::*;`, and a plugin's own label table goes in `Bound::panel`.

### Phase 5 — the editor

Read [`crates/ui/AGENTS.md`](../crates/ui/AGENTS.md) first. Then:

- **The editor is a panel, not a window.** `editor::panel` takes a `Ui` and draws into it. It does
  not create a window, run an event loop, or own a swapchain. That is what lets the same code be the
  CLAP editor and a standalone harness's contents.
- **Widgets and theme come from `mxm-ui`.** Plugins supply labels, bindings and data. A colour or a
  spacing literal in plugin code is a token that has escaped `crates/ui`.
- **One place brackets gestures.** `binding::Bound::apply` is the pattern: `begin_set_parameter` /
  `set_parameter_normalized` / `end_set_parameter`, once, rather than 27 chances to leave a host's
  automation lane latched. This one is load-bearing for the player — see
  [gotcha 3](#3-an-unbracketed-gesture-breaks-the-players-step-editing).
- **DSP never reads editor state**, and telemetry goes the other way through atomics only, written
  once per block. Two rules worth carrying: a **peak is max-combined and reset on read**, so a frame
  the UI missed cannot hide a transient, and a **clip latches** until acknowledged.
- **The app bar carries the display scaler** — `mxm_ui::shell::zoom_control`'s 75–200% steps, beside
  `mxm_ui::shell::editor_theme_control`; neither is copied into a plugin. The window resizes and the cards reflow now (Phase 5a above, and §3.4), so
  zoom is no longer how a player fits the editor to a display — it is how they set how *large the
  controls are*, independently of how many fit across. Both still matter on a high-DPI display. Zoom
  is **chosen, not derived from the window** — deriving it feeds back (zoom sets size, size sets
  zoom) and the window jitters; both mono editors record having walked into that, and a window that
  reflows makes that feedback easier to reach, not harder.
- **The knob draws the unmodulated value; modulation is drawn over it.** See
  [§5](#5-what-the-player-expects-of-your-instrument) — this is the whole of your integration with
  the sequencer, and it needs no protocol.

- **The developer channel** — `plugins/AGENTS.md`, *A developer channel in every editor*
  ([`plugin-conventions.md`](plugin-conventions.md#a-developer-channel-in-every-editor)). Copy
  the four constants, the `dev_cc` field, the four `MidiCC` arms, the four telemetry request slots
  and the hook at the top of `panel` from any sibling, and the gate test with them. It is how you
  will look at your own views without a mouse: start the player from a shell with `MXM_DEV_CC`
  exported, `mxm-cli load <plugin>`, then `mxm-cli cc 119 <category>` (0–5, or 127 for Parameters),
  `mxm-cli cc 118 127` for the expander, `mxm-cli cc 117 127` for the preset browser and
  `mxm-cli cc 116 <0–2>` for the theme.
- **Cards are pages derived from the space, and the opening size is measured with every expander
  open.** `mxm_ui::paging::editor` takes each card's stable key, primary category, kind, **floor**
  (§4.3), **ceiling** and same-category preferred groups; page count follows the window, and a row
  is as tall as its tallest card. Hold the opening size with
  `mxm_plugin_test::opening_size::is_the_budget_hugged`, and read painted shapes rather than `globally_used_rect`,
  which reads as full for any window taller than its content. `plugins/AGENTS.md`'s editor contract
  ([`plugin-conventions.md`](plugin-conventions.md#editor-contract)) has all of it, and the two editors that shipped clipped without the measurement.
- **The keyboard cursor runs in every editor.** `panel` takes a `navigation::State`, calls
  `navigation::paged` before the cards, and wraps every parameter control in `navigation::at`;
  `mxm_plugin_test::keyboard_checks`'s coverage check proves none was missed (`plugins/AGENTS.md`,
  *The keyboard cursor runs in every editor*;
  [`plugin-conventions.md`](plugin-conventions.md#the-keyboard-cursor-runs-in-every-editor-and-each-one-owes-it-three-things)).

Run design system §15's QA gate before calling the editor done. It is manual and it is real.


### Phase 5a — what order the cards go in

**The rule is design system §3.4's**, and it is normative there; this section is the route to it and
the evidence behind it, which is `research:interfaces/panel-layout-and-signal-flow.md` — fourteen
hardware panels and three plug-ins. The useful part of that survey is not what any panel looks
like — §2 forbids copying one — but **which parts of a panel the circuit decides and which parts a
designer does.**

**The whole of what is universal**, across all thirteen subtractive keyboards it counts:

```
[ sources ] → [ mixer ] → [ filter ] → [ amplifier ] → [ output ]
```

That is not a convention, it is the circuit. Everything else — where the LFO goes, where the
envelopes go, whether the mixer is a block at all — is a family habit, and each split runs close to
even.

**Order by the signal chain**, then: what feeds it, the chain itself, then what it feeds. §3.4 has
the rule and the placements; three things are worth repeating here because they are where a designer
goes wrong.

**A reflowing layout has no positions, only sequence.** "Modulation below the audio row" is a
statement about *where on the panel*, and a card in a wrapping layout lands wherever the window puts
it. Taken literally it becomes "last in the sequence", which no surveyed instrument does and which
puts an LFO after the filter it feeds. Do not cite it.

**What feeds the chain comes before it.** The voice block is upstream — the keyboard's note becomes
the oscillators' pitch CV by way of portamento and priority — and so is an LFO that reaches the
oscillator or the filter. Seven of thirteen panels put the LFO first for that reason.

**Place a card by what it is, not by what it is patched to.** An LFO that reaches nothing until the
matrix says so is still an LFO and belongs with the other modulation sources. Routing is state, a
preset changes it, and a panel that rearranges itself when a preset loads is not a panel.

**Envelope placement follows addressability, not taste.** Hard-wired to one destination: beside that
destination, named for it — `mxm-mono-00`'s two ADSRs. Shared or selectable: after the path it
serves, because there is no one destination to sit beside — `mxm-mono-01`'s single envelope drives
the filter *and* the amplifier, and `mxm-mono-02`'s drives a third thing as well.

Two more from the survey, easy to get wrong:

- **An Amplifier card that looks thin is not a mistake to pad out.** Three of thirteen machines had
  no amplifier block at all, and only six of the ten that did carried a level control.
- **Depth lives at the destination, whatever the machine did** (the owner, 2026-09-22). Roland puts
  the modulation amount in the block being modulated; Moog and Sequential put it at the source with
  destination switches. The collection's interface is the first on every instrument: a source is
  chosen on the card it moves, through `mxm_modulation_params::ui::stack`, and **no card chooses
  where its own output goes**. A machine's destination switch becomes routes into each destination
  it could reach, present in the init patch where the switch pointed — `mxm-mono-00`'s DESTINATION
  is its two pitch inputs' glide and LFO 1 routes. The copy is of the function, not the face.

### Phase 6 — the control map

`plugins/<plugin>/control-map.json`, mapping the collection's role names to *your* parameter ids.

- Role names come from [`MXM_CONTROL_MAP.md`](MXM_CONTROL_MAP.md), which is normative. A file
  naming a role the standard does not declare is refused, with the typo named. Adding a role means
  amending the standard first, in the same pass.
- Reference parameters by their permanent string `#[id]`, never by display name.
- **Leave a role out when your instrument does not have it.** Empty slots stay inert. That is what
  lets a two-oscillator instrument fill `osc2.*` without anything moving for anyone else.
- Eight slots per page, maximum.

`cargo xtask bundle` stages it as `target/bundled/<Bundle Name>.control-map.json`, which is where
the player looks. A plugin with no map is not an error — it simply has no roles filled.

### Phase 7 — bundle, validate, host

```bash
cargo xtask bundle <plugin> --release       # -> target/bundled/<plugin>.clap + .control-map.json
cargo xtask bundle <plugin>                 # debug too: assert_process_allocs only fires in debug
clap-validator validate "target/bundled/<plugin>.clap"
```

`clap-validator` is **not a cargo dependency and not on `PATH`**. Install it once:

```bash
cargo install --git https://github.com/free-audio/clap-validator.git --locked
```

Its absence is silent — the command is simply not found — and a verification block that "passed"
without it skipped the gate that matters most. **Zero failures, in both debug and release,** is the
expectation; instruments and effects exercise different subsets, so the passed count is not a fixed
number. Any failure is a regression, not a known issue.

Then, in order:

1. `cargo run -p mxm-player --release` in a checkout of
   [mxm-player](https://github.com/mxm-audio/mxm-player), load the plugin, play it. *Show editor* opens your own
   interface in a floating window; this is the only place the hosting path — `create`, ownership,
   `destroy`, reopen — is exercised at all.
2. A standalone run if you built one, for working on the editor without a host in the picture.
3. A real DAW at a small buffer size with automation running. Host parenting, host-driven resize,
   scale changes and open/close ordering happen nowhere else, and they are where embedded editors
   typically break. This gate is currently **recorded-unmet** for `mxm-mono-01`; be honest about
   whether you actually ran it. Getting the bundle where the host will find it is its own trap on
   Windows — see *Installing a bundle in a DAW* in
   [`plugin-conventions.md`](plugin-conventions.md#installing-a-bundle-in-a-daw) before assuming a copy was enough.

### Phase 8 — closeout

- Workspace `members`, and `default-members` if it should be in a plain `cargo build`
- `bundler.toml` row — crate name to display bundle name (the plugin's test calling
  `mxm_plugin_test::bundle::is_named` fails if they disagree, and nothing else would catch it)
- Root MSRV table
- `README.md` in the plugin folder, linked from the root `README.md`
- No CI is used (root *Windows, Linux and macOS*): the verification commands in the crate's own
  AGENTS.md are what runs, by hand, on the development machine. If you add a standalone harness,
  run the `cargo tree` boundary check on your plugin too.
  *Since the split (2026-10-06):* CI runs the root `AGENTS.md`'s verification on Windows, macOS
  and Linux, but only on a `v*` release tag or when started by hand (the owner, 2026-10-06). Copy
  `.github/workflows/ci.yml` from an existing product repository with your plugin's name, and
  before a push run the verification yourself on Windows and on Linux (WSL); only CI reaches macOS.
- **The DOX pass.** Update `plugins/AGENTS.md`, the root Child DOX Index if a new AGENTS.md
  appeared, `docs/AGENTS.md` if a brief was added, and delete anything that has gone stale.
  *Since the split:* a product repository has no `docs/AGENTS.md`; add the plugin to
  [`plugin-conventions.md`](plugin-conventions.md)'s *The plugins* table and *Each plugin's own
  contract* index here. The maintainer adds its editor to the collection-wide `editor_resize` inventory,
  which is not public yet.

---

## 4. Verification, all together

```bash
cargo build                                     # default-members: no eframe, cpal or midir
cargo build --workspace                         # everything, including the player the host tests link
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

cargo test -p <plugin>-dsp
cargo test -p <plugin>
cargo xtask bundle <plugin> --release
clap-validator validate "target/bundled/<plugin>.clap"
cargo test -p <plugin>-host-tests               # the slow tier: the release bundle through MXM Player
```

*Since the split (2026-10-06):* the block is the new repository's own; `clippy` and `fmt` take the
flags CI uses, and the host tests are a package of their own. Run it on Windows and on Linux (WSL)
before a push. Report which of these actually ran. `cargo build` alone does **not** produce a loadable plugin, and
a clean build is not a working synth: audio bugs are not compile errors.

---

## 5. What the player expects of your instrument

The player hosts any CLAP and knows nothing about you. Everything below is a property your plugin
either has or does not; there is no handshake, no capability negotiation beyond CLAP's own, and
nothing to register.

### It must fit the v1 compatibility envelope

`src/envelope.rs` (mxm-player's [`apps/mxm-player/src/envelope.rs`](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/src/envelope.rs)) refuses, **with the reason shown**:

| Refused | Because |
|---|---|
| Any audio input port | v1 hosts instruments only |
| More or fewer than one audio output port | The player drives exactly one |
| An output port not flagged main | CLAP requires the host to identify one |
| A main output that is not mono or stereo | Nothing else is in the envelope |

An instrument with `main_input_channels: None` and one stereo output passes. If you are refused,
the menu says why on the row — read it rather than guessing.

### Sequencer locks are `CLAP_EVENT_PARAM_MOD`, and that decides two things

A step that sets a parameter sends an **offset**, not a value. The parameter's own value stays the
patch for as long as the sequence runs. Consequences for you:

- **Only a parameter advertising `IS_MODULATABLE` can be sequenced**, and the player will not send a
  `PARAM_MOD` to one that has not. nice-plug grants that flag to every parameter that is automatable
  and not hidden — so a parameter you mark `HIDDEN` or `NON_AUTOMATABLE` silently becomes
  unsequenceable. That may be exactly right; know that you are choosing it.
- **`ext_params_get_value` reports the modulated value.** The player corrects for this on its own
  side; your editor should not try to. nice-plug already keeps the unmodulated value and the applied
  offset readable separately, which is what `binding::ErasedParam` exposes.

### Your editor should mark a modulated knob, and it costs nothing to

`mxm-ui`'s `ParamView` carries `marked` (something other than this control is also setting this
parameter) and `modulation` (how far). The knob draws its own value and paints an arc to where the
modulation takes it; a slider draws the same line under its track; a segmented control takes an
`Option<usize>` naming the cell the *modulated* value lands on.

**The editor does not know it is talking to a sequencer and must not need to.** It works in any host
that modulates. In MXM Player the mark happens to mean *this step deviates from the patch*; no
protocol exists or is wanted.

Two details that shipped wrong once each, so take them for free:

- **Aim double-click at the unmodulated base, not the factory default.** While a host modulates a
  parameter its unmodulated value *is* the patch, so §7.1's "put this back" means the patch.
  `binding::reset_target` is the one-liner. Shipped wrong once: a locked Range sounding 2'
  double-clicked to 8'.
- **A segmented control needs `default_cell`, and a click on the selected cell must fire while
  something else is sounding.** Refusing it reads as the button being broken — *"I can change 16' to
  8' but not back to 16'"*.

### Audio export needs the `state` extension

`sequencer::export` renders through a second plugin instance with the live one's CLAP state restored
into it. **A plugin with no `state` extension is refused**, naming the reason — envelope negotiation
does not require the extension, so this is reachable. nice-plug provides it, so you get it by
existing; know that it is load-bearing before you consider suppressing it.

### The floating editor comes from the vendored nice-plug

Upstream nice-plug refuses every floating window configuration outright. The patch in
[the nice-plug fork](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (the monorepo's
`vendor/nice-plug`) lifts that, and without it the player cannot show *any*
plugin's interface on any platform. Your plugin inherits this by depending on the workspace's
`nice-plug` — since the split, by the root `[patch.crates-io]` entry that points `nice-plug` at the
fork, as every product repository does. If a refresh drops the patch, `t7_editor.rs` fails — but only for a plugin that has a
test there, so add one. *Since the split:* `t7_editor.rs` is in mxm-player and opens mxm-mono-01's
editor; every product's floating editor is opened by the collection-wide `editor_resize`, which
the maintainer runs (not public yet) and adds yours to.

### Everything a person can do, a machine can do

The player's CLI (`src/cli.rs` — mxm-player's `apps/mxm-player/src/cli.rs` — one JSON line in, one out) is how an instrument gets tested without
screenshots:

```
dump | load <plugin> | select <1-16> | deselect | set <param> <value> | reset <param> |
toggle <step> <note> | tie <step> | lock <step> <param> <value> | unlock <step> <param> |
tempo <bpm> | clear | random | play | stop | note <key> | off <key> | cc <controller> <value> |
export <name> | save <name>
```

`dump` shows what the window shows **plus what it deliberately hides** — `raw_readback`,
`sequence_patch`, `parked_bases`, the `modulation` cache. That is the surface to reach for when your
instrument sounds wrong under the sequencer and the window looks right.

For an automated behaviour suite against your instrument, copy
`plugins/mxm-mono-01/host-tests/tests/behaviour.rs` from
[mxm-mono-01](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/mxm-mono-01/host-tests/tests/behaviour.rs). Every assertion in it is a property of **rendered
audio** — a frequency, an amplitude, a tail length — measured through the real player rather than by
reading the DSP and agreeing with it.

---

## 6. Gotchas

Ordered by how much time each has actually cost.

### 1. Building the crate does not build the bundle

`cargo build -p <plugin>`, `cargo test`, `cargo clippy` and a standalone harness all compile the
library and all show your change immediately. `target/bundled/<plugin>.clap` is a **separate
artifact**, and a host loads that one. Change the editor, run the standalone, see the change, take a
screenshot — and the plugin a person opens is still the previous build. Reported once as *"it didn't
stick"*, with a screenshot of the old panel taken minutes after a screenshot of the new one.

Run `cargo xtask bundle <plugin> --release` **before** looking at the plugin, and check the `.clap`'s
timestamp when what you see disagrees with what you wrote.

### 2. Nothing may be holding the bundle when you rebundle

`cargo xtask bundle` fails with `Access is denied (os error 5)` — *"Could not remove file before
reflinking"* — if a player still has the `.clap` loaded. **The dangerous part is what happens next:
the previous bundle is still sitting there, so the validator runs and passes against the build you
meant to replace.** Confirm bundling printed `Created a CLAP bundle` before believing a validator
result, and close the player first.

### 3. An unbracketed gesture breaks the player's step editing

Your editor reports **every frame of a drag** to the host. The player uses gesture brackets as the
seam: while one is open the parameter is somebody's to move, and the base is restored on the close.
An editor that sets values without `begin_set_parameter` / `end_set_parameter` gives the player no
close to act on — and the symptom is not "automation is latched", it is *"the knob jumps back to
the original value"* sixty times a second, or a step edit that never commits.

Bracket in exactly one place. `binding::Bound::apply` is that place in `mxm-mono-01`.

### 4. A preset that changes one parameter is recorded as a step lock

The player reads intent from shape: **one parameter arriving from your editor is somebody
sequencing; a whole instrument's worth arriving together is a preset being chosen.** CLAP offers
nothing better — a plugin reports a preset load exactly as it reports a knob turn.

So if your editor has an action that writes a single parameter and means "this is a new patch", and
a step is selected in the player, that write becomes a lock. One unwanted dot, undone by setting the
parameter back to the patch. Prefer to write the full parameter set on a patch change; it is also
what makes Init behave.

### 5. Init must open and close every gesture, and write nothing else

An Init that opens gestures and closes none latches every automation lane in the host. And Init
writes **persisted parameters only** — not sounding notes, not live controller state, and never a
host's sequencer or its per-step data.

### 6. Declare MIDI input, or the player's panic does nothing

The player picks its global recovery from your note ports. A port that accepts MIDI gets **CC 120**;
a CLAP-only note port gets a wildcard `NoteChoke` — which nice-plug's wrapper does not handle. So
for a nice-plug instrument, accepting MIDI is what makes panic actually clear stuck notes.
`MidiConfig::MidiCCs` also gets you pitch bend and CC 1.

### 7. `assert_process_allocs` only fires in debug

A release validator run proves strictly less. Bundle and validate in **both** profiles. Two of the
three vendored nice-plug defect fixes were invisible in release.

### 8. The pointer-facing rules that read as cosmetic and are not

Each of these was a reported defect:

- **Nothing changes size under the pointer.** Allocate geometry from the size tier *before* reading
  hover, focus or drag state. A knob that grows when grabbed is the same defect as a row that
  shifts.
- **A frame that appears on hover reads as movement even when nothing moves.** Give a control a
  resting border and let hover change only the fill. "The interface moved" and "a border appeared
  where there was none" look identical to a person and are different bugs — measure before believing
  either.
- **Never break a word.** egui only *prefers* a word boundary; the guarantee has to come from width.
  `Resonan` / `ce` is the failure. And never justify: `Ui::put` lays out
  `centered_and_justified`, which turned *Pulse width* into `P u l s e` over `width`.
- **A painted control has no accessible name until you call `widget_info`.** `on_hover_text` is a
  tooltip and reaches a pointer and nothing else. UI tests find controls by their accessible label,
  so a painted control without one is both invisible to a screen reader and unfindable by a test.

### 9. Light-theme amber is the classic way to make a warning unreadable

Pure yellow on the light panel measures **1.04 : 1** — not low contrast, *no* contrast. Take
`tokens.warning` from `crates/ui`. Never hard-code a colour in a consumer; if you need a value the
theme does not expose, add the token there rather than the literal here.

### 10. Do not put a `[[bin]]` for a standalone harness inside your plugin crate

nice-plug's own documentation suggests it, and it is wrong here. The `standalone` feature adds
eight crates and declares `mod jack;` unconditionally, so it link-depends on libjack. Cargo unifies
features across a build graph, so a bin in a default-member plugin puts cpal and a libjack link
into the shipped `.clap` — and the bundle still builds, still loads and still works. Silent.
Separate crate under `apps/`, outside `default-members`, plus a `cargo tree` check run by hand.

### 11. Do not pre-generalise the DSP

`crates/<plugin>-dsp` stays per-plugin until a **second** instrument demonstrates a genuinely
shared API — and your instrument is that second one, which makes this the moment the rule is
actually tested. Extract from two honest implementations; do not extract a filter from one because
it seems reusable. `crates/dsp-lab` (in [mxm-tools](https://github.com/mxm-audio/mxm-tools) since the split) is not the loophole: it ships nothing and holds measurement
harnesses for `docs/`, and no shipped DSP may move there.

### 12. Do not guess at nice-plug APIs

Experimental, thinly documented, no migration guide from nih-plug — and most of what is written
online is about nih-plug. Read the **pinned version's source** under `vendor/nice-plug/` (this
workspace redirects through `[patch.crates-io]`), not a git branch head that will not match what you
build. *Since the split:* that is [the nice-plug fork](https://github.com/mxm-audio/nice-plug) at the
tag your root `[patch.crates-io]` names, which cargo checks out under `$CARGO_HOME/git/checkouts/`. The differences that bite: `activate` rather than `initialize`, an associated `type Editor`
rather than a boxed trait object, `nice_export_clap!`, `nice_plug::prelude`.

### 13. A parameter's smoother reads zero until something activates it

nice-plug initialises a parameter's smoother to its value in `activate`, not in `FloatParam::new`.
A unit test that builds the plugin and reads `smoothed.next()` sees zero for every smoothed
parameter — including one whose default is not zero — and a test asserting the patch carries the
default fails for a reason that looks like the plugin's. Call `_internal_update_smoother(rate, true)`
on every `ParamPtr` from `param_map()` first, which is the call the wrapper makes.

### 14. `include_str!` factory presets need files before the generator can compile

The factory set is generated by an `#[ignore]`d test in the crate that includes the files, so the
first build has nothing to include. Create fifty placeholder files, build, run the generator,
and the runs-by-default comparison test then holds them honest. `mxm-poly-06` walked into this.

### 15. The player's tests skip when a bundle is missing

A behaviour suite or golden score in `apps/mxm-player/tests` **skips** when its bundle is missing,
and skipping is not failing. Bundle yours (`cargo xtask bundle`) before running the player's suite,
or your tests never run while reporting green. *Since the split:* those suites are your plugin's
own `plugins/<plugin>/host-tests` (`cargo test -p <plugin>-host-tests`), and they skip the same way.

### 16. Everything runs on Windows, Linux and macOS — all three, always

Not "ought to". Anything platform-specific is `cfg`-gated with **every arm implemented**, never one
arm and a silent nothing elsewhere. A dependency that does not support all three cannot be taken,
whatever else it offers. There is no CI: the development machine is Windows, and Linux and macOS
are unverified — say so rather than implying a change was tried on three platforms.
*Since the split (2026-10-06):* CI tests on all three, but only on a `v*` release tag or when started
by hand. Before that, check Windows and Linux (WSL) yourself; macOS is reached only by CI. Say which
ran rather than implying a change was tried on three platforms.

---

## 7. Constraints you will meet, and where they bite

Not bugs, but they shape what an instrument can do inside the player today.

| Limit | Value | What happens at the edge |
|---|---|---|
| Sequenceable parameters per pattern | 32 distinct | The 33rd is refused with a reason. Why the cap is what it is, and whether it should move, is `apps/mxm-player/AGENTS.md`'s (with its `NOTES.md`, *No maximum length: budgets and storage*) and `sequencer/locks.rs`'s module doc's, in [mxm-player](https://github.com/mxm-audio/mxm-player), to say — read them before designing around it, and do not repeat their reasoning here |
| Total locks per pattern | 32 × 16 | A memory budget, refused separately from the above |
| Steps per bar | 16 at most | Bars are unbounded; `apps/mxm-player/AGENTS.md`'s *Bars* section is the current contract (since the split, mxm-player's [`apps/mxm-player/NOTES.md`](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/NOTES.md#bars-steps--bar--pattern--sequence)) |
| Control map page | 8 slots | nice-plug silently splits a longer page into `"{name} {n}"`, renaming the page the map keys on |
| Control map pages | Append only | nice-plug numbers pages positionally, so inserting a section renumbers every later one |
| Controller resolution | 7-bit | 128 steps; smoothing does the rest |

The parameter cap is worth reading `apps/mxm-player/src/sequencer/locks.rs`'s module doc (in mxm-player) about
before you design around it.

---

## 8. Where an instrument gets tested

| Layer | What it can see | What it is blind to |
|---|---|---|
| `cargo test -p <plugin>-dsp` | The signal path, with no host | Anything about parameters, MIDI or hosting |
| `cargo test -p <plugin>` | Parameters, defaults, presets, identity | The bundle, the host, the sound |
| `clap-validator` | CLAP conformance, and allocations in debug | Whether it sounds right |
| `Session` (player, headless) | Byte-identical rendered audio through the real app | Anything drawn |
| `AppHarness` (player, `egui_kittest`) | The AccessKit tree, `PlayerState`, painted rects | Text layout, perceived colour, audio |
| The CLI over a socket | The live window's state **and** its uncorrected readbacks | Nothing about the host it is not told |
| A real DAW | Host parenting, resize, scale, open/close ordering | — |

Two warnings that come from this project's own history:

- **A locked parameter's reading cannot be observed from a session test.** A requery corrects it to
  the patch by design, and `Session::state()` requeries first — so a session test reports the patch
  whatever the player did. Use `AppHarness` for the host's own bookkeeping, and the CLI's `dump` for
  the uncorrected truth.
- **An oracle that cannot fail is not an oracle.** Tests were written here that passed with and
  without the fix they were meant to guard, and were deleted rather than kept. Remove the fix and
  confirm your test goes red before you believe it.

---

## 9. The copy-paste checklist

```
[ ] docs/briefs/<plugin>.md          — all ten §14 questions answered, before any editor code
[ ] the block order                  — audio path first, then modulation; §14 records any break
[ ] crates/<plugin>-dsp/             — mxm-modulation only, MSRV stated, silence/NaN/bound/reset tests
[ ] plugins/<plugin>/                — Cargo.toml, README.md (LICENSE: the repository root's since the split)
[ ]   plugin_name! macro             — the only literal; NAME and CLAP_ID derive from it
[ ]   CLAP_ID                        — dk.mxm.<plugin>, permanent from the first release
[ ]   crate-type                     — ["cdylib", "lib"], standalone feature NOT enabled
[ ]   params.rs                      — permanent #[id]s, linear gain, smoothing on signals only
[ ]   init patch                     — amounts zero, configurations useful, == CLAP defaults
[ ]   preset.rs                      — impl mxm_preset::Instrument + the factory set; nothing copied
[ ]   Init generated, never a file   — built from the defaults, so it cannot be deleted or drift
[ ]   factory set                    — ≥50 sounds, designed in FACTORY_DESIGN, generated to JSON
[ ]   preset browser                 — app bar patch slots: load, step, star, save/rename/delete
[ ]   editor                         — panel not window, mxm-ui widgets, one gesture bracket point
[ ]   app bar controls               — shell::zoom_control (75–200%, chosen not derived), shell::editor_theme_control
[ ]   telemetry                      — atomics only, once per block, peak reset-on-read, clip latches
[ ]   developer channel              — MXM_DEV_CC gate, CC 119 category / 127 Parameters, CC 118 expander, CC 117 browser, CC 116 theme, the gate test
[ ]   pages of cards, opening size   — paging::editor: stable keys, categories, floors, ceilings, groups; opening size hugged to the budget, expanders open
[ ]   keyboard cursor                — navigation::State, navigation::paged, navigation::at on every control; the keyboard coverage check
[ ]   modulation routing             — mxm-modulation sources/targets, mxm-modulation-params pairs, the machine's wiring as init routes
[ ] plugins/<plugin>/control-map.json— declared roles only, permanent ids, ≤8 per page
[ ] bundler.toml                     — crate name -> display bundle name
[ ] Cargo.toml workspace members     — added; default-members considered
[ ] Root AGENTS.md MSRV table        — both crates
[ ] cargo xtask bundle --release + debug, both validated, 0 failed in each
[ ] plugins/<plugin>/host-tests       — behaviour and a golden score, bundled before the suite runs, or it skips
                                       (in the monorepo, apps/mxm-player/tests/<plugin>_behaviour.rs)
[ ] cargo run -p mxm-player          — in mxm-player: loads, plays, sequences, editor opens and reopens
[ ] A real DAW at a small buffer     — or say plainly that this gate is unmet
[ ] Design system §15 QA gate        — manual, and real
[ ] .github/workflows/ci.yml         — copied from a product repository, with this plugin's name
[ ] DOX pass                         — plugins/AGENTS.md, plugin-conventions.md's plugin list (docs/AGENTS.md in the monorepo), every affected index
```

---

## 10. Where each rule actually lives

| Looking for | Read |
|---|---|
| Naming, licensing, MSRV policy, cross-platform requirement, dependency pinning | The monorepo's root `AGENTS.md`; since the split, [`collection-rules.md`](collection-rules.md) for naming and licensing, and the repository's root `AGENTS.md` for the rest |
| nice-plug conventions, permanent ids, parameters, init patch, presets, `process()` rules, editor contract, the add-a-plugin steps | [`plugin-conventions.md`](plugin-conventions.md), contracted by each repository's `plugins/AGENTS.md` |
| DSP rules, numeric contracts, denormals, what the tests must keep asserting | [`crates/mxm-mono-01-dsp/AGENTS.md`](https://github.com/mxm-audio/mxm-mono-01/blob/main/crates/mxm-mono-01-dsp/AGENTS.md) |
| Widgets, tokens, typography, control idioms, layout traps | [`crates/ui/AGENTS.md`](../crates/ui/AGENTS.md) · [`MXM_DESIGN_SYSTEM.md`](MXM_DESIGN_SYSTEM.md) |
| Roles, fixed knobs, pages, reserved CCs | [`MXM_CONTROL_MAP.md`](MXM_CONTROL_MAP.md) |
| The host: envelope, threading, sequencer, locks, CLI, export, editor hosting | [`apps/mxm-player/AGENTS.md`](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/AGENTS.md) |
| Bundling and control-map staging | [`crates/mxm-xtask/AGENTS.md`](../crates/mxm-xtask/AGENTS.md), which each repository's `xtask/` calls (in the monorepo, the root `xtask/AGENTS.md`) |
| Why nice-plug is vendored (since the split, forked) and what the patches do | [The fork's `PATCHES.md`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (in the monorepo, `vendor/AGENTS.md`) |
| Upstream bugs already diagnosed, with verdicts | [`known-issues.md`](known-issues.md) |
| Filter theory · oscillators · modulation | [`filters/`](filters/README.md) · [`oscillators/`](oscillators/README.md) · [`modulation/`](modulation/README.md) |
