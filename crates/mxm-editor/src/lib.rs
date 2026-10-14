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
use nice_plug::prelude::ResizeHint;
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

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.panel.ui(ui, &host);
    }

    fn editor_closed(&mut self) {
        if let Some(host) = self.host.take() {
            self.panel.closed(&host);
        }
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
    }

    impl Panel for Counting {
        fn ui(&mut self, _ui: &mut egui::Ui, _host: &GuiContext) {
            self.frames += 1;
        }
    }

    #[test]
    fn a_closed_window_draws_nothing_and_closing_it_twice_is_harmless() {
        let mut app = EditorApp::new(Counting { frames: 0 });
        assert!(!app.is_open());
        app.editor_closed();
        assert!(!app.is_open());
        assert_eq!(app.panel().frames, 0);
    }

    #[test]
    fn the_editor_is_built_with_the_plugins_title_and_sizes() {
        let editor = create("mxm-test", (900, 600), (400, 300), Counting { frames: 0 });
        assert!(editor.is_some());
    }
}
