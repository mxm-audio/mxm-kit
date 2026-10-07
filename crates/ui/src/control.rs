//! Parameter controls, buttons and segmented controls — design system §7.1 to §7.3.
//!
//! # The shape of this API, and why
//!
//! Every control here takes a **value and a description**, and returns **what happened**. It never
//! reaches for a host, a plugin, or a parameter object. That is not tidiness: it is the reason the
//! same control can be drawn by the player against a CLAP `ParamSnapshot` and by mxm-mono-01's editor
//! against its own `FloatParam`, without this crate knowing either type exists.
//!
//! Values are **normalised to `0..=1`**. Real units, ranges and skew belong to whoever owns the
//! parameter — a host that has to reason about a plugin's skew curve is a host that has to be
//! updated when the plugin changes.
//!
//! # What §7.1 requires of every continuous control
//!
//! All of it is implemented here rather than left to callers, because a requirement that each
//! consumer re-implements is a requirement that each consumer gets subtly differently:
//!
//! - drag, with one idiom per control type: a **knob is relational** (drag up to increase,
//!   wherever the pointer is), a **slider is positional** (the handle goes where you point). Those
//!   are not two inconsistent rules, they are the same rule — a control moves the way its shape
//!   says it moves. A horizontal slider whose handle refuses to follow the pointer is broken, and
//!   a knob that jumped to wherever you clicked on it would be unusable;
//! - fine adjustment with `Shift`;
//! - reset to default on double-click;
//! - direct text entry from a documented gesture;
//! - visible hover, active and keyboard-focus states;
//! - correct host automation begin/change/end gestures — reported, not sent;
//! - a tooltip with the full name, the exact value, and a one-sentence description.
//!
//! **Scroll-wheel editing is off by default** (§7.1). A stray wheel over a panel should scroll the
//! panel, not silently automate a parameter.
//!
//! # A knob is told how wide its column is
//!
//! `column` is the width its **name and value** are laid out in — the knob itself is always its
//! tier's diameter. It is a parameter and not `ui.available_width()`, which is only the column width
//! inside `Ui::columns`: in a plain row it is *everything left*, so a knob sharing a row with
//! anything else took the whole line and pushed its neighbour clean off the card. Asking cost a
//! working layout; being told cannot.
//!
//! # Nothing changes size under the pointer
//!
//! Four defects reported on this project were the interface moving during interaction. So every
//! control here allocates its geometry from [`Size`] **before** it knows whether it is hovered,
//! focused or being dragged, and interaction changes only colour and stroke width. A control that
//! grows when grabbed is the same defect at a smaller scale.

use egui::{
    Align2, Color32, Key, Modifiers, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2,
};

use crate::space::{HAIRLINE, MIN_TARGET, RADIUS, SPACE_1, SPACE_2, SPACE_3, SPACE_4};
use crate::theme::Tokens;
use crate::typography::{label_style, value_style};

/// §7.1's control hierarchy.
///
/// **Every tier is drawn at [`KNOB_DIAMETER`].** The tier no longer sets a size; it sets whether
/// the value is always visible.
///
/// The diameters were 64/48/32, then briefly 52/36/24, and are now one number (owner, 2026-09-07,
/// against Pigments: *"I don't think we should use the smaller knobs any more. Pigments also only
/// have one size."*). Three sizes were never carrying the hierarchy anyway: a knob is a circle
/// with an arc, and at 24 points it is a small circle with a small arc rather than a clearly
/// lesser control. What actually ranks a parameter is where it sits, what it is called, and
/// whether its value is on the face — which is what this enum still decides.
///
/// The enum is kept rather than deleted because [`Size::value_always_visible`] is a real
/// distinction, and because every consumer already names its tier.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Size {
    /// Always shows label and value.
    Primary,
    /// Always shows label and value.
    #[default]
    Standard,
    /// Value may appear on hover or focus when density requires it.
    Compact,
}

/// The one knob diameter, in logical pixels.
///
/// Big enough to aim at and to read an arc on, small enough that a row of them is not mostly
/// knob. Comfortably above [`MIN_TARGET`], which a knob should be: it is dragged, not tapped.
///
/// 36 while the collection rendered in the toolkit's default face; 40 since Inter, for the reason
/// [`MIN_TARGET`] gives — the type grew into the controls, and the controls are what should give.
pub const KNOB_DIAMETER: f32 = 40.0;
/// Every knob's column, in every editor: mxm-mono-08's, the collection's standard (the owner,
/// 2026-09-24: *"as much standardisation as possible"*). The diameter alone is not enough: parameter
/// labels need this shared reading width. Editors had set it five ways, from 44 to 78.
pub const KNOB_COLUMN_MIN: f32 = 76.0;

/// A fader's height in a row of faders — an envelope's stages, a mixer's levels — name, track and
/// reading together ([`slider_vertical`]): a track of about ninety points under a two-line name,
/// long enough that a level reads as a proportion of its travel, short enough that a card of faders
/// stands beside a card of knobs. The owner, 2026-09-25: *"ADSRs are typically sliders"*.
pub const FADER_HEIGHT: f32 = 144.0;

/// The column a knob of `size` stands in: its diameter and a gutter, at least [`KNOB_COLUMN_MIN`].
/// With every tier one diameter that is [`KNOB_COLUMN_MIN`] for every knob; a name or reading wider
/// than it still widens its own column (the tree's knob leaf).
#[must_use]
pub const fn knob_column(size: Size) -> f32 {
    (size.diameter() + crate::space::SPACE_5).max(KNOB_COLUMN_MIN)
}

impl Size {
    /// The control's diameter, in logical pixels. The same for every tier.
    #[must_use]
    pub const fn diameter(self) -> f32 {
        // One size. See the type's own documentation for why the tiers no longer differ here.
        let _ = self;
        KNOB_DIAMETER
    }

    /// Whether the formatted value is always visible, or only on hover and focus.
    ///
    /// §7.1 allows Compact to defer its value; Primary and Standard must always show it.
    #[must_use]
    pub const fn value_always_visible(self) -> bool {
        !matches!(self, Self::Compact)
    }
}

/// Everything a control needs to know about the parameter it edits.
///
/// The caller owns the parameter; this is a borrowed view of it for one frame.
#[derive(Clone, Copy, Debug)]
pub struct ParamView<'a> {
    /// The full name, in sentence case. The same in every view and in host automation (§7.1).
    pub name: &'a str,
    /// Visible label when the containing card already supplies the module prefix.
    /// Full names remain in accessibility and tooltips; this must not rename the parameter.
    pub label: &'a str,
    /// The formatted value, units included: `440 Hz`, `-12.0 dB`, `35%` (§6).
    pub text: &'a str,
    /// The widest value this parameter can show, for a layout that must not move as the value
    /// changes: [`slider_inline`] reserves its width. Empty measures only `text`. See
    /// [`widest_value`].
    pub widest: &'a str,
    /// One sentence on what it does. §7.1 requires it in the tooltip, and CLAP carries no such
    /// field — which is exactly why the editor belongs to the plugin that knows the answer.
    pub description: &'a str,
    /// The default, normalised. Double-click returns here.
    pub default: f64,
    /// Bipolar parameters get a centre-anchored arc and a detent at the middle.
    pub bipolar: bool,
    /// Read-only parameters draw at reduced emphasis and do not accept input. They stay legible:
    /// §3.3 forbids hiding a disabled control or reducing it to an unexplained icon.
    pub read_only: bool,
    /// How far something else is currently moving this parameter, normalised. Zero when nothing is.
    ///
    /// Drawn as an **arc from the knob's position to where the modulation takes it**, so the two
    /// facts a modulated control has to convey are separate shapes: the marker is what the knob is
    /// set to, and this arc is what is being played. Without it, a knob that stays put while the
    /// sound moves — which is the *right* behaviour — reads as the control refusing input.
    pub modulation: f64,
    /// **Something other than this control is also setting this parameter.**
    ///
    /// Drawn as a dot beside the name — a mark *about* the control rather than a change to how the
    /// control reads, which is why it is a separate shape in the margin and not a recolouring of the
    /// track (§7.2: state through more than hue). The same dot the player's step buttons use for the
    /// same fact, so the two ends of "this step sets the cutoff" look like each other.
    ///
    /// Deliberately not named after the sequencer: a control can be marked for any reason its owner
    /// has, and this crate has no business knowing which.
    pub marked: bool,
    /// How far one keyboard press moves this parameter. See [`Steps`].
    pub steps: Steps,
    /// The owner's step law, when it has one: where one press lands from any value. See
    /// [`NextValue`]. Without it, [`ParamView::steps`] applies.
    pub next: Option<Next<'a>>,
    /// Paints the name in the caption style and secondary ink — for a row under a title that
    /// already says what it moves, where the name is the lesser word (a route's source under its
    /// target's line; the owner, 2026-09-24).
    pub quiet_label: bool,
}

/// One keyboard press on a value: which way, whether it is the fine or the coarse axis, and
/// whether `Alt` asked for the finer layer.
///
/// Left/right is fine and up/down coarse under the cursor (see [`crate::navigation`]); the owner
/// of the parameter decides what either means in its own units. **`Alt` is a finer layer, and in
/// each layer up/down is the larger step** (the owner, 2026-09-24): 10 % and 1 % of the travel
/// without it, 1 % and 0.1 % with it — an octave and a semitone, or ten cents and a cent, on a
/// pitch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press {
    pub up: bool,
    pub coarse: bool,
    pub finer: bool,
}

/// Where one keyboard press lands, answered by the parameter's owner from **any** value.
///
/// [`Steps`] answers only "how far from here", which is enough for a law that moves the same
/// distance everywhere. A musical law does not: a semitone on a skewed hertz range, or a step onto
/// the whole semitone a readout shows, depends on the value the press starts from. When a frame
/// carries several presses, each one starts where the one before it landed, so the owner has to
/// be asked once per press — which a fixed magnitude cannot do.
///
/// Plain Rust, so this crate still knows nothing about the parameter behind it (`crates/ui`'s
/// contract is egui and nothing else). The plugin's binding implements it over its parameter.
pub trait NextValue {
    /// The normalised value one `press` lands on from `normalised`, already within `0..=1`.
    fn next_value(&self, normalised: f64, press: Press) -> f64;
}

/// A borrowed [`NextValue`], so a [`ParamView`] stays `Copy` and printable.
#[derive(Clone, Copy)]
pub struct Next<'a>(pub &'a dyn NextValue);

impl std::fmt::Debug for Next<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Next(..)")
    }
}

/// How far one keyboard press moves a parameter, normalised, in each of the four directions.
///
/// **Supplied by the parameter's owner, because only it knows.** A parameter declared with a step
/// size, an enum and a plain continuous float all step differently, and a skewed range steps by a
/// different amount at each end — so a single number computed here would be wrong for most of the
/// collection. The plugin computes these from its own parameter each frame, at its current value;
/// this crate only adds them.
///
/// The default is the flat fall-back for a control whose owner has not supplied any, so a control
/// still operates from the keyboard before its editor is converted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steps {
    pub fine_up: f64,
    pub fine_down: f64,
    pub coarse_up: f64,
    pub coarse_down: f64,
}

impl Steps {
    /// One percent fine, ten coarse — the ratio the plan chose, for a parameter that has not
    /// told us its own.
    pub const DEFAULT: Self = Self {
        fine_up: 0.01,
        fine_down: 0.01,
        coarse_up: 0.10,
        coarse_down: 0.10,
    };
}

impl Default for Steps {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<'a> ParamView<'a> {
    /// The common case: a unipolar, writable parameter that defaults to zero.
    #[must_use]
    pub const fn new(name: &'a str, text: &'a str, description: &'a str) -> Self {
        Self {
            name,
            label: name,
            text,
            widest: "",
            description,
            default: 0.0,
            bipolar: false,
            read_only: false,
            marked: false,
            modulation: 0.0,
            steps: Steps::DEFAULT,
            next: None,
            quiet_label: false,
        }
    }

    /// Sets how far one keyboard press moves this parameter. See [`Steps`].
    #[must_use]
    pub const fn stepping(mut self, steps: Steps) -> Self {
        self.steps = steps;
        self
    }

    /// Hands the keyboard the owner's step law, asked once per press. See [`NextValue`].
    #[must_use]
    pub const fn stepping_by(mut self, law: &'a dyn NextValue) -> Self {
        self.next = Some(Next(law));
        self
    }

    /// Reserves room for the widest value the parameter can show. See [`ParamView::widest`].
    #[must_use]
    pub const fn widest(mut self, text: &'a str) -> Self {
        self.widest = text;
        self
    }

    /// Omits a redundant module prefix from the visible label, not from the parameter identity.
    #[must_use]
    pub const fn labelled(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Sets how far something else is moving the parameter. See [`ParamView::modulation`].
    #[must_use]
    pub const fn modulated_by(mut self, offset: f64) -> Self {
        self.modulation = offset;
        self
    }

    /// Marks the parameter: something other than this control is also setting it. See
    /// [`ParamView::marked`].
    #[must_use]
    pub const fn marked(mut self, marked: bool) -> Self {
        self.marked = marked;
        self
    }

    /// Sets the default a double-click returns to.
    #[must_use]
    pub const fn default_at(mut self, normalised: f64) -> Self {
        self.default = normalised;
        self
    }

    /// Marks the parameter bipolar: centre-anchored arc, detent in the middle.
    #[must_use]
    pub const fn bipolar(mut self) -> Self {
        self.bipolar = true;
        self
    }

    /// Marks the parameter read-only.
    #[must_use]
    pub const fn read_only(mut self, yes: bool) -> Self {
        self.read_only = yes;
        self
    }
}

/// What a control did this frame.
///
/// The control **reports**; it does not send. Gesture bracketing is the caller's, because only the
/// caller knows whether it is talking to a CLAP host, a standalone wrapper, or a test.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ControlOutcome {
    /// A drag or edit started: open an automation gesture.
    pub gesture_started: bool,
    /// The value changed and must be sent.
    pub changed: bool,
    /// The drag or edit finished: close the gesture.
    pub gesture_ended: bool,
    /// The value was **returned to its default**, rather than moved to a new one.
    ///
    /// A caller that only wants the number cannot tell those apart, and one of them does. The
    /// player's sequencer reads it: with a step selected, moving a knob records what that step sets,
    /// and resetting it means *this step sets nothing* — which is a different act, not the same act
    /// with a particular value. Guessing from the value would make setting a knob to exactly its
    /// default indistinguishable from clearing, and those are different: a step that pins a
    /// parameter to the patch value while other steps move it is doing something.
    pub reset: bool,
}

impl ControlOutcome {
    #[must_use]
    pub const fn any(&self) -> bool {
        self.gesture_started || self.changed || self.gesture_ended
    }

    /// Merges another outcome into this one. Two input paths can fire in the same frame — a
    /// focused control dragged while an arrow key repeats — and dropping either half is how a
    /// gesture ends up unbalanced.
    pub fn merge(&mut self, other: Self) {
        self.gesture_started |= other.gesture_started;
        self.changed |= other.changed;
        self.gesture_ended |= other.gesture_ended;
        self.reset |= other.reset;
    }

    /// A complete edit in one frame — a double-click reset, or a committed text entry.
    ///
    /// Named because getting the bracketing wrong on an instantaneous edit is a classic way to
    /// leave a host's automation lane open forever.
    #[must_use]
    pub const fn instant() -> Self {
        Self {
            gesture_started: true,
            changed: true,
            gesture_ended: true,
            reset: false,
        }
    }

    /// A complete edit that returned the control to its default. See [`ControlOutcome::reset`].
    #[must_use]
    pub const fn reset_to_default() -> Self {
        Self {
            reset: true,
            ..Self::instant()
        }
    }
}

/// Whether the scroll wheel edits a control, and under what condition.
///
/// §7.1 is in two parts and the second is the one that gets dropped:
///
/// > Do not rely on scroll-wheel editing by default; accidental changes while scrolling a view are
/// > too easy. **If enabled, it must require focus or a modifier.**
///
/// So an application-level "enable scroll-wheel editing" setting resolves to
/// [`Wheel::FocusOrModifier`], never to plain hover. Hover-only wheel editing is how a user
/// scrolling a long parameter list silently automates whatever the pointer passed over.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Wheel {
    /// The wheel scrolls the view and never edits. The collection default.
    #[default]
    Off,
    /// The wheel edits, but only while the control has keyboard focus or `Shift` is held.
    FocusOrModifier,
}

impl std::ops::BitOrAssign for ControlOutcome {
    fn bitor_assign(&mut self, other: Self) {
        self.merge(other);
    }
}

/// How far a full-scale drag travels, in logical pixels.
///
/// One constant for the collection: §7.1 requires a consistent direction, and a consistent
/// *distance* is what makes muscle memory transfer between two MXM instruments.
const DRAG_TRAVEL: f32 = 200.0;

/// Where a drag started, and how far it has travelled since.
///
/// **A drag accumulates against this rather than against the value on screen**, and the difference
/// is the whole reason the type exists.
///
/// A plugin parameter is not a variable the editor owns. The editor asks the host to set it, and
/// under CLAP the value lands when the plugin next processes audio — which is a different clock
/// from the one the editor repaints on. Add each frame's delta to *the value read back this
/// frame*, and every editor frame that runs before the audio thread has caught up adds its delta
/// to a value that has not moved yet: the knob lurches forward, snaps back, and lurches again.
/// Reported against `mxm-chorus-06` in MXM Player as *the knobs jitter instead of turning*
/// (2026-09-04), and it is a property of every host that applies parameters asynchronously, which
/// is all of them.
///
/// Anchored, the readback cannot feed back: the position under the pointer is the value the drag
/// began at plus the distance the pointer has moved, whatever the parameter currently reads.
///
/// Kept in egui's own per-widget memory, so it is per control, survives a reflow, and is dropped
/// when the drag ends. Nothing outside a drag reads it.
#[derive(Clone, Copy, Debug, Default)]
struct DragAnchor {
    start: f64,
    travelled: f64,
}

/// Where one control's drag anchor lives.
fn anchor_id(response: &Response) -> egui::Id {
    response.id.with("mxm-drag-anchor")
}

/// `Shift` divides the travel by this. Enough to place a value exactly, not so much that a small
/// correction needs a long stroke.
const FINE_FACTOR: f64 = 8.0;

/// The value arc spans 270°, leaving a 90° gap at the bottom (§7.1).
const ARC_SWEEP: f32 = std::f32::consts::TAU * 0.75;
/// Straight down, where the gap is centred.
const ARC_START: f32 = std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * 0.125;

/// A knob: an abstract circular control with a neutral track, a value arc and a radial marker.
///
/// §7.1 is explicit that this is **not** a hardware knob — no cap texture, no lighting, no
/// perspective, no fake pointer shadow. What it has is a track, an arc and a line.
///
/// `normalised` is edited in place. Returns what happened, for the caller to bracket.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    ui: &mut Ui,
    tokens: &Tokens,
    param: &ParamView<'_>,
    normalised: &mut f64,
    size: Size,
    column: f32,
    text_entry: &mut Option<String>,
    wheel: Wheel,
) -> ControlOutcome {
    let mut outcome = ControlOutcome::default();

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SPACE_2;

        // The label comes first so `labelled_by` can tie the control to it. A control with no
        // accessible name is announced as just "slider".
        //
        // **A fixed box, centred over the knob.** Left-aligned text under a centred circle reads as
        // a mistake, and — worse — a name long enough to wrap used to push its own knob down, so a
        // row of knobs sat at three different heights depending on how long their names happened to
        // be. The box is [`NAME_LINES`] tall whatever the name does, so every knob in a row starts
        // at the same y by construction rather than by choosing short names.
        let mut column = column.max(size.diameter().max(MIN_TARGET));
        // A view that names its widest reading holds it whole on the value line, as `knob_size`
        // counts it; a caller drawing a tree's leaf already passes a column that wide.
        if size.value_always_visible() && !param.widest.is_empty() {
            column = column.max(text_width(ui, param.widest, value_style(ui.style())));
        }

        let label = fixed_label(
            ui,
            param.label,
            column,
            NAME_LINES,
            egui::TextStyle::Body,
            None,
        );

        if let Some(buffer) = text_entry.as_mut() {
            outcome = text_entry_field(ui, buffer, size.diameter());
            return;
        }

        // Geometry is allocated before any interaction state is read, so nothing here can depend
        // on hover or focus. That is the no-resize rule, enforced by construction.
        let diameter = size.diameter();
        let sense = if param.read_only {
            Sense::hover()
        } else {
            Sense::click_and_drag()
        };

        // **Centred in a band the column's width**, not allocated at its own size and left where it
        // lands. The name and value boxes are the full column, so the block around them is too, and
        // a knob allocated plainly sat against its left edge while its label sat over the middle —
        // which is the same misalignment as the shapes-beside-a-knob one, on the other axis.
        let (band, _) =
            ui.allocate_exact_size(Vec2::new(column, diameter.max(MIN_TARGET)), Sense::hover());
        let (rect, response) = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(band)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
                |ui| ui.allocate_exact_size(Vec2::splat(diameter.max(MIN_TARGET)), sense),
            )
            .inner;

        outcome = drag_edit(ui, &response, param, normalised, wheel);

        paint_knob(ui, tokens, rect, diameter, *normalised, param, &response);

        // **Painted over the band, never inserted beside the name.** A knob's column is a fixed
        // grid, name and value included; putting the dot in the label flow would shift the name and
        // make a marked knob a different shape from an unmarked one. Painting costs no layout, which
        // is the same reason the player draws this mark on a step button rather than laying it out.
        if param.marked {
            ui.painter().circle_filled(
                egui::pos2(band.right() - SPACE_2, band.top() + SPACE_2),
                HAIRLINE * 2.0,
                tokens.text_primary,
            );
        }

        response
            .widget_info(|| egui::WidgetInfo::slider(!param.read_only, *normalised, param.name));
        let response = if param.label == param.name {
            response.labelled_by(label.id)
        } else {
            response
        };
        tooltip(response, param);

        if size.value_always_visible() {
            value_label(ui, tokens, param, column);
        }
    });

    outcome
}

/// A horizontal slider, for the cases §7.1 prefers one: where range and comparison matter more
/// than compactness. The mixer's four levels are the canonical example — four knobs are four
/// separate readings, four sliders are one comparison.
pub fn slider(
    ui: &mut Ui,
    tokens: &Tokens,
    param: &ParamView<'_>,
    normalised: &mut f64,
    width: f32,
    text_entry: &mut Option<String>,
    wheel: Wheel,
) -> ControlOutcome {
    slider_impl(
        ui, tokens, param, normalised, width, text_entry, wheel, None,
    )
}

/// A vertical positional fader whose complete label/track/value column has a caller-selected height.
///
/// This is the same parameter control as [`slider`] turned through ninety degrees: bottom is zero,
/// top is one, `Shift` makes a drag relative and fine, and keyboard/reset/wheel behavior is shared.
/// It exists for signal-flow rails where the control is intentionally compared against a meter or
/// visualization of the same height; callers still own host gesture delivery and parameter skew.
#[allow(clippy::too_many_arguments)]
pub fn slider_vertical(
    ui: &mut Ui,
    tokens: &Tokens,
    param: &ParamView<'_>,
    normalised: &mut f64,
    width: f32,
    height: f32,
    text_entry: &mut Option<String>,
    wheel: Wheel,
) -> ControlOutcome {
    let mut outcome = ControlOutcome::default();
    let width = width.max(MIN_TARGET);
    let label_height = ui.text_style_height(&egui::TextStyle::Body) * NAME_LINES as f32;
    let value_height = ui.text_style_height(&value_style(ui.style()));
    let track_height = (height - label_height - value_height - SPACE_2 * 2.0).max(MIN_TARGET);

    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.y = SPACE_2;
            let label = fixed_label(
                ui,
                param.label,
                width,
                NAME_LINES,
                egui::TextStyle::Body,
                None,
            );

            if let Some(buffer) = text_entry.as_mut() {
                outcome = text_entry_field(ui, buffer, width);
                return;
            }

            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(width, track_height),
                if param.read_only {
                    Sense::hover()
                } else {
                    Sense::click_and_drag()
                },
            );
            outcome = vertical_slider_edit(ui, &response, rect, param, normalised, wheel);
            paint_vertical_slider(ui, tokens, rect, *normalised, param, &response);
            response.widget_info(|| {
                egui::WidgetInfo::slider(!param.read_only, *normalised, param.name)
            });
            let response = if param.label == param.name {
                response.labelled_by(label.id)
            } else {
                response
            };
            tooltip(response, param);
            value_label(ui, tokens, param, width);
        },
    );

    outcome
}

/// [`slider`], laid out on **one line** — name, track, value — for the app bar.
///
/// The stacked form is right in a card, where a name line above a track is the §7.1 shape and the
/// column has height to spend. It is wrong in the app bar, which is one row: a two-line control
/// there is taller than everything beside it, so its name rides above the row's centre and its
/// value above that, and the bar reads as a set of mismatched pieces rather than a line. Design
/// system §3.1 slot 6 puts the master output on that bar, so the bar needs a control shaped like
/// it.
///
/// **Everything about the interaction is the stacked one's**, down to the same `slider_edit`,
/// `paint_slider` and text entry — only the axis of the layout differs. `width` is the track alone
/// here rather than the whole control, because the name and value sit beside it instead of over it.
pub fn slider_inline(
    ui: &mut Ui,
    tokens: &Tokens,
    param: &ParamView<'_>,
    normalised: &mut f64,
    width: f32,
    text_entry: &mut Option<String>,
    wheel: Wheel,
) -> ControlOutcome {
    let mut outcome = ControlOutcome::default();

    // **The direction is stated, and the region is bounded — both, or neither works.**
    //
    // This control's whole purpose is the app bar, whose right-hand group is laid out right to left
    // so it can keep its edge whatever the preset name turns out to be. Inside that, a row that
    // *inherits* the direction comes out backwards — `80% ▭ Master` — and a row that only declares
    // its own direction takes the entire remaining width with it, which squeezed the wordmark and
    // the preset controls into each other and pushed this control off the bar altogether. A column
    // never had to care about either, which is why the stacked form has neither of these lines.
    //
    // So the size is measured first and the region allocated to it. The two texts are measured in
    // the same styles they are drawn in, so the box is the width of what is actually in it.
    //
    // **The value's room is its widest text, not its current one.** The bar's right group lays out
    // right to left, so a region that follows the current value (`0.0 dB`, then `-12.3 dB`) moves
    // the track under a dragging pointer by the difference, which reads as jitter.
    let label_width = text_width(ui, param.label, egui::TextStyle::Body);
    let value_width = text_width(ui, param.text, value_style(ui.style())).max(text_width(
        ui,
        param.widest,
        value_style(ui.style()),
    ));
    let gaps = ui.spacing().item_spacing.x * 3.0;
    let total = SPACE_2 + label_width + width + value_width + gaps;
    ui.allocate_ui_with_layout(
        Vec2::new(total, MIN_TARGET),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            // The row's height before anything is placed in it, for the reason the stacked form gives:
            // egui centres each item against the row as it stands, so a label added before a taller
            // track is never moved down again and sits a few points high.
            ui.set_min_height(MIN_TARGET);
            mark_slot(ui, tokens, param.marked);
            let label = ui.label(param.label);

            if let Some(buffer) = text_entry.as_mut() {
                outcome = text_entry_field(ui, buffer, width);
                return;
            }

            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(width, MIN_TARGET),
                if param.read_only {
                    Sense::hover()
                } else {
                    Sense::click_and_drag()
                },
            );

            outcome = slider_edit(ui, &response, rect, param, normalised, wheel);

            paint_slider(ui, tokens, rect, *normalised, param, &response);
            response.widget_info(|| {
                egui::WidgetInfo::slider(!param.read_only, *normalised, param.name)
            });
            let response = if param.label == param.name {
                response.labelled_by(label.id)
            } else {
                response
            };
            tooltip(response, param);

            value_text(ui, tokens, param);
        },
    );

    outcome
}

/// The widest of a parameter's value texts, sampled across its range, for [`ParamView::widest`].
///
/// Values are drawn in the monospaced value style, so the longest text is the widest. Sampled at
/// thirty-two steps plus both ends: the extremes carry the most digits and signs on the scales the
/// collection uses, and the interior samples catch a mid-range sign or unit change.
#[must_use]
pub fn widest_value(format: impl Fn(f64) -> String) -> String {
    (0..=32)
        .map(|step| format(f64::from(step) / 32.0))
        .max_by_key(|text| text.chars().count())
        .unwrap_or_default()
}

/// The width one string occupies in one text style, for a control that must size its own region.
///
/// **Measured, not budgeted.** A constant would be a guess that a longer name or a wider value
/// silently outgrows, and this is the app bar, where outgrowing the budget pushes a neighbour off
/// the edge rather than wrapping.
fn text_width(ui: &Ui, text: &str, style: egui::TextStyle) -> f32 {
    let font = style.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER)
        .size()
        .x
}

/// [`slider`], with its modulation source drawn **on the name line** rather than beneath the track.
///
/// A routed amount is named *"Clock period from Key"*, and drawing the source under the track
/// spent a whole row restating a word already in the label. The label becomes *"Clock period
/// from"* and the source follows it inline as a caret selector, so the row reads as one sentence
/// and the group loses a line. The caller supplies the shortened label through
/// [`ParamView::label`]; `name` stays whole for accessibility and host automation, which is the
/// same bargain §7.1 already strikes for a repeated module prefix.
///
/// `source` is called inside the name row, immediately after the label and before the value.
#[allow(clippy::too_many_arguments)]
pub fn slider_with_source(
    ui: &mut Ui,
    tokens: &Tokens,
    param: &ParamView<'_>,
    normalised: &mut f64,
    width: f32,
    text_entry: &mut Option<String>,
    wheel: Wheel,
    source: &mut dyn FnMut(&mut Ui),
) -> ControlOutcome {
    slider_impl(
        ui,
        tokens,
        param,
        normalised,
        width,
        text_entry,
        wheel,
        Some(source),
    )
}

