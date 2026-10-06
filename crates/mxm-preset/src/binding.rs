//! Binding one nice-plug parameter to one `mxm-ui` control — **the collection's one binding**.
//!
//! Until 2026-09-24 every editor carried its own copy of this module: eighteen of them, the effects'
//! within ten lines of each other and the instruments' drifted by up to five hundred. The owner's
//! standardisation principle — *"I want to make a 100 plugins. And they should all look, feel and
//! work the same"* — made it one (`plans/plan-editor-standard.md` R1c, in the private archive). It
//! lives here because this crate already names nice-plug and draws with `mxm-ui`; `crates/ui` may
//! name neither. A plugin's `binding` module re-exports it, and keeps only what is its own.
//!
//! # The one place gestures are bracketed
//!
//! `mxm-ui` reports what a control did; somebody has to turn that into
//! `begin_set_parameter` / `set_parameter_normalized` / `end_set_parameter`. Doing it per control
//! would mean one chance per control to leave a host's automation lane latched, which is silent
//! until someone records over it. So it happens exactly once, in [`Bound::apply`], and once for
//! each stepped control below.
//!
//! # A painted label and a canonical name
//!
//! Design system §7.1 (the owner, 2026-09-24: *"It is inside the Complex oscillator card, so it is
//! self evident"*): a control on a module's card paints its name without the module, and the
//! parameter's own name stays what the host, the tooltip and a screen reader read. [`Bound::panel`]
//! is the painted label; every drawer paints it, and the stepped controls take one as `panel`.
//!
//! # Descriptions are written by the plugin because only the plugin has them
//!
//! Design system §7.1 requires a one-sentence description in every tooltip. CLAP carries no such
//! field, so a host cannot supply one — which is the concrete reason an editor belongs to its plugin
//! rather than to the player. Each binding carries its own sentence.

use std::borrow::Cow;
use std::collections::HashMap;

use egui::Ui;
use mxm_ui::control::{self, ParamView, Size, Wheel};
use mxm_ui::theme::Tokens;
use nice_plug::prelude::ParamSetter;

pub use crate::erased::{ErasedParam, StepLaw};

/// One parameter, ready to draw.
///
/// Borrowed for the frame. The `id` is the parameter's stable CLAP id, which doubles as the key
/// for per-control editor state — text-entry buffers survive a reflow because the id does not
/// depend on where the control was drawn.
pub struct Bound<'a> {
    pub id: &'static str,
    pub param: &'a dyn ErasedParam,
    pub description: &'a str,
    /// What the control paints, where its card already says the rest (design system §7.1); `None`
    /// paints the parameter's own name. The canonical name stays the accessible name and the
    /// tooltip's, whatever this says.
    pub panel: Option<Cow<'a, str>>,
    /// Draws a parameter whose rest is the centre, not the minimum. See [`Bound::bipolar`].
    pub bipolar: bool,
    /// How the keyboard steps it: its own step, or a musical law its owner declared. See
    /// [`Bound::law`].
    pub law: StepLaw,
    /// Normalised `(fine, coarse)` steps an edit in the editor moves by, where the parameter itself
    /// is finer. See [`Bound::stepped`].
    pub stepped: Option<(f64, f64)>,
    /// A stepped parameter's **one sentence per option**, what each does, which its segmented
    /// control shows on that option's cell (design system §7.3; the owner, 2026-09-27). Empty for a
    /// continuous one. See [`Bound::details`].
    pub details: &'a [&'a str],
}

/// The keyboard's step law for one parameter, asked once per press from wherever the press starts.
/// A [`Bound::stepped`] grid outranks the law: it is the owner's own override.
impl control::NextValue for Bound<'_> {
    fn next_value(&self, normalised: f64, press: control::Press) -> f64 {
        if let Some((fine, coarse)) = self.stepped {
            // `Alt` has nothing finer than the grid's own step, which a press never goes under.
            let size = if press.coarse && !press.finer {
                coarse
            } else {
                fine
            };
            return (normalised + if press.up { size } else { -size }).clamp(0.0, 1.0);
        }
        self.param.step_from(normalised, press, self.law)
    }
}

impl<'a> Bound<'a> {
    pub fn new(id: &'static str, param: &'a dyn ErasedParam, description: &'a str) -> Self {
        Self {
            id,
            param,
            description,
            panel: None,
            bipolar: false,
            law: StepLaw::Own,
            stepped: None,
            details: &[],
        }
    }

