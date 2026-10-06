# AGENTS.md — docs/

Parent: [`../AGENTS.md`](../AGENTS.md)

# Purpose

The kit's durable documentation: the normative interface system every MXM editor follows, the
control-map contract, and the theory references behind the DSP. Per-instrument design briefs live
in each product's repository; collection-wide process notes in the workspace repository. The machine,
effect and family research the copies are built from lives in the private research repository
(root *Research citations*; in full, [`collection-rules.md`](collection-rules.md#research-boundary)) and is cited from here as `research:<path>`.

Durable, not diary. Implementation plans and their review cycles stay in MXM's private archive.

# Ownership

| Path | What it is | Status |
|---|---|---|
| `MXM_DESIGN_SYSTEM.md` | The collection's interface system, §1–§15 | **Normative** |
| `MXM_CONTROL_MAP.md` | One controller across the collection: roles, fixed knobs, pages, reserved CCs | **Normative** |
| `collection-rules.md` | The rules every MXM product keeps beyond the plugin conventions: what a copy owes its machine, naming, licensing and the research boundary; what product repositories cite as "root *Naming*" and the like | **Normative** |
| `plugin-conventions.md` | The conventions every MXM plugin keeps, with their reasons, measurements and history; each plugin repository's `plugins/AGENTS.md` is the short contract that links here | **Normative** for plugins |
| `adding-an-instrument.md` | The ordered route from an idea to a plugin that loads, sequences and passes the gates, with the traps; owns no rules | Guide |
| `code-review-notes.md` | Recurring plugin failure patterns: prompts for reviewing, repairing or releasing a plugin | Guide |
| `known-issues.md` | Upstream defects and integration failures, and what our own code can do about each | Living |
| `mxm-measure.html` | What `crates/mxm-measure` offers, as figures: every number and series is measured by `crates/mxm-measure/examples/showcase.rs` and embedded | Living; see *A measured page carries its generator* |
| `filters/` | Long-form filter theory, chapters `01`–`09`; the per-family deep-dives are `research:filters/machines/` | See [`filters/AGENTS.md`](filters/AGENTS.md) |
| `oscillators/` | Long-form oscillator theory: general methods and technique deep-dives, measured in-repo; the machine survey and the 208 appendix are `research:oscillators/` | See [`oscillators/AGENTS.md`](oscillators/AGENTS.md) |
| `modulation/` | LFOs, envelopes, smoothing and glide, measured in-repo; the 208 control-source appendix is `research:modulation/` | See [`modulation/AGENTS.md`](modulation/AGENTS.md) |
| *machine, effect and family research* | In the private research repository (`research:instruments/`, `research:effects/`, `research:filters/machines/`), cited as `research:<path>`; never files, images or verbatim text | Root *Research citations*; [`collection-rules.md`](collection-rules.md#research-boundary) |

**Name an influence; do not ship a screenshot of one.** `MXM_DESIGN_SYSTEM.md` §16 may name the
quality being referenced; an image of another product turns a reference into a visual target and is
forbidden.

# Local Contracts

## The design system is normative

`MXM_DESIGN_SYSTEM.md` binds every editor in the collection, including the player's. It is not
advice and a plugin may not opt out. The two rules most easily got wrong:

- **Explicitly non-hardware (§2)** — no fake knobs, switches, LEDs, textures or embossed labels, and
  no layout copied from the panel of the instrument that inspired the DSP.
- **Both themes are first-class** — dark and light get equal design and test coverage, from semantic
  tokens, with contrast *measured*.
- **Source-panel controls stay directly visible (§3.3)** — progressive disclosure is for
  software-added depth, diagnostics and context, never for a sound or performance control the
  original player could reach.
- **Finite parameter selectors use the caret form (§7.4)** — `‹ Off ›` through the shared selector,
  not bare dropdown text or a local menu treatment.

**§4.2's quarter-4K fit budget is collection-wide**, including factory worktrees, with
physical/DPI-aware QA in §15. The shared reference size is only a starting size; per-editor floors —
computed from each card's description, with any declared usability minimum — control resizing. A per-instrument brief owns card inventories and Compact control choices;
page count derives from available space, not a global or per-instrument fixed limit. Updating the rule does not establish compliance of existing editors.

§3.2’s view-bar cells share the widest label’s measured floor and reserve full wrapped height.
Musician pages derive from available space using category-first metadata and stable card identities.
Plugin editors use `crates/ui`’s paging renderer; Parameters remains developer-only.

**Every editor opens at the quarter-4K budget hugged:** lay the real panel out at the budget with
disclosures open, take away the slack every page leaves (not only the first page's), and hold the
result with the per-editor opening-size check. A whole category never costs a page (§3.2).
CC 119 addresses categories, not bar positions. Controller-page permanence below is independent
of GUI navigation; headless fit does not establish native-window or DAW quality.

**§11’s keyboard table is normative:** `Shift`+arrows select modules/cards, `Command`+arrows select
parameters, and bare arrows edit values. The higher key selects the higher level. Left/right remains fine and
up/down coarse, following the Dirtywave M8's axis orientation, and `Alt` is a finer layer of both:
10 % and 1 % of the travel, 1 % and 0.1 % under `Alt` — an octave and a semitone, ten cents and a
cent on a pitch — snapped to the parameter's grid and never less than one of its steps. Direction
follows painted geometry; paging order bridges page
edges; segmented controls are one parameter target; the visible target remains authoritative after
native widget-focus loss; cardless surfaces relinquish the cursor; and a held continuous edit is one
host gesture through release. Cardless developer surfaces have no cursor and retain their own
bare-arrow editing. Every control that edits a parameter joins
the cursor — knob, switch, segmented control and selector alike — and each editor proves its own
coverage, because a control that paints without registering is silently unreachable. Adopting it
was not a licence to change any parameter, page assignment or controller mapping, and none changed.

§7.1 keeps parameter names canonical while permitting omission of a repeated module prefix in a
named card/section; §7.4 permits source sublabels directly below their controls. These are opt-in
presentation choices, not a requirement to rewrite fixed-source instruments.

**§4.2's quarter-4K fit budget is collection-wide**, with physical/DPI-aware QA in §15. A
per-instrument brief may choose fewer pages or Compact controls; those choices are not global
limits. Updating the rule does not establish compliance of existing editors.

Changing it is a collection-wide change: check every consumer, in every product repository, and update
[`crates/ui/AGENTS.md`](../crates/ui/AGENTS.md) in the same pass.

## The control map is normative, and split by owner

`MXM_CONTROL_MAP.md` and `control-map.json` (in `crates/mxm-control-map`) are the **collection standard**: role names, the eight
fixed knobs, the page order. They bind every instrument.

**Which parameters fill those roles is not here.** That ships with the instrument, as
`control-map.json` in the plugin's own crate — see each plugin repository's `plugins/AGENTS.md`
([mxm-mono-01's](https://github.com/mxm-audio/mxm-mono-01/blob/main/plugins/AGENTS.md)) and
[`plugin-conventions.md`](plugin-conventions.md#a-plugin-ships-its-own-control-map).
Nobody is obliged to install the whole collection.

Two rules are load-bearing and easy to break:

- **Until 1.0, roles/pages may be corrected only in one collection-wide pass; at 1.0 roles become
  permanent and pages only append.** A role name is referenced by every instrument map and every
  controller template somebody has built. Page order is positional in CLAP, so any pre-1.0 reorder
  must update the standard, every affected product map and pinned player tests together.
- **A page holds eight slots or fewer**, or nice-plug silently splits and renames it.

Changing either is a collection-wide change: check every instrument's map, in its own repository,
in the same pass.

## A measured page carries its generator

`mxm-measure.html` states figures the crate produced, so it may not be hand-corrected. The JSON in
its `<script id="data">` block comes from `cargo run -p mxm-measure --release --example showcase`;
change the example, re-run it, and replace the block. A number written in the prose or a caption
must also appear in that data. The page is self-contained: no network fonts, scripts or styles, so
it opens from disk.

## Research documents state their evidence

Where a number is measured, say so and give it. Where it is derived or assumed, say that too. Where
sources conflict, record the conflict rather than picking silently. Code in a document is expected
to compile and to have been run.

# Work Guidance

- Prose, not transcripts. If something is only true this week, it belongs in a plan (until the
  split, `plans/`, now in the private archive), not here.
- Research for a machine, effect or family happens in the research repository, with its own skill
  (`research:.claude/skills/mxm-collection-research/SKILL.md`). A chapter here cites the result as
  `research:<path>` and takes facts, numbers and our own measurements only — never a third-party
  file, an image or verbatim text (root *Research citations*; in full,
  [`collection-rules.md`](collection-rules.md#research-boundary)).
- Keep AGENTS.md current: correct stale text and move its history to `NOTES.md` instead of explaining it here.
- Cross-link instead of restating. The design system is the single source for interface rules; DSP
  conventions live in the code's own AGENTS.md chain.

# Verification

No automated checks. Two manual gates, both real:

- **Design system §15 QA gate** — structure, themes and accessibility, interaction, audio safety and
  performance, originality. Run before any editor is called done.
- **Internal links resolve.** After moving or renaming a document:

  ```bash
  cd docs && grep -roh "](\([0-9A-Za-z._/-]*\.md\)" . | sed 's/](//' | sort -u
  ```

  and confirm each path exists relative to its referring file.
- **`research:` citations resolve**, when the research checkout is present — since the split it is
  in the private archive, not a sibling, so set `RESEARCH` to it (in the monorepo it was
  `../01-mxm-collection-research`):

  ```bash
  git grep -h -o -E 'research:[A-Za-z0-9._-][A-Za-z0-9._/-]*' -- . ':!automation/tests' | sed 's|^research:||' | sort -u | while read -r p; do [ -e "$RESEARCH/$p" ] || echo "MISSING $p"; done
  ```

# Child DOX Index

| Doc | Scope |
|---|---|
| [`filters/AGENTS.md`](filters/AGENTS.md) | Filter theory: the reference chapters, their evidence standard, and the obligations the research deep-dives place on them |
| [`oscillators/AGENTS.md`](oscillators/AGENTS.md) | Oscillator theory: antialiasing and waveshapes, per-technique deep dives, and the one program every in-repo measured number comes from |
| [`modulation/AGENTS.md`](modulation/AGENTS.md) | Modulation: generic LFO/envelope/smoothing/glide methods and the ownership line for measurements |

Owned here, no child doc: `MXM_DESIGN_SYSTEM.md`, `MXM_CONTROL_MAP.md`, `mxm-measure.html`.
The standard as data, `control-map.json`, is `crates/mxm-control-map`'s.