#[allow(clippy::too_many_arguments)]
fn slider_impl(
    ui: &mut Ui,
    tokens: &Tokens,
    param: &ParamView<'_>,
    normalised: &mut f64,
    width: f32,
    text_entry: &mut Option<String>,
    wheel: Wheel,
    mut source: Option<&mut dyn FnMut(&mut Ui)>,
) -> ControlOutcome {
    let mut outcome = ControlOutcome::default();

    ui.vertical(|ui| {
        // `SPACE_1`, as an optical adjustment rather than layout: the name/value row is taller
        // than the name in it, because the tabular value sets the row's height, so a `SPACE_2`
        // rhythm measured seven points between the name and the track it names. A label is the
        // most closely related thing there is to its own control (§4.1).
        ui.spacing_mut().item_spacing.y = SPACE_1;

        // The name and the formatted value share a line: §7.1's three parts, with the control
        // below them. The label's id is kept so the track can be `labelled_by` it — without that
        // a screen reader announces the control as bare "slider", and a UI test can only find it
        // by position.
        let label = ui
            .horizontal(|ui| {
                // The row is the selector's height *before* anything is placed in it. egui
                // positions each item as it is added and centres it against the row as it stands,
                // so a label added before a taller selector is never moved down again: the label
                // sat three points above the source beside it, which reads as the source jumping.
                if source.is_some() {
                    ui.set_min_height(MIN_TARGET);
                }
                mark_slot(ui, tokens, param.marked);
                let label = if param.quiet_label {
                    ui.label(
                        egui::RichText::new(param.label)
                            .text_style(crate::typography::caption_style(ui.style()))
                            .color(tokens.text_secondary),
                    )
                } else {
                    ui.label(param.label)
                };
                if let Some(source) = source.as_mut() {
                    source(ui);
                }
                // **The value never draws over the name.** Right-aligned into what is left, it
                // was pinned to the column's edge whatever was left, so a name and value wider
                // than the column were painted on top of each other (the owner's finding on
                // mxm-mono-08, 2026-09-24) — inside the card, where no width check sees it. When
                // they do not fit, the value follows the name instead and runs past the edge,
                // which is visible, measurable, and says the card should be wider. The room is
                // the widest value text, so a value changing length cannot flip the choice
                // mid-drag. Piloted on mxm-mono-08, and every editor's since 2026-09-24.
                let value_style = value_style(ui.style());
                let room = text_width(ui, param.text, value_style.clone()).max(text_width(
                    ui,
                    param.widest,
                    value_style,
                ));
                if ui.available_width() >= room + ui.spacing().item_spacing.x {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        value_text(ui, tokens, param);
                    });
                } else {
                    value_text(ui, tokens, param);
                }
                label
            })
            .inner;

        if let Some(buffer) = text_entry.as_mut() {
            outcome = text_entry_field(ui, buffer, width);
            return;
        }

        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, MIN_TARGET),
            if param.read_only {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            },
        );

        outcome = slider_edit(ui, &response, rect, param, normalised, wheel);

        paint_slider(ui, tokens, rect, *normalised, param, &response);
        response
            .widget_info(|| egui::WidgetInfo::slider(!param.read_only, *normalised, param.name));
        let response = if param.label == param.name {
            response.labelled_by(label.id)
        } else {
            response
        };
        tooltip(response, param);
    });

    outcome
}

/// The dot that says something other than this control is also setting the parameter.
///
/// **The colour of the marks around it**, not a colour of its own: beside a label it is the label's
/// colour, and where the player draws the same dot on a step button it is that button's mark colour,
/// which inverts under the playhead. One rule, and the dot is legible wherever it is put.
/// **The slot is always allocated; only the dot is conditional.** Modulation arrives and leaves
/// while the pointer is on the control - a drag is itself a deviation - so a dot that took its own
/// space shifted the whole name line, and everything after it, mid-gesture. That is the defect
/// this crate already fixed for the knob's marked corner and for mxm-mono-08's live route bar.
fn mark_slot(ui: &mut Ui, tokens: &Tokens, marked: bool) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SPACE_2), Sense::hover());
    if !marked {
        return;
    }
    ui.painter()
        .circle_filled(rect.center(), HAIRLINE * 2.0, tokens.text_primary);
    response.on_hover_text("A selected step sets this parameter");
}

/// How many lines a knob's name is given, whatever it needs.
///
/// Two, because *Filter envelope* and *Key tracking* wrap at a knob's width and *Cutoff* does not —
/// and a row whose knobs sit at different heights because of that is the defect this fixes. A name
/// needing three lines overflows its box rather than moving the layout, which is the right way
/// round: the row stays put and the name is visibly too long.
const NAME_LINES: usize = 2;

/// The height of a knob's name box: [`NAME_LINES`] lines, whatever the name does.
///
/// A control stacked beside a knob rather than under it drops by this much to clear the name — so
/// that its *box* starts where the name ends, which is what reads as aligned when the control is
/// taller than a line of text. Matching the text baselines instead leaves a button's frame
/// standing proud of the label it sits beside (owner, 2026-09-22).
///
/// Exposed because [`NAME_LINES`] is private and a copy in a plugin would rot silently.
/// `selector_impl` does the same sum inline for its own `beside_knob` case.
#[must_use]
pub fn name_box_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) * NAME_LINES as f32
}

/// A segmented control, §7.3: 2–5 mutually exclusive options.
///
/// More than five belongs in a menu. Segments **must not imitate mechanical switches** (§7.3), so
/// this is a row of flat cells with one selected — no bevel, no throw, no toggle travel.
///
/// Returns `true` when the selection changed. A selection is instantaneous, so callers bracket it
/// with [`ControlOutcome::instant`].
#[allow(clippy::too_many_arguments)] // One view struct would hide that every caller sets all of these.
pub fn segmented(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    details: &[&str],
) -> bool {
    segmented_impl(
        ui,
        tokens,
        label,
        label,
        options,
        selected,
        sounding,
        default_cell,
        None,
        None,
        details,
    )
}

/// [`segmented`], with every cell at least `cell` wide — for **controls that stand together**, so a
/// stack of selectors reads as one set instead of a ragged column. `cell` comes from
/// [`shared_cell_width`] over every control in the stack; a cell is never narrower than its own
/// control's floor whatever it is handed.
#[allow(clippy::too_many_arguments)]
pub fn segmented_with_cell(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    cell: f32,
    details: &[&str],
) -> bool {
    segmented_impl(
        ui,
        tokens,
        label,
        label,
        options,
        selected,
        sounding,
        default_cell,
        None,
        Some(cell),
        details,
    )
}

/// [`segmented`], for a control that shares a top-aligned row with a knob.
///
/// `beside` is that knob, and the control is laid out on its grid: label on the knob's name line,
/// cells centred on the circle. [`beside_a_knob`] has the geometry, and the measurement of what a
/// control laid out plainly beside a knob looks like.
///
/// **A second entry point rather than a changed one**, as with `AppBar::show_with`: most segmented
/// controls stand on a row of their own, and making every one of them pass `None` to gain an
/// alignment it does not use is how a shared API acquires ceremony. Both funnel into one
/// implementation, so the control cannot grow two layouts.
/// [`segmented_with_cell`], **painting a shorter label than it announces**: `painted` is the line
/// above the cells, which may drop what the card's title already says (design system §7.1 — *Mode*
/// on a card titled *LPG 1*), and `name` is the parameter's own, which every cell's accessible name
/// and tooltip carry.
#[allow(clippy::too_many_arguments)]
pub fn segmented_named(
    ui: &mut Ui,
    tokens: &Tokens,
    painted: &str,
    name: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    cell: f32,
    details: &[&str],
) -> bool {
    segmented_impl(
        ui,
        tokens,
        painted,
        name,
        options,
        selected,
        sounding,
        default_cell,
        None,
        Some(cell),
        details,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn segmented_beside(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    beside: Size,
    details: &[&str],
) -> bool {
    segmented_impl(
        ui,
        tokens,
        label,
        label,
        options,
        selected,
        sounding,
        default_cell,
        Some(beside),
        None,
        details,
    )
}

#[allow(clippy::too_many_arguments)]
fn segmented_impl(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    name: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    beside: Option<Size>,
    cell: Option<f32>,
    details: &[&str],
) -> bool {
    // Six for a range switch, §7.3's one exception (the owner, 2026-09-27: every range is buttons,
    // never a drop-down) — mxm-mono-00's 64' to 2'.
    debug_assert!(
        (2..=6).contains(&options.len()),
        "§7.3: a segmented control is for 2-5 options, or six for a range; {} needs a menu",
        options.len()
    );
    debug_details(details, options.len());

    let mut changed = false;
    let mut focused = false;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SPACE_2;
        beside_a_knob(ui, label, beside, MIN_TARGET);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = HAIRLINE;

            // **One width for every cell**, measured once: the widest option's floor, or the
            // shared width of the stack this control stands in. A segmented control's cells are
            // equal by definition; that is most of what makes it read as one control.
            let width = segment_width(ui, options, cell);

            for (index, option) in options.iter().enumerate() {
                let is_selected = index == *selected;
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(width, MIN_TARGET), Sense::click());

                let ink = segment_cell(ui, tokens, rect, &response, is_selected);
                focused |= response.has_focus();
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    *option,
                    egui::TextStyle::Button.resolve(ui.style()),
                    ink,
                );

                changed |= segment_edit(&response, index, selected, sounding, default_cell);

                segment_mark(
                    ui,
                    tokens,
                    rect,
                    index,
                    options.len(),
                    sounding,
                    is_selected,
                );

                segment_semantics(response, name, option, details[index], is_selected);
            }
        });
    });

    if owns_value_keys(ui, focused) {
        changed |= segmented_keyboard(ui, selected, options.len(), default_cell);
    }
    changed
}

/// Keyboard operation for a segmented parameter. All segmented controls have at most five cells,
/// so the parameter's fine and coarse axes both mean the adjacent legal value.
fn segmented_keyboard(
    ui: &Ui,
    selected: &mut usize,
    count: usize,
    default_cell: Option<usize>,
) -> bool {
    let mut target = None;
    let cursor = crate::navigation::running(ui);
    // The keyboard language, where the pilot runs it: VALUE's presses step one cell each, and
    // DELETE goes back to the default cell.
    if cursor && crate::pilot::on(ui) {
        let keys = crate::navigation::take_value_keys(ui.ctx());
        for press in &keys.presses {
            let from = target.unwrap_or(*selected);
            target = Some(if from >= count {
                if press.up { 0 } else { count - 1 }
            } else if press.up {
                (from + 1).min(count - 1)
            } else {
                from.saturating_sub(1)
            });
        }
        if keys.reset {
            target = default_cell;
        }
        ui.input_mut(|input| {
            if input.consume_key(Modifiers::NONE, Key::Home) {
                target = Some(0);
            }
            if input.consume_key(Modifiers::NONE, Key::End) {
                target = Some(count - 1);
            }
        });
        return match target {
            Some(target) if target != *selected => {
                *selected = target;
                true
            }
            _ => false,
        };
    }
    let arrow_modifiers = if cursor {
        Modifiers::NONE
    } else {
        Modifiers::COMMAND
    };
    ui.input_mut(|input| {
        // In the order they were pressed: Right-then-Left at the last cell ends one cell back,
        // Left-then-Right ends on it.
        // `Alt` too: an option list has nothing finer than the adjacent option, so the finer
        // layer moves one, as a bare arrow does.
        let admits = |pressed: Modifiers| {
            if cursor {
                pressed == Modifiers::NONE || pressed == Modifiers::ALT
            } else {
                pressed.matches_logically(arrow_modifiers)
            }
        };
        for (key, _) in take_arrows(input, admits) {
            let from = target.unwrap_or(*selected);
            let forward = matches!(key, Key::ArrowRight | Key::ArrowUp);
            target = Some(if from >= count {
                // **Nothing selected** — a row of buttons that set another control, standing
                // between its options (mxm-chorus-06's I / II / I + II beside a free Rate). An
                // arrow enters the row at the end it moves away from.
                if forward { 0 } else { count - 1 }
            } else if forward {
                (from + 1).min(count - 1)
            } else {
                from.saturating_sub(1)
            });
        }
        if input.consume_key(Modifiers::COMMAND, Key::Backspace) {
            target = default_cell;
        }
        if input.consume_key(Modifiers::NONE, Key::Home) {
            target = Some(0);
        }
        if input.consume_key(Modifiers::NONE, Key::End) {
            target = Some(count - 1);
        }
    });
    match target {
        Some(target) if target != *selected => {
            *selected = target;
            true
        }
        _ => false,
    }
}

/// Whether a control answers the value keys this frame.
///
/// **Under a running cursor, the cursor's target is the only way in.** The cursor hands egui focus
/// to the control it lands on, but a control can keep that focus after the cursor is suspended —
/// by an open menu, a text field or the preset browser — and a knob answering an arrow meant for
/// the menu edits a parameter behind it. Where no cursor runs, focus is the only way a control is
/// reached at all, as it always was.
fn owns_value_keys(ui: &Ui, focused: bool) -> bool {
    if crate::navigation::running(ui) {
        crate::navigation::keyboard_target(ui)
    } else {
        focused
    }
}

/// Takes this frame's arrow presses that `accept` admits, **in the order they were pressed**,
/// out of the queue.
///
/// `consume_key` cannot do this: it removes every matching press of one key at once and returns a
/// count, so a frame's presses come back grouped by key in whatever order the caller asks for
/// them. That was harmless while every press moved a fixed distance and nothing clamped, and
/// stops being so the moment a step depends on where it starts — Down-then-Up at the top of a
/// range must end at the top. [`crate::navigation`] walks its queue for the same reason.
///
/// The modifiers come back with each press, for a caller whose law depends on them.
fn take_arrows(
    input: &mut egui::InputState,
    accept: impl Fn(Modifiers) -> bool,
) -> Vec<(Key, Modifiers)> {
    let mut arrows = Vec::new();
    input.events.retain(|event| match event {
        egui::Event::Key {
            key: key @ (Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown),
            pressed: true,
            modifiers,
            ..
        } if accept(*modifiers) => {
            arrows.push((*key, *modifiers));
            false
        }
        _ => true,
    });
    arrows
}

/// What one segment's clicks mean. Returns whether the caller has an edit to commit.
///
/// **A double-click resets to the default**, exactly as a knob does — same gesture, same meaning.
///
/// **A click on the selected cell still fires while something else is sounding.** The selection
/// is the *base*; under modulation the sound is elsewhere, and the person clicking the selected
/// cell is saying "play this one" — with a step selected in the MXM player, that writes the lock.
/// Refusing it read as the button being broken: *"I can change 16' to 8' but not back to 16'"* —
/// the 16' cell looked unlit (the ring was on the sounding cell) and swallowed every click.
fn segment_edit(
    response: &Response,
    index: usize,
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
) -> bool {
    if response.double_clicked()
        && let Some(default_cell) = default_cell
    {
        *selected = default_cell;
        return true;
    }
    let editable_anyway = sounding.is_some_and(|cell| cell != index);
    if response.clicked() && (*selected != index || editable_anyway) {
        *selected = index;
        return true;
    }
    false
}

/// The modulation treatment for one segment, when something else is sounding a different cell.
///
/// A knob under modulation shows an arc and a mark; a row of buttons has no arc to draw, so the
/// **sounding** cell gets a ring in the same reduced-alpha accent the delta lines use — the one
/// colour this system reserves for "what is being played, as opposed to what is set" — and the
/// row gets the same corner dot a marked knob gets. Without either, a locked Range was audible
/// and invisible: the selection is the *base*, and nothing showed the step's choice at all.
fn segment_mark(
    ui: &Ui,
    tokens: &Tokens,
    rect: Rect,
    index: usize,
    count: usize,
    sounding: Option<usize>,
    is_selected: bool,
) {
    let Some(sounding) = sounding else { return };
    let painter = ui.painter();
    if sounding == index && !is_selected {
        painter.rect_stroke(
            rect.shrink(HAIRLINE),
            RADIUS as f32,
            Stroke::new(2.0, tokens.accent.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
    }
    // The mark rides the row's top-right corner, exactly where a knob and a step button carry it.
    if index + 1 == count {
        painter.circle_filled(
            egui::pos2(rect.right() - SPACE_2, rect.top() + SPACE_2),
            HAIRLINE * 2.0,
            tokens.text_primary,
        );
    }
}

/// One segment's background, border and focus ring. Returns the colour its content should take.
///
/// §7.2: state shows through fill **and** another treatment, never hue alone — the selected segment
/// gets the accent fill *and* an accent border *and* primary-weight content, so it survives a
/// monochrome print and a colour-blind viewer.
fn segment_cell(
    ui: &Ui,
    tokens: &Tokens,
    rect: Rect,
    response: &Response,
    selected: bool,
) -> Color32 {
    let (fill, stroke, ink) = if selected {
        (tokens.selection, tokens.accent, tokens.text_primary)
    } else if response.hovered() {
        (tokens.surface_3, tokens.border_strong, tokens.text_primary)
    } else {
        (tokens.surface_2, tokens.border, tokens.text_secondary)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS as f32, fill);
    painter.rect_stroke(
        rect,
        RADIUS as f32,
        Stroke::new(if selected { 2.0 } else { HAIRLINE }, stroke),
        StrokeKind::Inside,
    );

    // Joins the keyboard cursor's registry, if a card and a parameter scope are open around it.
    // Beside the focus ring because the two are the same fact seen from two sides: this is the
    // control the keyboard can reach, and that is what it looks like when it has.
    crate::navigation::mark(ui, response, rect);

    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }
    ink
}

/// The hover text and the accessible name for one segment.
///
/// **The name is not optional and never was.** A segment is a painted rect: `allocate_exact_size`
/// gives a `Response` with no semantics of its own, and `on_hover_text` is a tooltip, not a label.
/// A text segment at least had its word on screen; a waveform has nothing a reader or a test can
/// read, so the name has to be stated here.
fn segment_semantics(response: Response, label: &str, option: &str, detail: &str, selected: bool) {
    let (name, hover) = cell_hover(label, option, detail);
    response.on_hover_text(hover).widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, &name)
    });
}

/// A segmented cell's accessible name and hover text: `label: option`, and under it **what this
/// option does** — its own sentence, never one shared by the row (the owner, 2026-09-27: *Hold,
/// Envelope and Gate do not do the same, so should not have the same text*).
fn cell_hover(label: &str, option: &str, detail: &str) -> (String, String) {
    let name = format!("{label}: {option}");
    let hover = format!("{name}\n{detail}");
    (name, hover)
}

/// A stand-in sentence for each of `options`, for tests that are not about what a cell says.
#[cfg(test)]
pub(crate) fn described<T>(options: &[T]) -> Vec<&'static str> {
    vec!["What this option does."; options.len()]
}

/// **One sentence per option, each its own** — a row whose cells shared a sentence, or that forgot
/// one, fails here in every test that paints it.
fn debug_details(details: &[&str], options: usize) {
    debug_assert_eq!(
        details.len(),
        options,
        "§7.3: a segmented control describes each of its {options} options"
    );
    debug_assert!(
        details.iter().all(|detail| !detail.trim().is_empty()),
        "§7.3: every option of a segmented control has its own hover sentence"
    );
}

/// A waveform, drawn rather than named.
///
/// **The collection's shared vocabulary, not one instrument's.** Every synth here names the same
/// handful of shapes, and a picture of a square wave is the same picture in an LFO, an oscillator
/// and whatever MXM-303 needs — which is why this sits beside the control that draws it rather than
/// in a plugin. New shapes are one arm each; the list is short because it is only what exists.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Wave {
    Triangle,
    Square,
    /// A narrow pulse: high for a quarter of the cycle. Distinct from `Square` on purpose — a
    /// picker offering both, as a machine with a fixed square and a variable pulse does, needs
    /// two different pictures or it cannot be used.
    Pulse,
    /// Rises, then resets. The one usually called *saw*.
    RampUp,
    /// Falls, then resets.
    RampDown,
    /// Sample and hold: a new level each cycle, held flat between.
    Random,
    /// One cycle of a sine. Added for mxm-mono-00's LFOs, whose first shape it is; a shape the
    /// vocabulary lacked, and a picture that cannot be confused with the triangle beside it.
    Sine,
    /// A level near the top with one narrow dip: mxm-mono-08's complex-oscillator *spike*, whose
    /// oscillator holds a small positive level and drops for a twentieth of the cycle. Drawn about
    /// a tenth wide so a 34-point picture shows it, and hanging **down** from a high rest so it
    /// cannot be read as `Pulse`, which stands up from the floor for a quarter.
    Spike,
    /// Audio-rate noise: a jagged line of sloped segments. Distinct on purpose from `Random`, whose
    /// flat held steps are sample-and-hold, not noise.
    Noise,
    /// Random levels joined by smooth curves: a random LFO that glides between its values rather
    /// than stepping (mxm-fx-delay's *Random*).
    SmoothRandom,
    /// One trigger pulse: a low rest with a single narrow pulse standing up in the middle —
    /// mxm-mono-08's per-step trigger enable, which fires a one-sample event rather than a cycle.
    /// `Spike` upside down on purpose: that one hangs from a high rest, this stands from a low one,
    /// and neither starts at the cell's edge the way `Pulse` does.
    Trigger,
    /// An envelope rather than a cycle: an onset, then an exponential fall — mxm-classic-verb's
    /// *Natural* decay, the network's own.
    Decay,
    /// A flat block that stops dead, then silence: mxm-classic-verb's *Gated* decay (one across its
    /// length) and mxm-fx-convolution's *Gate* (one until three quarters of the tail). The cut sits
    /// three quarters along, where the swell's does, so it is high for three quarters where
    /// `Square` is for half.
    Gated,
    /// A swell that stops, then silence: `u²` rising to the cut — mxm-classic-verb's *Reverse*
    /// decay, whose law is `u²` across its length.
    Swell,
    /// An envelope law that leaves a response as it is: an onset and a level held to the end —
    /// mxm-fx-convolution's *Natural* tail shape, whose law multiplies by one.
    Level,
    /// An envelope law falling as `(1 − x)²` from the onset to silence at the end —
    /// mxm-fx-convolution's *Fade*.
    Fade,
    /// An envelope law rising as `x²` to the end, where the response stops — mxm-fx-convolution's
    /// *Swell*, which rises across the whole response rather than to an early cut as `Swell` does.
    Rise,
    /// A quarter note: a filled notehead tilted up to the right and its stem — *tempo sync*, the
    /// musician's sign for *in time with the song* (the owner, 2026-09-25, on mxm-mono-00's delay:
    /// *"a quarter note instead of the text, to make the button smaller"*). Drawn, not a font glyph,
    /// for the reason [`Wave::path`] gives.
    QuarterNote,
}

impl Wave {
    /// One cycle, as points in a unit box: x rightwards, y **downwards** to match screen space.
    ///
    /// A polyline rather than a font glyph or an image: it scales to any cell, takes the ink colour
    /// the segment hands it, and cannot go missing the way a glyph outside the font does — which is
    /// the failure the shared-envelope label already hit once in this repo.
    fn path(self) -> &'static [(f32, f32)] {
        match self {
            // Up and back down, one full cycle.
            Wave::Triangle => &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)],
            // High, then low, with the vertical edges that are the whole point of it.
            Wave::Square => &[(0.0, 1.0), (0.0, 0.0), (0.5, 0.0), (0.5, 1.0), (1.0, 1.0)],
            // The same edges, a quarter wide: what "narrower than square" looks like.
            Wave::Pulse => &[(0.0, 1.0), (0.0, 0.0), (0.25, 0.0), (0.25, 1.0), (1.0, 1.0)],
            // Sixteen segments of one cycle, a curve close enough at a cell's size.
            Wave::Sine => &[
                (0.0000, 0.5000),
                (0.0625, 0.3469),
                (0.1250, 0.2172),
                (0.1875, 0.1304),
                (0.2500, 0.1000),
                (0.3125, 0.1304),
                (0.3750, 0.2172),
                (0.4375, 0.3469),
                (0.5000, 0.5000),
                (0.5625, 0.6531),
                (0.6250, 0.7828),
                (0.6875, 0.8696),
                (0.7500, 0.9000),
                (0.8125, 0.8696),
                (0.8750, 0.7828),
                (0.9375, 0.6531),
                (1.0000, 0.5000),
            ],
            // The reset is drawn, or a ramp is just a diagonal line and the two ramps look alike.
            Wave::RampUp => &[(0.0, 1.0), (1.0, 0.0), (1.0, 1.0)],
            Wave::RampDown => &[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0)],
            // A high rest with one narrow dip in the middle of the cycle.
            Wave::Spike => &[
                (0.0, 0.3),
                (0.45, 0.3),
                (0.45, 1.0),
                (0.55, 1.0),
                (0.55, 0.3),
                (1.0, 0.3),
            ],
            // Sloped segments at irregular levels: no two consecutive levels held flat.
            Wave::Noise => &[
                (0.0, 0.5),
                (0.08, 0.2),
                (0.16, 0.72),
                (0.25, 0.35),
                (0.33, 0.9),
                (0.42, 0.15),
                (0.5, 0.62),
                (0.58, 0.3),
                (0.67, 0.85),
                (0.75, 0.25),
                (0.83, 0.7),
                (0.92, 0.4),
                (1.0, 0.6),
            ],
            // Four random levels, each joined to the next by a smoothstep.
            Wave::SmoothRandom => &[
                (0.0000, 0.6200),
                (0.0625, 0.5794),
                (0.1250, 0.4808),
                (0.1875, 0.3590),
                (0.2500, 0.2487),
                (0.3125, 0.1849),
                (0.3750, 0.2075),
                (0.4375, 0.3284),
                (0.5000, 0.5000),
                (0.5625, 0.6716),
                (0.6250, 0.7925),
                (0.6875, 0.8151),
                (0.7500, 0.7512),
                (0.8125, 0.6410),
                (0.8750, 0.5192),
                (0.9375, 0.4206),
                (1.0000, 0.3800),
            ],
            // A low rest with one narrow pulse standing up in the middle — drawn in the middle
            // three-fifths of the cell, with room around it, because a whole-cell pulse on a
            // row of five read as five loud blocks (the owner, 2026-09-24).
            Wave::Trigger => &[
                (0.2, 0.85),
                (0.44, 0.85),
                (0.44, 0.2),
                (0.56, 0.2),
                (0.56, 0.85),
                (0.8, 0.85),
            ],
            // A vertical onset, then `e^(-4x)` scaled into the box: most of the fall in the first third.
            Wave::Decay => &[
                (0.0, 0.9),
                (0.0, 0.1),
                (0.1, 0.364),
                (0.2, 0.54),
                (0.3, 0.66),
                (0.45, 0.768),
                (0.6, 0.828),
                (0.8, 0.868),
                (1.0, 0.886),
            ],
            // Onset, the level held flat, the cut at three quarters, then silence.
            Wave::Gated => &[
                (0.0, 0.9),
                (0.0, 0.15),
                (0.75, 0.15),
                (0.75, 0.9),
                (1.0, 0.9),
            ],
            // Onset, then the level held across the cell.
            Wave::Level => &[(0.0, 0.9), (0.0, 0.15), (1.0, 0.15)],
            // Onset, then `(1 − x)²` scaled into the box, landing on the floor at the far edge.
            Wave::Fade => &[
                (0.0, 0.9),
                (0.0, 0.15),
                (0.125, 0.326),
                (0.25, 0.478),
                (0.375, 0.607),
                (0.5, 0.713),
                (0.625, 0.795),
                (0.75, 0.853),
                (0.875, 0.888),
                (1.0, 0.9),
            ],
            // The stem, from the notehead's right edge up; the head is [`Wave::fill`]'s.
            Wave::QuarterNote => &[(0.535, 0.7), (0.535, 0.06)],
            // `x²` from the floor to the top at the far edge, where the response stops.
            Wave::Rise => &[
                (0.0, 0.9),
                (0.125, 0.888),
                (0.25, 0.853),
                (0.375, 0.795),
                (0.5, 0.713),
                (0.625, 0.607),
                (0.75, 0.478),
                (0.875, 0.326),
                (1.0, 0.15),
                (1.0, 0.9),
            ],
            // `u²` from the floor into the first three quarters, then the cut and silence.
            Wave::Swell => &[
                (0.0, 0.9),
                (0.0938, 0.888),
                (0.1875, 0.853),
                (0.2812, 0.795),
                (0.375, 0.713),
                (0.4688, 0.607),
                (0.5625, 0.478),
                (0.6562, 0.326),
                (0.75, 0.15),
                (0.75, 0.9),
                (1.0, 0.9),
            ],
            // Held levels with vertical jumps: the shape of sample-and-hold, not of noise.
            Wave::Random => &[
                (0.0, 0.65),
                (0.3, 0.65),
                (0.3, 0.1),
                (0.55, 0.1),
                (0.55, 0.9),
                (0.8, 0.9),
                (0.8, 0.45),
                (1.0, 0.45),
            ],
        }
    }
}

/// The room a caret needs beside the word: the button's horizontal padding.
const CARET_GUTTER: f32 = SPACE_4;

