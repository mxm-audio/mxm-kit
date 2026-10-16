# NOTES.md — mxm-kit (repository root)

The detail behind this folder's AGENTS.md: history, measurements, rationale and worked examples.
AGENTS.md is the contract; this file is the reference it links to.

## The DOX framework in full

The wording the root AGENTS.md carried until 2026-10-06; AGENTS.md now states the same rules as
bullets.

- DOX is a highly performant AGENTS.md hierarchy installed here
- Agents must follow DOX instructions across any edits

### Core Contract

- AGENTS.md files are binding work contracts for their subtrees
- Work products, source materials, instructions, records, assets, and durable docs must stay
  understandable from the nearest applicable AGENTS.md plus every parent AGENTS.md above it

### Read Before Editing

1. Read the root AGENTS.md
2. Identify every file or folder you expect to touch
3. Walk from the repository root to each target path
4. Read every AGENTS.md found along each route
5. If a parent AGENTS.md lists a child AGENTS.md whose scope contains the path, read that child and
   continue from there
6. Use the nearest AGENTS.md as the local contract and parent docs for repo-wide rules
7. If docs conflict, the closer doc controls local work details, but no child doc may weaken DOX

Do not rely on memory. Re-read the applicable DOX chain in the current session before editing.

### Update After Editing

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

### Hierarchy

- Root AGENTS.md is the DOX rail
- Child AGENTS.md files own domain-specific instructions and their own Child DOX Index
- Each parent explains what its direct children cover and what stays owned by the parent
- The closer a doc is to the work, the more specific and practical it must be

### Child Doc Shape

- Create a child AGENTS.md when a folder becomes a durable boundary with its own purpose, rules,
  responsibilities, workflow, materials, or quality standards
- Work Guidance must reflect current project standards or user instructions; leave it empty if
  there are none yet
- Verification must reflect an existing check; leave it empty until one exists

Default section order: Purpose · Ownership · Local Contracts · Work Guidance · Verification ·
Child DOX Index

### Style

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

### Closeout

1. Re-check changed paths against the DOX chain
2. Update nearest owning docs and any affected parents or children
3. Refresh every affected Child DOX Index
4. Remove stale or contradictory text
5. Run existing verification when relevant
6. Report any docs intentionally left unchanged and why

### Stale text: what moves and what is deleted

