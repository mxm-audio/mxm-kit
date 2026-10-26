# AGENTS.md — crates/mxm-drum-shell

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

What the collection's sixteen-slot drum plugins share on the plugin side, moved from the copies
`mxm-drum-machine` and `mxm-model-drums` carried (`plans/plan-drum-shell.md` in the workspace's
ops, owner-approved 2026-10-26): each slot's name and the host's note names, Resample's capture
thread, its handover to the audio thread and its state machine, the sample pack, the signals
between them, and the editor's Resample, Export and name box. The evidence is in the kit's
[`NOTES.md`](../../NOTES.md#the-crate-rules-in-full). `mxm-drum-engine` is the engine side.

# Ownership

Owns `src/` and `Cargo.toml`:

- `src/slot_names.rs`: the names' rules, the persistent field (`slot-names`), `filter`, `announce`,
  `HostNames`; a plugin's parameters implement `DrumSlots`.
- `src/bank.rs`: `CaptureBank<K>`, the lock-free handover of a kit.
- `src/capture.rs`: the capture thread (`CaptureWorker`, `Gate`, `Heard`) and `Resample`, the
  state machine `activate` and `process` run; a plugin implements `Capturer`.
- `src/pack.rs`: the pack's writer, its manifest, `slug`, `root`; a plugin implements `OneShots`.
- `src/signals.rs`: `Signals`, a field of each plugin's `Telemetry`.
- `src/editor.rs`: the app bar's pair, its `…` menu rows and status, the name box.

**Does not own** a capture, a frozen reader, a model, a parameter or its ID, a preset, or an
editor's layout: those stay with each instrument, and enter through the three traits.

# Local Contracts

- **A body moves here when both plugins' copies are the same**; what differs enters through a trait
  method or a field the plugin passes. A trait carries only what the two already decide, never a
  method for a consumer that does not exist (a sample-drums voice is not one yet).
- **No saved state changes**: the `slot-names` field and every parameter ID stay each plugin's,
  spelled as before; the editor's egui ids are each plugin's, from its name (`editor::Ids`).
- **Realtime**: `Resample::service`, `CaptureBank::take` and `retire` and every `Signals` method the
  audio thread calls allocate, lock and drop nothing. The capture runs on the plugin's own thread,
  never nice-plug's `AsyncExecutor::execute_background` (`capture`'s module documentation).
- **Teardown is bounded**: the worker stops between a start's steps, a render's chunks, a
  one-shot's chunks (`pack::CANCEL_CHECK_FRAMES`) and a file's mebibytes.
- **An existing pack is never overwritten**; each write stages on its own and the rename takes the
  name. **A test writes its packs under the build's folder** (`pack::tests::scratch`), never the
  real pack root, never on C:.
- MSRV **1.95** (nice-plug and egui). MIT.

# Verification

```bash
cargo +1.95.0 test -p mxm-drum-shell
cargo clippy -p mxm-drum-shell --all-targets -- -D warnings
```

Each drum plugin's own tests hold what needs a plugin: its Init's names, a kit load emptying them,
the names restored through the real wrapper, a capture through its engine, its one-shots.

# Child DOX Index

None.
