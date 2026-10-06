//! A card body as data: one description, measured and drawn from the same tree.
//!
//! `plans/plan-layout-tree.md`. The owner, 2026-09-24: *"all these cards have easily calculatable
//! minimum and maximum sizes. Because we know all the components on them. And the ones that are
//! not visible have simple rules for their sizes."* A body written as drawing code has no structure
//! until it runs, so the only way to learn its size was to run it — typed floors policed by
//! squeezing, and a hidden egui context for heights. Here the body is a [`Node`] tree built each
//! frame from parameters and editor state, and the one tree is walked twice, the way a browser walks
//! its DOM: once to add sizes up, once to draw. The two cannot disagree.
//!
//! # The rules, each written once
//!
//! - A **leaf** is one control ([`Kind`]). Its size comes from the control's own size function in
//!   [`crate::control`], from the fonts and style alone. A leaf that **fills** takes the width it is
//!   offered; every other leaf is drawn at its own width.
//! - [`Node::Row`]: side by side, top-aligned, `gap` apart; the sum; the tallest child. A
//!   [`Height::Row`] child takes the row's height. A [`switch_beside_knob`] stands
//!   [`switch_gap`] from the circle of the knob before it instead.
//! - [`Node::Columns`]: `ui.columns` as data — equal columns, children centred in theirs.
//! - [`Node::Grid`] and [`Node::Wrap`]: as many per line as the width holds.
//! - [`Node::Stack`]: one under another, `gap` apart; the widest child.
//! - [`Node::Group`]: [`crate::shell::group`] — a hairline, [`GROUP_INSET`], `SPACE_2` inside.
//! - [`Node::Share`]: every toggle, or every segmented cell, in a subtree at one width —
//!   [`crate::control::toggle_stack`] and [`crate::control::segmented_stack`] as data.
//! - [`Node::Reserve`]: draws one child and is as large as the largest of its alternatives — the
//!   rule for what is not visible.
//! - [`Node::Disclosure`]: its header, and its body as it stands; the card reserves the body open
//!   ([`Node::reserved_height`]), so opening it never grows the card.
//! - [`Node::Pad`], [`Node::Beside`], [`Node::Center`], [`Node::Disabled`], [`Node::Space`],
//!   [`Node::Separator`].
//!
//! **Below its minimum a tree does not shrink.** [`show`] lays a body out at no less than its
//! narrowest; offered less, it overflows visibly, which is the failure design system §4.3 asks for.
//!
//! **Heights are measured in the `Ui` the body is drawn in.** Some size rules read the item
//! spacing, and a card body's (`SPACE_3`) is not the panel's: measure with
//! [`crate::shell::body_ui`], or inside the body itself.

use std::fmt::Debug;
use std::hash::Hash;

use egui::{Align, Layout, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::control::{self, Size};
use crate::space::{HAIRLINE, RADIUS, SPACE_2, SPACE_3};
use crate::theme::Tokens;

/// The space between siblings in a card body: its rhythm (§4.1).
pub const GAP: f32 = SPACE_3;
/// A group's padding plus its hairline, on each side.
pub const GROUP_INSET: f32 = SPACE_3 + HAIRLINE;

/// How far a [`switch_beside_knob`] stands from the circle of a knob of `size`: the margin of the
/// knob column around it, so with room the switch stands against the column — 18 px, where it was 34
/// (the owner, 2026-09-25: the quarter note was *"too far from the knob it controls ... half that
/// distance"*) — and the same distance from a circle whose column is narrower at a card's floor.
#[must_use]
pub fn switch_gap(size: Size) -> f32 {
    (control::knob_column(size) - size.diameter()) / 2.0
}

/// A text leaf's font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Font {
    Body,
    Caption,
    Button,
    Heading,
    Monospace,
    /// The Body family at an explicit size — the `.size(11.0)` captions some editors draw.
    Sized(f32),
}

/// How a text leaf uses its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Wraps at spaces within the width offered; fills.
    Wrap,
    /// One line at its own width.
    Line,
    /// One line across the width offered, elided; fills.
    Truncate,
}

/// A `Custom` leaf's height.
#[derive(Clone, Copy, Debug)]
pub enum Height {
    Fixed(f32),
    /// A function of the width the leaf is drawn at.
    Of(fn(f32) -> f32),
    /// The height of the row it stands in, at least this.
    Row(f32),
}

/// What a leaf is, in the terms its size depends on. The caller's key says which parameter it
/// draws; this says only what it occupies.
#[derive(Clone, Debug)]
pub enum Kind {
    /// [`control::knob`], drawn in a column at least `column` wide.
    Knob {
        name: String,
        widest: String,
        size: Size,
        column: f32,
    },
    /// [`control::slider`]; fills.
    Slider {
        label: String,
        quiet: bool,
        widest: String,
    },
    /// [`control::slider_vertical`] at a caller's height; across the width offered when `fills`.
    VerticalSlider {
        label: String,
        widest: String,
        height: f32,
        fills: bool,
    },
    /// [`control::segmented`] and its variants: the painted label, beside a knob or above.
    Segmented {
        label: String,
        options: Vec<String>,
        beside: Option<Size>,
    },
    /// [`control::segmented_waves`] and its variants.
    Waves {
        label: Option<String>,
        count: usize,
        marks: Vec<String>,
        beside: Option<Size>,
    },
    /// [`control::toggle`] and its labelled and sized forms: the painted label.
    Toggle { label: String },
    /// [`control::toggle_compact`].
    CompactToggle,
    /// [`control::toggle_wave`].
    PictureToggle,
    /// A tempo sync's quarter note: [`control::toggle_wave`] with [`control::Wave::QuarterNote`],
    /// square at a wave cell's height.
    SyncToggle,
    /// [`control::selector`] (or [`control::selector_caption`] when `caption`); fills its row, or
    /// stands in a region `width` wide.
    Selector {
        label: String,
        options: Vec<String>,
        caption: bool,
        width: Option<f32>,
    },
    /// An `egui::Button` at least `min`; fills when it truncates.
    Button {
        label: String,
        min: Vec2,
        fills: bool,
    },
    /// `ui.checkbox`.
    Checkbox { label: String },
    /// An `egui::ComboBox` showing `selected`.
    Combo { selected: String },
    /// An `egui::Slider` whose value box holds `widest`, with an optional label.
    EguiSlider {
        widest: String,
        label: Option<String>,
    },
    /// [`control::remove_mark`], its square allocated whether shown or not.
    RemoveMark { size: f32 },
    /// Text.
    Text {
        text: String,
        font: Font,
        flow: Flow,
    },
    /// A plugin's visual or a shared composite that states its own size.
    Custom {
        min_width: f32,
        height: Height,
        fills: bool,
    },
}

/// A subtree's shared width, as data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Share {
    /// Every toggle in it at the widest one's width.
    Toggles,
    /// Every segmented control in it at one cell — the widest any of them needs.
    Cells,
}

