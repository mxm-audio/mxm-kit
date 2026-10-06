//! Cards in wrapping rows — the collection's reflowing panel layout.
//!
//! # Rows, and this module decides them rather than the engine
//!
//! Every card in a row starts on the row's top line and ends on its bottom line, and a row is
//! exactly as tall as the tallest card in it (§3.3). Rows stack down the panel. That is the whole
//! of it, and it is what makes a panel read as a panel rather than as boxes thrown in with a
//! pitchfork.
//!
//! **The packing is decided here, not by `flex_wrap`.** [`pack`] chooses which cards share a row and
//! each row is then an ordinary flex row that stretches its children. `flex_wrap` cannot do this,
//! because it wraps single items and has no idea that two cards belong together.
//!
//! # What replaced what
//!
//! This supersedes [`crate::shell::level_columns`] for any editor that reflows. That mechanism
//! dealt cards into a fixed number of columns and handed each column's deficit to the cards inside
//! it, one frame late. It has two properties a reflowing panel cannot have: a **column count**, and
//! a card stretched to a *column's* height rather than to a *row's* — which produces a card with
//! nothing in it as soon as one column is much shorter than its neighbour.
//!
//! `level_columns` stays for the fixed editors that still use it, and for the player.
//!
//! # Groups
//!
//! A **group** is a run of cards that belongs together — two oscillators, a filter with the envelope
//! wired to it. A row break never falls inside a group, so parallel branches stay side by side at
//! every width (§3.4). A group too wide for a row of its own is split into single cards, because at
//! that width no arrangement keeps it whole.

use egui::{Rect, Ui, vec2};
use egui_taffy::taffy::prelude::{auto, length, percent};
use egui_taffy::taffy::{self, Style};
use egui_taffy::{TuiBuilderLogic, TuiContainerResponse, tui};

use crate::space::SPACE_3;
use crate::theme::Tokens;

/// One card the flow lays out: its title, and the narrowest it may be drawn.
#[derive(Clone, Copy, Debug)]
pub struct Card<'a> {
    pub title: &'a str,
    /// The larger of *the width below which it overflows* and *the width below which its controls
    /// stop being usable* — §4.3. One of the two alone is a number the layout believes and a person
    /// does not.
    pub floor: f32,
    /// Optional card-local ceiling, used when categories with different widths share a page.
    pub ceiling: Option<f32>,
}

impl<'a> Card<'a> {
    #[must_use]
    pub const fn new(title: &'a str, floor: f32) -> Self {
        Self {
            title,
            floor,
            ceiling: None,
        }
    }

    #[must_use]
    pub const fn capped(mut self, ceiling: f32) -> Self {
        self.ceiling = Some(ceiling);
        self
    }
}

/// The gap between cards and between rows.
pub const GAP: f32 = SPACE_3;

