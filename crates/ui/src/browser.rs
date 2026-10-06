//! The preset browser's panes — §3.1's browser trigger, opened out into banks, categories and
//! presets — and the actions along its foot.
//!
//! **Rows in, actions out.** This widget knows nothing about files, parameters, banks or plugins:
//! a pane is a list of labelled rows with counts, a preset is a name with a detail beside it, and
//! what a person did comes back as a [`BrowserAction`]. The caller — `mxm-preset` — knows what the
//! rows are and what the actions mean. Same split as [`crate::shell::preset_browser`], which this
//! replaces the dropdown of: a flat list of fifty is already long, and one shared bank makes it
//! useless.

use egui::{
    Align2, Key, Modifiers, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo,
    WidgetType,
};

use crate::space::{HAIRLINE, MIN_TARGET, RADIUS, SPACE_2, SPACE_3, SPACE_4, SPACE_5};
use crate::theme::Tokens;

/// One row in a filter pane: a bank, or a category, with how many presets it holds.
pub struct PaneRow<'a> {
    pub label: &'a str,
    pub count: usize,
    /// `Some` for a bank whose own file could not be read; shown on hover, the row still selects.
    pub problem: Option<&'a str>,
}

/// One preset in the list.
pub struct BrowserRow<'a> {
    pub name: &'a str,
    /// Quietly beside the name: where it came from and what it is — *Factory · Bass*.
    pub detail: &'a str,
    pub favourite: bool,
    /// `Some` for a file that is present and broken: shown disabled, carrying the reason.
    pub problem: Option<&'a str>,
}

/// Which pane the keyboard is in: left and right move between them, up and down within one.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    Banks,
    Categories,
    #[default]
    Presets,
}

impl Pane {
    const fn left(self) -> Self {
        match self {
            Pane::Presets => Pane::Categories,
            Pane::Categories | Pane::Banks => Pane::Banks,
        }
    }

    const fn right(self) -> Self {
        match self {
            Pane::Banks => Pane::Categories,
            Pane::Categories | Pane::Presets => Pane::Presets,
        }
    }
}

/// Everything the browser shows this frame, and what the person has typed.
pub struct BrowserState<'a> {
    /// The pane the keyboard is in; its title is lit and its selected row ringed.
    pub focus: Pane,
    pub banks: &'a [PaneRow<'a>],
    /// Which bank row is selected.
    pub bank: usize,
    pub categories: &'a [PaneRow<'a>],
    pub category: usize,
    pub presets: &'a [BrowserRow<'a>],
    /// The loaded preset's row, if it is in the list.
    pub selected: Option<usize>,
    pub search: &'a mut String,
    /// Keep the search box focused this frame. **The keyboard lives there while the browser is
    /// open**: typing searches, the arrows step the presets, Enter is done — and egui's own
    /// arrow-key focus travel is locked out of it, or every press would also walk the focus ring
    /// onto a row or a button, where Enter would then click whatever it landed on.
    pub focus_search: bool,
    /// Scroll the list to the selected row this frame — the frame after the arrows moved it.
    pub scroll_to_selected: bool,
    /// How many bank files wait in the banks folder; *Import* says so.
    pub importable: usize,
    /// Whether the library can be written to at all; the bank actions are disabled otherwise.
    pub can_write: bool,
    /// A question with a few answers, over the list — *replace this bank?*, or which file to
    /// import. `(title, choices)`; a choice comes back as [`BrowserAction::pick`].
    pub picker: Option<(&'a str, &'a [String])>,
    /// The last thing that went right or wrong, along the foot.
    pub status: Option<&'a str>,
}

/// What a person did in the browser this frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BrowserAction {
    pub bank: Option<usize>,
    pub category: Option<usize>,
    /// A preset row was chosen; the index is into the slice that was passed in.
    pub load: Option<usize>,
    /// The arrow keys in the presets pane: the previous or next selectable row. In a filter pane
    /// they come back as `bank` or `category` instead.
    pub step: Option<i32>,
    /// Left or right moved the keyboard to another pane.
    pub focus: Option<Pane>,
    /// Enter: done. The caller keeps what is loaded — or loads the first match, when nothing from
    /// this list is — and closes.
    pub enter: bool,
    pub import: bool,
    pub export: bool,
    pub new_bank: bool,
    pub open_folder: bool,
    pub pick: Option<usize>,
    pub cancel_pick: bool,
    pub close: bool,
}

