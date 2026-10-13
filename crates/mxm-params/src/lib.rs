//! What a parameter is to a control, how an editor binds one, and how it reads — one crate for every
//! instrument and effect (`plans/plan-mxm-params.md`).
//!
//! - [`erased`]: [`ErasedParam`], the slice of a nice-plug parameter a control or a preset needs, and
//!   [`StepLaw`], the keyboard's musical step laws.
//! - [`binding`]: the collection's one parameter binding — every editor binds through it.
//! - [`text`]: the collection's value text — readings and parsers the plugins share.
//!
//! These lived in `mxm-preset`, which had named nice-plug and drawn with `mxm-ui` first; neither is a
//! preset, and `mxm-modulation-params` depended on the whole preset system for `ErasedParam` alone.
//! `crates/ui` may name no plugin framework, so they cannot live there.

pub mod binding;
pub mod erased;
pub mod text;

pub use erased::{ErasedParam, StepLaw};
