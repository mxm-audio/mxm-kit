# AGENTS.md — crates/mxm-fft

Parent: [`../../AGENTS.md`](../../AGENTS.md)

# Purpose

`mxm-fft` is **the collection's one FFT**: `rustfft` (complex) and `realfft` (real-input) behind
the collection's conventions, so the conventions exist once. Made by
the archive's `plans/plan-mxm-fft.md` (step X0) on the owner's ruling (2026-10-12:
*"make a shared fft crate. Havine 3 or for implementations is non DRY"*), against five in-house
radix-2 copies. **An FFT written anywhere else in the collection is a defect**: the remaining
copies move here one consumer at a time, `mxm-fx-eq-n`'s analyser the first
([NOTES.md § The crate rules in full](../../NOTES.md#the-crate-rules-in-full)).

# Ownership

Owns `src/` and `Cargo.toml`. Does **not** own windowing, smoothing, spectra stored between
transforms or their flushing: those are the consumer's. No minimum-phase helper lives here
(room-ir is its only user).

# Local Contracts

## The conventions

- **Forward is unnormalised, with a negative exponent**: `X[k] = Σ x[n]·e^(−2πi·kn/N)`.
- **Inverse is scaled by 1/N**, so a forward then an inverse returns the input.
- **Interleaved is the layout** (`Complex`, re-exported from `rustfft`). `forward_split` and
  `inverse_split` adapt split re/im slices through a caller's interleaved work buffer, keeping
  `mxm-measure`'s contract: a wrong length transforms nothing, and n = 1 is allowed.
- **A real transform** takes `len` samples to `len / 2 + 1` bins. Its input is garbage afterwards:
  `realfft` works in it. The inverse takes the imaginary parts a real signal's spectrum cannot
  have, at DC and an even length's Nyquist bin, as zero.
- **Any length**, 0 and 1 included. The tests cover 1 to 2²².

## Realtime: plan ahead, execute without allocating

- **Planning allocates, and cannot fail cleanly** — the X0 gate's finding (2026-10-13). Neither
  `rustfft` 6.4.1 nor `realfft` 3.5.0 reserves fallibly anywhere: no `try_reserve` in either, and
  planning builds twiddles with `vec!` and `collect`, so out-of-memory aborts as any `Vec` does.
  Plan off the audio thread, ahead of use, and keep the plan. **X5's consequence is the plan's**:
  the convolution's fixed audio-path sizes are planned once at activation, and whether its large,
  response-dependent offline transforms may abort on out-of-memory is the owner's decision there.
- **Executing takes caller scratch and allocates nothing** (`tests/realtime.rs`, a tracking
  allocator, every kind, direction, precision and layout).
- **A wrong length is an `Error`, and every buffer is left untouched.** `rustfft` itself panics on
  a wrong length, so this crate checks every length before calling it.

## Subnormals

- **While a transform runs, the hardware flushes subnormals** (`denormals::Guard`): the MXCSR
  flush-to-zero and denormals-are-zero bits on x86_64, the FPCR flush-to-zero bit on aarch64,
  nothing elsewhere. The previous state is restored afterwards, and a nested guard keeps the
  outer's. nice-plug's wrapper guard is compiled out in this workspace, so this does not lean on it.
- **The crate does not flush buffers.** A consumer that stores spectra or state flushes its own,
  the collection's rule that DSP flushes its own state rather than trusting a register.

## Determinism

`rustfft`'s planner picks its algorithm by the CPU's SIMD features, so results can differ at
roundoff between machines. That is fine for every display and measurement consumer. A consumer
with **bit-exact digests** needs the scalar planner, and each digest names its portability domain
(the archive's `plans/plan-mxm-fft.md` §3). That planner arrives with X5, its first user, not
before.

## Dependencies and build

- `rustfft = "=6.4.1"` (MIT OR Apache-2.0, MSRV 1.61) and `realfft = "=3.5.0"` (MIT), read from
  crates.io on 2026-10-12 and confirmed at X0. Pure Rust with runtime SIMD detection.
- **MSRV 1.87**, verified with `cargo +1.87.0 test -p mxm-fft`.
- **Optimise both in dev builds** (`[profile.dev.package.rustfft]` and `realfft`, `opt-level = 3`):
  Cargo reads only the root workspace's profiles, so the kit sets them for its tests and every
  consumer sets them in its own root. An editor's analyser transforms every frame, and unoptimised it
  stutters.

# Verification

```bash
cargo test -p mxm-fft
cargo +1.87.0 test -p mxm-fft
cargo clippy -p mxm-fft --all-targets -- -D warnings
```

Windows is verified here. Linux and macOS are not verified by anything today.

# Child DOX Index

None.