impl BrowserAction {
    pub fn any(&self) -> bool {
        *self != Self::default()
    }
}

/// The width a filter pane takes at a given browser width: a fifth, held between the narrowest a
/// bank name reads at and the widest that leaves the list room.
fn pane_width(total: f32) -> f32 {
    (total * 0.2).clamp(150.0, 240.0)
}

/// Draws the browser into the space `ui` has, and reports what was done.
///
/// Three panes, left to right — banks, categories, presets with a search box over them — then a
/// hairline and the bank actions. Escape closes; with the search box focused, the arrows step the
/// presets and Enter is the same as clicking the selected one.
pub fn panes(ui: &mut Ui, tokens: &Tokens, state: &mut BrowserState<'_>) -> BrowserAction {
    let mut action = BrowserAction::default();

    // **The keyboard, first and consumed**, whenever the browser is open and not asking a
    // question: left and right move between the panes, up and down move within the focused one —
    // stepping the presets, or choosing the next bank or category — and Enter is done. Consumed
    // before the search box is drawn, or that box would also move its caret on left and right
    // and jump it on up and down.
    if state.picker.is_none() {
        let (left, right, up, down, enter) = ui.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::ArrowLeft),
                input.consume_key(Modifiers::NONE, Key::ArrowRight),
                input.consume_key(Modifiers::NONE, Key::ArrowUp),
                input.consume_key(Modifiers::NONE, Key::ArrowDown),
                input.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        if left {
            action.focus = Some(state.focus.left());
        } else if right {
            action.focus = Some(state.focus.right());
        }
        let by = i32::from(down) - i32::from(up);
        if by != 0 {
            match state.focus {
                Pane::Presets => action.step = Some(by),
                Pane::Banks => {
                    let next = (state.bank as i32 + by).clamp(0, state.banks.len() as i32 - 1);
                    if next as usize != state.bank {
                        action.bank = Some(next as usize);
                    }
                }
                Pane::Categories => {
                    let next =
                        (state.category as i32 + by).clamp(0, state.categories.len() as i32 - 1);
                    if next as usize != state.category {
                        action.category = Some(next as usize);
                    }
                }
            }
        }
        if enter {
            action.enter = true;
        }
    }
    let total = ui.available_size();
    let footer = MIN_TARGET + SPACE_4 + HAIRLINE + SPACE_3;
    let body_height = (total.y - footer).max(MIN_TARGET * 3.0);
    let pane = pane_width(total.x);
    let list_width = (total.x - 2.0 * pane - 2.0 * (SPACE_4 * 2.0 + HAIRLINE)).max(MIN_TARGET);

    ui.horizontal_top(|ui| {
        if let Some(chosen) = filter_pane(
            ui,
            tokens,
            "Banks",
            pane,
            body_height,
            state.banks,
            state.bank,
            state.focus == Pane::Banks,
        ) {
            action.bank = Some(chosen);
        }
        vertical_rule(ui, tokens, body_height);
        if let Some(chosen) = filter_pane(
            ui,
            tokens,
            "Categories",
            pane,
            body_height,
            state.categories,
            state.category,
            state.focus == Pane::Categories,
        ) {
            action.category = Some(chosen);
        }
        vertical_rule(ui, tokens, body_height);

        ui.allocate_ui_with_layout(
            Vec2::new(list_width, body_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_max_width(list_width);
                pane_title(ui, tokens, "Presets", state.focus == Pane::Presets);
                // The search box, and the count beside it.
                ui.horizontal(|ui| {
                    ui.spacing_mut().interact_size.y = MIN_TARGET;
                    let field = ui.add_sized(
                        Vec2::new(list_width - 120.0, MIN_TARGET),
                        egui::TextEdit::singleline(state.search).hint_text("Search presets"),
                    );
                    if state.focus_search {
                        field.request_focus();
                        ui.memory_mut(|memory| {
                            memory.set_focus_lock_filter(
                                field.id,
                                egui::EventFilter {
                                    tab: false,
                                    horizontal_arrows: true,
                                    vertical_arrows: true,
                                    escape: false,
                                },
                            );
                        });
                    }
                    let count = state.presets.len();
                    ui.label(
                        egui::RichText::new(if count == 1 {
                            "1 preset".to_owned()
                        } else {
                            format!("{count} presets")
                        })
                        .color(tokens.text_secondary),
                    );
                });
                ui.add_space(SPACE_2);

                if let Some((title, choices)) = state.picker {
                    // A question over the list: the list waits until it is answered.
                    ui.label(egui::RichText::new(title).color(tokens.text_primary));
                    ui.add_space(SPACE_2);
                    for (index, choice) in choices.iter().enumerate() {
                        if ui
                            .add_sized(
                                Vec2::new(list_width, MIN_TARGET),
                                egui::Button::new(choice.as_str()),
                            )
                            .clicked()
                        {
                            action.pick = Some(index);
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        action.cancel_pick = true;
                    }
                } else {
                    let list_height = ui.available_height();
                    egui::ScrollArea::vertical()
                        .id_salt("mxm-browser-presets")
                        .max_height(list_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (index, row) in state.presets.iter().enumerate() {
                                let response = browser_row(
                                    ui,
                                    tokens,
                                    list_width - SPACE_3,
                                    row,
                                    state.selected == Some(index),
                                    state.focus == Pane::Presets,
                                );
                                if state.scroll_to_selected && state.selected == Some(index) {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                if response.clicked() && row.problem.is_none() {
                                    action.load = Some(index);
                                }
                            }
                        });
                }
            },
        );
    });

    ui.add_space(SPACE_3);
    crate::shell::separator(ui, tokens);
    ui.add_space(SPACE_2);

    ui.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = MIN_TARGET;
        ui.add_enabled_ui(state.can_write, |ui| {
            let import = if state.importable == 0 {
                "Import…".to_owned()
            } else {
                format!("Import… ({})", state.importable)
            };
            if ui
                .button(import)
                .on_hover_text("Unpack a bank file from the banks folder into the library")
                .clicked()
            {
                action.import = true;
            }
            if ui
                .button("Export…")
                .on_hover_text("Pack the presets shown into one bank file in the banks folder")
                .clicked()
            {
                action.export = true;
            }
            if ui
                .button("New bank…")
                .on_hover_text("An empty bank to save into")
                .clicked()
            {
                action.new_bank = true;
            }
            if ui
                .button("Open folder")
                .on_hover_text(
                    "Show the banks folder, where bank files are sent from and received into",
                )
                .clicked()
            {
                action.open_folder = true;
            }
        });
        if let Some(status) = state.status {
            ui.add_space(SPACE_3);
            ui.label(egui::RichText::new(status).color(tokens.text_secondary));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                action.close = true;
            }
        });
    });

    if ui.input(|input| input.key_pressed(Key::Escape)) {
        action.close = true;
    }
    action
}

