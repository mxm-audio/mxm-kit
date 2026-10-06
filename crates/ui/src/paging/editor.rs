//! Editor-thread integration of the pure planner, for an editor whose card bodies are
//! `crate::tree`s (plans/plan-layout-tree.md, in the private archive). A card's height at any
//! candidate width is what its tree states, so nothing is drawn to learn a size: there is no
//! measuring context, no measuring pass for a control to guard against, and no revision to keep a
//! cache honest. Callers still snapshot destructive telemetry reads before `show`, because painting
//! reads them.

use super::*;
use crate::{
    ModuleCard, ViewBar,
    space::{MIN_TARGET, SPACE_3, SPACE_4},
    theme::Tokens,
};
use egui::{Context, Id, Rect, Ui, UiBuilder, vec2};

#[derive(Clone, Debug)]
pub struct Report {
    pub plan: Plan,
    pub selected: usize,
    pub viewport: Rect,
    pub visible: Vec<(Key, Rect)>,
    pub scrolling: bool,
    pub compact_navigation: bool,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            plan: Plan::default(),
            selected: 0,
            viewport: Rect::NOTHING,
            visible: Vec::new(),
            scrolling: false,
            compact_navigation: false,
        }
    }
}

/// How the renderer learns a card's height and draws its body: both from the card's tree.
struct Source<'a> {
    draw: &'a mut dyn FnMut(&mut Ui, usize),
    /// A card's outer height at an outer width, as its tree states it.
    height: &'a mut dyn FnMut(usize, f32) -> f32,
}

#[derive(Clone, Copy, Debug)]
enum Request {
    Category(Category),
    Card(Key),
}
#[derive(Clone, Default)]
struct Runtime {
    state: State,
    report: Report,
    width: f32,
    /// This frame's heights, as the trees state them. Cleared every frame: a tree is rebuilt every
    /// frame, and a disclosure, a selected slot or a warning line changes one without a parameter.
    samples: Vec<(Measurement, f32)>,
}
fn runtime_id() -> Id {
    Id::new("mxm-dynamic-pages")
}
fn request_id() -> Id {
    runtime_id().with("request")
}
fn card_id(key: Key) -> Id {
    runtime_id().with(("card", key))
}
pub fn report(ctx: &Context) -> Option<Report> {
    ctx.data(|d| d.get_temp::<Runtime>(runtime_id()).map(|r| r.report))
}
pub fn request_card(ctx: &Context, key: Key) {
    ctx.data_mut(|d| d.insert_temp(request_id(), Request::Card(key)));
}
pub fn request_category(ctx: &Context, category: Category) {
    ctx.data_mut(|d| d.insert_temp(request_id(), Request::Category(category)));
}

/// Consume a developer request without interpreting invalid values as a different surface.
/// Parameters is a diagnostic surface, not a page in the derived musician navigation.
pub fn developer_request(ctx: &Context, view: &mut usize, requested: Option<usize>) {
    let id = runtime_id().with("developer-request");
    if let Some(requested) = requested {
        ctx.data_mut(|d| d.insert_temp(id, requested));
    }
    if held(ctx)
        || ctx.input(|i| i.pointer.any_down() || i.pointer.any_released())
        || egui::Popup::is_any_open(ctx)
    {
        return;
    }
    let requested = ctx.data_mut(|d| d.remove_temp::<usize>(id));
    if requested == Some(PARAMETERS) {
        *view = PARAMETERS;
    } else if let Some(category) = requested.and_then(Category::from_request) {
        *view = 0;
        request_category(ctx, category);
    }
}
/// Call before consuming developer navigation. Includes browser/search/naming and exact entry.
pub fn hold(ctx: &Context, held: bool) {
    ctx.data_mut(|d| d.insert_temp(runtime_id().with("held"), held));
}
fn held(ctx: &Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(runtime_id().with("held"))
            .unwrap_or(false)
    })
}

/// True for the release pass too: the last gesture-end must be painted before replacing its card.
pub fn interaction_owned(ui: &Ui, text_editing: bool) -> bool {
    text_editing
        || held(ui.ctx())
        || ui.input(|i| i.pointer.any_down() || i.pointer.any_released())
        || egui::Popup::is_any_open(ui.ctx())
}

