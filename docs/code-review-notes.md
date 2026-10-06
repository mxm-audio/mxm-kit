# Code-review notes — recurring plugin failure patterns

Use these notes when reviewing a new instrument, repairing a plugin, or preparing a release.
They are review prompts, not a shared DSP design or proof that every instrument is defective.
The root contracts (since the split, each repository's root `AGENTS.md` and the
[collection rules](collection-rules.md)), [plugin conventions](plugin-conventions.md),
[design system](MXM_DESIGN_SYSTEM.md), and vendor contract (since the split, the nice-plug fork's
[`PATCHES.md`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md)) remain authoritative.

## Evidence and applicability

These patterns were exposed by `mxm-para-07` implementation reviews. At the reference checkpoint
`da73171e1f2db4a5a2a9ac635e0ee2882f607ffe` on `factory/roland-sh-7`, candidate repairs and regressions
exist; that checkpoint is **not final review acceptance or a release**. The corresponding `main`
baseline is `d258d47cb9987f2945b1b4c4ce426fcc8ab29110`. The mxm-mono-pr1 review later added the
actual-discontinuity sync, normalized bend, all-layout host, broad golden, complete editor-paint,
shared visual-token and delivery-drift prompts below. A branch fix does not fix an installed bundle.
Inspect the revision actually being reviewed; do not assume later integration from these notes.