/// A pane's title: lit in the accent while the keyboard is in that pane.
fn pane_title(ui: &mut Ui, tokens: &Tokens, title: &str, focused: bool) {
    ui.label(
        egui::RichText::new(title)
            .text_style(crate::typography::label_style(ui.style()))
            .color(if focused {
                tokens.accent
            } else {
                tokens.text_secondary
            }),
    );
    ui.add_space(SPACE_2);
}

/// One filter pane: a title, then its rows in a scroll area. Returns the row clicked.
#[allow(clippy::too_many_arguments)]
fn filter_pane(
    ui: &mut Ui,
    tokens: &Tokens,
    title: &str,
    width: f32,
    height: f32,
    rows: &[PaneRow<'_>],
    selected: usize,
    focused: bool,
) -> Option<usize> {
    let mut chosen = None;
    ui.allocate_ui_with_layout(
        Vec2::new(width, height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_max_width(width);
            pane_title(ui, tokens, title, focused);
            egui::ScrollArea::vertical()
                .id_salt(title)
                .max_height(ui.available_height())
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (index, row) in rows.iter().enumerate() {
                        let count = row.count.to_string();
                        let response = list_row(
                            ui,
                            tokens,
                            width - SPACE_3,
                            row.label,
                            &count,
                            false,
                            index == selected,
                            true,
                            focused && index == selected,
                        );
                        let response = match row.problem {
                            Some(problem) => response.on_hover_text(problem),
                            None => response,
                        };
                        if response.clicked() {
                            chosen = Some(index);
                        }
                    }
                });
        },
    );
    chosen
}

/// A hairline between panes.
fn vertical_rule(ui: &mut Ui, tokens: &Tokens, height: f32) {
    ui.add_space(SPACE_4);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(HAIRLINE, height), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, tokens.border);
    ui.add_space(SPACE_4);
}