/// One caret, pointing left or right, painted at `centre`.
///
/// Two strokes rather than a filled triangle: at this size a filled arrow reads as a solid blob,
/// and the design system's ink is a line weight everywhere else.
fn caret(ui: &Ui, tokens: &Tokens, centre: egui::Pos2, left: bool) {
    let reach = SPACE_1 * 1.5;
    let x = if left { reach } else { -reach };
    ui.painter().add(Shape::line(
        vec![
            centre + egui::vec2(x, -reach),
            centre + egui::vec2(-x, 0.0),
            centre + egui::vec2(x, reach),
        ],
        Stroke::new(HAIRLINE, tokens.text_secondary),
    ));
}

/// The cell one waveform segment occupies.
///
/// A cycle is conventionally drawn about half again as wide as it is tall, and there is nothing to
/// gain by drawing it larger — so this is the whole cell, not a minimum. Both dimensions are at or
/// above §11's pointer floor.
fn wave_cell() -> Vec2 {
    Vec2::new(MIN_TARGET * 1.5, MIN_TARGET)
}

/// Lays out the label of a control that may share a row with a knob, so the control sits on the
/// knob's grid.
///
/// A knob's column is a fixed grid: a name box [`NAME_LINES`] tall with the name on its **bottom**
/// line, `SPACE_2`, the circle. A control laid out plainly beside it — one label line, `SPACE_2`,
/// its cells — starts where the knob's box starts, so in a top-aligned row its label sits a whole
/// line above the knob's name, and its cells sit that line plus a knob's radius minus half a cell
/// above the circle. **Measured**, at `Standard` with the Body line at 14.9 px: the label 14.9 px
/// high and the cells 22.9 px high, which is not a style. The wave picker had the eight and still
/// missed the line, because the eight was written when a name box was one line tall.
///
/// So beside a knob the label is dropped onto the knob's name line and the content onto the
/// circle's centre — two spacers, both derived from the knob's own geometry, so the next size tier
/// or a change to `NAME_LINES` moves them with it. **The row must be top-aligned**
/// (`Ui::horizontal_top`): the knob's grid is built from the top down, and a row that centres its
/// children moves each by half its own height, which undoes this by a different amount for every
/// pair.
///
/// `None` lays the label out plainly, for a control on a row of its own.
fn beside_a_knob(ui: &mut Ui, label: &str, beside: Option<Size>, content_height: f32) {
    if beside.is_some() {
        let line = ui.text_style_height(&egui::TextStyle::Body);
        ui.add_space(line * (NAME_LINES - 1) as f32);
    }
    ui.label(label);
    if let Some(beside) = beside {
        let lift = (beside.diameter().max(MIN_TARGET) - content_height) / 2.0;
        if lift > 0.0 {
            ui.add_space(lift);
        }
    }
}

impl Wave {
    /// The part of the picture that is **filled** rather than stroked, as a convex outline in the
    /// same unit box: a quarter note's head. `None` for a picture that is only a line.
    fn fill(self) -> Option<&'static [(f32, f32)]> {
        match self {
            // An ellipse about 7.6 × 5.2 points in the cell's drawing box, tilted 22° up to the
            // right, as a notehead is engraved; twelve points, scaled into the unit box.
            Wave::QuarterNote => Some(&[
                (0.544, 0.681),
                (0.544, 0.758),
                (0.517, 0.836),
                (0.469, 0.894),
                (0.413, 0.916),
                (0.365, 0.895),
                (0.336, 0.839),
                (0.336, 0.762),
                (0.363, 0.684),
                (0.411, 0.626),
                (0.467, 0.604),
                (0.515, 0.625),
            ]),
            _ => None,
        }
    }
}

/// Paints one cycle of `wave` inside `rect`, inset so the stroke never touches the border.
fn paint_wave(painter: &egui::Painter, rect: Rect, wave: Wave, ink: Color32) {
    let box_ = rect.shrink2(Vec2::new(SPACE_2, SPACE_2 + HAIRLINE * 2.0));
    let points = wave
        .path()
        .iter()
        .map(|(x, y)| {
            Pos2::new(
                box_.left() + box_.width() * x,
                box_.top() + box_.height() * y,
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, ink)));
    if let Some(fill) = wave.fill() {
        let outline = fill
            .iter()
            .map(|(x, y)| {
                Pos2::new(
                    box_.left() + box_.width() * x,
                    box_.top() + box_.height() * y,
                )
            })
            .collect();
        painter.add(egui::Shape::convex_polygon(outline, ink, Stroke::NONE));
    }
}

/// A segmented control whose options are **waveforms**, §7.3.
///
/// The same control as [`segmented`], with a picture where the word was. Words made every cell as
/// wide as *"Ramp down"*, which is a lot of panel for something a musician recognises instantly by
/// shape — which is what lets this sit beside the knob it belongs to instead of below it.
///
/// **`beside` is the knob it shares a top-aligned row with, if any, and the row is laid out on that
/// knob's grid** — label on the knob's name line, shapes centred on the circle. See
/// [`beside_a_knob`]. `None` for a row of its own: it then starts at its label, with nothing above
/// it and nothing between the label and the shapes but the usual spacing.
///
/// **Cells are small, and the row is spread.** A waveform needs about a pointer target to be
/// legible and nothing more, so the cell is `MIN_TARGET` tall — §11's floor — and no wider than a
/// cycle wants to be drawn. Cells sized to *fill* the row instead came out as wide flat slabs with
/// a small drawing adrift in each. The leftover width goes into the **gaps**, evenly, so the row
/// still spans the space beside the knob rather than bunching against it.
///
/// A stepped parameter with more options than §7.3's five: §7.4's **selector**.
///
/// # One row, not two
///
/// Every plugin that needed one rolled its own, and each drew a label on its own line above a
/// full-width combo box at the pointer height — two lines and a framed box to carry one word. On a
/// card whose job is the knobs above it that is a great deal of furniture, and it was reported as
/// *very large and unelegant* on the routing lines that borrowed the same shape.
///
/// So a selector is **one row**: the name on the left and the value on the right between the
/// shared drawn carets, flat at rest and taking the theme's interaction treatment under the
/// pointer. A column of them reads as a list of settings rather than as a stack of widgets.
///
/// Three rules it keeps, none negotiable:
///
/// - **§11's pointer floor.** The row is [`MIN_TARGET`] tall whatever the text measures.
/// - **Nothing changes size under the pointer.** Hover changes fill only; the geometry is
///   allocated before any interaction state is read.
/// - **A painted control has no name until you give it one.** The button's own text is only the
///   value, so the accessible name is *name: value* — both halves, or a screen reader hears a word
///   with nothing to attach it to.
///
/// Returns `true` when the selection changed. A selection is instantaneous, so callers bracket it
/// with [`ControlOutcome::instant`].
#[allow(clippy::too_many_arguments)]
pub fn selector(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    description: &str,
) -> bool {
    selector_impl(
        ui,
        tokens,
        label,
        label,
        options,
        selected,
        sounding,
        default_option,
        None,
        None,
        description,
        false,
    )
}

/// [`selector`], **painting a shorter name than it announces** — the short form its card allows
/// (design system §7.1), as [`segmented_named`] is for a segmented control. `name` stays the
/// accessible name and the hover text.
#[allow(clippy::too_many_arguments)]
pub fn selector_named(
    ui: &mut Ui,
    tokens: &Tokens,
    painted: &str,
    name: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    description: &str,
) -> bool {
    selector_impl(
        ui,
        tokens,
        painted,
        name,
        options,
        selected,
        sounding,
        default_option,
        None,
        None,
        description,
        false,
    )
}

/// A [`selector`] whose open menu inserts non-selectable headings when the group changes.
///
/// `groups` is parallel to `options`; adjacent equal names share one heading. Option order and
/// keyboard stepping remain exactly the supplied option order, so grouping cannot remap a stored
/// value or create an extra keyboard cell.
#[allow(clippy::too_many_arguments)]
pub fn selector_grouped(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    groups: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    description: &str,
) -> bool {
    assert_eq!(options.len(), groups.len(), "one selector group per option");
    selector_impl(
        ui,
        tokens,
        label,
        label,
        options,
        selected,
        sounding,
        default_option,
        None,
        Some(groups),
        description,
        false,
    )
}

/// [`selector`] in the caption typography, for a target card's routing footer.
/// Still a full pointer target and a real source menu, never passive status text.
#[allow(clippy::too_many_arguments)]
pub fn selector_caption(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    description: &str,
) -> bool {
    selector_impl(
        ui,
        tokens,
        label,
        label,
        options,
        selected,
        sounding,
        default_option,
        None,
        None,
        description,
        true,
    )
}

/// A source menu drawn as a secondary sublabel immediately below its owning control.
/// `displayed` may resolve a Default option to its actual source; the menu and accessibility
/// name retain the full option. The hit area is fixed before hover/modulation is considered.
#[allow(clippy::too_many_arguments)]
pub fn selector_sublabel(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    displayed: &str,
    description: &str,
) -> bool {
    let style = crate::typography::caption_style(ui.style());
    selector_caret(
        ui,
        tokens,
        label,
        options,
        selected,
        sounding,
        default_option,
        displayed,
        None,
        description,
        style,
    )
}

/// [`selector_sublabel`], on the **name line of the control it belongs to** rather than under it.
///
/// The only difference is the type: it takes the label's own text style instead of caption, because
/// a caption's line box is four points shorter than a label's and centring the two in one row left
/// the source sitting two points low against the words either side of it. Reported as the dropdown
/// text jumping compared to the rest, and it was.
#[allow(clippy::too_many_arguments)]
pub fn selector_inline(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    displayed: &str,
    description: &str,
) -> bool {
    selector_caret(
        ui,
        tokens,
        label,
        options,
        selected,
        sounding,
        default_option,
        displayed,
        None,
        description,
        egui::TextStyle::Body,
    )
}

/// Above this many options a selector's menu opens with a search field at its top — §7.4, *long
/// lists support type-to-search*.
///
/// **24, and above every list an editor draws today.** The longest are 17, mxm-creative-sampler's
/// Generate recipes and mxm-mono-08's `modulate` menus; mxm-mono-00's route menus and
/// mxm-mono-pr1's `modulate` menu are 15, and the other `modulate` menus 12. So no existing menu
/// changed when search arrived, and a list that grows past 17 still has room before its menu does. The list it
/// was built for is mxm-classic-verb's Space selector, at about a hundred positions.
pub const SEARCH_ABOVE: usize = 24;

/// What a selector menu remembers between frames and openings. The query stays until its clear
/// action is used; `frames` returns to zero when the popup closes so every reopening still gets
/// the same focus and scroll treatment as a newly opened menu.
#[derive(Clone, Default)]
struct SelectorMenu {
    query: String,
    /// Frames drawn since the current opening, saturating.
    frames: u8,
}

/// The open list of a caret selector. Returns the option chosen this frame, if any.
///
/// **It stays inside the window.** The options sit in a vertical `ScrollArea` whose height is the
/// larger of the room above and below the button, less the popup frame and the search field: egui
/// already tries the side below first and then the side above against the window
/// (`RectAlign::find_best_align`), so capping the list at the larger side is what lets one of them
/// fit. The cap is set as both the maximum *and* the minimum scrolled height, because a `ScrollArea`
/// otherwise measures against the popup's size from the last frame, and a menu that opened short
/// could never grow. It still shrinks to its content, so a list that fits is laid out exactly as it
/// was before there was a `ScrollArea` (`a_short_selector_menu_is_laid_out_as_a_plain_menu`).
///
/// **It opens at the selection.** For its first two frames — egui's invisible sizing pass and the
/// first one anybody sees — the selected row is scrolled to the middle, without animation.
///
/// **A long list is searchable** ([`SEARCH_ABOVE`]). The field takes the keyboard when the menu
/// opens; typing filters by case-insensitive substring; `Enter` chooses the first match and closes
/// (with nothing typed there is no match, so it only closes); `↓` hands the keyboard to the first
/// row, where egui's own focus travel walks the list as it does in a short menu; `Escape` closes
/// it as egui closes any menu.
#[allow(clippy::too_many_arguments)]
fn selector_menu(
    ui: &mut Ui,
    tokens: &Tokens,
    anchor: Rect,
    memory_id: egui::Id,
    label: &str,
    options: &[&str],
    selected: usize,
    groups: Option<&[&str]>,
) -> Option<usize> {
    let mut memory = ui
        .data(|d| d.get_temp::<SelectorMenu>(memory_id))
        .unwrap_or_default();
    let opening = memory.frames < 2;
    memory.frames = memory.frames.saturating_add(1);
    // §6: an option never breaks mid-word, whatever the popup's width.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

    let mut chosen = None;
    let mut enter = false;
    let mut down = false;
    if options.len() > SEARCH_ABOVE {
        // As wide as the widest row and no wider: a field at egui's default width would set the
        // menu's width instead of the options.
        let pad = ui.spacing().button_padding.x;
        let width = options
            .iter()
            .copied()
            .chain(groups.into_iter().flatten().copied())
            .map(|option| text_width(ui, option, egui::TextStyle::Button) + 2.0 * pad)
            .fold(
                text_width(ui, "Search", egui::TextStyle::Button) + 8.0,
                f32::max,
            );
        let clear_label = format!("Clear {label} search");
        let field_width = (width - REMOVE_SIZE - ui.spacing().item_spacing.x).max(MIN_TARGET);
        let field = ui
            .horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut memory.query)
                        .hint_text("Search")
                        .desired_width(field_width),
                );
                let clear = remove_mark(
                    ui,
                    tokens,
                    REMOVE_SIZE,
                    !memory.query.is_empty(),
                    &clear_label,
                    false,
                    "Clear the saved search and show every option.",
                );
                (field, clear)
            })
            .inner;
        // The hint is a placeholder, not a name. Unlabelled, a screen reader announces a text
        // field with nothing to say what it searches.
        let name = format!("{label} search");
        ui.ctx()
            .accesskit_node_builder(field.0.id, |node| node.set_label(name));
        if field.1 {
            memory.query.clear();
            field.0.request_focus();
        }
        if opening {
            field.0.request_focus();
        }
        enter = field.0.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        down =
            field.0.has_focus() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown));
    }

    let needle = memory.query.to_lowercase();
    let matches = |option: &str| needle.is_empty() || option.to_lowercase().contains(&needle);

    let window = ui.ctx().content_rect();
    let room = (window.bottom() - anchor.bottom()).max(anchor.top() - window.top());
    let frame = egui::Frame::popup(ui.style()).total_margin().sum().y;
    let above = ui.cursor().top() - ui.max_rect().top();
    let height = (room - frame - above).floor().max(MIN_TARGET);

    egui::ScrollArea::vertical()
        .id_salt("mxm-selector-options")
        .max_height(height)
        .min_scrolled_height(height)
        .content_margin(egui::Margin::ZERO)
        .animated(false)
        .show(ui, |ui| {
            let mut first = true;
            let mut last_group = None;
            for (index, option) in options.iter().enumerate() {
                if !matches(option) {
                    continue;
                }
                if let Some(group) = groups.and_then(|groups| groups.get(index)).copied()
                    && !group.is_empty()
                    && last_group != Some(group)
                {
                    // A heading is not an option. Give it the collection's semibold heading style,
                    // not merely `strong()` at the option's body size: at that size the two cuts
                    // were too similar to scan in a long drum catalogue. An empty group deliberately
                    // leaves utility choices such as Off standing alone above the first family.
                    ui.label(
                        egui::RichText::new(group)
                            .text_style(egui::TextStyle::Heading)
                            .strong(),
                    );
                    last_group = Some(group);
                }
                let row = ui.selectable_label(index == selected, *option);
                if row.clicked() {
                    chosen = Some(index);
                    ui.close();
                }
                // Scroll targets are set in here, never after: one set once this `ScrollArea`
                // has ended is taken by whichever encloses the selector, and scrolls the page.
                if opening && index == selected {
                    row.scroll_to_me(Some(egui::Align::Center));
                }
                if first {
                    if enter && !needle.is_empty() {
                        chosen = Some(index);
                    }
                    if down {
                        row.request_focus();
                        row.scroll_to_me(None);
                    }
                    first = false;
                }
            }
        });

    if enter {
        ui.close();
    }
    ui.data_mut(|d| d.insert_temp(memory_id, memory));
    chosen
}

#[allow(clippy::too_many_arguments)]
fn selector_caret(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    displayed: &str,
    groups: Option<&[&str]>,
    description: &str,
    style: egui::TextStyle,
) -> bool {
    assert!(!options.is_empty());
    let current = options[(*selected).min(options.len() - 1)];
    let mut chosen = None;
    ui.scope(|ui| {
        // Room for a caret either side of the word, and nothing else: the control is as wide as
        // what it says. Stretching it across the card put the value adrift in its own empty box
        // and made a one-word setting look like a text field.
        ui.spacing_mut().button_padding = egui::vec2(CARET_GUTTER, SPACE_1);
        // No fill and no border at rest. It carried the theme's resting treatment for one round,
        // before the carets existed; a caret either side says "one of a set" on its own, and the
        // box around it was then simply weight (owner, 2026-09-07).
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        let text = egui::RichText::new(displayed)
            .text_style(style)
            .color(tokens.text_secondary);
        // Never wrap: §7.1's unbreakable rule. Left to itself egui breaks mid-word, and a source
        // menu in a knob's column came out as "Oscillato" over "r 2". A word that cannot fit
        // overflows visibly instead, which points at the column as the thing to widen.
        let button = egui::Button::new(text)
            .wrap_mode(egui::TextWrapMode::Extend)
            .min_size(egui::vec2(MIN_TARGET, MIN_TARGET));
        // `MenuButton::ui`'s own two steps, taken in the open: the button, then `Popup::menu` on
        // its response with the menu config `MenuButton` would have found. The popup id is still
        // `Popup::default_response_id` of the button, so nothing painted beside it can move it.
        // Taking them apart gives the list the button's rect and id before it is drawn — the
        // rect sizes the list against the window, the id keys its search.
        let response = ui.add(button);
        let config = egui::containers::menu::MenuConfig::find(ui);
        let anchor = response.rect;
        let menu_memory = response.id.with("mxm-selector-menu");
        let open = egui::Popup::menu(&response)
            // Rows close explicitly. Keeping inside clicks open lets the search field and its
            // clear action behave like one editor rather than treating Clear as a selection.
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .style(config.style.clone())
            .info(
                egui::UiStackInfo::new(egui::UiKind::Menu)
                    .with_tag_value(egui::containers::menu::MenuConfig::MENU_CONFIG_TAG, config),
            )
            .show(|ui| {
                selector_menu(
                    ui,
                    tokens,
                    anchor,
                    menu_memory,
                    label,
                    options,
                    *selected,
                    groups,
                )
            });
        match &open {
            Some(inner) => {
                if let Some(index) = inner.inner {
                    chosen = Some(index);
                }
            }
            // Closing resets opening-only state but deliberately keeps the query. Long catalogue
            // work commonly means assigning several slots from one search; clearing is explicit.
            None => ui.data_mut(|d| {
                let mut memory = d.get_temp::<SelectorMenu>(menu_memory).unwrap_or_default();
                memory.frames = 0;
                d.insert_temp(menu_memory, memory);
            }),
        }

        // **A selector is a stepped parameter drawn as a menu**, so it joins the cursor's registry
        // and answers the arrows exactly as a segmented control does. Without this the cursor
        // cannot reach a routing surface at all: mxm-mono-08 draws its whole source list here, and
        // every one of them would have been a control the keyboard could see on screen and never
        // touch.
        crate::navigation::mark(ui, &response, response.rect);
        // Not while the menu is up. Its own list is what the arrows are walking then, and egui
        // owns that keyboard.
        if open.is_none() && owns_value_keys(ui, response.has_focus()) {
            let mut index = (*selected).min(options.len() - 1);
            if segmented_keyboard(ui, &mut index, options.len(), default_option) {
                chosen = Some(index);
            }
        }
        // Carets either side of the value, painted rather than set: no font in the stack is
        // guaranteed to carry a chevron glyph. A single trailing triangle said "a menu opens
        // here"; a pair says "this is one of a set", which is what a source actually is, and it
        // is the affordance the owner asked for from Arturia's interfaces.
        // They sit in the button's own padding, so the word is centred between them and
        // never runs into one.
        let inset = CARET_GUTTER / 2.0;
        caret(
            ui,
            tokens,
            response.rect.left_center() + egui::vec2(inset, 0.0),
            true,
        );
        caret(
            ui,
            tokens,
            response.rect.right_center() - egui::vec2(inset, 0.0),
            false,
        );
        if let Some(default) = default_option
            && response.double_clicked()
        {
            chosen = Some(default);
        }
        if sounding.is_some_and(|cell| cell != *selected) {
            // Painted, not laid out: automation must not move the value or replace the popup ID.
            ui.painter().circle_filled(
                response.rect.right_top() + egui::vec2(-SPACE_2, SPACE_2),
                SPACE_1,
                tokens.accent,
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::ComboBox,
                true,
                format!("{label}: {current}"),
            )
        });
        response.on_hover_text(format!("{label}: {current}\n{description}"));
    });
    match chosen {
        Some(index) if index != *selected || sounding.is_some_and(|cell| cell != index) => {
            *selected = index;
            true
        }
        _ => false,
    }
}

/// [`selector`], for a control that shares a top-aligned row with a knob.
///
/// A second entry point rather than a changed one, as with [`segmented_beside`]: most selectors
/// stand on a row of their own, and making all of them pass `None` to gain an alignment they do
/// not use is how a shared API acquires ceremony. Because selector name and value form one row,
/// the row aligns to the bottom line of the knob's fixed name box; unlike segmented cells, it is
/// not lowered onto the circle.
#[allow(clippy::too_many_arguments)]
pub fn selector_beside(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    beside: Size,
    description: &str,
) -> bool {
    selector_impl(
        ui,
        tokens,
        label,
        label,
        options,
        selected,
        sounding,
        default_option,
        Some(beside),
        None,
        description,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn selector_impl(
    ui: &mut Ui,
    tokens: &Tokens,
    painted: &str,
    label: &str,
    options: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_option: Option<usize>,
    beside: Option<Size>,
    groups: Option<&[&str]>,
    description: &str,
    caption: bool,
) -> bool {
    debug_assert!(
        !options.is_empty(),
        "a selector needs options; {label} was given none"
    );

    let current = options[(*selected).min(options.len() - 1)];
    let beside_knob = beside.is_some();
    let name_style = if caption {
        crate::typography::caption_style(ui.style())
    } else if beside_knob {
        egui::TextStyle::Body
    } else {
        label_style(ui.style())
    };
    let current_style = if caption || beside_knob {
        name_style.clone()
    } else {
        value_style(ui.style())
    };
    let mut changed = false;

    ui.vertical(|ui| {
        // A selector is already one text row: name and chosen value belong together. Beside a
        // knob that row aligns with the bottom line of the knob's fixed two-line name box. Unlike
        // a segmented control, it has no separate content row to lower onto the knob circle.
        if beside_knob {
            let body = ui
                .style()
                .text_styles
                .get(&egui::TextStyle::Body)
                .expect("egui always defines Body");
            let line = ui.text_style_height(&egui::TextStyle::Body);
            // `horizontal` centres the Body text in the pointer-height response. Offset by half
            // the glyph size so that centre lands on the bottom line of `fixed_label`, rather
            // than blindly adding a whole line and landing below it.
            ui.add_space((line * (NAME_LINES - 1) as f32 - body.size / 2.0).max(0.0));
        }

        ui.horizontal(|ui| {
            ui.set_min_height(MIN_TARGET);
            ui.label(
                egui::RichText::new(painted)
                    .text_style(name_style)
                    .color(if caption {
                        tokens.text_secondary
                    } else {
                        tokens.text_primary
                    }),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                changed = selector_caret(
                    ui,
                    tokens,
                    label,
                    options,
                    selected,
                    sounding,
                    default_option,
                    current,
                    groups,
                    description,
                    current_style,
                );
            });
        });
    });

    changed
}

/// Returns `true` when the selection changed.
///
/// **Up to five in a row, and more in balanced rows** — six are two rows of three (the owner,
/// 2026-09-23: *"What about putting them in 2 rows?"*). That is a declared exception to §7.3's "2–5,
/// otherwise a menu", for waveform pictures only: a cell is a small picture, so two rows of three
/// fit where a menu would hide every shape but one. The arrows walk the options in order across
/// the rows, and the grid is one control and one keyboard-cursor target.
///
/// **Cells are fixed and the gaps are too** — `SPACE_2`, never the leftover width shared out (the
/// owner's *"as small as possible"*).
#[allow(clippy::too_many_arguments)] // Same shape as `segmented`, plus the cell geometry.
pub fn segmented_waves(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[(Wave, &str)],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    beside: Option<Size>,
    details: &[&str],
) -> bool {
    segmented_waves_impl(
        ui,
        tokens,
        label,
        options,
        None,
        selected,
        sounding,
        default_cell,
        beside,
        Some(label),
        details,
    )
}

/// [`segmented_waves`], **painting no label** — for a grid whose card already says what it is, where
/// a label line would cost the card its height (mxm-drum-machine's LFO rows, whose rate knob carries
/// the LFO's number). `label` is still the accessible name and the hover text of every cell.
#[allow(clippy::too_many_arguments)]
pub fn segmented_waves_unlabelled(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[(Wave, &str)],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    details: &[&str],
) -> bool {
    segmented_waves_impl(
        ui,
        tokens,
        label,
        options,
        None,
        selected,
        sounding,
        default_cell,
        None,
        None,
        details,
    )
}

/// [`segmented_waves`], with a short **mark** painted beside each picture — for options that differ
/// in more than their shape: mxm-mono-01's sub-oscillator is a square one octave down, a square two
/// down, and a pulse two down, so the pictures carry `−1` and `−2` (the owner, 2026-09-23). Every
/// cell grows by the widest mark, so the cells stay equal.
#[allow(clippy::too_many_arguments)]
pub fn segmented_waves_marked(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[(Wave, &str)],
    marks: &[&str],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    beside: Option<Size>,
    details: &[&str],
) -> bool {
    debug_assert_eq!(options.len(), marks.len(), "one mark per option");
    segmented_waves_impl(
        ui,
        tokens,
        label,
        options,
        Some(marks),
        selected,
        sounding,
        default_cell,
        beside,
        Some(label),
        details,
    )
}

/// How many columns a wave control with `count` options lays out in: all of them up to five, and
/// then balanced rows of no more than five.
fn wave_columns(count: usize) -> usize {
    let rows = count.div_ceil(5).max(1);
    count.div_ceil(rows)
}

/// [`segmented_waves`], **painting a shorter label than it announces** — the waveform form of
/// [`segmented_named`]: `painted` is the line above the pictures (*Wave* on a card titled *Mod
/// oscillator*), `name` the parameter's own, which every cell's accessible name and tooltip carry.
#[allow(clippy::too_many_arguments)]
pub fn segmented_waves_named(
    ui: &mut Ui,
    tokens: &Tokens,
    painted: &str,
    name: &str,
    options: &[(Wave, &str)],
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    beside: Option<Size>,
    details: &[&str],
) -> bool {
    segmented_waves_impl(
        ui,
        tokens,
        name,
        options,
        None,
        selected,
        sounding,
        default_cell,
        beside,
        Some(painted),
        details,
    )
}

#[allow(clippy::too_many_arguments)]
fn segmented_waves_impl(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    options: &[(Wave, &str)],
    marks: Option<&[&str]>,
    selected: &mut usize,
    sounding: Option<usize>,
    default_cell: Option<usize>,
    beside: Option<Size>,
    painted: Option<&str>,
    details: &[&str],
) -> bool {
    debug_assert!(
        (2..=10).contains(&options.len()),
        "§7.3: a waveform picker holds 2-10 pictures in rows of five at most; {} needs a menu",
        options.len()
    );
    debug_details(details, options.len());

    let mut changed = false;
    let mut focused = false;
    let columns = wave_columns(options.len());
    let rows = options.len().div_ceil(columns);

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SPACE_2;
        let font = crate::typography::caption_style(ui.style()).resolve(ui.style());
        let mark_width = marks.map_or(0.0, |marks| {
            marks
                .iter()
                .map(|mark| {
                    ui.painter()
                        .layout_no_wrap((*mark).to_owned(), font.clone(), Color32::WHITE)
                        .size()
                        .x
                })
                .fold(0.0, f32::max)
                + SPACE_2
        });
        let size = wave_cell() + Vec2::new(mark_width, 0.0);
        let height = size.y * rows as f32 + SPACE_2 * (rows as f32 - 1.0);
        if let Some(painted) = painted {
            beside_a_knob(ui, painted, beside, height);
        }

        for row in options.chunks(columns).enumerate() {
            let (row, cells) = row;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = SPACE_2;
                for (column, (wave, name)) in cells.iter().enumerate() {
                    let index = row * columns + column;
                    let is_selected = index == *selected;
                    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

                    let ink = segment_cell(ui, tokens, rect, &response, is_selected);
                    focused |= response.has_focus();
                    let picture = Rect::from_min_size(rect.min, wave_cell());
                    paint_wave(ui.painter(), picture, *wave, ink);
                    if let Some(mark) = marks.and_then(|marks| marks.get(index)) {
                        ui.painter().text(
                            egui::pos2(picture.right(), rect.center().y),
                            Align2::LEFT_CENTER,
                            *mark,
                            font.clone(),
                            ink,
                        );
                    }

                    changed |= segment_edit(&response, index, selected, sounding, default_cell);

                    segment_mark(
                        ui,
                        tokens,
                        rect,
                        index,
                        options.len(),
                        sounding,
                        is_selected,
                    );

                    segment_semantics(response, label, name, details[index], is_selected);
                }
            });
        }
    });

    if owns_value_keys(ui, focused) {
        changed |= segmented_keyboard(ui, selected, options.len(), default_cell);
    }
    changed
}

