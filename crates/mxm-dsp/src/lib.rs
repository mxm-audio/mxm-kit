//! The collection's shared DSP primitives.
//!
//! The synth DSP crates were built as **honest copies**: each copied its siblings' primitives whole,
//! so that a shared crate could be extracted on the evidence of copies that proved identical. Every
//! item here is such a copy, moved verbatim (the architecture audit's O1 and O2); a copy that differs
//! stays with its instrument.
//!
//! - [`guard`]: keeping recursive state out of the denormal range, and non-finite values out.
//! - [`delay`]: the line read by age that the effects' and synths' delays store their samples in.
//! - [`shape`]: the saturators and the prewarp the filters share.
//! - [`blep`]: the PolyBLEP residual and the pulse-width clamp it needs.
//! - [`envelope`]: the analog-style ADSR.
//!
//! Plain values in, plain values out: no framework types, no allocation after construction (a
//! [`delay::AgeLine`] allocates in `new` alone), and no panic on any input a function's own
//! documentation admits (`diode_clamp`'s knee is positive, as every caller passes).

#![forbid(unsafe_code)]

pub mod blep;
pub mod delay;
pub mod envelope;
pub mod guard;
pub mod shape;