/// A preset row: a star's column, the name, and the detail right-aligned.
fn browser_row(
    ui: &mut Ui,
    tokens: &Tokens,
    width: f32,
    row: &BrowserRow<'_>,
    selected: bool,
    focused: bool,
) -> Response {
    let response = list_row(
        ui,
        tokens,
        width,
        row.name,
        row.detail,
        row.favourite,
        selected,
        row.problem.is_none(),
        focused && selected,
    );
    match row.problem {
        // Present and broken, so it is shown and cannot be chosen. The reason is on hover rather
        // than in the row, which would resize the list.
        Some(problem) => response.on_hover_text(problem),
        None => response,
    }
}

/// One row of a list, `MIN_TARGET` tall, painted rather than laid out so the name, the detail and
/// the star sit where they are told. Named for the accessibility tree, like every painted control.
#[allow(clippy::too_many_arguments)]
fn list_row(
    ui: &mut Ui,
    tokens: &Tokens,
    width: f32,
    name: &str,
    detail: &str,
    starred: bool,
    selected: bool,
    enabled: bool,
    // The keyboard is in this pane and this is its row: a ring in the focus colour.
    ring: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, MIN_TARGET),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, enabled, selected, name));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter().with_clip_rect(rect);
        if selected {
            painter.rect_filled(rect, RADIUS, tokens.selection);
        } else if enabled && response.hovered() {
            painter.rect_filled(rect, RADIUS, tokens.surface_2);
        }
        if ring {
            painter.rect_stroke(
                rect,
                RADIUS,
                Stroke::new(1.5, tokens.focus),
                StrokeKind::Inside,
            );
        }
        let body = crate::typography::label_style(ui.style()).resolve(ui.style());
        let caption = crate::typography::caption_style(ui.style()).resolve(ui.style());
        let ink = if enabled {
            tokens.text_primary
        } else {
            tokens.text_disabled
        };
        let centre_y = rect.center().y;
        let star_column = SPACE_5;
        if starred {
            painter.text(
                egui::pos2(rect.left() + SPACE_3, centre_y),
                Align2::LEFT_CENTER,
                "★",
                body.clone(),
                tokens.accent,
            );
        }
        painter.text(
            egui::pos2(rect.left() + SPACE_3 + star_column, centre_y),
            Align2::LEFT_CENTER,
            name,
            body,
            ink,
        );
        painter.text(
            egui::pos2(rect.right() - SPACE_3, centre_y),
            Align2::RIGHT_CENTER,
            detail,
            caption,
            tokens.text_secondary,
        );
    }
    response
}

