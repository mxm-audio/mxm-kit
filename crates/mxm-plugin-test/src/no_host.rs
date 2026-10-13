//! The host a headless editor test hands its `ParamSetter` when nothing it does should be recorded.

use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::{PluginApi, PluginState};

/// A GUI context that takes every edit and does nothing with it.
///
/// **For laying a panel out, painting it or walking its tree**, where the editor reports edits through
/// a `ParamSetter` and the test does not care about them. A test that asserts on edits takes
/// [`crate::keyboard_checks::Recorder`] instead, or a host of its own that applies them. It was
/// eighteen identical copies, one per plugin test module.
pub struct NoHost;

impl nice_plug::context::gui::GuiContextInner for NoHost {
    // A test or offline host has no host to ask for a restart (nice-plug 0.4).
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: Default::default(),
            fields: Default::default(),
        }
    }
    fn set_state(&self, _state: PluginState) {}
}
