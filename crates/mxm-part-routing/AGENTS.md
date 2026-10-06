# AGENTS.md — crates/mxm-part-routing

Parent: [`../../AGENTS.md`](../../AGENTS.md)

## Purpose

Framework-free routing laws for instruments with independently addressed parts: bounded assignment values, fixed-note/channel matching, same-offset monophonic arbitration, CLAP-style note-owner matching and click-free destination transfer.

## Ownership

- Owns policy-neutral `Main | Auxiliary(n)` and `FixedNote | Channel(n)` assignment values.
- Owns claimed-channel matching with an explicit consumer-selected collision policy.
- Owns fixed-capacity same-offset monophonic arbitration with consumer-supplied owner priority and strike reduction, plus an in-place `retain` for removals that depend on the note alone (a same-offset choke), so a consumer pays them once per group rather than once per part.
- `ArbitrationResult` reports both strikes and the consumer picks: `strike` is every accepted candidate's, combined by its `reduce_strike`, and `owner_strike` is the winning candidate's own. Reduction still has to run either way, because it is what makes the result independent of host event order; which of the two *sounds* is a product choice and stays with the consumer. `mxm-drum-machine` takes `owner_strike`, because a hit there is its own velocity.
- Owns note owner/address value types and wildcard matching.
- Owns allocation-free equal-gain destination transfer, in-place reversal and latest-wins pending reassignment.
- Consumers own CLAP event translation, the selected musical policies, parameter IDs, port layouts and output naming.

## Local Contracts

- Zero runtime dependencies; MSRV 1.87.
- Assignment constructors contain hostile parameter values to the consumer-declared channel and auxiliary bounds.
- Arbitration capacity is a consumer const generic; overflow is reported without partial insertion, and the consumer decides the safety response.
- `tests/policy_neutral.rs` deliberately uses three parts, two auxiliaries, lowest-note priority, summed strike and layering so no drum-machine count or musical choice enters the shared API.
- A destination transfer exposes at most two destinations per part per sample.
- A request for the origin reverses the current transfer in place; a third destination is one latest-wins pending request.
- `None` in `NoteAddress` represents CLAP's `-1` wildcard. Explicit note ID matching takes precedence over key; an ID wildcard matches key or all keys.
- No allocation, locks, I/O or unbounded work.
- This crate is an owner-approved exception to evidence-first extraction (2026-09-18): it is created for `mxm-drum-machine` before the planned second drum machine consumes it. `mxm-model-drums` is that second consumer: its engine routes every slot through `DestinationRouter` and its shell resolves notes through the same arbitration.

## Work Guidance

Keep APIs consumer-neutral. Assignment and arbitration mechanisms belong here; product labels and policy choices do not. Do not add nice-plug, CLAP, parameter or audio-buffer types.

## Verification

```bash
cargo test -p mxm-part-routing
cargo +1.87.0 test -p mxm-part-routing
```

## Child DOX Index