/// The rectangle the browser takes under the app bar: the panel's width less a gutter, down to
/// the panel's bottom less the same. Its caller places an `Area` there.
pub fn overlay_rect(panel: Rect, top: f32) -> Rect {
    Rect::from_min_max(
        egui::pos2(panel.left() + SPACE_5, top),
        egui::pos2(panel.right() - SPACE_5, panel.bottom() - SPACE_5),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::LIGHT;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A small browser: two banks, two categories, three presets, laid out in the harness with
    /// what it reported this frame kept for the test.
    fn harness(actions: Rc<RefCell<Vec<BrowserAction>>>) -> egui_kittest::Harness<'static> {
        let search = Rc::new(RefCell::new(String::new()));
        let focus = Rc::new(std::cell::Cell::new(Pane::Presets));
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 500.0))
            .build_ui(move |ui| {
                crate::theme::apply(ui.ctx());
                crate::typography::apply(ui.ctx());
                let banks = [
                    PaneRow {
                        label: "All",
                        count: 3,
                        problem: None,
                    },
                    PaneRow {
                        label: "Factory",
                        count: 2,
                        problem: None,
                    },
                    PaneRow {
                        label: "Cold Pads",
                        count: 1,
                        problem: None,
                    },
                ];
                let categories = [
                    PaneRow {
                        label: "All",
                        count: 3,
                        problem: None,
                    },
                    PaneRow {
                        label: "Bass",
                        count: 1,
                        problem: None,
                    },
                    PaneRow {
                        label: "Pad",
                        count: 2,
                        problem: None,
                    },
                ];
                let presets = [
                    BrowserRow {
                        name: "Sub bass",
                        detail: "Factory · Bass",
                        favourite: true,
                        problem: None,
                    },
                    BrowserRow {
                        name: "Glass",
                        detail: "Cold Pads · Pad",
                        favourite: false,
                        problem: None,
                    },
                    BrowserRow {
                        name: "broken",
                        detail: "",
                        favourite: false,
                        problem: Some("unreadable"),
                    },
                ];
                let mut search = search.borrow_mut();
                let mut state = BrowserState {
                    focus: focus.get(),
                    banks: &banks,
                    bank: 0,
                    categories: &categories,
                    category: 0,
                    presets: &presets,
                    selected: Some(0),
                    search: &mut search,
                    focus_search: true,
                    scroll_to_selected: false,
                    importable: 2,
                    can_write: true,
                    picker: None,
                    status: Some("Exported 2 presets"),
                };
                let action = panes(ui, &LIGHT, &mut state);
                if let Some(pane) = action.focus {
                    focus.set(pane);
                }
                if action.any() {
                    actions.borrow_mut().push(action);
                }
            })
    }

    #[test]
    fn every_row_and_action_is_in_the_accessibility_tree() {
        // A painted row has no name until it is given one; a browser an AI cannot read is a
        // browser nobody can debug from the CLI.
        use kittest::Queryable;
        let actions = Rc::new(RefCell::new(Vec::new()));
        let mut harness = harness(Rc::clone(&actions));
        harness.run();
        for label in [
            "Banks",
            "Categories",
            "Factory",
            "Cold Pads",
            "Bass",
            "Pad",
            "Sub bass",
            "Glass",
            "broken",
            "Import… (2)",
            "Export…",
            "New bank…",
            "Open folder",
            "Close",
            "Exported 2 presets",
            "3 presets",
        ] {
            assert!(
                harness.query_by_label(label).is_some(),
                "{label} is missing"
            );
        }
    }

    #[test]
    fn clicking_a_category_and_a_preset_report_their_rows() {
        use kittest::Queryable;
        let actions = Rc::new(RefCell::new(Vec::new()));
        let mut harness = harness(Rc::clone(&actions));
        harness.run();
        harness.get_by_label("Pad").click();
        harness.run();
        harness.get_by_label("Glass").click();
        harness.run();
        harness.get_by_label("broken").click();
        harness.run();
        let seen = actions.borrow().clone();
        assert_eq!(
            seen,
            vec![
                BrowserAction {
                    category: Some(2),
                    ..Default::default()
                },
                BrowserAction {
                    load: Some(1),
                    ..Default::default()
                },
            ],
            "a broken row is shown and cannot be chosen"
        );
    }

    /// The owner's asks (2026-09-04): the arrow keys navigate the presets and Enter closes, from
    /// anywhere in the browser; and left and right reach the banks and the categories.
    #[test]
    fn the_arrows_step_the_focused_pane_and_left_and_right_move_between_panes() {
        let actions = Rc::new(RefCell::new(Vec::new()));
        let mut harness = harness(Rc::clone(&actions));
        harness.run();
        for key in [
            Key::ArrowDown,  // presets: step
            Key::ArrowUp,    // presets: step back
            Key::ArrowLeft,  // to categories
            Key::ArrowDown,  // categories: the next row
            Key::ArrowLeft,  // to banks
            Key::ArrowDown,  // banks: the next row
            Key::ArrowUp,    // banks: back to the first — a change, so reported
            Key::ArrowUp,    // banks: already first, nothing to report
            Key::ArrowRight, // to categories
            Key::ArrowRight, // to presets
            Key::Enter,      // done
        ] {
            harness.key_press(key);
            harness.run();
        }
        let seen = actions.borrow().clone();
        let a = BrowserAction::default;
        assert_eq!(
            seen,
            vec![
                BrowserAction {
                    step: Some(1),
                    ..a()
                },
                BrowserAction {
                    step: Some(-1),
                    ..a()
                },
                BrowserAction {
                    focus: Some(Pane::Categories),
                    ..a()
                },
                BrowserAction {
                    category: Some(1),
                    ..a()
                },
                BrowserAction {
                    focus: Some(Pane::Banks),
                    ..a()
                },
                BrowserAction {
                    bank: Some(1),
                    ..a()
                },
                // the harness's bank stays 0 between frames, so up from the first is nothing
                BrowserAction {
                    focus: Some(Pane::Categories),
                    ..a()
                },
                BrowserAction {
                    focus: Some(Pane::Presets),
                    ..a()
                },
                BrowserAction { enter: true, ..a() },
            ]
        );
    }

    #[test]
    fn a_pane_is_a_fifth_of_the_width_within_bounds() {
        assert_eq!(pane_width(1200.0), 240.0);
        assert_eq!(pane_width(700.0), 150.0);
        assert_eq!(pane_width(1000.0), 200.0);
    }
}