/// An on/off switch that **draws its waveform instead of naming it** — for oscillators and LFOs
/// whose shapes are separate switches that combine, rather than one choice among several
/// (mxm-poly-06's *Saw* and *Pulse*, mxm-mono-pr1's shapes). The owner, 2026-09-23: *"picture
/// switches, but drop the on off mark."*
///
/// A wave cell's size, a toggle's states: the fill and a border that doubles when on. The name is
/// the accessible name and the tooltip, as a painted segment's is.
pub fn toggle_wave(
    ui: &mut Ui,
    tokens: &Tokens,
    wave: Wave,
    name: &str,
    on: &mut bool,
    marked: bool,
    description: &str,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(toggle_wave_size_of(wave), Sense::click());
    let (fill, stroke, ink) = if *on {
        (tokens.selection, tokens.accent, tokens.text_primary)
    } else if response.hovered() {
        (tokens.surface_3, tokens.border_strong, tokens.text_primary)
    } else {
        (tokens.surface_2, tokens.border, tokens.text_secondary)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS as f32, fill);
    painter.rect_stroke(
        rect,
        RADIUS as f32,
        Stroke::new(if *on { 2.0 } else { HAIRLINE }, stroke),
        StrokeKind::Inside,
    );
    // Painted in a wave cell centred on the button, so a square one (the quarter note) keeps the
    // picture's proportions; for every other wave the two are the same rectangle.
    paint_wave(
        painter,
        Rect::from_center_size(rect.center(), wave_cell()),
        wave,
        ink,
    );
    if marked {
        painter.circle_filled(
            rect.right_top() + Vec2::new(-MARK_INSET, MARK_INSET),
            HAIRLINE * 2.0,
            ink,
        );
    }
    crate::navigation::mark(ui, &response, rect);
    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }
    let clicked = response.clicked();
    if clicked {
        *on = !*on;
    }
    let keyed = toggle_keyboard(ui, on);
    let response = response.on_hover_text(format!("{name}\n{description}"));
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *on, name));
    clicked || keyed
}

/// A toggle, §7.2. State shows through fill **and** label treatment, never hue alone.
/// `marked` is [`ParamView::marked`]'s fact on a control that has no `ParamView`: **something other
/// than this button is also setting it.** A toggle bound to a parameter a host can modulate needs it
/// for the same reason a knob and a segmented control do — a control that stays put while the sound
/// moves reads as broken, and that was the same defect three times over before the other two
/// carried it.
pub fn toggle(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    on: &mut bool,
    marked: bool,
    description: &str,
) -> bool {
    toggle_labelled(ui, tokens, label, label, on, marked, description)
}

/// How far the modulation dot's centre sits from a toggle's top-right corner: inside the `SPACE_3`
/// the label leaves, so the dot's two-point radius never reaches the text.
const MARK_INSET: f32 = SPACE_2 + HAIRLINE;

/// [`toggle`], with the painted label and the canonical name separated.
///
/// The split is [`ParamView`]'s, one control down: `label` is what the button says when the card
/// around it already supplies the prefix, and `name` is the parameter's own name, which host
/// automation, the tooltip and a screen reader use. **The width follows `label`, not `name`** —
/// that is the entire point. A row of five `Step N trigger` toggles is wider than the one-card
/// minimum; five saying `Trigger` is not, and neither the accessibility tree nor a test looking a
/// control up by name can tell the difference.
#[allow(clippy::too_many_arguments)]
pub fn toggle_labelled(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    name: &str,
    on: &mut bool,
    marked: bool,
    description: &str,
) -> bool {
    toggle_sized(ui, tokens, label, name, on, marked, description, 0.0)
}

/// [`toggle_labelled`], at least `width` wide — for **toggles that stand together**, which share
/// the widest label's width ([`shared_toggle_width`]) so a stack is even. Never narrower than its
/// own label, whatever it is handed.
///
/// **No ●/○ mark** (the owner, 2026-09-23: *"the pronounced border is enough"*). State shows
/// through the fill and the border, whose width doubles when on: a width is not a hue, so the
/// state still does not rest on colour alone (§7.2).
#[allow(clippy::too_many_arguments)]
pub fn toggle_sized(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    name: &str,
    on: &mut bool,
    marked: bool,
    description: &str,
    width: f32,
) -> bool {
    let width = width
        .max(scoped_floor(ui, TOGGLE_FLOOR).unwrap_or(0.0))
        .max(toggle_min_width(ui, label));
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, MIN_TARGET), Sense::click());

    let (fill, stroke, text) = if *on {
        (tokens.selection, tokens.accent, tokens.text_primary)
    } else if response.hovered() {
        (tokens.surface_3, tokens.border_strong, tokens.text_primary)
    } else {
        (tokens.surface_2, tokens.border, tokens.text_secondary)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS as f32, fill);
    painter.rect_stroke(
        rect,
        RADIUS as f32,
        Stroke::new(if *on { 2.0 } else { HAIRLINE }, stroke),
        StrokeKind::Inside,
    );
    painter.text(
        rect.left_center() + Vec2::new(SPACE_3, 0.0),
        Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Button.resolve(ui.style()),
        text,
    );

    // The mark, painted in the corner padding rather than laid out beside the label: the button is
    // a fixed rect, and the label ends `SPACE_3` short of the right edge, so a dot centred
    // `MARK_INSET` in stays clear of it at every width.
    if marked {
        painter.circle_filled(
            rect.right_top() + Vec2::new(-MARK_INSET, MARK_INSET),
            HAIRLINE * 2.0,
            text,
        );
    }

    // Joins the keyboard cursor's registry, if a card and a parameter scope are open around it.
    // Beside the focus ring because the two are the same fact seen from two sides: this is the
    // control the keyboard can reach, and that is what it looks like when it has.
    crate::navigation::mark(ui, &response, rect);

    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }

    let clicked = response.clicked();
    if clicked {
        *on = !*on;
    }
    let keyed = toggle_keyboard(ui, on);
    // Named for the accessibility tree, as every control here must be: without this a screen
    // reader cannot find it and neither can a test that measures where it landed. The name is the
    // canonical one, never the painted label, so shortening the label costs nothing here.
    let response = response.on_hover_text(format!("{name}\n{description}"));
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *on, name));
    clicked || keyed
}

/// A standalone toggle aligned beside a knob in a top-aligned row.
///
/// The button already carries its own label, so unlike [`segmented_beside`] it needs only the
/// knob-grid drop: its centre lands on the knob circle's centre at every semantic size.
pub fn toggle_beside(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    on: &mut bool,
    marked: bool,
    beside: Size,
    description: &str,
) -> bool {
    let line = ui.text_style_height(&egui::TextStyle::Body);
    let knob = beside.diameter().max(MIN_TARGET);
    let drop = line * NAME_LINES as f32 + SPACE_2 + (knob - MIN_TARGET) / 2.0;
    let mut changed = false;
    ui.vertical(|ui| {
        ui.add_space(drop);
        changed = toggle(ui, tokens, label, on, marked, description);
    });
    changed
}

/// A compact square toggle for dense inventory rows. `mark` is the short visible text while
/// `label` remains the complete tooltip and accessibility name. State uses fill plus border weight,
/// not colour alone; the target remains the collection pointer minimum.
pub fn toggle_compact(
    ui: &mut Ui,
    tokens: &Tokens,
    mark: &str,
    label: &str,
    on: &mut bool,
    marked: bool,
    description: &str,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(MIN_TARGET), Sense::click());
    let (fill, stroke, text) = if *on {
        (tokens.selection, tokens.accent, tokens.text_primary)
    } else if response.hovered() {
        (tokens.surface_3, tokens.border_strong, tokens.text_primary)
    } else {
        (tokens.surface_2, tokens.border, tokens.text_secondary)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS as f32, fill);
    painter.rect_stroke(
        rect,
        RADIUS as f32,
        Stroke::new(if *on { 2.0 } else { HAIRLINE }, stroke),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        mark,
        egui::TextStyle::Button.resolve(ui.style()),
        text,
    );
    if marked {
        painter.circle_filled(
            rect.right_top() + Vec2::new(-SPACE_3, SPACE_3),
            HAIRLINE * 2.0,
            text,
        );
    }

    crate::navigation::mark(ui, &response, rect);
    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }
    let clicked = response.clicked();
    if clicked {
        *on = !*on;
    }
    let keyed = toggle_keyboard(ui, on);
    let response = response.on_hover_text(format!("{label}\n{description}"));
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *on, label));
    clicked || keyed
}

/// The bare arrows on a toggle the cursor is on: a two-cell segmented control, off then on.
///
/// Right/Up is on and Left/Down off, `Home`/`End` the same two, exactly as [`segmented`] reads
/// them. Without it a toggle could be selected — by the cursor, or now by clicking it — and then
/// ignore every arrow, which is a target the keyboard can land on and do nothing with. Only under
/// the cursor: `Enter`/`Space` still click a focused toggle everywhere, as egui does for any
/// clickable widget, and a toggle that is not a parameter (a disclosure) is never a cursor target.
fn toggle_keyboard(ui: &Ui, on: &mut bool) -> bool {
    if !crate::navigation::keyboard_target(ui) {
        return false;
    }
    let mut cell = usize::from(*on);
    if segmented_keyboard(ui, &mut cell, 2, None) {
        *on = cell == 1;
        return true;
    }
    false
}

/// The width and height a [`remove_mark`] is given in a route row: square, at the pointer floor.
///
/// A row reserves this before handing the rest of its width to the slider, because a slider lays
/// its value out right-aligned across the whole width its `Ui` has and would otherwise push the
/// cross past the card's border.
pub const REMOVE_SIZE: f32 = MIN_TARGET;

/// A **remove**, drawn as the mark alone: no fill, no border, just the cross.
///
/// A cross at the end of a row of framed things must not be framed itself — a second frame reads as
/// another item rather than as an action on the item before it. That is the owner's ruling, first
/// for the sampler's layer chips and again for the route rows, which is why there is no framed
/// form: every remove in the collection sits at the end of a framed row.
///
/// **The hit area is still the full square**, so losing the frame costs nothing in pointer target:
/// the caller passes the row's own height and the cross is drawn inset inside it. Meaning lives in
/// the tooltip, as it does for every icon-only control — §3.3 asks that one explain itself, not
/// that it grow a label.
///
/// `shown` draws nothing while still allocating the square. A row whose crosses came and went as
/// layers loaded would reflow under the pointer, and the measured card floor would depend on what
/// happened to be loaded when it was measured.
///
/// **It is a parameter control wherever it clears a parameter.** `label` is what it removes —
/// *"Remove LFO from Cutoff"* — and is the accessible name rather than a bare "Remove"; `marked`
/// draws the dot that says something else is setting the same parameter. A caller that clears no
/// parameter passes its own description as the label and `false`.
pub fn remove_mark(
    ui: &mut Ui,
    tokens: &Tokens,
    size: f32,
    shown: bool,
    label: &str,
    marked: bool,
    description: &str,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(size),
        if shown {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if !shown {
        return false;
    }

    let ink = if response.hovered() {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    // Inset by a third, so the cross reads as a mark with air around it rather than as an X filling
    // a box. Two line segments, never a `✕`: the editor's font has no glyph for U+2715 and it drew
    // as `?`.
    let arm = rect.shrink(size / 3.0);
    let cross = Stroke::new(HAIRLINE * 1.5, ink);
    let painter = ui.painter();
    painter.line_segment([arm.left_top(), arm.right_bottom()], cross);
    painter.line_segment([arm.right_top(), arm.left_bottom()], cross);

    // The mark sits in the corner rather than in the flow: the square is fixed and has no text for
    // a dot to displace. Kept inside the allocated rect, so it cannot widen the row.
    if marked {
        painter.circle_filled(
            rect.right_top() + Vec2::new(-SPACE_2, SPACE_2),
            HAIRLINE * 2.0,
            ink,
        );
    }

    crate::navigation::mark_unclaimed(ui, response.id, rect);
    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }

    let clicked = response.clicked();
    let response = response.on_hover_text(format!("{label}\n{description}"));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    clicked
}

// ---------------------------------------------------------------------------------------------
// Shared interaction
// ---------------------------------------------------------------------------------------------

/// Drag, fine adjustment, double-click reset and keyboard nudging — §7.1's continuous-control set.
fn drag_edit(
    ui: &Ui,
    response: &Response,
    param: &ParamView<'_>,
    normalised: &mut f64,
    wheel: Wheel,
) -> ControlOutcome {
    let mut outcome = ControlOutcome::default();
    if param.read_only {
        return outcome;
    }

    if response.double_clicked() {
        *normalised = param.default.clamp(0.0, 1.0);
        return ControlOutcome::reset_to_default();
    }

    if response.drag_started() {
        outcome.gesture_started = true;
        // The value the drag starts from, remembered now — see `DragAnchor`.
        ui.data_mut(|d| {
            d.insert_temp(
                anchor_id(response),
                DragAnchor {
                    start: *normalised,
                    travelled: 0.0,
                },
            )
        });
    }

    if response.dragged() {
        // Up increases, wherever the pointer is. A knob has no track to point at, so it is
        // relational by nature, and vertical is the direction knobs have used for forty years.
        let delta = -response.drag_delta().y;
        if delta != 0.0 {
            let travel = if ui.input(|i| i.modifiers.shift) {
                DRAG_TRAVEL as f64 * FINE_FACTOR
            } else {
                DRAG_TRAVEL as f64
            };
            // **From the anchor, not from the value on screen.** See `DragAnchor` for what that
            // costs when it is wrong.
            let mut anchor = ui
                .data_mut(|d| d.get_temp::<DragAnchor>(anchor_id(response)))
                .unwrap_or(DragAnchor {
                    start: *normalised,
                    travelled: 0.0,
                });
            anchor.travelled += f64::from(delta) / travel;
            *normalised = (anchor.start + anchor.travelled).clamp(0.0, 1.0);
            ui.data_mut(|d| d.insert_temp(anchor_id(response), anchor));
            outcome.changed = true;
        }
    }

    if response.drag_stopped() {
        outcome.gesture_ended = true;
        ui.data_mut(|d| d.remove_temp::<DragAnchor>(anchor_id(response)));
    }

    outcome |= keyboard_edit(ui, response, param, normalised);
    outcome |= wheel_edit(ui, response, normalised, wheel);
    outcome
}

/// A slider is **positional**: the handle goes where the pointer is along the track.
///
/// That is what a slider means, and it is what direct manipulation requires — a fader whose cap
/// lags the pointer reads as broken before it reads as precise. `Shift` switches to a relative
/// fine adjustment, because at that point the user is asking for precision rather than for
/// position, and pixel-accurate pointing is the thing they are struggling with.
fn slider_edit(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    param: &ParamView<'_>,
    normalised: &mut f64,
    wheel: Wheel,
) -> ControlOutcome {
    let mut outcome = ControlOutcome::default();
    if param.read_only {
        return outcome;
    }

    if response.double_clicked() {
        *normalised = param.default.clamp(0.0, 1.0);
        return ControlOutcome::reset_to_default();
    }

    if response.drag_started() {
        outcome.gesture_started = true;
    }

    if response.dragged() || response.clicked() {
        if ui.input(|i| i.modifiers.shift) {
            // Relative, and slowed: the pointer stops being an absolute position and becomes a
            // rate. Horizontal here, matching the track, so the gesture does not change axis
            // halfway through a correction.
            let delta = response.drag_delta().x;
            if delta != 0.0 {
                let travel = f64::from(rect.width()) * FINE_FACTOR;
                *normalised = (*normalised + f64::from(delta) / travel).clamp(0.0, 1.0);
                outcome.changed = true;
            }
        } else if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
            let width = rect.width();
            if width > 0.0 {
                let t = f64::from((pointer.x - rect.left()) / width).clamp(0.0, 1.0);
                if (t - *normalised).abs() > f64::EPSILON {
                    *normalised = t;
                    outcome.changed = true;
                }
            }
        }
    }

    if response.drag_stopped() {
        outcome.gesture_ended = true;
    }

    // A click with no drag is a complete edit on its own, so it brackets itself. Without this a
    // click-to-set would open a gesture the host never sees closed.
    if response.clicked() && outcome.changed {
        outcome.gesture_started = true;
        outcome.gesture_ended = true;
    }

    outcome |= keyboard_edit(ui, response, param, normalised);
    outcome |= wheel_edit(ui, response, normalised, wheel);
    outcome
}

fn vertical_slider_edit(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    param: &ParamView<'_>,
    normalised: &mut f64,
    wheel: Wheel,
) -> ControlOutcome {
    let mut outcome = ControlOutcome::default();
    if param.read_only {
        return outcome;
    }

    if response.double_clicked() {
        *normalised = param.default.clamp(0.0, 1.0);
        return ControlOutcome::reset_to_default();
    }
    if response.drag_started() {
        outcome.gesture_started = true;
    }
    if response.dragged() || response.clicked() {
        if ui.input(|input| input.modifiers.shift) {
            let delta = -response.drag_delta().y;
            if delta != 0.0 {
                let travel = f64::from(rect.height()) * FINE_FACTOR;
                *normalised = (*normalised + f64::from(delta) / travel).clamp(0.0, 1.0);
                outcome.changed = true;
            }
        } else if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
            let height = rect.height();
            if height > 0.0 {
                let t = vertical_slider_value(rect, pointer.y);
                if (t - *normalised).abs() > f64::EPSILON {
                    *normalised = t;
                    outcome.changed = true;
                }
            }
        }
    }
    if response.drag_stopped() {
        outcome.gesture_ended = true;
    }
    if response.clicked() && outcome.changed {
        outcome.gesture_started = true;
        outcome.gesture_ended = true;
    }

    outcome |= keyboard_edit(ui, response, param, normalised);
    outcome |= wheel_edit(ui, response, normalised, wheel);
    outcome
}

fn vertical_slider_value(rect: Rect, pointer_y: f32) -> f64 {
    f64::from((rect.bottom() - pointer_y) / rect.height()).clamp(0.0, 1.0)
}

/// Keyboard operation, design system §11.
///
/// A control that can be selected but not operated is worse than one that cannot be selected at
/// all: it gives the cursor somewhere to land and gives nothing back.
///
/// **A bare arrow sets the value: left and right are fine, up and down are coarse.** The axis
/// orientation comes from the M8; the modifier hierarchy is the owner's live-use correction:
/// `Shift` moves modules/cards, `Command` moves parameters, and no modifier edits the value.
/// Navigation consumes its two tiers before any control is drawn, so one press cannot be spent
/// twice.
///
/// How far a press moves is the parameter's own business — see [`Steps`].
///
/// A control that can be focused but not operated is worse than one that cannot be focused at
/// all: it takes a tab stop and gives nothing back.
fn keyboard_edit(
    ui: &Ui,
    response: &Response,
    param: &ParamView<'_>,
    normalised: &mut f64,
) -> ControlOutcome {
    let gesture_id = response.id.with("mxm-keyboard-gesture");
    // **The held key's anchor: the value this gesture's last press sent.** A held arrow is one
    // gesture across many frames, and the value read back each frame is only what the host has
    // applied so far — CLAP applies an editor's writes on the audio clock, not this one. Chaining
    // a repeat from that readback repeats or drops a step whenever the host is behind; chaining it
    // from what the last press sent does not. This is [`DragAnchor`]'s reasoning for a drag, and
    // like it the anchor lives only as long as its gesture: a new press starts from the readback.
    let anchor = ui.data(|data| data.get_temp::<f64>(gesture_id));
    let active = anchor.is_some();
    if !owns_value_keys(ui, response.has_focus()) {
        if active {
            ui.data_mut(|data| data.remove::<f64>(gesture_id));
            return ControlOutcome {
                gesture_ended: true,
                ..Default::default()
            };
        }
        return ControlOutcome::default();
    }

    // Where no cursor runs — an editor this rollout has not reached — the bare arrows still edit
    // the focused control as they always did, and nothing is taken away before its replacement
    // arrives. See [`crate::navigation::running`].
    let cursor = crate::navigation::running(ui);
    if cursor && crate::pilot::on(ui) {
        return language_edit(ui, gesture_id, param, normalised);
    }

    let (mut presses, mut reset, mut absolute) = (Vec::new(), false, None);
    ui.input_mut(|i| {
        // Under the cursor, the owner's physical hierarchy: unmodified arrows edit the lowest
        // tier, the value, and **`Alt` arrows edit it finer** — `Shift` and `Command` arrows were
        // taken by the cursor before any control was drawn. Without a cursor, the pre-cursor
        // grammar, unchanged: bare arrows, and `Shift` to refine. `matches_logically` ignores an
        // extra `Shift` (and `Alt`), so that one pattern admits both, and each press carries
        // which it was.
        presses = take_arrows(i, |pressed| {
            if cursor {
                pressed == Modifiers::NONE || pressed == Modifiers::ALT
            } else {
                pressed.matches_logically(Modifiers::NONE)
            }
        });
        // The M8's `EDIT` + `OPTION`: back to the default. Double-click already means this.
        if i.consume_key(Modifiers::COMMAND, Key::Backspace) {
            reset = true;
        }
        if i.consume_key(Modifiers::NONE, Key::Home) {
            absolute = Some(0.0);
        }
        if i.consume_key(Modifiers::NONE, Key::End) {
            absolute = Some(1.0);
        }
    });

    if reset || absolute.is_some() {
        *normalised = absolute.unwrap_or(param.default).clamp(0.0, 1.0);
        ui.data_mut(|data| data.remove::<f64>(gesture_id));
        return ControlOutcome {
            gesture_started: !active,
            changed: true,
            gesture_ended: true,
            reset,
        };
    }

    let still_held = ui.input(|input| {
        let arrow_down = [
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::ArrowUp,
            Key::ArrowDown,
        ]
        .into_iter()
        .any(|key| input.key_down(key));
        arrow_down && (!cursor || !(input.modifiers.shift || input.modifiers.command))
    });

    if presses.is_empty() {
        if active && !still_held {
            ui.data_mut(|data| data.remove::<f64>(gesture_id));
            return ControlOutcome {
                gesture_ended: true,
                ..Default::default()
            };
        }
        return ControlOutcome::default();
    }

    // Every press in the frame, in the order it was pressed, each from where the one before it
    // landed. A frame can carry several repeats; all of them apply, inside one host gesture.
    let mut value = anchor.unwrap_or(*normalised).clamp(0.0, 1.0);
    for (key, modifiers) in presses {
        value = if cursor {
            // Left/right is fine and up/down coarse, the M8's axis orientation; `Alt` is the
            // finer layer of both.
            let press = Press {
                up: matches!(key, Key::ArrowRight | Key::ArrowUp),
                coarse: matches!(key, Key::ArrowUp | Key::ArrowDown),
                finer: modifiers.alt,
            };
            step_once(param, value, press)
        } else {
            // Up and right increase, both on the fine axis; `Shift` is a tenth of a fine step.
            let press = Press {
                up: matches!(key, Key::ArrowRight | Key::ArrowUp),
                coarse: false,
                finer: false,
            };
            if modifiers.shift {
                let steps = param.steps;
                let fine = if press.up {
                    steps.fine_up
                } else {
                    -steps.fine_down
                };
                (value + fine * 0.1).clamp(0.0, 1.0)
            } else {
                step_once(param, value, press)
            }
        };
    }

    *normalised = value;
    if still_held {
        ui.data_mut(|data| data.insert_temp(gesture_id, value));
    } else {
        ui.data_mut(|data| data.remove::<f64>(gesture_id));
    }
    ControlOutcome {
        gesture_started: !active,
        changed: true,
        gesture_ended: !still_held,
        reset: false,
    }
}

/// The keyboard language's value keys, where the pilot runs it: VALUE's presses, each from where
/// the one before landed, as one gesture until it is kept or cancelled; DELETE the default, and
/// Home and End the ends, as before.
fn language_edit(
    ui: &Ui,
    gesture_id: egui::Id,
    param: &ParamView<'_>,
    normalised: &mut f64,
) -> ControlOutcome {
    let origin_id = gesture_id.with("origin");
    let anchor = ui.data(|data| data.get_temp::<f64>(gesture_id));
    let active = anchor.is_some();
    let keys = crate::navigation::take_value_keys(ui.ctx());
    let absolute = ui.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::Home) {
            Some(0.0)
        } else if input.consume_key(Modifiers::NONE, Key::End) {
            Some(1.0)
        } else {
            None
        }
    });
    let forget = |ui: &Ui| {
        ui.data_mut(|data| {
            data.remove::<f64>(gesture_id);
            data.remove::<f64>(origin_id);
        });
    };
    if keys.reset || absolute.is_some() {
        *normalised = absolute.unwrap_or(param.default).clamp(0.0, 1.0);
        forget(ui);
        return ControlOutcome {
            gesture_started: !active,
            changed: true,
            gesture_ended: true,
            reset: keys.reset,
        };
    }
    if keys.cancel {
        let origin = ui.data(|data| data.get_temp::<f64>(origin_id));
        forget(ui);
        return match origin {
            Some(origin) => {
                *normalised = origin;
                ControlOutcome {
                    gesture_started: false,
                    changed: true,
                    gesture_ended: true,
                    reset: false,
                }
            }
            None => ControlOutcome::default(),
        };
    }
    if keys.presses.is_empty() {
        if keys.keep && active {
            forget(ui);
            return ControlOutcome {
                gesture_ended: true,
                ..Default::default()
            };
        }
        return ControlOutcome::default();
    }
    if !active {
        let origin = *normalised;
        ui.data_mut(|data| data.insert_temp(origin_id, origin));
    }
    let mut value = anchor.unwrap_or(*normalised).clamp(0.0, 1.0);
    for press in keys.presses {
        value = step_once(param, value, press);
    }
    *normalised = value;
    if keys.keep {
        forget(ui);
    } else {
        ui.data_mut(|data| data.insert_temp(gesture_id, value));
    }
    ControlOutcome {
        gesture_started: !active,
        changed: true,
        gesture_ended: keys.keep,
        reset: false,
    }
}

/// Where one press lands from `value`: the owner's law when it gave one, its fixed [`Steps`] when
/// it did not — under `Alt`, fine for up/down and a tenth of fine for left/right.
fn step_once(param: &ParamView<'_>, value: f64, press: Press) -> f64 {
    if let Some(Next(law)) = param.next {
        return law.next_value(value, press).clamp(0.0, 1.0);
    }
    let steps = param.steps;
    let (up, down) = match (press.coarse, press.finer) {
        (true, false) => (steps.coarse_up, steps.coarse_down),
        (false, false) | (true, true) => (steps.fine_up, steps.fine_down),
        (false, true) => (steps.fine_up / 10.0, steps.fine_down / 10.0),
    };
    (value + if press.up { up } else { -down }).clamp(0.0, 1.0)
}

/// The wheel, under §7.1's condition: focus **or** a held modifier, never hover alone.
///
/// Hover alone is the case that turns scrolling a parameter list into editing it, silently.
fn wheel_edit(ui: &Ui, response: &Response, normalised: &mut f64, wheel: Wheel) -> ControlOutcome {
    if wheel != Wheel::FocusOrModifier || !response.hovered() {
        return ControlOutcome::default();
    }

    let (delta, shifted) = ui.input(|i| (i.smooth_scroll_delta.y, i.modifiers.shift));
    if delta == 0.0 || !(response.has_focus() || shifted) {
        return ControlOutcome::default();
    }

    *normalised = (*normalised + f64::from(delta) / f64::from(DRAG_TRAVEL)).clamp(0.0, 1.0);
    // A wheel notch is a discrete edit with no press or release to bracket it, so it brackets
    // itself. Left open, a host's automation lane would stay latched.
    ControlOutcome::instant()
}

/// The tooltip §7.1 requires: full name, exact value, one sentence.
fn tooltip(response: Response, param: &ParamView<'_>) -> Response {
    response.on_hover_text(format!(
        "{}\n{}\n{}",
        param.name, param.text, param.description
    ))
}

/// Direct text entry. §7.1 requires it on every continuous control.
fn text_entry_field(ui: &mut Ui, buffer: &mut String, width: f32) -> ControlOutcome {
    let response = ui.add_sized(
        Vec2::new(width, MIN_TARGET),
        egui::TextEdit::singleline(buffer),
    );
    if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        return ControlOutcome::instant();
    }
    ControlOutcome::default()
}

/// The value under a knob: **always one line, whatever it says.**
///
/// It used to be laid out in a box the knob's width and allowed to wrap, so a cutoff of `1.2 kHz`
/// was one line and `20.0 kHz` was two — and the card grew a row while you turned the knob. That is
/// the interface moving under the pointer, which this crate exists partly to prevent.
///
/// So the box is one line tall and the text does not wrap. It is the **column's** width, not the
/// knob's, which is enough for every value these parameters produce; the truncation is a backstop.
/// Extending instead was the first attempt and it moved the defect to the other axis — the card grew
/// *sideways* as the knob turned.
fn value_label(ui: &mut Ui, tokens: &Tokens, param: &ParamView<'_>, width: f32) {
    let colour = if param.read_only {
        tokens.text_disabled
    } else {
        tokens.text_primary
    };
    let style = value_style(ui.style());
    fixed_label(ui, param.text, width, 1, style, Some(colour));
}