    /// What each option of a stepped parameter does, one sentence each, in the parameter's own
    /// order: its segmented control shows each on its own cell, never one sentence for the row.
    #[must_use]
    pub fn details(mut self, details: &'a [&'a str]) -> Self {
        self.details = details;
        self
    }

    /// Marks a parameter whose meaningful rest position is the centre, not the minimum: a signed
    /// depth runs −100 % to +100 % and its centre is "no modulation", so a half-filled arc would
    /// read as "half on" when it means "off".
    #[must_use]
    pub fn bipolar(mut self) -> Self {
        self.bipolar = true;
        self
    }

    /// Steps this parameter from the keyboard by a musical law rather than its own step. See
    /// [`StepLaw`]. The owner declares it where the parameter is bound; nothing guesses it from a
    /// unit, which cannot tell a cutoff from an LFO rate.
    #[must_use]
    pub fn law(mut self, law: StepLaw) -> Self {
        self.law = law;
        self
    }

    /// Paints `label` where the parameter's name would be — the short form its card allows.
    #[must_use]
    pub fn labelled(mut self, label: impl Into<Cow<'a, str>>) -> Self {
        self.panel = Some(label.into());
        self
    }

    /// Paints the parameter's own name: for a surface with no card to shorten it, the developer
    /// Parameters list.
    #[must_use]
    pub fn unlabelled(mut self) -> Self {
        self.panel = None;
        self
    }

    /// Edits in the editor move by whole `fine` steps, and the keyboard's coarse press by `coarse`,
    /// both normalised, **while the parameter itself holds anything between** — mxm-creative-sampler's
    /// Root holds cents so a loop between notes plays in tune, and a knob turned by hand still lands
    /// on notes. A typed value keeps what was typed.
    #[must_use]
    pub fn stepped(mut self, fine: f64, coarse: f64) -> Self {
        self.stepped = Some((fine, coarse));
        self
    }

    /// What the control paints: [`Bound::panel`], or the parameter's own name.
    #[must_use]
    pub fn painted(&self) -> &str {
        self.panel.as_deref().unwrap_or_else(|| self.param.name())
    }

    /// `normalised`, on this control's step grid when it has one.
    fn on_step(&self, normalised: f64) -> f32 {
        match self.stepped {
            Some((fine, _)) if fine > 0.0 => (normalised / fine).round() * fine,
            _ => normalised,
        }
        .clamp(0.0, 1.0) as f32
    }

    /// The value this drag started from, remembered so the delta can be drawn while it lasts.
    ///
    /// **In egui's own memory, keyed by the parameter**, because the editor is a panel that owns no
    /// state between frames. In the common case the drag starts from the patch, so the line is the
    /// deviation being authored, live: *"it should show while dragging it"*.
    fn drag_origin_id(&self) -> egui::Id {
        egui::Id::new(("mxm-drag-origin", self.id))
    }

    /// A one-frame view of this parameter, painted as [`Bound::panel`] says.
    ///
    /// Built inline from borrows that all live as long as the call, which is why it takes `name`
    /// and `text` rather than reaching for them: `ErasedParam::name` borrows the trait object for
    /// the call, not for `'a`.
    fn view<'v>(&'v self, name: &'v str, text: &'v str) -> ParamView<'v> {
        let (fine_up, fine_down, coarse_up, coarse_down) = match self.stepped {
            Some((fine, coarse)) => (fine, fine, coarse, coarse),
            None => self.param.stepping(),
        };
        // **Marked when something else is moving it.** In the MXM player that something is the step
        // sequencer, and the mark is exactly "this step deviates from the patch" — but the editor
        // knows only that the parameter is at one value and is being played at another, which is
        // true in any host that modulates.
        let view = ParamView::new(name, text, self.description)
            .default_at(f64::from(reset_target(self.param)))
            .marked(self.param.modulation() != 0.0)
            .modulated_by(f64::from(self.param.modulation()))
            // How far one keyboard press moves this parameter. Computed from the parameter itself
            // every frame, at its current value, because a skewed range does not step by the same
            // amount at both ends.
            .stepping(control::Steps {
                fine_up,
                fine_down,
                coarse_up,
                coarse_down,
            })
            // And where each press lands, asked once per press, so several presses in one frame
            // follow a law that depends on where it starts. See [`StepLaw`].
            .stepping_by(self);
        let view = match self.panel.as_deref() {
            Some(panel) => view.labelled(panel),
            None => view,
        };
        if self.bipolar { view.bipolar() } else { view }
    }

    /// The modulation to draw this frame: the host's, or — while a drag is open — the distance back
    /// to where the drag began. The two never overlap, because a dragged parameter is not modulated.
    fn shown_modulation(&self, ui: &Ui) -> f32 {
        let host = self.param.modulation();
        if host != 0.0 {
            return host;
        }
        let origin: Option<f32> = ui.data(|d| d.get_temp(self.drag_origin_id()));
        origin.map_or(0.0, |origin| origin - self.param.normalised())
    }

    /// Keeps the drag-origin record in step with the gesture.
    fn track_drag(&self, ui: &Ui, outcome: &mxm_ui::ControlOutcome, before: f32) {
        if outcome.gesture_started {
            ui.data_mut(|d| d.insert_temp(self.drag_origin_id(), before));
        }
        if outcome.gesture_ended {
            ui.data_mut(|d| d.remove_temp::<f32>(self.drag_origin_id()));
        }
    }

    /// This control's text-entry buffer, **closed by `Escape`** before the control draws — the
    /// cancel §7.1's direct entry needs, and mxm-mono-08's rule, every editor's since 2026-09-24.
    fn entry<'e>(
        &self,
        ui: &Ui,
        text_entry: &'e mut HashMap<&'static str, Option<String>>,
    ) -> &'e mut Option<String> {
        let entry = text_entry.entry(self.id).or_default();
        if entry.is_some()
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            *entry = None;
        }
        entry
    }

    /// Draws a knob and applies whatever it did.
    pub fn knob(
        &self,
        ui: &mut Ui,
        tokens: &Tokens,
        setter: &ParamSetter<'_>,
        size: Size,
        column: f32,
        text_entry: &mut HashMap<&'static str, Option<String>>,
    ) {
        self.knob_reading(ui, tokens, setter, size, column, text_entry, None);
    }

    /// Draws a knob with a context-dependent panel reading while retaining the parameter's
    /// canonical host text for exact entry and automation (mxm-drum-machine's synced LFO rate).
    #[allow(clippy::too_many_arguments)]
    pub fn knob_with_reading(
        &self,
        ui: &mut Ui,
        tokens: &Tokens,
        setter: &ParamSetter<'_>,
        size: Size,
        column: f32,
        text_entry: &mut HashMap<&'static str, Option<String>>,
        reading: &str,
    ) {
        self.knob_reading(ui, tokens, setter, size, column, text_entry, Some(reading));
    }

    #[allow(clippy::too_many_arguments)]
    fn knob_reading(
        &self,
        ui: &mut Ui,
        tokens: &Tokens,
        setter: &ParamSetter<'_>,
        size: Size,
        column: f32,
        text_entry: &mut HashMap<&'static str, Option<String>>,
        reading: Option<&str>,
    ) {
        let name = self.param.name().to_owned();
        let formatted = self.param.text();
        let text = reading.unwrap_or(&formatted);
        let before = self.param.normalised();
        let view = self
            .view(&name, text)
            .modulated_by(f64::from(self.shown_modulation(ui)));
        let mut normalised = f64::from(before);
        let entry = self.entry(ui, text_entry);

        // Names the parameter for the keyboard cursor's registry, which is built by drawing
        // rather than declared in a table. See `mxm_ui::navigation`.
        let outcome = mxm_ui::navigation::at(ui, self.id, |ui| {
            control::knob(
                ui,
                tokens,
                &view,
                &mut normalised,
                size,
                column,
                entry,
                Wheel::Off,
            )
        });
        self.track_drag(ui, &outcome, before);
        self.apply(setter, outcome, normalised, entry);
    }

    /// Draws a slider across the width it is given and applies whatever it did.
    pub fn slider(
        &self,
        ui: &mut Ui,
        tokens: &Tokens,
        setter: &ParamSetter<'_>,
        text_entry: &mut HashMap<&'static str, Option<String>>,
    ) {
        let name = self.param.name().to_owned();
        let text = self.param.text();
        let before = self.param.normalised();
        let view = self
            .view(&name, &text)
            .modulated_by(f64::from(self.shown_modulation(ui)));
        let mut normalised = f64::from(before);
        let entry = self.entry(ui, text_entry);
        let width = ui.available_width();

        let outcome = mxm_ui::navigation::at(ui, self.id, |ui| {
            control::slider(ui, tokens, &view, &mut normalised, width, entry, Wheel::Off)
        });
        self.track_drag(ui, &outcome, before);
        self.apply(setter, outcome, normalised, entry);
    }

    /// Draws a one-line slider, for the app bar. See [`control::slider_inline`].
    ///
    /// `track` is the track's width rather than the control's: the name and value sit beside it,
    /// not above it.
    pub fn slider_inline(
        &self,
        ui: &mut Ui,
        tokens: &Tokens,
        setter: &ParamSetter<'_>,
        text_entry: &mut HashMap<&'static str, Option<String>>,
        track: f32,
    ) {
        let name = self.param.name().to_owned();
        let text = self.param.text();
        let before = self.param.normalised();
        // The widest reading holds the bar still while the value changes under a drag.
        let widest = control::widest_value(|normalised| self.param.format(normalised as f32));
        let view = self
            .view(&name, &text)
            .widest(&widest)
            .modulated_by(f64::from(self.shown_modulation(ui)));
        let mut normalised = f64::from(before);
        let entry = self.entry(ui, text_entry);

        let outcome = mxm_ui::navigation::at(ui, self.id, |ui| {
            control::slider_inline(ui, tokens, &view, &mut normalised, track, entry, Wheel::Off)
        });
        self.track_drag(ui, &outcome, before);
        self.apply(setter, outcome, normalised, entry);
    }

    /// Draws the parameter as a vertical fader in a fixed-width column. See
    /// [`control::slider_vertical`].
    ///
    /// `label` is what the column says — a step number, where the card already says *Steps* and
    /// the parameter is *Step 3 level*; it outranks [`Bound::panel`]. The full name stays in
    /// `WidgetInfo` and the tooltip.
    #[allow(clippy::too_many_arguments)]
    pub fn slider_vertical(
        &self,
        ui: &mut Ui,
        tokens: &Tokens,
        setter: &ParamSetter<'_>,
        text_entry: &mut HashMap<&'static str, Option<String>>,
        label: &str,
        width: f32,
        height: f32,
    ) {
        let name = self.param.name().to_owned();
        let text = self.param.text();
        let before = self.param.normalised();
        let view = self
            .view(&name, &text)
            .labelled(label)
            .modulated_by(f64::from(self.shown_modulation(ui)));
        let mut normalised = f64::from(before);
        let entry = self.entry(ui, text_entry);

        let outcome = mxm_ui::navigation::at(ui, self.id, |ui| {
            control::slider_vertical(
                ui,
                tokens,
                &view,
                &mut normalised,
                width,
                height,
                entry,
                Wheel::Off,
            )
        });
        self.track_drag(ui, &outcome, before);
        self.apply(setter, outcome, normalised, entry);
    }

    /// Turns a [`ControlOutcome`](mxm_ui::ControlOutcome) into host automation.
    ///
    /// The order matters and is the whole point of this function existing once: begin before any
    /// set, end after the last one. `mxm-ui` guarantees an instantaneous edit reports all three in
    /// the same frame, so a double-click reset or a committed text entry is a complete, balanced
    /// gesture rather than an open one.
    fn apply(
        &self,
        setter: &ParamSetter<'_>,
        outcome: mxm_ui::ControlOutcome,
        normalised: f64,
        entry: &mut Option<String>,
    ) {
        // A committed text entry arrives as an instantaneous outcome with the buffer still open.
        // Parsing is the plugin's, because the plugin owns the formatting that produced the text,
        // and a typed value is not put on a stepped grid: what was typed is what was meant.
        if outcome.changed && entry.is_some() {
            let parsed = entry.as_deref().and_then(|text| self.param.parse(text));
            *entry = None;
            let Some(value) = parsed else {
                // Unparseable input is discarded rather than clamped to something arbitrary, and
                // the control simply keeps its value. Guessing is worse than doing nothing.
                return;
            };
            self.param.begin(setter);
            self.param.set(setter, value);
            self.param.end(setter);
            return;
        }

        if outcome.gesture_started {
            self.param.begin(setter);
        }
        if outcome.changed {
            self.param.set(setter, self.on_step(normalised));
        }
        if outcome.gesture_ended {
            self.param.end(setter);
        }
    }
}

