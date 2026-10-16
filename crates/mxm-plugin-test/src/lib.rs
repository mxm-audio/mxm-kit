//! The headless checks every plugin's tests share: the keyboard cursor's coverage, paging and the
//! opening size, layout-tree cards, route parameters against the DSP's declaration, every
//! parameter's text and time readings through the host's text round trip, the plugin's entry in `bundler.toml`, the words a player
//! reads on hover and in a host's browser, and the VST3 identity held to the CLAP one.
//!
//! A `[dev-dependencies]` entry only, so nothing here reaches a bundle. One copy for every plugin:
//! a check trimmed or varied per consumer is how one shared rule quietly becomes twenty.

pub mod bundle;
pub mod formats;
pub mod hover_text;
pub mod keyboard_checks;
pub mod no_host;
pub mod opening_size;
pub mod paging_checks;
pub mod routing_checks;
pub mod text_checks;
pub mod time_text_checks;
pub mod tree_checks;

pub use no_host::NoHost;