/// The width of the longest single word in `text`, laid out in `style`.
///
/// **Nothing here may ever break a word.** `break_anywhere` is false throughout this crate, which
/// makes egui *prefer* a word boundary — and its fallback, when no word fits the line, is to break
/// mid-word anyway. `Resonance` in a 48 px box came out as "Resonan" over "ce". Preference is not a
/// guarantee; width is. So a box is never narrower than its longest word, and text that will not fit
/// overflows visibly instead of being mangled unreadably.
fn longest_word(ui: &Ui, text: &str, style: &egui::TextStyle) -> f32 {
    let font = style.resolve(ui.style());
    text.split_whitespace()
        .map(|word| {
            ui.ctx().fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(word.to_owned(), font.clone(), Color32::PLACEHOLDER)
                    .rect
                    .width()
            })
        })
        .fold(0.0, f32::max)
}

/// A label in a box of exactly `lines` lines, centred, whose height cannot depend on its content.
///
/// Returns the label's `Response` so a control can still be `labelled_by` it — a painted string
/// would have cost the accessible name, which is never worth a layout fix.
fn fixed_label(
    ui: &mut Ui,
    text: &str,
    width: f32,
    lines: usize,
    style: egui::TextStyle,
    colour: Option<Color32>,
) -> Response {
    let height = ui.text_style_height(&style) * lines as f32;
    let width = width.max(longest_word(ui, text, &style));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());

    let mut rich = egui::RichText::new(text).text_style(style);
    if let Some(colour) = colour {
        rich = rich.color(colour);
    }

    let label = egui::Label::new(rich).halign(egui::Align::Center);
    // One line extends into the spacing beside it; more than one wraps inside the box it was given.
    // **Truncate, not extend.** Extending kept the height fixed and moved the defect to the other
    // axis: a long value pushed `min_rect` out and the whole card grew sideways as the knob turned.
    // A box that is the column's width fits every value these parameters produce, so the ellipsis
    // is a backstop and not the normal case.
    let label = if lines == 1 {
        label.truncate()
    } else {
        label.wrap()
    };

    // **Not `Ui::put`**, which lays its widget out `centered_and_justified` — and a *justified*
    // wrapped label stretches every line but the last to fill the width, so "Pulse width" came out
    // as "P u l s e" over "width". Justification is for paragraphs, not for a two-word name.
    //
    // Bottom-up for a name, so a one-line name sits against its knob instead of floating in the
    // middle of a box sized for two.
    let layout = if lines == 1 {
        egui::Layout::top_down(egui::Align::Center)
    } else {
        egui::Layout::bottom_up(egui::Align::Center)
    };
    if lines == 1 {
        return ui
            .scope_builder(egui::UiBuilder::new().max_rect(rect).layout(layout), |ui| {
                ui.add(label)
            })
            .inner;
    }
    // **Half a point of slack each side for a wrapped name.** The box is at least the name's
    // longest word, and a knob at its narrowest is exactly that wide; egui then broke the word
    // itself on a hair of floating point — *envelope* over three lines above a card's knob row. The
    // name is centred, so a word that fits the box still lands inside it.
    //
    // **In a child that does not grow this `Ui`**, because the slack is room for egui's wrap
    // decision and nothing more: the box above is the name's allocation. A scope would add what the
    // child drew to this `Ui`'s extent, and a knob measured half a point wider than the tree says
    // it is (`tree::tests::a_knob_is_the_size_it_says_in_every_column`). `new_child` takes the
    // same ids a scope would and advances this `Ui`'s the same way.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.expand2(Vec2::new(0.5, 0.0)))
            .layout(layout),
    );
    child.add(label)
}

fn value_text(ui: &mut Ui, tokens: &Tokens, param: &ParamView<'_>) {
    let colour = if param.read_only {
        tokens.text_disabled
    } else {
        tokens.text_primary
    };
    let style = value_style(ui.style());
    ui.label(
        egui::RichText::new(param.text)
            .text_style(style)
            .color(colour),
    );
}

/// One cell's width, so every cell in a control is the same: **the widest option's floor, and no
/// wider** — or the shared width of a stack, when the control stands in one.
///
/// # As small as the widest option allows (the owner, 2026-09-23)
///
/// Cells used to stretch to share whatever width the row was handed, so a three-option switch drew
/// across a whole card and read as the card's most important control, and several plugins grew width
/// caps to tame it. The owner: *"Make buttons the same size as the largest one must be, but as small
/// as possible."* A control is now as wide as its content needs, and controls that stand together
/// share the widest one's width ([`shared_cell_width`]).
///
/// # Two floors, and the pointer minimum was never the binding one
///
/// §11's pointer minimum is a floor: an unhittable segment is worse than an untidy row. But it is
/// **32 points, and a word is usually wider than that**, so flooring there alone let a narrow card
/// paint *Envelope* straight across its neighbours — the cell text is painted centred at
/// `rect.center()` and is not clipped, so nothing stopped it and nothing measured it either. Seen
/// in `mxm-mono-02`'s VCA, filter-envelope and PWM-source rows, in a reflowing layout; the fixed
/// editors never showed it because their cards are always wide enough.
///
/// So the real floor is **the widest option's own text**, plus the padding a word needs not to
/// touch the cell edge. A control whose options cannot fit then **overflows its card** rather than
/// painting outside itself, which is the right failure: it is visible, it is measurable, and the
/// caller's card is the thing that should be wider.
fn segment_width(ui: &Ui, options: &[&str], shared: Option<f32>) -> f32 {
    let own = segment_cell_width(ui, options);
    shared
        .or_else(|| scoped_floor(ui, CELL_FLOOR))
        .map_or(own, |shared| shared.max(own))
}

/// The scoped floors [`segmented_stack`] and [`toggle_stack`] set while their controls draw.
const CELL_FLOOR: &str = "mxm-ui-stack-cell";
const TOGGLE_FLOOR: &str = "mxm-ui-stack-toggle";

fn scoped_floor(ui: &Ui, key: &'static str) -> Option<f32> {
    ui.data(|data| data.get_temp::<f32>(egui::Id::new(key)))
}

fn with_scoped_floor<R>(
    ui: &mut Ui,
    key: &'static str,
    floor: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let id = egui::Id::new(key);
    let previous = ui.data(|data| data.get_temp::<f32>(id));
    ui.data_mut(|data| data.insert_temp(id, floor));
    let result = add(ui);
    ui.data_mut(|data| match previous {
        Some(previous) => {
            data.insert_temp(id, previous);
        }
        None => {
            data.remove::<f32>(id);
        }
    });
    result
}

/// Draws a **stack of segmented controls** that share one cell width, `cell` — normally
/// [`shared_cell_width`] over the stack's option lists. Every segmented control drawn inside `add`
/// takes at least that cell, so the stack is even rather than ragged, without each caller's binding
/// having to carry the width through.
pub fn segmented_stack<R>(ui: &mut Ui, cell: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    with_scoped_floor(ui, CELL_FLOOR, cell, add)
}

/// Draws a **stack of toggles** that share one width, `width` — normally [`shared_toggle_width`]
/// over their labels, or a pair's own measure where a toggle stands beside a button. Every toggle
/// drawn inside `add` is at least that wide.
pub fn toggle_stack<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    with_scoped_floor(ui, TOGGLE_FLOOR, width, add)
}

/// The width every cell of a segmented control takes: the widest option's text with `SPACE_3`
/// either side, floored at §11's pointer minimum.
#[must_use]
pub fn segment_cell_width(ui: &Ui, options: &[&str]) -> f32 {
    segment_text_floor(ui, options).max(MIN_TARGET)
}

/// One cell width for several segmented controls that stand together — a stack of selectors — so
/// every cell in the stack is the same width: the widest any of them needs.
#[must_use]
pub fn shared_cell_width(ui: &Ui, controls: &[&[&str]]) -> f32 {
    controls
        .iter()
        .map(|options| segment_cell_width(ui, options))
        .fold(MIN_TARGET, f32::max)
}

/// The narrowest a cell may be and still hold the longest option, with `SPACE_3` either side.
///
/// Public so a caller that must know its own minimum width can ask, rather than measuring a render
/// or guessing — which is what a reflowing layout needs in order to declare a card's floor.
#[must_use]
pub fn segment_min_width(ui: &Ui, options: &[&str]) -> f32 {
    let cell = segment_cell_width(ui, options);
    cell * options.len() as f32 + HAIRLINE * (options.len() as f32 - 1.0)
}

/// A [`toggle`]'s width: its label with `SPACE_3` either side, floored at §11's pointer minimum —
/// **as small as the label allows, and not a point wider** (the owner, 2026-09-23). It does not
/// depend on `marked`: the modulation dot sits in the corner padding, so a toggle never changes
/// size when something starts modulating it.
///
/// Public for the same reason as [`segment_min_width`]: a layout that places a toggle in a column
/// must be able to declare the column's floor instead of guessing.
#[must_use]
pub fn toggle_min_width(ui: &Ui, label: &str) -> f32 {
    let style = egui::TextStyle::Button.resolve(ui.style());
    let text = ui
        .painter()
        .layout_no_wrap(label.to_owned(), style, egui::Color32::WHITE)
        .size()
        .x;
    (SPACE_3 + text + SPACE_3).max(MIN_TARGET)
}

/// One width for toggles that stand together — a stack of switches — so they are all as wide as the
/// widest label needs and no wider.
#[must_use]
pub fn shared_toggle_width(ui: &Ui, labels: &[&str]) -> f32 {
    labels
        .iter()
        .map(|label| toggle_min_width(ui, label))
        .fold(MIN_TARGET, f32::max)
}

// ---------------------------------------------------------------------------------------------
// Sizes without drawing (`crate::tree`, plans/plan-layout-tree.md in the private archive).
//
// Each function answers, from the fonts and the style alone, what the control beside it occupies
// when drawn: the narrowest it may be, and its height. They sit here rather than in the tree so that
// a change to a control's geometry and to its size are one edit in one place, and `tree`'s tests
// draw each control and compare, so the two cannot drift silently.
//
// A size read from `ui.spacing()` is read from the `Ui` the control will be drawn in, which for a
// card body is `shell::body_ui` — the card body's rhythm, not the panel's.
// ---------------------------------------------------------------------------------------------

/// A [`knob`]'s narrowest column and its height.
///
/// `column` is the least column the caller draws it in — `0.0` in the collection's knob row
/// (`tree::knob_row`), which sizes the columns itself. The knob is never narrower than its circle,
/// and [`fixed_label`] never lets a box be narrower than a word. **Where the tier shows the value, the
/// column holds the widest reading whole**: the value is one line and truncated where it does not
/// fit, so a column holding only its longest word printed `0.060 s/o…` at a card's floor — the
/// reason three editors had worked out their own reading columns. The height is the fixed two-line
/// name box, the circle's band, and the value line when the tier shows one, each `SPACE_2` apart.
#[must_use]
pub fn knob_size(ui: &Ui, name: &str, widest: &str, size: Size, column: f32) -> Vec2 {
    let value = value_style(ui.style());
    let reading = if size.value_always_visible() {
        text_width(ui, widest, value.clone())
    } else {
        longest_word(ui, widest, &value)
    };
    let width = column
        .max(size.diameter().max(MIN_TARGET))
        .max(longest_word(ui, name, &egui::TextStyle::Body))
        .max(reading);
    let mut height = name_box_height(ui) + SPACE_2 + size.diameter().max(MIN_TARGET);
    if size.value_always_visible() {
        height += SPACE_2 + ui.text_style_height(&value);
    }
    Vec2::new(width, height)
}

/// How far a control is pushed down to stand centred on a knob's circle in a top-aligned row:
/// the name box, `SPACE_2`, and half the difference between the circle's band and a pointer
/// target.
#[must_use]
pub fn knob_line_drop(ui: &Ui, size: Size) -> f32 {
    name_box_height(ui) + SPACE_2 + (size.diameter().max(MIN_TARGET) - MIN_TARGET) / 2.0
}

/// How far a control is pushed down to stand on a labelled segmented control's cell line: the
/// label's Body line and `SPACE_2`.
#[must_use]
pub fn cell_line_drop(ui: &Ui) -> f32 {
    label_line(ui) + SPACE_2
}

/// A one-line `ui.label` in the Body style, as tall as egui lays it out — which is not the font's row
/// height (16.0 against 15.72 in Inter's Body size).
fn label_line(ui: &Ui) -> f32 {
    ui.painter()
        .layout_no_wrap(
            "A".to_owned(),
            egui::TextStyle::Body.resolve(ui.style()),
            Color32::PLACEHOLDER,
        )
        .size()
        .y
}

/// A [`toggle`]'s size at its own width, or at `shared` when it stands in a stack of toggles.
#[must_use]
pub fn toggle_size(ui: &Ui, label: &str, shared: f32) -> Vec2 {
    Vec2::new(toggle_min_width(ui, label).max(shared), MIN_TARGET)
}

/// A [`toggle_compact`]'s size: the pointer square.
#[must_use]
pub fn toggle_compact_size() -> Vec2 {
    Vec2::splat(MIN_TARGET)
}

/// A [`toggle_wave`]'s size: one wave cell.
#[must_use]
pub fn toggle_wave_size() -> Vec2 {
    wave_cell()
}

/// A [`toggle_wave`]'s size for `wave`: one wave cell, except the tempo sync's quarter note, which
/// is a symbol rather than a waveform and is **square at the cell's height** (the owner, 2026-09-25:
/// *"the button itself should be square. Keep the current height"*).
#[must_use]
pub fn toggle_wave_size_of(wave: Wave) -> Vec2 {
    match wave {
        Wave::QuarterNote => Vec2::splat(wave_cell().y),
        _ => wave_cell(),
    }
}

/// What [`beside_a_knob`] adds above a control's content: its label line, the drop onto a knob's
/// name line when beside one, and the lift that centres `content_height` on the circle.
fn label_block(ui: &Ui, beside: Option<Size>, content_height: f32) -> f32 {
    // The label is a `ui.label`; the drop onto a knob's name line is `add_space` of row heights.
    let mut height = label_line(ui) + SPACE_2;
    if let Some(beside) = beside {
        height += ui.text_style_height(&egui::TextStyle::Body) * (NAME_LINES - 1) as f32;
        let lift = (beside.diameter().max(MIN_TARGET) - content_height) / 2.0;
        if lift > 0.0 {
            height += lift;
        }
    }
    height
}

/// A [`segmented`] control's size — its painted label on the line above the cells, or on a knob's
/// name line when `beside` one — with every cell at least `shared` wide when it stands in a stack
/// whose cells are shared ([`segmented_stack`]).
#[must_use]
pub fn segmented_size(
    ui: &Ui,
    label: &str,
    options: &[&str],
    beside: Option<Size>,
    shared: f32,
) -> Vec2 {
    let cell = segment_cell_width(ui, options).max(shared);
    let cells = cell * options.len() as f32 + HAIRLINE * (options.len() as f32 - 1.0);
    Vec2::new(
        text_width(ui, label, egui::TextStyle::Body).max(cells),
        label_block(ui, beside, MIN_TARGET) + MIN_TARGET,
    )
}

/// The cell a segmented control's options need on their own, for a stack that shares one.
#[must_use]
pub fn segmented_cell(ui: &Ui, options: &[&str]) -> f32 {
    segment_cell_width(ui, options)
}

/// A [`segmented_waves`] grid's size: rows of at most five pictures, each widened by the widest
/// mark when marked, `SPACE_2` between cells and between rows; with its painted label above, on a
/// knob's name line when `beside` one, or none.
#[must_use]
pub fn waves_size(
    ui: &Ui,
    label: Option<&str>,
    count: usize,
    marks: &[&str],
    beside: Option<Size>,
) -> Vec2 {
    let columns = wave_columns(count);
    let rows = count.div_ceil(columns);
    let mark = if marks.is_empty() {
        0.0
    } else {
        let font = crate::typography::caption_style(ui.style());
        marks
            .iter()
            .map(|m| text_width(ui, m, font.clone()))
            .fold(0.0, f32::max)
            + SPACE_2
    };
    let cell = wave_cell() + Vec2::new(mark, 0.0);
    let cells = Vec2::new(
        cell.x * columns as f32 + SPACE_2 * (columns as f32 - 1.0),
        cell.y * rows as f32 + SPACE_2 * (rows as f32 - 1.0),
    );
    match label {
        Some(label) => Vec2::new(
            text_width(ui, label, egui::TextStyle::Body).max(cells.x),
            label_block(ui, beside, cells.y) + cells.y,
        ),
        None => cells,
    }
}

/// A [`selector`]'s narrowest size: its name in the label style, the row's spacing, and a caret
/// button around the widest option in the value style. Beyond that it takes whatever width it is
/// given — the button is laid out right-to-left across the rest of its row.
#[must_use]
pub fn selector_size(ui: &Ui, label: &str, options: &[&str]) -> Vec2 {
    selector_size_in(
        ui,
        label_style(ui.style()),
        value_style(ui.style()),
        label,
        options,
    )
}

/// A [`selector_caption`]'s narrowest size: [`selector_size`] with both name and value in the
/// caption style.
#[must_use]
pub fn selector_caption_size(ui: &Ui, label: &str, options: &[&str]) -> Vec2 {
    let caption = crate::typography::caption_style(ui.style());
    selector_size_in(ui, caption.clone(), caption, label, options)
}

fn selector_size_in(
    ui: &Ui,
    name: egui::TextStyle,
    value: egui::TextStyle,
    label: &str,
    options: &[&str],
) -> Vec2 {
    let widest = options
        .iter()
        .map(|option| text_width(ui, option, value.clone()))
        .fold(0.0, f32::max);
    let button = (widest + 2.0 * CARET_GUTTER).max(MIN_TARGET);
    Vec2::new(
        text_width(ui, label, name) + ui.spacing().item_spacing.x + button,
        MIN_TARGET,
    )
}

/// A stacked [`slider`]'s narrowest size: its name line, then `SPACE_1` and the track. `quiet` is
/// [`ParamView::quiet_label`]: the name in the caption style.
///
/// **The reading never draws over the name** (the owner, 2026-09-24), so the line is the mark
/// slot, the name and the widest reading with the row's spacing between them.
#[must_use]
pub fn slider_size(ui: &Ui, label: &str, quiet: bool, widest: &str) -> Vec2 {
    let gap = ui.spacing().item_spacing.x;
    let name = if quiet {
        crate::typography::caption_style(ui.style())
    } else {
        egui::TextStyle::Body
    };
    let named = SPACE_2 + gap + text_width(ui, label, name.clone());
    let reading = text_width(ui, widest, value_style(ui.style()));
    // The reading's right-to-left region starts one item spacing after the name.
    let line = named + gap + reading;
    // `ui.horizontal` starts a row `interact_size.y` tall, so the name line is never shorter.
    let name_row = ui
        .spacing()
        .interact_size
        .y
        .max(ui.text_style_height(&name))
        .max(ui.text_style_height(&value_style(ui.style())));
    Vec2::new(line.max(MIN_TARGET), name_row + SPACE_1 + MIN_TARGET)
}

/// A [`slider_vertical`]'s narrowest width at the caller's `height`: the pointer target, or a word
/// of its name or of its widest reading if either is wider.
#[must_use]
pub fn slider_vertical_size(ui: &Ui, label: &str, widest: &str, height: f32) -> Vec2 {
    // **The whole reading, not its longest word**: the value is one line under the track, and a
    // fader in a row is as wide as it — *300.0 ms*, not *300.0* and an ellipsis. A knob's column
    // holds its reading the same way.
    let width = MIN_TARGET
        .max(longest_word(ui, label, &egui::TextStyle::Body))
        .max(text_width(ui, widest, value_style(ui.style())));
    Vec2::new(width, height)
}

/// A [`remove_mark`]'s size: its square, allocated whether or not it is shown.
#[must_use]
pub fn remove_mark_size(size: f32) -> Vec2 {
    Vec2::splat(size)
}

// Plain egui widgets, as fx-convolution and fx-curve use them unwrapped. They stay egui's own —
// replacing them would be a redesign — and their sizes are egui's arithmetic, held by `tree`'s
// draw-and-compare tests like every other size here.

/// An `egui::Button` with `label` in the button style, at least `min` (`Button::min_size`).
#[must_use]
pub fn button_size(ui: &Ui, label: &str, min: Vec2) -> Vec2 {
    let padding = ui.spacing().button_padding;
    let text = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            egui::TextStyle::Button.resolve(ui.style()),
            Color32::PLACEHOLDER,
        )
        .size();
    Vec2::new(
        (text.x + 2.0 * padding.x).max(min.x),
        (text.y + 2.0 * padding.y)
            .max(ui.spacing().interact_size.y)
            .max(min.y),
    )
}

/// `ui.checkbox(_, label)`: the icon, its spacing and the label, one interact row tall.
#[must_use]
pub fn checkbox_size(ui: &Ui, label: &str) -> Vec2 {
    let spacing = ui.spacing();
    let text = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            egui::TextStyle::Button.resolve(ui.style()),
            Color32::PLACEHOLDER,
        )
        .size();
    Vec2::new(
        spacing.icon_width + spacing.icon_spacing + text.x,
        text.y.max(spacing.interact_size.y),
    )
}

/// An `egui::ComboBox` showing `selected`, at egui's own default width.
#[must_use]
pub fn combo_size(ui: &Ui, selected: &str) -> Vec2 {
    let spacing = ui.spacing();
    let text = ui
        .painter()
        .layout_no_wrap(
            selected.to_owned(),
            egui::TextStyle::Button.resolve(ui.style()),
            Color32::PLACEHOLDER,
        )
        .size();
    // egui's own sum: the text, `icon_spacing`, the arrow; at least `combo_width` with its padding.
    let content = (text.x + spacing.icon_spacing + spacing.icon_width)
        .max(spacing.combo_width - 2.0 * spacing.button_padding.x);
    Vec2::new(
        content + 2.0 * spacing.button_padding.x,
        (text.y + 2.0 * spacing.button_padding.y).max(spacing.interact_size.y),
    )
}

/// An `egui::Slider` with its value box sized for `widest` (suffix included) and an optional
/// `label` beside it: the rail, the value box and the label, `item_spacing.x` apart.
#[must_use]
pub fn egui_slider_size(ui: &Ui, widest: &str, label: Option<&str>) -> Vec2 {
    let spacing = ui.spacing();
    let button = egui::TextStyle::Button.resolve(ui.style());
    let value = ui
        .painter()
        .layout_no_wrap(widest.to_owned(), button.clone(), Color32::PLACEHOLDER)
        .size();
    let value_box = (value.x + 2.0 * spacing.button_padding.x).max(spacing.interact_size.x);
    let mut width = spacing.slider_width + spacing.item_spacing.x + value_box;
    if let Some(label) = label {
        width += spacing.item_spacing.x
            + ui.painter()
                .layout_no_wrap(
                    label.to_owned(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    Color32::PLACEHOLDER,
                )
                .size()
                .x;
    }
    Vec2::new(
        width,
        spacing
            .interact_size
            .y
            .max(value.y + 2.0 * spacing.button_padding.y),
    )
}

fn segment_text_floor(ui: &Ui, options: &[&str]) -> f32 {
    let style = egui::TextStyle::Button.resolve(ui.style());
    options
        .iter()
        .map(|option| {
            ui.painter()
                .layout_no_wrap((*option).to_owned(), style.clone(), egui::Color32::WHITE)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
        + 2.0 * SPACE_3
}

// ---------------------------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------------------------

fn paint_knob(
    ui: &Ui,
    tokens: &Tokens,
    rect: Rect,
    diameter: f32,
    normalised: f64,
    param: &ParamView<'_>,
    response: &Response,
) {
    let painter = ui.painter();
    let centre = rect.center();
    let radius = diameter * 0.5 - 3.0;
    let width = (diameter * 0.09).max(2.0);

    // §7.1 asks for a visible hover state *and* a visible active state, and they must differ:
    // "the pointer is over this" and "I am changing this" are different facts. Hover brightens the
    // arc; dragging also lifts the track, so the control reads as engaged from across the panel.
    let (track, arc, marker) = if param.read_only {
        (tokens.track, tokens.text_disabled, tokens.text_disabled)
    } else if response.dragged() {
        (tokens.surface_3, tokens.accent_hover, tokens.text_primary)
    } else if response.hovered() {
        (tokens.track, tokens.accent_hover, tokens.text_primary)
    } else {
        (tokens.track, tokens.accent, tokens.text_primary)
    };

    // The neutral track: the full 270°, always visible, so the control reads as a control before
    // anyone touches it. That was the original defect this whole styling effort started from.
    painter.add(Shape::line(
        arc_points(centre, radius, 0.0, 1.0),
        Stroke::new(width, track),
    ));

    // The value arc. Bipolar parameters grow from the centre, so "no modulation" is a visibly
    // empty arc rather than a half-full one.
    let (from, to) = if param.bipolar {
        let centre_t = 0.5;
        if normalised >= centre_t {
            (centre_t as f32, normalised as f32)
        } else {
            (normalised as f32, centre_t as f32)
        }
    } else {
        (0.0, normalised as f32)
    };
    if (to - from).abs() > f32::EPSILON {
        painter.add(Shape::line(
            arc_points(centre, radius, from, to),
            Stroke::new(width, arc),
        ));
    }

    // **The modulation arc: from where the knob is to where something else is taking it.** Inside
    // the value arc's radius, in the accent at reduced alpha, so it reads as an annotation over the
    // control rather than a second value. This is what makes "the knob returns but the step still
    // deviates" legible: the marker is yours, the arc is the sequencer's.
    if param.modulation != 0.0 {
        let target = (normalised + param.modulation).clamp(0.0, 1.0) as f32;
        let (m_from, m_to) = if target >= normalised as f32 {
            (normalised as f32, target)
        } else {
            (target, normalised as f32)
        };
        if (m_to - m_from) > f32::EPSILON {
            painter.add(Shape::line(
                arc_points(centre, radius - width * 1.4, m_from, m_to),
                Stroke::new(width * 0.7, tokens.accent.gamma_multiply(0.6)),
            ));
        }
    }

    // A bipolar control gets a visible detent at the centre, so the zero it snaps to is findable
    // without dragging to look for it.
    if param.bipolar {
        let angle = ARC_START + ARC_SWEEP * 0.5;
        let inner = centre + Vec2::angled(angle) * (radius - width);
        let outer = centre + Vec2::angled(angle) * (radius + width * 0.5);
        painter.line_segment([inner, outer], Stroke::new(HAIRLINE, tokens.border_strong));
    }

    // The radial marker: a plain line, no shadow, no cap, no bevel.
    let angle = ARC_START + ARC_SWEEP * normalised as f32;
    let marker_inner = centre + Vec2::angled(angle) * (radius * 0.35);
    let marker_outer = centre + Vec2::angled(angle) * (radius - width * 0.5);
    painter.line_segment(
        [marker_inner, marker_outer],
        Stroke::new(width * 0.6, marker),
    );

    // Joins the keyboard cursor's registry, if a card and a parameter scope are open around it.
    // Beside the focus ring because the two are the same fact seen from two sides: this is the
    // control the keyboard can reach, and that is what it looks like when it has.
    crate::navigation::mark(ui, response, rect);

    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }
}

fn paint_slider(
    ui: &Ui,
    tokens: &Tokens,
    rect: Rect,
    normalised: f64,
    param: &ParamView<'_>,
    response: &Response,
) {
    let painter = ui.painter();
    let track_height = 6.0;
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), track_height));

    let (track_colour, fill, handle) = if param.read_only {
        (tokens.surface_2, tokens.text_disabled, tokens.text_disabled)
    } else if response.hovered() || response.dragged() {
        (tokens.surface_2, tokens.accent_hover, tokens.text_primary)
    } else {
        (tokens.surface_2, tokens.accent, tokens.text_primary)
    };

    painter.rect_filled(track, track_height * 0.5, track_colour);
    painter.rect_stroke(
        track,
        track_height * 0.5,
        Stroke::new(HAIRLINE, tokens.border),
        StrokeKind::Inside,
    );

    let x = |t: f64| rect.left() + rect.width() * t as f32;
    let (from, to) = if param.bipolar {
        if normalised >= 0.5 {
            (0.5, normalised)
        } else {
            (normalised, 0.5)
        }
    } else {
        (0.0, normalised)
    };
    if to > from {
        let filled = Rect::from_min_max(
            Pos2::new(x(from), track.top()),
            Pos2::new(x(to), track.bottom()),
        );
        painter.rect_filled(filled, track_height * 0.5, fill);
    }

    // **The delta line: from where the handle is to where something else is taking it.** The same
    // fact the knob's modulation arc carries, in the slider's geometry — below the track so it reads
    // as an annotation, in the accent at reduced alpha. Without it a slider whose sound moves while
    // its handle stays put is a slider that appears to be ignoring the sequence.
    if param.modulation != 0.0 {
        let target = (normalised + param.modulation).clamp(0.0, 1.0);
        let y = track.bottom() + 3.0;
        painter.line_segment(
            [Pos2::new(x(normalised), y), Pos2::new(x(target), y)],
            Stroke::new(2.0, tokens.accent.gamma_multiply(0.6)),
        );
    }

    // A fixed-size handle. It does not grow on hover — see this module's header.
    let handle_rect = Rect::from_center_size(
        Pos2::new(x(normalised), rect.center().y),
        Vec2::new(8.0, MIN_TARGET * 0.6),
    );
    painter.rect_filled(handle_rect, RADIUS as f32, handle);

    // Joins the keyboard cursor's registry, if a card and a parameter scope are open around it.
    // Beside the focus ring because the two are the same fact seen from two sides: this is the
    // control the keyboard can reach, and that is what it looks like when it has.
    crate::navigation::mark(ui, response, rect);

    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }
}

