//! Rules being tried on **one** editor before the collection adopts them.
//!
//! The owner, 2026-09-24: *"You are testing the rules on all the synths when we dont even know if
//! the mono/08 is alright. It is too soon."* A rule that will belong to every editor is built here,
//! in the shared code, but only draws where an editor has turned the pilot on — so the one being
//! judged shows it and every other editor draws exactly as it did. Once the owner approves the
//! pilot, rolling a rule out is deleting its `on` check; the collection then behaves as one again.
//!
//! **Nothing is piloted now.** The first three rules — a slider's reading never drawing over its
//! name, a route stack's target line titling its box with each row naming only its source, and a
//! route's minimum track — were piloted on mxm-mono-08 and rolled out to every editor on 2026-09-24
//! (`plans/plan-editor-standard.md` R1). mxm-mono-08 keeps the pilot on: it is where the next rule
//! is tried.

use egui::{Context, Id, Ui};

fn key() -> Id {
    Id::new("mxm-ui-pilot")
}

/// Turns the pilot on for this editor's context. An editor window is its own context, so this
/// reaches that editor and no other.
pub fn enable(ctx: &Context) {
    ctx.data_mut(|data| data.insert_temp(key(), true));
}

/// Whether the editor drawing into `ui` is the pilot.
#[must_use]
pub fn on(ui: &Ui) -> bool {
    ui.ctx()
        .data(|data| data.get_temp::<bool>(key()))
        .unwrap_or(false)
}