/// Fills the planner's missing heights from the trees: arithmetic, never a draw.
fn measure(
    runtime: &mut Runtime,
    items: &[Item<'_>],
    requests: Vec<Measurement>,
    source: &mut Source<'_>,
) {
    for request in requests {
        if runtime.samples.iter().any(|(m, _)| *m == request) {
            continue;
        }
        let index = items
            .iter()
            .position(|item| item.key == request.key)
            .expect("planner key");
        let height = (source.height)(index, request.width);
        runtime.samples.push((request, height));
    }
}

/// The same rectangles budget and paint compact navigation. Reserve the widest page number,
/// so selecting a page cannot move a button. Cells wrap without shrinking text or pointer targets.
struct CompactNavigation {
    cells: [Rect; 3],
    height: f32,
    button_font: egui::FontId,
    label_font: egui::FontId,
}
impl CompactNavigation {
    fn new(ui: &Ui, width: f32, count: usize) -> Self {
        let button_font = egui::TextStyle::Button.resolve(ui.style());
        let label_font = egui::TextStyle::Monospace.resolve(ui.style());
        let label = format!("Page {} / {count}", "8".repeat(count.to_string().len()));
        let text_size = |text: &str, font: &egui::FontId| {
            ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(text.to_owned(), font.clone(), ui.visuals().text_color())
                    .size()
            })
        };
        let padding = ui.spacing().button_padding.max(vec2(SPACE_3, 0.0)) * 2.0;
        let sizes = [
            text_size("Previous", &button_font) + padding,
            text_size(&label, &label_font),
            text_size("Next", &button_font) + padding,
        ]
        .map(|size| size.max(vec2(MIN_TARGET, MIN_TARGET)));
        let row_height = sizes.iter().map(|size| size.y).fold(MIN_TARGET, f32::max);
        let mut x = 0.0;
        let mut y = 0.0;
        let cells = sizes.map(|size| {
            if x > 0.0 && x + size.x > width {
                x = 0.0;
                y += row_height + SPACE_3;
            }
            let rect = Rect::from_min_size(egui::pos2(x, y), vec2(size.x, row_height));
            x += size.x + SPACE_3;
            rect
        });
        Self {
            cells,
            height: y + row_height + SPACE_3,
            button_font,
            label_font,
        }
    }

    fn show(&self, ui: &mut Ui, labels: &[&str], selected: &mut usize) {
        let origin = ui.cursor().min;
        let current = *selected;
        for (index, rect) in self.cells.iter().enumerate() {
            let rect = rect.translate(origin.to_vec2());
            let mut child = ui.new_child(
                UiBuilder::new()
                    .id(runtime_id().with(("compact", index)))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            if index == 1 {
                let text = format!("Page {} / {}", current + 1, labels.len());
                let response = child.allocate_rect(rect, egui::Sense::hover());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Label, child.is_enabled(), &text)
                });
                child.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    self.label_font.clone(),
                    ui.visuals().text_color(),
                );
                response.on_hover_text(labels[current]);
            } else {
                let previous = index == 0;
                if if previous {
                    current == 0
                } else {
                    current + 1 == labels.len()
                } {
                    child.disable();
                }
                let button = egui::Button::new(
                    egui::RichText::new(if previous { "Previous" } else { "Next" })
                        .font(self.button_font.clone()),
                )
                .wrap_mode(egui::TextWrapMode::Extend)
                .min_size(rect.size());
                if child.add_sized(rect.size(), button).clicked() {
                    *selected = if previous { current - 1 } else { current + 1 };
                }
            }
        }
        // Children are explicitly positioned: allocate once, including the gap after the bar.
        ui.advance_cursor_after_rect(Rect::from_min_size(
            origin,
            vec2(
                ui.available_width(),
                self.height - ui.spacing().item_spacing.y,
            ),
        ));
    }
}

fn navigation_height(ui: &Ui, labels: &[String], budget: Budget, compact: bool) -> f32 {
    if labels.len() <= 1 {
        return 0.0;
    }
    if compact {
        CompactNavigation::new(ui, budget.width, labels.len()).height
    } else {
        let names: Vec<_> = labels.iter().map(String::as_str).collect();
        let geometry = ViewBar::new(&names).geometry(ui, budget.width);
        if geometry.overflow || geometry.height + 2.0 * MIN_TARGET > budget.height {
            budget.height
        } else {
            geometry.height
        }
    }
}