fn paint_vertical_slider(
    ui: &Ui,
    tokens: &Tokens,
    rect: Rect,
    normalised: f64,
    param: &ParamView<'_>,
    response: &Response,
) {
    let painter = ui.painter();
    let track_width = 6.0;
    let track = Rect::from_center_size(rect.center(), Vec2::new(track_width, rect.height()));
    let (track_colour, fill, handle) = if param.read_only {
        (tokens.surface_2, tokens.text_disabled, tokens.text_disabled)
    } else if response.hovered() || response.dragged() {
        (tokens.surface_2, tokens.accent_hover, tokens.text_primary)
    } else {
        (tokens.surface_2, tokens.accent, tokens.text_primary)
    };

    painter.rect_filled(track, track_width * 0.5, track_colour);
    painter.rect_stroke(
        track,
        track_width * 0.5,
        Stroke::new(HAIRLINE, tokens.border),
        StrokeKind::Inside,
    );

    let y = |t: f64| rect.bottom() - rect.height() * t as f32;
    let (from, to) = if param.bipolar {
        if normalised >= 0.5 {
            (0.5, normalised)
        } else {
            (normalised, 0.5)
        }
    } else {
        (0.0, normalised)
    };
    if to > from {
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(track.left(), y(to)),
                Pos2::new(track.right(), y(from)),
            ),
            track_width * 0.5,
            fill,
        );
    }

    if param.modulation != 0.0 {
        let target = (normalised + param.modulation).clamp(0.0, 1.0);
        let x = track.right() + 3.0;
        painter.line_segment(
            [Pos2::new(x, y(normalised)), Pos2::new(x, y(target))],
            Stroke::new(2.0, tokens.accent.gamma_multiply(0.6)),
        );
    }

    let handle_rect = Rect::from_center_size(
        Pos2::new(rect.center().x, y(normalised)),
        Vec2::new(MIN_TARGET * 0.6, 8.0),
    );
    painter.rect_filled(handle_rect, RADIUS as f32, handle);
    crate::navigation::mark(ui, response, rect);
    if response.has_focus() {
        focus_ring(ui, tokens, rect);
    }
}

/// The keyboard focus ring, §11. Drawn **outside** the control's rect so it cannot be mistaken for
/// a value change, and so it never alters the control's own geometry.
///
/// **Shown to whoever is using the keyboard, not to everybody.** `plans/plan-keyboard-editing.md`
/// §3 (in the private archive) makes this ring the parameter half of the keyboard cursor — one
/// visual with the card outline — so it follows the same reveal rule: hidden on a pointer press,
/// back on the next keyboard gesture. See [`crate::navigation::shown`] for what the owner reported
/// and why position and paint had to become two facts.
///
/// The gate is `running`-conditional because the rule belongs to the cursor. Where none runs — the
/// player, the cardless developer Parameters surface — `Tab` is the only way focus arrives at all,
/// and the ring behaves exactly as it always has.
fn focus_ring(ui: &Ui, tokens: &Tokens, rect: Rect) {
    if crate::navigation::running(ui) && !crate::navigation::shown(ui.ctx()) {
        return;
    }
    ui.painter().rect_stroke(
        rect.expand(2.0),
        RADIUS as f32 + 2.0,
        Stroke::new(2.0, tokens.focus),
        StrokeKind::Outside,
    );
}

