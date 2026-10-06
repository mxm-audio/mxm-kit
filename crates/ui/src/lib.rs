//! Shared interface foundation for the MXM Synth Collection.
//!
//! Implements [`docs/MXM_DESIGN_SYSTEM.md`]: semantic theme tokens with Dark/Light
//! resolution, typography, the app-bar/view-bar/module-card shell, and the basic
//! parameter controls. Per design system §13 these define the collection and
//! therefore live here from day one, rather than waiting for a second consumer.
//!
//! # What this crate may depend on
//!
//! **`egui` and nothing else.** Not the player, not `clack`, not a plugin framework, and above
//! all **no windowing crate**. The same controls are drawn by the player as a pane today and by
//! mxm-mono-01's editor inside a DAW's window later, so nothing here may assume it owns a window, an
//! event loop or a swapchain. A dependency that pulled in a platform backend would foreclose that
//! before it was built. [`crate::tests::the_crate_is_windowing_free`] is the mechanical check.
//!
//! # What this crate owns, and what it does not
//!
//! Owns geometry, visual state, focus treatment, interaction conventions, token resolution and
//! typography. Does **not** own labels, parameter bindings or data — callers supply those, which
//! is why every control takes a value and returns what changed rather than reaching for a host.
//!
//! [`docs/MXM_DESIGN_SYSTEM.md`]: ../../../docs/MXM_DESIGN_SYSTEM.md

pub mod browser;
pub mod control;
pub mod flow;
pub mod navigation;
pub mod offthread;
pub mod paging;
pub mod pilot;
pub mod shell;
pub mod space;
pub mod theme;
pub mod tree;
pub mod typography;
pub mod visual;

pub use control::{ControlOutcome, ParamView, Size, Wheel};
pub use shell::{AppBar, ModuleCard, ViewBar};
pub use theme::{DARK, LIGHT, Tokens};

#[cfg(test)]
mod tests {
    /// The does-not-foreclose obligation, mechanically.
    ///
    /// Native GUI hosting is deferred, but nothing here may make it harder. The concrete way that
    /// would happen is a dependency that drags in a platform windowing backend, after which the
    /// controls could no longer be drawn into a window somebody else owns. `egui` is a pure
    /// immediate-mode painter with no such backend; `eframe` is the crate that has one, and it is
    /// deliberately absent.
    #[test]
    fn the_crate_is_windowing_free() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in [
            "eframe",
            "winit",
            "glutin",
            "sdl2",
            "baseview",
            "raw-window-handle",
            "clack",
            "mxm-player",
            "nice-plug",
        ] {
            assert!(
                !manifest.contains(forbidden),
                "`{forbidden}` in mxm-ui's manifest would foreclose hosting the editor in a \
                 window this crate does not own"
            );
        }
    }
}