// Keeps the renderer, pure metadata, this frame's height samples and budget explicit at this boundary.
#[allow(clippy::too_many_arguments)]
fn solve(
    runtime: &mut Runtime,
    ui: &Ui,
    items: &[Item<'_>],
    groups: &[&[Key]],
    budget: Budget,
    compact: bool,
    source: &mut Source<'_>,
) -> Result<Plan, Error> {
    // At most all distinct contiguous candidate row widths; finite even for a cold start.
    for _ in 0..=items.len() * items.len() + 2 {
        let outcome = plan(
            items,
            groups,
            budget,
            |request| {
                runtime
                    .samples
                    .iter()
                    .find(|(m, _)| *m == request)
                    .map(|(_, h)| *h)
            },
            |labels| navigation_height(ui, labels, budget, compact),
        )?;
        match outcome {
            Outcome::Ready(plan) => return Ok(plan),
            Outcome::Pending(requests) => measure(runtime, items, requests, source),
        }
    }
    Err(Error::NonConvergentBar)
}

fn current_fits(
    runtime: &mut Runtime,
    ui: &Ui,
    items: &[Item<'_>],
    groups: &[&[Key]],
    budget: Budget,
    source: &mut Source<'_>,
) -> bool {
    let labels: Vec<_> = runtime
        .state
        .plan
        .pages
        .iter()
        .map(|page| page.label.clone())
        .collect();
    // Font/style changes can grow the retained labels even when width and page count agree.
    if navigation_height(ui, &labels, budget, runtime.report.compact_navigation)
        > runtime.state.plan.bar_height
    {
        return false;
    }
    let mut missing = Vec::new();
    let mut fits = !runtime.state.plan.pages.is_empty();
    for page in &runtime.state.plan.pages {
        let Some(indices) = page
            .cards
            .iter()
            .map(|key| items.iter().position(|i| i.key == *key))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let mut height = |request| {
            runtime
                .samples
                .iter()
                .find(|(m, _)| *m == request)
                .map(|(_, h)| *h)
        };
        let mut solver = Solver {
            items,
            cards: items.iter().map(|i| i.card).collect(),
            groups,
            budget,
            height: &mut height,
            missing: Vec::new(),
        };
        fits &= solver
            .fits(&indices, budget.height - runtime.state.plan.bar_height)
            .unwrap_or(false);
        missing.extend(solver.missing);
    }
    if !missing.is_empty() {
        measure(runtime, items, missing, source);
        return current_fits(runtime, ui, items, groups, budget, source);
    }
    fits
}

/// Draw the musician surface inside an already-guttered workspace, for an editor whose card bodies
/// are `crate::tree`s: `card` builds card `index`'s tree, measured in a card body's style, and
/// `leaf` draws one of its leaves. All cards are metadata; only the selected page registers visible
/// controls. `text_editing` includes plugin-owned exact value entry and browser/naming editors.
///
/// Heights come from the trees, so nothing is drawn to learn one, and there is no parameter
/// revision to pass: the trees are rebuilt every frame and the planner's heights with them, so a
/// disclosure opening, a selected slot or a warning line re-plans at once, which a cache keyed on
/// parameter values would miss (plans/plan-layout-tree.md §2.5, in the private archive).
pub fn show<K: std::hash::Hash + std::fmt::Debug>(
    ui: &mut Ui,
    tokens: &Tokens,
    items: &[Item<'_>],
    groups: &[&[Key]],
    text_editing: bool,
    card: &mut dyn FnMut(&Ui, usize) -> crate::tree::Node<K>,
    leaf: &mut dyn FnMut(&mut Ui, usize, &K, Rect),
) -> Report {
    let body = crate::shell::body_ui(ui);
    let trees: Vec<_> = (0..items.len()).map(|index| card(&body, index)).collect();
    // The chrome is the card's own: its title row and separator in the panel's style.
    let chrome = crate::shell::card_height(ui, "", 0.0);
    let frame = 2.0 * (crate::space::SPACE_5 + crate::space::HAIRLINE);
    let mut height = |index: usize, width: f32| {
        let tree = &trees[index];
        // Sized to what it reserves; the tree draws what it shows at the top.
        chrome + tree.reserved_height(&body, tree.drawn_width(&body, width - frame))
    };
    let mut draw = |ui: &mut Ui, index: usize| {
        crate::tree::show(ui, tokens, &trees[index], |ui, key, rect| {
            leaf(ui, index, key, rect);
        });
    };
    show_source(
        ui,
        tokens,
        items,
        groups,
        text_editing,
        &mut Source {
            draw: &mut draw,
            height: &mut height,
        },
    )
}

fn show_source(
    ui: &mut Ui,
    tokens: &Tokens,
    items: &[Item<'_>],
    groups: &[&[Key]],
    text_editing: bool,
    source: &mut Source<'_>,
) -> Report {
    let mut runtime = ui
        .ctx()
        .data_mut(|d| d.remove_temp::<Runtime>(runtime_id()))
        .unwrap_or_default();
    let locked = interaction_owned(ui, text_editing);
    let budget = Budget {
        width: ui.available_width().max(1.0),
        height: ui.available_height().max(1.0),
        ceiling: None,
    };
    let mut compact = runtime.report.compact_navigation;
    if !locked || runtime.state.plan.pages.is_empty() {
        // Every card's height, hidden ones included, is its tree's: rebuilt this frame.
        runtime.samples.clear();
        compact = false;
        let mut candidate = solve(&mut runtime, ui, items, groups, budget, false, source);
        if matches!(candidate, Err(Error::NavigationExhausted)) {
            compact = true;
            candidate = solve(&mut runtime, ui, items, groups, budget, true, source);
        }
        match candidate {
            Ok(plan) => {
                // A retained partition must retain its navigation mode as well as its height.
                // Otherwise a compact budget can be reused while painting a much taller tab bar.
                let fits = runtime.width == budget.width
                    && runtime.report.compact_navigation == compact
                    && current_fits(&mut runtime, ui, items, groups, budget, source);
                if fits && plan.pages.len() < runtime.state.plan.pages.len() {
                    // One grid gutter of spare height is a chosen UX dead band, tested by sweeps;
                    // it is not a measured hardware constant or a zoom heuristic.
                    if let Ok(merge) = solve(
                        &mut runtime,
                        ui,
                        items,
                        groups,
                        Budget {
                            height: (budget.height - SPACE_4).max(1.0),
                            ..budget
                        },
                        compact,
                        source,
                    ) {
                        runtime
                            .state
                            .update_with_hysteresis(Outcome::Ready(merge), true, false);
                    }
                } else {
                    runtime
                        .state
                        .update_with_hysteresis(Outcome::Ready(plan), fits, false);
                }
                runtime.width = budget.width;
            }
            Err(error) => {
                // A viewport shorter than even compact navigation cannot be made bigger here.
                // Keep a navigable one-card sequence and allow both-axis reachability.
                let mut order: Vec<_> = items.iter().collect();
                order.sort_by_key(|i| i.category);
                runtime.state.update(
                    Outcome::Ready(Plan {
                        pages: order
                            .iter()
                            .map(|i| Page {
                                cards: vec![i.key],
                                label: i.card.title.to_owned(),
                                overflow: true,
                            })
                            .collect(),
                        bar_height: CompactNavigation::new(ui, budget.width, items.len()).height,
                    }),
                    false,
                );
                runtime.width = budget.width;
                compact = true;
                ui.label(format!("Layout: {error:?}"));
            }
        }
    }
    if !locked
        && let Some(request) = ui.ctx().data_mut(|d| {
            let request = d.get_temp::<Request>(request_id());
            d.remove::<Request>(request_id());
            request
        })
    {
        let key = match request {
            Request::Card(key) => Some(key),
            Request::Category(category) => {
                items.iter().find(|i| i.category == category).map(|i| i.key)
            }
        };
        if let Some(key) = key.filter(|key| {
            runtime
                .state
                .plan
                .pages
                .iter()
                .any(|p| p.cards.contains(key))
        }) {
            runtime.state.anchor = Some(key);
        }
    }
    let mut selected = runtime.state.selected_page();
    let count = runtime.state.plan.pages.len();
    let labels: Vec<_> = runtime
        .state
        .plan
        .pages
        .iter()
        .map(|p| p.label.as_str())
        .collect();
    if count > 1 {
        if compact {
            let before = ui.available_rect_before_wrap().top();
            CompactNavigation::new(ui, budget.width, count).show(ui, &labels, &mut selected);
            let consumed = ui.available_rect_before_wrap().top() - before;
            ui.add_space((runtime.state.plan.bar_height - consumed).max(0.0));
        } else {
            let before = ui.available_rect_before_wrap().top();
            ViewBar::new(&labels).show_paged(ui, tokens, &mut selected);
            let consumed = ui.available_rect_before_wrap().top() - before;
            ui.add_space((runtime.state.plan.bar_height - consumed).max(0.0));
        }
        if selected != runtime.state.selected_page() {
            if locked {
                if let Some(key) = runtime.state.plan.pages[selected].cards.first() {
                    request_card(ui.ctx(), *key);
                }
                selected = runtime.state.selected_page();
            } else {
                runtime.state.select(selected);
            }
        }
    }
    let viewport = ui.available_rect_before_wrap();
    let mut report = Report {
        plan: runtime.state.plan.clone(),
        selected,
        viewport,
        compact_navigation: compact,
        ..Default::default()
    };
    if let Some(page) = runtime.state.plan.pages.get(selected) {
        let indices: Vec<_> = page
            .cards
            .iter()
            .filter_map(|key| items.iter().position(|i| i.key == *key))
            .collect();
        let cards: Vec<_> = items.iter().map(|item| item.card).collect();
        let mut height = |_| None;
        let solver = Solver {
            items,
            cards: cards.clone(),
            groups,
            budget,
            height: &mut height,
            missing: Vec::new(),
        };
        let units = solver.units(&indices);
        let refs: Vec<_> = units.iter().map(Vec::as_slice).collect();
        let width = if locked { runtime.width } else { budget.width };
        let rows = flow::pack(&cards, &refs, width);
        let mut layout = Vec::new();
        let mut total = 0.0_f32;
        for row in rows {
            let widths = flow::row_widths(&cards, &row, width, None);
            // This frame's trees at the widths drawn, not the planner's samples: while an
            // interaction holds the page, nothing is re-planned, yet a tree may change under it — a
            // warning appears, a floor moves — and a sample from another width or tree would leave
            // the card too short or at nothing.
            let heights: Vec<_> = row
                .iter()
                .zip(&widths)
                .map(|(index, width)| (source.height)(*index, *width))
                .collect();
            let row_height = heights.iter().copied().fold(0.0, f32::max);
            total += row_height + flow::GAP;
            layout.push((row, widths, heights, row_height));
        }
        total -= flow::GAP.min(total);
        report.scrolling = page.overflow
            || total > viewport.height() + 0.5
            || cards
                .iter()
                .enumerate()
                .any(|(i, c)| indices.contains(&i) && c.floor > viewport.width());
        let mut paint = |ui: &mut Ui| {
            let mut content = Rect::from_min_size(ui.cursor().min, vec2(width, total.max(0.0)));
            let mut y = ui.cursor().top();
            for (row, widths, heights, row_height) in &layout {
                let mut x = ui.cursor().left();
                for ((index, width), _natural) in row.iter().zip(widths).zip(heights) {
                    let rect = Rect::from_min_size(egui::pos2(x, y), vec2(*width, *row_height));
                    let mut child = ui.new_child(
                        UiBuilder::new()
                            .id(card_id(items[*index].key))
                            .max_rect(rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    let key = items[*index].key;
                    ModuleCard::new(items[*index].card.title).show(&mut child, tokens, |ui| {
                        // egui 0.36 anchors set_min_height at the CURRENT cursor; reserve before
                        // drawing, not afterwards. The room is the row's: this card's reserved
                        // height or a taller neighbour's; the tree draws what it shows at the top.
                        let top = ui.cursor().top();
                        ui.set_min_height(
                            (rect.bottom() - crate::space::SPACE_5 - crate::space::HAIRLINE - top)
                                .max(0.0),
                        );
                        // Names the card every control inside it registers under, so the keyboard
                        // cursor's map is built by drawing rather than by an authored table.
                        crate::navigation::card(ui, key.0, |ui| (source.draw)(ui, *index));
                    });
                    // Painted after the card, and outside its rectangle, so the cursor can never
                    // change what is inside one or move anything beside it.
                    crate::navigation::paint_card(&child, tokens, key.0, child.min_rect());
                    content = content.union(child.min_rect());
                    report.visible.push((items[*index].key, child.min_rect()));
                    x += width + flow::GAP;
                }
                y += row_height + flow::GAP;
            }
            // new_child does not allocate in its parent. ScrollArea needs the full painted union,
            // not just the viewport width, to make an indivisible card's far edge reachable.
            ui.allocate_space(content.size());
        };
        if report.scrolling {
            egui::ScrollArea::both()
                .auto_shrink([false, true])
                .show(ui, &mut paint);
        } else {
            paint(ui);
        }
    }
    runtime.report = report.clone();
    ui.ctx().data_mut(|d| d.insert_temp(runtime_id(), runtime));
    report
}