/// Points along the value arc, from `from` to `to` in normalised units.
fn arc_points(centre: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    // Enough segments that the arc reads as a curve at 200% scale, few enough that a panel of 27
    // of them is not a per-frame cost worth thinking about.
    const SEGMENTS: usize = 48;
    let span = to - from;
    let count = ((SEGMENTS as f32 * span.abs()).ceil() as usize).max(2);
    (0..=count)
        .map(|i| {
            let t = from + span * (i as f32 / count as f32);
            centre + Vec2::angled(ARC_START + ARC_SWEEP * t) * radius
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Each cell hovers its own sentence** (the owner, 2026-09-27: *Hold, Envelope and Gate do not
    /// do the same, so should not have the same text*): hovering one shows what it does, and no
    /// other cell's sentence.
    #[test]
    fn each_cell_of_a_segmented_control_hovers_its_own_sentence() {
        use egui_kittest::kittest::Queryable as _;
        const OPTIONS: [&str; 3] = ["Hold", "Envelope", "Gate"];
        const DETAILS: [&str; 3] = [
            "Always open: the sound drones with no key held.",
            "The envelope shapes the volume of each note.",
            "Full volume while a key is held, silent when released.",
        ];
        for (index, option) in OPTIONS.iter().enumerate() {
            let mut harness = egui_kittest::Harness::new_ui(|ui| {
                ui.ctx()
                    .all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
                let mut selected = 1;
                segmented(
                    ui,
                    &crate::LIGHT,
                    "VCA",
                    &OPTIONS,
                    &mut selected,
                    None,
                    None,
                    &DETAILS,
                );
            });
            harness.run();
            harness.get_by_label(&format!("VCA: {option}")).hover();
            harness.run_steps(8);
            for (other, detail) in DETAILS.iter().enumerate() {
                let shown = harness.query_by_label_contains(detail).is_some();
                assert_eq!(
                    shown,
                    other == index,
                    "hovering {option}: {detail:?} is {}shown",
                    if shown { "" } else { "not " }
                );
            }
        }
    }

    /// Where an app-bar inline slider's name and track begin, drawn in a right-to-left group as the
    /// bar draws it.
    fn inline_slider_left(text: &str, widest: &str) -> f32 {
        let ctx = egui::Context::default();
        let mut left = f32::NAN;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 60.0),
            )),
            ..Default::default()
        };
        // Two frames: fonts are installed by the first.
        for _ in 0..2 {
            let mut output = ctx.run_ui(input.clone(), |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    left = ui
                        .scope(|ui| {
                            let view =
                                ParamView::new("Master", text, "A parameter.").widest(widest);
                            let mut normalised = 0.5;
                            let mut entry = None;
                            let _ = slider_inline(
                                ui,
                                &crate::theme::LIGHT,
                                &view,
                                &mut normalised,
                                96.0,
                                &mut entry,
                                Wheel::Off,
                            );
                        })
                        .response
                        .rect
                        .min
                        .x;
                });
            });
            output.textures_delta.clear();
        }
        left
    }

    #[test]
    fn an_inline_slider_holds_its_place_whatever_value_it_shows() {
        let narrow = inline_slider_left("0.0 dB", "-60.0 dB");
        let wide = inline_slider_left("-12.3 dB", "-60.0 dB");
        assert_eq!(
            narrow, wide,
            "the track would move under a dragging pointer"
        );
        // Without a declared widest value the region follows the text, which is the defect.
        assert_ne!(
            inline_slider_left("0.0 dB", ""),
            inline_slider_left("-12.3 dB", "")
        );
    }

    #[test]
    fn a_vertical_slider_runs_from_zero_at_the_bottom_to_one_at_the_top() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(40.0, 200.0));
        assert_eq!(vertical_slider_value(rect, rect.bottom()), 0.0);
        assert_eq!(vertical_slider_value(rect, rect.top()), 1.0);
        assert_eq!(vertical_slider_value(rect, rect.center().y), 0.5);
        assert_eq!(vertical_slider_value(rect, rect.bottom() + 20.0), 0.0);
        assert_eq!(vertical_slider_value(rect, rect.top() - 20.0), 1.0);
    }

    #[test]
    fn the_widest_value_is_sampled_across_the_range() {
        let widest = widest_value(|normalised| format!("{:.1} dB", -60.0 + 66.0 * normalised));
        // Every eight-character reading (`-60.0 dB`, `-10.5 dB`) is equally wide when monospaced.
        assert_eq!(widest.chars().count(), 8);
        assert_eq!(widest_value(|_| String::new()), "");
    }

    /// Drags a knob upwards, with a host that applies what the editor asks only every
    /// `apply_every` frames, and returns where the parameter ended up.
    ///
    /// The editor only ever reads what the host has **applied**, which is the whole point: a
    /// plugin parameter is not the editor's variable.
    fn drag_under_a_host_applying_every(apply_every: usize) -> f64 {
        const FRAMES: usize = 30;
        const STEP: f32 = 2.0;

        let ctx = egui::Context::default();
        let mut applied = 0.5_f64;
        let mut requested: Option<f64> = None;
        // Inside the knob's circle: the control is drawn at the panel's origin and the circle
        // sits under its two-line name box.
        let mut pointer = egui::pos2(32.0, 66.0);

        for frame in 0..FRAMES {
            // Frame 0 only puts the pointer on the knob: egui hit-tests against the geometry of
            // the frame before, so a press on the first frame lands on nothing. Frame 1 presses,
            // and every frame after drags upwards, which is what increases a knob.
            let mut events = Vec::new();
            match frame {
                0 => events.push(egui::Event::PointerMoved(pointer)),
                1 => events.push(egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                }),
                _ => {
                    pointer.y -= STEP;
                    events.push(egui::Event::PointerMoved(pointer));
                }
            }

            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(200.0, 200.0),
                )),
                events,
                ..Default::default()
            };

            let mut asked: Option<f64> = None;
            let mut output = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        let view = ParamView::new("Cutoff", "1.2 kHz", "A parameter.");
                        let mut normalised = applied;
                        let mut entry = None;
                        let outcome = knob(
                            ui,
                            &crate::theme::LIGHT,
                            &view,
                            &mut normalised,
                            Size::Primary,
                            Size::Primary.diameter(),
                            &mut entry,
                            Wheel::Off,
                        );
                        if outcome.changed {
                            asked = Some(normalised);
                        }
                    });
            });
            output.textures_delta.clear();

            if let Some(value) = asked {
                requested = Some(value);
            }
            // The audio thread's clock, which is not the editor's.
            if frame % apply_every == apply_every - 1
                && let Some(value) = requested.take()
            {
                applied = value;
            }
        }
        if let Some(value) = requested.take() {
            applied = value;
        }
        applied
    }

    /// **A late-applying host must not change where a drag ends up.**
    ///
    /// This is the jitter reported against `mxm-chorus-06` in MXM Player, as arithmetic. A plugin
    /// parameter is not the editor's variable: the editor asks, and the value lands when the
    /// plugin next processes audio, which is a different clock from the one the editor repaints
    /// on. Before `DragAnchor` the knob added each frame's delta to whatever it read back, so
    /// every frame that ran before the host caught up added its delta to a value that had not
    /// moved — the knob travelled a fraction of the pointer and lurched while doing it.
    ///
    /// Comparing against the instant host rather than against a computed distance is deliberate:
    /// egui eats the first few pixels of a press before calling it a drag, and a test that hard-
    /// coded that threshold would break the next time egui tuned it. What must hold is that the
    /// *lag* costs nothing.
    ///
    /// Falsified by reverting `DragAnchor`: the lagged drags then land well short of the instant
    /// one.
    #[test]
    fn a_late_applying_host_does_not_change_where_a_drag_ends() {
        let instant = drag_under_a_host_applying_every(1);
        assert!(
            instant > 0.6,
            "the drag has to move the knob for this to prove anything; it reached {instant}"
        );
        for apply_every in [2, 3, 5] {
            let lagged = drag_under_a_host_applying_every(apply_every);
            assert!(
                (lagged - instant).abs() < 1e-6,
                "applied every {apply_every} frames the drag reached {lagged}, and applied at \
                 once it reached {instant}"
            );
        }
    }

    /// Every painted shape of a top-aligned row holding a knob of `tier` and `build`, flattened.
    fn row_beside_a_knob(tier: Size, mut build: impl FnMut(&mut Ui) + 'static) -> Vec<egui::Shape> {
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            ui.horizontal_top(|ui| {
                let view = ParamView::new("Cutoff", "1.2 kHz", "A parameter.");
                let mut normalised = 0.5;
                let mut entry = None;
                let _ = knob(
                    ui,
                    &crate::theme::LIGHT,
                    &view,
                    &mut normalised,
                    tier,
                    tier.diameter(),
                    &mut entry,
                    Wheel::Off,
                );
                ui.add_space(SPACE_3);
                build(ui);
            });
        });
        harness.run();
        fn collect(shape: &egui::Shape, into: &mut Vec<egui::Shape>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                other => into.push(other.clone()),
            }
        }
        let mut out = Vec::new();
        for clipped in &harness.output().shapes {
            collect(&clipped.shape, &mut out);
        }
        out
    }

    /// Where a row's parts were painted: the knob's name and circle, the other control's label
    /// and cells. Read off the paint output, because that is what a person compares.
    struct Landed {
        knob_name: f32,
        knob_centre: f32,
        label: f32,
        cells: f32,
    }

    fn landed(shapes: &[egui::Shape], tier: Size, label: &str) -> Landed {
        let text_top = |wanted: &str| {
            shapes
                .iter()
                .find_map(|s| match s {
                    egui::Shape::Text(t) if t.galley.text() == wanted => Some(t.pos.y),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no text {wanted:?} was painted"))
        };
        // The track is the one path painted at the track's width: `paint_knob` draws it at
        // `diameter / 2 - 3` in radius with a stroke of `max(0.09 * diameter, 2)`, and the visual
        // bounds add the stroke. Its 270° arc reaches the circle's top but not its bottom, so the
        // centre is the top plus half the width, not the bounds' middle.
        let diameter = tier.diameter();
        let track_width = (diameter - 6.0) + (diameter * 0.09).max(2.0);
        let knob_centre = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Path(p) => {
                    let bounds = p.visual_bounding_rect();
                    ((bounds.width() - track_width).abs() < 0.5)
                        .then(|| bounds.top() + bounds.width() / 2.0)
                }
                _ => None,
            })
            .expect("no knob track was painted");
        let cells = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if (r.rect.height() - MIN_TARGET).abs() < 0.5 => {
                    Some(r.rect.center().y)
                }
                _ => None,
            })
            .expect("no cell was painted");
        Landed {
            knob_name: text_top("Cutoff"),
            knob_centre,
            label: text_top(label),
            cells,
        }
    }

    #[test]
    fn a_toggle_beside_a_knob_centres_on_the_knob_circle() {
        for tier in [Size::Primary, Size::Standard, Size::Compact] {
            let shapes = row_beside_a_knob(tier, move |ui| {
                let mut on = false;
                toggle_beside(
                    ui,
                    &crate::theme::LIGHT,
                    "Sync",
                    &mut on,
                    false,
                    tier,
                    "Follow host tempo.",
                );
            });
            let landed = landed(&shapes, tier, "Sync");
            assert!(
                (landed.cells - landed.knob_centre).abs() < 0.5,
                "{tier:?}: toggle centre {}, knob centre {}",
                landed.cells,
                landed.knob_centre
            );
        }
    }

    #[test]
    fn a_switch_beside_a_knob_sits_on_the_knobs_grid() {
        // **Measured, not reasoned about**, on a 1:1 render. Laid out plainly beside a `Standard`
        // knob in a top-aligned row, a segmented control's label painted 14.9 px above the knob's
        // name -- one Body line, because the knob's name box is two lines tall with the name on
        // the second -- and its cells 22.9 px above the circle's centre. The wave picker's old
        // eight-pixel lift left it 14.9 px high on both counts. Every tier, both kinds: a constant
        // would be right for one tier and wrong for the rest.
        for tier in [Size::Primary, Size::Standard, Size::Compact] {
            let words = row_beside_a_knob(tier, move |ui| {
                let mut selected = 0;
                segmented_beside(
                    ui,
                    &crate::theme::LIGHT,
                    "Mode",
                    &["A", "B"],
                    &mut selected,
                    None,
                    None,
                    tier,
                    &described(&["A", "B"]),
                );
            });
            let waves = row_beside_a_knob(tier, move |ui| {
                let mut selected = 0;
                segmented_waves(
                    ui,
                    &crate::theme::LIGHT,
                    "Shape",
                    &[(Wave::Triangle, "Triangle"), (Wave::Square, "Square")],
                    &mut selected,
                    None,
                    None,
                    Some(tier),
                    &described(&["Triangle", "Square"]),
                );
            });
            for (kind, shapes, label) in [("words", words, "Mode"), ("waves", waves, "Shape")] {
                let at = landed(&shapes, tier, label);
                assert!(
                    (at.label - at.knob_name).abs() < 0.5,
                    "{tier:?}, {kind}: the label painted at y {} and the knob's name at {}",
                    at.label,
                    at.knob_name
                );
                assert!(
                    (at.cells - at.knob_centre).abs() < 0.5,
                    "{tier:?}, {kind}: the cells centre at y {} and the circle at {}",
                    at.cells,
                    at.knob_centre
                );
            }
        }
    }

    #[test]
    fn a_caret_selector_beside_a_knob_aligns_with_the_knobs_name_line() {
        for tier in [Size::Primary, Size::Standard, Size::Compact] {
            let shapes = row_beside_a_knob(tier, move |ui| {
                let mut selected = 1;
                selector_beside(
                    ui,
                    &crate::theme::LIGHT,
                    "Route",
                    &["Direct", "Off", "Wheel"],
                    &mut selected,
                    None,
                    Some(1),
                    tier,
                    "A description.",
                );
            });
            let text_top = |wanted: &str| {
                shapes
                    .iter()
                    .find_map(|shape| match shape {
                        egui::Shape::Text(text) if text.galley.text() == wanted => Some(text.pos.y),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("no text {wanted:?} was painted"))
            };
            let route_top = text_top("Route");
            let knob_top = text_top("Cutoff");
            assert!(
                (route_top - knob_top).abs() < 0.5,
                "{tier:?}: selector label painted at {route_top}, knob label at {knob_top}"
            );
            let carets = shapes
                .iter()
                .filter(|shape| {
                    matches!(shape, egui::Shape::Path(path) if path.points.len() == 3 && path.visual_bounding_rect().width() < 10.0)
                })
                .count();
            assert_eq!(carets, 2, "{tier:?}: selector did not paint two carets");
        }
    }

    /// Five families of twenty, *Room 0* to *Cathedral 19*: a list as long as mxm-classic-verb's
    /// Space selector is about to be, and one whose names a search can narrow in steps.
    fn many_options() -> Vec<String> {
        ["Room", "Chamber", "Hall", "Plate", "Cathedral"]
            .iter()
            .flat_map(|family| (0..20).map(move |i| format!("{family} {i}")))
            .collect()
    }

    /// One `selector` named *Space*, `lift` points down a window of `size`. The harness state is
    /// the selection, so a test reads back what a gesture chose.
    fn selector_harness(
        options: Vec<String>,
        selected: usize,
        size: Vec2,
        lift: f32,
    ) -> egui_kittest::Harness<'static, usize> {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(size)
            .build_ui_state(
                move |ui, selected: &mut usize| {
                    ui.add_space(lift);
                    let names: Vec<&str> = options.iter().map(String::as_str).collect();
                    selector(
                        ui,
                        &crate::theme::LIGHT,
                        "Space",
                        &names,
                        selected,
                        None,
                        Some(0),
                        "A description.",
                    );
                },
                selected,
            );
        harness.run();
        harness
    }

    /// A card-relative label is painted, and nothing else: every cell still announces the
    /// parameter's own name, which is what a host and a screen reader read.
    #[test]
    fn a_named_segmented_control_announces_its_full_name_not_its_painted_one() {
        use kittest::Queryable;
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(400.0, 120.0))
            .build_ui_state(
                |ui, selected: &mut usize| {
                    segmented_named(
                        ui,
                        &crate::theme::LIGHT,
                        "Mode",
                        "LPG 1 mode",
                        &["VCA", "VCF"],
                        selected,
                        None,
                        Some(0),
                        0.0,
                        &described(&["VCA", "VCF"]),
                    );
                },
                0usize,
            );
        harness.run();
        assert!(
            harness.query_by_label("LPG 1 mode: VCA").is_some(),
            "a cell lost the parameter's own name"
        );
        assert!(
            harness.query_by_label("Mode: VCA").is_none(),
            "a cell announced the painted label instead"
        );
    }

    /// **A name in a box exactly its longest word wide keeps to its lines.** A knob at its narrowest
    /// is that wide, and egui broke the word itself on a hair of floating point: *Pitch envelope*
    /// came out over three lines above a two-line box.
    #[test]
    fn a_name_at_exactly_its_longest_word_keeps_to_its_lines() {
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let mut height = 0.0;
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let name = "Pitch envelope";
                let width = longest_word(ui, name, &egui::TextStyle::Body);
                let lines = ui.text_style_height(&egui::TextStyle::Body) * NAME_LINES as f32;
                let label = fixed_label(ui, name, width, NAME_LINES, egui::TextStyle::Body, None);
                height = label.rect.height() / lines;
            });
            output.textures_delta.clear();
        }
        // A third line is half the box again; egui's rows are a hair taller than the style's line.
        assert!(
            height < 1.25,
            "the name took {height:.2} of its two-line box"
        );
    }

    #[test]
    fn a_named_selector_paints_its_short_name_and_announces_its_full_one() {
        use kittest::Queryable;
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(400.0, 120.0))
            .build_ui_state(
                |ui, selected: &mut usize| {
                    selector_named(
                        ui,
                        &crate::theme::LIGHT,
                        "Range",
                        "Range 1",
                        &["16'", "8'", "4'"],
                        selected,
                        None,
                        Some(1),
                        "",
                    );
                },
                1usize,
            );
        harness.run();
        assert!(
            harness.query_by_label("Range").is_some(),
            "the short name is not painted"
        );
        assert!(
            harness.query_by_label("Range 1").is_none(),
            "the full name is painted"
        );
        assert!(
            harness.query_by_label_contains("Range 1").is_some(),
            "the control lost the parameter's own name"
        );
    }

    fn grouped_selector_harness() -> egui_kittest::Harness<'static, usize> {
        let options: Vec<String> = (0..30).map(|i| format!("Model {i}")).collect();
        let groups: Vec<String> = (0..30)
            .map(|i| {
                if i < 10 {
                    "Kicks"
                } else if i < 20 {
                    "Snares"
                } else {
                    "Metal"
                }
                .to_owned()
            })
            .collect();
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(360.0, 480.0))
            .build_ui_state(
                move |ui, selected: &mut usize| {
                    let options: Vec<&str> = options.iter().map(String::as_str).collect();
                    let groups: Vec<&str> = groups.iter().map(String::as_str).collect();
                    selector_grouped(
                        ui,
                        &crate::theme::LIGHT,
                        "Model",
                        &options,
                        &groups,
                        selected,
                        None,
                        Some(0),
                        "A description.",
                    );
                },
                0,
            );
        harness.run();
        harness
    }

    #[test]
    fn grouped_selector_headings_do_not_become_values_or_change_sparse_option_indices() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        let mut harness = grouped_selector_harness();
        harness.get_by_role(Role::ComboBox).click();
        harness.run();
        for heading in ["Kicks", "Snares", "Metal"] {
            assert!(
                harness.query_by_label(heading).is_some(),
                "missing {heading}"
            );
        }
        assert_eq!(
            menu_rows(&harness).len(),
            30,
            "headings became option buttons"
        );

        harness.get_by_role(Role::TextInput).type_text("Model 17");
        harness.run();
        assert!(harness.query_by_label("Kicks").is_none());
        assert!(harness.query_by_label("Metal").is_none());
        assert!(harness.query_by_label("Snares").is_some());
        harness
            .get_by_role_and_label(Role::Button, "Model 17")
            .click();
        harness.run();
        assert_eq!(*harness.state(), 17, "a heading changed option indexing");
    }

    /// The open menu's rectangle, frame included: a menu is the one foreground area.
    fn menu_rect<S>(harness: &egui_kittest::Harness<'_, S>) -> Rect {
        harness.ctx.memory(|memory| {
            let layer = memory
                .areas()
                .top_layer_id(egui::Order::Foreground)
                .expect("no menu is open");
            memory.area_rect(layer.id).expect("the menu has no rect")
        })
    }

    /// The labels of every row in the open menu, in order.
    fn menu_rows<S>(harness: &egui_kittest::Harness<'_, S>) -> Vec<String> {
        use kittest::{NodeT, Queryable};
        harness
            .query_all_by_role(egui::accesskit::Role::Button)
            .map(|row| row.accesskit_node().label().unwrap_or_default())
            .filter(|label| !(label.starts_with("Clear ") && label.ends_with(" search")))
            .collect()
    }

    /// **A hundred options stay inside a small window, on whichever side of the button has the
    /// room.** Before the list scrolled, its rows ran past the bottom of the window — §7.4 says a
    /// menu remains inside plugin bounds — and at the foot of the window the whole list opened
    /// downwards into nothing.
    #[test]
    fn a_long_selector_menu_opens_toward_the_room_and_stays_inside_a_small_window() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        let size = Vec2::new(320.0, 240.0);
        let window = Rect::from_min_size(Pos2::ZERO, size);
        for (lift, below) in [(0.0, true), (180.0, false)] {
            let mut harness = selector_harness(many_options(), 0, size, lift);
            let button = harness.get_by_role(Role::ComboBox).rect();
            harness.get_by_role(Role::ComboBox).click();
            harness.run();
            let menu = menu_rect(&harness);
            assert!(
                window.contains_rect(menu),
                "lift {lift}: the menu {menu:?} runs outside the {size:?} window"
            );
            if below {
                assert!(
                    menu.top() >= button.bottom() - 0.5,
                    "lift {lift}: the room is below the button {button:?}, the menu is {menu:?}"
                );
            } else {
                assert!(
                    menu.bottom() <= button.top() + 0.5,
                    "lift {lift}: the room is above the button {button:?}, the menu is {menu:?}"
                );
            }
            // A list, not a sliver: the cap is the room, not a token few rows.
            let in_view = harness
                .query_all_by_role(Role::Button)
                .filter(|row| menu.contains_rect(row.rect()))
                .count();
            assert!(
                in_view >= 5,
                "lift {lift}: only {in_view} rows are inside the menu"
            );
        }
    }

    /// **The selection is on screen, and the keyboard is in the search, the moment the menu
    /// opens.** Row 73 is far past the first screenful, so a list that opened at its top would
    /// show neither it nor where it is.
    #[test]
    fn a_long_selector_menu_opens_on_its_selection_with_the_keyboard_in_its_search() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        let options = many_options();
        let selected = 73;
        let size = Vec2::new(320.0, 240.0);
        let window = Rect::from_min_size(Pos2::ZERO, size);
        let mut harness = selector_harness(options.clone(), selected, size, 0.0);
        harness.get_by_role(Role::ComboBox).click();
        harness.run();
        let menu = window.intersect(menu_rect(&harness));
        let search = harness.get_by_role_and_label(Role::TextInput, "Space search");
        assert!(
            search.is_focused(),
            "the search field does not have the keyboard"
        );
        let field = search.rect();
        let row = harness
            .get_by_role_and_label(Role::Button, &options[selected])
            .rect();
        assert!(
            menu.contains_rect(row) && row.top() >= field.bottom(),
            "{} at {row:?} is not in view below the search {field:?} inside the menu {menu:?}",
            options[selected]
        );
    }

    /// **Typing narrows the list, whatever the case, and a row keeps its own index.** A filtered
    /// row that reported its position in the *filtered* list would set the wrong space.
    #[test]
    fn typing_narrows_a_long_menu_and_a_filtered_row_sets_its_own_index() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        let mut harness = selector_harness(many_options(), 0, Vec2::new(360.0, 480.0), 0.0);
        harness.get_by_role(Role::ComboBox).click();
        harness.run();
        assert_eq!(menu_rows(&harness).len(), 100);

        harness.get_by_role(Role::TextInput).type_text("HALL 1");
        harness.run();
        let mut expected = vec!["Hall 1".to_owned()];
        expected.extend((10..20).map(|i| format!("Hall {i}")));
        assert_eq!(menu_rows(&harness), expected);

        harness
            .get_by_role_and_label(Role::Button, "Hall 14")
            .click();
        harness.run();
        assert_eq!(*harness.state(), 2 * 20 + 14, "Hall 14 is option 54");
        assert!(
            harness.query_by_role(Role::TextInput).is_none(),
            "choosing a row closes the menu"
        );
    }

    /// **`Enter` takes the first match, `Escape` takes nothing, and the query survives closing**
    /// until its explicit clear action is used.
    #[test]
    fn enter_and_escape_close_without_discarding_the_search_and_clear_restores_the_list() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        let mut harness = selector_harness(many_options(), 0, Vec2::new(360.0, 480.0), 0.0);
        harness.get_by_role(Role::ComboBox).click();
        harness.run();
        harness.get_by_role(Role::TextInput).type_text("plate 1");
        harness.run();
        harness.key_press(Key::Enter);
        harness.run();
        assert_eq!(*harness.state(), 3 * 20 + 1, "Plate 1 is the first match");
        assert!(
            harness.query_by_role(Role::TextInput).is_none(),
            "Enter closes the menu"
        );

        harness.get_by_role(Role::ComboBox).click();
        harness.run();
        assert_eq!(
            menu_rows(&harness),
            vec![
                "Plate 1".to_owned(),
                "Plate 10".to_owned(),
                "Plate 11".to_owned(),
                "Plate 12".to_owned(),
                "Plate 13".to_owned(),
                "Plate 14".to_owned(),
                "Plate 15".to_owned(),
                "Plate 16".to_owned(),
                "Plate 17".to_owned(),
                "Plate 18".to_owned(),
                "Plate 19".to_owned()
            ],
            "a menu must reopen on its saved search"
        );
        harness
            .get_by_role_and_label(Role::Button, "Clear Space search")
            .click();
        harness.run();
        assert_eq!(
            menu_rows(&harness).len(),
            100,
            "clear did not restore the list"
        );
        harness.get_by_role(Role::TextInput).type_text("hall");
        harness.run();
        harness.key_press(Key::Escape);
        harness.run();
        assert_eq!(*harness.state(), 3 * 20 + 1, "Escape chose something");
        assert!(
            harness.query_by_role(Role::TextInput).is_none(),
            "Escape closes the menu"
        );
        harness.get_by_role(Role::ComboBox).click();
        harness.run();
        assert_eq!(
            menu_rows(&harness).len(),
            20,
            "Escape discarded the saved query"
        );
    }

    /// **`↓` hands the keyboard from the search to the list, and egui's focus travel walks it from
    /// there**, as it walks a short menu. The field locks the arrows while it has focus, so without
    /// the hand-over the rows could not be reached from the keyboard at all.
    #[test]
    fn down_arrow_moves_the_keyboard_from_the_search_into_the_rows() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        let mut harness = selector_harness(many_options(), 0, Vec2::new(360.0, 480.0), 0.0);
        harness.get_by_role(Role::ComboBox).click();
        harness.run();
        harness.get_by_role(Role::TextInput).type_text("hall 1");
        harness.run();
        for row in ["Hall 1", "Hall 10"] {
            harness.key_press(Key::ArrowDown);
            harness.run();
            assert!(
                harness
                    .get_by_role_and_label(Role::Button, row)
                    .is_focused(),
                "{row} does not have the keyboard"
            );
        }
        harness.key_press(Key::Enter);
        harness.run();
        assert_eq!(
            *harness.state(),
            2 * 20 + 10,
            "Enter on a row chooses that row"
        );
    }

    /// **Search starts one past [`SEARCH_ABOVE`].** No editor draws a list that long today, so no
    /// menu an editor already had gained a field.
    #[test]
    fn a_short_selector_menu_has_no_search_field() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        for count in [3, SEARCH_ABOVE, SEARCH_ABOVE + 1] {
            let options: Vec<String> = (0..count).map(|i| format!("Source {i}")).collect();
            let mut harness = selector_harness(options, 0, Vec2::new(360.0, 900.0), 0.0);
            harness.get_by_role(Role::ComboBox).click();
            harness.run();
            assert_eq!(
                harness.query_all_by_role(Role::TextInput).count(),
                usize::from(count > SEARCH_ABOVE),
                "{count} options"
            );
            assert_eq!(menu_rows(&harness).len(), count, "{count} options");
        }
    }

    /// **A list that fits is laid out exactly as the menu was before it could scroll**: egui's
    /// `MenuButton` over a plain loop of rows, which is what `selector_caret` drew. Fifteen rows,
    /// mxm-mono-00's route menus, compared row by row relative to the menu's corner.
    #[test]
    fn a_short_selector_menu_is_laid_out_as_a_plain_menu() {
        use egui::accesskit::Role;
        use kittest::Queryable;
        let options: Vec<String> = (0..15).map(|i| format!("Source {i}")).collect();
        let size = Vec2::new(400.0, 600.0);

        let mut ours = selector_harness(options.clone(), 3, size, 0.0);
        ours.get_by_role(Role::ComboBox).click();
        ours.run();

        let rows = options.clone();
        let mut plain = egui_kittest::Harness::builder()
            .with_size(size)
            .build_ui(move |ui| {
                egui::containers::menu::MenuButton::new("Open").ui(ui, |ui| {
                    for (index, row) in rows.iter().enumerate() {
                        if ui.selectable_label(index == 3, row.as_str()).clicked() {
                            ui.close();
                        }
                    }
                });
            });
        plain.run();
        plain.get_by_role_and_label(Role::Button, "Open").click();
        plain.run();

        let (our_menu, plain_menu) = (menu_rect(&ours), menu_rect(&plain));
        assert!(
            (our_menu.size() - plain_menu.size()).length() < 0.5,
            "the menu is {:?}, a plain menu {:?}",
            our_menu.size(),
            plain_menu.size()
        );
        for option in &options {
            let our_row = ours.get_by_role_and_label(Role::Button, option).rect();
            let plain_row = plain.get_by_role_and_label(Role::Button, option).rect();
            let moved = (our_row.min - our_menu.min) - (plain_row.min - plain_menu.min);
            assert!(
                moved.length() < 0.5 && (our_row.size() - plain_row.size()).length() < 0.5,
                "{option} is {our_row:?} in {our_menu:?}, and {plain_row:?} in a plain menu \
                 {plain_menu:?}"
            );
        }
    }

    #[test]
    fn a_switch_on_a_row_of_its_own_starts_at_its_label() {
        // `None` is not "beside a Compact knob": nothing above the label, and nothing between it
        // and the cells but the usual spacing. mxm-mono-03's waveform row stands alone and used to
        // pass `Standard`, which quietly put eight pixels of nothing under its label.
        let measured = std::rc::Rc::new(std::cell::Cell::new((0.0f32, 0.0f32)));
        let out = std::rc::Rc::clone(&measured);
        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            ui.set_max_width(200.0);
            let line = ui.text_style_height(&egui::TextStyle::Body);
            let top = ui.cursor().top();
            let mut selected = 0;
            segmented_waves(
                ui,
                &crate::theme::LIGHT,
                "Shape",
                &[(Wave::Triangle, "Triangle"), (Wave::Square, "Square")],
                &mut selected,
                None,
                None,
                None,
                &described(&["Triangle", "Square"]),
            );
            out.set((ui.min_rect().bottom() - top, line));
        });
        harness.run();
        let (height, line) = measured.get();
        let expected = line + SPACE_2 + MIN_TARGET;
        assert!(
            (height - expected).abs() < 0.5,
            "a wave picker on its own row is {height} tall, not label + spacing + cells = {expected}"
        );
    }

    /// Draws one segmented control alone, returns its cell rects (left to right).
    fn segmented_cells(ctx: &egui::Context, run: &mut dyn FnMut(&mut Ui)) -> Vec<Rect> {
        cells_in(ctx, 120.0, run)
    }

    /// [`segmented_cells`] on a screen `height` tall, for controls stacked one under another.
    fn cells_in(ctx: &egui::Context, height: f32, run: &mut dyn FnMut(&mut Ui)) -> Vec<Rect> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(400.0, height),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| run(ui));
        output.textures_delta.clear();
        let mut cells: Vec<Rect> = Vec::new();
        fn collect(shape: &egui::Shape, into: &mut Vec<Rect>) {
            match shape {
                egui::Shape::Rect(rect) if (rect.rect.height() - MIN_TARGET).abs() < 0.5 => {
                    into.push(rect.rect);
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, into);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut cells);
        }
        // Row by row, then left to right; a cell paints a fill and a border at one position, so
        // anything at the same corner is the same cell. Both coordinates, or a stack collapses.
        cells.sort_by(|a, b| {
            a.min
                .y
                .total_cmp(&b.min.y)
                .then(a.min.x.total_cmp(&b.min.x))
        });
        cells.dedup_by(|a, b| (a.min - b.min).length() < 0.5);
        cells
    }

    /// One frame carrying a full click (or two, for a double) at `pos`.
    fn frame_with_clicks(pos: egui::Pos2, clicks: usize) -> egui::RawInput {
        let mut events = vec![egui::Event::PointerMoved(pos)];
        for _ in 0..clicks {
            for pressed in [true, false] {
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(400.0, 120.0),
            )),
            events,
            ..Default::default()
        }
    }

    fn run_segmented(
        ctx: &egui::Context,
        input: egui::RawInput,
        selected: &mut usize,
        sounding: Option<usize>,
        default_cell: Option<usize>,
    ) -> bool {
        let mut changed = false;
        let mut output = ctx.run_ui(input, |ui| {
            let tokens = crate::theme::DARK;
            changed = segmented(
                ui,
                &tokens,
                "Range",
                &["16'", "8'", "4'", "2'"],
                selected,
                sounding,
                default_cell,
                &described(&["16'", "8'", "4'", "2'"]),
            );
        });
        output.textures_delta.clear();
        changed
    }

    #[test]
    fn a_click_on_the_selected_cell_fires_while_something_else_sounds() {
        // "I can change 16' to 8' but not back to 16'": the selection is the base, the lock was
        // sounding another cell, and the click on the base cell was swallowed as a no-op. Under
        // modulation, clicking the selected cell is an edit - "play this one".
        let ctx = egui::Context::default();
        let mut selected = 0;
        let cells = segmented_cells(&ctx, &mut |ui| {
            let tokens = crate::theme::DARK;
            segmented(
                ui,
                &tokens,
                "Range",
                &["16'", "8'", "4'", "2'"],
                &mut selected,
                Some(2),
                Some(1),
                &described(&["16'", "8'", "4'", "2'"]),
            );
        });
        assert_eq!(cells.len(), 4, "four cells found: {cells:?}");

        let changed = run_segmented(
            &ctx,
            frame_with_clicks(cells[0].center(), 1),
            &mut selected,
            Some(2),
            Some(1),
        );
        assert!(
            changed,
            "the click on the sounding-elsewhere selection fires"
        );
        assert_eq!(selected, 0, "and it re-asserts the same cell");
    }

    #[test]
    fn a_click_on_the_selected_cell_stays_inert_when_nothing_sounds() {
        let ctx = egui::Context::default();
        let mut selected = 0;
        let cells = segmented_cells(&ctx, &mut |ui| {
            let tokens = crate::theme::DARK;
            segmented(
                ui,
                &tokens,
                "Range",
                &["16'", "8'", "4'", "2'"],
                &mut selected,
                None,
                Some(1),
                &described(&["16'", "8'", "4'", "2'"]),
            );
        });
        let changed = run_segmented(
            &ctx,
            frame_with_clicks(cells[0].center(), 1),
            &mut selected,
            None,
            Some(1),
        );
        assert!(!changed, "an unmodulated re-click is still a no-op");

        // And the same frame geometry genuinely lands clicks - a different cell changes.
        let changed = run_segmented(
            &ctx,
            frame_with_clicks(cells[2].center(), 1),
            &mut selected,
            None,
            Some(1),
        );
        assert!(
            changed && selected == 2,
            "the no-op above was a decision, not a miss"
        );
    }

    #[test]
    fn a_double_click_resets_to_the_default_cell() {
        // The same gesture a knob answers, with the same meaning.
        let ctx = egui::Context::default();
        let mut selected = 3;
        let cells = segmented_cells(&ctx, &mut |ui| {
            let tokens = crate::theme::DARK;
            segmented(
                ui,
                &tokens,
                "Range",
                &["16'", "8'", "4'", "2'"],
                &mut selected,
                None,
                Some(1),
                &described(&["16'", "8'", "4'", "2'"]),
            );
        });
        let changed = run_segmented(
            &ctx,
            frame_with_clicks(cells[3].center(), 2),
            &mut selected,
            None,
            Some(1),
        );
        assert!(changed, "the double-click is an edit");
        assert_eq!(selected, 1, "back to the default cell");
    }

    #[test]
    fn a_waveform_cell_is_a_pointer_target() {
        // §11, in the one place a cell could be shrunk to look tidier: a picture needs less room
        // than a word, and that is not a reason to go under the floor.
        let cell = wave_cell();
        assert!(cell.x >= MIN_TARGET && cell.y >= MIN_TARGET, "{cell:?}");
    }

    #[test]
    fn every_waveform_is_a_path_that_can_actually_be_drawn() {
        // A single point is not a line, and a shape whose path is empty renders as nothing --
        // which looks exactly like a segment that failed to paint.
        for wave in [
            Wave::Triangle,
            Wave::Square,
            Wave::RampUp,
            Wave::RampDown,
            Wave::Random,
            Wave::Pulse,
            Wave::Sine,
            Wave::Spike,
            Wave::Noise,
            Wave::SmoothRandom,
            Wave::Trigger,
            Wave::Decay,
            Wave::Gated,
            Wave::Swell,
            Wave::Level,
            Wave::Fade,
            Wave::Rise,
            Wave::QuarterNote,
        ] {
            let path = wave.path();
            assert!(path.len() >= 2, "{wave:?} is not a line");
            for (x, y) in path {
                assert!(
                    (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y),
                    "{wave:?} leaves the unit box at ({x}, {y}), so it would paint outside its cell"
                );
            }
        }
    }

    #[test]
    fn no_two_waveforms_draw_the_same_picture() {
        // The one failure that is invisible in a screenshot: two shapes that look alike are not a
        // rendering bug, they are a control that cannot be used. The ramps are the pair at risk --
        // both are a diagonal, and only the drawn reset tells them apart.
        let all = [
            Wave::Triangle,
            Wave::Square,
            Wave::RampUp,
            Wave::RampDown,
            Wave::Random,
            Wave::Pulse,
            Wave::Sine,
            Wave::Spike,
            Wave::Noise,
            Wave::SmoothRandom,
            Wave::Trigger,
            Wave::Decay,
            Wave::Gated,
            Wave::Swell,
            Wave::Level,
            Wave::Fade,
            Wave::Rise,
            Wave::QuarterNote,
        ];
        for (index, a) in all.iter().enumerate() {
            for b in &all[index + 1..] {
                assert_ne!(a.path(), b.path(), "{a:?} and {b:?} draw the same shape");
            }
        }
    }

    #[test]
    fn a_ramp_draws_its_reset_so_the_two_ramps_are_not_one_diagonal() {
        // Without the vertical return, rising and falling ramps are the same line read in two
        // directions -- and a still picture has no direction.
        for wave in [Wave::RampUp, Wave::RampDown] {
            let path = wave.path();
            let last = path[path.len() - 1];
            let previous = path[path.len() - 2];
            assert_eq!(
                last.0, previous.0,
                "{wave:?} must end with a vertical reset, not a bare diagonal"
            );
        }
    }

    #[test]
    fn sample_and_hold_is_drawn_as_held_levels_rather_than_a_scribble() {
        // It is the shape of the modulation, not of noise: flat runs joined by vertical jumps. A
        // wandering line would say "noise", which is a different thing the LFO does not do.
        let path = Wave::Random.path();
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let flat = a.1 == b.1;
            let jump = a.0 == b.0;
            assert!(
                flat || jump,
                "sample and hold has no sloped segments; {a:?} to {b:?} is one"
            );
        }
    }

    /// The height one knob's whole column takes, with `text` as its value.
    ///
    /// Rendered rather than reasoned about: the defect was a text layout deciding a height, and no
    /// arithmetic over the inputs would have caught it — only laying the thing out does.
    fn column_height(name: &str, text: &str) -> f32 {
        let measured = std::rc::Rc::new(std::cell::Cell::new(0.0f32));
        let out = std::rc::Rc::clone(&measured);

        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            let view = ParamView::new(name, text, "A parameter.");
            let mut normalised = 0.5;
            let mut entry = None;

            // **Constrained to the knob's own width**, which is what a row of them gives each
            // column. Without this the harness hands the label the whole window, nothing ever
            // wraps, and the test passes against the very layout it is meant to reject.
            ui.set_max_width(Size::Standard.diameter());

            let top = ui.cursor().top();
            let _ = knob(
                ui,
                &crate::theme::LIGHT,
                &view,
                &mut normalised,
                Size::Standard,
                Size::Standard.diameter(),
                &mut entry,
                Wheel::Off,
            );
            out.set(ui.min_rect().bottom() - top);
        });
        harness.run();
        measured.get()
    }

    /// Every line a knob's name is laid out on, at `width`.
    fn name_lines(name: &str, width: f32) -> Vec<String> {
        let rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let out = std::rc::Rc::clone(&rows);

        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            let style = egui::TextStyle::Body;
            let font = style.resolve(ui.style());
            // Measured **before** the layout call: both take the font lock, and nesting them
            // deadlocks rather than failing, which costs ten seconds to discover.
            let wrap_at = width.max(longest_word(ui, name, &style));
            let galley = ui.ctx().fonts_mut(|fonts| {
                fonts.layout(name.to_owned(), font, Color32::PLACEHOLDER, wrap_at)
            });
            *out.borrow_mut() = galley
                .rows
                .iter()
                .map(|row| row.text().to_owned())
                .collect();
        });
        harness.run();
        let cloned = rows.borrow().clone();
        // Cloned out before the borrow ends, so the harness can be dropped with it still in hand.
        cloned
    }

    #[test]
    fn a_label_never_breaks_a_word_however_narrow_its_box_is() {
        // **Reported, and made a standing rule.** `break_anywhere` is false throughout this crate,
        // which makes egui *prefer* a word boundary -- and its fallback, when no word fits the
        // line, is to break mid-word anyway. `Resonance` in a 48 px box came out as "Resonan" over
        // "ce", and `Key tracking` as "Key" over "tracki" over "ng".
        //
        // Preference is not a guarantee. Width is: a box is never narrower than its longest word.
        for name in [
            "Resonance",
            "Filter envelope",
            "Key tracking",
            "Filter LFO",
            "Cutoff",
        ] {
            // Absurdly narrow on purpose -- the rule has to hold at any width, not at the one the
            // filter card happens to give it today.
            for width in [8.0, 24.0, 48.0, 72.0] {
                let lines = name_lines(name, width);
                let rejoined = lines.join(" ");
                let words: Vec<&str> = rejoined.split_whitespace().collect();
                let expected: Vec<&str> = name.split_whitespace().collect();
                assert_eq!(
                    words, expected,
                    "{name:?} at {width} px came out as {lines:?}, which splits a word"
                );
            }
        }
    }

    /// How far the arc `paint_knob` drew sits from the centre of the column it was given.
    ///
    /// Read off the paint output rather than the layout: `paint_knob` centres its arc on the rect
    /// it is handed, so where the arc ended up *is* where the knob is — which is the thing a person
    /// sees and the thing that was wrong.
    fn knob_offset_from_column_centre(name: &str, width: f32) -> f32 {
        let column = std::rc::Rc::new(std::cell::Cell::new(f32::NAN));
        let out = std::rc::Rc::clone(&column);

        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            let view = ParamView::new(name, "1.2 kHz", "A parameter.");
            let mut normalised = 0.5;
            let mut entry = None;
            ui.set_max_width(width);

            let _ = knob(
                ui,
                &crate::theme::LIGHT,
                &view,
                &mut normalised,
                Size::Standard,
                width,
                &mut entry,
                Wheel::Off,
            );
            out.set(ui.min_rect().center().x);
        });
        harness.run();

        // The knob's track and value arcs are the only paths as wide as the tier's diameter.
        let diameter = Size::Standard.diameter();
        let arc = harness
            .output()
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Path(path) => Some(path.visual_bounding_rect()),
                _ => None,
            })
            .find(|bounds| (bounds.width() - diameter).abs() < 3.0)
            .map(|bounds| bounds.center().x);

        match arc {
            Some(x) => x - column.get(),
            None => f32::NAN,
        }
    }

    #[test]
    fn a_knob_sits_in_the_middle_of_its_column_like_its_label_does() {
        // **Reported twice, on two axes.** The name and value boxes are the full column width, so
        // the block around them is too -- and a knob allocated at its own size then sat against
        // that block's left edge while its label sat over the middle. A label centred over nothing
        // is worse than no label.
        for width in [48.0f32, 72.0, 120.0] {
            let offset = knob_offset_from_column_centre("Cutoff", width);
            assert!(
                offset.is_finite(),
                "no knob arc was painted in a {width} px column"
            );
            assert!(
                offset.abs() < 1.5,
                "in a {width} px column the knob painted {offset} px off centre"
            );
        }
    }

    #[test]
    fn a_knob_column_is_the_same_height_whatever_its_value_says() {
        // **Reported from two screenshots.** Turning cutoff from 1.2 kHz to 20.0 kHz made the
        // Filter card grow a row: the value was laid out in a box the knob's width and allowed to
        // wrap, so the longer reading took two lines. The interface moved while you were using it,
        // which is the whole thing this crate's fixed-geometry rule exists to prevent.
        let short = column_height("Cutoff", "1.2 kHz");
        let long = column_height("Cutoff", "20.0 kHz");
        assert!(
            (short - long).abs() < 0.5,
            "a longer value made the column {long} tall instead of {short}"
        );

        // And the extreme, so the assertion is not passing on two values that happen to fit.
        let absurd = column_height("Cutoff", "-1234.5678 kHz");
        assert!(
            (short - absurd).abs() < 0.5,
            "a very long value made the column {absurd} tall instead of {short}"
        );
    }

    #[test]
    fn a_knob_column_is_the_same_height_whatever_its_name_says() {
        // The same defect one row up, and the one that made a row of knobs sit at three different
        // heights: *Cutoff* is one line at a knob's width and *Filter envelope* is two.
        let short = column_height("Cutoff", "1.2 kHz");
        let wrapping = column_height("Filter envelope", "1.2 kHz");
        assert!(
            (short - wrapping).abs() < 0.5,
            "a wrapping name made the column {wrapping} tall instead of {short}"
        );
    }

    #[test]
    fn the_size_tiers_match_the_design_system() {
        // §7.1: one diameter for every tier; the tier decides value visibility, not size.
        for size in [Size::Primary, Size::Standard, Size::Compact] {
            assert_eq!(size.diameter(), KNOB_DIAMETER);
        }
        assert!(Size::Primary.value_always_visible());
        assert!(Size::Standard.value_always_visible());
        assert!(!Size::Compact.value_always_visible());
    }

    #[test]
    fn every_tier_meets_the_minimum_pointer_target() {
        // §4.2: 24x24 logical at every scale. Compact sits exactly on the floor, which is why the
        // allocation in `knob` takes `max(MIN_TARGET)` rather than trusting the diameter.
        for size in [Size::Primary, Size::Standard, Size::Compact] {
            assert!(
                size.diameter() >= MIN_TARGET,
                "{size:?} is below the pointer minimum"
            );
        }
    }

    #[test]
    fn only_compact_may_defer_its_value() {
        assert!(Size::Primary.value_always_visible());
        assert!(Size::Standard.value_always_visible());
        assert!(!Size::Compact.value_always_visible());
    }

    #[test]
    fn an_instant_edit_is_fully_bracketed() {
        // A double-click reset that opens a gesture and never closes it leaves a host's automation
        // lane latched. This is the case that is easy to get wrong and silent when wrong.
        let outcome = ControlOutcome::instant();
        assert!(outcome.gesture_started && outcome.changed && outcome.gesture_ended);
    }

    #[test]
    fn a_default_outcome_reports_nothing() {
        assert!(!ControlOutcome::default().any());
    }

    #[test]
    fn the_arc_spans_270_degrees_from_the_bottom_gap() {
        // The gap must be centred at the bottom, or the control reads as tilted. Start and end
        // should sit symmetrically about straight down.
        let start = ARC_START;
        let end = ARC_START + ARC_SWEEP;
        let down = std::f32::consts::FRAC_PI_2;
        let before = start - down;
        let after = (end - down) - std::f32::consts::TAU;
        assert!(
            (before + after).abs() < 1e-4,
            "the 90 degree gap is not centred: {before} vs {after}"
        );
    }

    #[test]
    fn the_arc_reaches_both_ends() {
        let centre = Pos2::new(0.0, 0.0);
        let full = arc_points(centre, 10.0, 0.0, 1.0);
        assert!(full.len() > 24, "too coarse to read as a curve");

        // Zero-length arcs must still produce a drawable line rather than panicking on an empty
        // point list, because a parameter resting at its minimum is the common case.
        let empty = arc_points(centre, 10.0, 0.0, 0.0);
        assert!(empty.len() >= 2);
    }

    #[test]
    fn merging_outcomes_never_loses_half_a_gesture() {
        let mut outcome = ControlOutcome {
            gesture_started: true,
            ..Default::default()
        };
        outcome |= ControlOutcome {
            gesture_ended: true,
            ..Default::default()
        };
        assert!(outcome.gesture_started && outcome.gesture_ended);
        assert!(!outcome.changed);
    }

    /// **As small as the widest option allows, and not a point wider** (the owner, 2026-09-23:
    /// *"Make buttons the same size as the largest one must be, but as small as possible"*). The
    /// row is handed a whole 400-point panel and must not stretch into it, and every cell is the
    /// widest option's floor, so the cells stay equal.
    #[test]
    fn a_segmented_control_is_as_wide_as_its_widest_option_and_no_wider() {
        const OPTIONS: &[&str] = &["VCA", "VCF", "VCA + VCF"];
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let mut expected = 0.0_f32;
        let mut cells = Vec::new();
        for _ in 0..2 {
            cells = segmented_cells(&ctx, &mut |ui| {
                expected = segment_cell_width(ui, OPTIONS);
                let mut selected = 0;
                segmented(
                    ui,
                    &crate::LIGHT,
                    "Gate mode",
                    OPTIONS,
                    &mut selected,
                    None,
                    None,
                    &described(OPTIONS),
                );
            });
        }
        assert_eq!(cells.len(), OPTIONS.len(), "{cells:?}");
        for cell in &cells {
            assert!(
                (cell.width() - expected).abs() < 0.5,
                "a cell is {:.1} wide; the widest option needs {expected:.1}",
                cell.width()
            );
        }
        let row = cells.last().unwrap().right() - cells[0].left();
        assert!(
            row < 300.0,
            "the row stretched to {row:.0} of the 400 it was handed"
        );
    }

    /// Controls that stand together share one cell width, so a stack is even rather than ragged,
    /// and no cell is ever narrower than its own control's widest option.
    #[test]
    fn a_stack_of_segmented_controls_shares_the_widest_cell() {
        const SHORT: &[&str] = &["Off", "On"];
        const LONG: &[&str] = &["Mod oscillator", "Gate 1"];
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let mut shared = 0.0_f32;
        let mut cells = Vec::new();
        for _ in 0..2 {
            cells = cells_in(&ctx, 400.0, &mut |ui| {
                shared = shared_cell_width(ui, &[SHORT, LONG]);
                let (mut a, mut b) = (0, 0);
                segmented_with_cell(
                    ui,
                    &crate::LIGHT,
                    "A",
                    SHORT,
                    &mut a,
                    None,
                    None,
                    shared,
                    &described(SHORT),
                );
                segmented_with_cell(
                    ui,
                    &crate::LIGHT,
                    "B",
                    LONG,
                    &mut b,
                    None,
                    None,
                    shared,
                    &described(LONG),
                );
            });
        }
        assert_eq!(cells.len(), 4, "{cells:?}");
        assert!(shared > segment_cell_width_for_test(&ctx, SHORT) + 1.0);
        for cell in &cells {
            assert!((cell.width() - shared).abs() < 0.5, "{cells:?}");
        }
    }

    fn segment_cell_width_for_test(ctx: &egui::Context, options: &[&str]) -> f32 {
        let mut width = 0.0;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            width = segment_cell_width(ui, options);
        });
        output.textures_delta.clear();
        width
    }

    /// A toggle is its label and its padding and nothing else — no longer stretched to 120 — and
    /// **marking it moves nothing**: the modulation dot sits in the corner padding, so a toggle
    /// does not grow under the pointer when a host starts modulating it, and the dot stays clear of
    /// the label.
    #[test]
    fn a_toggle_is_as_small_as_its_label_and_marking_it_moves_nothing() {
        const LABEL: &str = "Clock loop";
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let mut widths = Vec::new();
        let mut floor = 0.0_f32;
        let mut text = Vec2::ZERO;
        // Warmed first, both states measured after: the font atlas settles over the first frames and
        // a text width taken before it has would differ between the two runs for that reason alone.
        for marked in [false, false, true] {
            let mut frame = Rect::NOTHING;
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(600.0, 400.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        // Handed 300 points, far more than the label needs.
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(
                            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(300.0, 200.0)),
                        ));
                        floor = toggle_min_width(&child, LABEL);
                        text = child
                            .painter()
                            .layout_no_wrap(
                                LABEL.to_owned(),
                                egui::TextStyle::Button.resolve(child.style()),
                                egui::Color32::WHITE,
                            )
                            .size();
                        let mut on = false;
                        toggle(&mut child, &crate::LIGHT, LABEL, &mut on, marked, "");
                        frame = child.min_rect();
                    },
                );
                output.textures_delta.clear();
            }
            widths.push(frame.width());
            // The dot, in the corner, against the label's box, centred on the frame's left edge.
            let label = Rect::from_min_size(
                egui::pos2(frame.left() + SPACE_3, frame.center().y - text.y / 2.0),
                text,
            );
            let dot = Rect::from_center_size(
                frame.right_top() + Vec2::new(-MARK_INSET, MARK_INSET),
                Vec2::splat(HAIRLINE * 4.0),
            );
            assert!(
                !dot.intersects(label),
                "the dot {dot:?} overlaps the label {label:?}"
            );
        }
        assert!(
            (widths[2] - floor).abs() < 0.5,
            "{widths:?} against {floor}"
        );
        assert!(
            floor < 120.0,
            "a short label must not be stretched: {floor}"
        );
        assert_eq!(widths[1], widths[2], "marking changed the width");
    }

    /// The scoped forms are the explicit ones by another road, and they end with their scope: a
    /// control drawn after the stack is its own width again.
    #[test]
    fn a_scoped_stack_shares_its_width_and_ends_with_its_scope() {
        const SHORT: &[&str] = &["Off", "On"];
        const LONG: &[&str] = &["Mod oscillator", "Gate 1"];
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let (mut shared, mut own) = (0.0_f32, 0.0_f32);
        let mut cells = Vec::new();
        for _ in 0..3 {
            cells = cells_in(&ctx, 400.0, &mut |ui| {
                shared = shared_cell_width(ui, &[SHORT, LONG]);
                own = segment_cell_width(ui, SHORT);
                let (mut a, mut b, mut c) = (0, 0, 0);
                segmented_stack(ui, shared, |ui| {
                    segmented(
                        ui,
                        &crate::LIGHT,
                        "A",
                        SHORT,
                        &mut a,
                        None,
                        None,
                        &described(SHORT),
                    );
                    segmented(
                        ui,
                        &crate::LIGHT,
                        "B",
                        LONG,
                        &mut b,
                        None,
                        None,
                        &described(LONG),
                    );
                });
                segmented(
                    ui,
                    &crate::LIGHT,
                    "C",
                    SHORT,
                    &mut c,
                    None,
                    None,
                    &described(SHORT),
                );
            });
        }
        assert_eq!(cells.len(), 6, "{cells:?}");
        for cell in &cells[..4] {
            assert!((cell.width() - shared).abs() < 0.5, "{cells:?}");
        }
        for cell in &cells[4..] {
            assert!(
                (cell.width() - own).abs() < 0.5,
                "the scope leaked: {cells:?}"
            );
        }
    }

    /// **Six pictures are two rows of three** (the owner's choice for the six-shape LFOs), one
    /// control: every cell is named, the grid is one keyboard-cursor target, and the arrows walk
    /// the options in order across the rows.
    #[test]
    fn six_waveforms_are_two_rows_of_three_and_one_control() {
        const SIX: [(Wave, &str); 6] = [
            (Wave::Sine, "Sine"),
            (Wave::Triangle, "Triangle"),
            (Wave::RampUp, "Ramp up"),
            (Wave::RampDown, "Ramp down"),
            (Wave::Square, "Square"),
            (Wave::Random, "Sample & hold"),
        ];
        assert_eq!(wave_columns(6), 3);
        assert_eq!(wave_columns(5), 5);
        assert_eq!(wave_columns(3), 3);
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let mut cells = Vec::new();
        for _ in 0..2 {
            cells = cells_in(&ctx, 400.0, &mut |ui| {
                let mut selected = 0;
                segmented_waves(
                    ui,
                    &crate::LIGHT,
                    "Shape",
                    &SIX,
                    &mut selected,
                    None,
                    None,
                    None,
                    &described(&SIX),
                );
            });
        }
        assert_eq!(cells.len(), 6, "{cells:?}");
        let rows: Vec<f32> = cells.iter().map(|cell| cell.top()).collect();
        assert!(
            (rows[0] - rows[2]).abs() < 0.5 && (rows[3] - rows[5]).abs() < 0.5,
            "{cells:?}"
        );
        assert!(
            rows[3] > rows[0] + MIN_TARGET - 0.5,
            "the second row is below the first"
        );
        assert!(
            (cells[0].left() - cells[3].left()).abs() < 0.5,
            "the rows are aligned"
        );

        // Keyboard: the arrows walk in order, so Right from the third cell reaches the fourth,
        // which is the first of the second row.
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(400.0, 300.0))
            .build_ui(|ui| {
                let mut selected = ui
                    .data(|d| d.get_temp::<usize>(egui::Id::new("sel")))
                    .unwrap_or(2);
                segmented_waves(
                    ui,
                    &crate::LIGHT,
                    "Shape",
                    &SIX,
                    &mut selected,
                    None,
                    None,
                    None,
                    &described(&SIX),
                );
                ui.data_mut(|d| d.insert_temp(egui::Id::new("sel"), selected));
            });
        harness.run();
        {
            use egui::accesskit::Role;
            use kittest::Queryable;
            for (_, name) in SIX {
                harness.get_by_role_and_label(Role::RadioButton, &format!("Shape: {name}"));
            }
            harness
                .get_by_role_and_label(Role::RadioButton, "Shape: Ramp up")
                .focus();
        }
        harness.run();
        // Without the keyboard cursor running, a focused control takes Command+arrow.
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::ArrowRight);
        harness.run();
        let selected = harness
            .ctx
            .data(|d| d.get_temp::<usize>(egui::Id::new("sel")))
            .unwrap();
        assert_eq!(
            selected, 3,
            "Right from the end of the first row goes to the second row"
        );
    }

    /// **A row with nothing selected is entered at the end the arrow moves away from**: Right from
    /// nothing is the first cell, Left the last. Before, both landed on the last — `none` is one past
    /// it, and each direction clamped there.
    #[test]
    fn an_arrow_enters_a_row_with_nothing_selected_at_its_near_end() {
        const OPTIONS: [&str; 3] = ["I", "II", "I + II"];
        for (key, expected) in [(egui::Key::ArrowRight, 0usize), (egui::Key::ArrowLeft, 2)] {
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(400.0, 120.0))
                .build_ui(|ui| {
                    let mut selected = ui
                        .data(|d| d.get_temp::<usize>(egui::Id::new("sel")))
                        .unwrap_or(OPTIONS.len());
                    segmented(
                        ui,
                        &crate::LIGHT,
                        "Mode",
                        &OPTIONS,
                        &mut selected,
                        None,
                        None,
                        &["First.", "Second.", "Third."],
                    );
                    ui.data_mut(|d| d.insert_temp(egui::Id::new("sel"), selected));
                });
            harness.run();
            {
                use egui::accesskit::Role;
                use kittest::Queryable;
                harness
                    .get_by_role_and_label(Role::RadioButton, "Mode: II")
                    .focus();
            }
            harness.run();
            harness.key_press_modifiers(egui::Modifiers::COMMAND, key);
            harness.run();
            let selected = harness
                .ctx
                .data(|d| d.get_temp::<usize>(egui::Id::new("sel")))
                .unwrap();
            assert_eq!(selected, expected, "{key:?} from nothing selected");
        }
    }

    /// The unlabelled grid paints no label line — it starts where the labelled one's label does —
    /// and every cell still carries the full name.
    #[test]
    fn an_unlabelled_wave_grid_saves_its_label_line_and_keeps_its_names() {
        const SHAPES: [(Wave, &str); 2] = [(Wave::Sine, "Sine"), (Wave::Square, "Square")];
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let top = |unlabelled: bool| {
            let mut cells = Vec::new();
            for _ in 0..2 {
                cells = cells_in(&ctx, 400.0, &mut |ui| {
                    let mut selected = 0;
                    if unlabelled {
                        segmented_waves_unlabelled(
                            ui,
                            &crate::LIGHT,
                            "LFO 1 shape",
                            &SHAPES,
                            &mut selected,
                            None,
                            None,
                            &described(&SHAPES),
                        );
                    } else {
                        segmented_waves(
                            ui,
                            &crate::LIGHT,
                            "LFO 1 shape",
                            &SHAPES,
                            &mut selected,
                            None,
                            None,
                            None,
                            &described(&SHAPES),
                        );
                    }
                });
            }
            cells.first().expect("cells are painted").top()
        };
        assert!(
            top(true) + 10.0 < top(false),
            "no label line above the unlabelled cells"
        );

        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(300.0, 100.0))
            .build_ui(|ui| {
                let mut selected = 0;
                segmented_waves_unlabelled(
                    ui,
                    &crate::LIGHT,
                    "LFO 1 shape",
                    &SHAPES,
                    &mut selected,
                    None,
                    None,
                    &described(&SHAPES),
                );
            });
        harness.run();
        use egui::accesskit::Role;
        use kittest::Queryable;
        harness.get_by_role_and_label(Role::RadioButton, "LFO 1 shape: Sine");
        harness.get_by_role_and_label(Role::RadioButton, "LFO 1 shape: Square");
    }

    /// A picture switch is a wave cell, named for the parameter, and toggles like a switch.
    #[test]
    fn a_picture_switch_is_named_and_toggles() {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(200.0, 100.0))
            .build_ui(|ui| {
                let mut on = ui
                    .data(|d| d.get_temp::<bool>(egui::Id::new("on")))
                    .unwrap_or(false);
                toggle_wave(
                    ui,
                    &crate::LIGHT,
                    Wave::RampUp,
                    "LFO saw",
                    &mut on,
                    false,
                    "",
                );
                ui.data_mut(|d| d.insert_temp(egui::Id::new("on"), on));
            });
        harness.run();
        {
            use egui::accesskit::Role;
            use kittest::Queryable;
            let node = harness.get_by_role_and_label(Role::CheckBox, "LFO saw");
            assert!((node.rect().width() - wave_cell().x).abs() < 0.5);
            node.click();
        }
        harness.run();
        assert_eq!(
            harness
                .ctx
                .data(|d| d.get_temp::<bool>(egui::Id::new("on"))),
            Some(true)
        );
    }

    /// A stack of toggles is as wide as its widest label, every one of them.
    #[test]
    fn a_stack_of_toggles_shares_the_widest_label() {
        const LABELS: &[&str] = &[
            "Trigger from Key",
            "Trigger from Clock",
            "Trigger from Step",
        ];
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let mut shared = 0.0_f32;
        let mut cells = Vec::new();
        for _ in 0..2 {
            cells = cells_in(&ctx, 400.0, &mut |ui| {
                shared = shared_toggle_width(ui, LABELS);
                for label in LABELS {
                    let mut on = false;
                    toggle_sized(ui, &crate::LIGHT, label, label, &mut on, false, "", shared);
                }
            });
        }
        assert_eq!(cells.len(), LABELS.len(), "{cells:?}");
        for cell in &cells {
            assert!((cell.width() - shared).abs() < 0.5, "{cells:?}");
        }
    }

    #[test]
    fn the_wheel_is_off_unless_a_caller_asks() {
        // §7.1: "Do not rely on scroll-wheel editing by default." The default has to be the safe
        // one, because the unsafe one is silent — a user scrolling a list would not see the edit
        // until they wondered why the patch changed.
        assert_eq!(Wheel::default(), Wheel::Off);
    }

    #[test]
    fn a_bipolar_view_is_built_by_intent() {
        let param = ParamView::new(
            "Filter envelope",
            "+40%",
            "How much the envelope opens the filter.",
        )
        .default_at(0.7)
        .bipolar();
        assert!(param.bipolar);
        assert!((param.default - 0.7).abs() < f64::EPSILON);
        assert!(!param.read_only);
    }

    /// A toggle is never narrower than its label.
    ///
    /// The same paint bug as the segmented cell below: the label is painted from a fixed inset and
    /// is not clipped, so a frame of `min(available, 120)` let *Complex keyboard tracking* run
    /// through its own border while every rect measurement said it fitted. Found by looking at
    /// `mxm-mono-08`'s oscillator card.
    #[test]
    fn a_toggle_is_never_narrower_than_its_label() {
        const LABEL: &str = "Complex keyboard tracking";

        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);

        let mut frame = egui::Rect::NOTHING;
        let mut text = 0.0_f32;
        let mut floor = 0.0_f32;
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(600.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(300.0, 200.0)),
                    ));
                    floor = toggle_min_width(&child, LABEL);
                    text = child
                        .painter()
                        .layout_no_wrap(
                            LABEL.to_owned(),
                            egui::TextStyle::Button.resolve(child.style()),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    let mut on = false;
                    toggle(&mut child, &crate::LIGHT, LABEL, &mut on, true, "");
                    frame = child.min_rect();
                },
            );
            output.textures_delta.clear();
        }

        assert!(
            (frame.width() - floor).abs() < 0.5,
            "the frame is {:.1} wide but the label floor is {floor:.1}",
            frame.width()
        );
        // The painted label starts SPACE_3 in and must end before the trailing pad.
        assert!(
            SPACE_3 + text + SPACE_3 <= frame.width() + 0.5,
            "label of {text:.1} points does not fit inside a {:.1} frame",
            frame.width()
        );
    }

    #[test]
    fn a_compact_toggle_is_square_at_the_pointer_floor() {
        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);
        let mut rect = Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0))),
                ..Default::default()
            },
            |ui| {
                let mut on = false;
                toggle_compact(
                    ui,
                    &crate::LIGHT,
                    "M",
                    "Mute",
                    &mut on,
                    false,
                    "Silence this slot.",
                );
                rect = ui.min_rect();
            },
        );
        output.textures_delta.clear();
        assert!((rect.width() - MIN_TARGET).abs() < 0.5, "{rect:?}");
        assert!((rect.height() - MIN_TARGET).abs() < 0.5, "{rect:?}");
    }

    /// A segmented cell is never narrower than the word in it.
    ///
    /// **This is a paint bug, not a layout bug, which is why nothing caught it for so long.** The
    /// cell text is painted centred at `rect.center()` and is not clipped, so a cell narrower than
    /// its label spills across its neighbours and outside the control — while `min_rect` stays
    /// obediently inside the space it was given, so every measurement said the row was fine. Found
    /// by putting `mxm-mono-02` in a narrow card and looking at *Envelope*.
    ///
    /// The row is therefore allowed to **overflow its container** instead: that is visible, it is
    /// measurable, and it points at the caller's card, which is the thing that should be wider.
    #[test]
    fn a_segment_is_never_narrower_than_its_word() {
        const OPTIONS: &[&str] = &["Hold", "Envelope", "Gate"];

        let ctx = egui::Context::default();
        crate::typography::apply(&ctx);

        let mut cells: Vec<egui::Rect> = Vec::new();
        let mut floor = 0.0_f32;
        for _ in 0..2 {
            cells.clear();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(600.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    // Deliberately far too narrow for "Envelope".
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(
                        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(120.0, 200.0)),
                    ));
                    floor = segment_min_width(&child, OPTIONS);
                    let mut selected = 1;
                    segmented(
                        &mut child,
                        &crate::LIGHT,
                        "VCA",
                        OPTIONS,
                        &mut selected,
                        None,
                        None,
                        &described(OPTIONS),
                    );
                    cells.push(child.min_rect());
                },
            );
            output.textures_delta.clear();
        }

        let row = cells.last().copied().expect("a frame ran");
        assert!(
            floor > 120.0,
            "the three options need {floor:.0} points, which should exceed the 120 the card gave"
        );
        assert!(
            row.width() >= floor - 0.5,
            "the control was drawn {:.0} points wide but its words need {floor:.0};              the cells are narrower than their labels and the text paints outside them",
            row.width()
        );
    }

    // ---- The keyboard cursor over real controls: pointer targeting and the value's step law ----

    /// Drives real controls inside a card and parameter scopes with the real cursor, one frame at
    /// a time, the way an editor does: `navigation::run` first, then the controls.
    struct Cursor {
        ctx: egui::Context,
        nav: crate::navigation::State,
    }

    impl Cursor {
        const SCREEN: egui::Rect =
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(400.0, 300.0));

        fn new() -> Self {
            Self {
                ctx: egui::Context::default(),
                nav: crate::navigation::State::default(),
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>, mut draw: impl FnMut(&mut Ui)) {
            let input = egui::RawInput {
                screen_rect: Some(Self::SCREEN),
                events,
                ..Default::default()
            };
            let nav = &mut self.nav;
            let mut output = self.ctx.run_ui(input, |ui| {
                let _ = crate::navigation::run(ui.ctx(), nav, &[0], &[(0, Self::SCREEN)], false);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| crate::navigation::card(ui, 0, |ui| draw(ui)));
            });
            output.textures_delta.clear();
        }

        /// The rectangle a parameter's control registered last frame.
        fn rect_of(&self, key: &str) -> egui::Rect {
            crate::navigation::spots(&self.ctx)
                .into_iter()
                .find(|spot| spot.key == key)
                .unwrap_or_else(|| panic!("{key} registered"))
                .rect
        }
    }

    fn key_event(key: Key, pressed: bool, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers: Modifiers::NONE,
        }
    }

    fn click_at(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    /// A knob named `key`, reporting what it asked the host for.
    fn cursor_knob(
        ui: &mut Ui,
        key: &str,
        value: f64,
        law: Option<&dyn NextValue>,
        asked: &mut Option<f64>,
    ) {
        crate::navigation::at(ui, key, |ui| {
            let mut view = ParamView::new(key, "text", "A parameter.").stepping(Steps {
                fine_up: 0.01,
                fine_down: 0.01,
                coarse_up: 0.1,
                coarse_down: 0.1,
            });
            if let Some(law) = law {
                view = view.stepping_by(law);
            }
            let mut normalised = value;
            let mut entry = None;
            let outcome = knob(
                ui,
                &crate::theme::LIGHT,
                &view,
                &mut normalised,
                Size::Standard,
                Size::Standard.diameter(),
                &mut entry,
                Wheel::Off,
            );
            if outcome.changed {
                *asked = Some(normalised);
            }
        });
    }

    /// A law that depends on where a press starts: coarse doubles or halves, fine adds or takes
    /// 0.3. A fixed per-frame magnitude can express neither, which is what these tests need.
    struct Doubling;

    impl NextValue for Doubling {
        fn next_value(&self, normalised: f64, press: Press) -> f64 {
            match (press.coarse, press.up) {
                (true, true) => normalised * 2.0,
                (true, false) => normalised / 2.0,
                (false, true) => normalised + 0.3,
                (false, false) => normalised - 0.3,
            }
            .clamp(0.0, 1.0)
        }
    }

    /// **The owner's report of 2026-09-23, through real controls**: the knob the pointer touched is
    /// the one the next arrow edits, whichever the cursor was on before.
    #[test]
    fn the_knob_the_pointer_clicked_is_the_one_the_next_arrow_edits() {
        let mut rig = Cursor::new();
        let (mut asked_a, mut asked_b) = (None, None);
        let draw = |ui: &mut Ui, asked_a: &mut Option<f64>, asked_b: &mut Option<f64>| {
            ui.horizontal(|ui| {
                cursor_knob(ui, "a", 0.5, None, asked_a);
                cursor_knob(ui, "b", 0.5, None, asked_b);
            });
        };
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| draw(ui, &mut asked_a, &mut asked_b));
        }
        assert_eq!(
            rig.nav.parameter(),
            Some("a"),
            "the cursor starts on the first"
        );

        let b = rig.rect_of("b").center();
        rig.frame(click_at(b), |ui| draw(ui, &mut asked_a, &mut asked_b));
        rig.frame(Vec::new(), |ui| draw(ui, &mut asked_a, &mut asked_b));
        assert_eq!(rig.nav.parameter(), Some("b"), "the click moved the cursor");

        (asked_a, asked_b) = (None, None);
        rig.frame(
            vec![
                key_event(Key::ArrowUp, true, false),
                key_event(Key::ArrowUp, false, false),
            ],
            |ui| draw(ui, &mut asked_a, &mut asked_b),
        );
        assert_eq!(asked_a, None, "the knob the cursor left is not edited");
        assert!(
            asked_b.is_some_and(|value| (value - 0.6).abs() < 1e-9),
            "the clicked knob moved one coarse step: {asked_b:?}"
        );
    }

    /// A press on a remove deletes the row it sits on, so it never selects.
    #[test]
    fn a_press_on_a_remove_never_moves_the_cursor() {
        let mut rig = Cursor::new();
        let mut asked = None;
        let draw = |ui: &mut Ui, asked: &mut Option<f64>| {
            ui.horizontal(|ui| {
                cursor_knob(ui, "a", 0.5, None, asked);
                crate::navigation::at(ui, "remove", |ui| {
                    let _ = remove_mark(
                        ui,
                        &crate::theme::LIGHT,
                        MIN_TARGET,
                        true,
                        "Remove a",
                        false,
                        "Removes a.",
                    );
                });
            });
        };
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| draw(ui, &mut asked));
        }
        let cross = rig.rect_of("remove").center();
        rig.frame(click_at(cross), |ui| draw(ui, &mut asked));
        rig.frame(Vec::new(), |ui| draw(ui, &mut asked));
        assert_eq!(rig.nav.parameter(), Some("a"));
    }

    /// Two presses in one frame each start where the one before landed, in the order pressed.
    #[test]
    fn same_frame_presses_chain_through_the_law_in_the_order_pressed() {
        let mut rig = Cursor::new();
        let mut asked = None;
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| {
                cursor_knob(ui, "a", 0.1, Some(&Doubling), &mut asked);
            });
        }
        let up = key_event(Key::ArrowUp, true, false);
        rig.frame(vec![up.clone(), up], |ui| {
            cursor_knob(ui, "a", 0.1, Some(&Doubling), &mut asked);
        });
        assert!(
            asked.is_some_and(|value| (value - 0.4).abs() < 1e-9),
            "0.1 doubled twice is 0.4, not two first steps: {asked:?}"
        );

        // Left-then-Right from 0.9 ends at 0.9; Right-then-Left would end at 0.7.
        let mut rig = Cursor::new();
        let mut asked = None;
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| {
                cursor_knob(ui, "a", 0.9, Some(&Doubling), &mut asked);
            });
        }
        rig.frame(
            vec![
                key_event(Key::ArrowLeft, true, false),
                key_event(Key::ArrowRight, true, false),
            ],
            |ui| cursor_knob(ui, "a", 0.9, Some(&Doubling), &mut asked),
        );
        assert!(
            asked.is_some_and(|value| (value - 0.9).abs() < 1e-9),
            "left then right from 0.9 is 0.6 then 0.9: {asked:?}"
        );
    }

    /// **A held key chains from what it last sent, not from a readback the host has not applied.**
    /// The host here applies nothing at all, which is the worst case of a late one.
    #[test]
    fn a_held_key_advances_once_per_repeat_while_the_host_lags() {
        let mut rig = Cursor::new();
        let applied = 0.1;
        let mut asked = None;
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| {
                cursor_knob(ui, "a", applied, Some(&Doubling), &mut asked);
            });
        }
        let mut sent = Vec::new();
        for repeat in [false, true, true] {
            asked = None;
            rig.frame(vec![key_event(Key::ArrowRight, true, repeat)], |ui| {
                cursor_knob(ui, "a", applied, Some(&Doubling), &mut asked);
            });
            sent.push(asked.expect("each repeat sends"));
        }
        for (sent, expected) in sent.iter().zip([0.4, 0.7, 1.0]) {
            assert!(
                (sent - expected).abs() < 1e-9,
                "sent {sent}, expected {expected}"
            );
        }

        // Released, the anchor is gone: a new press starts from what the host reads back.
        rig.frame(vec![key_event(Key::ArrowRight, false, false)], |ui| {
            cursor_knob(ui, "a", applied, Some(&Doubling), &mut asked);
        });
        asked = None;
        rig.frame(
            vec![
                key_event(Key::ArrowRight, true, false),
                key_event(Key::ArrowRight, false, false),
            ],
            |ui| cursor_knob(ui, "a", applied, Some(&Doubling), &mut asked),
        );
        assert!(
            asked.is_some_and(|value| (value - 0.4).abs() < 1e-9),
            "a new gesture starts from the readback: {asked:?}"
        );
    }

    /// `Alt` is the finer layer under the cursor (the owner, 2026-09-24): `Alt` + right moves a
    /// tenth of fine, `Alt` + up moves fine — in each layer up/down is the larger step.
    #[test]
    fn an_alt_arrow_is_the_finer_layer_under_the_cursor() {
        for (key, expected) in [(Key::ArrowRight, 0.501), (Key::ArrowUp, 0.51)] {
            let mut rig = Cursor::new();
            let mut asked = None;
            for _ in 0..3 {
                rig.frame(Vec::new(), |ui| cursor_knob(ui, "a", 0.5, None, &mut asked));
            }
            rig.frame(
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::ALT,
                }],
                |ui| cursor_knob(ui, "a", 0.5, None, &mut asked),
            );
            assert!(
                asked.is_some_and(|value| (value - expected).abs() < 1e-9),
                "Alt + {key:?} landed on {asked:?}, not {expected}"
            );
        }
    }

    /// While a menu is open the cursor is inert, and a knob that kept egui focus from it must not
    /// answer the arrows meant for the menu.
    #[test]
    fn a_focused_knob_is_silent_while_a_popup_has_the_keyboard() {
        let mut rig = Cursor::new();
        let mut asked = None;
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| cursor_knob(ui, "a", 0.5, None, &mut asked));
        }
        let focused = rig.ctx.memory(|m| m.focused());
        assert!(focused.is_some(), "the cursor handed the knob focus");
        egui::Popup::open_id(&rig.ctx, egui::Id::new("a menu"));
        rig.frame(vec![key_event(Key::ArrowRight, true, false)], |ui| {
            cursor_knob(ui, "a", 0.5, None, &mut asked);
        });
        assert_eq!(asked, None, "the arrow was the menu's");
    }

    /// A toggle under the cursor is a two-cell control: right is on, left is off.
    #[test]
    fn a_toggle_under_the_cursor_answers_the_arrows() {
        let mut rig = Cursor::new();
        let mut on = false;
        let draw = |ui: &mut Ui, on: &mut bool| {
            crate::navigation::at(ui, "sync", |ui| {
                let _ = toggle(ui, &crate::theme::LIGHT, "Sync", on, false, "A switch.");
            });
        };
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| draw(ui, &mut on));
        }
        rig.frame(vec![key_event(Key::ArrowRight, true, false)], |ui| {
            draw(ui, &mut on);
        });
        assert!(on, "right is on");
        rig.frame(vec![key_event(Key::ArrowLeft, true, false)], |ui| {
            draw(ui, &mut on);
        });
        assert!(!on, "left is off");
    }

    // ---- The keyboard language, where the pilot runs it (the owner, 2026-10-07) ----

    /// A tap of one key: its press and its release, in one frame.
    fn tap(key: Key) -> Vec<egui::Event> {
        vec![key_event(key, true, false), key_event(key, false, false)]
    }

    fn taps(keys: &[Key]) -> Vec<egui::Event> {
        keys.iter().flat_map(|&key| tap(key)).collect()
    }

    /// Two knobs side by side, under the pilot; returns the rig after the cursor has landed on `a`.
    fn language_rig(asked: &mut [Option<f64>; 2]) -> Cursor {
        let mut rig = Cursor::new();
        crate::pilot::enable(&rig.ctx);
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| two_knobs(ui, asked));
        }
        *asked = [None, None];
        rig
    }

    fn two_knobs(ui: &mut Ui, asked: &mut [Option<f64>; 2]) {
        let [a, b] = asked;
        ui.horizontal(|ui| {
            cursor_knob(ui, "a", 0.5, None, a);
            cursor_knob(ui, "b", 0.5, None, b);
        });
    }

    /// VALUE + arrows change the knob the cursor is on, in its own steps: FINE by default, COARSE
    /// with S; the bare arrows go to the next parameter instead of editing.
    #[test]
    fn under_the_language_value_and_the_arrows_edit_and_bare_arrows_move() {
        let mut asked = [None, None];
        let mut rig = language_rig(&mut asked);
        assert_eq!(rig.nav.parameter(), Some("a"));

        rig.frame(taps(&[Key::W, Key::ArrowUp, Key::Tab]), |ui| {
            two_knobs(ui, &mut asked)
        });
        assert!(
            asked[0].is_some_and(|value| (value - 0.51).abs() < 1e-9),
            "VALUE + ↑ is a fine step: {asked:?}"
        );

        asked = [None, None];
        rig.frame(tap(Key::ArrowRight), |ui| two_knobs(ui, &mut asked));
        assert_eq!(rig.nav.parameter(), Some("b"), "→ is the next parameter");
        assert_eq!(asked, [None, None], "a bare arrow edits nothing");

        rig.frame(taps(&[Key::W, Key::S, Key::ArrowUp, Key::Tab]), |ui| {
            two_knobs(ui, &mut asked)
        });
        assert!(
            asked[1].is_some_and(|value| (value - 0.6).abs() < 1e-9),
            "VALUE COARSE + ↑ is a coarse step: {asked:?}"
        );
    }

    /// BACK cancels the gesture back to where it began, and DELETE puts the default back.
    #[test]
    fn under_the_language_back_cancels_and_delete_resets() {
        let mut asked = [None, None];
        let mut rig = language_rig(&mut asked);
        rig.frame(taps(&[Key::W, Key::ArrowUp, Key::ArrowUp]), |ui| {
            two_knobs(ui, &mut asked)
        });
        assert!(asked[0].is_some_and(|value| (value - 0.52).abs() < 1e-9));
        rig.frame(tap(Key::Escape), |ui| two_knobs(ui, &mut asked));
        assert!(
            asked[0].is_some_and(|value| (value - 0.5).abs() < 1e-9),
            "BACK sent the value it began at: {asked:?}"
        );

        asked = [None, None];
        rig.frame(tap(Key::Delete), |ui| two_knobs(ui, &mut asked));
        assert_eq!(asked[0], Some(0.0), "DELETE is the default");
    }

    /// Under the language, **each cell of a segmented parameter is a stop for the arrows** (the
    /// owner, 2026-10-07: "It fits what I see on the screen"): a bare arrow goes to the next cell
    /// without choosing it, OPEN chooses the cell the cursor is on, and VALUE + an arrow still
    /// chooses the next one from the chosen.
    #[test]
    fn under_the_language_the_arrows_stop_on_each_cell_and_open_chooses_it() {
        let mut rig = Cursor::new();
        crate::pilot::enable(&rig.ctx);
        let mut selected = 0usize;
        let draw = |ui: &mut Ui, selected: &mut usize| {
            crate::navigation::at(ui, "length", |ui| {
                segmented(
                    ui,
                    &crate::theme::LIGHT,
                    "Length",
                    &["2 steps", "3 steps", "4 steps", "5 steps"],
                    selected,
                    None,
                    Some(0),
                    &["Two steps.", "Three steps.", "Four steps.", "Five steps."],
                );
            });
        };
        for _ in 0..3 {
            rig.frame(Vec::new(), |ui| draw(ui, &mut selected));
        }
        rig.frame(tap(Key::ArrowRight), |ui| draw(ui, &mut selected));
        rig.frame(Vec::new(), |ui| draw(ui, &mut selected));
        assert_eq!(rig.nav.cell(), 1, "right is the next cell");
        assert_eq!(selected, 0, "a bare arrow chooses nothing");
        rig.frame(tap(Key::Enter), |ui| draw(ui, &mut selected));
        assert_eq!(selected, 1, "OPEN chooses the cell the cursor is on");
        rig.frame(taps(&[Key::W, Key::ArrowRight, Key::Tab]), |ui| {
            draw(ui, &mut selected)
        });
        assert_eq!(
            selected, 2,
            "VALUE + right is the next cell from the chosen one"
        );
    }
}