/// Where a double-click puts the parameter: **the patch while something else is sounding it,
/// the factory default otherwise.**
///
/// The player's rule, translated to what the editor can see: §7.1's double-click is
/// *reinterpreted, not duplicated* — with a step selected, "put this back" means the patch's
/// value, never the factory default. While a host modulates the parameter the unmodulated base
/// **is** the patch, so aiming the reset there is the same rule from this side of the wall.
///
/// Shipped wrong once: buttons reset to the factory default while a Range lock was sounding —
/// "when I double click it goes to 8'" — and the widget test verified the mechanism without
/// asking whether the target matched the model.
#[must_use]
pub fn reset_target(param: &dyn ErasedParam) -> f32 {
    if param.modulation() != 0.0 {
        param.normalised()
    } else {
        param.default_normalised()
    }
}

/// Which cell the *modulated* value lands on, when that differs from the set one.
///
/// This is a stepped parameter's whole modulation display: there is no arc to draw between cells,
/// so the widget shows the sounding cell ringed and the row marked. `None` when nothing modulates
/// the parameter, or when the offset is too small to reach a different cell — a deviation that
/// rounds to the same choice is the same sound.
#[must_use]
pub fn sounding_cell(param: &dyn ErasedParam, last: f32, selected: usize) -> Option<usize> {
    let modulation = param.modulation();
    if modulation == 0.0 {
        return None;
    }
    let cell = ((param.normalised() + modulation).clamp(0.0, 1.0) * last).round() as usize;
    (cell != selected).then_some(cell)
}

