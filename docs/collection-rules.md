# MXM collection rules

The rules every MXM product keeps beyond the plugin conventions ([`plugin-conventions.md`](plugin-conventions.md)): what a copy of a machine owes the machine, how products are named, how code is licensed, and where the research behind the copies may and may not go. A product repository cites them as "root *Naming*", "root *Research boundary*" and so on.

Moved here word for word on 2026-10-06 from the root `AGENTS.md` of the private monorepo the products were split from, with links made to work from here; the lines that described the monorepo are kept and marked *Since the split*.

## The goal, and how much licence a copy has

Open-source CLAP audio plugins for DAWs — the **MXM Synth Collection** — written in Rust with
[nice-plug](https://codeberg.org/RustAudio/nice-plug) and egui, plus **MXM Player**, the CLAP host
that plays and tests them.

**The goal is functional copies of classic machines, and then building blocks taken from them.**
Each instrument reproduces what a particular machine *does*; later, the parts that prove reusable
combine into instruments that never existed. **Which of the two an instrument is decides how much
licence it has**, and the rules below run from that:

- **A copy is warts and all; an original design owes nobody's limitations** (the owner's ruling,
  2026-09-02). Where an instrument copies a machine, it reproduces what that machine *does* —
  including what it does badly. The aliasing, the drift, the noise floor, the narrow range, the
  coupling nobody would design on purpose: those are not defects to be tidied on the way past, they
  are most of why anyone wants the machine. **The instruments that never existed are free**, and
  that freedom is exactly why the copies must stay honest — a machine that has already been
  idealised has nothing left to extract from, and nothing to be contrasted with.
- **Reproduce a wart deliberately, and label it**, or the next reader fixes it. A wart is a property
  of the hardware, named with its evidence in the plugin's AGENTS.md or its brief;
  `crates/mxm-mono-03-dsp/AGENTS.md`'s *The droop is the topology — compensate outside the filter,
  never inside* is the shape. **This is not a licence for bugs.** A NaN, a denormal stall, a click
  from state left over between notes is nobody's hardware and the numeric contracts still bind: the
  difference is that a wart is reproducible on the original and a defect is only reproducible here.
- **The copy is of the function, not the face.** "Homages, not clones" (see *Naming*) is about
  identity and interface — hardware architecture may inspire the DSP, and it never dictates the
  panel. A faithful filter behind an original interface is exactly the intent, and *warts and all*
  stops at the DSP: `docs/MXM_DESIGN_SYSTEM.md` is normative, there are no hardware-replica UIs, and
  a machine's **interface** limitations are the one thing a copy is free to improve. `todo.txt` asks
  for exactly that — a TB-303 "with a better interface", its glide moved into the synth where a
  keyboard player can reach it.