/// Where [`Node::Beside`] puts its child.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    /// Centred on the circle of a knob of this size beside it.
    Knob(Size),
    /// [`Anchor::Knob`], and in a row straight after its knob — or a knob row, whose last knob it
    /// belongs to — [`switch_gap`] from that knob's circle rather than a gap from its column, and
    /// never inside the column: the row reads the column's drawn width, so the distance is the same
    /// with room and at a card's floor. [`switch_beside_knob`].
    KnobSwitch(Size),
    /// On a labelled segmented control's cell line.
    CellLine,
}

/// A disclosure's header.
#[derive(Clone, Debug)]
pub enum Header {
    /// [`crate::shell::disclosure`]: a hairline and a labelled toggle.
    Toggle { label: String, description: String },
    /// An egui collapsing header, its title in the caption style when `caption`.
    Collapsing { title: String, caption: bool },
}

/// One card body, as data. `K` is the caller's key for a leaf — which parameter it draws — and is
/// what the drawing callback receives.
#[derive(Clone, Debug)]
pub enum Node<K> {
    Leaf(K, Kind),
    /// Room and nothing in it.
    Space(Vec2),
    /// [`crate::shell::separator`]; fills.
    Separator,
    Row {
        gap: f32,
        children: Vec<Node<K>>,
    },
    Columns {
        gap: f32,
        /// A cap: `ui.columns` inside a width of at most `n × column` — each column that share of
        /// it, gaps included. The columns take what they are offered up to the cap and shrink below
        /// it as far as their widest child allows, as `set_max_width(total.min(available))` let the
        /// hand-drawn knob rows do. `None`: the widest child and the gaps, exactly.
        column: Option<f32>,
        /// Across the whole width offered instead, however wide.
        stretch: bool,
        children: Vec<Node<K>>,
    },
    Grid {
        gap: f32,
        row_gap: f32,
        column: f32,
        children: Vec<Node<K>>,
    },
    Wrap {
        gap: f32,
        row_gap: f32,
        children: Vec<Node<K>>,
    },
    Stack {
        gap: f32,
        children: Vec<Node<K>>,
    },
    Group(Vec<Node<K>>),
    Share(Share, Box<Node<K>>),
    Reserve {
        shown: Box<Node<K>>,
        alternatives: Vec<Node<K>>,
    },
    Pad {
        top: f32,
        left: f32,
        bottom: f32,
        child: Box<Node<K>>,
    },
    Beside(Anchor, Box<Node<K>>),
    /// Its child centred across the width offered, as `vertical_centered` put it.
    Center(Box<Node<K>>),
    Disabled(Box<Node<K>>),
    Disclosure {
        header: Header,
        /// 0 closed, 1 open, and between while a collapsing header animates, as egui's does.
        openness: f32,
        body: Box<Node<K>>,
    },
}

// ---------------------------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------------------------

/// A leaf.
pub fn leaf<K>(key: K, kind: Kind) -> Node<K> {
    Node::Leaf(key, kind)
}

/// A fader at the collection's [`control::FADER_HEIGHT`], filling its column: the leaf a
/// [`fader_row`] is made of. `label` is what it paints — *A* for *Envelope 1 attack* — and `widest`
/// its widest reading ([`control::widest_value`]).
pub fn fader<K>(key: K, label: &str, widest: String) -> Node<K> {
    leaf(
        key,
        Kind::VerticalSlider {
            label: label.to_owned(),
            widest,
            height: control::FADER_HEIGHT,
            fills: true,
        },
    )
}

/// **The collection's fader row**: [`fader`]s side by side in equal columns, each as wide as the
/// widest of them, so a row reads as one set whose levels compare at a glance — an envelope's
/// A, D, S and R, a mixer's sources. The owner, 2026-09-25: *"ADSRs are typically sliders"*, and
/// mixers with them.
pub fn fader_row<K>(ui: &Ui, faders: Vec<Node<K>>) -> Node<K> {
    Node::Columns {
        gap: ui.spacing().item_spacing.x,
        column: None,
        stretch: false,
        children: faders,
    }
}

/// **The collection's one knob row**: equal columns, each at most its knob's
/// [`control::knob_column`] — the cap is their sum **and the gaps between them**, so a card with room
/// draws every column exactly that wide — and shrinking below it only as far as its widest knob
/// allows. mxm-mono-08's row, every editor's since 2026-09-24. A knob in it declares no column of its
/// own (`column: 0.0`): the row decides, and `control::knob_size` keeps its reading whole.
pub fn knob_row<K>(ui: &Ui, knobs: Vec<(Size, Node<K>)>) -> Node<K> {
    let gap = ui.spacing().item_spacing.x;
    let count = knobs.len().max(1) as f32;
    let total = knobs
        .iter()
        .map(|(size, _)| control::knob_column(*size))
        .sum::<f32>()
        + gap * (count - 1.0);
    Node::Columns {
        gap,
        column: Some(total / count),
        stretch: false,
        children: knobs.into_iter().map(|(_, child)| child).collect(),
    }
}

/// Side by side, top-aligned, `GAP` apart.
pub fn row<K>(children: Vec<Node<K>>) -> Node<K> {
    Node::Row { gap: GAP, children }
}

/// Side by side, `gap` apart.
pub fn row_gap<K>(gap: f32, children: Vec<Node<K>>) -> Node<K> {
    Node::Row { gap, children }
}

/// One under another, `GAP` apart: a card body.
pub fn stack<K>(children: Vec<Node<K>>) -> Node<K> {
    Node::Stack { gap: GAP, children }
}

/// One under another, `gap` apart.
pub fn stack_gap<K>(gap: f32, children: Vec<Node<K>>) -> Node<K> {
    Node::Stack { gap, children }
}

/// A hairline group: [`crate::shell::group`].
pub fn group<K>(children: Vec<Node<K>>) -> Node<K> {
    Node::Group(children)
}

/// `child`, every toggle or every segmented cell in it at one width.
pub fn share<K>(share: Share, child: Node<K>) -> Node<K> {
    Node::Share(share, Box::new(child))
}

/// `child`, pushed down by `top`.
pub fn pad<K>(top: f32, child: Node<K>) -> Node<K> {
    Node::Pad {
        top,
        left: 0.0,
        bottom: 0.0,
        child: Box::new(child),
    }
}

/// `child` with room above, to its left and below.
pub fn pad_all<K>(top: f32, left: f32, bottom: f32, child: Node<K>) -> Node<K> {
    Node::Pad {
        top,
        left,
        bottom,
        child: Box::new(child),
    }
}

/// `child`, pushed down onto a knob's circle beside it.
pub fn beside_knob<K>(size: Size, child: Node<K>) -> Node<K> {
    Node::Beside(Anchor::Knob(size), Box::new(child))
}

/// **A switch beside a knob**, the one geometry for it: on the knob's circle line, [`switch_gap`]
/// from the circle — a tempo sync's quarter note beside its Rate or Time
/// (`plans/plan-tempo-sync-controls.md`). It is the next child of a row after its knob (or the knob
/// row ending in it): the row measures from the circle, never into the knob's own room, and anywhere
/// else it stands `switch_gap` from whatever precedes it.
pub fn switch_beside_knob<K>(size: Size, child: Node<K>) -> Node<K> {
    Node::Beside(Anchor::KnobSwitch(size), Box::new(child))
}