/// A stepped parameter's cells: its index now, the sounding one, the reset one and the last.
fn cells(param: &dyn ErasedParam, count: usize, what: &str) -> (usize, Option<usize>, usize, f32) {
    debug_assert_eq!(
        param.steps().map(|s| s + 1),
        Some(count),
        "the {what} list and the parameter's step count disagree for {}",
        param.name()
    );
    let last = (count - 1) as f32;
    let selected = (param.normalised() * last).round() as usize;
    let sounding = sounding_cell(param, last, selected);
    let default_cell = (reset_target(param) * last).round() as usize;
    (selected, sounding, default_cell, last)
}

/// An instantaneous write — a segment click, a menu choice, a switch — as one complete gesture: it
/// has no press-and-hold phase to bracket.
fn write(param: &dyn ErasedParam, setter: &ParamSetter<'_>, normalised: f32) {
    param.begin(setter);
    param.set(setter, normalised);
    param.end(setter);
}

/// Draws a stepped parameter as a segmented control, §7.3.
///
/// Separate from [`Bound`] because a segmented control edits an index rather than a continuous
/// value, and pretending otherwise would put a drag gesture on a mode switch. Beside a knob the
/// control is laid out on the knob's grid — label on its name line, cells on its circle — which is
/// the shared crate's rule and its measurement, not a nudge here.
#[allow(clippy::too_many_arguments)]
pub fn segmented(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    options: &[&str],
    beside: Option<Size>,
    details: &[&str],
    setter: &ParamSetter<'_>,
) {
    let (mut selected, sounding, default_cell, last) = cells(param, options.len(), "option");
    let changed = mxm_ui::navigation::at(ui, id, |ui| match beside {
        Some(beside) => control::segmented_beside(
            ui,
            tokens,
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            beside,
            details,
        ),
        None => control::segmented(
            ui,
            tokens,
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            details,
        ),
    });
    if changed {
        write(param, setter, selected as f32 / last);
    }
}