- **No effect that was not on the original instrument** (the owner's ruling, 2026-09-02). Effects are
  their own collection *and* an effect the machine shipped with is part of the machine — the second
  wins where they meet, because an instrument missing an effect its original had is not a faithful
  copy of it. So a Juno's chorus belongs inside the Juno; a 303's distortion does not belong inside
  the 303, because the 303 never had one and players reached for a pedal. The test is not "is this
  an effect" but **"was it on the machine"**, and it cuts both ways: it puts the chorus in, and it
  keeps out every effect somebody would merely *like* to have.
  `mxm-chorus-06` demonstrates the extraction rule: the 106's chorus is available as a separate
  building block, with the circuit positions as presets and the instrument DSP depended on in place
  rather than moved.
  **One instrument is exempt, by ruling (owner, 2026-09-02): `mxm-mono-00`.** Its feature set is
  Roland's SYSTEM-100 plug-out rather than the 101/102 pair alone, so it carries the plug-out's
  **phaser and delay**, which no System-100 module had, beside the spring reverb the 103 mixer did
  have. `research:instruments/system-100.md` §12 and §14 keep the
  record of what the hardware had. The exemption is specific to that instrument and does not soften
  the rule for any other.
- **An instrument does not carry what the player or the DAW already does** (the owner's rule of
  thumb, 2026-09-03 — *non-strict*, a default rather than a ruling). An arpeggiator, key hold, an
  octave shift, a transpose, a step sequencer: the host has one, MXM Player has one, and a copy
  inside the plugin is a second implementation that has to be kept in step with the first. So
  `mxm-poly-06` rejected key transpose and `mxm-mono-00` leaves the plug-out's arpeggiator,
  scatter, key hold and octave shift out, though both were on the source. **The test is "is this
  the voice, or the keyboard in front of it"** — and it is a default, not a wall: where a machine's
  own note-side feature *is* its sound (a 303's slide, an SH-101's own sequencer-driven
  articulation), the copy keeps it and the plugin's AGENTS.md says why.
- **Don't pre-generalise** — and the destination is why, not an exception to it. Shared building
  blocks come from several honest per-machine implementations, extracted once the evidence says
  what is genuinely shared. Extracting early guesses at it. See the ten crate rules below.

Reference-quality open source: clarity beats cleverness, and every nontrivial algorithm names the
technique or paper it comes from.

**Under git, so history is recoverable** — but still read a file before overwriting it, and prefer
relocating content over deleting it.

## Naming

Every instrument that copies hardware is **`mxm-<category>-<model-token>`, all lower case**, where
`<category>` is the voice architecture and `<model-token>` is a compact lowercase alphanumeric
normalisation of the inspiring hardware's model designation. **It is not a sequence number and does
not always mean “last two digits.”** This is the collection-wide rule for copies, not an exception
for nonnumeric models (owner, 2026-09-08). Letter prefixes are retained when they distinguish the
model, punctuation is removed, and a word may be shortened when the owner fixes the product
identity.

**An original instrument has no hardware token to invent.** It takes an owner-approved descriptive
`mxm-<name>` instead (owner, 2026-09-09), under the same lowercase and permanent-identity rules.
`mxm-creative-sampler` is a small-sample sound-design instrument, not a copy and not a realistic
multisampler. **`mxm-drum-machine` is the approved name and product boundary for the in-development
drum instrument** (owner, 2026-09-17): it has **sixteen simultaneously available voices**, while each
voice selects its sound engine from a larger pool of machine-specific models through a classic
dropdown-style selector; its concrete widget follows design system §7.4 rather than inventing a
local dropdown. Sixteen is the live voice count, not a cap on implementations; preserve as many
evidence-backed models as practical rather than collapsing the pool to sixteen generic sounds. A
selected model exposes parameters for creative sound shaping; faithful machine behaviour supplies
the model and its reference settings, not an artificial ban on adjustment.

| Hardware | Plugin |
|---|---|
| Roland System-100 | `mxm-mono-00` |
| Roland SH-101 | `mxm-mono-01` |
| Roland SH-2 | `mxm-mono-02` |
| Roland TB-303 | `mxm-mono-03` |
| Roland JUNO-106 | `mxm-poly-06` |
| Buchla Model 208 | `mxm-mono-08` |
| Sequential Circuits Pro-One | `mxm-mono-pr1` |
| Roland SH-7 | `mxm-para-07` |

The **`para` category was established for `mxm-para-07`** by the owner on 2026-09-04: two keyed
pitches with one shared filter and VCA. It names the voice architecture, not a marketing variant of
`mono` or `poly`.


**For copies, model tokens are a namespace keyed to hardware, not a sequence.** The existing numeric
tokens are permanent product identities and are not renamed. New model tokens are approved at intake
rather than generated by an algorithm that can collide silently; `mxm-mono-pr1` was explicitly
selected by the owner. The category prefix remains load-bearing: it
distinguishes voice architectures without requiring model tokens to be globally unique.

**Collisions and ambiguous normalisations are settled one at a time, by ruling, when an instrument
is proposed.** Last-two-digits alone already collides — SH-1 and SH-101 both reduce to `01`, while
MC-202 and SH-2 both reduce to `02` — and within one category the prefix cannot separate them. The
first collision has been settled (owner, 2026-09-02): the System-100 is named for the **system**,
`mxm-mono-00`, not for its Model 101 keyboard unit, whose `01` the SH-101 already holds. The SH-1 /
SH-101 and MC-202 / SH-2 collisions stay undecided until one of those machines is proposed.

Use the approved form in the crate directory (`plugins/mxm-mono-pr1`), package name,
`bundler.toml` and bundle filename. **Never use the original manufacturer's name or full model
designation as a product name** — these are homages, not clones. The rule extends to parameter
labels, filter mode names and preset names: "Ladder" is fine, "Moog" is not.

**One literal per plugin, and a rename is that one line.** The plugin's name is written once in its
crate — `plugin_name!` in `plugins/mxm-mono-01/src/lib.rs` — and both the display name and the
`CLAP_ID` derive from it. `bundler.toml` is the only place it is duplicated outside the crate, and a
test in the plugin fails if the two disagree. **`CLAP_ID` is deliberately not derived from
`CARGO_PKG_NAME`**: a permanent identifier must not follow a directory rename, which would change it
with no compile error and orphan every preset and saved project written under the old one.

**An effect is named for what it is, not for the synth that first carried its code** — the owner's
ruling, 2026-09-03, for the effects collection. `mxm-<effect>-<NN>` where `<NN>` keys the specific
box, or a descriptive name where no box has a number to key. `mxm-chorus-06` keeps the `06` because
it is specifically the 106's chorus circuit sold separately, with the circuit's positions as its
presets and proved equal to the built-in at each; a plug-out effect with no box behind it gets no
number and no product until a box is chosen (`plans/plan-mxm-fx-collection.md` §1).

**A new effect takes `mxm-fx-<effect>`** — the owner's ruling, 2026-09-16, taken when `mxm-fx-curve`
was proposed and the paragraph above was found not to permit it. The prefix marks the effects
collection in a host's plugin list. `mxm-fx-convolution` (owner-approved 2026-09-14, with no
rationale recorded at the time, and until now the collection's only `mxm-fx-` name) is retroactively
consistent rather than the outlier it had been. **A box-keyed effect keeps its `-<NN>` behind the
prefix** — `mxm-fx-<effect>-<NN>` — so the two halves compose rather than competing.

**Existing effect names do not change, and that is not untidiness.** `CLAP_ID` is permanent and a
rename orphans every preset and saved project written under the old one, which the paragraph above
this one explains. So `mxm-chorus-06`, `mxm-folded-spring`, `mxm-bucket-delay`, `mxm-shimmer`,
`mxm-classic-verb` and `mxm-grain-fx` keep theirs and the collection stays mixed, deliberately. The
trailing `-fx` in `mxm-grain-fx` is a different thing and not this prefix: it is the 2026-09-09
ruling separating it from the granular *instrument* that follows.

**An example's name is unique across the whole workspace, so it carries the crate it belongs to** —
`mono_01_filter_spike`, `mono_03_render_demo`, `shimmer_preset_audit`, or a machine name like
`sh2_demo` and `juno_demo` that is already unique. This is a cargo constraint, not a style
preference: cargo writes every example in a workspace to **one flat `target/<profile>/examples/`
directory**, keyed by target name alone. Two crates sharing a name share an output file, and the
consequences are worse than a duplicate — `cargo run --example` runs whichever binary won the last
race, silently and with no warning, and a parallel `--workspace` build dies with a linker error
naming a file that looks locked. Cargo emits `warning: output filename collision` and nothing else.
Four crates shared `render_demo` and two pairs shared `filter_spike` and `preset_audit`; the fix and
the three-day misdiagnosis it caused are in [`docs/known-issues.md`](known-issues.md).

Hardware architecture may inspire the DSP. It never dictates the interface.

## Licensing

**Per-plugin, not per-workspace.** Where all code is original, **MIT**. Each plugin folder — and
the player — carries its own `LICENSE`, so a future GPL-derived plugin can sit alongside without
relicensing everything.

- Borrowing from a GPL project (VCV Rack is GPL-3.0) forces that plugin to GPL. Decide before
  copying, not after.
- Adding a VST3 export pulls in GPL bindings. CLAP-only avoids this entirely.
- Check the licence before porting any algorithm, and record source and licence in a comment at the
  top of the file. Cite techniques (TPT/ZDF, PolyBLEP) even when the implementation is original.
- MIT licensing of our code resolves nothing about trademarks, trade dress, fonts or artwork.
- **MPL-2.0 is accepted in shipped plugins** (the owner, 2026-09-15) — for symphonia, in
  `crates/mxm-audio-file-decode` only, used unmodified. It is file-level copyleft: a binary that links
  it must tell recipients where the source is, so the notice is compiled into that crate and travels
  with every binary that links it. **Never vendor, patch or modify an MPL crate** — that is where its
  obligations bite. The ruling does not extend to LGPL (LAME, for MP3 export), which stays a decision.

*Since the split (2026-10-06):* every product repository — each instrument, effect, MXM Player and the tools — is GPL-3.0-or-later as a whole, with the licence at its root, and mxm-kit is MIT. The per-plugin MIT above describes the monorepo. The rest of this section still holds.

## Research boundary

The research behind the copies — what a machine, an effect or a filter family *does*, read off
its own manuals, service notes, schematics and recordings — lives in a separate, **private**
repository, conventionally a sibling checkout at `../01-mxm-collection-research`
(`maxmcorp/mxm-collection-research`, split from this one on 2026-09-04). It is private because it
holds third-party material. Research source material stays in that repository; fixed product assets
stay with and are documented by their shipping crate rather than treated as research records. The
checklist below protects that boundary. Nothing here depends on the research checkout at build or test time. `docs/filters/`, `docs/oscillators/`
and `docs/modulation/` stay here because they are our own theory, measured by `crates/dsp-lab`
against our own code; the machine pages, the effect pages and the family deep-dives they cite went
across.

**Citing across the boundary.** A reference from this repository into the research is written as
plain text in a code span, never as a link, a URL or a filesystem path:

    `research:instruments/sh-7.md` §5.6
    `research:filters/machines/ir3109-roland.md` §10

The path is relative to the research repository's root, with forward slashes; a section number
follows outside the code span, as this repository already writes its own citations. The research
repository cites back the same way, as `` `software:<path>` `` relative to this root. The link
check in `docs/AGENTS.md` ignores these on purpose; its resolver checks them when the sibling
checkout is present.

**What may cross into this repository from the research:** facts; numbers; measurements we made
ourselves and our own prose, rewritten for their destination; a short quotation (a sentence or
two, quoted, naming the document and page) where the maker's words are the point; and the
citation. **What may not:** any third-party research file — a scan, a PDF, a schematic, a photograph,
a source recording, a scraped page, a search transcript; any image of any product; verbatim text
beyond short quotation; a transcription or OCR of a document; a redrawn schematic that reproduces a
sheet. Reading is unrestricted in both directions. The boundary governs what is committed and
distributed.

**What a disassembly may contribute, and what it may not** — the owner's ruling, 2026-09-08, taken
for `mxm-shimmer` and binding on every effect after it. The research repository may study an
installed binary by static analysis, and a page written that way is evidence like any other
(`research:AGENTS.md`, *Installed software*). **What it establishes does not all cross.** This
repository is MIT and is meant to be published, so the line runs between the technique and the
artefact:

- **Crosses:** the topology, and the arithmetic that is true of any implementation of it — a
  structure, a signal path, the invariants a correct build must satisfy, the shape of the test that
  proves them. Anything the maker published themselves, in a manual or in the open. Our own
  measurements of what the software does.
- **Does not cross:** a constant read byte-exact out of a commercial binary and specific to that
  product — its delay values, its buffer sizes, its block granularity, its internal structure
  offsets, its factory preset values. **Our constants come from our own measurement**, which
  `plans/AGENTS.md` required of a plan long before any binary was read: those are decided at the
  keyboard, against the compiler and a measurement.

**Published source is a different question, and it is ruled** (the owner, 2026-09-09, taken for
`mxm-grain-fx`). Where a maker has released a box's own firmware under a **permissive licence**,
reading it is neither a measurement nor a reverse-engineering: it is the box's own documentation in
the form its author chose to publish, and the research repository records it as its own evidence
tier (`research:effects/AGENTS.md`, *Published source is a sixth tier*). Unlike a commercial
binary's, **its facts cross into this repository freely** — the numbers, the topology, the
identities — because the licence permits use without restriction and a value is not copyrightable.
What does not cross freely is **code**: a file *derived from* the source carries that licence's own
obligation (MIT requires its copyright and permission notice in "all copies or substantial
portions"), while a clean-room implementation written from a research page's prose carries none. A
plan on this tier states which of the two it is doing before a line is written, and the research
side's sources README records the licence verbatim. This is not a licence to read any public
repository: it applies to the **box's own** published code, by its maker.

Two things this ruling is not. It is **not a judgement that the risk is zero**: a clean room whose
reading and writing were done by the same party is not a clean room in the legal sense, where the
point is a wall between the two. And it is **not a shortcut** — a topology taken this way still
names the published technique it implements, under *Licensing* above, and *Don't open existing
implementations* still binds, because a licence that forbids copying source is not softened by
having read a binary first.

**Before this repository is made public, and before every release after that:**

1. `git ls-files | grep -iE '\.(pdf|jpe?g|png|gif|webp|bmp|tiff?|wav|flac|mp3|ogg|m4a|aiff?|aifc|caf|opus|zip|7z|docx?|xlsx?)$'`
   is empty, or every hit is our own work and says so in the nearest AGENTS.md.
2. `git log --all --oneline -- temp/buchla-research docs/effects/samples temp/Roland_SH-101.webp`
   is empty. The purge of 2026-09-04 removed those paths from history; a hit means an old clone
   was merged back in, and the fix is a fresh clone, never a pull.
3. `git grep -n -i -E 'Users/maxm|AppData.Local.Temp|scratchpad|temp/buchla-research|docs/effects/samples' -- . ':!AGENTS.md'`
   finds no committed path into a purged folder or onto a local disk.
4. The resolver prints nothing when the sibling checkout is present and up to date with its origin
   (fetch and fast-forward it first; a stale checkout reports pages that exist):
   `git grep -h -o -E 'research:[A-Za-z0-9._-][A-Za-z0-9._/-]*' -- . ':!automation/tests' | sed 's|^research:||' | sort -u | while read -r p; do [ -e "../01-mxm-collection-research/$p" ] || echo "MISSING $p"; done`
5. Trademarks: makers' and models' names appear only as references to the hardware being copied
   (*Naming* already forbids them in a product name, `plugin_name!`, a bundle filename, a parameter
   label or a preset name). Read every hit of
   `git grep -n -i -E 'roland|juno|buchla|moog|korg|yamaha|oberheim|sequential|prophet' -- bundler.toml 'plugins/*/presets' 'plugins/*/src/params.rs'`.
6. MPL notices ship in the artefact. Every release bundle whose package's normal graph contains
   `mxm-audio-file-decode` (`cargo tree -e normal -p <package>`) carries that crate's `NOTICE.md` text
   in its binary, and `cargo tree -e normal -p mxm-player` contains no symphonia crate.