/// `child`, centred across the width offered.
pub fn center<K>(child: Node<K>) -> Node<K> {
    Node::Center(Box::new(child))
}

/// `child`, disabled.
pub fn disabled<K>(child: Node<K>) -> Node<K> {
    Node::Disabled(Box::new(child))
}

/// Draws `shown` and reserves the largest of it and `alternatives`.
pub fn reserve<K>(shown: Node<K>, alternatives: Vec<Node<K>>) -> Node<K> {
    Node::Reserve {
        shown: Box::new(shown),
        alternatives,
    }
}

/// [`crate::shell::disclosure`] as data, its open state read from the editor's memory.
pub fn disclosure<K>(
    ctx: &egui::Context,
    label: &str,
    description: &str,
    body: Node<K>,
) -> Node<K> {
    let open = ctx.data(|d| {
        d.get_temp::<bool>(crate::shell::disclosure_id(label))
            .unwrap_or(false)
    });
    Node::Disclosure {
        header: Header::Toggle {
            label: label.to_owned(),
            description: description.to_owned(),
        },
        openness: if open { 1.0 } else { 0.0 },
        body: Box::new(body),
    }
}

/// A collapsing header as data, its open state read from the editor's memory. Its body opens and
/// closes over egui's animation time, as `egui::CollapsingHeader`'s does; the card reserves it
/// open throughout, so only what is under the header in the card moves.
pub fn collapsing<K>(ctx: &egui::Context, title: &str, caption: bool, body: Node<K>) -> Node<K> {
    let openness = egui::collapsing_header::CollapsingState::load_with_default_open(
        ctx,
        collapsing_id(title),
        false,
    )
    .openness(ctx);
    Node::Disclosure {
        header: Header::Collapsing {
            title: title.to_owned(),
            caption,
        },
        openness,
        body: Box::new(body),
    }
}

/// Where a collapsing header titled `title` keeps its state.
#[must_use]
pub fn collapsing_id(title: &str) -> egui::Id {
    egui::Id::new(("mxm-collapsing", title))
}

/// Text that wraps in the caption style.
pub fn caption<K>(key: K, text: &str) -> Node<K> {
    leaf(
        key,
        Kind::Text {
            text: text.to_owned(),
            font: Font::Caption,
            flow: Flow::Wrap,
        },
    )
}

// ---------------------------------------------------------------------------------------------
// Measuring
// ---------------------------------------------------------------------------------------------

/// What an enclosing [`Node::Share`] or [`Node::Disabled`] says to everything under it — and, when
/// `reserve`, that heights are the reserved ones: every disclosure open, every [`Node::Reserve`] at
/// its largest alternative.
#[derive(Clone, Copy, Debug, Default)]
struct Scope {
    toggle: f32,
    cell: f32,
    disabled: bool,
    reserve: bool,
}

fn strs(options: &[String]) -> Vec<&str> {
    options.iter().map(String::as_str).collect()
}

/// The font a text leaf is drawn in.
#[must_use]
pub fn font_id(ui: &Ui, font: Font) -> egui::FontId {
    let style = ui.style();
    match font {
        Font::Body => egui::TextStyle::Body.resolve(style),
        Font::Caption => crate::typography::caption_style(style).resolve(style),
        Font::Button => egui::TextStyle::Button.resolve(style),
        Font::Heading => egui::TextStyle::Heading.resolve(style),
        Font::Monospace => egui::TextStyle::Monospace.resolve(style),
        Font::Sized(size) => egui::FontId::new(size, egui::TextStyle::Body.resolve(style).family),
    }
}

fn galley(ui: &Ui, text: &str, font: Font, wrap: f32) -> Vec2 {
    ui.painter()
        .layout(
            text.to_owned(),
            font_id(ui, font),
            egui::Color32::PLACEHOLDER,
            wrap,
        )
        .size()
}

fn line(ui: &Ui, text: &str, font: Font) -> Vec2 {
    ui.painter()
        .layout_no_wrap(
            text.to_owned(),
            font_id(ui, font),
            egui::Color32::PLACEHOLDER,
        )
        .size()
}

impl Kind {
    fn fills(&self) -> bool {
        match self {
            Self::Slider { .. } => true,
            Self::VerticalSlider { fills, .. } => *fills,
            Self::Selector { width, .. } => width.is_none(),
            Self::Button { fills, .. } | Self::Custom { fills, .. } => *fills,
            Self::Text { flow, .. } => *flow != Flow::Line,
            _ => false,
        }
    }

    fn min_width(&self, ui: &Ui, scope: Scope) -> f32 {
        match self {
            Self::Knob {
                name,
                widest,
                size,
                column,
            } => control::knob_size(ui, name, widest, *size, *column).x,
            Self::Slider {
                label,
                quiet,
                widest,
            } => control::slider_size(ui, label, *quiet, widest).x,
            Self::VerticalSlider {
                label,
                widest,
                height,
                ..
            } => control::slider_vertical_size(ui, label, widest, *height).x,
            Self::Segmented {
                label,
                options,
                beside,
            } => control::segmented_size(ui, label, &strs(options), *beside, scope.cell).x,
            Self::Waves {
                label,
                count,
                marks,
                beside,
            } => control::waves_size(ui, label.as_deref(), *count, &strs(marks), *beside).x,
            Self::Toggle { label } => control::toggle_size(ui, label, scope.toggle).x,
            Self::CompactToggle => control::toggle_compact_size().x,
            Self::PictureToggle => control::toggle_wave_size().x,
            Self::SyncToggle => control::toggle_wave_size_of(control::Wave::QuarterNote).x,
            Self::Selector {
                label,
                options,
                caption,
                width,
            } => {
                let own = if *caption {
                    control::selector_caption_size(ui, label, &strs(options))
                } else {
                    control::selector_size(ui, label, &strs(options))
                };
                width.map_or(own.x, |w| w.max(own.x))
            }
            Self::Button { label, min, fills } => {
                if *fills {
                    min.x
                } else {
                    control::button_size(ui, label, *min).x
                }
            }
            Self::Checkbox { label } => control::checkbox_size(ui, label).x,
            Self::Combo { selected } => control::combo_size(ui, selected).x,
            Self::EguiSlider { widest, label } => {
                control::egui_slider_size(ui, widest, label.as_deref()).x
            }
            Self::RemoveMark { size } => *size,
            Self::Text { text, font, flow } => match flow {
                Flow::Wrap => text
                    .split_whitespace()
                    .map(|word| line(ui, word, *font).x)
                    .fold(0.0, f32::max),
                Flow::Line => line(ui, text, *font).x,
                Flow::Truncate => line(ui, "…", *font).x,
            },
            Self::Custom { min_width, .. } => *min_width,
        }
    }