*Since the split (2026-10-06):* these notes were written in the monorepo. A `crates/<product>-…` or
`plugins/<product>/…` path below is in that product's repository
(`https://github.com/mxm-audio/<product>`), an `apps/mxm-player/…` path is in
[mxm-player](https://github.com/mxm-audio/mxm-player), `crates/mxm-measure` and `crates/ui` are in
this repository, and `plans/…` is in the private archive.

- **Shared dependency risks:** sample-offset automation, input/output queue saturation, malformed
  state loading. Audit every plugin using the affected nice-plug wrapper, including effects where
  applicable. A wrapper fix requires rebuilding and verifying all affected bundles.
- **Shared UI/test risks:** geometry, zoom, selectors, accessibility and false-positive harnesses.
  Shared components can be fixed centrally; local layouts and tests still need individual checks.
- **Per-instrument audit candidates:** note ownership, termination, parking/wake, non-finite inputs,
  antialiasing, and mix continuity. Similar architecture is a reason to inspect, not a diagnosis.
- **Coverage status:** these notes do not establish a completed collection-wide audit of
  `mxm-mono-00`, `mxm-mono-01`, `mxm-mono-02`, `mxm-mono-03`, `mxm-mono-08`, `mxm-mono-pr1`,
  `mxm-poly-06`, `mxm-chorus-06`, `mxm-folded-spring`, `mxm-bucket-delay`, or `mxm-shimmer`. Their
  applicable cases
  remain **UNAUDITED by this review series** unless a separate review supplies evidence. Existing
  tests are not being declared absent or invalid.

For each applicable item, record **finding**, **covered** (test/command, artifact revision/profile,
platform and result), **not applicable** (reason), or **unverified**. A passing static review is not
an executed test. Keep findings in the owning review/plan and regression rules in local DOX; do not
turn this page into a rolling transcript. A known applicable defect must not be silently waived at
release, and an unverified case must not be reported as passed.

## 1. Host events and realtime safety

| Look for | Distinguishing proof |
|---|---|
| Trigger-like parameters polled once per unsplit block; wrapper automation applied before audio regardless of event offset | Through the actual plugin wrapper, send low/high/low at nonzero offsets in one block. Require exactly one correctly timed trigger, block-partition invariance, and no trigger caused solely by state restoration. Check both the plugin's sample-accurate setting and wrapper implementation. |
| A raw event timestamp is returned as an audio split point before its later event conversion clamps it | Clamp timestamps in absolute buffer coordinates before any split-point comparison or return, not only when converting events afterward. Derive segment-relative timing from that clamped value without unchecked subtraction. Send a split-causing event beyond `frames_count` and prove no segment can exceed the host buffers. |
| A large DSP value held by value on a control-thread path — `activate`, a state restore, a capture — that the release bundle survives | Debug keeps several copies of a by-value struct per frame, so a path that fits in release can overflow a host's 1 MB Windows main thread in debug. `mxm-drum-machine`'s Resample capture did: its ~100 KB engine, by value in two nested frames, crashed the debug bundle's `param-fuzz-basic` with `0xc00000fd` while release passed. Validate the debug bundle, not only release. To find the frames, disassemble the debug DLL and symbolize its largest `__chkstk` reservations; to hold the fix, run the path on a `std::thread::Builder::stack_size(1 << 20)` thread in a debug test. Box the large value. |
| A queue that is merely preallocated rather than hard-bounded, or whose raw host-event traversal still follows the reported count | Bound storage, raw-event inspection and overflow-search work separately, and test a hostile count many times capacity rather than only the first overflowing push. Exceed input **and** output capacities, including many events at one sample. CLAP event count is not bounded by audio-frame count. Use an allocation-guarded debug artifact or an explicit callback-thread counter; prove no growth, allocation, abort, or unbounded work. |
| Overflow priority disagrees with consumer release semantics, dropping NoteOff/Choke/termination CCs or a zero-velocity NoteOn used as NoteOff | Classify input releases against the consuming handler's semantics, including zero-velocity NoteOn where it means NoteOff; do not silently change output-event semantics. Establish and verify the target note as sounding in an earlier callback, then saturate a later callback ending in each termination form and observe silence/downstream release. A same-queue NoteOn can be evicted and make a silence assertion vacuous. The reference branch's `apps/mxm-player/tests/plugin_robustness.rs::a_zero_velocity_note_on_after_input_saturation_still_terminates_the_note` distinguishes this case. Specify deterministic overflow policy, ordering, and exhaustion of termination-only capacity; checking the first N NoteOns is insufficient. |
| Only input saturation is tested, while `send_event` changed too | Use a test-only nice-plug emitter that actually overflows the output path. Removing either input or output guard must fail its own regression. Keep fixtures out of distributable plugin sets. |
| Bounded state length still followed by infallible large allocation, unbounded reads, or malformed decompression | Fallible reservation, exact declared-span reads, rejected truncated/oversized/corrupt state, and safe allocation-failure handling. A size cap alone does not prevent allocation failure from aborting the host. |
| Asset-bearing preset/state is preflighted but then publishes content before or after a visible parameter burst | Prove a process-boundary commit barrier under a host consuming the burst concurrently: no callback may render half-new assets with half-old metadata in either order. A sequence of GUI calls is not an atomic host transaction. Rejection must emit no gestures, preserve the working sound/identity, and not mark the rejected file loaded. **The shape that satisfied this** (mxm-creative-sampler, 2026-09-10): split publication into a `stage` that does every allocation and destruction off audio and a `commit` that is one atomic store, and give the callback a single read guard for the whole block — then the boundary *is* the barrier. Whoever stages must commit, and each entry path says who: the shared preset seam commits after the gesture burst, the editor after an import's reset gestures, and a host state restore not at all, because the wrapper already writes parameters before fields. A held guard across a whole prepare/write/commit sequence is the test. |
| A deferred transaction API is complete and unit-tested, but no shipped UI path owns or drives it — or its acknowledgement recaptures live state and competing durable operations bypass its ordering | Treat production reachability and one cancellation domain as part of delivery, not integration glue to infer from shared tests. Through the real editor app-bar surface, activate preset selection and Init and require both to dispatch background preparation; while Pending require response/parameters/identity unchanged, every Save action disabled even when its naming row was already open, and newer selection still live. Begin each ordinary source/model operation and require it to retire unpublished preset work; a stale worker returning “not staged” must not strand Pending. After Ready require one bounded publication and keep identity unchanged; edit one parameter before publication and another after it, then invoke the plugin's actual `process()` callback. Acknowledgement must install the retained target baseline and publication-time durable fingerprint, making both edits Modified rather than snapshotting them into Clean. Latest-wins must compare a monotonic per-parameter **base** edit generation, not final value equality or a generic value callback: nice-plug's callback also fires for modulation. Sample unmodulated bits at every stated observation boundary, drive the modulation seam and require the preset target still to publish, then move a base away and back across observations and require the preset not to overwrite it; document any between-observation bound. Host-state restore must advance the same durable-response ordering generation; complete an older worker after restore and require its parameters, response and identity all to remain inert. Any new trait hook used only by that opt-in route needs an honest compatibility default for existing continuous adapters. Run every existing synchronous preset consumer, discovered from manifests rather than a remembered shortlist, to prove the opt-in API changed no default path. |
| A deferred write snapshots a value and revision separately, then publication trusts only the revision | A base edit can land between reads and be incorporated into the captured revision while the captured value stays old. Require both unchanged unmodulated bits and unchanged revision at publication, or capture the pair atomically. The distinguishing regression injects an edit from the revision-read seam itself, leaves the resulting revision equal at publication, and requires the intervening value not to be overwritten. |
| Any newer durable request invalidates a preset identity that has already crossed its publication boundary | Separate unpublished worker ordering from published identity acknowledgement. An ordinary source/model successor after publication must let callback acknowledgement install the published preset baseline, after which the successor makes it Modified; host restore or rollback must still cancel it. Regress publication → ordinary model/source start → callback acknowledgement → successor commit, plus host restore at the same boundary. |
| A newer deferred request replaces the single transaction slot after its predecessor has already published but before callback acknowledgement | Publication makes response and parameters durable before identity is allowed to change, so the predecessor needs an independent awaiting slot. Block successor publication until that slot is acknowledged; cancellation or rejection applies only to the successor. The distinguishing regressions publish A, begin B before A's callback, cancel and reject B separately, then require A's committed response, parameters and Clean identity after acknowledgement. |
| A state migration canonicalizes parameters or durable content but leaves a loaded-preset baseline or fingerprint in the old schema | Compatibility includes the persisted identity used to decide Clean versus Modified. Begin both a unit test and a real-bundle host test with `loaded = Some`, the exact legacy baseline and the legacy content fingerprint; restore the state and require the same preset name to remain Clean against the canonical content and all neutral additions. Migrate only an exact legacy shape whose fingerprint matches that state's content; a mismatch must remain Modified (or be deliberately cleared), never be laundered into Clean. |
| A "recipe" preset that preserves loaded content still writes the parameters that *describe* that content | A root note, a region or a loop is a measurement of one recording. Applying it from a preset that brings no audio crops or detunes whatever is loaded, using numbers authored against something else. Declare those ids and skip them wherever the durable payload is absent; a preset that embeds its own audio must still place them. |
| A budget decision taken from a timing measured on a busy machine | **A wrong number gets a feature declined.** mxm-creative-sampler's grain pool was held at four by a measurement of 85% that was really 55%, taken while the same session was building and pushing; the correct figure made the pool the owner had asked for affordable outright. Timings are one process against one core, on a quiet machine, repeated. When a recorded figure "does not reproduce", suspect the new measurement as readily as the old one, and say which conditions each was taken under. |
| An antialiasing method selected only because off-harmonic energy fell | Measure deviation from a phase-aligned, band-limited high-rate reference too. An antiderivative method can lower aliases while moving the nonlinear component in time enough to make the total waveform less faithful. `mxm-fx-curve` exposed the distinction: residual ADAA improved every alias figure, but on continuous smooth/corner curves its phase error cost more reference fidelity than direct lookup; only the equal-X discontinuity improved against the 7 kHz reference as well. Keep both metrics and a clean/reference control, then make any shape- or frequency-dependent selection explicit rather than calling the lower alias number universally better. |
| A per-grain or per-voice normalisation that divides by the **live** count of contributors | Dividing by how many grains happen to be sounding cancels exactly the build-up density is reached for — N grains come out as loud as one, so density changes texture and never weight — and it modulates the level every time one starts or ends. Normalise by the overlap the *parameters* request, clamped by the pool, and choose the exponent from whether the contributors are actually decorrelated. The distinguishing test renders the same patch at two densities and requires the coherent case to grow and the scattered case to hold; under the defective law the denser cloud measures *quieter*. |
| Two musical behaviours welded to one control because they were implemented together | A single "randomness" control that moves pitch, position, direction and pan at once cannot express the most common request — thicken without scattering. Splitting costs a parameter and no CPU. Ask what a player would want to do to one and not the other before treating a shared coefficient as a design. |
| A bounded granular/polyphonic design is declared affordable without rendering the product of all ceilings | Time the release build at maximum voices × loaded layers × live grains and the most expensive clean reader. Report one-core realtime share; if it misses, optimize or reduce the provisional ceilings before documenting them as supported. Typical-patch CPU does not bound the callback. |
| A documented finite routing transition implemented as an asymptotic one-pole, leaving nominally inactive DSP running forever | Require the exact endpoint after the declared sample count and prove the inactive branch stops being called. A time constant is not a duration. Silent/parked edits should settle immediately so the next excitation does not replay the old route as automation. |
| A stateful delay/read-direction selector hard-switches between unrelated read positions | Under a live tail, require the shifted branch to reach exact silence before the direction changes and to return smoothly afterwards. Do not clear large buffers on that sample unless changing nodes makes their history invalid. |
| A logical reset calls `Vec::fill()` on a sample-rate-sized delay during discrete automation | Invalidate history with a write generation or bounded valid-sample count so reset is constant-time; prove stale samples cannot return before the new write head has replaced them. Allocation-free is not enough when one audio sample performs O(sample rate) writes. |
| A convolution reset/park/wake clears every partition spectrum even though callback allocation is zero | Tag each history slot with a generation and treat old generations as zero. The regression snapshots populated response-sized spectra, resets short and long engines, proves backing cells were not rewritten, then requires exact silence through wake. |
| An asset processor ships a fixed embedded response while its editor and preset claims describe user acquisition or modelling | Import a real bounded mono and stereo file through the production gesture, preserve immutable path-free source samples plus reversible operation metadata, and prove stage/commit, malformed rejection, latest-request wins, host-state round trip and source-bearing user-preset round trip. The semantic plot must be drawn from the committed samples and operation region; a synthetic decay curve is not evidence. |
| Asset preflight validates coefficients but discards the prepared runtime object, so activation repeats allocation/FFT work | Stage canonical state **and the complete runtime engine** as one candidate; publication transfers that exact engine into activation. Arm deterministic rejection of any second preparation after staging, commit, and require the prepared engine to publish successfully. A test that only compares resulting coefficients cannot detect duplicate control-thread work or callback-deferred preparation. |
| A wrapper mutates parameters and persistent fields before reactivation, defers GUI state restoration to the audio callback, or resets the working processor after rolling back a rejected state | Hold the plugin lock across off-audio deserialize/reactivate, snapshot first, and restore/reactivate the snapshot before unlocking on any field or activation rejection. Persistent fields need a rejection signal; an infallible setter that silently keeps old content is not transaction success. Through two matched active real-bundle instances, excite the same tail, reject a valid but unpreparable asset plus changed parameters in one, require byte-identical prior state, then feed both silence and require bit-identical nonzero tail output. Serialized state alone cannot detect a rollback reset that erased runtime history. |
| A GUI-authored model uses a full state restore merely to mark non-parameter state dirty | A control-thread operation that already prepares and publishes canonical state must not hold the processor mutex while repeating that preparation. Under sounding audio, perform many expensive edits across multiple models with the debug allocation guard active; the callback must neither block nor allocate. `mxm-fx-curve` exposed the Windows signature: parking_lot's first contended lock initialized a 1,024-byte global parking table inside `process()` and aborted only while sound was running. The accepted shape publishes first, then sends a parameter-free transaction whose fields already equal `Params::serialize_fields()`; the wrapper treats that semantic no-op as dirty-only, calls `host.state.mark_dirty()`, and never locks, reactivates or resets. Real host/preset restores still take the complete rollback transaction above. |
| A persistent-field round-trip guard treats every changed serialization as rejection, so a legitimate clamp is deferred while audio and saved state disagree | Keep rejection as the default. Permit a mismatch only through a restore-scoped, exact-key acknowledgement emitted after the field successfully installs its canonical value; marks outside that restore must not leak. Through the real wrapper, load an over-budget value and save immediately **without activation**, requiring the applied maximum. Separately submit genuinely unpreparable content alongside changed parameters and require the complete prior state and live history to roll back; accepting every mismatch makes that second oracle fail. |
| A bounded source-duration claim is mistaken for a realtime callback envelope at every advertised sample rate | Derive a rate-dependent prepared-sample ceiling, reject unsupported rate × duration × time-scale combinations before activation/publication, and time the release engine at every envelope corner after filling all partition history. Measure both steady state and the actual maximum old/new transition, aligning the old engine so its heaviest scheduling boundary lands inside the crossfade; an incidental phase or one engine does not bound replacement. If the product promises a duration floor through a named rate, optimize scheduling until that complete corner passes rather than shrinking the promise. Report worst callback against that rate's block deadline; a 48 kHz average does not cover 96 or 384 kHz. |
| An expensive asset's processing envelope is enforced by rejecting host activation | Separate host compatibility from content availability. At every required finite positive host rate, activate a preallocated inert subsystem, pass exact finite dry audio, retain canonical content, publish a visible reason, and retry preparation on a later supported activation. The distinguishing real-bundle oracle crosses both range edges plus the validator's fractional and maximum rates under the debug allocation guard; plugin/editor proofs inspect the rejection telemetry and accessible reason. A clean refusal from `activate()` still fails the host contract. |
| A duration ceiling floors or independently recomputes a sample count that conversion rounds | Put duration-to-samples conversion and its finite/overflow checks in one helper used by both preparation and its ceiling. Regress full-limit preparation at fractional rates near an ordinary point and the promised upper-rate boundary; integer-rate tests cannot expose the one-sample disagreement. |
| Delay time can change after input activity expires, exposing samples that tail accounting has forgotten | Track valid nonzero delay history independently of the current read tap and extend activity when an edit can expose it. The distinguishing regression reaches nominal idle, increases delay, requires Tail until the stored impulse emerges and expires, then parks/wakes and rejects any ghost. |
| A file-duration cap rejects longer source material or fades the exact boundary case | Separate acquisition from canonical state: accept longer files, decode only the retained prefix, and apply a deterministic smooth fade to exact zero only when source audio exists beyond the boundary. The regression imports both `limit + 1` and exactly `limit`, proves the former is accepted/bounded/continuous/exact-zero, and compares every retained sample of the latter unchanged. An assertion that both outputs merely have the limit's length misses the off-by-one fade. |
| Offline response conversion uses linear interpolation and calls it preparation | Measure both wanted passband retention and above-target-Nyquist rejection at the largest downsampling ratio, plus duration/onset preservation in both conversion directions. Finite output and a correctly sized vector do not detect folded response coefficients. |
| A modified overcomplete STFT magnitude is inverted as though it were a consistent signal | Long low-level tails can acquire a cyclic `drrr` that sounds like a very short feedback delay even when spectral and envelope rulers are close. More Griffin–Lim iterations, coherent initialization and activation smoothing may retain or move it because none makes arbitrary overlapping frames jointly consistent. Audition the tail against the reference. Where NMF is used as a separator, form soft masks that sum to one, multiply the original complex STFT, invert each coherent component once, and transform components in time rather than repeatedly inverting a modified aggregate magnitude. |
| User-controlled response dimensions reach `vec!`, `with_capacity`, `collect` or unchecked FFT-history growth after a nominal size cap | Carry `try_reserve` failure through decode, transforms, resampling, coefficient spectra, runtime history and delay construction; reject atomically and preserve the old engine/state. The oracle supplies malformed and arithmetically impossible dimensions and verifies no partial publication; a maximum byte count alone does not prove allocation failure is recoverable. |

Evidence anchors on the reference branch: `apps/mxm-player/tests/plugin_robustness.rs`,
`plugins/mxm-para-07/host-tests/tests/behaviour.rs`, and `vendor/nice-plug/PATCHES.md` — since the
split, in [mxm-player](https://github.com/mxm-audio/mxm-player/blob/main/apps/mxm-player/tests/plugin_robustness.rs),
[mxm-para-07](https://github.com/mxm-audio/mxm-para-07/blob/main/plugins/mxm-para-07/host-tests/tests/behaviour.rs)
and [the nice-plug fork](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md).
Dependency edits belong under the existing vendor ownership (since the split, the nice-plug fork and
its [`PATCHES.md`](https://github.com/mxm-audio/nice-plug/blob/main/PATCHES.md)), never new DOX
inside dependency source. Update every affected file's patch marker and refresh instructions.

## 2. Note identity, termination and wake transitions

**Build an event-transition table before trusting an event match.** Include first/second/middle and
duplicate NoteOn, NoteOff, final and non-final Choke, All Notes Off, All Sound Off, same-offset
combinations, live source changes, and wake after host sleep.

- **A channel/key pair is not a press identity.** Identical ID-less duplicates can have different
  ages, tuning and ownership. Where the ledger distinguishes presses, retained high/low/external
  owners must retain that distinction too. Test tuning the oldest duplicate while the newest owns
  another lane, then release/choke handoffs. Follow the instrument's voice-ID and ID-less matching
  contract; do not invent one globally from the SH-7.
- **Retained pitch target is not post-glide pitch.** On release, retaining the keyboard target and
  owner need not freeze downstream integrators. Test release during glide and owner-specific bend,
  expression and tuning across final release and handoff.
- **Store controller position, not a patch-scaled result — and smooth the mutable scale.** A channel
  bend received under a 2-semitone range must become 12 semitones when the patch range changes
  without another MIDI event. Retain the normalized per-channel position and apply the current range
  at the control frame, but consume a signal-rate smoother there: multiplying a held nonzero bend by
  an unsmoothed range merely trades stale pitch for a broadband pitch step. Keep the unsmoothed
  target only for event-time bookkeeping. The distinguishing regressions edit range while held,
  require a bounded monotonic rendered pitch ramp and low early audio divergence, then return
  ownership across channels and require the retained normalized bend.
- **Pending events are state.** Queue a note assertion, discrete trigger or delayed retrigger, then
  terminate at the same offset and wake deliberately. No cancelled event may reappear. Specify
  separately what All Notes Off, final Choke and All Sound Off cancel; they are not synonyms.
- **Panic must clear the state its local contract owns**, including recursive noise/follower/filter
  state, held/lagged modulation, pending events and oversampling history—not only the final output.
  Compare deliberate wake after differing pre-panic histories while checking retained note ownership.
  Do not reset free-running hardware state on ordinary NoteOff merely to make this test easy.
- **A parked voice still needs wake bookkeeping.** Panic with HOLD already open; close and reopen.
  Panic in an autonomous mode; leave and re-enter. Test both selector threshold directions through
  the plugin fast path, not just by calling the DSP directly.
- **One event can feed several distinct circuit lines.** Keyboard depression, shared gate assertion,
  external gate, trigger, LFO phase reset, sine-delay reset and auto-bend are not interchangeable.
  Test legato depression with gate already high and external-writer changes with no physical
  keyboard assertion. Route termination only to its actual gate source: note accounting must not
  accidentally retrigger or cut an independent clock-driven envelope.

- **Never use whole-owner equality as a proxy for pitch-value change.** Identity, channel or tuning
  can hand off while the key target stays equal. Edge tests must perform release and choke handoffs
  while lagged state is still moving and after the legitimate target-change edge has been consumed;
  settled-state tests can conceal a duplicate edge.

## 3. Audible activity and numerical recovery

- **Tail accounting follows the complete output graph, not the first connected route.** A long
  filter envelope routed to an oscillator or cutoff is still inaudible after a downstream final VCA
  closes exactly; route connectivity alone must not keep exact-silent processing alive. Conversely,
  parking must not freeze hidden state that can later revive as a ghost tail. Test DSP activity and
  wrapper status together. The distinguishing regression gives the filter envelope a much longer
  release than the amplifier, proves its connected modulation while the VCA is open, then requires
  both connected and disconnected cases to become exact Inert/Normal together and rejects revival
  after rerouting.
- **A per-event accumulator must be multiplied by the interval, not divided by it — and it must sit
  before the early return, not after.** Grain's playhead advance read
  `travel * duration / (rate / density)`; reducing that expression to units gives `grain_size ×
  density`, the dimensionless overlap ratio, so a playhead meant to move at source rate crawled at
  1/2700 of it. Reduce any per-event accumulation to units before believing it — an expression that
  looks dimensionally busy is where this hides. Separately, the advance sat *after*
  `let Some(slot) = … else { return; }`, so it stopped entirely whenever the pool was saturated,
  which is exactly the setting the feature exists for; state that must advance regardless of whether
  an event was admitted belongs on the per-sample path, not in the spawn. Both halves were silent:
  the code compiled, stayed finite, kept the reader's only test ("is it audible") green, and the
  correlation probe reported the reader as *distinct* from the others precisely because it was
  broken. A reader with a position needs a test that the position moves.
- **A timing taken on a machine that is also building is not a measurement, and it can decline a
  feature.** The Grain pool was held at four because the probe read 85% of a core; on an idle
  machine the same command reads 55%, and the whole pool table beside it was uniformly ~1.55× high.
  A correct earlier figure was overwritten as "unreproducible" on that evidence and a bigger cloud
  was declined for a year of the project's calendar. **Before quoting any cost figure, establish
  that nothing else was running** — and prefer a *marginal* cost taken across two occupancies to an
  absolute taken at one, because the marginal separates the thing being priced from the engine's
  fixed work and is far less sensitive to a noisy machine.
- **A per-grain-per-sample transcendental or integer division is the granular failure mode, and the
  kernel is usually where it hides.** Measured across a ladder of grain kernels
  ([`oscillators/10-granular.md`](oscillators/10-granular.md) §10.6.1): computing a sixteen-tap
  windowed-sinc's coefficients per read costs **6.8×** a polyphase table, `rem_euclid` per tap costs
  **1.41×**, and a `cos()` window costs **1.06×**. The review question is not "how many taps" — eight
  tabled taps beat four computed ones on both cost *and* quality — but **"what in this inner loop is
  constant for the grain's life, or constant for the whole program, and is being recomputed
  anyway?"** Pan, rate, length, window increment and kernel coefficients are all in that class, and
  two engines in this repository got different answers to the same question.
- **A level follower that decides activity is a *time*, and a bare per-sample coefficient is the
  way that goes wrong.** `mxm-grain-fx`'s sustain measurement smoothed the wet with a fixed
  `0.0005` per sample — 42 ms at 48 kHz, but 250 ms at 8 kHz and 2 s at the 1 kHz the validator
  sweeps down to, where the follower's own lag was the whole comparison window. The engine then told
  the host a *held* texture was a finite tail, which is the one outcome its activity contract exists
  to prevent, and every other time in that crate was already converted from the sample rate. The
  review question is **"what does this coefficient mean in seconds at 1 kHz and at 768 kHz?"**, and
  the test that catches it asserts the *decision* at three rates rather than the coefficient.
  **The same pattern was then found twice in `mxm-shimmer-dsp` and fixed there** (2026-09-11):
  `reverb.rs`'s `self.level` (0.05 / 0.0005) and `lib.rs`'s `self.activity` (0.05 / 0.0002), both
  feeding `is_quiet()` and the park decision. **They were the same pattern at a much lower severity,
  and the difference is the shape of the consumer** — worth knowing, because it is what decides
  whether one of these is a bug or hygiene. Shimmer's are *peak* followers with a fast attack and a
  slow release, so each is an upper bound on the recent envelope and cannot report quiet before the
  signal is: the failure was parking **late** at a low rate, not truncating at a high one. grain-fx's
  compared a level against *its own value a window ago*, and a lagging follower flips that comparison
  in the dangerous direction. **Ask what the follower's output is compared *against* before ranking
  one of these** — and note the ranking is about severity, not about whether to fix it.
- **Anchor the converted time at the rate the coefficient was written for, and the fix cannot
  truncate.** Round-tripping shimmer's coefficients at 48 kHz gave 42 ms and 104 ms, which leaves
  that rate bit-unchanged, speeds the low rates up to match it — the repair — and *slows the high
  rates down* to match it, which is the conservative direction for a park decision. That is what made
  a change to a shipped activity contract safe to take on hygiene grounds alone. The measurement that
  showed it worked is the park time itself: 11.24 s at 8 kHz against 6.59 s at 48 kHz and 6.54 s at
  192 kHz before, 6.586 s at all three after, where the remainder is the audio tail rather than the
  follower. **A test budget stated in seconds is what has teeth**; asserting the coefficient asserts
  nothing, and asserting the three rates agree exactly would be asserting the tail's own length.
- **Output sanitization is too late.** `clamp` alone does not reject NaN. Reject/neutralize non-finite
  controller, tuning and auxiliary-input values before oscillator or recursive arithmetic. Inject
  one bad event/sample, then finite input: prior valid controls and subsequent finite audio must
  survive, rather than becoming permanently silent behind a final finite-output guard.
  **Test every public DSP seam directly, not only the wrapper.** An outer engine may sanitize what it
  feeds a public core while direct core input still turns an activity envelope into infinity and then
  NaN; one bad sample followed by a finite render must recover at each public entry point.
- **Every accepted sample rate is a promise.** Audit constructors, rate changes and activation for
  consistent limits. A small positive rate can invert `clamp(min, sample_rate * fraction)` bounds.
  Test the lowest accepted rate, rates below it, non-finite rates, and ordinary supported rates.
- **`f32` audio does not imply `f32` recursive pole coefficients.** The first drum-machine resonator
  calculated a 55 Hz pole pair in `f64` but stored its cosine and recursive state in `f32`; an
  interpolated two-second crossing measurement still missed by 0.19 cents. Keeping coefficients and
  state in `f64`, then converting only the output sample, restored the analytic frequency while the
  audio path remained `f32`. Test the lowest intended pole frequency over enough cycles; a 440 Hz or
  short-window check may not expose coefficient quantisation that compounds through a long tail.
- **An all-pole resonator's raw impulse gain is not a physical strike level.** A unit numerator gives
  `rⁿ sin((n+1)ω) / sin(ω)`, so the first drum-machine kick gained roughly 136 at 56 Hz before any
  circuit gain, drove its provisional nonlinearity and spent 81 consecutive samples on the emergency
  clamp. Finite/bounded tests all passed while listening correctly identified a square-wave attack.
  Give a struck pole pair its `sin(ω)` numerator (or derive the circuit's actual numerator), test
  unit-scale impulse gain across frequency and require the ordinary reference render to stay clear of
  its final safety bound. A clamp is recovery policy, never a waveshaper unless the model says so.
- **An envelope duration is not automatically a T60.** The first reset-VCO kick had the right pitch
  path and a nominally plausible 22 ms constant, but interpreting that number as time-to-minus-60 dB
  collapsed almost the whole sweep into the click; listening reported that the pitch envelope was
  missing. The same failure later made six resonant percussion voices roughly three times too short:
  acquired recordings showed their service-note “decay” values aligned with −20 dB, not −60 dB.
  State whether a number is an RC time constant, T60, threshold crossing or total event duration,
  then test the audible trajectory in separate early and settled windows. If a source leaves the
  endpoint unnamed, compare it against measured crossings instead of choosing T60 by convention.
- **A capability declaration is executable behavior, not editor metadata.** The completed drum
  catalogue exposed several axes that the editor enabled while DSP ignored them, and several metal
  Pitch controls that still read the shared zero-deviation frame. For every selectable model, render
  each declared unsupported axis at a hostile nonzero value and require bit-exact equality; render
  each supported axis away from reference and require an audio difference. This catches both halves
  of a stale capability table without relying on visual disabled-state tests.
- **A catalogue selector needs an explicit output plane.** Ninety-four complete drum renderers all
  sounded and stayed bounded, yet their zero-deviation reference peaks spanned nearly 28 dB. Preset
  Level values therefore described neither balance nor hierarchy. Measure every selectable model
  under one named sample rate, control state and velocity; apply a fixed post-topology trim; and hold
  the complete catalogue to the target in one test. Keep that separate from dynamic normalization:
  a static adaptation preserves accent and envelopes, while AGC would rewrite the circuit behavior.
- **Fractional ring-buffer wrapping needs enough index precision for the largest accepted rate.** A
  wrapped `f32` position just below a roughly 50k-sample buffer length can round up to the length and
  panic at interpolation. Exercise the seam directly (`write = 1`, delay just above one sample) and
  run sustained renders at the largest lines; split the delay into integer sample age and fraction
  before wrapping, compute in `f64`, or prove an explicit post-wrap bound before indexing.
- **Mix normalization must remain continuous.** Dividing by the sum of active slider levels can
  turn any positive single-slider value into full scale. Check zero, epsilon, partial and full
  settings, multiple sources, and smoothed automation—not just endpoints. Derive the loading law
  from evidence; do not prescribe one generic normalization formula for all instruments.
- **Correct parts do not prove an emergent effect has its claimed identity.** A shifter can pass an
  octave test and a reverb can be finite while the closed loop still sounds like a slap delay. When
  listening rejects a reverb or delay, measure the integrated impulse over seconds — onset, envelope
  peak, density and decay — and use tone bursts to distinguish upward spectral migration from a
  retained fundamental or descending accumulation. Black-box reference audio may supply behavioural
  targets; regression thresholds and every implementation constant remain independently chosen.
- **A bounded feedback loop can still become a late saturated attractor.** A three-second adversarial
  render proves finite amplitude, not decay: a reverb can grow only after ten seconds and remain
  safely below its saturator forever. Render high-feedback impulses for the product's actual tail,
  compare separated RMS windows, and require ordinary decay to keep falling. Calibrate the direct
  and windowed shifted returns separately when their average gains differ.
- **A frequency-response magnitude is not a feedback oscillation threshold.** `max |H(e^jw)|` is a
  small-gain/contraction bound; instability depends on the complete open loop's phase and first
  positive-real Nyquist crossing, including routing, in-loop filters and explicit delays. A fitted
  multiplier can make a few impulses look right while falsely certifying another already-unstable
  setting as finite. Keep gain margin and contraction as separate prepared measurements. The
  distinguishing proof uses responses with deliberately different peak/RMS shapes, brackets the
  sustained edge from both sides around the displayed contract, and fails an unbracketed result;
  comparing against a near-silent reference or starting the probe grid above the crossing is not an
  oracle.
- **Sparse factory overrides inherit every Init retune.** Making Init stronger can silently move ten
  nominally different presets onto the same dominant Size, Diffusion, Shimmer and Regen setting.
  Regenerate the complete files, render every preset from a fresh settled engine over stimuli long
  enough for the effect, compare pairwise descriptors, and require meaningful movement on several
  parameter axes. File inequality and different names prove nothing audible.
- **A sustaining test does not define Freeze.** Input gating and transformation of the held return
  are independent choices: a loop can sustain forever while rejecting later input, repeatedly
  shifting or filtering the captured material, or both. Test silence after capture for stable
  spectrum, arm an empty engine and excite it later, add a second distinguishable tone to an existing
  field, and verify whether pitch treatment belongs once to new input or repeatedly to the held
  return. State natural modulation separately from harmonic drift; “output remains nonzero” cannot
  distinguish any of these contracts. **A level normaliser can sustain while ring-modulating its own
  field:** an attack/release envelope fast enough to follow the rectified wet waveform returns that
  waveform as gain sidebands. Slowing that normaliser may only postpone the same failure—the field
  decays first, then the rising compensation exposes a sparse stutter. Prefer preserving energy in
  the held state itself, with no output-derived gain in the audio path; compare separated long-hold
  energy windows and density, not just nonzero output or the normaliser's control signal.

- **A crossfade built from a paired read must fetch the pair raw, and blend under the
  interpolator.** A loop seam's partner frame fetched *through* the wrap it spans lands on the very
  frame it should differ from, and the fade renders nothing; a blend placed *above* the kernel has to
  stop the taps wrapping whenever the fade is non-zero, which switches regime as the fade leaves zero
  and turns a fade shorter than the kernel into an unband-limited cut. The distinguishing regressions:
  a whole-region loop that does not close must change under any non-zero fade, and a fade shrinking
  toward zero must converge to the hard-wrap render. `crates/mxm-creative-sampler-dsp/AGENTS.md`
  records the case.
- **A loop that wraps every read also wraps the first pass.** An index wrap applied to every fetch in
  a looping mode folds whatever precedes Loop start — the attack — into loop material from a note's
  first sample, and a reader that folds its playhead into the loop skips the approach entirely. It
  hides behind patches whose loop is the whole region. The distinguishing regression renders a looping
  note with Loop start past Start against the same note one-shot and requires them equal, bit for bit,
  until the kernel reaches the loop — for every reader that reads the loop, with and without any seam
  treatment layered on it.
- **"Has this reader reached the loop" is state, not a position.** The first repair of the above
  decided from each read's position, and a position cannot tell a first pass approaching Loop start
  from a head anchored just before it after a wrap — which must read the loop. Latch it per reader
  where a looped read and a plain one fetch the same frames, so the switch is silent, and let
  anything derived from a reader (a re-anchored head, a correlation series) inherit the latch rather
  than re-derive it. Probe the hold (zero speed), the reversed direction, and a step that crosses the
  whole loop at once: those are where a sign test or an "overshoot" shortcut silently takes the wrong
  branch. **A margin capped to fit a short span is no margin**: where no silent switch point exists,
  fade from the old read, carried on, to the new one, rather than moving the switch.
- **Unity pitch hides a reader's mistakes.** At unity a read on a whole frame weights every tap but
  the centre zero, and a granular head sits exactly on its playhead. A tap reading the wrong frames,
  or a head anchored in the wrong place, then changes nothing measurable. Two regressions here passed
  their mutations at unity. They caught them only once rendered a semitone down, or transposed with a
  state oracle — where the heads are, not how loud they sound, because a newly anchored head's window
  is closed.
- **One boundary model for a span.** A file format's inclusive loop end, an inclusive integer fetch
  and an exclusive float period described one loop and disagreed by a frame. That was inaudible on
  the long buffers every test used, and ten cents sharp with a frame skipped each pass on the
  single-cycle waves the feature was for. Choose one model (half-open, `[start, end + 1)`), derive
  every wrap and limit from one helper, and test on the shortest material the feature serves, with
  points between frames as well as on them.
- **A detector asked to count cycles in a loop is listening to the loop.** Repeating a loop so a
  detector can hear it makes the signal periodic at the loop, so where the cycles inside differ
  slightly it answers the loop, an octave or two low. The approved plan trusted the detector; a
  synthetic loop holding a non-whole count caught it, and a measurement on real files showed 16 of 71
  counted wrong where the file's own unity note counted all of them right. Measure a guide on the
  material before trusting it to count.

- **Tail accounting follows audible routing.** A disconnected long-release envelope must not keep
  exact-silent processing alive after the audible path has ended. Conversely, parking must not
  freeze hidden state that can later revive as a ghost tail. Switch to a previously disconnected
  envelope after idle; require the documented silence/settling behavior. For interpolated inputs,
  compare identical wakes after deliberately different pre-sleep histories; exact idle output alone
  cannot expose stale reconstruction state. Test DSP activity and the wrapper's reported tail
  duration together.
- **Root uniqueness is not fixed-step convergence proof.** For a nonlinear sample solve, compare
  residual and root error with a separately converged reference at parameter boundaries and after
  abrupt opposite-sign input/state transitions. Include the old seed/count as a sabotage oracle;
  a nonzero derivative only proves that a Newton step is defined, not that a chosen number
  converges.

## 4. Fidelity proof that cannot be satisfied by a different instrument

- **Trace state ownership through sync/dividers/waveshapers.** Resetting the core must not reset
  an independently driven audible ramp unless its own circuit edge occurs. Use a state/sabotage
  test and audible lock/failure-regime scores; an alias reference sharing the same wrong phase
  equation cannot validate topology.
- **Band-limit every nonlinear/discontinuous path that needs it.** Saw tests alone do not cover
  folded triangle, moving narrow PWM, sync, register combinations or ordinary/synced/external ring
  products. Multiplication generates new frequencies even when its inputs were band-limited.
- **Hard sync is an ordered edge sequence, not an extra reset BLEP.** The slave may reset from any
  phase: retain natural saw/pulse edges strictly before the master edge, suppress coincident and
  later free-running edges that the reset preempts, apply the waveform's immediate-before to
  phase-zero reset jump, then add any pulse-width crossing created by the post-reset trajectory.
  Every surviving edge contributes both pre/post lobes; merely replacing the origin BLEP misses a
  reset-created narrow-pulse lobe and double-counts preempted wraps. The distinguishing analytic
  oracle separately exercises those three sequences over two samples. Independent offset-phase,
  Nyquist-limited additive references then compare synced saw and pulse/PWM separately across A/B
  ratios, including a width narrower than the post-reset advance. Ordinary unsynced upper-band
  tests are supporting coverage, not a sync oracle.
- **A duplicated ruler carries duplicated bugs, and the fix does not travel.** `mxm-mono-01-dsp` and
  `mxm-poly-06-dsp` both asserted oscillator tuning *accurate to one cent* while measuring it by
  **counting** whole zero crossings over a fixed window — which quantises to ±1 cycle, about ±9 cents
  at 55 Hz over two seconds. `mxm-mono-00-dsp` had found that, interpolated its crossings, and written
  the reason in a comment; the correction stayed where it was made for months while two suites kept
  reporting green about a property they never checked. Both assertions were re-derived against the
  shared ruler and now hold at a **tenth** of a cent. Take a measurement from
  [`crates/mxm-measure`](../crates/mxm-measure/AGENTS.md) rather than writing one, and when reviewing
  a test that asserts a precision, ask what resolves it.
  Two corollaries worth carrying separately: **a test frequency that divides evenly into its window
  hides a quantisation defect** — at exactly 55 Hz over two seconds the coarse count is exact, which
  is why the ruler survived in crates whose test frequencies were all round numbers — and **one name
  for three quantities is worse than three names.** `magnitude_at` existed twelve times computing a
  component amplitude, a transfer gain and an energy gain, two of them disagreeing by a factor of two,
  so no figure could be compared across crates. Split by quantity before counting copies: twelve
  sites were counts of five, three and one, and the one does not qualify for extraction.
- **A census is only as good as what it searches for, and a count from a filename is a lower bound.**
  The duplicated WAV writers were counted as six, then seven, then nine, and every correction came
  from a reviewer rather than from the method. Searching for the shared module found the copies made
  by copying a file; searching for the helper name found one more; searching for `b"RIFF"` — the
  **behaviour** — found them all. The copy somebody typed out again is invisible to a filename search
  and is the one most likely to have drifted, because nothing kept it in step. Search for a thing the
  code must contain and a non-instance will not.
- **Measure unwanted alias energy and wanted-spectrum retention together.** Compare with an
  appropriate additive or high-rate reference and a trivial baseline, over the named regimes.
  Derive defensible margins from measurements; a treble-removing filter or a threshold chosen only
  to match the current implementation is not a quality oracle. The SH-7's chosen oversampling
  factor is not a collection-wide prescription.
- **A wart needs a test that fails when it is polished away.** “Some output exceeds epsilon” does
  not prove the zero-time envelope click. Measure a transient-sensitive property and show that an
  inserted de-click ramp fails. Apply the same reasoning to droop, drift and other evidenced warts.
- **Measure routed controls at intermediate settings.** Centred versus unipolar LFO signals, pedal
  depth versus fixed keyboard contribution, and bender CV/OFF/LFO destinations can sound plausible
  while being wrong. Test zero, partial and full amounts and actual destination signals.
- **A score that saturates at "good" passes the failure it cannot see.** `mxm-classic-verb-fit` searched
  on the echo density profile, which reads one for any window that looks Gaussian. A network of eight
  4 ms lines does, so the fit chose it for a 12.8 s space and reported a close match; the owner heard a
  comb, and the tail's spectral peakiness measured 15 against the response's 2. Echo density measures
  time and ringing lives in frequency, and the recipe's modal density floor was on the research page
  with nothing checking it. When a search or a test scores a perceptual property, ask what the wrong
  answer that maximises the score would sound like, and measure that too
  (`crates/mxm-classic-verb-fit/AGENTS.md`).
- **An experiment that overrides one parameter must hold what is stored in its units.** Raising the
  same reverb's Size to test a denser network also moved its early reflections, whose times are stored
  in Size units, up to 47 times later, and the first measurement blamed the network for clicks the taps
  made. Hold dependent quantities at their measured values, and check a surprising result against a
  control that changes nothing else.
- **Model a measurement the way the measurement reads.** The same fitter matched its decay filter's
  closed form at each octave's centre to a T30 the analyser reads across the whole octave. Where decay
  changes steeply inside an octave the slower frequencies own the band's late energy, so renders read
  long while the fit's own residual looked small: against the pool's renders the centre missed 8 kHz by
  p90 7.5 %, a model read through the analyser's band by 4.7 %. When a fit or a test compares a model
  with a measured descriptor, put the model through the same window, band or ruler the measurement
  uses (`crates/mxm-classic-verb-fit/AGENTS.md`, *Decay is solved across each band*).

These are methods, **not SH-7 behavior mandated for other machines**. The SH-7's core-only sync,
LFO reset conditions, retained paraphonic writers, pedal tracking and S&H normalled gate must be
checked against `research:instruments/sh-7.md` and its own brief. Other machines require their own
evidence. Calibration faults, single-source anecdotes and software emulations are not mandatory
hardware warts.

## 5. Dynamic reflow and semantic editor tests

Use the current [design system](MXM_DESIGN_SYSTEM.md) §§3.3–4.3, 6–7 and 15 and
[shared UI contract](../crates/ui/AGENTS.md), not a remembered fixed-column layout.

- Render minimum, default and wide windows; inspect **drawn card rectangles** for order, floors,
  row alignment, overlap and lone-card width ceilings. Check parallel grouping while space permits
  and the documented narrow-window behavior. Repeated frames must not feed stretched heights back
  into natural-height measurements.
- **Card rectangles and a sample of labels do not prove the shipped editor.** Enumerate every
  permanent parameter and every semantic display from the production musician surface, request
  every dynamic page/card, and require each actual AccessKit control/display rectangle inside its
  owning card across both themes, relevant scales and disclosure states. Keep the Parameters view
  check separate: it can contain every parameter while a musician control is missing.
- **The app bar at the minimum window, not only the cards.** A minimum one card wide said nothing
  about the bar: an instrument's master slider and meter left no room for the `…` menu, so presets,
  theme and zoom were unreachable and the slider was drawn over the wordmark (seven editors, found
  2026-09-26). `opening_size::bar_holds_from_the_minimum` is the check. **Sweep widths, not one
  size**: a width miscount in the bar's own step choice (`add_space` comes on top of the item
  spacing) cut the `…` short only near step boundaries, at one width in sixty points.
- **A cut reading is an ellipsis in the value style.** `tree_checks::card` fails on monospace text
  painted with an ellipsis; a reported `5.50…` is a knob column narrower than its widest reading.
- **A control whose travel is silently inert past some point is an interface defect, not a tuning
  question.** mxm-creative-sampler's grain size runs to 1 s, but above `pool / density` the extra
  onsets are dropped and the knob does nothing — two thirds of its travel at the default density,
  with nothing on screen saying so. Where a bound is real and a control can cross it, the editor
  states what is being asked for against what is played, from the same arithmetic the DSP uses so
  the two cannot disagree. Clamping the control instead is the same defect wearing a different hat.
- **A direct-manipulation drag is acquired at the press origin, not where the pointer first clears
  the drag threshold.** The threshold consumes several pixels before `drag_started`; hit-testing at
  the later pointer position can miss a small point or handle and make the first drag appear
  directionally inert. Hit-test the stored press origin, retain that target for the gesture, then
  apply the current pointer in the same frame. The distinguishing probe adds a point to a diagonal
  curve and requires its first drag to cross the curve in either direction without a prior move.
- **A parameter drawn outside the paging renderer is invisible to the keyboard cursor, and only the
  coverage check will say so.** The cursor's registry is assembled from what is painted inside a
  card scope, and `navigation::paged` takes its card order and geometry from the renderer's report —
  which by construction cannot see a surface above it. mxm-creative-sampler's persistent macro rail
  shipped with four knobs that painted, clicked, held focus and could never be reached; the editor
  assembles that order itself instead, on last frame's rectangle. The rail went with the macros and
  the mechanism stayed — it carries the app bar's master output now, which is the same problem
  wearing one control. When an editor
  has anything above or beside its cards, run the coverage check before believing the cursor runs
  there. *(Found 2026-09-10 on a shipped editor; carry it into every editor review.)*
- **When one card shows one of several things at a time, `Coverage::Exactly` needs one pass per
  state.** The sampler's Source and Reader cards show the selected layer only, so a single pass can
  never reach every parameter and `Within` would assert nothing. Two passes, one per selection, each
  asserting `Exactly` what that state reaches, is the honest form.
- **Plugin-local telemetry must not invent a visual mini-system.** Canvas radii, trace/meter stroke
  hierarchy, plot extents, marker geometry and caption typography belong to shared UI tokens; use
  explicit theme colors instead of deriving alpha variants in the consumer. The distinguishing
  regression renders the full semantic inventory in both themes and the shared token test pins the
  geometry/stroke hierarchy.
- Test zoom at an **unchanged physical window size**, including the actual minimum at 200%.
  Enlarging the test window by the zoom factor hides lost logical space. Navigation, app-bar
  actions, Parameters, disclosures, browser and naming overlays must all remain reachable, not
  only the Synth cards. Operate actual zoom/theme controls rather than testing context getters.
- Query meaningful dynamic semantics: owner identities, selected source and gate level, set versus
  sounding values, legends and model caveats where the brief requires them. A generic “display”
  accessibility label can survive deletion of every useful drawing and is not adequate proof.
- **An editable target must not regress to a passive caption.** A prototype can retain working
  dropdowns while the shipped panel only prints their values. Exercise the actual plugin panel,
  query the control role as well as its text, click each target/source option and assert the exact
  bound parameter plus one complete host gesture. Repeat after reflow and in both themes; test
  Default, cancellation and external updates. `plugins/mxm-mono-00/tests/routing_editor.rs` is the
  shipped-panel oracle. Parameter-count and card-bounds tests cannot distinguish controls from labels.
  **Replacing a workflow also removes its old navigation.** Do not retain an unrequested optional
  overview. Assert the rendered tabs, absence of retired widgets, relocated controls, and developer
  category addresses; a test that only exercises the new dropdown can pass with the old page still present.
  **Card-local is not control-local.** A menu at the card's foot can still be detached from its
  amount. Pin the source rectangle immediately below its knob/slider at narrow and wide widths,
  across every source name and both themes. Names must describe the input's role after re-routing,
  not its default source; compare generator titles, menu options and host formatting. Preserve
  permanent parameter and enum IDs when changing display names.
- Inspect painted text at constrained widths; rectangle containment alone misses word splitting,
  justification and labels spilling across segmented cells. Never use `Ui::put` for labels whose
  layout must not justify. Test focus/reading order through interaction where supported.
  **A painter clip does not reserve room for text drawn inside it.** Plot captions positioned from a
  guessed gutter can be partly clipped even when the surrounding card fits. Resolve the actual text
  style first, reserve its row in the plot geometry, keep data frames above that row, and inspect the
  painted text shapes to require their visual rectangles inside their clip rectangles.
- **Keyboard navigation must be proved against the rendered surface, not a self-consistent list.**
  Compare the keys given to navigation with the paging plan's final category-first order; use the
  paging report's card rectangles, require every target in the requested half-plane and prevent a
  well-aligned later row from skipping the adjacent one. Exercise page boundaries, every cell of a
  segmented control, native widget-focus loss while its visible cursor remains selected, and
  developer/cardless surfaces after leaving a musician page. Send multiple arrows in one frame and
  real held-key press/repeat/release sequences: consuming an event is not executing it, and one
  continuous hold must remain one balanced host gesture. A control-level test
  alone misses panel order and stale-surface ownership; a single synthetic press misses repeats.
- **A semantic overlay must not allocate reserved display space twice or move its parent's cursor
  backward.** A child placed over an already allocated telemetry rectangle can preserve real
  accessibility nodes, but layout helpers that report their allocation to the parent may rewind the
  next control into the display. Render the real display, locate its painted background, then require
  the following control to begin after its painted bottom plus spacing. Exercise every display in
  both themes, constrained widths and all zoom steps; an outer card-bounds assertion can pass while
  its contents overlap. `plugins/mxm-para-07/src/editor/visuals.rs::telemetry_overlays_leave_following_controls_below_the_painted_display`
  is the distinguishing regression on the reference branch.
- **An embedded payload must not be cloned or decoded per repaint.** Build a fixed-size semantic
  display when the asset candidate is prepared, cache it in the editor by a monotonic display
  revision, and paint only that bounded view. At the maximum accepted payload, repeat paints/reads
  and require no decode/display rebuild while bounding the view's size; a visually correct plot from
  `snapshot()` can still allocate megabytes every frame.
- **A completion edge may occur while its transient editor does not exist.** Reconstructing an
  editor with `observed_revision = current_revision` strands an already-Ready result forever if
  commit only happens on revision change. On build/update, consume Ready as a level condition and
  make commit change it synchronously to Idle so it happens once. The distinguishing regression
  closes during Loading, completes in the background, initializes the reopened baseline to the
  already-current revision, then requires exactly one commit.
- **Separate layout cost from native resize scheduling.** Count passes/card draws, but also trace
  host callbacks on the real window. A floating editor must not request a host parent's resize:
  there is no parent, and waking/redrawing the host per drag event defeats idle throttling. Check
  accepted native sizes, zero unnecessary host round-trips, and close/reopen; preserve embedded
  host negotiation. `apps/mxm-player/tests/editor_resize.rs` is the Windows bundle-level oracle
  (since the split, `collection-tests/editor_resize.rs` in the owner's local workspace, not on
  GitHub yet, because it needs every product's bundle).
  Callback counts and headless milliseconds do not establish displayed FPS or smoothness.
- **Native-editor inventories and mutable bundle profiles drift independently of plugin tests.** A
  new floating editor can pass its own open/reopen proof while remaining absent from the collection
  resize loop. Keep one explicit inventory, assert the new product occurs exactly once with no
  duplicates, and update its documented count and every complete-profile staging list in the same
  change. `target/bundled/` is one mutable
  profile slot: stage the complete inventory as debug, run validator/lifecycle/resize before
  replacement, then stage and test the complete release inventory last. A hardcoded length plus one
  requested product is self-consistent even when another shipped editor is absent. The
  distinguishing oracle compares the unique inventory names against every `bundler.toml`
  declaration; the ignored native loop must then print two zero-round-trip opens for every member in
  each staged profile.
- **Paging is not proved by a width-only packer or the selected page's measurements.** Request
  natural heights at every candidate width, including hidden cards; unknown is not zero. Invalidate
  font/zoom/disclosure generations without changing interaction IDs. Test against real rendered
  widths and unstretched heights, not a second copy of flex arithmetic. More pages may shorten
  labels and reduce bar rows: reserve bar height monotonically inside a bounded solve. Keep stable
  card anchors across splits/merges/removal and defer replacement during gestures, text entry and
  popups. Distinguish an indivisible overflow requiring scrolling from ordinary multi-card overflow
  that paging should remove. `crates/ui/tests/paging.rs` and `crates/ui/tests/view_bar.rs` cover the shared
  foundation; `tests/paged_editor.rs` adds integrated ownership/cache tests and the plugin suites
  test their real panels. Neither establishes the physical fit gate. The rollout's baseline and
  repair evidence is in `plans/plan-dynamic-view-paging.md` §9 (in the private archive), not
  inferred from these lessons.
- **A scroll flag is not reachability.** Drive wheel/scrollbar input until an over-wide card's
  far-edge control enters the clip rectangle. Manually positioned `new_child` Uis do not allocate
  their extent in the parent; allocating only viewport width leaves `ScrollArea::both` with no
  horizontal range even when the report says it scrolls.
- **Hysteresis must retain navigation geometry with the partition.** Sweep compact/wrapped mode
  transitions at a fixed width. Current-fit evidence must use the bar actually about to be drawn,
  not the previous mode's reservation; unchanged page count does not mean unchanged overhead.
- **Compact navigation still owes width and pointer floors.** Measure Previous/Next hit rectangles
  and painted labels at the zoomed minimum workspace. Compare against the original workspace,
  before an overflowing horizontal row can expand the parent's reported rectangle.
- **A measured height cannot repair an understated width floor.** Test real cards at 2× with the
  simulated physical budget fixed: font rounding and new row combinations expose widths a tall
  1× component canvas never exercises. Mono-02's Envelope and mono-03's Filter floors needed
  correction; mono-03's Advanced needed internal knob-row wrapping rather than eight squeezed
  columns. Keep indivisible card identity while making its body honest at candidate widths.
- **Reserve minimum body height before drawing.** egui 0.36's `set_min_height` is relative to the
  current cursor; calling it at the foot adds that height again. Use ordinary inputless layout for
  hidden measurement: egui's sizing pass can alter spacing, so it is not necessarily paint geometry.
- Test private editor state in-process; test advertisement/open/close/reopen through a real loaded
  bundle. Do not add a host-to-plugin CLI protocol as a substitute. Manual §15 and real-DAW gates
  remain explicitly unmet until performed.

### Transient actions, routing and ownership

- **No modal OS loop inside an editor frame.** A synchronous `rfd::FileDialog::pick_file()` in `ui()` re-enters egui-baseview mid-frame and aborts the host with `RefCell already mutably borrowed` in a window procedure (mxm-fx-convolution Browse and mxm-creative-sampler Load, 2026-09-15). Headless kittest never opens the dialog, so no harness catches it: search editors for synchronous native dialogs and require `mxm_ui::offthread` with a later-frame collection. The oracle is a native click in a real host with the full panic output captured.

- **A GUI command consumed only by `process()` must wake a sleeping host.** An atomic counter proves bounded delivery but cannot make an inert plugin run. Exercise the editor submission, non-parameter `host.request_process`, resumed audio, and exactly one completed act through a real bundle; rejection must schedule no wake.
- **Cross-thread test triggers must use immutable startup configuration.** Do not mutate process environment after threads exist. Launch a child with a private file, pipe or socket signal and a bounded timeout.
- **Mode and source selectors are orthogonal until evidence says otherwise.** Enumerate every mode/source pair and pin zero-index dry equality plus full-index sidebands.
- **Sound presets must not silently own instance wiring.** Audio destinations and external MIDI-channel assignments belong to the DAW/controller instance unless the product explicitly defines them as part of a sound. Give the preset adapter a named exclusion instead of hand-filtering factory files, then prove save, apply, Init, missing-parameter reporting, identity baselines and dirty comparison all agree. Host/project state must still round-trip the excluded parameters.
- **A live destination change is a bounded ownership transfer, not two permanent sends.** Prove only old/new can carry a part during the declared window, their gains form the specified partition, reversal is continuous, third-target requests are latest-wins and every retired destination reaches exact zero. Test Pan at the pre-Pan mono boundary and deliberate many-to-one summing separately; a clean main mix cannot reveal duplicated auxiliary audio.
- **A release ends expression, not the hit's address.** A one-shot keeps sounding after NoteOff, and host choke groups send NoteChoke after the release, so clearing the note owner at NoteOff/CC123 silently disables choke. Keep the owner until choke, retrigger or panic, track expression separately, and test NoteOn → NoteOff → NoteChoke. The same class: a transfer or smoother that only advances while `process()` runs must not be required for silent state, because a host may stop processing an idle instrument — install the target directly when nothing is audible, and judge audibility after Mute/Solo/Level from the previous sample, since an unmute or hit can land on the edit's own sample.
- **A route-local low-rate deformation must not become an audio-rate low-pass.** Measure the observed low-rate class, the top of the panel range and a CV-extended audio-rate setting.
- **Oversampling claims must name their exact boundary.** Test discontinuous, FM/AM and product paths against a higher-quality reference, or state which remain at host rate.
- **Self-running state is live only when the complete effective graph can reach audio.** Follow derived selectors and held state, enabled stages inside the active length, signed cancellation, interior overlap through serial gates and jointly refreshed random sources. Apply parameter-owned source state before the inert shortcut. Disconnected or cancelled paths park; every genuinely audible path stays live.
- **Channel-owned expression must refresh when note ownership falls back.** Cover same- and different-channel held notes, and prove channel pressure cannot overwrite a per-note value.
- **Live routing feedback needs the source sample as well as the route parameter.** Exercise continuous, held-random and pulse sources across reflow and both themes. One-sample pulse telemetry must latch across DSP samples and audio publications, with editor-generation tagging so close/reopen cannot create stale flashes or erase new ones.

### Documentation drift

- **Workspace examples share one flat target-name namespace.** Two crates can each build an
  `examples/render_demo.rs` while racing to the same output path or silently running the last binary
  written there. Prefix every example target with its owning crate's product token. The
  distinguishing oracle enumerates example target names from workspace metadata and rejects any
  duplicate before building all examples; invoking each documented command must then identify the
  intended package.
- **Release-artifact counts must follow the executable inventory.** Update every release instruction when a bundle is added and verify the full inventory in debug and release.
- **A delivered phase must stop being described as future work.** Reconcile the plan header,
  revision row, phase list, final status paragraph, parent plan index and nearest ownership DOX
  against shipped modules and tests during closeout. Searching only the plan title misses stale
  “editor remains” claims in its index and the DSP/plugin ownership boundary.

- **Give a responsive task page exactly one scroll owner.** Wrapping a shared flow that already
  scrolls in another `ScrollArea` can make the inner layout see the outer area's unconstrained
  content extent instead of the native viewport, while two retained offsets compete as the window
  changes. Remove the redundant owner, then rerun narrow/default/wide reflow and fixed-physical-size
  200% reachability tests against controls at the **horizontal and vertical geometric extrema**;
  sequence endpoints can both sit at the left edge and miss lost horizontal access. Those checks
  distinguish a safe removal from merely hiding overflow. Exercise the release bundle with a real
  native border drag as well — headless layout timing does not include window-message dispatch or
  presentation. This pattern caused the reported slow/jumpy `mxm-para-07` resize after its two-page
  consolidation.

## 6. Verify the test, the artifact and the closeout

- **Name the broken implementation the regression rejects.** Remove the guard, insert the unwanted
  smoothing, swap the owner, or use a naive generator: the test should fail. Two values calculated
  from the same mutable constant are not independent evidence.
- **Run the falsification; do not infer it.** A largest-step metric on rendered audio cannot see a
  defect no larger than the signal's own per-sample slope. The creative sampler's loop-crossfade test
  for a period rounded to whole frames measured the output, looked right, and stayed green with the
  period rounded: a one-frame partner shift is an ordinary-sized step at the output. Measure such a
  defect where it lives — here, fixed fetched frames against a derivative bound as the loop end
  slides — and keep the output test only for what it does prove.
- **An in-place render test must erase the previous output before calling it silent input.** Reusing
  the same block without clearing it feeds the effect's last output back as fresh excitation and can
  make a freeze, wake or tail assertion pass for the wrong reason.
- **Parameter text must be idempotent through the host's normalized conversion.** A formatter that
  selects units at a raw floating-point boundary can emit `1.0 kHz`, parse it to 1000 Hz, preview
  that value just below the boundary, and then emit `1000.0 Hz`. The distinguishing oracle is exact
  `format(parse(format(normalized))) == format(normalized)` over the validator's normalized grid and
  values on both sides of every unit boundary; a plain-value formatter/parser test misses the
  normalized inverse that triggers the defect.
- **Do not hide a bypass sentinel inside a frequency control's public domain.** Encoding an open
  high-cut filter as `0 Hz` put Open at the knob's bottom instead of making zero the darkest cutoff;
  moving the same sentinel to the top would still make the label lie. Keep every public value a real
  frequency, put the closed low endpoint at the bottom and display the real maximum as Open at the
  top. If bit-compatibility needs a bypass sentinel, keep it private to the DSP seam. Test endpoint
  audio and the host-reported default position. If the old plain value shipped in state, migrate it
  and the matching normalized preset-identity baseline together; changing factory JSON alone leaves
  old projects or Clean/Modified status behind.
- **Exercise every advertised I/O configuration.** Explicitly select each configuration through the
  real bundle while deactivated; negotiation that always accepts the first row is not coverage.
  Feed distinguishable channel signals, assert each dry and wet routing law, verify bit-identical
  dual mono where specified, and attach an output-event sink on every callback to test absence of
  emitted notes. Audio from one default layout alone cannot prove port advertisement.
  For presence-selected synth inputs, use a host harness that selects every configuration while
  deactivated and supplies every declared port in order: present-silent Audio must replace internal
  noise, present-low Gate/clock must suppress internal timing, the both-present layout must prove
  Audio first/Gate second, and every stereo variant must duplicate an audible programme exactly.
- **A golden must exercise the interactions it claims and explain controlled regeneration.** Use a
  deterministic score through the real bundle/host path that spans the named oscillators, sync/PWM,
  modulation, articulation, ownership and release paths; assert length and non-silence, and include
  a perturbation that moves the digest. The source must tell the maintainer to rebuild, run, listen
  to the emitted artifact, review the intended DSP diff, update the digest with a reason, and rerun.
  A digest with no listening/regeneration procedure turns review into copying the failure output.
- **Freeze compatibility evidence.** Checking a control map only against today's mutable standard
  does not prove old-player compatibility; also use the claimed frozen baseline and loaded params.
- **A data loader can accept an obsolete shape as an empty no-op.** A collection-wide control-map
  sweep that checks only `load_instrument_map(...).is_ok()` missed `mxm-grain-fx`'s old single-map
  object after the schema moved to an `instruments` array: parsing succeeded and registered no
  product. Enumerate the shipping manifest, require one map per product, load each against the
  current standard, then assert its declared CLAP id is known. Success without a published entry is
  failure, not backward compatibility.
- **An absent bundle and a broken bundle are different outcomes.** Optional local suites may skip
  absence; once present, load/ID failures must fail. Release verification must stage required
  artifacts and account for skipped cases, rather than treating a green skipped suite as coverage.
- **Validate each build before overwriting its bundle:** debug bundle → debug validator/tests,
  then release bundle → release validator/tests. `target/bundled` is shared by both profiles, so the
  owning plugin's scoped verification sequence must encode that order and explicitly leave validated
  release output staged; a correct repo-wide command elsewhere does not repair a reversed local gate.
  When real-host tests resolve that mutable staged path at runtime, rerun them after the release copy
  replaces debug—the same test binary passing before replacement is evidence only for debug.
  Validator exclusions are not a substitute for the clean full-suite contract; reconcile the host
  activation envelope and any narrower DSP/content envelope instead, then report every warning.
  Allocation tests must deterministically select an instrumented artifact, even after release
  bundling. Close loaders before replacing bundles and verify bundling actually succeeded. Record
  the reviewed source commit and hash both the release build output and its final staged copy; a
  digest repeated from before material source changes is evidence for the old binary, not the one
  under review.
- **Reconcile docs against code and tests.** Capacities, signal equations, input/output patch files,
  pending/fixed status, command order, child indexes and delivery claims are part of review. A newly
  passing test does not prove unrelated old claims.
- **Search removed contracts in prose as well as symbols.** Deleting a shared constant does not
  remove comments or briefs that still promise its old value; search both the identifier and the
  stated dimensions/behavior, then distinguish current requirements from historical examples.
- **Check upstream changes during long jobs.** A stale worktree can pass its old fixed-layout tests
  while violating the current collection contract. Record the review base and distinguish upstream
  integration from instrument changes. **A clean merge does not carry a collection-wide pass into
  the branch's own editor:** `mxm-para-07` merged with no conflict in its editor after main had moved
  every other editor onto derived pages, the keyboard cursor, the derived opening size, the shared
  zoom/theme controls and CC 116, and the only signal was two of its own budget tests failing
  against taller shared controls. After merging a long-lived instrument, check its editor against
  what every sibling now runs — `mxm_plugin_test::paging_checks`,
  `mxm_plugin_test::keyboard_checks` — rather than against its own tests.
- **egui's `consume_key` ignores an extra `Shift` or `Alt`.** It matches with `matches_logically`,
  so a `Modifiers::NONE` pattern also swallows `Shift`+key — the most specific modifier must be
  tested first. Getting the order backwards makes the modified chord silently unreachable while
  nothing fails, and both keys still appear to "work". Check the order wherever more than one
  modifier combination is consumed for the same key.
- **A step count is not a step size.** `FloatParam::step_count` is unconditionally `None` in
  nice-plug, so a `with_step_size` parameter reports itself continuous; anything that moves a value
  by a "step" must go through `next_step`/`previous_step`, which also carry the range's skew.
  Review any new increment arithmetic for which of the two it actually consulted.
- **Re-spelling a shared input mapping is a change to every consumer, not one.** Moving the value
  edit off the bare arrows fixed the converted editor and silently removed keyboard editing from the
  eight that had not been converted. A shared re-mapping needs an explicit switch and a stated
  behaviour for consumers that have not adopted it; "the rollout will reach them" is not a state the
  tree may be left in.
- **Prove a pre-existing failure against a clean baseline before attributing it — but do not turn
  provenance into a permanent waiver.** Shared card bodies can drift while an archived bench
  remains untouched. Reproduce the failure on a clean baseline, then remeasure every feature's
  inventory rather than fixing only the first reported card. Require each content-controlled floor
  to fit while its preceding measurement probe fails, require preferred height to equal rather
  than merely exceed measured content, and retain direct clipping/overlap checks. A reported green
  factory checkpoint must still be reverified after reconciliation with concurrent layout work.

- **A golden's sensitivity check compares two renders from one build, not a render against the
  pinned constant.** Against the constant it passes whenever the base render moves, whatever caused
  it — `mxm-mono-03`'s first version stayed green under a bundle that ignored the very depth it
  claimed to prove. Rendering the base score and the altered one from the same bundle makes one
  mutant falsify it. The six goldens before it compare against the constant, which is equivalent
  only while their main digest passes (`plans/plan-collection-sync.md` revision 9).
- **A signed format prints a negative zero, and a negative zero does not survive a host's round
  trip.** `{:+.0}` shows `-0` for a value that rounds to zero; it parses to zero and comes back
  `+0`, so the text is not idempotent and `clap-validator`'s `param-conversions` fails — but only
  when its random values land in that sliver, so a clean run proves nothing. Format a signed reading
  through `mxm_modulation_params::signed`, which changes nothing but that zero's sign, and test the
  round trip at amounts either side of zero, not only at the round numbers. It was visible as well
  as latent: `mxm-mono-02`'s negative full scales made every pulse-width route, and Pitch from Auto
  bend, read a negative zero at zero amount, on screen and in all fifty of its factory files.
- **A guard with no fix to remove is falsified by introducing the defect, and recorded as a guard
  when nothing can.** `mxm-mono-03`'s ladder is bounded by its saturator, so its audio-rate tests
  stayed green under an energy-injecting coefficient change, a fully linear loop and a removed
  cutoff clamp. That is a measurement of the model, not a failure of the test; say so beside the
  test, as that crate's AGENTS.md does, rather than claiming a falsification that did not happen.

## 7. Modulation routing: what an absent route still owes

Found on `mxm-mono-pr1`'s conversion (2026-09-13 and 2026-09-14) and on `mxm-mono-00`'s
(2026-09-14). The first two **apply to every instrument that carries the shared routing** — the
pilot and `mxm-creative-sampler` had both latent until 2026-09-15, when each gained the repair and a
falsified test (`plans/plan-collection-sync.md` item 2) — and so do the reset, zero-depth and
per-sample-cost items below; the rest are lessons about how the routing is specified, reviewed and displayed.

**Five of these were found twice**, on two instruments, by two different conversions. That is what
makes them rules rather than anecdotes, and it is the reason to read this section before converting
the next one rather than after.

- **An absent route's smoother is not advanced, so it must not be resumed either.** While a pair is
  absent nothing calls `smoothed.next()`, but the parameter stays editable: a host automating it, or
  a preset load, moves the *target* and leaves the smoother's current value wherever the last live
  sample left it. Resuming ramps the route in from a stale number over a span set by **how long it
  was absent**, which makes a route's first audible value depend on the host's buffer sizes. Snap a
  newly present route's smoother to its stored value at the topology transition. The falsifiable
  test is remove → edit while absent → re-add, asserting the *first* sample is the stored depth;
  without the fix it reads a point on the ramp down from the old one.
- **A source that starts being read must not deliver a value from a previous phrase.** Publishing
  only the sources some live route reads is the routing's main saving, but an unpublished slot keeps
  whatever it held the last time something *did* read it. Adding a **backward** route to a running
  voice then reads that value for exactly one sample, and how stale it is depends on how long the
  source went unread. `mxm_modulation::SourceFrame::clear` at the transition makes the first sample a
  deterministic zero instead. Falsify with a long unread gap: without the clear the first read is the
  ancient value.
- **A law that does not survive a transformation is a reason to undo the transformation at the
  boundary, not to drop the feature.** `mxm-mono-pr1` scales every source into a ⅛ frame, and
  `product`'s `1 + amount × (source − 1)` is not scale-invariant — so a plan review concluded that a
  scaled frame cannot carry a multiplier target and the module was dropped. It was the owner's own
  decision 1.14, and the owner noticed it missing. **Un-scaling on the way into the law**
  (`frame.read(source) × FRAME_SCALE`) restores every property in one line. When an obstacle removes
  something a specification asked for, the removal is a **product change** and must be surfaced as
  one, whatever the technical argument behind it looks like.
- **A target with no scale must not fall through to another target's scale column.** The same
  `Option<scale>` table said `None` for two targets for two different reasons — cutoff, whose column
  is per-route, and the multiplier, which has no scale at all — and the `match` arm that handled
  cutoff silently claimed the multiplier too. Nothing crashed; a factor that applies in full simply
  displayed as `+800 %`. Where one sentinel means two things, give each target its own arm and make
  the laws `debug_assert` which target is theirs.
- **Arming a topology is not exercising a route: a routing proof must advance the amounts too.**
  Which pairs are live and how deep each one is arrive by different paths — the topology at the
  processing interval, the depths from the parameter smoothers per sample. A render that calls only
  the first plays **every route at zero depth**, which contributes nothing and still sounds, so the
  test passes while proving nothing about modulation. `mxm-mono-pr1` rendered all fifty factory
  sounds this way. **And at least one test must play the instrument's headline gesture end to end
  through the real callback, by permanent id, measuring rendered audio** — the one thing that would
  have caught a parameter side wired to the wrong target, which is the failure a player reports as
  *"it does not work at all."*
- **A declared evaluation order needs a test that runs, not a test that compares constants.** A
  forward/backward table checked against itself stays green through any reordering of the code it
  describes. Drive real samples and read the delay off what the instrument publishes, and cover the
  layer that actually publishes each source — a control-side test that stands in for the voice will
  not catch a publication moved inside the voice.

- **Every piece of state a topology transition leaves unfed owes a reset, not just the frame.**
  `SourceFrame::clear` covers the published values; it does not cover a *detector*. `mxm-mono-00`'s
  sync routes carry an `EdgeRow` each, and one that is not fed keeps its `prev`, so a route removed
  while its source was high and re-added while it is low reports a crossing that never happened —
  and *whether* it does depends on how long the route was absent, which is the host's buffer sizes
  deciding a sound. **When a route's evaluation owns per-route state of any kind, enumerate it and
  reset the newly live ones.** Two of the three pieces on that instrument were found this way; the
  third, the amount smoother, `mxm-mono-pr1` had already paid for.
- **A route at zero depth carries nothing, and every predicate that reads presence must ask that
  too.** Presence answers *does this route exist*; it does not answer *does it do anything*. On
  `mxm-mono-00` the keyboard gate became an ordinary route, and reading presence alone to decide
  "the keyboard reaches this envelope" made a zero-depth route retrigger a GATE+TRIG envelope on
  every note-on while its gate never crossed the threshold — a press with no gate. The same question
  reaches the activity predicate: a route at zero depth cannot make a patch live either. Conversely,
  an execution optimization must not turn zero into absence: `mxm-drum-machine` may omit a settled
  zero route from its per-sample list, but the separate presence still keeps the row visible and the
  assignment in a preset. Test both halves together — zero work and unchanged topology.
- **An amount that is signed does not mean every law can invert.** A sync route's amount is *reset
  depth*, and a reset has no inverse, so the DSP clamps the pull to `0…1`. Selecting the winning
  route by **magnitude** then let a route at `-1.0` beat one at `+1.0` and produce no sync at all.
  Where a law discards part of an amount's range, the selection and the combination must both use
  the value the law will actually apply.
- **A push added into a signed amount cannot both deepen it and stay continuous — and a review finding
  that changes what a plan specified is checked against the plan before it is acted on.** The mod
  wheel adds into vibrato. Once vibrato is a route's signed amount, the addition partly cancels a
  negative route. `mxm-mono-02`'s review called that a defect, and the push was changed on three
  instruments to take the route's sign; `mxm-poly-06`'s review then found the change stepped the depth
  by twice the push as a swept amount crossed zero — up to seven semitones in one sample — and it was
  reverted the same day (2026-09-15). The approved plans had specified the addition, which is also the
  machine's arithmetic: the lever's depth and the slider's are two depths that sum. **Test a fix at its
  discontinuities, not only at the case reported.**
- **A grid does not belong in a struct that is `Copy` and rebuilt per sample.** `mxm-mono-00`'s 300
  pairs are 1.5 KB of presences and amounts that change on a *parameter event*; carrying them in
  `Patch` measured **9 ns/sample** of pure memcpy against a 266 ns voice — more than the routing's
  own work cost, for nothing. It travels beside the patch instead. `mxm-mono-pr1` had already
  moved its grid out of `control::Params` for the same reason, which makes this a rule rather than
  an anecdote: **measure the per-sample cost of a conversion before and after, and separate the
  routing's own cost from the struct's.**
- **What a route's amount *reads* is what that pair delivers, not what the target's scale is.** A
  target's scale is per unit of source, and an instrument's sources rarely all fill the unit:
  `mxm-mono-00`'s envelopes peak at 0.6 of a ten-volt column and its oscillators at 0.5, so reading
  a route as the scale alone made the filter's envelope route say `+11.67 oct` where it sweeps
  seven — the number the knob it replaced had always meant. `mxm-mono-pr1` hit the same thing
  through its frame unit. **Check a converted instrument's readings against the constants the
  retired controls named**, and make the check a test: it is the one defect a player meets on the
  first knob they turn and no audio assertion can see.
- **A control-map role may take a route's *amount* only where Init wires that route.** An absent
  pair contributes nothing whatever its amount holds, so a controller knob bound to one is dead
  — `mxm-mono-pr1` wired three routes at Init for exactly this reason. A role bound to a
  **presence** is the exception and is never dead, because turning it on is what creates the route.
  Make it a test: `mxm-mono-00`'s caught a miscount in its own DOX on the first run.
- **A summing input can drive a modulated coefficient where one source never could — so a swept
  corner needs a bounded *speed*, not only a bounded value.** `mxm-mono-00`'s phaser sweeps four
  all-passes inside a fixed feedback loop, which is passive only while the coefficient holds still.
  Before the conversion, MANUAL IN took one column and a column is a *waveform*, smooth between its
  edges; after it, several audio sources at full depth saturate the ±4-octave clamp and toggle
  between its ends, the loop's gain at Nyquist exceeds one, and the phaser **self-oscillates to
  infinity from silence**. `clap-validator`'s `param-fuzz-bounds` found it; a probe of every column
  into the same input on the pre-conversion build stays finite and quiet. Three things made the
  diagnosis, and a conversion review should look for all three: the instability is in the **loop**
  (at zero feedback no excursion reaches it), its driver is coefficient **speed** (a random CV at the
  same excursion is stable — it takes a coherent tone to pump), and it needs **no input** at all.
  **Fix the driver, not the symptom**: bounding the feedback state is finite and wrong, leaving a
  full-scale self-oscillation at about six times whatever bound it is given. And the corner that
  bounds the driver cannot be in Hz alone — a `NYQUIST_FRACTION`-style clamp keeps it *looking*
  clamped while passing an alternating CV nearly whole at low rates.
- **A bounds fuzz must fuzz the bounds the host can actually ask for, and a bisect must replay the
  same run.** Hunting the above cost two false trails. The first fuzz used minimums the parameters
  do not have (a zero envelope stage where the minimum is 0.4 ms), so its first reproducer was the
  test's own fault. The second bisect re-derived the topology churn from a *fresh* draw, so every
  arm played a different sound and the table proved nothing; extracting one `trial(seed, tweak)` that
  regenerates patch **and** churn from the seed made the culprit fall out on the first run.
- **A card's floor is measured with every route revealed, and the row's text is what sets it.** An
  absent route draws nothing, so a floor taken at the init patch covers a few per cent of the
  surface. And a row reads `<target> from <source>`: where *both* halves are module-qualified, the
  longest pair sets the floor — `mxm-mono-00` paid a hundred points a card for the long form, which
  was enough to push its editor's first musician page down to a single card. The painted name may
  drop the prefix the card already carries; the canonical name stays for the parameter, the host's
  automation list and the accessibility tree. **A new source's name can move every floor at once**:
  it appears in every target's rows, so `mxm-mono-00`'s *LFO 1 core reverse saw* widened eleven
  cards by forty points where *LFO 1 reverse saw*, as long as the longest name already there, moved
  them eight.
- **A destination switch at a source is a route the conversion missed** (found on `mxm-mono-00`'s
  Rev 3, 2026-09-22, after its patch bay had been converted). DESTINATION, a depth knob wired to one
  source, a source-selecting switch: each is modulation the shared routing does not see, and the
  owner's rule is that no card chooses where its output goes. Convert it to a route into each
  destination it reached, present in the init patch where it pointed. Review for four things the
  first draft got wrong: **absorbing a path into an existing target changes that target's meaning**
  — its reach, its fader law, what its presence switches on — so its ids retire rather than being
  reused; **paths that used to add beside each other must still fit together**, so size the reach
  and the target bound for their concurrent sum; **each input is read in its consumer's stage**, so
  a path that went through an intermediate input may change timing, and that must be named; and a
  **control-map role on the retired knob may become unfillable** when one knob fed two routes.
- **Read the target list as a player will.** Two targets on one parameter (*Modulator 1*,
  *Modulator 2* on a filter) and a target named for its jack rather than what it moves survived
  `mxm-mono-00`'s conversion, its review and a six-round design review, and the owner found both at
  first sight of the panel. Check every painted target name against `plugins/AGENTS.md`'s
  *Declaring the target list* (since the split,
  [`plugin-conventions.md`](plugin-conventions.md#declaring-the-target-list--what-mxm-mono-00-had-to-discover-twice));
  tests pass either way.
- **An editor test that picks from a routing menu must scroll to the option.** Twenty-five sources
  is a menu that scrolls, and on a card low on the page its last options open below the window,
  where a click lands on nothing. `mxm-mono-00`'s sweep failed on the twentieth source of its first
  low card; `scroll_to_me` before the click is what a player does, and what `mxm-mono-pr1`'s proofs
  already did.

## 8. Audio files and the crates that read and write them

These came out of `crates/mxm-audio-file` and `crates/mxm-audio-file-decode` (2026-09-15).

- **An offset measured on a periodic signal is ambiguous by a whole period.** The decoder's first
  fixtures were steady 440 and 660 Hz sines, and the gapless check reported MP3 and Vorbis 2,205
  frames late at a correlation of 1.000000 — exactly 22 periods of 440 Hz, a match one period over.
  Any alignment, latency or "starts at zero" assertion needs a signal with one alignment: a sweep, an
  onset, or noise. Ask what the ruler would read if the answer were wrong by a period.
- **Two established crates can each be defensible and still not interoperate.** flacenc counted the
  short final block in STREAMINFO's minimum block size; symphonia reads min ≠ max as variable
  blocksize and rejected the whole file. A 4,096-frame round trip passed and every other length
  failed, while ffmpeg decoded all of them. Round-trip an encoder through **the decoder the
  collection actually uses**, at lengths that are not a multiple of the block size.
- **A reader may not declare the property a consumer relies on.** symphonia's RIFF readers set no
  sample format, so a float WAV reported itself as integer until the codec id was consulted. When a
  consumer promises a property of a file (the player's export is float), assert it through the real
  reader, not through the writer's intent.
- **A compiled-in notice has to be checked in the built binary.** A `const` nothing references can be
  stripped at link time, so a licence notice that "is compiled in" proves nothing until the release
  artefact is searched for its text (root *Before this repository is made public*, item 7 — the MPL
  item, which that list numbers 6; since the split it is in
  [`collection-rules.md`](collection-rules.md#research-boundary)).
- **A reader that skips damage returns success, and a fixture too small to lose a piece cannot show
  it.** symphonia's Ogg and FLAC readers step over a page or frame whose checksum fails without an
  error. The decoder's byte-mutation sweep asserted only "no panic", on a 0.5 s Ogg with one audio
  page: losing that page left nothing, so every mutation looked refused. On a 5 s file, 36 of 59 flips
  returned a second less audio as `Ok`, and the first fix still let 11 through. A mutation test must
  also check what an `Ok` holds, on input long enough to lose something from the middle and the end
  (the decoder does so in per-format damaged-file tests beside its containment sweeps) —
  and "the library reports errors" is a claim to test per format, not to assume.
- **A length field may be an end timestamp.** symphonia's `num_frames` for Ogg is the last granule
  position, so a stream starting away from zero looked truncated. The first length rule was measured
  only on streams from zero and refused valid cuts; the reviewer's corrections were then wrong by the
  encoder delay, twice. Test a length rule on cut and re-timed streams, against a measured table rather
  than a formula.

## 9. Variable authored state and realtime publication

These came out of `mxm-fx-curve` C2 (2026-09-17).

- **A size check after deserializing a `Vec` is not a decode bound.** The allocation has already
  happened. Bound sequence growth in its Serde visitor: reject an over-limit `size_hint` before
  reserving, cap initial capacity, and reject the first element beyond the model limit. The outer
  state-chunk limit and the model's cardinality limit solve different problems; keep both.
- **Publishing a prepared object without allocating is only half the realtime contract.** Replacing
  it can run `Drop` on the callback. Give every slot an explicit ownership state and make audio
  publish RETIRED only after its final access; control is the only side that turns RETIRED into FREE
  and clears the value. Exercise publication, transition completion and retirement inside the
  allocation guard, not only steady processing.
- **Latest-wins needs one ordering authority.** A pointer/index exchange without a monotonic revision
  can let expensive older preparation publish after a newer edit. Assign revisions at the shared
  publication gate and have audio refuse anything no newer than what it has accepted.
- **A nonlinear model swap is two processors, not two tables.** History carries only through an
  unchanged compatible serial prefix. The changed stage and every stage downstream must process
  their real new upstream during the overlap; copying downstream detector state creates a smooth
  handoff to the wrong trajectory.

C3 added the editor-side half:

- **A persistent field can change while every parameter stays still.** Such an edit does not make a
  CLAP host dirty by implication. Commit the complete model through one GUI state transaction and
  call `host.state.mark_dirty()` only after it succeeds; putting the call in the shared host-load
  path makes every project restore dirty itself.
- **Snapshot history belongs to the durable model owner, not the transient editor.** Otherwise closing
  the window silently erases recovery. Bound whole-model snapshots, make one completed gesture one
  step, and explicitly start a new epoch when preset/project restore changes the model so Undo cannot
  cross contexts. Preserve the old epoch when restoration is unchanged or rolls back after refusal.
- **A GUI state restore may carry an engine correctly and then erase that work in `reset()`.** Read the
  wrapper's full transaction, not only the plugin callback. `mxm-fx-curve` suppresses exactly the
  editor-originated post-reactivation reset after its compatible-prefix carry; host restore keeps the
  normal reset contract.
- **Selected-stage telemetry must not make selection DSP state.** Audio observes every bounded stage
  into a fixed array and publishes lossy atomics; the editor chooses which observation to display.
  Sending the selected index down to audio would turn transient navigation into a realtime control.
- **Audible authored-state dragging is not a durable transaction per frame.** Prepare each complete
  preview off audio and use the ordinary fixed-slot handoff, but leave committed state, fingerprint,
  history and host dirty state unchanged until release. Returning to the origin, failed commit and
  editor close must republish the committed engine. Exercise many previews against active guarded
  audio, then prove release creates exactly one undo step and dirty notification.

## 10. Per-sample transcendentals on arguments that do not move

Found on `mxm-mono-08` when it was reported costing ~14% of a player callback with nothing playing.
Ablation put **26.5% of the whole instrument in its two low-pass gates**, which were evaluating
**eight** transcendental functions per sample. Memoising them cut the running voice by 28% and the
played voice by 12%, bit-identically.

- **`1 - exp(-1/(tau × fs))` is a constant wearing a per-sample disguise.** It is the standard
  one-pole follower coefficient, and both of its inputs are fixed for the life of an activation. The
  exact shape is in `mxm-mono-02`, `mxm-poly-06` and `mxm-mono-pr1` as well, and every instrument
  DSP crate has five to fifteen `exp`/`powf` sites. **Check a per-sample coefficient against what
  actually varies before assuming it has to be recomputed.**
- **Measure before optimising, and be willing to lose the hypothesis.** The first candidate here was
  hoisting the activity predicates, which are evaluated twice per sample and walk the routing
  matrix. They were worth 3%. A plausible, well-argued target was wrong by an order of magnitude,
  and only ablation said so.
- **Key a memo on everything the result depends on, `fs` included**, so a rate change is a miss
  rather than an invalidation path that can be got wrong. A mutation dropping `fs` from a pole's key
  was caught by the differential test at the rate-change sample; without it the bank digests passed,
  because they pin one rate.
- **A direction-selected coefficient needs one slot per direction.** Attack/release alternates every
  sample on rectified audio, so a single slot misses on all of it and adds a comparison.
- **Nothing approximate may hide behind a memo.** `exp2(v·log2 b)` for `powf(b, v)` is cheaper and
  differs in the last ulp; that is a sound change and an owner decision, not an optimisation.
- **Pinned digests are not sufficient evidence for a numeric change.** They fix one rate and one set
  of patches. Pair them with a differential test against an unmemoised reference across the rate
  range, every mode, a rate change and a reset — and mutate something to prove the test can fail.

`plans/plan-mxm-mono-08-per-sample-cost.md` has the measurements;
`crates/mxm-mono-08-dsp/AGENTS.md` holds the rules as that crate's contract.

A release review should identify remaining applicable findings, unexecuted checks, artifact/profile
and platform coverage, and manual gates. “No new findings” is not “all machines audited.”