Stale or misplaced text is corrected and its history moved to `NOTES.md`, never just deleted (the
owner, 2026-10-06). **What moves is the reasoning behind what is still in force; a discarded idea is
deleted, not archived** (the owner, 2026-10-08: "it is a discarded idea that we are never gonna use
again"): no copied old tables, no "until <date> it was…" asides.

## What the kit holds

**The MXM instrument kit**: the MIT-licensed libraries every MXM instrument and effect is built
on, published so anyone can build instruments that look, feel and work the same — and work fully
in newDAWn. The interface foundation and its keyboard navigation (`mxm-ui`), presets
(`mxm-preset`), modulation routing (`mxm-modulation`, `mxm-modulation-params`), tempo sync
(`mxm-tempo`), drum part routing (`mxm-part-routing`), the controller-map standard
(`mxm-control-map`), audio file reading and writing (`mxm-audio-file`, `mxm-audio-file-decode`), the
measurement rulers (`mxm-measure`), the checks every plugin's tests share (`mxm-plugin-test`) and
the bundling every repository's `xtask` shares (`mxm-xtask`). Beside them: the normative design
system and control map, and the collection's filter, oscillator and modulation theory, in `docs/`.

*Moved from AGENTS.md (2026-10-07), a status rather than a rule:* shipping mxm-tools'
`apps/mxm-listener-hud` is deferred.

## The crate rules in full

The ten rules as the root AGENTS.md stated them until 2026-10-06, with the evidence each crate was
extracted on.

- `crates/ui` is **shared from day one**. Design system §13 exempts the shell, theme tokens,
  typography and basic parameter controls from the usual wait-for-a-second-consumer rule — they
  define the collection. Anything *beyond* those still waits until two instruments need it.
- `crates/mxm-preset` is shared on evidence from five matching instrument implementations. A plugin
  supplies `Instrument` — its id, parameters, identity slot and factory files — and nothing else.
  The rule remains: extract on evidence from honest copies, not ahead of it.
- `crates/mxm-params` holds the collection's one parameter binding (`binding`, 2026-09-24),
  extracted from eighteen per-plugin copies under the owner's standardisation principle, with
  `ErasedParam` and the step laws. Both were first extracted into `mxm-preset`, the crate that already
  named nice-plug and drew with `mxm-ui`, and moved out because neither is a preset and
  `mxm-modulation-params` depended on the whole preset system for `ErasedParam` alone. Its `text` is
  the formatter groups the plugins carried as identical copies (80 functions, 47 distinct bodies,
  the largest group in fifteen plugins), moved with a before-and-after census as proof.
- `crates/mxm-telemetry` is shared on evidence from every plugin's `telemetry.rs`: the meter's read
  side was one body in nineteen plugins and its write side five policies for a non-finite peak,
  and the developer channel's four request slots were copied into all eleven instruments. Each
  plugin keeps its own telemetry — envelopes, modulators, stage levels — and holds these as
  fields; the policy and its history are in that crate's `NOTES.md`. The architecture audit's O10,
  owner-approved 2026-10-13, made the 19 and 11 copies one.
- `crates/mxm-modulation` is shared on evidence from three instrument source frames, and **every
  instrument uses it** (the owner, 2026-09-15: *it is why it was made*) — all eleven instruments do,
  `plans/plan-modulation-routing.md` (in the private archive) M5 delivered; `mxm-model-drums` for the standard's source and
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
  `crates/mxm-listening` (*since the split*, `dsp-lab`, `mxm-listening` and the HUD below are in
  mxm-tools) — are **not shared-DSP crates, and are not an exception to the rule above.**
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
  bundler, the control-map staging, and `fetch`, which builds other repositories' plugins at pinned
  tags for this repository's tests — so each product repository bundles and tests the same way. The root
  `xtask` passes its workspace root and keeps only `fixtures` (*since the split*, that is mxm-player's;
  every other repository's `xtask` passes its root to the shared tooling and adds nothing). See
  [`crates/mxm-xtask/AGENTS.md`](crates/mxm-xtask/AGENTS.md).
- `crates/mxm-plugin-test` is **the checks every plugin's tests share** — keyboard coverage, paging,
  the opening size, layout-tree cards, route parameters, time readings, the bundle's name, the words
  a player reads — and neither shared DSP nor
  shared interface. It was five files each plugin `include!`d, made a crate so a plugin can leave the
  repository with its tests (`plans/plan-repo-split.md`). A `[dev-dependencies]` entry only, held by
  the same leak check as the measurement crates. See
  [`crates/mxm-plugin-test/AGENTS.md`](crates/mxm-plugin-test/AGENTS.md).

**`crates/mxm-fft`** (2026-10-13). The collection had five FFTs, every one an iterative radix-2:
the convolution's on the audio thread (f32, interleaved), classic-verb-fit's periodogram (f64,
split, twiddles allocated per call), room-ir's (f64, interleaved), `mxm-measure`'s rulers (f64,
split, `Option` on a wrong length) and a test copy in the creative sampler, each with its own
conventions. Three said a shared FFT waited for another consumer, and `mxm-fx-eq-n`'s analyser was
it. The owner ruled on 2026-10-12, and `rustfft`/`realfft` do the arithmetic so the algorithm is
not the collection's to maintain. Planning was checked before the dependency was committed:
neither crate reserves fallibly, so planning cannot fail cleanly and the convolution plans its
fixed audio-path sizes at activation. The plan and its migration order are the archive's
`plans/plan-mxm-fft.md`. The owner's words: *"make a shared fft crate. Havine 3 or for
implementations is non DRY"*. It owns the conventions: an unnormalised forward, a 1/N inverse,
planning ahead, execution with caller scratch that allocates nothing, and a scoped hardware flush of
subnormals.

**`crates/mxm-stretch`** is an owner-approved exception to evidence-first extraction (2026-10-16).
newDAWn needed its recordings to follow the song's tempo, its pitch kept, and the owner chose the
collection's WSOLA, the creative sampler's Stretch, then asked "Should it not be a shared crate?".
The sampler's reader couldn't serve as it was: it reads one sample at a time inside its voice,
rounds even its clean setting to 16 bits, ramps every note in and goes no faster than twice. So
its splice search, period track, onset detector and sinc read moved here and the reader around
them was made offline; the owner, the sampler's only author, relicensed them MIT ("Relicense into
kit"). What a whole buffer allows is the crate's own: no head reads past the next onset before its
place, so an attack is read once, from its first frame. A release build stretched a stereo
drum loop of 10 s at 44.1 kHz in 31 to 42 ms (2026-10-16).

**`crates/mxm-vst3-host`** is an owner-approved exception to evidence-first extraction
(2026-10-09: "We need VST3 support ourselves in the player and daw"): a `.vst3` module offered as
an in-process CLAP plugin entry, so MXM Player and newDAWn host VST3 through the CLAP code they
have.

**`crates/mxm-keys`** is an owner-approved exception to evidence-first extraction (2026-10-05: the
keyboard library is built here from the start, so the DAW and the instruments share one). Every
editor's cursor reads it since 2026-10-08: the owner decided on 2026-10-07 that the editors convert,
keys first in newDAWn and the collection; `mxm_ui::reach` is the first shared part.

**`crates/mxm-rng`** (2026-10-14, the architecture audit's O3). The collection carried its
generators as copies: xorshift32 in seven byte-identical synth `Rng`s and nine more places,
xorshift64 in the player, both drum engines, the delay and the reverb's tests, Vigna's xorshift64\*
in grain-fx, the model drums, `mxm-measure` and `mxm-listening`, SplitMix64 in the sampler, the
909's sample noise and room-ir, and the Numerical Recipes LCG in the reverb's shaped path and four
products' tests — about forty shipped and seventy test copies, each written again with its own
seeding rule and output mapping. The seeds are load-bearing: golden digests and recorded renders
depend on every stream, so the owner's ruling was *one crate offering each family bit-exactly; no
consumer changes generator*. The crate's reference test pins each family against the first outputs
of the copies themselves, printed before they were removed, and every consumer's goldens held
unchanged. The plan is `ops/plans/plan-mxm-rng.md` in the workspace. A copy of one of the five written
elsewhere is a defect. Room-ir's moved with O6 (2026-10-15, bit-exact); the one that remains,
mxm-fx-eq-n's, moves in its own work. A sixth family arrives with its second copy; a machine's own
register (the 909's LFSR) is the machine, not a generator.

**`crates/mxm-dsp`** (2026-10-14, the architecture audit's O1). The synth DSP crates were honest
copies by design, each carrying an evidence row for the extraction plan. The census found nine
byte-identical `flush`es, six `tanh_approx`, five `tan_approx` and `poly_blep`, four
`clamp_pulse_width`, three `diode_clamp`, and three whole `envelope.rs` files (mono-02's, para-07's
and poly-06's ADSR). Those moved verbatim; the copies that differ stayed: mono-00's ADSR (its
machine's 0.4 ms floor), mono-01's (without the decay stall fix the others gained), mono-pr1's
envelope, mono-03's `tan_approx` (libm's `tan`), para-07's and mono-08's BLEPs and pulse clamp. The
ladders, one-poles, DC blockers and SVFs differ in every copy and wait for a parametrised decision of
their own. The owner's recommendation was *byte-identical items first, golden digests as the proof;
warts stay per crate*. The plan is `ops/plans/plan-mxm-dsp.md` in the workspace.

The effects' half joined with **O2** (2026-10-14): `guard::finite_or` (ten helpers under several names,
three of them with a fallback of zero; inline expressions stay) and `delay::AgeLine`, the ring fifteen
lines kept — age 1 the newest, zero past the written history, a constant-time clear, the fractional age
split before it wraps. Fourteen lines adopted it, each proved bit-identical to the ring it replaced;
each keeps its length, interpolation, clamps, write flush and age origin. fx-delay's generation-tagged
line and fx-oldify's transport (its sinc read takes the raw buffer) keep their own. The rate guards
take `finite_or` and keep their bounds. The effects' other flushes and their SVFs and biquads differ
in every copy; unifying them would change bits and goldens for an inaudible gain, so each effect
keeps its own for now (the owner, 2026-10-14). The plan is
`ops/plans/plan-mxm-dsp-effects.md` in the workspace.

**`crates/mxm-drum-engine`, and note ownership in `crates/mxm-part-routing`** (2026-10-14, the
architecture audit's O4). `mxm-model-drums` was built after `mxm-drum-machine` and took its shell and
engine scaffolding whole: fifteen engine items (88 lines) and the plugins' event group and its impl
(138 lines) proved identical, and the two plugins' resolution of a group (~150 lines) differed only in
comments, the first key and the chromatic reference. The engine items moved to the new crate, the
LFO as `value` then `advance` (the drum machine's `tick` was those two in that order). The note half
moved into `mxm-part-routing` as mechanism — `EventGroup` and `PartState::resolve` — with every choice
passed in through `PartPolicy` and `ResolveFlags`, as that crate's policy-neutral contract requires:
the plugins keep the CLAP translation and its ranges, the priority order, the strike reduction and
which strike sounds, the chromatic offset and the response to a flood. The plan is
`ops/plans/plan-drum-shared.md` in the workspace.

## Why every MSRV floor is compiled against

**Every floor in this table has been compiled against, and a floor that has not is a guess.** Cargo
enforces `rust-version` when it resolves *dependencies*, never when it compiles your own crate, so on
a current toolchain a wrong number is silent — and two of these were wrong for months. Checking one
is a single command per crate.

Run it whenever a crate takes a language feature it did not have before. The two that moved to 1.88
did so because `let` chains stabilised there, which is the shape this will usually take: syntax
arrives, the compiler in use accepts it, and the declared floor quietly stops being true.