    /// Its height at `width`; `row` is the height of the row it stands in, when it does.
    fn height(&self, ui: &Ui, width: f32, scope: Scope, row: Option<f32>) -> f32 {
        match self {
            Self::Knob {
                name,
                widest,
                size,
                column,
            } => control::knob_size(ui, name, widest, *size, *column).y,
            Self::Slider {
                label,
                quiet,
                widest,
            } => control::slider_size(ui, label, *quiet, widest).y,
            Self::VerticalSlider { height, .. } => *height,
            Self::Segmented {
                label,
                options,
                beside,
            } => control::segmented_size(ui, label, &strs(options), *beside, scope.cell).y,
            Self::Waves {
                label,
                count,
                marks,
                beside,
            } => control::waves_size(ui, label.as_deref(), *count, &strs(marks), *beside).y,
            Self::Toggle { label } => control::toggle_size(ui, label, scope.toggle).y,
            Self::CompactToggle => control::toggle_compact_size().y,
            Self::PictureToggle => control::toggle_wave_size().y,
            Self::SyncToggle => control::toggle_wave_size_of(control::Wave::QuarterNote).y,
            Self::Selector { .. } => crate::space::MIN_TARGET,
            Self::Button { label, min, .. } => control::button_size(ui, label, *min).y,
            Self::Checkbox { label } => control::checkbox_size(ui, label).y,
            Self::Combo { selected } => control::combo_size(ui, selected).y,
            Self::EguiSlider { widest, label } => {
                control::egui_slider_size(ui, widest, label.as_deref()).y
            }
            Self::RemoveMark { size } => *size,
            Self::Text { text, font, flow } => match flow {
                Flow::Wrap => galley(ui, text, *font, width).y,
                Flow::Line | Flow::Truncate => line(ui, text, *font).y,
            },
            Self::Custom { height, .. } => match height {
                Height::Fixed(h) => *h,
                Height::Of(f) => f(width),
                Height::Row(min) => row.map_or(*min, |r| r.max(*min)),
            },
        }
    }

    fn stretches_to_row(&self) -> bool {
        matches!(
            self,
            Self::Custom {
                height: Height::Row(_),
                ..
            }
        )
    }
}

fn gaps(count: usize) -> f32 {
    count.saturating_sub(1) as f32
}

/// The gap between a disclosure header's rows: an item spacing and `SPACE_2`, as
/// [`crate::shell::disclosure`] draws them.
fn disclosure_gap(ui: &Ui) -> f32 {
    ui.spacing().item_spacing.y + SPACE_2
}

fn header_height(ui: &Ui, header: &Header) -> f32 {
    match header {
        Header::Toggle { .. } => SPACE_3 + HAIRLINE + disclosure_gap(ui) + crate::space::MIN_TARGET,
        Header::Collapsing { title, caption } => collapsing_header_size(ui, title, *caption).y,
    }
}

fn header_width(ui: &Ui, header: &Header) -> f32 {
    match header {
        Header::Toggle { label, .. } => control::toggle_min_width(ui, label),
        Header::Collapsing { title, caption } => collapsing_header_size(ui, title, *caption).x,
    }
}

/// A collapsing header's row, as `egui::CollapsingHeader` lays it out: the title an `indent` in,
/// with a button's padding after it and above and below, at least one interact size.
fn collapsing_header_size(ui: &Ui, title: &str, caption: bool) -> Vec2 {
    let spacing = ui.spacing();
    let text = line(ui, title, header_font(caption));
    Vec2::new(
        spacing.indent + text.x + spacing.button_padding.x,
        text.y + 2.0 * spacing.button_padding.y,
    )
    .max(spacing.interact_size)
}

fn header_font(caption: bool) -> Font {
    if caption { Font::Caption } else { Font::Button }
}

/// What egui's indented body adds below itself: the horizontal closing line's room, when the
/// style draws one.
fn body_end(ui: &Ui, header: &Header) -> f32 {
    match header {
        Header::Collapsing { .. } if ui.spacing().indent_ends_with_horizontal_line => SPACE_2,
        _ => 0.0,
    }
}

/// Where a disclosure's body starts below its header, and how far it is indented.
fn body_offset(ui: &Ui, header: &Header) -> Vec2 {
    match header {
        Header::Toggle { .. } => vec2(0.0, disclosure_gap(ui)),
        Header::Collapsing { .. } => vec2(ui.spacing().indent, ui.spacing().item_spacing.y),
    }
}

impl<K: Hash + Debug> Node<K> {
    /// The narrowest this can be drawn without anything in it overflowing.
    pub fn min_width(&self, ui: &Ui) -> f32 {
        self.min_in(ui, Scope::default())
    }

    /// Whether it takes the width it is offered rather than its own.
    pub fn fills(&self) -> bool {
        match self {
            Self::Leaf(_, kind) => kind.fills(),
            Self::Space(_) => false,
            Self::Separator | Self::Grid { .. } | Self::Wrap { .. } | Self::Center(_) => true,
            Self::Columns {
                stretch,
                column,
                children,
                ..
            } => *stretch || column.is_some() || children.iter().any(Self::fills),
            Self::Row { children, .. } | Self::Stack { children, .. } | Self::Group(children) => {
                children.iter().any(Self::fills)
            }
            Self::Reserve {
                shown,
                alternatives,
            } => shown.fills() || alternatives.iter().any(Self::fills),
            Self::Disclosure { header, body, .. } => {
                matches!(header, Header::Toggle { .. }) || body.fills()
            }
            Self::Share(_, child)
            | Self::Pad { child, .. }
            | Self::Beside(_, child)
            | Self::Disabled(child) => child.fills(),
        }
    }

    /// Its height when drawn `width` wide, as it stands: disclosures as they are, each
    /// [`Node::Reserve`] at the child it shows. This is what [`show`] allocates.
    pub fn height(&self, ui: &Ui, width: f32) -> f32 {
        self.height_in(ui, width, Scope::default(), None)
    }

    /// Its height `width` wide with everything not shown reserved: every disclosure open, every
    /// [`Node::Reserve`] at its largest alternative. A card is sized to this and draws its natural
    /// [`Node::height`] at its top, so what is not shown leaves its room at the card's foot rather
    /// than in the middle of it — and nothing moves when it appears.
    pub fn reserved_height(&self, ui: &Ui, width: f32) -> f32 {
        let scope = Scope {
            reserve: true,
            ..Scope::default()
        };
        self.height_in(ui, width, scope, None)
    }

    /// The width it is drawn at when `offered` is available: all of it if it fills, its own
    /// narrowest otherwise — and never less than its narrowest.
    pub fn drawn_width(&self, ui: &Ui, offered: f32) -> f32 {
        self.drawn_in(ui, offered, Scope::default())
    }

    fn drawn_in(&self, ui: &Ui, offered: f32, scope: Scope) -> f32 {
        let min = self.min_in(ui, scope);
        match self {
            // Capped columns take what they are offered up to their cap.
            Self::Columns {
                column: Some(column),
                stretch: false,
                children,
                ..
            } => offered.min(column * children.len() as f32).max(min),
            _ if self.fills() => offered.max(min),
            _ => min,
        }
    }

