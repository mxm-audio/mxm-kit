# AGENTS.md — mxm-kit

DOX rail for this repository. Project-wide instructions, durable workflow rules, and the
top-level Child DOX Index. The detail behind them is in [NOTES.md](NOTES.md).

# Purpose

**The MXM instrument kit**: the MIT-licensed libraries every MXM instrument and effect is built
on, published so anyone can build instruments that look, feel and work the same — and work fully
in newDAWn. The crates are below; `docs/` holds the normative design system and control map, the
conventions every MXM plugin keeps ([`docs/plugin-conventions.md`](docs/plugin-conventions.md)), and the
collection's filter, oscillator and modulation theory ([NOTES.md § What the kit holds](NOTES.md#what-the-kit-holds)).

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

The evidence each was extracted on: [NOTES.md § The crate rules in full](NOTES.md#the-crate-rules-in-full).
**Ten crate rules, deliberately different** — count the bullets before editing this number:

- `crates/ui` is **shared from day one**: design system §13 exempts the shell, theme tokens,
  typography and basic parameter controls; anything *beyond* them waits for two instruments.
- `crates/mxm-preset` is shared on evidence from five matching implementations; a plugin supplies
  `Instrument` and nothing else. It also holds the collection's one parameter `binding`. **Extract on
  evidence from honest copies, not ahead of it.**
- `crates/mxm-modulation`: **every instrument uses it** (the owner, 2026-09-15). It owns *when a value
  is readable*, *which routes are live* and **what a performance source means**; an instrument keeps
  its machine's values and *how they are applied*. Its bounded publication is load-bearing, and it
  has zero dependencies.
- `crates/mxm-tempo` owns musical time only — the division ladder, a control's position → its
  division, the reach clamp and the tempo in force — never a plugin's range, smoothing or transition
  law. Zero dependencies and MSRV 1.87.
- `crates/mxm-part-routing` is an **owner-approved exception to evidence-first extraction**
  (2026-09-18): policy-neutral part assignment mechanics; consumers keep musical policy, parameter IDs
  and CLAP layouts. Zero dependencies and MSRV 1.87.
- `crates/mxm-audio-file` and `crates/mxm-audio-file-decode` own bytes ↔ samples and nothing a
  consumer decides. **Two crates, not a feature**: the decoder holds MPL-2.0 symphonia, so an
  export-only consumer never carries it.
- `crates/mxm-control-map` is **the control-map standard and its schema**: the data and how a file is
  read and validated, never how a host applies a map.
- **The unshipped measurement crates** — `crates/mxm-measure` here, `dsp-lab` and `mxm-listening` in
  `mxm-tools` — are **not shared DSP and not an exception**: no shipped DSP moves in and none appears
  in a shipped graph (`crates/mxm-measure/AGENTS.md`'s Verification). `dsp-lab` holds harnesses and
  their policy, `mxm-measure` rulers (never a threshold or verdict), `mxm-listening` interpretation.
  A harness that verifies one plugin stays with it. Shipping mxm-tools' `apps/mxm-listener-hud` is
  deferred.
- `crates/mxm-xtask` is **the build tooling every repository's `xtask` shares** — bundling,
  control-map staging and `fetch`; mxm-player's root `xtask` adds only `fixtures`.
- `crates/mxm-plugin-test` is **the checks every plugin's tests share** — neither shared DSP nor
  shared interface; a `[dev-dependencies]` entry only, held by the same leak check.

# Local Contracts

## DOX

The full wording: [NOTES.md § The DOX framework in full](NOTES.md#the-dox-framework-in-full).

- **AGENTS.md files are binding work contracts for their subtrees.** Work products, sources,
  instructions, records, assets and durable docs stay understandable from the nearest AGENTS.md plus
  every parent above it.
- **Before editing**, re-read in this session, not from memory, the root AGENTS.md and every
  AGENTS.md on the route from the root to each path you will touch, following a parent's index into
  the child whose scope holds it. The nearest is the local contract; a closer doc controls local
  detail, but no child may weaken DOX.
- **Every meaningful change gets a DOX pass.** Update the closest owning AGENTS.md when purpose,
  scope, ownership, structure, contracts, workflows, inputs, outputs, permissions, constraints, side
  effects, artifacts, the owner's preferences, or AGENTS.md files and indexes change, and the parents
  and children it affects; correct stale or contradictory text at once, moving its history to `NOTES.md` rather than deleting it. A small edit that changes no
  behaviour may leave docs unchanged, but the pass still happens.
- **Hierarchy**: the root is the rail; a child owns its domain and its own Child DOX Index; a parent
  says what its children cover and what it keeps; the closer to the work, the more concrete.
- **A child AGENTS.md** is made when a folder becomes a durable boundary. Sections, in order:
  Purpose · Ownership · Local Contracts · Work Guidance · Verification · Child DOX Index. Work
  Guidance and Verification stay empty until a current standard or an existing check fills them.
- **An AGENTS.md is the contract and stays under about 200 lines.** History, measurements, rationale
  and worked examples go in the `NOTES.md` beside it, linked from the rule they explain.

## MSRV is per crate, not per workspace

`egui` 0.36.1 declares `rust-version = "1.95"`, so anything with a GUI needs 1.95. Framework-free
crates stay lower and say so explicitly rather than inheriting.

**Every floor in this table has been compiled against**: Cargo never checks your own crate's
`rust-version`, so verify a floor with the toolchain itself whenever a crate takes a language feature
it did not have before ([NOTES.md § Why](NOTES.md#why-every-msrv-floor-is-compiled-against)):

```bash
rustup toolchain install 1.87.0
cargo +1.87.0 build -p <crate>          # and `test`, which is the harder floor
```

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

# Work Guidance

- **Style**: concise, current, operational; stable contracts, not diary entries; broad rules in
  parents, concrete detail in children; direct bullets with explicit names; no rule repeated across
  files unless each scope needs it; stale, obvious and misplaced text deleted, not explained.
- **A value the code holds is named, not copied** (the owner, 2026-09-24): a size a test derives or a
  constant the code declares is stated as its rule, its constant and the test that holds it, never
  its number. Two exceptions: the design system's own tokens and rules, and a plan's dated revision
  history.
- **Closeout**: re-check changed paths against the DOX chain; update the nearest owning docs and any
  affected parents or children; refresh every affected Child DOX Index; correct (never just delete) stale or contradictory
  text; run existing verification when relevant; report any docs intentionally left unchanged and why.

# Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same on Windows, macOS and Linux, but only on a `v*` release tag or when started by hand
(the owner, 2026-10-06); before a push, run it on Windows and on Linux (WSL). Each crate's own `AGENTS.md` names its narrower
checks; the check that no test-only crate (`mxm-measure`, `mxm-plugin-test`) reaches a shipped
graph runs in the product repositories, over their own shipped packages.

# Child DOX Index

| Doc | Scope |
|---|---|
| [`crates/mxm-modulation/AGENTS.md`](crates/mxm-modulation/AGENTS.md) | Shared source frames, route compaction, combination laws and bounded publication |
| [`crates/mxm-tempo/AGENTS.md`](crates/mxm-tempo/AGENTS.md) | Tempo sync: the one sixteen-step division ladder, a control's span and direction, position → division, the clamp-never-rescale reach law, and the tempo in force |
| [`crates/mxm-part-routing/AGENTS.md`](crates/mxm-part-routing/AGENTS.md) | Shared part assignments, claimed-channel matching, fixed-capacity arbitration, CLAP-style note-owner matching and bounded destination transfer |
| [`crates/mxm-modulation-params/AGENTS.md`](crates/mxm-modulation-params/AGENTS.md) | The routing's parameter and interface half: why it is a second crate rather than a feature, removal as one parameter write with the amount left alone, rows derived from values rather than editor state, and the source as a label because a row *is* its source |
| [`crates/mxm-measure/AGENTS.md`](crates/mxm-measure/AGENTS.md) | The measurement rulers every test and harness shares: a named computation and never a verdict, the dev-dependency-only rule and its check, the result forms, the extraction gate (its **declined register** in that folder's `NOTES.md`), and the closed-form controls that keep a shared ruler honest |
| [`crates/mxm-control-map/AGENTS.md`](crates/mxm-control-map/AGENTS.md) | The control-map standard (`control-map.json`) and its schema — layout, instrument maps, parsing, validation and the parameter id hash — read by MXM Player and the plugins' tests |
| [`crates/mxm-plugin-test/AGENTS.md`](crates/mxm-plugin-test/AGENTS.md) | The checks every plugin's tests share — keyboard coverage, paging, the opening size, layout-tree cards, route parameters, time readings, the bundle's name in `bundler.toml` and the words a player reads (with their word lists) — one copy for all, dev-dependency only |
| [`crates/mxm-xtask/AGENTS.md`](crates/mxm-xtask/AGENTS.md) | The build tooling every repository's `xtask` shares: bundling through `nice_plug_xtask`, control-map staging, and the outermost-workspace trap |
| [`crates/mxm-audio-file/AGENTS.md`](crates/mxm-audio-file/AGENTS.md) | Writing WAV, AIFF and FLAC through established encoders: the formats and their limits, no MPL code, quantisation as a published definition equal to `mxm-measure`'s at 16 bits, refusal of non-finite samples, reported clipping, atomic writes, the 32-bit WAV size bound, the `acid` exception, and the flacenc STREAMINFO correction |
| [`crates/mxm-audio-file-decode/AGENTS.md`](crates/mxm-audio-file-decode/AGENTS.md) | Reading audio through symphonia (MPL-2.0): the formats it reads, a second crate rather than a feature, the compiled-in notice, Great-or-Excellent components only, what is bounded and what is not, what is refused and how skipped audio is found, per-file gapless reporting, and fixtures that are our own work |
| [`crates/ui/AGENTS.md`](crates/ui/AGENTS.md) | The shared interface foundation and its design-system contract |
| [`crates/mxm-preset/AGENTS.md`](crates/mxm-preset/AGENTS.md) | Shared preset format, library, browser, identity, authored-model Init and durable-content transaction |
| [`docs/AGENTS.md`](docs/AGENTS.md) | The normative design system and control map, the plugin conventions, the measured `mxm-measure` page, and the filter, oscillator and modulation theory |
