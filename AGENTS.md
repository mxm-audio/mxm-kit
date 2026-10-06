# AGENTS.md — mxm-kit

DOX rail for this repository. Project-wide instructions, durable workflow rules, and the
top-level Child DOX Index.

---

# DOX framework

- DOX is a highly performant AGENTS.md hierarchy installed here
- Agents must follow DOX instructions across any edits

## Core Contract

- AGENTS.md files are binding work contracts for their subtrees
- Work products, source materials, instructions, records, assets, and durable docs must stay
  understandable from the nearest applicable AGENTS.md plus every parent AGENTS.md above it

## Read Before Editing

1. Read the root AGENTS.md
2. Identify every file or folder you expect to touch
3. Walk from the repository root to each target path
4. Read every AGENTS.md found along each route
5. If a parent AGENTS.md lists a child AGENTS.md whose scope contains the path, read that child and
   continue from there
6. Use the nearest AGENTS.md as the local contract and parent docs for repo-wide rules
7. If docs conflict, the closer doc controls local work details, but no child doc may weaken DOX

Do not rely on memory. Re-read the applicable DOX chain in the current session before editing.

## Update After Editing

Every meaningful change requires a DOX pass before the task is done.

Update the closest owning AGENTS.md when a change affects:

- purpose, scope, ownership, or responsibilities
- durable structure, contracts, workflows, or operating rules
- required inputs, outputs, permissions, constraints, side effects, or artifacts
- user preferences about behavior, communication, process, organization, or quality
- AGENTS.md creation, deletion, move, rename, or index contents

Update parent docs when parent-level structure, ownership, workflow, or child index changes. Update
child docs when parent changes alter local rules. Remove stale or contradictory text immediately.
Small edits that do not change behavior or contracts may leave docs unchanged, but the DOX pass
still must happen.

## Hierarchy

- Root AGENTS.md is the DOX rail
- Child AGENTS.md files own domain-specific instructions and their own Child DOX Index
- Each parent explains what its direct children cover and what stays owned by the parent
- The closer a doc is to the work, the more specific and practical it must be

## Child Doc Shape

- Create a child AGENTS.md when a folder becomes a durable boundary with its own purpose, rules,
  responsibilities, workflow, materials, or quality standards
- Work Guidance must reflect current project standards or user instructions; leave it empty if
  there are none yet
- Verification must reflect an existing check; leave it empty until one exists

Default section order: Purpose · Ownership · Local Contracts · Work Guidance · Verification ·
Child DOX Index

## Style