    fn min_in(&self, ui: &Ui, scope: Scope) -> f32 {
        match self {
            Self::Leaf(_, kind) => kind.min_width(ui, scope),
            Self::Space(size) => size.x,
            Self::Separator => 0.0,
            Self::Row { gap, children } => row_min(ui, children, *gap, scope),
            Self::Columns {
                gap,
                column,
                children,
                ..
            } => {
                let n = children.len() as f32;
                let widest = children
                    .iter()
                    .map(|c| c.min_in(ui, scope))
                    .fold(0.0, f32::max);
                let _ = (n, column);
                widest * children.len() as f32 + gap * gaps(children.len())
            }
            Self::Grid {
                column, children, ..
            } => children
                .iter()
                .map(|c| c.min_in(ui, scope))
                .fold(*column, f32::max),
            Self::Wrap { children, .. } | Self::Stack { children, .. } => children
                .iter()
                .map(|c| c.min_in(ui, scope))
                .fold(0.0, f32::max),
            Self::Group(children) => {
                children
                    .iter()
                    .map(|c| c.min_in(ui, scope))
                    .fold(0.0, f32::max)
                    + 2.0 * GROUP_INSET
            }
            Self::Share(kind, child) => child.min_in(ui, child.shared(ui, *kind, scope)),
            Self::Reserve {
                shown,
                alternatives,
            } => alternatives
                .iter()
                .map(|c| c.min_in(ui, scope))
                .fold(shown.min_in(ui, scope), f32::max),
            Self::Pad { left, child, .. } => left + child.min_in(ui, scope),
            Self::Beside(_, child) | Self::Center(child) => child.min_in(ui, scope),
            Self::Disabled(child) => child.min_in(ui, scope),
            Self::Disclosure { header, body, .. } => {
                header_width(ui, header).max(body_offset(ui, header).x + body.min_in(ui, scope))
            }
        }
    }

    fn height_in(&self, ui: &Ui, width: f32, scope: Scope, row: Option<f32>) -> f32 {
        match self {
            Self::Leaf(_, kind) => kind.height(ui, width, scope, row),
            Self::Space(size) => size.y,
            Self::Separator => HAIRLINE,
            Self::Row { gap, children } => {
                let widths = row_widths(ui, children, *gap, width, scope);
                row_height(ui, children, &widths, scope)
            }
            Self::Columns { children, .. } => {
                let (column, _) = self.column_width(ui, width, scope);
                children
                    .iter()
                    .map(|c| c.height_in(ui, c.in_column(ui, column, scope), scope, None))
                    .fold(0.0, f32::max)
            }
            Self::Grid {
                gap,
                row_gap,
                column,
                children,
            } => {
                let lines = grid_lines(children, *column, width);
                lines
                    .iter()
                    .map(|line| line_height(ui, line, *gap, *column, scope))
                    .sum::<f32>()
                    + row_gap * gaps(lines.len())
            }
            Self::Wrap {
                gap,
                row_gap,
                children,
            } => {
                let lines = wrap_lines(ui, children, *gap, width, scope);
                lines
                    .iter()
                    .map(|line| {
                        line.iter()
                            .map(|c| c.height_in(ui, c.min_in(ui, scope), scope, None))
                            .fold(0.0, f32::max)
                    })
                    .sum::<f32>()
                    + row_gap * gaps(lines.len())
            }
            Self::Stack { gap, children } => stack_height(ui, children, *gap, width, scope),
            Self::Group(children) => {
                stack_height(ui, children, SPACE_2, width - 2.0 * GROUP_INSET, scope)
                    + 2.0 * GROUP_INSET
            }
            Self::Share(kind, child) => {
                child.height_in(ui, width, child.shared(ui, *kind, scope), row)
            }
            Self::Reserve {
                shown,
                alternatives,
            } => {
                let own = shown.height_in(ui, shown.drawn_in(ui, width, scope), scope, row);
                if scope.reserve {
                    alternatives
                        .iter()
                        .map(|c| c.height_in(ui, c.drawn_in(ui, width, scope), scope, row))
                        .fold(own, f32::max)
                } else {
                    own
                }
            }
            Self::Pad {
                top,
                left,
                bottom,
                child,
            } => top + child.height_in(ui, width - left, scope, row) + bottom,
            Self::Beside(anchor, child) => {
                anchor_drop(ui, *anchor) + child.height_in(ui, width, scope, row)
            }
            Self::Center(child) => {
                child.height_in(ui, child.drawn_in(ui, width, scope), scope, row)
            }
            Self::Disabled(child) => child.height_in(ui, width, scope, row),
            Self::Disclosure {
                header,
                openness,
                body,
            } => {
                let head = header_height(ui, header);
                let shown = if scope.reserve { 1.0 } else { *openness };
                if shown > 0.0 {
                    let offset = body_offset(ui, header);
                    let body_width = body.drawn_in(ui, width - offset.x, scope);
                    // Opening, egui reveals the gap above the body and the body together.
                    head + shown
                        * (offset.y
                            + body.height_in(ui, body_width, scope, None)
                            + body_end(ui, header))
                } else {
                    head
                }
            }
        }
    }

    /// For a `Columns`: each column's width at `width`, and the columns' total.
    fn column_width(&self, ui: &Ui, width: f32, scope: Scope) -> (f32, f32) {
        let Self::Columns {
            gap,
            column,
            stretch,
            children,
        } = self
        else {
            unreachable!("columns only")
        };
        let n = children.len().max(1) as f32;
        let min = self.min_in(ui, scope);
        let total = if *stretch {
            width.max(min)
        } else {
            column.map_or(min, |c| width.min(c * n).max(min))
        };
        ((total - gap * gaps(children.len())) / n, total)
    }

    /// How far in from its right edge its last knob's circle ends, drawn `width` wide: what a
    /// [`switch_beside_knob`] after it reaches back over. Nothing for anything not ending in a knob.
    fn beyond_circle(&self, ui: &Ui, width: f32, scope: Scope) -> f32 {
        match self {
            // The circle is centred in the knob's band, which is its whole width.
            Self::Leaf(_, Kind::Knob { size, .. }) => ((width - size.diameter()) / 2.0).max(0.0),
            Self::Columns { children, .. } => {
                let Some(last) = children.last() else {
                    return 0.0;
                };
                let (column, total) = self.column_width(ui, width, scope);
                let drawn = last.in_column(ui, column, scope);
                // Where `place_columns` puts the last one: centred in the last column.
                let right = total - column + ((column - drawn) / 2.0).max(0.0) + drawn;
                (width - right).max(0.0) + last.beyond_circle(ui, drawn, scope)
            }
            Self::Disabled(child) => child.beyond_circle(ui, width, scope),
            _ => 0.0,
        }
    }

    /// The width a child is drawn at in a column `column` wide.
    fn in_column(&self, ui: &Ui, column: f32, scope: Scope) -> f32 {
        let min = self.min_in(ui, scope);
        match self {
            // A knob's band is its column: it is drawn across the whole of it, as a fader is.
            Self::Leaf(_, Kind::Knob { .. } | Kind::VerticalSlider { .. }) => column.max(min),
            _ if self.fills() => column.max(min),
            _ => min,
        }
    }