/// [`segmented`], painting `panel` where the parameter's name would be — the short form its card
/// allows — every cell at least `cell` wide: `0.0` for a control standing alone,
/// [`control::shared_cell_width`] for one in a stack.
#[allow(clippy::too_many_arguments)]
pub fn segmented_named(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    panel: Option<&str>,
    options: &[&str],
    details: &[&str],
    setter: &ParamSetter<'_>,
    cell: f32,
) {
    let (mut selected, sounding, default_cell, last) = cells(param, options.len(), "option");
    if mxm_ui::navigation::at(ui, id, |ui| {
        control::segmented_named(
            ui,
            tokens,
            panel.unwrap_or(param.name()),
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            cell,
            details,
        )
    }) {
        write(param, setter, selected as f32 / last);
    }
}

/// A stepped waveform parameter drawn as pictures (design system §7.3). The option list is the
/// shapes in the parameter's own order; the names stay as hover text and accessible names.
#[allow(clippy::too_many_arguments)]
pub fn segmented_waves(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    options: &[(control::Wave, &str)],
    beside: Option<Size>,
    details: &[&str],
    setter: &ParamSetter<'_>,
) {
    segmented_waves_named(
        ui, tokens, id, param, None, options, beside, details, setter,
    );
}

/// [`segmented_waves`], painting `panel` where the parameter's name would be.
#[allow(clippy::too_many_arguments)]
pub fn segmented_waves_named(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    panel: Option<&str>,
    options: &[(control::Wave, &str)],
    beside: Option<Size>,
    details: &[&str],
    setter: &ParamSetter<'_>,
) {
    let (mut selected, sounding, default_cell, last) = cells(param, options.len(), "shape");
    if mxm_ui::navigation::at(ui, id, |ui| {
        control::segmented_waves_named(
            ui,
            tokens,
            panel.unwrap_or(param.name()),
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            beside,
            details,
        )
    }) {
        write(param, setter, selected as f32 / last);
    }
}

/// [`segmented_waves`], with a short mark beside each picture — the octave a sub shape sits below
/// the oscillator. See [`control::segmented_waves_marked`].
#[allow(clippy::too_many_arguments)]
pub fn segmented_waves_marked(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    options: &[(control::Wave, &str)],
    marks: &[&str],
    details: &[&str],
    setter: &ParamSetter<'_>,
) {
    let (mut selected, sounding, default_cell, last) = cells(param, options.len(), "shape");
    if mxm_ui::navigation::at(ui, id, |ui| {
        control::segmented_waves_marked(
            ui,
            tokens,
            param.name(),
            options,
            marks,
            &mut selected,
            sounding,
            Some(default_cell),
            None,
            details,
        )
    }) {
        write(param, setter, selected as f32 / last);
    }
}

/// [`segmented_waves`], painting no label; see [`control::segmented_waves_unlabelled`].
pub fn segmented_waves_unlabelled(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    options: &[(control::Wave, &str)],
    details: &[&str],
    setter: &ParamSetter<'_>,
) {
    let (mut selected, sounding, default_cell, last) = cells(param, options.len(), "shape");
    if mxm_ui::navigation::at(ui, id, |ui| {
        control::segmented_waves_unlabelled(
            ui,
            tokens,
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            details,
        )
    }) {
        write(param, setter, selected as f32 / last);
    }
}

