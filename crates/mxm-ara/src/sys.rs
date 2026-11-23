//! ARA 2.3's C API, as bindgen reads `ara_api/ARAInterface.h` (`tools/gen-bindings.sh`).
//!
//! The header packs its structs to 1 byte on x86 and to 8 on ARM, so there is one file per
//! architecture; each checks its own layouts at compile time. Packed structs (x86): read a field
//! by value, never take a reference to it.

#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    unsafe_op_in_unsafe_fn,
    clippy::all
)]

#[cfg(target_arch = "x86_64")]
include!("sys/x86_64.rs");

#[cfg(target_arch = "aarch64")]
include!("sys/aarch64.rs");

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("mxm-ara has ARA bindings for x86_64 and aarch64 only (tools/gen-bindings.sh)");