    /// The scope a `Share` gives its subtree.
    fn shared(&self, ui: &Ui, kind: Share, mut scope: Scope) -> Scope {
        let mut widest = 0.0_f32;
        self.visit(&mut |node| {
            if let Self::Leaf(_, leaf) = node {
                match (kind, leaf) {
                    (Share::Toggles, Kind::Toggle { label }) => {
                        widest = widest.max(control::toggle_min_width(ui, label));
                    }
                    (Share::Cells, Kind::Segmented { options, .. }) => {
                        widest = widest.max(control::segmented_cell(ui, &strs(options)));
                    }
                    _ => {}
                }
            }
        });
        match kind {
            Share::Toggles => scope.toggle = scope.toggle.max(widest),
            Share::Cells => scope.cell = scope.cell.max(widest),
        }
        scope
    }

    fn visit(&self, f: &mut dyn FnMut(&Self)) {
        f(self);
        match self {
            Self::Row { children, .. }
            | Self::Columns { children, .. }
            | Self::Grid { children, .. }
            | Self::Wrap { children, .. }
            | Self::Stack { children, .. }
            | Self::Group(children) => children.iter().for_each(|c| c.visit(f)),
            Self::Reserve {
                shown,
                alternatives,
            } => {
                shown.visit(f);
                alternatives.iter().for_each(|c| c.visit(f));
            }
            Self::Share(_, child)
            | Self::Pad { child, .. }
            | Self::Beside(_, child)
            | Self::Center(child)
            | Self::Disabled(child) => child.visit(f),
            Self::Disclosure { body, .. } => body.visit(f),
            Self::Leaf(..) | Self::Space(_) | Self::Separator => {}
        }
    }

    /// Every leaf key in the tree, in drawing order — for a check that each is unique.
    pub fn keys(&self) -> Vec<&K> {
        let mut keys = Vec::new();
        self.collect(&mut keys);
        keys
    }

    fn collect<'a>(&'a self, keys: &mut Vec<&'a K>) {
        match self {
            Self::Leaf(key, _) => keys.push(key),
            Self::Row { children, .. }
            | Self::Columns { children, .. }
            | Self::Grid { children, .. }
            | Self::Wrap { children, .. }
            | Self::Stack { children, .. }
            | Self::Group(children) => children.iter().for_each(|c| c.collect(keys)),
            Self::Reserve {
                shown,
                alternatives,
            } => {
                shown.collect(keys);
                alternatives.iter().for_each(|c| c.collect(keys));
            }
            Self::Share(_, child)
            | Self::Pad { child, .. }
            | Self::Beside(_, child)
            | Self::Center(child)
            | Self::Disabled(child) => child.collect(keys),
            Self::Disclosure { body, .. } => body.collect(keys),
            Self::Space(_) | Self::Separator => {}
        }
    }
}

fn anchor_drop(ui: &Ui, anchor: Anchor) -> f32 {
    match anchor {
        Anchor::Knob(size) | Anchor::KnobSwitch(size) => control::knob_line_drop(ui, size),
        Anchor::CellLine => control::cell_line_drop(ui),
    }
}

/// How far a row moves `child` from `gap` after `before`, which is drawn `width` wide: a
/// [`switch_beside_knob`] to [`switch_gap`] from the circle of the knob before it, and never into that
/// knob's room. Nothing for anything else.
fn shift<K: Hash + Debug>(
    ui: &Ui,
    before: &Node<K>,
    width: f32,
    child: &Node<K>,
    gap: f32,
    scope: Scope,
) -> f32 {
    match child {
        Node::Beside(Anchor::KnobSwitch(size), _) => {
            (switch_gap(*size) - before.beyond_circle(ui, width, scope)).max(0.0) - gap
        }
        _ => 0.0,
    }
}

/// A row's narrowest: its children's, the gaps between them, and each [`shift`].
fn row_min<K: Hash + Debug>(ui: &Ui, children: &[Node<K>], gap: f32, scope: Scope) -> f32 {
    let mins: Vec<f32> = children.iter().map(|c| c.min_in(ui, scope)).collect();
    let shifts: f32 = children
        .windows(2)
        .zip(&mins)
        .map(|(pair, width)| shift(ui, &pair[0], *width, &pair[1], gap, scope))
        .sum();
    mins.iter().sum::<f32>() + gap * gaps(children.len()) + shifts
}

/// Each child's width in a row `width` wide: its own, except that fillers share what is left.
fn row_widths<K: Hash + Debug>(
    ui: &Ui,
    children: &[Node<K>],
    gap: f32,
    width: f32,
    scope: Scope,
) -> Vec<f32> {
    let mins: Vec<f32> = children.iter().map(|c| c.min_in(ui, scope)).collect();
    let fillers = children.iter().filter(|c| c.fills()).count();
    let spare = (width - row_min(ui, children, gap, scope)).max(0.0);
    let share = if fillers == 0 {
        0.0
    } else {
        spare / fillers as f32
    };
    // A filler takes its share, or as much of it as it will draw: a capped `Columns` stops at its
    // cap, and what it leaves stays empty rather than pushing its neighbours along.
    children
        .iter()
        .zip(mins)
        .map(|(c, min)| {
            if c.fills() {
                c.drawn_in(ui, min + share, scope)
            } else {
                min
            }
        })
        .collect()
}

/// A row's height: its tallest child, with row-height children taking that.
fn row_height<K: Hash + Debug>(ui: &Ui, children: &[Node<K>], widths: &[f32], scope: Scope) -> f32 {
    let natural = children
        .iter()
        .zip(widths)
        .filter(|(c, _)| !stretches(c))
        .map(|(c, w)| c.height_in(ui, *w, scope, None))
        .fold(0.0, f32::max);
    children
        .iter()
        .zip(widths)
        .filter(|(c, _)| stretches(c))
        .map(|(c, w)| c.height_in(ui, *w, scope, Some(natural)))
        .fold(natural, f32::max)
}

fn stretches<K>(node: &Node<K>) -> bool {
    matches!(node, Node::Leaf(_, kind) if kind.stretches_to_row())
}

fn stack_widths<K: Hash + Debug>(
    ui: &Ui,
    children: &[Node<K>],
    width: f32,
    scope: Scope,
) -> Vec<f32> {
    children
        .iter()
        .map(|c| c.drawn_in(ui, width, scope))
        .collect()
}

fn stack_height<K: Hash + Debug>(
    ui: &Ui,
    children: &[Node<K>],
    gap: f32,
    width: f32,
    scope: Scope,
) -> f32 {
    stack_widths(ui, children, width, scope)
        .into_iter()
        .zip(children)
        .map(|(w, c)| c.height_in(ui, w, scope, None))
        .sum::<f32>()
        + gap * gaps(children.len())
}

/// A grid's lines at `width`: as many columns as `width / column` holds, at least one.
fn grid_lines<K>(children: &[Node<K>], column: f32, width: f32) -> Vec<&[Node<K>]> {
    let per_line = ((width / column).floor() as usize).max(1);
    children.chunks(per_line).collect()
}

/// A grid line is `ui.columns` inside `k × column`: each column's width.
fn line_columns(count: usize, gap: f32, column: f32) -> f32 {
    (column * count as f32 - gap * gaps(count)) / count.max(1) as f32
}