- Keep docs concise, current, and operational
- Document stable contracts, not diary entries
- **A value the code holds is named, not copied** (the owner, 2026-09-24: *"Why are you writing the
  opening pages size in such detail. Is that not already described in the code?"*). A size a test
  derives — an opening size, a minimum, a card floor — or a constant the code declares is stated in
  DOX as its rule, its constant and the test that holds it, never its number: a copied number goes
  stale the day the code moves. Two exceptions: the design system states its own tokens and
  rules, because it is the normative source the code implements; and a plan's revision history
  records what was measured when — a dated record, not the current value
- Put broad rules in parent docs and concrete details in child docs
- Prefer direct bullets with explicit names
- Do not duplicate rules across many files unless each scope needs a local version
- Delete stale notes instead of explaining history
- Trim obvious statements, repeated rules, misplaced detail, and warnings for risks that no longer
  exist

## Closeout

1. Re-check changed paths against the DOX chain
2. Update nearest owning docs and any affected parents or children
3. Refresh every affected Child DOX Index
4. Remove stale or contradictory text
5. Run existing verification when relevant
6. Report any docs intentionally left unchanged and why

---

---

# Purpose

**The MXM instrument kit**: the MIT-licensed libraries every MXM instrument and effect is built
on, published so anyone can build instruments that look, feel and work the same — and work fully
in newDAWn. The interface foundation and its keyboard navigation (`mxm-ui`), presets
(`mxm-preset`), modulation routing (`mxm-modulation`, `mxm-modulation-params`), tempo sync
(`mxm-tempo`), drum part routing (`mxm-part-routing`), the controller-map standard
(`mxm-control-map`), audio file reading and writing (`mxm-audio-file`, `mxm-audio-file-decode`), the
measurement rulers (`mxm-measure`), the checks every plugin's tests share (`mxm-plugin-test`) and
the bundling every repository's `xtask` shares (`mxm-xtask`). Beside them: the normative design
system and control map, and the collection's filter, oscillator and modulation theory, in `docs/`.

The MXM products — the instruments and effects, MXM Player, newDAWn — are GPL-3.0, each in its own
repository under [github.com/mxm-audio](https://github.com/mxm-audio), and take this kit as a git
dependency at a tag. Until 2026-10 all of it was one repository (`mxm-collection`); references
to `plans/` name its design history, which stays in a private archive.

Reference-quality open source: clarity beats cleverness, and every nontrivial algorithm names the
technique or paper it comes from.

# Ownership

Root owns `Cargo.toml`, `Cargo.lock`, `LICENSE`, `README.md`, `CONTRIBUTING.md` and `.github/`.
Each crate owns its folder and its `AGENTS.md`; `docs/` is owned by
[`docs/AGENTS.md`](docs/AGENTS.md).

**DOX goes only into folders this project authors** — never into `target/`, fetched dependency
source, or tool state.

**Dependencies are pinned exactly and `Cargo.lock` is committed.** nice-plug is patched to
[mxm-audio/nice-plug](https://github.com/mxm-audio/nice-plug) — 0.3.0 as published plus the
fixes in its `PATCHES.md` — which every repository that builds a plugin patches the same way.

## The crates

**Ten crate rules, deliberately different** — count the bullets before editing this number:

- `crates/ui` is **shared from day one**. Design system §13 exempts the shell, theme tokens,
  typography and basic parameter controls from the usual wait-for-a-second-consumer rule — they
  define the collection. Anything *beyond* those still waits until two instruments need it.
- `crates/mxm-preset` is shared on evidence from five matching instrument implementations. A plugin
  supplies `Instrument` — its id, parameters, identity slot and factory files — and nothing else.
  **It also holds the collection's one parameter binding** (`binding`, 2026-09-24), extracted from
  eighteen per-plugin copies under the owner's standardisation principle. The rule remains: extract
  on evidence from honest copies, not ahead of it.
- `crates/mxm-modulation` is shared on evidence from three instrument source frames, and **every
  instrument uses it** (the owner, 2026-09-15: *it is why it was made*) — all eleven instruments do,
  `plans/plan-modulation-routing.md` M5 delivered; `mxm-model-drums` for the standard's source and
  amplitude laws, its four route slots a drum a recorded deviation from the full grid. It owns *when a value is readable*, *which routes
  are live* and, since the modulation standard (`plans/plan-modulation-standard.md`), **what a
  performance source means** and what an added route reaches; an instrument keeps its machine's own
  values and *how they are applied*. Its bounded publication contract is load-bearing for user-created feedback.
  Zero dependencies keep it from raising any DSP crate's MSRV.
- `crates/mxm-tempo` is shared on evidence from **four copies of tempo sync** — mxm-mono-00,
  mxm-drum-machine, mxm-bucket-delay and mxm-fx-delay each carried a division table (three of them
  slices of one), the same rounding and the same reach arithmetic — and the owner's *"1 common way it
  works"* (2026-09-25). It owns musical time only: the division ladder, a control's position → its
  division, the reach clamp and the tempo in force; never a plugin's range, smoothing or transition
  law. Zero dependencies and MSRV 1.87.
- `crates/mxm-part-routing` is an **owner-approved exception to evidence-first extraction**
  (2026-09-18). It is created with `mxm-drum-machine`'s assignable-output work because at least one
  further drum machine with the same multi-part architecture is planned. It owns policy-neutral
  fixed-note/channel and main/auxiliary assignment values, claimed-channel matching, fixed-capacity
  same-offset arbitration, CLAP-style owner matching and bounded click-free destination transfer;
  consumers select musical policies and keep parameter IDs and CLAP layouts. Zero dependencies and
  MSRV 1.87 keep it usable by DSP crates.
- `crates/mxm-audio-file` and `crates/mxm-audio-file-decode` are shared on evidence from **copies of
  the same import and export**: near-identical hound WAV decoders in the sampler and the convolution
  plugin, two player writers, and the hand-written RIFF writers the harnesses carried. They own bytes ↔
  samples and nothing a consumer decides — length limits, channel choice, quantisation, state.
  **Two crates, not a feature**: the decoder holds MPL-2.0 symphonia, and a crate outside a package's
  graph cannot be linked into it under any build grouping, so an export-only consumer never carries
  it. See [`crates/mxm-audio-file/AGENTS.md`](crates/mxm-audio-file/AGENTS.md) and
  [`crates/mxm-audio-file-decode/AGENTS.md`](crates/mxm-audio-file-decode/AGENTS.md).
- `crates/mxm-control-map` is **the control-map standard and its schema**, moved out of MXM Player
  because the player is not its only reader: every plugin's test holds its map to it, and a DAW
  will read it. It owns the data and how a file is read and validated, never how a host applies a
  map. See [`crates/mxm-control-map/AGENTS.md`](crates/mxm-control-map/AGENTS.md).
- **The unshipped measurement crates** — `crates/dsp-lab`, `crates/mxm-measure` and
  `crates/mxm-listening` — are **not shared-DSP crates, and are not an exception to the rule above.**
  No shipped DSP may move into any of them, and none may appear in any shipped graph;
  `crates/mxm-measure/AGENTS.md`'s verification section checks the second of those rather than
  asserting it. What may move there is measurement code that was never plugin-specific in the first
  place, and the three split the job:
  `dsp-lab` holds the **harnesses** behind `docs/oscillators/` and `docs/modulation/` — `osc_spike`
  measures nine techniques mxm-mono-01 does not ship — and keeps the measurement *policy* inside
  them, because deciding which bins count as wanted *is* the experiment. `mxm-measure` holds the
  **rulers**: a named computation with a stated unit, and never a threshold, a verdict or a
  normalisation law. `mxm-listening` holds the **interpretation**: the onset and level-match policy,
  the analysis windows, thresholds, verdicts and the owner's vocabulary — what a sound does and how
  two differ, so an agent can hear what the owner hears (`plans/plan-mxm-listening.md`).
  `mxm-measure` is a `[dev-dependencies]` entry everywhere but `dsp-lab` and `mxm-listening`, which
  take it normally because neither has a shipped graph to protect; `mxm-listening` is itself a
  dev-dependency wherever it is used, except the unshipped `apps/mxm-listener-hud`, which takes it
  normally for the same reason. **Shipping that app is a decision the owner deferred**, and the leak
  check forces it: adding the app to the check's list of shipped packages makes the check fail. A
  harness that *verifies one plugin* —
  `mono_01_filter_spike`, `mono_01_render_demo`, `resonance_gain` — stays with that plugin's own
  crate. See
  [`crates/dsp-lab/AGENTS.md`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/dsp-lab/AGENTS.md),
  [`crates/mxm-measure/AGENTS.md`](crates/mxm-measure/AGENTS.md) and
  [`crates/mxm-listening/AGENTS.md`](https://github.com/mxm-audio/mxm-tools/blob/main/crates/mxm-listening/AGENTS.md).
- `crates/mxm-xtask` is **the build tooling every repository's `xtask` shares** — `nice_plug_xtask`'s
  bundler and the control-map staging — so each product repository bundles the same way. The root
  `xtask` passes its workspace root and keeps only `fixtures`. See
  [`crates/mxm-xtask/AGENTS.md`](crates/mxm-xtask/AGENTS.md).
- `crates/mxm-plugin-test` is **the checks every plugin's tests share** — keyboard coverage, paging,
  the opening size, layout-tree cards, route parameters, time readings, the bundle's name, the words
  a player reads — and neither shared DSP nor
  shared interface. It was five files each plugin `include!`d, made a crate so a plugin can leave the
  repository with its tests (`plans/plan-repo-split.md`). A `[dev-dependencies]` entry only, held by
  the same leak check as the measurement crates. See
  [`crates/mxm-plugin-test/AGENTS.md`](crates/mxm-plugin-test/AGENTS.md).

## MSRV is per crate, not per workspace

`egui` 0.36.1 declares `rust-version = "1.95"`, so anything with a GUI needs 1.95. Framework-free
crates stay lower and say so explicitly rather than inheriting.

**Every floor in this table has been compiled against, and a floor that has not is a guess.** Cargo
enforces `rust-version` when it resolves *dependencies*, never when it compiles your own crate, so on
a current toolchain a wrong number is silent — and two of these were wrong for months. Checking one
is a single command per crate:

```bash
rustup toolchain install 1.87.0
cargo +1.87.0 build -p <crate>          # and `test`, which is the harder floor
```

Run it whenever a crate takes a language feature it did not have before. The two that moved to 1.88
did so because `let` chains stabilised there, which is the shape this will usually take: syntax
arrives, the compiler in use accepts it, and the declared floor quietly stops being true.

| Crate | MSRV | Why |
|---|---|---|
| `crates/mxm-modulation` | **1.87** | Zero dependencies, so every DSP crate can take it without inheriting the GUI floor |
| `crates/mxm-tempo` | **1.87** | Zero dependencies, so a plugin's audio half and any DSP crate can take it without inheriting the GUI floor. **Verified on 1.87**, library and tests |
| `crates/mxm-modulation-params` | **1.95** | nice-plug and egui: the parameter and interface half, which is why it is a second crate rather than a feature |
| `crates/mxm-control-map` | **1.95** | Inherited, not lowered: serde alone would allow less, but every consumer (the player, the plugins' tests) is at the GUI floor and none needs lower |
| `crates/mxm-part-routing` | **1.87** | Zero dependencies; framework-free part assignments, claimed-channel matching, fixed-capacity arbitration, owner matching and bounded destination transfer. **Verified on 1.87**, library and tests |
| `crates/mxm-measure` | **1.87** | Zero dependencies, so every crate that measures sound can dev-depend on it without inheriting the GUI floor. **Verified on 1.87**, library and tests |
| `crates/mxm-xtask` | **1.95** | Build tooling, inherited: it runs on the developer's toolchain, never in a plugin |
| `crates/mxm-plugin-test` | **1.95** | Every check paints a real panel through egui; a dev-dependency of the plugins, which are at this floor already |
| `crates/mxm-audio-file` | **1.87** | hound, aifc and flacenc build there, so a DSP crate's harness can dev-depend on it without inheriting the GUI floor. **Verified on 1.87**, library and tests — the run caught a `let` chain stable had accepted |
| `crates/mxm-audio-file-decode` | **1.87** | symphonia declares 1.85. **Verified on 1.87**, library, unit and fixture tests |
| `crates/ui` | **1.95** | egui |
| `crates/mxm-preset` | **1.95** | egui, for the app-bar controls; the format and the library alone would stand at 1.87 |

## Licensing

**MIT** for everything here (`LICENSE`), so the kit can be used by any instrument, open or closed.

- **MPL-2.0 is accepted** for symphonia, in `crates/mxm-audio-file-decode` only, used unmodified.
  It is file-level copyleft: the notice is compiled into that crate and travels with every binary
  that links it. **Never vendor, patch or modify an MPL crate.**
- The Inter fonts in `crates/ui/assets/inter/` are under the SIL Open Font License
  (`LICENSE.txt` beside them).
- nice-plug is ISC; its fork carries the upstream licence.
- Check the licence before porting any algorithm, and record source and licence in a comment at
  the top of the file. Cite techniques (TPT/ZDF, PolyBLEP) even when the implementation is original.
- The MIT licence covers code, not the MXM name (see the products' `TRADEMARKS.md`).

## Research citations

A citation written `` `research:<path>` `` names a page in MXM's private research repository. It
is plain text in a code span, never a link, and nothing here depends on that repository at build
or test time. Facts, numbers, our own measurements and short quotations cross into this
repository; third-party files, images and verbatim text never do.

## Windows, Linux and macOS — all three, always

Everything here runs on all three. Anything platform-specific is `cfg`-gated with every arm
implemented. A dependency that does not support all three cannot be taken.

# Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same on Windows, macOS and Linux. Each crate's own `AGENTS.md` names its narrower
checks; the check that no test-only crate (`mxm-measure`, `mxm-plugin-test`) reaches a shipped
graph runs in the product repositories, over their own shipped packages.

# Child DOX Index

| Doc | Scope |
|---|---|
| [`crates/mxm-modulation/AGENTS.md`](crates/mxm-modulation/AGENTS.md) | Shared source frames, route compaction, combination laws and bounded publication |
| [`crates/mxm-tempo/AGENTS.md`](crates/mxm-tempo/AGENTS.md) | Tempo sync: the one sixteen-step division ladder, a control's span and direction, position → division, the clamp-never-rescale reach law, and the tempo in force |
| [`crates/mxm-part-routing/AGENTS.md`](crates/mxm-part-routing/AGENTS.md) | Shared part assignments, claimed-channel matching, fixed-capacity arbitration, CLAP-style note-owner matching and bounded destination transfer |
| [`crates/mxm-modulation-params/AGENTS.md`](crates/mxm-modulation-params/AGENTS.md) | The routing's parameter and interface half: why it is a second crate rather than a feature, removal as one parameter write with the amount left alone, rows derived from values rather than editor state, and the source as a label because a row *is* its source |
| [`crates/mxm-measure/AGENTS.md`](crates/mxm-measure/AGENTS.md) | The measurement rulers every test and harness shares: a named computation and never a verdict, the dev-dependency-only rule and its check, the result forms, the extraction gate with its **declined register**, and the closed-form controls that keep a shared ruler honest |
| [`crates/mxm-control-map/AGENTS.md`](crates/mxm-control-map/AGENTS.md) | The control-map standard (`control-map.json`) and its schema — layout, instrument maps, parsing, validation and the parameter id hash — read by MXM Player and the plugins' tests |
| [`crates/mxm-plugin-test/AGENTS.md`](crates/mxm-plugin-test/AGENTS.md) | The checks every plugin's tests share — keyboard coverage, paging, the opening size, layout-tree cards, route parameters, time readings, the bundle's name in `bundler.toml` and the words a player reads (with their word lists) — one copy for all, dev-dependency only |
| [`crates/mxm-xtask/AGENTS.md`](crates/mxm-xtask/AGENTS.md) | The build tooling every repository's `xtask` shares: bundling through `nice_plug_xtask`, control-map staging, and the outermost-workspace trap |
| [`crates/mxm-audio-file/AGENTS.md`](crates/mxm-audio-file/AGENTS.md) | Writing WAV, AIFF and FLAC through established encoders: the formats and their limits, no MPL code, quantisation as a published definition equal to `mxm-measure`'s at 16 bits, refusal of non-finite samples, reported clipping, atomic writes, the 32-bit WAV size bound, the `acid` exception, and the flacenc STREAMINFO correction |
| [`crates/mxm-audio-file-decode/AGENTS.md`](crates/mxm-audio-file-decode/AGENTS.md) | Reading audio through symphonia (MPL-2.0): the formats it reads, a second crate rather than a feature, the compiled-in notice, Great-or-Excellent components only, what is bounded and what is not, what is refused and how skipped audio is found, per-file gapless reporting, and fixtures that are our own work |
| [`crates/ui/AGENTS.md`](crates/ui/AGENTS.md) | The shared interface foundation and its design-system contract |
| [`crates/mxm-preset/AGENTS.md`](crates/mxm-preset/AGENTS.md) | Shared preset format, library, browser, identity, authored-model Init and durable-content transaction |
| [`docs/AGENTS.md`](docs/AGENTS.md) | The normative design system and control map, the measured `mxm-measure` page, and the filter, oscillator and modulation theory |
