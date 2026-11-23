# NOTES — crates/mxm-ara

The reasoning behind [`AGENTS.md`](AGENTS.md)'s rules. Written 2026-11-23, at stage 5 of
newDAWn's `plans/plan-pitch-plugin.md`.

## Bindings

- **One file per architecture, not per OS.** `ARAInterface.h` packs its structs with
  `#pragma pack(push, 1)` on x86 and `pack(8)` on ARM, so x86_64 has packed layouts and aarch64
  natural ones (their sizes differ: `ARAInterfaceConfiguration` is 20 bytes on x86_64, 24 on
  aarch64). Windows and Linux on x86_64 share a layout; bindgen's compile-time layout checks hold
  each file to its target.
- **Read as C++17**, with `--disable-name-namespacing`: there ARA's 32-bit enums
  (`ARA_32_BIT_ENUM`) have the fixed underlying type `ARAInt32` on every compiler, where C leaves
  an anonymous enum's type to the compiler. `--translate-enum-integer-types` makes the
  constants' types `i32`/`u32`, not `c_int`/`c_uint`.
- **No doc comments** (`--no-doc-comments`): the header's comments hold indented blocks rustdoc
  would run as doctests. The header itself is beside the bindings.
- The aarch64 file's layout checks have not been compiled here (no aarch64 std on this machine);
  the owner's macOS batch compiles them.

## The renderer's guard

- It is ARA TestPlugIn's (`rendererWillAccessModelGraph`): enter by counting in, then look at the
  flag; an edit lowers the flag, then waits for the count to drain. Sequentially consistent
  operations make it a Dekker pair, so either the renderer sees the flag down or the edit sees
  the renderer in.
- **A refused renderer counts itself out at once.** In the TestPlugIn a refused
  `rendererWillAccessModelGraph` leaves the count raised (its caller only calls
  `rendererDidAccessModelGraph` when admitted), so the next edit would spin forever.
- Each extension has a guard of its own over its region lists, so assigning regions to one
  instance's renderer doesn't silence the others; a renderer holds both guards while it reads.
- An edit the host makes outside `beginEditing`/`endEditing` is reported, as ARA_Library reports
  it, but still made with the guard closed: ARA_Library would change the graph under a renderer.

## Audio readers

- **mxm-ara cancels and joins, not the plug-in.** A reader's host ref sits behind a lock that a
  read holds; cancelling takes the lock (waiting out the read under way: the join), destroys the
  host's reader and leaves the `AudioReader` dead (`ReadError::Cancelled`). So "the read is joined
  before the call returns" holds by construction, whatever the plug-in's worker does.
- ARA lets `createAudioReaderForSource` and `destroyAudioReader` be called only from that
  source's management calls or from `endEditing`, on the model thread. A reader dropped on a
  worker can't destroy its host reader there, so it is marked abandoned and destroyed at the next
  such call.

## Archives

- **The container is the document controller's**, so filtering, persistent-ID mapping and "a
  newer schema kept verbatim" are done once, here, and the plug-in only writes and reads its own
  payloads (the plan's schema-versioned serde, keyed by persistent ID).
- Restore order is sources, then modifications, then the document's data, whatever the archive's
  order: ARA wants an object's parents restored first, and the document data last.
- A restore filter or a NULL filter is applied as ARA_Library's `RestoreObjectsFilter` applies it:
  archived IDs, mapped to current IDs where the host maps them, then to the objects carrying
  those IDs; anything else is ignored.
- A malformed payload (the controller's restore hook returns false) makes the host's restore call
  return false while the other objects are restored: ARA asks the plug-in to recover as much as
  it can and the host to tell the user.

## ARA semantics resolved one way

- **Roles the host doesn't know are assumed**, as ARA_Library assumes them (a host that knows
  none gets all three).
- **`notifyModelUpdates` while editing is refused** (reported): ARA says the host calls it
  "whenever not editing nor restoring".
- **A playback region without a region sequence** (an ARA 1-shaped struct) is reported, since
  ARA 2 requires one, and kept with no sequence rather than refused.
- **Destroying an object that still has children** (a source with modifications, a sequence with
  regions) is refused, keeping the graph whole; a region still in a renderer's list is reported
  and taken out of every list before it goes.
- **Notifications about an object queued before it was destroyed are dropped** at its
  destruction: a new object could take its address before the host polls.
- **The host's assert function is installed before `initializeARAWithConfiguration` validates
  its configuration**, so the host hears a refusal of that very call.
- **`getPlaybackRegionHeadAndTailTime` trusts its region ref.** It may come from the audio thread,
  where checking against the graph would need the guard or a lock; ARA_Library checks it only in
  debug builds.

## The test host

- One `TestHost` lives at a time in a process: ARA's initialization is per library, so tests in
  one binary take turns (a second `TestHost::new` waits for the first to drop).
- It fills every slot of an interface even past the `structSize` it declares, so a plug-in that
  reads past the size calls a function the host never declared, and the tests see it.