/// The width a group needs: its cards at their floors, plus the gaps between them.
#[must_use]
pub fn group_width(cards: &[Card<'_>], group: &[usize]) -> f32 {
    let sum: f32 = group
        .iter()
        .filter_map(|c| cards.get(*c))
        .map(|c| c.floor)
        .sum();
    sum + GAP * (group.len().saturating_sub(1)) as f32
}

/// The narrowest width that draws every card at its floor: one card per row.
#[must_use]
pub fn minimum_width(cards: &[Card<'_>]) -> f32 {
    cards.iter().map(|c| c.floor).fold(0.0_f32, f32::max)
}

/// Which cards share a row, at this width.
///
/// First fit, in order, and **the order is never changed** — only where the breaks fall. §3.4 makes
/// the sequence the contract; this decides nothing but the wrapping.
#[must_use]
pub fn pack(cards: &[Card<'_>], groups: &[&[usize]], width: f32) -> Vec<Vec<usize>> {
    let mut units: Vec<Vec<usize>> = Vec::new();
    for group in groups {
        if group.len() > 1 && group_width(cards, group) > width {
            units.extend(group.iter().map(|card| vec![*card]));
        } else {
            units.push(group.to_vec());
        }
    }

    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row: Vec<usize> = Vec::new();
    let mut used = 0.0_f32;

    for unit in units {
        let needs = group_width(cards, &unit);
        if !row.is_empty() && used + GAP + needs > width {
            rows.push(std::mem::take(&mut row));
            used = 0.0;
        }
        used = if row.is_empty() {
            needs
        } else {
            used + GAP + needs
        };
        row.extend(unit);
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// Where each card was drawn, by index, from the frame that just ran.
///
/// A test reads its editor's rows from this, the way one reads a fixed editor's columns from
/// [`crate::shell::levelled`].
#[must_use]
pub fn drawn(ctx: &egui::Context, id: egui::Id, count: usize) -> Vec<Option<Rect>> {
    (0..count)
        .map(|card| ctx.data(|d| d.get_temp::<Rect>(rect_id(id, card))))
        .collect()
}

/// Natural (unstretched) outer height, only at the width it was measured at.
///
/// `None` means measure, not zero height. Callers must use [`invalidate_measurements`] when
/// fonts, zoom or size-changing content changes. This readback does not measure
/// hidden cards and is not, by itself, a paging measurement pipeline.
#[must_use]
pub fn natural_height(ctx: &egui::Context, id: egui::Id, card: usize, width: f32) -> Option<f32> {
    ctx.data(|d| d.get_temp::<(f32, f32)>(natural_id(id, card)))
        .filter(|(height, measured_width)| {
            height.is_finite() && *height >= 0.0 && (measured_width - width).abs() < 0.01
        })
        .map(|(height, _)| height)
}

/// Invalidate natural-height memory without changing widget IDs or the visible-rectangle readback.
/// Call before planning after a font, zoom or size-changing content/disclosure update, including
/// an update to a card that is not currently visible.
pub fn invalidate_measurements(ctx: &egui::Context, id: egui::Id, count: usize) {
    ctx.data_mut(|data| {
        for card in 0..count {
            data.remove::<(f32, f32)>(natural_id(id, card));
        }
    });
}

/// Outer widths for one packed row, computed by the same CSS styles as the renderer, unrounded.
/// This is deliberately not a second handwritten approximation of flex-grow/ceiling behavior. The
/// paging renderer places cards at exactly these widths; [`cards`] lays out through `egui_taffy`,
/// which rounds to whole points, so its readback can differ from these by under a point.
/// Indices and finite, positive floors/width must be validated by the caller.
#[must_use]
pub fn row_widths(cards: &[Card<'_>], row: &[usize], width: f32, ceiling: Option<f32>) -> Vec<f32> {
    use taffy::prelude::{AvailableSpace, Size, TaffyTree};
    let mut tree: TaffyTree<()> = TaffyTree::new();
    // Exact widths, not taffy's whole-point rounding: a floor computed from a card's tree is
    // fractional, and a row rounded up past the budget it was solved for read to the paging planner
    // as a row that does not fit — every card on a page of its own at a width that holds them all.
    tree.disable_rounding();
    let children: Vec<_> = row
        .iter()
        .map(|index| {
            tree.new_leaf(card_style(
                cards[*index].floor,
                cards[*index].ceiling.or(ceiling),
            ))
            .expect("fresh node")
        })
        .collect();
    let root = tree
        .new_with_children(row_of_cards(), &children)
        .expect("fresh row");
    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::MaxContent,
        },
    )
    .expect("valid row tree");
    children
        .iter()
        .map(|node| tree.layout(*node).expect("computed child").size.width)
        .collect()
}

fn rect_id(id: egui::Id, card: usize) -> egui::Id {
    id.with(("flow-rect", card))
}

fn natural_id(id: egui::Id, card: usize) -> egui::Id {
    id.with(("flow-natural", card))
}

/// Lays `cards` out in wrapping rows and calls `draw` with the `Ui` inside each card's body.
///
/// `groups` are the runs that must not be split across a row break; pass one index per card for no
/// grouping at all. `ceiling` caps how wide a card may grow — **give it one**: a row holding a
/// single card will otherwise stretch it across the whole panel, and a card much wider than the
/// controls in it is empty space wearing a border (§3.3).
pub fn cards(
    ui: &mut Ui,
    tokens: &Tokens,
    id: egui::Id,
    cards: &[Card<'_>],
    groups: &[&[usize]],
    ceiling: Option<f32>,
    draw: &mut dyn FnMut(&mut Ui, usize),
) {
    cards_with(
        ui,
        tokens,
        id,
        cards,
        groups,
        ceiling,
        Options::default(),
        draw,
    );
}

/// Which axes the flow scrolls on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scroll {
    /// Vertical only — **what every editor ships**. The flow takes the width it is given, so a
    /// card is never reachable by scrolling sideways; a card whose floor exceeds the window is
    /// drawn past the edge instead.
    #[default]
    Vertical,
    /// No scroll container. For a page whose measured content fits its viewport; an indivisible
    /// overflowing page must instead use `Both` so its controls remain reachable.
    None,
    /// Both axes, so a card whose declared floor is wider than the viewport can still be reached
    /// — the case that turns up at high editor zoom in a host window that cannot grow.
    ///
    /// **It is not free.** A horizontal scrollbar appearing steals height and a vertical one
    /// steals width, and either changes the rect this module hands taffy, which is one of the two
    /// conditions that force a relayout (see [`Options::quantum`]). Measure before shipping it.
    Both,
}

/// How the flow scrolls, and how stable a width it lays out at.
///
/// # Why a quantum exists at all
///
/// `egui_taffy` recomputes the whole tree, and calls `Context::request_discard`, whenever
/// `taffy.dirty(root) || last_size != root_rect.size()` — `egui_taffy-0.14.0/src/lib.rs:632`. The
/// root rect is [`Ui::available_rect_before_wrap`], so **while a window is being dragged it changes
/// every frame**, every frame is discarded, and every frame is laid out and painted twice. This
/// multiplies layout work, but says nothing about native event scheduling or displayed frame rate.
///
/// A quantum rounds the rect the layout is computed in **down** to a multiple of that many points,
/// so a smooth drag crosses a boundary once every *n* points instead of every frame. The cost is
/// visible: the cards lag the window edge by up to one quantum and snap when it is crossed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Options {
    pub scroll: Scroll,
    /// Round the layout rect down to a multiple of this many points. `None` — the default, and
    /// what every editor ships — follows the window exactly, at one relayout per resize frame.
    pub quantum: Option<f32>,
    /// Skip drawing a card that is wholly outside the viewport, reusing the height it was last
    /// measured at.
    ///
    /// **Every editor ships `false`**, which means every card's body is drawn on every pass whether
    /// it is on screen or not — immediate mode's default, and the reason `ScrollArea::show_rows`
    /// exists. A narrow window wraps ten cards into many rows and shows two of them, so the work
    /// skipped here can be far larger than the doubled pass a resize costs.
    ///
    /// The price is that a culled card's height goes stale: it keeps the height it had when it was
    /// last on screen, so content that changes size off screen moves the rows below it only once it
    /// is scrolled back into view. Cards do not change height on their own, so this is a real but
    /// narrow cost — an expander opened off screen is the case to think about.
    pub cull: bool,
}

/// How far outside the viewport a card is still drawn, so scrolling does not pop.
const CULL_MARGIN: f32 = 240.0;

/// [`cards`], with the scrolling and relayout policy spelled out.
///
/// [`cards`] is this with [`Options::default`], which is the behaviour every editor ships. Reach
/// for this one only with a measurement in hand: both knobs trade something away.
#[allow(clippy::too_many_arguments)]
pub fn cards_with(
    ui: &mut Ui,
    tokens: &Tokens,
    id: egui::Id,
    cards: &[Card<'_>],
    groups: &[&[usize]],
    ceiling: Option<f32>,
    options: Options,
    draw: &mut dyn FnMut(&mut Ui, usize),
) {
    // Packing reads the *outer* width, before any scroll area, so the row breaks are decided from
    // the window and never from a viewport a scrollbar has just shrunk.
    let rows = pack(cards, groups, ui.available_width());

    // Scrolling over the cards is safe: `control::Wheel::default()` is `Off`, so the wheel never
    // edits a control it happens to be over (§7.1).
    // `auto_shrink` is **off horizontally and on vertically**: the flow always takes the width it
    // is given, and takes only the height its rows need. Forcing the height too pushed everything
    // after it off the panel — mxm-mono-03 draws a divider and a second zone below its cards, and
    // they went under the fold.
    let mut contents = |ui: &mut Ui| {
        match options.quantum {
            None => tree(ui, tokens, id, cards, &rows, ceiling, options.cull, draw),
            Some(step) => {
                // The layout runs in a child whose rect is quantised, because the rect taffy
                // compares against is the *Ui's* available rect and nothing else.
                let free = ui.available_rect_before_wrap();
                let size = vec2(quantise(free.width(), step), quantise(free.height(), step));
                let mut child = ui.new_child(
                    egui::UiBuilder::new().max_rect(Rect::from_min_size(free.min, size)),
                );
                tree(
                    &mut child,
                    tokens,
                    id,
                    cards,
                    &rows,
                    ceiling,
                    options.cull,
                    draw,
                );
                let used = child.min_rect();
                ui.allocate_rect(used, egui::Sense::hover());
            }
        }
    };
    match options.scroll {
        Scroll::None => contents(ui),
        Scroll::Vertical => {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, contents);
        }
        Scroll::Both => {
            egui::ScrollArea::both()
                .auto_shrink([false, true])
                .show(ui, contents);
        }
    }
}

/// Rounds down to a multiple of `step`, so the value only moves when a boundary is crossed.
fn quantise(value: f32, step: f32) -> f32 {
    if step <= 0.0 || !value.is_finite() {
        return value;
    }
    (value / step).floor() * step
}

/// The taffy tree itself: a column of rows, each row a run of cards.
#[allow(clippy::too_many_arguments)]
fn tree(
    ui: &mut Ui,
    tokens: &Tokens,
    id: egui::Id,
    cards: &[Card<'_>],
    rows: &[Vec<usize>],
    ceiling: Option<f32>,
    cull: bool,
    draw: &mut dyn FnMut(&mut Ui, usize),
) {
    // `reserve_available_width`, never `reserve_available_space`: inside a scroll area the
    // available height is unbounded, and reserving it would size the layout to nothing.
    tui(ui, id)
        .reserve_available_width()
        .style(column_of_rows())
        .show(|tui| {
            for row in rows {
                tui.style(row_of_cards()).add(|tui| {
                    for card in row {
                        leaf(tui, tokens, id, *card, cards[*card], ceiling, cull, draw);
                    }
                });
            }
        });
}

fn column_of_rows() -> Style {
    Style {
        flex_direction: taffy::FlexDirection::Column,
        gap: taffy::Size {
            width: length(GAP),
            height: length(GAP),
        },
        size: taffy::Size {
            width: percent(1.0_f32),
            height: auto(),
        },
        ..Default::default()
    }
}

/// One row: cards side by side, every one of them the height of the tallest.
///
/// `align_items: Stretch` is the whole mechanism — §3.3's *cards laid out side by side end on one
/// line* — applied where it belongs, to a row of cards.
fn row_of_cards() -> Style {
    Style {
        flex_direction: taffy::FlexDirection::Row,
        align_items: Some(taffy::AlignItems::Stretch),
        justify_content: Some(taffy::JustifyContent::FlexStart),
        gap: taffy::Size {
            width: length(GAP),
            height: length(GAP),
        },
        size: taffy::Size {
            width: percent(1.0_f32),
            height: auto(),
        },
        ..Default::default()
    }
}

/// Draws one card.
///
/// # The trap this carries
///
/// A card stretches to its row's height, and it must **not feed that stretched height back into its
/// own measurement**. [`crate::shell::Level`] records the same rule the hard way: *what is
/// remembered is the natural height, the deficit subtracted back out*, because storing the padded
/// height makes the correction feed on itself and the rows creep.
///
/// So the growth is spent inside the card with [`crate::shell::grow`] — never `Ui::add_space`, which
/// also commits the item spacing — and the size reported back is the height **with that growth
/// subtracted out again**.
#[allow(clippy::too_many_arguments)]
fn leaf(
    tui: &mut egui_taffy::Tui,
    tokens: &Tokens,
    id: egui::Id,
    index: usize,
    card: Card<'_>,
    ceiling: Option<f32>,
    cull: bool,
    draw: &mut dyn FnMut(&mut Ui, usize),
) {
    let ceiling = card.ceiling.or(ceiling);
    tui.style(card_style(card.floor, ceiling))
        .ui_manual(|ui, container| {
            // Where taffy put this card, on screen, with the scroll offset already applied.
            let placed = container.full_container();
            let target = placed.height();
            // The remembered height, **and the width it was measured at**. A card's height is only
            // valid for one width, so the pair travels together; keeping the height alone is what
            // made a culled card freeze at a height from a layout it was no longer in.
            let remembered: Option<(f32, f32)> =
                ui.ctx().data(|d| d.get_temp(natural_id(id, index)));
            let previous = remembered.map_or(0.0, |(natural, _)| natural);

            // **Wholly out of view, at a width it has already been measured at: draw nothing.** It
            // still records where it would have been, so `drawn` and every editor's layout test
            // read the same rects whether this is on or off.
            //
            // The width guard is why culling does nothing for a *horizontal* drag: the width
            // changes every frame, so nothing is ever reusable. Pair it with `quantum` and the
            // width only moves at a boundary, which is where the two options compose.
            let measured_here =
                remembered.is_some_and(|(_, width)| (width - placed.width()).abs() < 0.5);
            if cull
                && measured_here
                && !container.first_frame()
                && previous > 0.0
                && !placed.intersects(ui.clip_rect().expand(CULL_MARGIN))
            {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(rect_id(id, index), placed));
                return TuiContainerResponse {
                    inner: (),
                    intrinsic_size: None,
                    infinite: egui::Vec2b::FALSE,
                    min_size: vec2(card.floor, previous),
                    max_size: vec2(ceiling.unwrap_or(4096.0), previous),
                };
            }

            // On a node's first frame the container's rect is a sizing pass, not a layout: there is
            // no row height to match yet, so nothing is spent.
            let spare = if container.first_frame() || previous <= 0.0 {
                0.0
            } else {
                (target - previous).max(0.0)
            };

            crate::ModuleCard::new(card.title).show(ui, tokens, |ui| {
                draw(ui, index);
                crate::shell::grow(ui, spare);
            });

            let drawn = ui.min_rect();
            let natural = drawn.height() - spare;
            ui.ctx().data_mut(|d| {
                d.insert_temp(natural_id(id, index), (natural, placed.width()));
                d.insert_temp(rect_id(id, index), drawn);
            });

            TuiContainerResponse {
                inner: (),
                intrinsic_size: None,
                infinite: egui::Vec2b::FALSE,
                min_size: vec2(card.floor, natural),
                max_size: vec2(ceiling.unwrap_or(4096.0), natural),
            }
        });
}

fn card_style(floor: f32, ceiling: Option<f32>) -> Style {
    Style {
        flex_grow: 1.0,
        flex_shrink: 0.0,
        flex_basis: length(floor),
        min_size: taffy::Size {
            width: length(floor),
            height: auto(),
        },
        max_size: taffy::Size {
            width: ceiling.map_or(auto(), length),
            height: auto(),
        },
        align_self: Some(taffy::AlignItems::Stretch),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARDS: &[Card<'static>] = &[
        Card::new("A", 200.0),
        Card::new("B", 200.0),
        Card::new("C", 300.0),
        Card::new("D", 100.0),
    ];

    /// A group never straddles a row break, which is the whole reason the packing is not
    /// `flex_wrap`.
    #[test]
    fn a_group_is_never_split_while_it_fits_a_row() {
        let groups: &[&[usize]] = &[&[0, 1], &[2, 3]];

        // Both groups fit one row.
        assert_eq!(pack(CARDS, groups, 1000.0), vec![vec![0, 1, 2, 3]]);

        // Neither pair fits beside the other, so each takes a row — and neither is broken up.
        assert_eq!(pack(CARDS, groups, 500.0), vec![vec![0, 1], vec![2, 3]]);
    }

    /// A group too wide for any row is split, because at that width nothing keeps it whole.
    #[test]
    fn a_group_wider_than_the_row_is_split_rather_than_overflowing() {
        let groups: &[&[usize]] = &[&[0, 1], &[2, 3]];
        let rows = pack(CARDS, groups, 320.0);
        assert_eq!(rows, vec![vec![0], vec![1], vec![2], vec![3]]);
    }

    /// The order is the contract: packing decides breaks, never sequence.
    #[test]
    fn packing_never_reorders() {
        for width in [120.0, 260.0, 410.0, 700.0, 1600.0] {
            let groups: &[&[usize]] = &[&[0, 1], &[2], &[3]];
            let read: Vec<usize> = pack(CARDS, groups, width).into_iter().flatten().collect();
            assert_eq!(read, vec![0, 1, 2, 3], "reordered at {width}");
        }
    }

    /// A quantum only ever rounds **down**, so the layout never claims width the window does not
    /// have — which would put a card past the edge to buy a smoother drag.
    #[test]
    fn a_quantum_rounds_down_and_never_up() {
        for step in [8.0_f32, 16.0, 32.0] {
            for width in [360.0_f32, 361.0, 700.0, 1023.9, 1600.0] {
                let q = quantise(width, step);
                assert!(q <= width, "{q} exceeds {width} at a {step} quantum");
                assert!(
                    width - q < step,
                    "{q} is more than one {step} step under {width}"
                );
                assert!((q % step).abs() < 0.001, "{q} is not a multiple of {step}");
            }
        }
    }

    /// A quantum of zero or less is off, not a division by zero.
    #[test]
    fn a_quantum_that_is_not_a_step_is_ignored() {
        assert!((quantise(700.0, 0.0) - 700.0).abs() < f32::EPSILON);
        assert!((quantise(700.0, -8.0) - 700.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_minimum_is_one_card_wide() {
        assert!((minimum_width(CARDS) - 300.0).abs() < f32::EPSILON);
    }
}
