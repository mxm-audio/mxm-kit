//! The collection's shared DSP primitives.
//!
//! The synth DSP crates were built as **honest copies**: each copied its siblings' primitives whole,
//! so that a shared crate could be extracted on the evidence of copies that proved identical. Every
//! item here is such a copy, moved verbatim (the architecture audit's O1); a copy that differs stays
//! with its instrument.
//!
//! - [`guard`]: keeping recursive state out of the denormal range.
//! - [`shape`]: the saturators and the prewarp the filters share.
//! - [`blep`]: the PolyBLEP residual and the pulse-width clamp it needs.
//! - [`envelope`]: the analog-style ADSR.
//!
//! Plain values in, plain values out: no framework types, no allocation, nothing that can panic on a
//! finite input.

#![forbid(unsafe_code)]

pub mod blep;
pub mod envelope;
pub mod guard;
pub mod shape;