fn line_height<K: Hash + Debug>(
    ui: &Ui,
    line: &[Node<K>],
    gap: f32,
    column: f32,
    scope: Scope,
) -> f32 {
    let width = line_columns(line.len(), gap, column);
    line.iter()
        .map(|c| c.height_in(ui, c.in_column(ui, width, scope), scope, None))
        .fold(0.0, f32::max)
}

/// A wrap's lines at `width`: children at their own widths, a new line when the next would not fit.
fn wrap_lines<'a, K: Hash + Debug>(
    ui: &Ui,
    children: &'a [Node<K>],
    gap: f32,
    width: f32,
    scope: Scope,
) -> Vec<Vec<&'a Node<K>>> {
    let mut lines: Vec<Vec<&Node<K>>> = Vec::new();
    let mut used = 0.0;
    for child in children {
        let w = child.min_in(ui, scope);
        match lines.last_mut() {
            Some(line) if used + gap + w <= width + 0.5 => {
                line.push(child);
                used += gap + w;
            }
            _ => {
                lines.push(vec![child]);
                used = w;
            }
        }
    }
    lines
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

struct Painter<'p, K> {
    tokens: &'p Tokens,
    leaf: &'p mut dyn FnMut(&mut Ui, &K, Rect),
    observe: &'p mut dyn FnMut(&K, Rect, Rect),
}

impl<K: Hash + Debug> Node<K> {
    fn place(&self, ui: &mut Ui, rect: Rect, scope: Scope, p: &mut Painter<'_, K>) {
        match self {
            Self::Leaf(key, _) => {
                let mut builder = UiBuilder::new()
                    .id_salt(("mxm-tree-leaf", key))
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min));
                if scope.disabled {
                    builder = builder.disabled();
                }
                let mut child = ui.new_child(builder);
                let leaf = &mut *p.leaf;
                within(&mut child, scope, |ui| leaf(ui, key, rect));
                (p.observe)(key, rect, child.min_rect());
            }
            Self::Space(_) => {}
            Self::Separator => {
                let line = Rect::from_min_size(rect.min, vec2(rect.width(), HAIRLINE));
                ui.painter().rect_filled(line, 0.0, p.tokens.border);
            }
            Self::Row { gap, children } => {
                let widths = row_widths(ui, children, *gap, rect.width(), scope);
                let natural = row_height(ui, children, &widths, scope);
                let mut x = rect.left();
                for (i, (child, &width)) in children.iter().zip(&widths).enumerate() {
                    if i > 0 {
                        x += shift(ui, &children[i - 1], widths[i - 1], child, *gap, scope);
                    }
                    let row = stretches(child).then_some(natural);
                    let height = child.height_in(ui, width, scope, row);
                    let at = Rect::from_min_size(pos2(x, rect.top()), vec2(width, height));
                    child.place(ui, at, scope, p);
                    x += width + gap;
                }
            }
            Self::Columns { gap, children, .. } => {
                let (column, _) = self.column_width(ui, rect.width(), scope);
                place_columns(ui, children, *gap, column, rect.min, scope, p);
            }
            Self::Grid {
                gap,
                row_gap,
                column,
                children,
            } => {
                let mut y = rect.top();
                for line in grid_lines(children, *column, rect.width()) {
                    let width = line_columns(line.len(), *gap, *column);
                    place_columns(ui, line, *gap, width, pos2(rect.left(), y), scope, p);
                    y += line_height(ui, line, *gap, *column, scope) + row_gap;
                }
            }
            Self::Wrap {
                gap,
                row_gap,
                children,
            } => {
                let mut y = rect.top();
                for line in wrap_lines(ui, children, *gap, rect.width(), scope) {
                    let mut x = rect.left();
                    let mut tallest = 0.0_f32;
                    for child in line {
                        let w = child.min_in(ui, scope);
                        let h = child.height_in(ui, w, scope, None);
                        child.place(ui, Rect::from_min_size(pos2(x, y), vec2(w, h)), scope, p);
                        x += w + gap;
                        tallest = tallest.max(h);
                    }
                    y += tallest + row_gap;
                }
            }
            Self::Stack { gap, children } => place_stack(ui, children, *gap, rect, scope, p),
            Self::Group(children) => {
                ui.painter().rect_stroke(
                    rect,
                    RADIUS as f32,
                    Stroke::new(HAIRLINE, p.tokens.border),
                    StrokeKind::Inside,
                );
                place_stack(ui, children, SPACE_2, rect.shrink(GROUP_INSET), scope, p);
            }
            Self::Share(kind, child) => {
                let scope = child.shared(ui, *kind, scope);
                child.place(ui, rect, scope, p);
            }
            Self::Reserve { shown, .. } => {
                let width = shown.drawn_in(ui, rect.width(), scope);
                let height = shown.height_in(ui, width, scope, None);
                shown.place(
                    ui,
                    Rect::from_min_size(rect.min, vec2(width, height)),
                    scope,
                    p,
                );
            }
            Self::Pad {
                top, left, child, ..
            } => {
                let width = child.drawn_in(ui, rect.width() - left, scope);
                let height = child.height_in(ui, width, scope, None);
                let at = Rect::from_min_size(rect.min + vec2(*left, *top), vec2(width, height));
                child.place(ui, at, scope, p);
            }
            Self::Beside(anchor, child) => {
                let mut rect = rect;
                rect.min.y += anchor_drop(ui, *anchor);
                child.place(ui, rect, scope, p);
            }
            Self::Center(child) => {
                let width = child.drawn_in(ui, rect.width(), scope);
                let height = child.height_in(ui, width, scope, None);
                let left = rect.left() + ((rect.width() - width) / 2.0).max(0.0);
                let at = Rect::from_min_size(pos2(left, rect.top()), vec2(width, height));
                child.place(ui, at, scope, p);
            }
            Self::Disabled(child) => {
                let scope = Scope {
                    disabled: true,
                    ..scope
                };
                child.place(ui, rect, scope, p);
            }
            Self::Disclosure {
                header,
                openness,
                body,
            } => {
                let head = header_height(ui, header);
                // The body is drawn as the tree was built — the height it stated. A click here
                // shows next frame, which the header asks for.
                draw_header(ui, p.tokens, header, *openness > 0.0, rect, scope);
                if *openness > 0.0 {
                    let offset = body_offset(ui, header);
                    let width = body.drawn_in(ui, rect.width() - offset.x, scope);
                    let height = body.height_in(ui, width, scope, None);
                    let at = Rect::from_min_size(
                        rect.min + vec2(offset.x, head + offset.y),
                        vec2(width, height),
                    );
                    // Part-open, the body shows down to the height the tree stated, as egui clips
                    // an animating body. The clip is the parent's, so no widget id moves.
                    let clip = ui.clip_rect();
                    if *openness < 1.0 {
                        let mut shown = clip;
                        shown.max.y = shown
                            .max
                            .y
                            .min(rect.top() + self.height_in(ui, rect.width(), scope, None));
                        ui.set_clip_rect(shown);
                    }
                    body.place(ui, at, scope, p);
                    if matches!(header, Header::Collapsing { .. }) {
                        indent_lines(ui, at);
                    }
                    ui.set_clip_rect(clip);
                }
            }
        }
    }
}

