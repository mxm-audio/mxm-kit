//! The plugin editor's window, the same for every MXM plugin.
//!
//! Every editor was the same egui app written again around its own panel: the window's size and
//! resize policy, the setup a window applies when it opens, holding the host's context while it is
//! open and dropping it when it closes (the architecture audit's O7). This crate is that app once.
//! A plugin keeps its [`Panel`] — the state its editor carries and what it draws — and its own work
//! when the window opens and closes, which the panel's hooks receive with the host's context.
//!
//! `mxm-ui` stays free of any plugin framework (the player draws it as a pane); this crate is the
//! part that knows nice-plug.

#![forbid(unsafe_code)]

use nice_plug::context::gui::GuiContext;
use nice_plug::editor::SizeConstraints;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::prelude::{ResizeHint, TrackInfo};
use nice_plug_egui::{
    EguiEditor, EguiEditorState, EguiNiceSettings, Frame, NiceEguiApp, RepaintNotifier,
    create_egui_editor,
};

/// What a plugin's editor is: its state, what it draws each frame, and its own work when the window
/// opens and closes. The window's setup and the host context's lifecycle are [`EditorApp`]'s.
pub trait Panel: Send + 'static {
    /// The window has opened and its setup (`mxm_ui::shell::install`) is applied. `window` is its
    /// egui context; `host` the plugin host's, from which a parameter setter comes.
    fn opened(&mut self, window: &egui::Context, host: &GuiContext) {
        let _ = (window, host);
    }

    /// One frame.
    fn ui(&mut self, ui: &mut egui::Ui, host: &GuiContext);

    /// The window is closing; `host` is still the context it was opened with.
    fn closed(&mut self, host: &GuiContext) {
        let _ = host;
    }

    /// The host's track for this instance: its name and colour, from CLAP's `clap.track-info`,
    /// when the plugin is made and whenever the host changes them, whether or not a window is
    /// open (nice-plug hands it to the editor object, which lives as long as the plugin).
    fn track_info_changed(&mut self, info: &TrackInfo) {
        let _ = info;
    }
}

/// The egui app every plugin's editor runs: the window setup, the host context held while the
/// window is open, and the plugin's [`Panel`] inside.
pub struct EditorApp<P: Panel> {
    panel: P,
    host: Option<GuiContext>,
}

impl<P: Panel> EditorApp<P> {
    pub fn new(panel: P) -> Self {
        Self { panel, host: None }
    }

    pub fn panel(&self) -> &P {
        &self.panel
    }

    pub fn panel_mut(&mut self) -> &mut P {
        &mut self.panel
    }

    /// Whether a window is open: between `build` and `editor_closed`.
    pub fn is_open(&self) -> bool {
        self.host.is_some()
    }
}

impl<P: Panel> NiceEguiApp for EditorApp<P> {
    fn build(
        &mut self,
        window: egui::Context,
        host: GuiContext,
        _frame: &mut Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        mxm_ui::shell::install(&window);
        self.panel.opened(&window, &host);
        self.host = Some(host);
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.panel.ui(ui, &host);
        // VIEW past the editor's edge: the keys go back to the host's window, the one this window
        // is transient for or embedded in (newDAWn's `plans/plan-keyboard-reach.md`, S11). Where
        // the system refuses, they stay here, which is all a failure could mean.
        if mxm_ui::navigation::take_leave(ui.ctx()) {
            let _ = frame.baseview_window().focus_owner();
        }
    }

    fn editor_closed(&mut self) {
        if let Some(host) = self.host.take() {
            self.panel.closed(&host);
        }
    }

    fn track_info_changed(&mut self, info: TrackInfo) {
        self.panel.track_info_changed(&info);
    }
}

/// A plugin's editor: the shared app around its panel.
pub type Editor<P> = EguiEditor<EditorApp<P>>;

/// The editor every MXM plugin opens: titled `title`, `reference` logical points at zoom 1,
/// resizable down to `minimum` (the cards reflow; scaling is the app bar's zoom).
pub fn create<P: Panel>(
    title: &str,
    reference: (u32, u32),
    minimum: (u32, u32),
    panel: P,
) -> Option<Editor<P>> {
    let state = EguiEditorState::from_size(LogicalSize::new(reference.0, reference.1), 1.0);
    create_egui_editor(
        state,
        RepaintNotifier::new(),
        EguiNiceSettings {
            title: title.to_owned(),
            resize_hint: ResizeHint {
                size_constraints: SizeConstraints::min_logical_size(LogicalSize::new(
                    minimum.0 as f32,
                    minimum.1 as f32,
                )),
                ..ResizeHint::RESIZABLE
            },
            // The keys a host offers instead of letting the window have them (Ableton Live's
            // letters and Shift with the arrows): those the keyboard language uses.
            host_keys: Some(std::sync::Arc::new(mxm_ui::navigation::takes_key)),
            ..Default::default()
        },
        EditorApp::new(panel),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counting {
        frames: usize,
        track: Option<String>,
    }

    impl Counting {
        fn new() -> Self {
            Counting {
                frames: 0,
                track: None,
            }
        }
    }

    impl Panel for Counting {
        fn ui(&mut self, _ui: &mut egui::Ui, _host: &GuiContext) {
            self.frames += 1;
        }

        fn track_info_changed(&mut self, info: &TrackInfo) {
            self.track = Some(info.name().to_owned());
        }
    }

    #[test]
    fn a_closed_window_draws_nothing_and_closing_it_twice_is_harmless() {
        let mut app = EditorApp::new(Counting::new());
        assert!(!app.is_open());
        app.editor_closed();
        assert!(!app.is_open());
        assert_eq!(app.panel().frames, 0);
    }

    #[test]
    fn the_editor_is_built_with_the_plugins_title_and_sizes() {
        let editor = create("mxm-test", (900, 600), (400, 300), Counting::new());
        assert!(editor.is_some());
    }

    /// The host's track reaches the panel with no window open, and again when it changes.
    #[test]
    fn the_hosts_track_reaches_the_panel_while_closed() {
        let mut app = EditorApp::new(Counting::new());
        app.track_info_changed(TrackInfo::new("Bass", None));
        assert!(!app.is_open());
        assert_eq!(app.panel().track.as_deref(), Some("Bass"));
        app.track_info_changed(TrackInfo::new("Bass 2", None));
        assert_eq!(app.panel().track.as_deref(), Some("Bass 2"));
        assert_eq!(app.panel().frames, 0);
    }
}
