# Known issues

Upstream defects and local integration failures we have diagnosed, with whether our own code can
do anything about them.

**Seven of these are fixed locally.** They are present in the pinned published release, so they are
still recorded here — but `nice-plug` is redirected to a patched copy in
[`../vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) and the defects do not reach a build. Their
`plugin_robustness.rs`, bundled-behavior and validator regressions stay after an upstream upgrade,
because they are what proves the upgrade worked.

| Issue | Status |
|---|---|
| No `params.rescan(VALUES)` after a host state load | **Fixed in `vendor/nice-plug`** |
| Parameters exposed on a normalised range | By design; a host must not assume otherwise |
| Wildcard `NoteChoke` channel and key values | Not patched — the player avoids the path instead |
| More than 512 events in one process block allocates | **Fixed in `vendor/nice-plug`** |
| The state loader trusts a length field from the stream | **Fixed in `vendor/nice-plug`** |
| A non-finite parameter value from the host reaches the smoother and the DSP | **Fixed in `vendor/nice-plug`** |
| Unequal main input/output ports are declared as an in-place pair | **Fixed in `vendor/nice-plug`** |
| The first sample-accurate parameter event is applied before its offset | **Fixed in `vendor/nice-plug`** |
| An out-of-range event timestamp is used as an audio split point before clamping | **Fixed in `vendor/nice-plug`** |
| The player and a plugin editor both use OpenGL | **Fixed** — the player renders through `wgpu` |
| A plugin editor renders white when the host repaints too often | **Understood**; worked around in the player |
| Floating editors resize slowly | **Fixed in the local floating-window patch**; owner confirmed smooth resizing |
| Windows takes the audio device away from the player | **Understood**; the player names it and reconnects |
| Four examples shared the name `render_demo`, so `cargo test --workspace` raced | **Fixed** — every example name is now unique across the workspace |
| Two crates declared MSRV 1.87 and could not build on it | **Fixed** — both raised to 1.88; a recurrence in `mxm-mono-00-dsp` and vendored nice-plug fixed by nesting (2026-09-16), every 1.87 crate compiled against |

## nice-plug: no `params.rescan(VALUES)` after the host loads state

**Status:** upstream bug, present in `nice-plug 0.3.0` *and* on `main` as of 2026-08-25.
**Fixed locally** in [`vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (patch 3).
**Impact, before the patch:** 3 `clap-validator` failures on every nice-plug CLAP plugin, ours
included, and a stale parameter panel in any host that trusts the callback.
**Fixable from plugin code:** no.
**Regression test:** `plugin_robustness.rs::loading_state_tells_the_host_its_parameter_values_are_stale`.

### Symptom

```
state-reproducibility-basic     FAILED
state-reproducibility-binary    FAILED
state-reproducibility-buffered  FAILED
  After reloading the state, these parameter values changed without a rescan request:
   - Output (1041081293) - 0.0 dB (0.8822) vs -60.0 dB (0.0556)
```

### Diagnosis

The state round-trip itself is **correct** — values save and load accurately. What is missing is
the host notification afterwards.

`Wrapper::set_state_inner()` is the path taken when the *host* calls `clap_plugin_state::load()`.
It ends with (`nice-plug-0.3.0/src/wrapper/clap/wrapper.rs:1986-1991`):

```rust
#[cfg(feature = "editor")]
{
    let task_posted = self.schedule_gui(Task::StateChanged);
    nice_debug_assert!(task_posted, "The task queue is full, dropping task...");
}
```

`Task::StateChanged` notifies the *editor*. It never schedules `Task::RescanParamValues`, which is
what actually calls `host_params->rescan(CLAP_PARAM_RESCAN_VALUES)` (`wrapper.rs:460-468`). So the
host is never told its cached parameter values are stale.

The neighbouring `set_state()` path — used when the *plugin* changes its own state — does schedule
it, and its comment assumes `set_state_inner` did the same (`wrapper.rs:1837-1840`):

```rust
// After the state has been updated, notify the host about the new parameter values
let task_posted = self.schedule_gui(Task::RescanParamValues);
...
} // Else the RescanParamValues task has already been sent
```

That trailing comment is the bug: the `set_state_inner` branch does *not* send it.

Note the missing call is also inside `#[cfg(feature = "editor")]`, so a plugin built without an
editor gets no notification of any kind after a host state load.

### Real-world consequence

After a DAW loads a project or preset, the host's cached parameter values may stay stale until
something else triggers a rescan. Hosts that re-read parameters on their own will look fine; hosts
that trust the cache will show wrong values in automation lanes and generic UIs.

### The one-line fix

In `set_state_inner()`, schedule the rescan unconditionally (outside the `editor` cfg):

```rust
let task_posted = self.schedule_gui(Task::RescanParamValues);
nice_debug_assert!(task_posted, "The task queue is full, dropping task...");
```

### Current resolution

The repository takes the vendored-fix option: `[patch.crates-io]` redirects the pinned release to
`vendor/nice-plug`, where patch 3 schedules `Task::RescanParamValues` after a host state load. The
regression remains in the player so a future upstream release can replace the vendored copy only
when it preserves the callback. Until then this is maintained patch debt, not a pending decision.

### Verification

`plugin_robustness.rs::loading_state_tells_the_host_its_parameter_values_are_stale` exercises the
host-load path directly. The debug `clap-validator` run documented in `vendor/nice-plug/PATCHES.md`
passes the state-reproducibility checks with the vendored patch; they failed without it.

## nice-plug: parameters are exposed to CLAP hosts on a normalised range

**Status:** by design in `nice-plug 0.3.0`, but worth writing down.
**Impact:** none, once a host does not assume otherwise.

`clap_param_info` reports `min_value = 0.0`, `max_value = 1.0` for every nice-plug parameter,
whatever the plugin's own units. `value_to_text` and `text_to_value` both work in that normalised
space too, so they remain each other's inverse — `text_to_value("1.0 kHz")` on mxm-mono-01's cutoff
returns roughly `0.47`, not `1000.0`.

A host must therefore treat the declared range as authoritative and never assume plain units.
MXM Player does: it stores whatever the plugin declares and asks the plugin to format it. This is
recorded because the opposite assumption is easy to make and produces a panel that looks right
until a value is typed in.

## nice-plug: wildcard `NoteChoke` channel and key values

**Status:** reported during the player's design review; not independently reproduced here, and
**not patched** — the player avoids the path rather than relying on it.
**Impact:** the player never relies on it for an MXM plugin.

CLAP defines `-1` in a note event's channel, key, port or note-id fields as "match all", which is
what makes a wildcard `NoteChoke` the natural global recovery for a CLAP-only note port.
nice-plug's wrapper is reported not to handle those wildcard values correctly.

MXM Player therefore chooses its global recovery from **what the port accepts, not the dialect it
is using**: a port that accepts MIDI 1 — which is every MXM plugin today — is sent CC 120, even
though ordinary notes go out as CLAP events. The wildcard-choke path is reserved for genuinely
CLAP-only ports, and is verified against `dk.mxm.fixture.clap-only-notes` rather than assumed
(`apps/mxm-player/tests/verification.rs`).

## nice-plug: more than 512 events in one process block allocates on the audio thread

**Status:** upstream bug in `nice-plug 0.3.0`, still present on `main` (`542daf1`). Reproduced and
bisected here. **Fixed locally** in [`vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (patch 1).
**Impact, before the patch:** debug builds **abort**; release builds allocate on the audio thread
silently.
**Fixable from plugin code:** no — the buffer belongs to the wrapper.
**Regression tests:**
`plugin_robustness.rs::more_events_than_the_configured_wrapper_capacity_do_not_allocate_or_abort`
and `more_output_events_than_the_configured_wrapper_capacity_do_not_allocate_or_abort`.

### Symptom

`cargo xtask bundle mxm-mono-01` (debug, so `assert_process_allocs` is active) then
`clap-validator validate` crashes one test:

```
ERROR Test process-varying-block-sizes crashed: exit code: 0xc0000409
memory allocation of 20480 bytes failed
```

`0xc0000409` is Rust's abort. The test description is the clue: it "processes random audio and
random **note events** ... while trying different maximum block sizes ranging from 1 to 16k".

### Diagnosis

`nice-plug-0.3.0/src/wrapper/clap/wrapper.rs:603`:

```rust
input_events: AtomicRefCell::new(VecDeque::with_capacity(512)),
output_events: AtomicRefCell::new(VecDeque::with_capacity(512)),
```

A fixed capacity, allocated once — but `handle_in_events_until` pushes into it without any bound
(`wrapper.rs:1530`, `:1552`, `:1583`, and the rest of that match). The 513th event in a single
process block makes the `VecDeque` double, **inside** the `process_wrapper` allocation guard
(`wrapper.rs:2148`).

The arithmetic matches exactly: `NoteEvent<()>` is 20 bytes, and 1024 × 20 = **20480**.

### Reproduction

Bisected with MXM Player's own host, which counts allocations on the callback thread:

| Events in one process block | Result |
|---|---|
| 400 | no allocation |
| 500 | no allocation |
| 520 | `memory allocation of 20480 bytes failed`, abort |

The original bisection motivated moving allocation to `activate()`, but reservation alone was not a
complete fix: CLAP does not bound event count by frame count. The patched copy now also enforces a
hard processing-time queue limit. Activation reserves the frame-scaled event budget plus one value
event per exposed parameter, because a host may send a complete patch at one sample and process-time
application must match `params.flush()` even for a large parameter inventory. That parameter budget
exists from wrapper construction because flush is valid before activation; activation adds the frame
budget.

A second fixed queue was exposed by mxm-drum-machine's then-3,210-parameter routing surface. An editor
preset emits begin/value/end for each parameter, but `output_parameter_events` held only 2,048
events. A factory selection therefore changed about 682 parameters—the first four slots in declaration
order—and silently discarded the other twelve slots. That GUI queue now retains its 2,048-event
live-edit reserve plus three events per exposed parameter, allocated with the wrapper on the main
thread. A focused capacity test and a live factory-preset switch through the bundled CLAP cover the
two halves. Excess ordinary input and ordinary plugin output are dropped;
releases replace ordinary events in the same queue whenever one is available, prioritising note
termination under overload. The input regression activates for 64 frames (limit 512), sends 1000
events ending in a release, and proves the overflow path neither allocates nor aborts under
`assert_process_allocs` while that release closes the accepted note. The output regression's
nice-plug fixture emits 1000 note-ons followed by a distinct admitted note's release; the host must
receive both ends without the queue growing.

### Why it matters beyond the validator

512 events in one buffer is not exotic. A dense MIDI file, a controller sweep recorded at sample
accuracy, or several keyboards merged into one buffer all reach it. In release the abort does not
happen — the wrapper simply allocates on the audio thread, which is the realtime violation this
project flushes denormals and preallocates everything else to avoid, and it announces itself only
as an occasional dropout.

## nice-plug: a non-finite parameter value from the host reaches the smoother and the DSP

**Status:** upstream defect in `nice-plug 0.3.0`. **Fixed locally** in
[`vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (patch 4).
**Impact, before the patch:** one `CLAP_EVENT_PARAM_VALUE` or `PARAM_MOD` carrying NaN or an
infinity silences the instrument until it is reloaded.
**Fixable from plugin code:** only by sanitising every parameter read in every plugin — four
copies of a guard for one boundary the wrapper owns.
**Regression test:** `plugin_robustness.rs::a_non_finite_parameter_value_from_the_host_is_dropped`.

### Symptom

Found by reading, in mxm-mono-00's code review (round 4, 2026-09-03), not by a host: no host in
use sends one. A reviewer holding the DSP to its contract — host input is untrusted, every range is
bounded in the DSP too — noticed that the bounds are `f32::clamp`, and `clamp` returns NaN for NaN.

### Diagnosis

`wrapper.rs`, `update_plain_value_by_hash`: the plain value is divided by the step count,
normalised and handed to `_internal_set_normalized_value`, whose range `normalize` clamps to
`0..=1` with `f32::clamp`. A NaN survives every step, the smoother takes it as its target, the
plugin reads it, and in the DSP `NaN * 0` is `NaN` — so a parameter nobody has turned up still
poisons the mix, exactly as mxm-mono-00's unclamped filter corners did at 1234.57 Hz.

### The fix

Both arms return early when the value is not finite. The event is consumed and the parameter keeps
its value: a NaN is not a value the host can have meant.

## nice-plug: the CLAP state loader trusts a length field from the stream

**Status:** upstream bug in `nice-plug 0.3.0`, still present on `main` (`542daf1`). Reproduced here.
**Fixed locally** in [`vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (patch 2).
**Impact, before the patch:** a corrupt or truncated preset **aborts the process**, taking the host
with it.
**Fixable from plugin code:** no — the length is read before the plugin's deserializer is reached.
**Regression test:** `plugin_robustness.rs::malformed_state_is_rejected_rather_than_aborting_the_plugin`.

### Symptom

```
ERROR Test state-invalid-random crashed: exit code: 0xc0000409
memory allocation of 1025222176999353387 bytes failed
```

### Diagnosis

`nice-plug-0.3.0/src/wrapper/clap/wrapper.rs:3637-3646`:

```rust
let mut length_bytes = [0u8; 8];
if !read_stream(unsafe { &*stream }, length_bytes.as_mut_slice()) { ... }
let length = u64::from_le_bytes(length_bytes);
let mut read_buffer: Vec<u8> = Vec::with_capacity(length as usize);
```

Eight bytes are read from the stream and used directly as an allocation size, with no upper bound
and no cross-check against how much data the stream actually holds. `state-invalid-random` feeds
random bytes, so the prefix became ~1.02 **exabytes**, `Vec::with_capacity` called
`handle_alloc_error`, and the process aborted.

A host that loads a truncated project file gets the same outcome. The correct behaviour is to
reject the state and return `false`.

### Reproduction

`plugin_robustness.rs::malformed_state_is_rejected_rather_than_aborting_the_plugin` loads a blob
with a hostile length prefix through the player's ordinary state path. The patched copy refuses any
declared length above 512 MiB. Accepted lengths use `try_reserve_exact`; allocation failure, an
unrepresentable length or a short stream causes `false` to be returned, and the read is limited to
exactly the declared span. Vendored unit regressions force the accepted 512 MiB reservation to fail without
aborting and verify that trailing stream bytes are not consumed.

## nice-plug: unequal main ports are declared as an in-place pair

**Status:** defect in `nice-plug 0.3.0`. **Fixed locally** in
[`vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (patch 5).
**Impact, before the patch:** a mono-in, stereo-out effect advertises an impossible in-place pairing;
`clap-validator` refuses to process it.
**Fixable from plugin code:** no — the wrapper owns CLAP audio-port declarations.
**Regression:** `clap-validator validate "target/bundled/mxm-chorus-06.clap"` passes
`process-audio-basic-in-place` and `layout-audio-ports-config`.

`nice-plug-0.3.0/src/wrapper/clap/wrapper.rs`, `ext_audio_ports_get` (around line 2824), paired main
port zero whenever both directions existed. CLAP permits that pair only when input and output have
the same shape. The patch keeps the pairing for equal channel counts and reports
`CLAP_INVALID_ID` otherwise.

## nice-plug: the first sample-accurate parameter event is applied before its offset

**Status:** defect in `nice-plug 0.3.0`. **Fixed locally** in
[`vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (patch 6).
**Impact, before the patch:** the first nonzero-offset automation event in a block takes effect at
sample zero. A low-high-low trigger pulse can fire early, while automation without block splitting
can lose it entirely.
**Fixable from plugin code:** no — only the wrapper can split processing before applying host events.
**Regression test:**
`plugins/mxm-para-07/host-tests/tests/behaviour.rs::trigger_parameter_edges_are_sample_accurate_and_restore_only_a_level`.

`nice-plug-0.3.0/src/wrapper/clap/wrapper.rs`, `handle_in_events_until` (around line 1052), already
looked ahead before applying every event after the first unread one, but applied that first event
before checking its timestamp. The patch gives it the same stop-predicate check: render the leading
span, then resume at the event and apply it there.

## nice-plug: an out-of-range event timestamp becomes an out-of-bounds audio split

**Status:** defect in `nice-plug 0.3.0`. **Fixed locally** in
[`vendor/nice-plug`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) (patch 7).
**Impact, before the patch:** a parameter or transport event beyond `frames_count` is returned as
`block_end`; `BufferManager::create_buffers` then constructs slices beyond the host's audio buffers.
**Fixable from plugin code:** no — event splitting and host-buffer slicing belong to the wrapper.
**Regression test:**
`mxm_state_tests::out_of_range_split_event_is_clamped_before_buffer_partitioning`.

`handle_in_events_until` compared and returned `raw_event.time`, while `handle_in_event` clamped the
timestamp only later when converting the event. That later clamp cannot repair a split point already
used to form an audio slice. The patch clamps the absolute timestamp before comparison and return,
then derives segment-relative timing with saturating subtraction from the same clamped value. The
regression puts a split-causing event beyond a 64-frame buffer, requires the first segment to end at
sample 63, and proves the event is consumed at relative sample zero when processing resumes there.

---

## The player and a plugin editor both use OpenGL, in one process

**Status: the shared-GL problem is gone** — `apps/mxm-player` renders through wgpu, see *The fix,
applied*. **But it was not the cause of the white editor**, which is a separate, measured mechanism
recorded in the next entry. This one is kept because the upstream interaction is real for any host
that does share a GL context with a plugin.
Diagnosed on Windows; not investigated on Linux or macOS.

`apps/mxm-player` renders with `eframe`'s glow backend. mxm-mono-01's editor renders with
`nice-plug-egui`'s `opengl` feature, which is `egui-baseview` on WGL. When the editor is open, both
paint from the same thread, into the same process, through the same global "current GL context".

They are not isolated from each other:

- eframe binds its glutin context every frame on Windows — `glow_integration.rs:1010` disables the
  usual early-out with a note that *"we cannot do this early-out on Windows"*.
- baseview's `make_not_current` is `wglMakeCurrent(null, null)`
  (`baseview-0.3.2/src/wrappers/win32/window/wgl.rs:50`). It unbinds **whatever context is
  current**, not only its own.

### What it looks like

Intermittently, and worse the more the player repaints:

- the editor renders **white**;
- the player shows **garbage** after the editor closes;
- the player shows a **black window** after the editor is hidden, if nothing repaints it.

Two of those three turned out **not** to be this issue, which is why attributing all three to the GL
contention was a mistake worth recording:

- The **white editor** is thread starvation, not context contention. It reproduces with the player
  on Vulkan and no OpenGL in it at all. See the next entry.
- The **black window** is reactive repainting: a foreign window appearing or disappearing
  invalidates ours without giving egui any input to react to. Fixed with a `request_repaint` on
  every editor lifecycle change.

That leaves **garbage after close** as the only symptom still plausibly caused by the GL contention
described here — and it has not been reproduced since the wgpu swap, on a sample of one session.

### The fix, applied

**Stop the two sharing a graphics API.** A host on D3D12, Vulkan or Metal and a plugin on OpenGL
share no state at all, which is why GL plugins work in commercial DAWs.

`apps/mxm-player` now renders through `eframe`'s **`wgpu`** backend. The alternative — moving
`nice-plug-egui` to its `wgpu` feature — would have put the same large dependency in **every plugin
we ship** to fix one host, so the cost lands in the one binary instead.

Three things the change needed beyond the feature flag, each of which would have made it silently
incomplete:

- **wgpu has its own GL backend.** Asking for wgpu is not by itself asking for something that is not
  OpenGL. `main.rs` restricts `Backends` to D3D12, Vulkan and Metal, and logs the adapter actually
  selected at startup — otherwise a fallback to GL would restore the conflict with no visible sign.
- **No usable adapter is now fatal**, where a software OpenGL implementation would have limped. That
  is an accepted cost, and `main.rs` reports it in those terms rather than failing opaquely.
- **The unfocused-servicing workaround is lifted.** `logic()` runs only when eframe draws, so an
  unfocused player used to stop servicing the sequencer and MIDI; repainting to fix that made the GL
  contention constant, which is why the stall was the accepted symptom. With nothing to contend for,
  the player now requests repaints while the transport is playing or an editor is visible.

`egui_kittest`'s GPU path is behind **its own** `wgpu` feature, which this repository does not
enable, so the test harness initialises no adapter and CI needed no change.

---

## A plugin editor renders white when the host repaints too often

**Status: understood and worked around**, in `apps/mxm-player`.
**Impact:** the plugin's interface is a blank white rectangle. It looks like a plugin bug and is not.
**Fixable from our code: yes** — it is a host-side scheduling choice.
**Fixable from plugin code: no.**

### It is not the OpenGL issue above, and that is the point

The entry above attributed this to two OpenGL painters sharing one global current context. **It
reproduces with the player rendering through Vulkan, with no OpenGL in the player at all** — so that
explanation is wrong for this symptom. The player logs its adapter at startup precisely so this kind
of claim can be checked rather than assumed.

### The mechanism

A plugin's floating editor is a window **on the host's own thread**. Its `WM_PAINT` and other
messages are dispatched by whatever message loop is running on that thread — the host's.

If the host asks for frames at or below the display's frame interval, its event loop never idles,
and the editor's messages are never dispatched. The window is created, sized and visible, and never
paints. White.

### Measured

Windows, 60 Hz display, wgpu on Vulkan, mxm-mono-01's editor auto-opened and captured by its
`Baseview-<uuid>` window class:

| Requested repaint interval | Editor |
|---|---|
| 16 ms | **white** |
| 20 ms | **white** |
| 25 ms | correct |
| 30, 33, 50, 100, 150, 250, 500 ms | correct |
| no repaint requested at all | correct, but the interface feels sluggish |

The knee sits just above the display's ~16.7 ms frame interval, which is what identifies the
mechanism. **It therefore moves with the refresh rate** — a 144 Hz display would put it lower — so a
host must leave real margin rather than tuning to one machine.

### The trap on the other side

Not repainting is not a fix. The host's `logic()` runs only when it draws, so an unfocused player
stops servicing the engine: the sequencer stalls and MIDI presses stop reaching the selected step.
Both failures are caused by the same coupling, in opposite directions.

`apps/mxm-player` therefore requests frames at **50 ms while an editor is visible** — 2.5× the
measured knee — and at the ordinary frame rate when none is open, since nothing then shares the
thread. `MXM_SERVICE_MS` overrides it for diagnosing a machine whose knee is different.

### Reproducing it

No headless test can see this: it needs a real editor window, and the evidence is what the window
looks like. `MXM_AUTO_EDITOR=<clap-id>` makes the player load a plugin and open its editor
unattended, which is enough for a script to launch, raise the editor by its window class, screenshot
it and count distinct colours — a white editor has almost none.

**Capture the editor window, not the screen.** The player overlaps it, and measuring the overlap
reports the player's pixels and a healthy-looking result for a completely blank editor. That mistake
cost a round of wrong conclusions here.

---

## Floating editors resize slowly

**Status: fixed**, in our floating-window extension to `vendor/nice-plug`, not an upstream
floating-editor regression: upstream did not offer that capability.
**Impact:** slow/choppy drag-resizing of the synth editors in MXM Player.
**Fixable from our code: yes**, at the shared wrapper boundary, not through per-synth layouts.
**Owner confirmation:** after the seven release bundles were rebuilt, the owner reported
“It is running smooth as butter.” This confirms the reported symptom is resolved in their setup;
no measured FPS, DPI breakdown or per-instrument manual sweep was supplied.

### Cause and invariant

`ClapHostCallbacks::request_resize` in `vendor/nice-plug/src/wrapper/clap/wrapper.rs` forwarded
every native floating resize to `host.gui.request_resize`. That CLAP callback requests a
**parent's client area**; a floating editor has no parent, and baseview has already resized it.
The player queued and echoed the request while waking its GUI on each drag event, bypassing its
50 ms editor-visible service cadence and making the host compete with the editor on their shared
GUI thread.

**Floating resizes are accepted locally; embedded resizes still negotiate with the host.** Capture
`is_floating` when constructing the callbacks, including callbacks during spawn. Do not remove
this distinction, lower `MXM_SERVICE_MS`, or change layout/renderer defaults to compensate for
unnecessary host wakeups. [Vendor DOX](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md) owns the upgrade and regression gate.

### Evidence and regression guard

- The broken Windows release bundles for mono-00 and mono-01 generated **24 host round-trips in
  24 native resizes**. The regression failed against those bundles before the fix.
- `apps/mxm-player/tests/editor_resize.rs` now opens, resizes 24 times, closes and reopens **all eight
  plugins**, requiring accepted window sizes and **zero host resize round-trips**. The recorded debug
  and release run covered the seven bundles shipped at the time, all with zero resize round-trips and
  validator failures; the current eight-bundle inventory remains the per-profile release gate.
- The headless layout benchmark measured **0.254 ms/frame**, or **0.116 ms with a 32-point quantum**,
  on representative card bodies. That was not a diagnosis of native-window slowness. Quantization
  and culling remain off by default; neither was needed for the confirmed fix.
- Invoke the native test explicitly after rebuilding all bundles, once per profile:
  `cargo test -p mxm-player --test editor_resize -- --ignored --nocapture`.
  A normal workspace test run skips it. Keep it across dependency upgrades and add new editors
  to its inventory; a validator or panel-layout test is not a substitute.

The native driver is Windows-only and counts callbacks, not displayed FPS. Linux/macOS and
embedded DAW resizing remain unverified by this repair; the owner's confirmation does not claim
those gates. The separate host-starvation workaround above remains in place.

---

## Windows takes the audio device away from the player

**Status: understood**; not a defect in the player or in CPAL, and handled in `apps/mxm-player`.
**Impact, before the handling:** the status bar read *audio device stopped — Failed to get current
padding: OS Error -2004287484 (FormatMessageW() returned error 317)*, audio stayed dead until a
plugin was loaded or a rescan run, and Play answered "playing from step 1" over silence.
**Fixable from our code: the reaction, yes; the cause, no.** Windows-only in the first instance;
the other platforms take devices away too, and CPAL classifies those the same way.

### Symptom

Reported as "when loading and unloading the different synths". It is not: fifteen plugin switches
with notes, editors and rescans over the CLI, and twenty build/play/drop cycles on the bare CPAL
device, never produced it. It appears when something *outside the player* reconfigures or takes over
the endpoint, and a load happened to be the next thing the user did.

### Diagnosis

-2004287484 is `0x88890004`, WASAPI's `AUDCLNT_E_DEVICE_INVALIDATED`. Windows raises it on every
call after the audio engine has disconnected a session — `DisconnectReasonExclusiveModeOverride`,
`DisconnectReasonFormatChanged`, `DisconnectReasonDeviceRemoval`, `DisconnectReasonServerShutdown`.
CPAL's worker loop meets it in `IAudioClient::GetCurrentPadding`
(`cpal-0.18.2/src/host/wasapi/stream.rs`, `get_available_frames`, the "Failed to get current
padding" context), calls the error callback, and exits; no further data callback runs. The
"FormatMessageW() returned error 317" is only `windows-core` failing to find text for the HRESULT.
`cpal-0.18.2/src/host/wasapi/mod.rs:57` maps the code to `ErrorKind::DeviceNotAvailable`, and
`AUDCLNT_E_DEVICE_IN_USE` — what `Initialize` answers while another application holds the device —
to `ErrorKind::DeviceBusy`; `audio::describe` builds its sentences on those.

On the machine it was found on, Windows' *Microsoft-Windows-Audio/PlaybackManager* log carried two
event-23 entries (*Released exclusive mode resource false*) at the moment the stream died, with no
device-state change logged around them. Ordinary stream teardown logs nothing there — the twenty
CPAL cycles produced no entry — and an exclusive-mode open from another process produces exactly
one per session it disconnects. Two entries is two shared-mode sessions disconnected at once: the
player's and one other application's. The likeliest sources there are an exclusive-mode open, or
the interface's sample rate or clock being switched from its own mixer or by an ASIO host, either of
which reconfigures every WDM endpoint the device exposes.

### Reproduction

From a second process, on the same endpoint the player is using:

```text
IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eConsole)
IMMDevice::Activate(IAudioClient)
IAudioClient::IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, 16-bit PCM at the mix rate)
IAudioClient::Initialize(AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                         default period, default period, that format)
IAudioClient::Start(); hold; IAudioClient::Stop()
```

A one-millisecond hold reproduced the status-bar text byte for byte. Holding for three seconds is
what shows the handling: the death named in words, refused reconnect attempts counted with the
device's reason, Play refused, and the stream back within the backoff once the hold ends.

### What the player does now

`apps/mxm-player/AGENTS.md`, *A dead stream is not a wedged plugin*: the reason is described, the
engine reconnects with a doubling backoff and never gives up, Play refuses while the stream is dead,
and a start refused by the device deactivates the plugin so the next attempt can activate it again.

## Four examples shared one output filename, and `cargo test --workspace` raced on it

**Ours, not upstream, it predated the measurement crate that exposed it, and it is fixed
(2026-09-13). Recorded because of how long it went on looking like something else.**

`crates/mxm-creative-sampler-dsp`, `crates/mxm-grain-fx-dsp`, `crates/mxm-mono-01-dsp` and
`crates/mxm-mono-03-dsp` each carried `examples/render_demo.rs`; `crates/mxm-mono-01-dsp` and
`crates/mxm-mono-02-dsp` each carried `examples/filter_spike.rs`; `plugins/mxm-bucket-delay` and
`plugins/mxm-shimmer` each carried `examples/preset_audit.rs`. Cargo writes every example in a
workspace to one flat directory keyed by target name alone, so all four linked to
`target/debug/examples/render_demo.exe`.

Cargo says so itself, and the warning has been in the build output all along:

```
warning: output filename collision at target/debug/examples/render_demo.exe
```

Most of the time the jobs happen not to overlap. When two do, the second linker cannot open the file
the first is writing and the build dies:

```
LINK : fatal error LNK1104: cannot open file '.../target/debug/examples/render_demo.exe'
```

**It is a race, so it fails intermittently and on a different crate each time** — which is what makes
it look like a lock or an antivirus problem rather than a name collision. Observed on
`mxm-shimmer`'s `preset_audit`, then `mxm-mono-03-dsp`'s and `mxm-grain-fx-dsp`'s `render_demo`,
across three runs of the same unchanged tree on 2026-09-12.

**The quiet failure is worse than the loud one.** A linker error at least stops the build.
`cargo run -p mxm-mono-03-dsp --example render_demo` did not: it ran whichever of the four binaries
won the last race, so a listening pass could audition mxm-mono-01 while the person at the keyboard
believed they were judging mxm-mono-03. Two crates had already been named defensively around this —
`mxm-mono-00-dsp`'s `system_demo` and `mxm-poly-06-dsp`'s `juno_demo` both say so in their own
headers — which meant the collection was working around the defect instead of fixing it.

**Fixed by renaming, 2026-09-13.** Every example now carries its crate or its machine:
`creative_sampler_render_demo`, `grain_fx_render_demo`, `mono_01_render_demo`,
`mono_03_render_demo`, `mono_01_filter_spike`, `mono_02_filter_spike`,
`bucket_delay_preset_audit`, `shimmer_preset_audit`. `[[example]] name = ...` in each manifest
would also have worked and was rejected: it puts the real name somewhere other than the file, which
is how the two drift. The rule is now in root `AGENTS.md` *Naming* and in
`docs/adding-an-instrument.md`, because the instrument guide is what kept minting `render_demo.rs`
for each new instrument.

**Verify it stays fixed:** `ls */*/examples/*.rs | sed 's|.*/||' | sort | uniq -d` prints nothing,
and `cargo build --workspace --examples` no longer prints `output filename collision`. `cargo test
--workspace` now passes **without `-j 1`** — 122 suites, 2401 tests, full parallelism (2026-09-13).

## Two crates declared an MSRV they could not build on

**Ours, pre-existing, fixed 2026-09-13 — and recorded because the *way* it hid is the lesson.**

`crates/mxm-mono-pr1-dsp` and `crates/mxm-creative-sampler-dsp` both declared `rust-version = "1.87"`
and both used **`let` chains** — `if cond && let Some(x) = opt` — which stabilised in Rust **1.88**.
On 1.87 they did not compile at all:

```
crates/mxm-mono-pr1-dsp/src/keyboard.rs:192      error[E0658]: `let` expressions in this position are unstable
crates/mxm-mono-pr1-dsp/src/keyboard.rs:257      error[E0658]
crates/mxm-creative-sampler-dsp/src/lib.rs:2000  error[E0658]
```

**Fixed by raising both to 1.88** (the owner's call, 2026-09-13): the code already required it, 1.88
is from mid-2025, and the honest floor is the one the code actually has. The three `let` chains stay
as written. All thirteen crates have since been compiled against their declared floor and all thirteen
pass.

**Why it hid, which is the part worth keeping.** Cargo enforces `rust-version` when it resolves
*dependencies*, never when it compiles your own crate — so on a current toolchain a wrong floor is
completely silent. The number sat in two manifests as a claim nothing exercised, and it was found only
because a new crate's own 1.87 claim was being verified and the others were checked while the
toolchain was installed.

**So a floor that has not been compiled against is a guess.** Root `AGENTS.md`'s *MSRV is per crate*
section now carries the command and says to run it whenever a crate takes a language feature it did
not have before — which is the shape this will always take: syntax arrives, the compiler in use
accepts it, and the declared floor quietly stops being true.

**It happened again within three days, and the same way (found 2026-09-16, fixed the same day).**
Two more `let` chains arrived in code held to 1.87. `2b1b608`, the `mxm-mono-00` routing conversion,
added one to `crates/mxm-mono-00-dsp/src/voice.rs` (`sync_edge`). `f44be89`, a factory repair that
added nice-plug patch 8, added two to `vendor/nice-plug/src/wrapper/clap/wrapper.rs`. The vendored
crate declares 1.87, and `tests/clap-fixtures`' nice-plug-output package inherits that floor through
it. The chains were written across two lines, `if let Some(x) = opt` then `&& cond` below it, so a
single-line grep for `&& let` misses them. Compiling every 1.87 crate found all three. **Fixed by
nesting**, because all of this code must build at 1.87:

```bash
CARGO_TARGET_DIR=target/msrv-1.87 cargo +1.87.0 check --locked --all-targets --keep-going \
  -p <every crate whose rust-version is 1.87>
```

The check has to cover the whole set, and it has to compile: neither the conversion's review nor
its closeout ran the floor, and nothing else would have caught it.
