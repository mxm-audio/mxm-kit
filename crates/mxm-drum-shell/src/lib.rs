//! What the collection's sixteen-slot drum plugins share on the plugin side.
//!
//! `mxm-drum-machine` built its slot names and Resample first, and `mxm-model-drums` took them on
//! 2026-10-26; every fix since went into both. These are the parts that proved the same between
//! them, moved here (`plans/plan-drum-shell.md` in the workspace's ops; the evidence in this
//! crate's `AGENTS.md`):
//!
//! - [`slot_names`]: each slot's name, typed or made up from its model's kind, and the host's note
//!   names;
//! - [`bank`]: the lock-free handover of a captured kit to the audio thread;
//! - [`capture`]: the plugin's own capture thread, the kit each engage heard, and Resample's state
//!   machine;
//! - [`pack`]: writing a captured kit out as a sample pack;
//! - [`signals`]: the telemetry those share between the audio thread, the capture thread and the
//!   editor;
//! - [`editor`]: the app bar's Resample and Export, and the slot rows' name box.
//!
//! **What stays with each plugin** is everything its engine decides: the capture itself and the
//! frozen reader that plays it (each DSP crate's `capture`), its parameters and their IDs, its
//! presets and its editor's layout. A plugin plugs in through three traits:
//! [`slot_names::DrumSlots`] (its slots), [`capture::Capturer`] (its capture, and what its engine
//! and its kit must do) and [`pack::OneShots`] (its one-shots).

pub mod bank;
pub mod capture;
pub mod editor;
pub mod pack;
pub mod signals;
pub mod slot_names;

pub use mxm_drum_engine::SLOT_COUNT;