/// Draws a stepped parameter as the collection's caret selector, §7.4 — for a list too long for a
/// segmented control, or one whose length is not fixed. The shared control steps to the adjacent
/// option from the keyboard however many there are.
#[allow(clippy::too_many_arguments)]
pub fn selector(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    options: &[&str],
    beside: Option<Size>,
    description: &str,
    setter: &ParamSetter<'_>,
) {
    let (mut selected, sounding, default_cell, last) = cells(param, options.len(), "option");
    let changed = mxm_ui::navigation::at(ui, id, |ui| match beside {
        Some(beside) => control::selector_beside(
            ui,
            tokens,
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            beside,
            description,
        ),
        None => control::selector(
            ui,
            tokens,
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            description,
        ),
    });
    if changed {
        write(param, setter, selected as f32 / last);
    }
}

/// [`selector`], painting `panel` where the parameter's name would be — the short form its card
/// allows — while the name stays its accessible name and hover text.
#[allow(clippy::too_many_arguments)]
pub fn selector_named(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    panel: Option<&str>,
    options: &[&str],
    description: &str,
    setter: &ParamSetter<'_>,
) {
    let (mut selected, sounding, default_cell, last) = cells(param, options.len(), "option");
    if mxm_ui::navigation::at(ui, id, |ui| {
        control::selector_named(
            ui,
            tokens,
            panel.unwrap_or(param.name()),
            param.name(),
            options,
            &mut selected,
            sounding,
            Some(default_cell),
            description,
        )
    }) {
        write(param, setter, selected as f32 / last);
    }
}

/// Draws a boolean parameter as a switch under its own name.
pub fn toggle(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    description: &str,
    setter: &ParamSetter<'_>,
) {
    toggle_labelled(
        ui,
        tokens,
        id,
        param,
        param.name(),
        description,
        setter,
        0.0,
    );
}

/// An on/off switch painting `label` — the parameter's name, or the shorter form its card allows —
/// at least `width` wide, while the parameter's own name stays its accessible name and tooltip.
/// See [`control::toggle_labelled`]. `width` is `0.0` for a switch standing alone and
/// [`control::shared_toggle_width`] for one in a stack.
#[allow(clippy::too_many_arguments)]
pub fn toggle_labelled(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    label: &str,
    description: &str,
    setter: &ParamSetter<'_>,
    width: f32,
) {
    let mut on = param.normalised() >= 0.5;
    if mxm_ui::navigation::at(ui, id, |ui| {
        control::toggle_sized(
            ui,
            tokens,
            label,
            param.name(),
            &mut on,
            param.modulation() != 0.0,
            description,
            width,
        )
    }) {
        write(param, setter, if on { 1.0 } else { 0.0 });
    }
}

/// An on/off switch drawn as a picture (design system §7.3): a pointer-minimum cell rather than a
/// word, with the parameter's own name as its accessible name and hover text.
pub fn toggle_picture(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    wave: control::Wave,
    description: &str,
    setter: &ParamSetter<'_>,
) {
    let mut on = param.normalised() >= 0.5;
    if mxm_ui::navigation::at(ui, id, |ui| {
        control::toggle_wave(
            ui,
            tokens,
            wave,
            param.name(),
            &mut on,
            param.modulation() != 0.0,
            description,
        )
    }) {
        write(param, setter, if on { 1.0 } else { 0.0 });
    }
}

/// What every tempo sync's quarter note says in its tooltip, beside the parameter's own name.
pub const SYNC_DESCRIPTION: &str =
    "Locks the control beside it to the host's tempo, in note lengths.";

/// **A tempo sync's switch**: the quarter note (`Wave::QuarterNote`), the collection's one form for
/// it (`plans/plan-tempo-sync-controls.md`, in the private archive). Drawn beside the control it
/// syncs, on that knob's grid (`mxm_ui::tree::switch_beside_knob`); its accessible name is the
/// parameter's.
pub fn sync_picture(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    param: &dyn ErasedParam,
    setter: &ParamSetter<'_>,
) {
    toggle_picture(
        ui,
        tokens,
        id,
        param,
        control::Wave::QuarterNote,
        SYNC_DESCRIPTION,
        setter,
    );
}

/// The widest reading a syncable control can show: its own widest formatted value, or its span's
/// longest division label when that is wider — so the column holds the reading whichever way the
/// switch is.
#[must_use]
pub fn synced_widest(param: &dyn ErasedParam, span: mxm_tempo::Span) -> String {
    let free = control::widest_value(|n| param.format(n as f32));
    span.divisions()
        .iter()
        .map(|division| division.label())
        .chain(std::iter::once(free.as_str()))
        .max_by_key(|text| text.chars().count())
        .unwrap_or_default()
        .to_owned()
}

