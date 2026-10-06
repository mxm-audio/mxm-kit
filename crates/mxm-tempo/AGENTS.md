# AGENTS.md — crates/mxm-tempo

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

The collection's tempo sync, once (`plans/plan-tempo-sync-controls.md`, Revision 2). A tempo-syncable
time or rate is one continuous control with one adjacent quarter-note button; with it on, the
control's own position picks a musical division and the control reads it. Zero dependencies and MSRV
**1.87**, so the audio half of any plugin can take it without inheriting the GUI floor.

Shared on evidence: mxm-mono-00, mxm-drum-machine, mxm-bucket-delay and mxm-fx-delay each carried a
copy — three division tables that are slices of one, the same rounding, the same reach arithmetic and
the same no-tempo test. The owner, 2026-09-25: *"we should make 1 common way it works"*.

# Ownership

Musical time, and nothing a plugin decides:

- **`Division`** — the one table, sixteen steps shortest to longest (1/64 … 4 bars, with triplets and
  dotted values), with beats, labels, parsing, seconds and hertz.
- **`Span`** — a contiguous run of it: the divisions one control offers. `Span::LFO` (1/32 to four
  bars, the drum pilot's) is every LFO's.
- **`Direction`** — `Time` puts the longest division at the top of the travel, `Rate` the fastest:
  **a synced control moves the way it moves free**.
- **`Ladder`** — a span and a direction: `pick` (position → division, rounded, as every copy did),
  `position` (the inverse, for preset designs and tests), `reachable` and `division` (clamp into what
  the control's range holds at the tempo, never rescale), `resolve` (the synced value or `None` for
  the free one) and `shown` (what the control reads).
- **`tempo`** — only a finite, positive tempo is a tempo.
- **`TempoCell`** — the tempo in force, published by the audio thread for the editor.

**Not here:** a plugin's parameters, ranges, skew, smoothing, and any transition law — a delay's
Glide, Snap, Fade or Repitch is the plugin's.

# Rules

- **A stored position is a contract.** Presets and automation store the control's position, and the
  position picks the division. `Division::ALL` is closed: a step inserted between the ends of a
  shipped span moves every stored position in it — a migration, never an addition.
  `the_legacy_tables_are_contiguous_slices` pins the three tables this replaced.
- **Clamp, never rescale.** A division the range cannot hold at the tempo becomes the nearest it can — by ratio, also when a narrow range falls between two;
  the travel goes flat at the end, and a position means the same division at every tempo it is
  reachable at.
- **No allocation, no locks.** Every type is `Copy`, every string `'static`; `TempoCell` is one atomic.

# Verification

```bash
cargo test -p mxm-tempo
cargo +1.87.0 test -p mxm-tempo
cargo clippy -p mxm-tempo --all-targets
```

# Child DOX Index

None.