/// Draws `add` inside the shared widths `scope` carries — and only those: a scope opened at zero
/// would override one the caller had opened around the whole tree.
fn within(ui: &mut Ui, scope: Scope, add: impl FnOnce(&mut Ui)) {
    let cells = |ui: &mut Ui| {
        if scope.cell > 0.0 {
            control::segmented_stack(ui, scope.cell, add);
        } else {
            add(ui);
        }
    };
    if scope.toggle > 0.0 {
        control::toggle_stack(ui, scope.toggle, cells);
    } else {
        cells(ui);
    }
}

/// The lines egui's `Ui::indent` draws beside and under an indented body.
fn indent_lines(ui: &Ui, body: Rect) {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let x = body.left() - ui.spacing().indent / 2.0;
    let mut bottom = body.bottom();
    if ui.spacing().indent_ends_with_horizontal_line {
        bottom += SPACE_2;
    }
    let bottom = bottom - 2.0;
    if ui.visuals().indent_has_left_vline {
        ui.painter()
            .line_segment([pos2(x, body.top()), pos2(x, bottom)], stroke);
    }
    if ui.spacing().indent_ends_with_horizontal_line {
        ui.painter()
            .line_segment([pos2(x, bottom), pos2(body.right() - 2.0, bottom)], stroke);
    }
}

fn place_columns<K: Hash + Debug>(
    ui: &mut Ui,
    children: &[Node<K>],
    gap: f32,
    column: f32,
    at: egui::Pos2,
    scope: Scope,
    p: &mut Painter<'_, K>,
) {
    let mut x = at.x;
    for child in children {
        let width = child.in_column(ui, column, scope);
        let height = child.height_in(ui, width, scope, None);
        // Centred in its column, as `vertical_centered` put it.
        let left = x + ((column - width) / 2.0).max(0.0);
        child.place(
            ui,
            Rect::from_min_size(pos2(left, at.y), vec2(width, height)),
            scope,
            p,
        );
        x += column + gap;
    }
}

fn place_stack<K: Hash + Debug>(
    ui: &mut Ui,
    children: &[Node<K>],
    gap: f32,
    rect: Rect,
    scope: Scope,
    p: &mut Painter<'_, K>,
) {
    let mut y = rect.top();
    for (child, width) in children
        .iter()
        .zip(stack_widths(ui, children, rect.width(), scope))
    {
        let height = child.height_in(ui, width, scope, None);
        child.place(
            ui,
            Rect::from_min_size(pos2(rect.left(), y), vec2(width, height)),
            scope,
            p,
        );
        y += height + gap;
    }
}

/// Draws a disclosure's header in `rect`'s top and returns whether it is now open, and where it
/// drew.
fn draw_header(
    ui: &mut Ui,
    tokens: &Tokens,
    header: &Header,
    open: bool,
    rect: Rect,
    scope: Scope,
) -> (bool, Rect) {
    let mut builder = UiBuilder::new()
        .max_rect(rect)
        .layout(Layout::top_down(Align::Min));
    if scope.disabled {
        builder = builder.disabled();
    }
    let mut child = ui.new_child(builder);
    let open = match header {
        Header::Toggle { label, description } => {
            child.add_space(SPACE_3);
            crate::shell::separator(&mut child, tokens);
            child.add_space(SPACE_2);
            let was = open;
            let mut open = open;
            crate::control::toggle(&mut child, tokens, label, &mut open, false, description);
            let id = crate::shell::disclosure_id(label);
            child.data_mut(|d| d.insert_temp(id, open));
            if open != was {
                // The tree drew this frame as it was built; the next one shows the change.
                child.ctx().request_repaint();
            }
            open
        }
        Header::Collapsing { title, caption } => {
            // `egui::CollapsingHeader::begin`'s layout, with the state under a key the tree can
            // read when it is built: the whole row toggles, the icon centred in the indent, the
            // title an indent in.
            let id = collapsing_id(title);
            let size = collapsing_header_size(&child, title, *caption);
            let (rect, _) = child.allocate_exact_size(size, Sense::hover());
            let response = child.interact(rect, id.with("header"), Sense::click());
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                child.ctx(),
                id,
                false,
            );
            if response.clicked() {
                state.toggle(&child);
            }
            let open = state.is_open();
            let openness = state.openness(child.ctx());
            state.store(child.ctx());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, true, title)
            });
            let (mut icon, _) = child.spacing().icon_rectangles(rect);
            icon.set_center(pos2(
                rect.left() + child.spacing().indent / 2.0,
                rect.center().y,
            ));
            egui::collapsing_header::paint_default_icon(
                &mut child,
                openness,
                &response.clone().with_new_rect(icon),
            );
            let font = font_id(&child, header_font(*caption));
            let colour = if *caption {
                tokens.text_secondary
            } else {
                child
                    .style()
                    .interact_selectable(&response, false)
                    .text_color()
            };
            let galley = child
                .painter()
                .layout_no_wrap(title.to_owned(), font, colour);
            let at = pos2(
                rect.left() + child.spacing().indent,
                rect.center().y - galley.size().y / 2.0,
            );
            child.painter().galley(at, galley, colour);
            open
        }
    };
    (open, child.min_rect())
}

/// The narrowest a [`crate::ModuleCard`] titled `title` can be around `body`: the body's narrowest
/// and the card's chrome — the content floor of design system §4.3. A plugin's declared usability
/// minimum, where it has one, is the larger of the two.
#[must_use]
pub fn card_floor<K: Hash + Debug>(ui: &Ui, title: &str, body: &Node<K>) -> f32 {
    crate::shell::card_floor(ui, title, body.min_width(ui))
}

/// Lays `node` out in the space `ui` offers and draws it: allocates exactly the size the tree
/// computes — never less than its narrowest — then hands each leaf its rectangle through `leaf`, in
/// a child `Ui` bounded by it and salted with the leaf's key.
///
/// Returns the allocated rectangle, which is also the answer the tree gave before anything was
/// drawn.
pub fn show<K: Hash + Debug>(
    ui: &mut Ui,
    tokens: &Tokens,
    node: &Node<K>,
    mut leaf: impl FnMut(&mut Ui, &K, Rect),
) -> Rect {
    show_observed(ui, tokens, node, &mut leaf, &mut |_, _, _| {})
}

/// [`show`], reporting each leaf's given rectangle and the rectangle it actually drew in — for a
/// check that nothing paints outside the room it was given.
pub fn show_observed<K: Hash + Debug>(
    ui: &mut Ui,
    tokens: &Tokens,
    node: &Node<K>,
    leaf: &mut dyn FnMut(&mut Ui, &K, Rect),
    observe: &mut dyn FnMut(&K, Rect, Rect),
) -> Rect {
    let width = node.drawn_width(ui, ui.available_width());
    let height = node.height(ui, width);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let mut painter = Painter {
        tokens,
        leaf,
        observe,
    };
    node.place(ui, rect, Scope::default(), &mut painter);
    rect
}

#[cfg(test)]
mod tests;