/// Draws a boolean parameter as the compact square switch of a dense inventory row, with one short
/// visible mark; the parameter's name stays its accessible name. Returns whether it changed.
pub fn toggle_compact(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    mark: &str,
    param: &dyn ErasedParam,
    description: &str,
    setter: &ParamSetter<'_>,
) -> bool {
    let mut on = param.normalised() >= 0.5;
    let changed = mxm_ui::navigation::at(ui, id, |ui| {
        control::toggle_compact(
            ui,
            tokens,
            mark,
            param.name(),
            &mut on,
            param.modulation() != 0.0,
            description,
        )
    });
    if changed {
        write(param, setter, if on { 1.0 } else { 0.0 });
    }
    changed
}

/// Draws a boolean parameter on the grid of the knob beside it. Returns whether it changed.
#[allow(clippy::too_many_arguments)]
pub fn toggle_beside(
    ui: &mut Ui,
    tokens: &Tokens,
    id: &str,
    label: &str,
    param: &dyn ErasedParam,
    beside: Size,
    description: &str,
    setter: &ParamSetter<'_>,
) -> bool {
    let mut on = param.normalised() >= 0.5;
    let changed = mxm_ui::navigation::at(ui, id, |ui| {
        control::toggle_beside(
            ui,
            tokens,
            label,
            &mut on,
            param.modulation() != 0.0,
            beside,
            description,
        )
    });
    if changed {
        write(param, setter, if on { 1.0 } else { 0.0 });
    }
    changed
}

