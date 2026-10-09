# AGENTS.md — crates/mxm-vst3-host

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

VST3 plugins in a CLAP host. A `.vst3` module is offered as an in-process CLAP plugin entry
(`Vst3Entry`), which clack-host loads with `PluginEntry::load_from_clack`; each plugin it creates
is a CLAP plugin driving one VST3 component. So MXM Player and newDAWn host VST3 with a few lines in
their loader and scanner, and everything else they do for CLAP applies unchanged. An
owner-approved exception to evidence-first extraction (the owner, 2026-10-09: "We need VST3 support
ourselves in the player and daw"), shared from the start because both hosts need it. The mapping,
CLAP to VST3, is the table in `src/lib.rs`.

# Ownership

- `module.rs`: finding a bundle's library per platform, its entry and exit functions, the factory
  and its audio classes; one load per process, shared.
- `scan.rs`: reading a module's classes out of process, in a copy of the host started with
  `PROBE_ARG` (`serve_probe`), with `PROBE_TIMEOUT`.
- `component.rs`: one instance — component and controller created, initialised, connected and torn
  down as the SDK's host does; buses; the state blob.
- `plugin.rs`: the CLAP plugin — activation, processing, parameters, ports, state, latency, tail,
  render mode. `params.rs`, `events.rs`: the fixed-size change and event lists of a block.
- `handler.rs`: `IComponentHandler`, the editor's edits and restart requests. `host_objects.rs`:
  the host application, messages, attribute lists, memory streams.
- `gui.rs`: CLAP `gui` on `IPlugView`, embedded or floating; `window.rs` (+ `window_macos.rs`,
  `window_x11.rs`): a floating editor's window; `run_loop.rs`: Linux's `IRunLoop`.
- `examples/vst3_probe.rs` and `examples/vst3_editor.rs`: checks against installed plugins.

# Local Contracts

- **Scanning runs no plugin code in the host.** An entry's classes come from a probe child; the
  module is loaded into the host when its first plugin is created. A host calls `serve_probe()`
  first thing in `main`; a program that does not (a test binary) is noticed by the missing header
  and loads the module in-process. Falcon, installed but not logged in, hung its load for 27 s to
  minutes behind a login window (2026-10-09): one such plugin must never stall a host's scan.
- **Why an entry failed is `load_error(bundle)`**, kept from the scan; clack's entry error carries
  no reason, and asking the module again would load it.
- **A plugin id is `vst3:` and the class ID** as the SDK prints it, the same on every platform
  (`strings::class_id_text`). It is stored in projects: never change its form.
- **The audio thread never allocates, locks or calls the controller.** Change and event lists are
  sized at activation; the editor's edits and the controller's values cross on `rtrb` queues; a
  full list or queue drops and counts (`HandlerState::lost_edits`), never grows.
- **Values:** a continuous parameter is its normalised value, a stepped one its step
  (`params::to_clap`, the SDK's rule). CLAP modulation is added before the processor hears it; the
  controller keeps showing the value.
- **Only main buses are offered**, and an instrument's input is not (hosts take an instrument as a
  source without one); the rest are processed with silence in. Stereo is asked for when a main bus
  is neither mono nor stereo.
- **A tail of 0 is "unknown"**, CLAP's infinite: VST3 hosts process regardless, and plugins report 0
  while ringing (ValhallaSupermassive).
- **A floating editor is a window of this crate's**, the view attached inside; closing it hides it
  and tells the host (`gui.closed`) on the next main-thread callback, never from inside the window
  procedure.
- **Linux: a module's library is never unmapped** (`RTLD_NODELETE`), the workspace's rule for every
  plugin library.
- **Dependencies are pinned exactly**, except two caret requirements MXM Player and newDAWn lock
  differently: `objc2` (0.6.4 and 0.6.5) and `libc` (0.2.189 and 0.2.190).
- `#![allow(clippy::unnecessary_cast)]`: the SDK's enumerations are `int` on Windows and
  `unsigned` elsewhere, so a cast one platform needs is a no-op on another.

# Work Guidance

- Follow the SDK's own host (`public.sdk/source/vst/hosting/`) for any order of calls; say where
  it is the source.
- Unverified on macOS and Linux beyond compiling: the editor windows and the run loop there wait for
  the other platforms' batch.

# Verification

```bash
cargo test -p mxm-vst3-host
cargo clippy -p mxm-vst3-host --all-targets -- -D warnings
cargo clippy -p mxm-vst3-host --lib --target x86_64-unknown-linux-gnu -- -D warnings
cargo clippy -p mxm-vst3-host --lib --target aarch64-apple-darwin -- -D warnings
# Installed plugins (Windows): parameters, a sine through it, state round trip; and the editor.
cargo run -p mxm-vst3-host --example vst3_probe -- "<path.vst3>"
cargo run -p mxm-vst3-host --example vst3_editor -- "<path.vst3>" floating out.bmp
```

The two cross-target checks compile only (no linker, no run): they are how the other platforms'
code is kept compiling between batches. A change to `gui.rs` or `window*.rs` gets a
`vst3_editor` run, floating and embedded.

# Child DOX Index

None.