/// Writes several parameters as one instantaneous edit.
///
/// Every gesture opens before the first value and closes after the last, so a host records one
/// simultaneous change rather than a staggered run of single ones, and no lane is left latched. A
/// parameter already at its target is left out: rewriting it would record automation for nothing.
pub fn set_together(setter: &ParamSetter<'_>, edits: &[(&dyn ErasedParam, f32)]) {
    let changed: Vec<_> = edits
        .iter()
        .filter(|(param, normalised)| param.normalised() != *normalised)
        .collect();
    for (param, _) in &changed {
        param.begin(setter);
    }
    for (param, normalised) in &changed {
        param.set(setter, *normalised);
    }
    for (param, _) in &changed {
        param.end(setter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_ui::control::{NextValue, Press};
    use nice_plug::params::internals::ParamPtr;
    use nice_plug::prelude::{FloatParam, FloatRange, Param, PluginApi, PluginState};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A host that records `begin` as 1, `set` as 2 and `end` as 3, digit by digit, and applies
    /// nothing unless `applies`.
    struct RecordingHost {
        record: AtomicU32,
        applies: bool,
    }

    impl RecordingHost {
        fn new(applies: bool) -> Self {
            Self {
                record: AtomicU32::new(0),
                applies,
            }
        }
        // `fetch_update` is renamed `try_update` in Rust after 1.98; the new name does not exist at
        // this crate's 1.95 floor, so the old one stays until the floor moves past the rename.
        #[allow(deprecated)]
        fn push(&self, event: u32) {
            let _ = self
                .record
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                    Some(old.wrapping_mul(10).wrapping_add(event))
                });
        }
    }

    impl nice_plug::context::gui::GuiContextInner for RecordingHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {
            self.push(1);
        }
        unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
            self.push(2);
            if self.applies {
                // SAFETY: the test owns the parameter behind `param` for the whole call.
                unsafe { param._internal_set_normalized_value(normalized) };
            }
        }
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {
            self.push(3);
        }
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    fn outcome(started: bool, changed: bool, ended: bool) -> mxm_ui::ControlOutcome {
        mxm_ui::ControlOutcome {
            gesture_started: started,
            changed,
            gesture_ended: ended,
            reset: false,
        }
    }

    /// A syncable control's column holds its widest reading either way: the span's longest label
    /// when the free readings are narrower, the free reading when it is wider.
    #[test]
    fn a_synced_control_holds_its_widest_reading_either_way() {
        let narrow = FloatParam::new("Rate", 2.0, FloatRange::Linear { min: 1.0, max: 9.0 })
            .with_value_to_string(std::sync::Arc::new(|v| format!("{v:.0}")));
        assert_eq!(synced_widest(&narrow, mxm_tempo::Span::LFO), "4 bars");
        let wide = FloatParam::new("Rate", 2.0, FloatRange::Linear { min: 0.0, max: 1.0 })
            .with_value_to_string(std::sync::Arc::new(|v| format!("{v:.4} Hz")));
        assert_eq!(synced_widest(&wide, mxm_tempo::Span::LFO), "1.0000 Hz");
    }

    #[test]
    fn every_instant_editor_write_is_one_complete_host_gesture() {
        let param = FloatParam::new("Test", 0.25, FloatRange::Linear { min: 0.0, max: 1.0 });
        let bound = Bound::new("test", &param, "A test parameter.");
        let host = RecordingHost::new(false);
        let setter = ParamSetter::new(&host);
        bound.apply(&setter, outcome(true, true, true), 0.75, &mut None);
        assert_eq!(host.record.load(Ordering::SeqCst), 123);
    }

    #[test]
    fn a_drag_opens_once_changes_and_closes_once() {
        let param = FloatParam::new("Test", 0.25, FloatRange::Linear { min: 0.0, max: 1.0 });
        let bound = Bound::new("test", &param, "A test parameter.");
        let host = RecordingHost::new(false);
        let setter = ParamSetter::new(&host);
        bound.apply(&setter, outcome(true, false, false), 0.25, &mut None);
        bound.apply(&setter, outcome(false, true, false), 0.5, &mut None);
        bound.apply(&setter, outcome(false, true, true), 0.75, &mut None);
        assert_eq!(host.record.load(Ordering::SeqCst), 1223);
        assert_eq!(
            param.unmodulated_normalized_value(),
            0.25,
            "the host stub records but deliberately does not apply"
        );
    }

    /// **A drag on a stepped control lands on its step; a typed value keeps what was typed**:
    /// mxm-creative-sampler's Root holds cents so an imported loop plays in tune, and the step is
    /// what still lands a hand on a note.
    #[test]
    fn a_stepped_control_lands_a_drag_on_its_step_and_keeps_a_typed_value() {
        let root = FloatParam::new(
            "Root",
            60.0,
            FloatRange::Linear {
                min: 0.0,
                max: 127.0,
            },
        );
        let note = 1.0 / 127.0;
        let host = RecordingHost::new(true);
        let setter = ParamSetter::new(&host);
        let drag = |bound: &Bound<'_>, normalised: f64, typed: Option<&str>| {
            let mut entry = typed.map(str::to_owned);
            bound.apply(&setter, outcome(true, true, true), normalised, &mut entry);
            root.value()
        };
        let close = |heard: f32, wanted: f32, what: &str| {
            assert!(
                (heard - wanted).abs() < 1.0e-3,
                "{what}: Root {heard}, wanted {wanted}"
            );
        };
        let stepped = Bound::new("root", &root, "").stepped(note, 12.0 * note);
        close(
            drag(&stepped, 59.6 * note, None),
            60.0,
            "a drag between notes",
        );
        close(
            drag(&stepped, 59.4 * note, None),
            59.0,
            "a drag below the half",
        );
        close(drag(&stepped, 0.0, Some("59.75")), 59.75, "a typed Root");
        let plain = Bound::new("root", &root, "");
        close(drag(&plain, 59.6 * note, None), 59.6, "an unstepped drag");
    }

    /// A stepped grid outranks the keyboard law, and `Alt` goes no finer than the grid's own step.
    #[test]
    fn a_stepped_grid_outranks_the_law_and_alt_keeps_its_step() {
        let root = FloatParam::new(
            "Root",
            60.0,
            FloatRange::Linear {
                min: 0.0,
                max: 127.0,
            },
        );
        let note = 1.0 / 127.0;
        let bound = Bound::new("root", &root, "")
            .stepped(note, 12.0 * note)
            .law(StepLaw::Semitones);
        let from = 60.0 * note;
        let press = |up, coarse, finer| Press { up, coarse, finer };
        for (press, wanted) in [
            (press(true, false, false), 61.0),
            (press(false, true, false), 48.0),
            (press(true, true, true), 61.0),
            (press(true, false, true), 61.0),
        ] {
            let landed = bound.next_value(from, press);
            assert!(
                (landed - wanted * note).abs() < 1e-9,
                "{press:?}: {landed} against {wanted}"
            );
        }
    }

    /// The painted label is the panel's; the parameter's own name is what everything else reads.
    #[test]
    fn a_label_is_painted_and_the_name_stays_the_parameters() {
        let param = FloatParam::new(
            "Complex frequency",
            0.5,
            FloatRange::Linear { min: 0.0, max: 1.0 },
        );
        let bound = Bound::new("complexfreq", &param, "").labelled("Frequency");
        assert_eq!(bound.painted(), "Frequency");
        assert_eq!(bound.param.name(), "Complex frequency");
        assert_eq!(bound.unlabelled().painted(), "Complex frequency");
    }

    /// `set_together` opens every gesture before the first value and skips what would not change.
    #[test]
    fn a_group_write_opens_all_sets_all_then_closes_all_and_skips_the_unchanged() {
        let a = FloatParam::new("A", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 });
        let b = FloatParam::new("B", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 });
        let c = FloatParam::new("C", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 });
        let host = RecordingHost::new(false);
        let setter = ParamSetter::new(&host);
        set_together(&setter, &[(&a, 1.0), (&b, 0.5), (&c, 1.0)]);
        assert_eq!(host.record.load(Ordering::SeqCst), 112233);
    }
}
