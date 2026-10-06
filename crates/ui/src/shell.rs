//! The mandatory shell: app bar, view bar, module card — design system §3.1 to §3.3.
//!
//! Every MXM interface wears the same frame. That is the point of §13 putting the shell in this
//! crate from day one rather than waiting for a second consumer: a shell that arrives after two
//! instruments exist is a shell that has to accommodate two sets of accidents.
//!
//! # What the shell does not do
//!
//! It does not own a window, and it does not lay out an instrument. It draws the frame and hands
//! back the space inside. mxm-mono-01's editor is one caller; the player is another; a future
//! instrument is a third, and none of them tells this module anything about itself beyond a title
//! and a list of views.

use egui::{Align, Color32, Layout, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::space::{HAIRLINE, MIN_TARGET, RADIUS, SPACE_2, SPACE_3, SPACE_5};
use crate::theme::Tokens;

/// **How far the app bar has given things up to fit its window** — design system §3.1's compact
/// bar (the owner, 2026-09-24). Each step keeps the one before it and gives up one more thing, and
/// the bar takes the first step whose measured width fits: a window only ever loses what it must.
/// Below the last step the preset group is cut off at the right-hand group's edge — never drawn
/// over it.
///
/// The step is chosen from widths the bar measured while drawing ([`BarWidths`]), never from typed
/// ones, and it holds only while the bar draws: a control outside an app bar is never compact.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum BarStep {
    /// Everything on the bar.
    #[default]
    Full,
    /// The theme and zoom controls move into the `…` menu.
    ThemeAndZoomInMenu,
    /// …and the preset name narrows to [`PRESET_NAME_WIDTH_COMPACT`].
    NarrowName,
    /// …and the product name hides. The wordmark stays; the window's title names the product.
    NoProductName,
    /// …and the favourite and *Save preset* move into the `…` menu.
    PatchActionsInMenu,
    /// …and the product's own bar actions ([`product_actions`]) move into the `…` menu. A bar with
    /// none reaches no further than the step before.
    ProductActionsInMenu,
}

impl BarStep {
    const ALL: [Self; 6] = [
        Self::Full,
        Self::ThemeAndZoomInMenu,
        Self::NarrowName,
        Self::NoProductName,
        Self::PatchActionsInMenu,
        Self::ProductActionsInMenu,
    ];
}

/// The step the app bar being drawn has taken, or `None` outside one — which is what keeps the
/// player's own theme control, and any control not in a bar, whole.
#[must_use]
pub fn bar_step(ctx: &egui::Context) -> Option<BarStep> {
    ctx.data(|d| d.get_temp(bar_step_id()))
}

fn bar_step_id() -> egui::Id {
    egui::Id::new("mxm-app-bar-step")
}

/// What the bar measured of itself: the width everything takes when nothing optional is drawn,
/// and each optional piece's own width with its spacing — recorded whenever the piece is drawn on
/// the bar, and kept while a step has moved it into the menu.
#[derive(Clone, Copy, Debug, Default)]
struct BarWidths {
    /// `None` until the bar has drawn once; the first frame is drawn whole.
    base: Option<f32>,
    /// Whether the last frame drew a `…` menu to take what the bar gives up —
    /// [`preset_browser`]'s. **A bar with no menu never compacts**: `AppBar::show`, with no
    /// presets, would otherwise hide its theme and zoom with nowhere to put them.
    menu: bool,
    /// The preset group's own width on the last frame, which is where its `…` menu ends.
    patch: f32,
    theme: f32,
    zoom: f32,
    product: f32,
    favourite: f32,
    save: f32,
    /// The product's own actions ([`product_actions`]); zero for a bar that has none.
    actions: f32,
}

impl BarWidths {
    /// What the pieces a bar at `step` still draws add to [`BarWidths::base`].
    fn pieces(&self, step: BarStep) -> f32 {
        let mut width = 0.0;
        if step < BarStep::ThemeAndZoomInMenu {
            width += self.theme + self.zoom;
        }
        if step < BarStep::NarrowName && self.menu {
            width += PRESET_NAME_WIDTH - PRESET_NAME_WIDTH_COMPACT;
        }
        if step < BarStep::NoProductName {
            width += self.product;
        }
        if step < BarStep::PatchActionsInMenu {
            width += self.favourite + self.save;
        }
        if step < BarStep::ProductActionsInMenu {
            width += self.actions;
        }
        width
    }

    /// Whether even the last step does not fit `available`: the preset group is then cut off at
    /// its left, so the `…` menu that holds what the bar gave up stays whole.
    fn overflows(&self, available: f32) -> bool {
        self.menu
            && self.base.is_some_and(|base| {
                base + self.pieces(BarStep::ProductActionsInMenu) > available + 0.5
            })
    }

    /// The first step whose width fits `available`, and the last when none does.
    fn step(&self, available: f32) -> BarStep {
        let Some(base) = self.base.filter(|_| self.menu) else {
            return BarStep::Full;
        };
        BarStep::ALL
            .into_iter()
            .find(|step| base + self.pieces(*step) <= available + 0.5)
            .unwrap_or(BarStep::ProductActionsInMenu)
    }
}

fn bar_widths_id() -> egui::Id {
    egui::Id::new("mxm-app-bar-widths")
}

/// Stands in for an optional piece a step has taken off the bar. **Every optional piece is drawn in
/// a scope of its own**, which takes one automatic id from the `Ui` around it, and a piece not
/// drawn skips that one id — so every widget after it keeps its id at every step. Without it, the
/// Volume slider the keyboard cursor had focused on one frame had another id on the next, and a
/// focus on a widget that no longer exists is what an accessibility tree refuses.
fn skip_piece(ui: &mut Ui) {
    ui.skip_ahead_auto_ids(1);
}

/// Records one optional piece's width, spacing included, when it was drawn on a bar.
fn record_piece(ctx: &egui::Context, width: f32, piece: impl FnOnce(&mut BarWidths) -> &mut f32) {
    ctx.data_mut(|d| {
        let widths = d.get_temp_mut_or_default::<BarWidths>(bar_widths_id());
        *piece(widths) = width;
    });
}

/// One of a product's own bar actions, as the `…` menu lists it once the bar has moved them there
/// ([`product_actions`]). Rows in, actions out, as the preset browser works: the menu draws the row
/// and reports which was chosen ([`take_product_action`]), and the product — which owns the
/// parameter or the command — does it.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    pub label: String,
    /// The tooltip while it can be chosen.
    pub hover: String,
    pub kind: MenuKind,
    /// Why it cannot be chosen now, shown as its tooltip; `None` when it can.
    pub disabled: Option<String>,
}

/// What a [`MenuItem`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    /// An on/off setting, drawn selected while on.
    Toggle(bool),
    /// A command.
    Action,
    /// A line of status: drawn, never chosen.
    Note,
}

fn product_items_id() -> egui::Id {
    egui::Id::new("mxm-app-bar-product-items")
}

fn product_choice_id() -> egui::Id {
    egui::Id::new("mxm-app-bar-product-choice")
}

/// **A product's own actions in the bar's right-hand group**: drawn by `draw` while the bar has
/// room, and listed in the `…` menu as `items` from [`BarStep::ProductActionsInMenu`], the last
/// step, when it has not (design system §3.1). Returns `draw`'s result, or `None` when the menu has
/// them this frame — the caller then collects a choice with [`take_product_action`] after the bar,
/// and takes anything it registered for the drawn form (a keyboard cursor bar card) off the screen.
///
/// Last to go, because a product puts an action in the bar only when it has a reason to: the
/// drum machine's kit-wide Resample and its export (owner, 2026-09-22).
pub fn product_actions<R>(
    ui: &mut Ui,
    items: Vec<MenuItem>,
    draw: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let step = bar_step(ui.ctx());
    if step.is_some_and(|step| step >= BarStep::ProductActionsInMenu) {
        ui.ctx()
            .data_mut(|d| d.insert_temp(product_items_id(), items));
        skip_piece(ui);
        return None;
    }
    ui.ctx()
        .data_mut(|d| d.remove::<Vec<MenuItem>>(product_items_id()));
    let spacing = ui.spacing().item_spacing.x;
    let drawn = ui.scope(draw);
    if step.is_some() {
        record_piece(ui.ctx(), drawn.response.rect.width() + spacing, |w| {
            &mut w.actions
        });
    }
    Some(drawn.inner)
}

/// The index of the [`MenuItem`] chosen from the `…` menu this frame, once.
#[must_use]
pub fn take_product_action(ctx: &egui::Context) -> Option<usize> {
    ctx.data_mut(|d| d.remove_temp::<usize>(product_choice_id()))
}

/// The persistent app bar, §3.1.
///
/// §3.1 lists six slots in order. mxm-mono-01 has no presets and no undo, and its brief records both
/// as deliberate deviations — so this builder makes each slot **optional but positional**: a
/// caller that has no presets omits them, and the slots that remain stay in the order §3.1 gives
/// them. The alternative, letting each instrument arrange its own bar, is how a collection stops
/// looking like one.
///
/// §3.1's closing rule is the one to keep in mind: *do not turn the app bar into a second
/// parameter panel.*
pub struct AppBar<'a> {
    product: &'a str,
    height: f32,
}

impl<'a> AppBar<'a> {
    /// §3.1 slot 1: the wordmark and product name.
    #[must_use]
    pub const fn new(product: &'a str) -> Self {
        Self {
            product,
            height: 44.0,
        }
    }

    /// Overrides the bar height. Rarely needed; the default is on the 4 px grid and leaves room
    /// for a 32 px pointer target with padding above and below.
    #[must_use]
    pub const fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Draws the bar. `right` fills slots 5 and 6 — utility, theme, help, and the output meter —
    /// laid out right to left, which is where §3.1 puts them.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        tokens: &Tokens,
        right: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        self.show_with(ui, tokens, |_| {}, right).map(|(_, r)| r)
    }

    /// Draws the bar with **slots 2 to 4** as well.
    ///
    /// `patch` fills the middle of §3.1's list — preset navigation, the browser trigger, and the
    /// favourite and save actions — laid out left to right, immediately after the wordmark and
    /// before the right-hand group. `right` is slots 5 and 6, unchanged.
    ///
    /// **A second entry point rather than a changed one.** `show` is what a caller with no presets
    /// wants and is most of them; making every caller pass an empty closure to gain a slot they do
    /// not use is how a shared API acquires ceremony. Both funnel into one implementation, so the
    /// bar cannot grow two layouts.
    ///
    /// §3.1's closing rule still applies and is easy to lose here: *do not turn the app bar into a
    /// second parameter panel.* Slots 2 to 4 are for the patch, not for the sound.
    pub fn show_with<P, R>(
        self,
        ui: &mut Ui,
        tokens: &Tokens,
        patch: impl FnOnce(&mut Ui) -> P,
        right: impl FnOnce(&mut Ui) -> R,
    ) -> Option<(P, R)> {
        let mut result = None;

        egui::Panel::top("mxm-app-bar")
            .exact_size(self.height)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(tokens.surface_1)
                    .inner_margin(egui::Margin::symmetric(SPACE_5 as i8, SPACE_2 as i8))
                    .stroke(Stroke::new(HAIRLINE, tokens.border)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let ctx = ui.ctx().clone();
                    let widths: BarWidths = ctx
                        .data(|d| d.get_temp(bar_widths_id()))
                        .unwrap_or_default();
                    let step = widths.step(ui.available_width());
                    let overflows = widths.overflows(ui.available_width());
                    ctx.data_mut(|d| {
                        d.insert_temp(bar_step_id(), step);
                        // Set again this frame by the menu that draws, if one does.
                        d.get_temp_mut_or_default::<BarWidths>(bar_widths_id()).menu = false;
                    });
                    let spacing = ui.spacing().item_spacing.x;

                    let mut wordmark = ui
                        .label(egui::RichText::new("MXM").color(tokens.accent).strong())
                        .rect
                        .width()
                        + spacing;
                    if step < BarStep::NoProductName {
                        let product = ui
                            .scope(|ui| {
                                ui.label(
                                    egui::RichText::new(self.product).color(tokens.text_primary),
                                )
                            })
                            .response
                            .rect
                            .width()
                            + spacing;
                        record_piece(&ctx, product, |w| &mut w.product);
                        wordmark += product;
                    } else {
                        skip_piece(ui);
                    }

                    // **The right-hand group is laid out first**, so it keeps its edge whatever the
                    // patch group turns out to be. Filling left to right first would let a long
                    // preset name push the utility menu off the bar — the interface moving because
                    // of what a preset happens to be called.
                    let (inner, right_width, patch_width) = ui
                        .with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let right = right(ui);
                            let right_width = ui.min_rect().width();
                            ui.add_space(SPACE_5);
                            // Left to right from here, so slots 2-4 read in §3.1's order — which is
                            // the order of the list, not a distribution across the bar. **Clipped
                            // to the room left**, so below the most compact step the preset group is
                            // cut off rather than drawn over the right-hand group — **at its left**:
                            // it ends against that group, so its `…` menu, which holds everything the
                            // bar gave up, stays whole and reachable.
                            let room = ui.available_rect_before_wrap();
                            let start = if overflows {
                                room.right() - widths.patch
                            } else {
                                room.left()
                            };
                            let (patch, patch_width) = ui
                                .scope_builder(
                                    egui::UiBuilder::new()
                                        .max_rect(Rect::from_min_max(
                                            egui::pos2(start, room.top()),
                                            room.max,
                                        ))
                                        .layout(Layout::left_to_right(Align::Center)),
                                    |ui| {
                                        ui.set_clip_rect(ui.clip_rect().intersect(room));
                                        let patch = patch(ui);
                                        (patch, ui.min_rect().width())
                                    },
                                )
                                .inner;
                            ((patch, right), right_width, patch_width)
                        })
                        .inner;
                    result = Some(inner);

                    // The width this bar would take with nothing optional on it, from what it just
                    // drew: the next frame's step is chosen from it. **The gap between the groups
                    // is `SPACE_5` and the item spacing**: `add_space` comes on top of the spacing
                    // egui leaves after the right-hand group's last widget. Counting `SPACE_5`
                    // alone chose a step one spacing too wide, and the `…` menu was cut short.
                    ctx.data_mut(|d| {
                        let widths = d.get_temp_mut_or_default::<BarWidths>(bar_widths_id());
                        let drawn = wordmark + patch_width + SPACE_5 + spacing + right_width;
                        widths.base = Some(drawn - widths.pieces(step));
                        widths.patch = patch_width;
                        d.remove::<BarStep>(bar_step_id());
                    });
                });
            });

        result
    }
}

/// One row in a [`preset_browser`], as the caller sees it.
///
/// Deliberately not a preset: `crates/ui` knows nothing about parameters, files or plugins, and a
/// browser that did would be a browser only this instrument could use. A row is a name, a note about
/// where it came from, and whether it can be chosen.
pub struct PresetRow<'a> {
    pub name: &'a str,
    /// A short word for where it came from — shown quietly beside the name, because a factory and a
    /// user preset sharing a name is routine and not a warning.
    pub origin: &'a str,
    /// Starred. **Marked in the list, and the caller puts starred rows first** — a star that only
    /// lit up on the bar would record a preference nothing acted on, which is a control that does
    /// nothing dressed as one that does something.
    pub favourite: bool,
    /// `Some` for a row that cannot be chosen, carrying the reason. A file that is present and
    /// broken is shown disabled rather than hidden: omitting it would tell somebody their preset
    /// had vanished.
    pub problem: Option<&'a str>,
}

/// What a person did in the preset controls this frame.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct PresetAction {
    /// A row was chosen. The index is into the slice that was passed in.
    pub load: Option<usize>,
    /// The name was clicked: open the browser, or close it.
    pub browse: bool,
    /// The arrows: the previous or next **selectable** row.
    pub step: Option<i32>,
    pub save: bool,
    pub save_as: bool,
    pub rename: bool,
    pub delete: bool,
    pub favourite: bool,
    pub init: bool,
}

impl PresetAction {
    pub const fn any(&self) -> bool {
        self.browse
            || self.load.is_some()
            || self.step.is_some()
            || self.save
            || self.save_as
            || self.rename
            || self.delete
            || self.favourite
            || self.init
    }
}

/// How the current patch stands, for the name the browser shows.
pub struct PresetState<'a> {
    /// `None` is *no preset* — a fresh instance, or after Init.
    pub name: Option<&'a str>,
    /// Whether the values have moved since it was loaded.
    pub modified: bool,
    pub favourite: bool,
    /// `Some` when the loaded preset cannot be written to — a factory preset, or no config
    /// directory. Shown on the control rather than discovered when Save does nothing.
    pub read_only: Option<&'a str>,
    /// `Some` temporarily disables Save and Save As while a complete deferred patch is pending.
    /// Navigation and Init remain live so a newer request can supersede the work.
    pub save_disabled: Option<&'a str>,
}

/// §3.1 slots 2 to 4: preset navigation, the name and browser, and the patch actions.
///
/// **Init lives here, beside Save**, and not in the utility menu. It was put there when slots 2 to 4
/// had no API at all — a decision taken to avoid designing a shared API for one button, with the
/// stated intent that it would move once presets arrived. This is that move.
///
/// **The name is a fixed-width button**, because it is the widest thing in the bar and the one whose
/// content changes most: sizing it to the name would shift the arrows and every action beside them
/// each time a preset was loaded.
pub fn preset_browser(ui: &mut Ui, tokens: &Tokens, state: &PresetState<'_>) -> PresetAction {
    let mut action = PresetAction::default();

    // **One height for everything in this group, set once.** egui sizes a button, a combo box and a
    // menu button from `interact_size.y`, so setting it here reaches all of them — including the
    // `ComboBox`, which has no height of its own and came out sixteen pixels tall beside
    // thirty-two-pixel arrows. That mix is what read as the bar having grown; the bar had not moved.
    //
    // `MIN_TARGET` is §4.3's pointer floor, so this is the smallest these may legally be.
    ui.spacing_mut().interact_size.y = MIN_TARGET;
    let step = bar_step(ui.ctx());
    let at = |from: BarStep| step.is_some_and(|step| step >= from);
    let spacing = ui.spacing().item_spacing.x;

    let arrow = Vec2::splat(MIN_TARGET);
    if ui
        .add_sized(arrow, egui::Button::new("‹"))
        .on_hover_text("Previous preset")
        .clicked()
    {
        action.step = Some(-1);
    }

    // The name, and the whole browser behind it.
    let label = match state.name {
        // A marker rather than a different colour: §7.2 wants state to show through more than hue,
        // and this one has to read in a bar that is already mostly text.
        Some(name) if state.modified => format!("{name} •"),
        Some(name) => name.to_owned(),
        None => "No preset".to_owned(),
    };
    // §7.2: state through more than hue. The bullet is the shape; `warning` is the colour, and it
    // is the theme's rather than one picked here — light-theme amber is the classic way to make an
    // unreadable marker, and `crates/ui` already solved that once.
    let label = egui::RichText::new(label).color(if state.modified {
        tokens.warning
    } else {
        tokens.text_primary
    });

    // **The name is the browser's door.** It used to open a dropdown of every preset; a flat list
    // does not survive a shared bank, so the click opens the three-pane browser under the bar
    // (`crate::browser`), and the caller — who knows the banks and the files — decides what it shows.
    if ui
        .add_sized(
            Vec2::new(
                if at(BarStep::NarrowName) {
                    PRESET_NAME_WIDTH_COMPACT
                } else {
                    PRESET_NAME_WIDTH
                },
                MIN_TARGET,
            ),
            egui::Button::new(label),
        )
        .on_hover_text("Browse presets")
        .clicked()
    {
        action.browse = true;
    }

    if ui
        .add_sized(arrow, egui::Button::new("›"))
        .on_hover_text("Next preset")
        .clicked()
    {
        action.step = Some(1);
    }

    ui.add_space(SPACE_2);

    // Slot 3: the patch actions — on the bar, or in the `…` menu at the bar's last step.
    let star = if state.favourite { "★" } else { "☆" };
    let actions_in_menu = at(BarStep::PatchActionsInMenu);
    if actions_in_menu {
        skip_piece(ui);
    } else {
        let favourite = ui
            .scope(|ui| ui.add_sized(arrow, egui::Button::new(star)))
            .inner
            .on_hover_text("Favourite");
        if step.is_some() {
            record_piece(ui.ctx(), favourite.rect.width() + spacing, |w| {
                &mut w.favourite
            });
        }
        if favourite.clicked() {
            action.favourite = true;
        }
    }

    // **"Save preset", and the name decides what happens** (owner, 2026-09-22). It always asks
    // for a name, prefilled with the loaded one: keep it and the preset is replaced — after the
    // naming row's own "A preset of that name exists / Replace it" — change it and a new one is
    // written. One path, and the button can be read literally.
    //
    // It used to say "Save" and branch invisibly: with a factory preset loaded it silently became
    // Save As, so the nine presets a person actually starts from turned a button labelled Save
    // into a dialog. The owner asked what Save saved, twice, which is the evidence that the
    // branch was unreadable.
    let save_button = |ui: &mut Ui, text: &str| {
        let save = ui.add_enabled(state.save_disabled.is_none(), egui::Button::new(text));
        match state.save_disabled {
            Some(why) => save.on_disabled_hover_text(why),
            None => save.on_hover_text(
                "Name this patch and keep it. The same name replaces that preset; a new name adds one.",
            ),
        }
    };
    if actions_in_menu {
        skip_piece(ui);
    } else {
        let save = ui.scope(|ui| save_button(ui, "Save preset")).inner;
        if step.is_some() {
            record_piece(ui.ctx(), save.rect.width() + spacing, |w| &mut w.save);
        }
        if save.clicked() {
            action.save_as = true;
        }
    }

    if step.is_some() {
        ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<BarWidths>(bar_widths_id()).menu = true;
        });
    }
    ui.menu_button("…", |ui| {
        if actions_in_menu {
            if save_button(ui, "Save preset…").clicked() {
                action.save_as = true;
                ui.close();
            }
            if ui
                .button(format!("{star} Favourite"))
                .on_hover_text("Favourite")
                .clicked()
            {
                action.favourite = true;
                ui.close();
            }
            ui.separator();
        }
        // No "Save as…" here any more: now that Save preset always asks for a name, the two were
        // the same click through two doors.
        let rename = ui.add_enabled(state.read_only.is_none(), egui::Button::new("Rename…"));
        if rename.clicked() {
            action.rename = true;
            ui.close();
        }
        let delete = ui.add_enabled(state.read_only.is_none(), egui::Button::new("Delete"));
        let delete = match state.read_only {
            Some(why) => delete.on_disabled_hover_text(why),
            None => delete,
        };
        if delete.clicked() {
            action.delete = true;
            ui.close();
        }
        ui.separator();
        if ui
            .button("Init patch")
            .on_hover_text("Return every parameter to its default")
            .clicked()
        {
            action.init = true;
            ui.close();
        }
        // The product's own actions, at the bar's last step: rows the product listed, the choice
        // reported back rather than acted on here.
        let items = at(BarStep::ProductActionsInMenu)
            .then(|| {
                ui.ctx()
                    .data(|d| d.get_temp::<Vec<MenuItem>>(product_items_id()))
            })
            .flatten()
            .unwrap_or_default();
        if !items.is_empty() {
            ui.separator();
        }
        for (index, item) in items.iter().enumerate() {
            let button = match item.kind {
                MenuKind::Note => {
                    ui.label(egui::RichText::new(&item.label).color(tokens.text_secondary));
                    continue;
                }
                MenuKind::Toggle(on) => egui::Button::new(&item.label).selected(on),
                MenuKind::Action => egui::Button::new(&item.label),
            };
            let response = ui.add_enabled(item.disabled.is_none(), button);
            let response = match &item.disabled {
                Some(why) => response.on_disabled_hover_text(why),
                None => response.on_hover_text(&item.hover),
            };
            if response.clicked() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(product_choice_id(), index));
                ui.close();
            }
        }
        // What the bar's right-hand group gave up, labelled here because the menu has no group to
        // say what a bare value is.
        if at(BarStep::ThemeAndZoomInMenu) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Theme");
                if let Some(chosen) = theme_selector(ui) {
                    let _ = crate::theme::store(chosen);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Scale");
                zoom_selector(ui);
            });
        }
    });

    action
}

/// How wide the preset name is, whatever it says.
///
/// Fixed because it is the widest thing in the bar and the one whose content changes most: sizing it
/// to the name would shift the arrows and every action beside them each time a preset was loaded,
/// which is the defect `crates/ui`'s *nothing changes size* rule exists to prevent.
const PRESET_NAME_WIDTH: f32 = 168.0;

/// How wide the preset name is once the bar has narrowed it ([`BarStep::NarrowName`]): room for
/// *No preset* and a short name, still one fixed width whatever it says.
const PRESET_NAME_WIDTH_COMPACT: f32 = 96.0;

/// The next selectable row in `direction`, wrapping, or `None` when nothing can be chosen.
///
/// **Skips the broken rows.** An arrow that stops on a file it cannot load is an arrow that appears
/// to do nothing, and the rows are shown precisely because they are unloadable.
pub fn step_preset(
    rows: &[PresetRow<'_>],
    current: Option<usize>,
    direction: i32,
) -> Option<usize> {
    let selectable: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.problem.is_none())
        .map(|(index, _)| index)
        .collect();
    if selectable.is_empty() {
        return None;
    }

    let at = current
        .and_then(|current| selectable.iter().position(|index| *index == current))
        .map_or(0, |at| {
            let len = selectable.len() as i32;
            (((at as i32 + direction) % len) + len) as usize % selectable.len()
        });
    selectable.get(at).copied()
}

/// The view bar, §3.2.
///
/// `show` retains a singleton for existing authored-view callers. `show_paged` hides it, for
/// derived pages. Both use uniform text-measured cells and the same wrapping geometry.
pub struct ViewBar<'a> {
    views: &'a [&'a str],
}

impl<'a> ViewBar<'a> {
    #[must_use]
    pub const fn new(views: &'a [&'a str]) -> Self {
        Self { views }
    }

    /// Geometry for derived navigation. Zero/one page consumes no height. Width includes the
    /// bar's outer gutters; labels and painting use the same Button font and pointer floor.
    pub fn geometry(&self, ui: &Ui, width: f32) -> ViewBarGeometry {
        self.geometry_impl(ui, width, true)
    }

    fn geometry_impl(&self, ui: &Ui, width: f32, hide_single: bool) -> ViewBarGeometry {
        if self.views.is_empty() || (hide_single && self.views.len() == 1) {
            return ViewBarGeometry::default();
        }
        let cell_width = self
            .views
            .iter()
            .map(|label| crate::control::segment_min_width(ui, &[*label]))
            .fold(MIN_TARGET, f32::max);
        let available = (width - 2.0 * SPACE_5).max(0.0);
        let columns = (((available + SPACE_2) / (cell_width + SPACE_2)).floor() as usize)
            .max(1)
            .min(self.views.len());
        let rows = self.views.len().div_ceil(columns);
        ViewBarGeometry {
            cell_width,
            columns,
            rows,
            height: rows as f32 * MIN_TARGET
                + rows.saturating_sub(1) as f32 * SPACE_2
                + 2.0 * SPACE_2,
            overflow: cell_width > available,
        }
    }

    /// Draw authored views; retain a singleton for compatibility until editors adopt paging.
    pub fn show(self, ui: &mut Ui, tokens: &Tokens, selected: &mut usize) -> bool {
        self.show_impl(ui, tokens, selected, false)
    }

    /// Draw derived pages. A one-page editor has no navigation at all.
    pub fn show_paged(self, ui: &mut Ui, tokens: &Tokens, selected: &mut usize) -> bool {
        self.show_impl(ui, tokens, selected, true)
    }

    fn show_impl(
        self,
        ui: &mut Ui,
        tokens: &Tokens,
        selected: &mut usize,
        hide_single: bool,
    ) -> bool {
        let geometry = self.geometry_impl(ui, ui.available_width(), hide_single);
        if geometry.rows == 0 {
            return false;
        }
        let mut changed = false;
        let tabs_id = ui.make_persistent_id("mxm-view-bar-tabs");

        egui::Panel::top("mxm-view-bar")
            .exact_size(geometry.height)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(tokens.canvas)
                    .inner_margin(egui::Margin::symmetric(SPACE_5 as i8, SPACE_2 as i8)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(SPACE_2);
                for (row, views) in self.views.chunks(geometry.columns).enumerate() {
                    ui.horizontal(|ui| {
                        for (column, view) in views.iter().enumerate() {
                            let index = row * geometry.columns + column;
                            let is_selected = index == *selected;
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::new(geometry.cell_width, MIN_TARGET),
                                Sense::hover(),
                            );
                            // A wrap changes the row Ui's auto-ID, not the tab's identity.
                            let response = ui.interact(rect, tabs_id.with(index), Sense::click());
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::SelectableLabel,
                                    ui.is_enabled(),
                                    is_selected,
                                    *view,
                                )
                            });

                            // Selected state through fill, border **and** an underline: §7.2 forbids
                            // hue alone, and a view bar is exactly where a colour-blind user would
                            // otherwise lose their place.
                            let (fill, border, text) = if is_selected {
                                (tokens.selection, tokens.accent, tokens.text_primary)
                            } else if response.hovered() || response.has_focus() {
                                (tokens.surface_3, tokens.border_strong, tokens.text_primary)
                            } else {
                                (tokens.surface_2, tokens.border, tokens.text_secondary)
                            };

                            let painter = ui.painter();
                            painter.rect_filled(rect, RADIUS as f32, fill);
                            painter.rect_stroke(
                                rect,
                                RADIUS as f32,
                                if response.has_focus() {
                                    Stroke::new(2.0 * HAIRLINE, tokens.focus)
                                } else {
                                    Stroke::new(HAIRLINE, border)
                                },
                                StrokeKind::Inside,
                            );
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                *view,
                                egui::TextStyle::Button.resolve(ui.style()),
                                text,
                            );
                            if is_selected {
                                let y = rect.bottom() - 2.0;
                                painter.line_segment(
                                    [
                                        egui::Pos2::new(rect.left() + SPACE_3, y),
                                        egui::Pos2::new(rect.right() - SPACE_3, y),
                                    ],
                                    Stroke::new(2.0, tokens.accent),
                                );
                            }

                            if response.clicked() && !is_selected {
                                *selected = index;
                                changed = true;
                            }
                        }
                    });
                }
            });

        changed
    }
}

/// Exact geometry used by both reservation and painting. An over-wide word is reported rather
/// than squeezed below its text/pointer floor; a paged renderer must provide a compact fallback.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewBarGeometry {
    pub cell_width: f32,
    pub columns: usize,
    pub rows: usize,
    pub height: f32,
    pub overflow: bool,
}

/// A module card, §3.3: one coherent operation — an oscillator, a filter, an envelope.
///
/// §3.3's rule about appearance is the one that keeps this collection on the right side of §2:
///
/// > Cards use flat surfaces, a one-pixel border, and spacing — not simulated panels — to
/// > establish groups.
///
/// So: a flat fill, a hairline, and padding. No bevel, no gradient, no inset shadow, no screw.
pub struct ModuleCard<'a> {
    title: &'a str,
    /// §3.3 allows an enable/bypass in the header. `None` means the module cannot be bypassed —
    /// which is mxm-mono-01's case throughout, since none of its six sections is optional.
    enabled: Option<&'a mut bool>,
}

impl<'a> ModuleCard<'a> {
    #[must_use]
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            enabled: None,
        }
    }

    /// Adds the header's enable/bypass toggle.
    ///
    /// §7.2: bypass is not deletion, and §3.3: a disabled module stays legible at reduced
    /// emphasis rather than being hidden.
    #[must_use]
    pub fn bypassable(mut self, enabled: &'a mut bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Draws the card and calls `body` with the space inside it.
    pub fn show<R>(self, ui: &mut Ui, tokens: &Tokens, body: impl FnOnce(&mut Ui) -> R) -> R {
        let is_enabled = self.enabled.as_ref().is_none_or(|e| **e);

        egui::Frame::new()
            .fill(tokens.surface_1)
            .stroke(Stroke::new(HAIRLINE, tokens.border))
            .corner_radius(RADIUS)
            .inner_margin(egui::Margin::same(SPACE_5 as i8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.title)
                            .heading()
                            .color(if is_enabled {
                                tokens.text_primary
                            } else {
                                tokens.text_disabled
                            }),
                    );

                    if let Some(enabled) = self.enabled {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            crate::control::toggle(
                                ui,
                                tokens,
                                "On",
                                enabled,
                                false,
                                "Bypasses this module. Bypass is not deletion; settings are kept.",
                            );
                        });
                    }
                });

                ui.add_space(SPACE_3);
                separator(ui, tokens);
                ui.add_space(SPACE_3);

                // A disabled module reads at reduced emphasis but stays operable-looking and
                // legible: §3.3 forbids hiding it or collapsing it to an unexplained icon.
                ui.scope(|ui| {
                    // The card body's own rhythm. egui's default is 3.0 — off §4.1's four-point
                    // grid, and so close to nothing that a toggle sat against the segmented
                    // control above it as though the two were one control. §4.1 asks for related
                    // things to be closer than unrelated ones, and a card's direct children are
                    // siblings, not a group: `SPACE_3` here against `SPACE_2` inside a `group`.
                    ui.spacing_mut().item_spacing.y = SPACE_3;
                    if !is_enabled {
                        ui.set_opacity(0.55);
                    }
                    body(ui)
                })
                .inner
            })
            .inner
    }
}

/// The narrowest a [`ModuleCard`] titled `title` can be drawn around a body `body` wide: the body,
/// or the title if that is wider, plus the card's `SPACE_5` padding and hairline on each side.
///
/// With [`crate::tree::Node::min_width`] this is a card's floor as arithmetic rather than a number
/// typed by hand (the `new-layout` prototype).
#[must_use]
pub fn card_floor(ui: &Ui, title: &str, body: f32) -> f32 {
    let heading = egui::TextStyle::Heading.resolve(ui.style());
    let title = ui
        .painter()
        .layout_no_wrap(title.to_owned(), heading, Color32::PLACEHOLDER)
        .size()
        .x;
    body.max(title) + 2.0 * (SPACE_5 + HAIRLINE)
}

/// The height of a [`ModuleCard`] titled `title` around a body `body` tall, drawn in `ui`: the
/// padding and hairline top and bottom, the title row, the separator between two `SPACE_3`s, and
/// the item spacing `ui` puts after the title and after the separator.
#[must_use]
pub fn card_height(ui: &Ui, title: &str, body: f32) -> f32 {
    // The title never wraps: one heading line, as tall as the laid-out text — which is not the
    // font's row height (19.0 against 19.375 in Inter's heading size).
    let heading = egui::TextStyle::Heading.resolve(ui.style());
    let line = ui
        .painter()
        .layout_no_wrap(
            if title.is_empty() { "A" } else { title }.to_owned(),
            heading,
            Color32::PLACEHOLDER,
        )
        .size()
        .y;
    let header = ui.spacing().interact_size.y.max(line);
    let gap = ui.spacing().item_spacing.y;
    2.0 * (SPACE_5 + HAIRLINE) + header + gap + SPACE_3 + HAIRLINE + gap + SPACE_3 + body
}

/// A `Ui` with a card body's style — [`ModuleCard`]'s `SPACE_3` rhythm — for measuring a body
/// that has not been drawn (`crate::tree`). It is a child of `ui` that allocates nothing.
#[must_use]
pub fn body_ui(ui: &mut Ui) -> Ui {
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
    body.spacing_mut().item_spacing.y = SPACE_3;
    body
}

/// A group of related controls inside a card body: the hairline panel of §3.3.
///
/// §3.3 says groups are established by "flat surfaces, a one-pixel border, and spacing — not
/// simulated panels". This is that sentence drawn: [`ModuleCard`]'s own outline at the nested
/// radius, with no header and no bypass. Anything that needs either of those is a card.
///
/// **It draws no fill.** It carried `surface_2` first — §5.1's nested-group tint — and the owner
/// reported it as too much visual weight beside everything else on the card, which it was: a
/// group is a boundary, not a surface, and the tint also swallowed the inputs drawn on that same
/// token inside it. The border alone says where the group ends, and leaves the tint to mean
/// *control*.
///
/// **It sets the body's vertical rhythm to `SPACE_2`, and that is the point of it.** A card body
/// inherits egui's default `item_spacing.y` of 3.0 — off the 4-point grid §4.1 requires, and so
/// nearly equal to the space between one group and the next that a column of routing controls read
/// as one undifferentiated list. The owner's word for it was *indecipherable*. The ladder a group
/// establishes is 4 inside against the 8 plus a border outside, which is §4.1's proximity rule:
/// related controls closer together than unrelated ones.
///
/// The caller owns the space **above** the group, since only the caller knows whether something
/// precedes it.
pub fn group<R>(ui: &mut Ui, tokens: &Tokens, body: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .stroke(Stroke::new(HAIRLINE, tokens.border))
        .corner_radius(RADIUS)
        .inner_margin(egui::Margin::same(SPACE_3 as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = SPACE_2;
            body(ui)
        })
        .inner
}

/// What `level_columns` remembers between frames, and what a test reads back: each column's
/// **natural** height — what it needed with nothing added — and where each actually ended.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Levelled {
    pub natural: Vec<f32>,
    pub bottoms: Vec<f32>,
}

/// What `level_columns` (or a `Level` driven by hand) stored under `id`, once a frame has run.
pub fn levelled(ctx: &egui::Context, id: egui::Id) -> Option<Levelled> {
    ctx.data(|d| d.get_temp(id))
}

/// Columns of cards that end on one line — §3.3: cards laid out side by side share a bottom edge.
///
/// Immediate mode cannot know the tallest column before the others are drawn, so each column's
/// natural height is measured this frame and the difference handed out the next: `draw(column,
/// index, deficit)` draws one column and **shares `deficit` equally among its cards, inside them**
/// — `grow(ui, share)` at each card's foot, or a display that is strictly better bigger, as
/// mxm-mono-03's and mxm-poly-06's filter curves are. Inside the cards, so it is their bottoms that
/// line up rather than the empty space below them; shared, because a column's whole difference in
/// its last card read as a card with nothing in it.
///
/// What is stored is the natural height with the deficit subtracted back out; storing the padded
/// height would feed the correction into itself and the columns would creep. The memory is egui's,
/// keyed by `id` — one per view — because a panel here is a free function over borrowed state. A
/// frame is requested whenever the heights moved (the first frame, an expander opening), so the
/// alignment is at most one frame behind, and settled by the third.
///
/// mxm-mono-01 carried this inline first, mxm-mono-03 spent the difference on its curve and
/// mxm-mono-02 filled to the panel's bottom instead: three honest copies, and the owner's rule that
/// every instrument's cards end level, is what put it here.
pub fn level_columns(
    ui: &mut Ui,
    id: egui::Id,
    count: usize,
    mut draw: impl FnMut(&mut Ui, usize, f32),
) {
    let mut level = Level::load(ui.ctx(), id, count);
    ui.columns(count, |columns| {
        for (index, column) in columns.iter_mut().enumerate() {
            let top = column.cursor().top();
            let deficit = level.deficit(index);
            draw(column, index, deficit);
            level.record(index, top, column.cursor().top(), deficit);
        }
    });
    level.store(ui.ctx());
}

/// Grows the card being drawn by exactly `by` points at its foot — how a column's share of the
/// levelling is spent.
///
/// **Not `Ui::add_space`.** The cursor already sits one item spacing below the last widget, and
/// `add_space` commits that spacing to the card along with the amount, and only when it is called:
/// a padded card came out six points taller than its padding and an unpadded one not at all. The
/// leveller subtracts only the padding, so a padded column measured six points taller than it was,
/// and where two columns were within six points of each other the tallest flipped every frame
/// (mxm-mono-01 at 900 px, caught by its own paint tests). This extends the card's rectangle and
/// nothing else, so zero grows it by zero and the measurement is continuous.
pub fn grow(ui: &mut Ui, by: f32) {
    let foot = egui::pos2(ui.min_rect().left(), ui.min_rect().bottom() + by.max(0.0));
    ui.expand_to_include_rect(Rect::from_min_max(foot, foot));
}

/// The measurement behind `level_columns`, for a row of cards that is not `Ui::columns` — cards
/// told their own widths, as mxm-mono-00's Patch view tells its matrix and its sample-and-hold.
///
/// `load` before the row, `deficit` for each card as it is drawn, `record` after it, `store` once
/// the row is done. The rules are `level_columns`'s.
pub struct Level {
    id: egui::Id,
    was: Levelled,
    now: Levelled,
}

impl Level {
    /// Last frame's measurement of `count` columns under `id`, or nothing if there was none or the
    /// count changed — in which case every deficit is zero and this frame measures afresh.
    #[must_use]
    pub fn load(ctx: &egui::Context, id: egui::Id, count: usize) -> Self {
        let was = levelled(ctx, id)
            .filter(|l| l.natural.len() == count)
            .unwrap_or_default();
        Self {
            id,
            was,
            now: Levelled {
                natural: vec![0.0; count],
                bottoms: vec![0.0; count],
            },
        }
    }

    /// How much shorter than the tallest column this one was last frame. Zero for the tallest,
    /// and zero on the first frame.
    #[must_use]
    pub fn deficit(&self, index: usize) -> f32 {
        let tallest = self.was.natural.iter().copied().fold(0.0_f32, f32::max);
        self.was
            .natural
            .get(index)
            .map_or(0.0, |height| (tallest - height).max(0.0))
    }

    /// Where the column began and ended this frame, and the deficit it was handed — which is
    /// subtracted back out, so what is remembered is the natural height.
    pub fn record(&mut self, index: usize, top: f32, bottom: f32, deficit: f32) {
        if let Some(natural) = self.now.natural.get_mut(index) {
            *natural = bottom - top - deficit;
        }
        if let Some(end) = self.now.bottoms.get_mut(index) {
            *end = bottom;
        }
    }

    /// Remembers this frame's measurement, and asks for a frame if the natural heights moved.
    pub fn store(self, ctx: &egui::Context) {
        let moved = |now: &[f32], was: &[f32]| {
            now.len() != was.len() || now.iter().zip(was).any(|(a, b)| (a - b).abs() > 0.5)
        };
        let natural_moved = moved(&self.now.natural, &self.was.natural);
        if natural_moved || moved(&self.now.bottoms, &self.was.bottoms) {
            ctx.data_mut(|d| d.insert_temp(self.id, self.now));
        }
        if natural_moved {
            ctx.request_repaint();
        }
    }
}

/// A hairline separator on the design system's border token.
/// Where a [`disclosure`] keeps its open flag.
///
/// Public so a reachability check can open one: the keyboard cursor's coverage test paints the real
/// panel, and a control behind a closed disclosure is legitimately absent rather than unreachable.
#[must_use]
pub fn disclosure_id(label: &str) -> egui::Id {
    egui::Id::new(("mxm-disclosure", label))
}

/// Design system §5's labelled disclosure: an expander, never a hover-reveal.
///
/// **Shared rather than a fourth private copy.** `mxm-mono-00`, `mxm-mono-01`, `mxm-mono-02` and
/// `mxm-poly-06` each wrote their own. In a card, it is `crate::tree::disclosure`, which reserves
/// the open body whether or not it is open — a card sized closed and painted open throws its own
/// controls off the bottom.
///
/// The state is the editor's, remembered by egui between frames; nothing of the sound depends on it.
pub fn disclosure(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    description: &str,
    body: impl FnOnce(&mut Ui),
) {
    ui.add_space(crate::space::SPACE_3);
    separator(ui, tokens);
    ui.add_space(crate::space::SPACE_2);
    let id = disclosure_id(label);
    let mut open = ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    crate::control::toggle(ui, tokens, label, &mut open, false, description);
    ui.data_mut(|d| d.insert_temp(id, open));
    if open {
        ui.add_space(crate::space::SPACE_2);
        body(ui);
    }
}

pub fn separator(ui: &mut Ui, tokens: &Tokens) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), HAIRLINE), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, tokens.border);
}

/// The theme control, §3.1 slot 5 and §10: Dark, Light and System, in the app bar.
///
/// **One control for the whole collection**, in `crates/ui` rather than in each editor, for the
/// reason [`AppBar`] itself gives: a bar each product arranges for itself is how a collection stops
/// looking like one. Every editor and the player draw this same widget in the same slot.
///
/// It is a [`egui::ComboBox`] carrying the current choice as its own label, which is the shape the
/// zoom control beside it already uses for *pick one of these* in this bar. §10's "switching is
/// immediate" is why the context is set here rather than by the caller: a control that only
/// reported the click would have every caller re-implementing the one thing it is for.
///
/// **The caller remembers it, and this does not.** The player keeps its choice in its own settings
/// file and an editor in [`crate::theme::store`]; this crate knows neither. Returns the chosen
/// preference on the frame it changes, and `None` on every other frame.
pub fn theme_control(ui: &mut Ui) -> Option<egui::ThemePreference> {
    on_bar(
        ui,
        BarStep::ThemeAndZoomInMenu,
        |w| &mut w.theme,
        theme_selector,
    )
    .flatten()
}

/// Draws a piece of the app bar's right-hand group unless the bar's step has moved it into the `…`
/// menu, and records its width when it drew it on a bar ([`BarStep`]). Outside a bar it always
/// draws.
fn on_bar<R>(
    ui: &mut Ui,
    moves_at: BarStep,
    piece: impl FnOnce(&mut BarWidths) -> &mut f32,
    draw: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let step = bar_step(ui.ctx());
    if step.is_some_and(|step| step >= moves_at) {
        skip_piece(ui);
        return None;
    }
    let spacing = ui.spacing().item_spacing.x;
    let drawn = ui.scope(draw);
    if step.is_some() {
        record_piece(ui.ctx(), drawn.response.rect.width() + spacing, piece);
    }
    Some(drawn.inner)
}

/// The theme selector itself, wherever it is drawn: on the bar or in its `…` menu.
fn theme_selector(ui: &mut Ui) -> Option<egui::ThemePreference> {
    let current = ui.ctx().options(|options| options.theme_preference);
    let mut chosen = None;

    const PREFERENCES: [egui::ThemePreference; 3] = [
        egui::ThemePreference::Light,
        egui::ThemePreference::Dark,
        egui::ThemePreference::System,
    ];

    let labels: Vec<&str> = PREFERENCES
        .iter()
        .map(|preference| crate::theme::label_of(*preference))
        .collect();
    let mut selected = PREFERENCES
        .iter()
        .position(|preference| *preference == current)
        .unwrap_or(0);
    let displayed = labels[selected];

    if crate::control::selector_inline(
        ui,
        crate::theme::tokens(ui.ctx()),
        "Theme",
        &labels,
        &mut selected,
        None,
        None,
        displayed,
        "Light, dark, or whatever this computer is set to. Remembered for every MXM editor, \
         and never carried by a preset.",
    ) {
        let preference = PREFERENCES[selected];
        ui.ctx().set_theme(preference);
        chosen = Some(preference);
    }

    chosen
}

/// [`theme_control`], remembering the choice for every MXM editor.
///
/// What the eight editors want, in the one line they all write: the plain control leaves storage
/// to its caller because the player keeps its choice in its own settings file, and that is the
/// only caller for which that is true.
///
/// **A failed write is not reported.** The theme is on screen either way, the only loss is that
/// the next editor opens on the old one, and there is nowhere to report it to that would not be
/// worse — an editor runs inside somebody else's DAW, and a preference file is not worth a banner
/// across the interface, let alone a panic.
pub fn editor_theme_control(ui: &mut Ui) {
    if let Some(chosen) = theme_control(ui) {
        let _ = crate::theme::store(chosen);
    }
}

/// The zoom steps every editor offers, §4.2's 75–200% range.
const ZOOM_STEPS: [(f32, &str); 5] = [
    (0.75, "75%"),
    (1.0, "100%"),
    (1.25, "125%"),
    (1.5, "150%"),
    (2.0, "200%"),
];

/// How large the instrument is drawn — §3.1's right-hand group, beside the theme control.
///
/// **Nine editors carried a byte-identical copy of this**, each with its own `ZOOM_STEPS` and its
/// own combo box, which is eight more than the shared crate's own rule allows. It is the same
/// control doing the same job in every one of them, and it now looks the same in every one of them
/// too: the caret selector, as the owner asked for the theme control beside it.
///
/// Setting the context's zoom is the whole implementation — the host resizes the window to match,
/// so the instrument scales and the window follows. The layout never changes: every control stays
/// exactly where it was, larger or smaller.
pub fn zoom_control(ui: &mut Ui) {
    on_bar(
        ui,
        BarStep::ThemeAndZoomInMenu,
        |w| &mut w.zoom,
        zoom_selector,
    );
}

/// The zoom selector itself, wherever it is drawn: on the bar or in its `…` menu.
fn zoom_selector(ui: &mut Ui) {
    let current = ui.ctx().zoom_factor();
    let labels: Vec<&str> = ZOOM_STEPS.iter().map(|(_, label)| *label).collect();
    let mut selected = ZOOM_STEPS
        .iter()
        .position(|(zoom, _)| (zoom - current).abs() < 0.01)
        .unwrap_or(1);
    let displayed = labels[selected];

    if crate::control::selector_inline(
        ui,
        crate::theme::tokens(ui.ctx()),
        "Scale",
        &labels,
        &mut selected,
        None,
        Some(1),
        displayed,
        "How large the instrument is drawn. The layout does not change — every control stays \
         where it is, and the window resizes to match.",
    ) {
        ui.ctx().set_zoom_factor(ZOOM_STEPS[selected].0);
    }
}

/// A vertical scroll area whose content stays clear of its scroll bar — every editor's developer
/// Parameters list.
///
/// egui's scroll bar floats: it is drawn over the content, which is given the whole width, so the
/// last column's readings ran under it and read `+0.00 oc` and `Of`. The content is inset on the
/// right by the bar at its widest, which is where a floating bar draws.
pub fn scroll_list(ui: &Ui) -> egui::ScrollArea {
    let scroll = &ui.spacing().scroll;
    let bar = scroll.bar_inner_margin + scroll.bar_width + scroll.bar_outer_margin;
    egui::ScrollArea::vertical().content_margin(egui::Margin {
        right: bar.ceil() as i8,
        ..egui::Margin::ZERO
    })
}

/// A compact output level and clip indicator, §3.1 slot 6 and §7.5.
///
/// §7.5's two rules that are easy to miss and expensive to retrofit: **peak and clip states are
/// visually distinct**, and **clip indication remains visible until acknowledged** (§5.4). So clip
/// is a separate, latching element rather than the meter turning red — a meter that goes red and
/// back tells you nothing a moment later.
///
/// Returns `true` when the user acknowledges a clip by clicking it.
pub fn level_meter(ui: &mut Ui, tokens: &Tokens, peak: f32, clipped: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(96.0, 12.0), Sense::click());
    let painter = ui.painter();

    painter.rect_filled(rect, RADIUS as f32, tokens.surface_2);
    painter.rect_stroke(
        rect,
        RADIUS as f32,
        Stroke::new(HAIRLINE, tokens.border),
        StrokeKind::Inside,
    );

    // The bar. Green below the warning zone, amber approaching full scale — §5.1 gives `warning`
    // for exactly this "clip-near" state, and it is a different colour from `danger` so that
    // "loud" and "clipped" are never the same signal.
    let filled = rect.width() * peak.clamp(0.0, 1.0);
    if filled > 0.0 {
        let colour = if peak >= 0.9 {
            tokens.warning
        } else {
            tokens.success
        };
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(filled, rect.height())),
            RADIUS as f32,
            colour,
        );
    }

    // Clip: a distinct latching square, not a recolouring of the bar.
    let clip_rect = Rect::from_center_size(
        egui::Pos2::new(rect.right() - 4.0, rect.center().y),
        Vec2::splat(6.0),
    );
    painter.rect_filled(
        clip_rect,
        1.0,
        if clipped {
            tokens.danger
        } else {
            Color32::TRANSPARENT
        },
    );

    let acknowledged = clipped && response.clicked();
    let _: Response = response.on_hover_text(if clipped {
        "Output clipped. Click to acknowledge."
    } else {
        "Output level."
    });
    acknowledged
}

/// Width of one channel in the full-height vertical meter used beside a primary visualization.
pub const VERTICAL_METER_WIDTH: f32 = 16.0;

// Signal lights need a vivid palette, not the theme's status-text palette. In particular, light
// theme `warning` is deliberately brown enough to carry text contrast on white and therefore does
// not read as yellow in a meter. These colours remain in fixed low/middle/high positions, with
// threshold dividers and a peak cap, so red/green discrimination is never the only cue.
const VERTICAL_METER_GREEN: Color32 = Color32::from_rgb(0x00, 0xA4, 0x68);
const VERTICAL_METER_YELLOW: Color32 = Color32::from_rgb(0xFF, 0xF2, 0x3D);
const VERTICAL_METER_RED: Color32 = Color32::from_rgb(0xFF, 0x35, 0x55);

/// A dB-scaled vertical peak meter with fixed green, yellow and red zones plus a separate latched
/// clip cell. The caller supplies the accessible channel name and owns the clip latch; clicking a
/// clipped meter returns `true` to acknowledge only that channel.
pub fn vertical_level_meter(
    ui: &mut Ui,
    tokens: &Tokens,
    label: &str,
    peak: f32,
    clipped: bool,
    height: f32,
) -> bool {
    let height = height.max(MIN_TARGET * 2.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(VERTICAL_METER_WIDTH, height), Sense::click());
    let painter = ui.painter();
    let clip_height = 12.0;
    let clip_gap = SPACE_2;
    let clip_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), clip_height));
    let track = Rect::from_min_max(
        egui::pos2(rect.left(), clip_rect.bottom() + clip_gap),
        rect.right_bottom(),
    );

    painter.rect_filled(track, RADIUS as f32, tokens.surface_2);
    painter.rect_stroke(
        track,
        RADIUS as f32,
        Stroke::new(HAIRLINE, tokens.border),
        StrokeKind::Inside,
    );

    let zones = [
        (0.0, 0.80, VERTICAL_METER_GREEN),
        (0.80, 0.95, VERTICAL_METER_YELLOW),
        (0.95, 1.0, VERTICAL_METER_RED),
    ];
    let y = |value: f32| track.bottom() - track.height() * value;
    let level = vertical_meter_level(peak);
    for (low, high, colour) in zones {
        let top = level.min(high);
        if top > low {
            painter.rect_filled(
                Rect::from_min_max(
                    egui::pos2(track.left(), y(top)),
                    egui::pos2(track.right(), y(low)),
                ),
                0.0,
                colour,
            );
        }
    }

    // Position and shape carry the thresholds for red/green-colour-blind readers. These dividers
    // stay visible independently of level, and the cap identifies the measured peak without hue.
    let marker = Stroke::new(HAIRLINE * 2.0, tokens.text_primary.gamma_multiply(0.72));
    for threshold in [0.80, 0.95] {
        painter.hline(track.x_range(), y(threshold), marker);
    }
    if level > 0.0 {
        painter.hline(track.x_range(), y(level), marker);
    }
    painter.rect_stroke(
        track,
        RADIUS as f32,
        Stroke::new(HAIRLINE, tokens.border),
        StrokeKind::Inside,
    );

    let active_clip = peak.is_finite() && peak >= 1.0;
    painter.rect_filled(
        clip_rect,
        2.0,
        vertical_clip_colour(tokens, active_clip, clipped),
    );
    painter.rect_stroke(
        clip_rect,
        2.0,
        Stroke::new(HAIRLINE, tokens.border),
        StrokeKind::Inside,
    );

    let peak_db = if peak.is_finite() && peak > 0.0 {
        20.0 * peak.log10()
    } else {
        f32::NEG_INFINITY
    };
    let reading = if peak_db.is_finite() {
        format!("{peak_db:.1} dBFS")
    } else {
        "−∞ dBFS".to_owned()
    };
    let description = if active_clip {
        format!("{label}\n{reading}\nClipping now. Click to acknowledge the held indication.")
    } else if clipped {
        format!("{label}\n{reading}\nEarlier clip held. Click to acknowledge.")
    } else {
        format!("{label}\n{reading}\nPeak level; clip remains latched until acknowledged.")
    };
    let acknowledged = clipped && response.clicked();
    let response = response.on_hover_text(description);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    acknowledged
}

fn vertical_clip_colour(tokens: &Tokens, active: bool, latched: bool) -> Color32 {
    if active {
        tokens.danger
    } else if latched {
        // A held event remains legible but must not look identical to a new clip. Otherwise a
        // second clip produces no visible change until the user first clears the old latch.
        tokens.danger.gamma_multiply(0.38)
    } else {
        tokens.surface_2
    }
}

fn vertical_meter_level(peak: f32) -> f32 {
    if !peak.is_finite() || peak <= 0.0 {
        return 0.0;
    }
    ((20.0 * peak.log10()).clamp(-60.0, 0.0) + 60.0) / 60.0
}

#[cfg(test)]
mod tests {
    use super::{
        AppBar, ModuleCard, PresetState, VERTICAL_METER_GREEN, VERTICAL_METER_RED,
        VERTICAL_METER_YELLOW, ViewBar, group, preset_browser, vertical_clip_colour,
        vertical_meter_level,
    };
    use crate::space::{HAIRLINE, MIN_TARGET, SPACE_2};
    use crate::theme::LIGHT;
    use kittest::{NodeT, Queryable};

    #[test]
    fn a_vertical_meter_uses_a_decibel_scale_from_minus_sixty_to_zero() {
        assert_eq!(vertical_meter_level(0.0), 0.0);
        assert_eq!(vertical_meter_level(0.001), 0.0);
        assert!((vertical_meter_level(10.0_f32.powf(-30.0 / 20.0)) - 0.5).abs() < 1.0e-6);
        assert_eq!(vertical_meter_level(1.0), 1.0);
        assert_eq!(vertical_meter_level(2.0), 1.0);
        assert_eq!(vertical_meter_level(f32::NAN), 0.0);
    }

    #[test]
    fn a_new_clip_is_brighter_than_the_held_latch() {
        assert_eq!(vertical_clip_colour(&LIGHT, true, true), LIGHT.danger);
        assert_ne!(vertical_clip_colour(&LIGHT, false, true), LIGHT.danger);
        assert_eq!(vertical_clip_colour(&LIGHT, false, false), LIGHT.surface_2);
    }

    #[test]
    fn vertical_meter_signal_colours_are_vivid_and_yellow_is_really_yellow() {
        for colour in [
            VERTICAL_METER_GREEN,
            VERTICAL_METER_YELLOW,
            VERTICAL_METER_RED,
        ] {
            let channels = [colour.r(), colour.g(), colour.b()];
            let high = f32::from(*channels.iter().max().unwrap());
            let low = f32::from(*channels.iter().min().unwrap());
            let saturation = if high > 0.0 { (high - low) / high } else { 0.0 };
            assert!(
                saturation >= 0.75,
                "signal colour is too desaturated: {colour:?}"
            );
        }

        assert!(VERTICAL_METER_YELLOW.r() >= 240);
        assert!(VERTICAL_METER_YELLOW.g() >= 220);
        assert!(VERTICAL_METER_YELLOW.b() <= 80);
        assert_ne!(VERTICAL_METER_YELLOW, LIGHT.warning);
    }

    /// A group is a hairline panel with no fill, and it tightens the rhythm inside it.
    ///
    /// Both halves matter. The panel is what says "these belong together"; the rhythm is what
    /// stops the gaps inside a group from matching the gaps between groups, which is the defect
    /// that produced this control — a routing column whose every gap was egui's off-grid 3.0.
    /// The absent fill is the second report: a filled group outweighed the controls in it.
    #[test]
    fn a_group_is_an_unfilled_hairline_panel_that_tightens_its_body() {
        fn gap(inside: bool) -> (f32, Vec<(egui::Color32, egui::Stroke)>) {
            let rects = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let out = std::rc::Rc::clone(&rects);
            let mut harness = egui_kittest::Harness::new_ui(move |ui| {
                let body = |ui: &mut egui::Ui| {
                    out.borrow_mut().push(ui.label("first").rect);
                    out.borrow_mut().push(ui.label("second").rect);
                };
                if inside {
                    group(ui, &LIGHT, |ui| body(ui));
                } else {
                    body(ui);
                }
            });
            harness.run();

            let mut panels = Vec::new();
            fn walk(shape: &egui::Shape, panels: &mut Vec<(egui::Color32, egui::Stroke)>) {
                match shape {
                    egui::Shape::Rect(rect) => panels.push((rect.fill, rect.stroke)),
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            walk(shape, panels);
                        }
                    }
                    _ => {}
                }
            }
            for clipped in &harness.output().shapes {
                walk(&clipped.shape, &mut panels);
            }

            let rects = rects.borrow();
            (rects[1].top() - rects[0].bottom(), panels)
        }

        let (outside, _) = gap(false);
        let (inside, panels) = gap(true);

        assert!(
            (inside - SPACE_2).abs() < 0.01,
            "a group's own rhythm is {inside}, not the {SPACE_2} grid"
        );
        assert!(
            inside < outside + 8.0,
            "a group must be tighter than the 8 points that separate it from the next"
        );
        assert!(
            panels.iter().any(|(fill, stroke)| {
                fill.a() == 0
                    && stroke.color == LIGHT.border
                    && (stroke.width - HAIRLINE).abs() < 0.01
            }),
            "no unfilled hairline panel was painted: {panels:?}"
        );
    }

    /// **A scroll list's content stays clear of its scroll bar.** egui's bar floats over the
    /// content, so a reading pinned to a plain vertical scroll area's right edge ran under it; the
    /// list is inset by the bar at its widest, and the reading ends before it.
    #[test]
    fn a_scroll_lists_content_stays_clear_of_its_bar() {
        use std::cell::Cell;
        use std::rc::Rc;

        let reading = Rc::new(Cell::new(0.0f32));
        let edge = Rc::new(Cell::new(0.0f32));
        let (read, bounds) = (Rc::clone(&reading), Rc::clone(&edge));
        let mut harness = egui_kittest::Harness::builder()
            .with_size(Vec2::new(300.0, 200.0))
            .build_ui(move |ui| {
                bounds.set(ui.max_rect().right());
                scroll_list(ui).show(ui, |ui| {
                    // Tall enough to scroll, so the bar is there to be clear of.
                    for _ in 0..40 {
                        let label = ui
                            .with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label("+0.00 oct")
                            })
                            .inner;
                        read.set(label.rect.right());
                    }
                });
            });
        harness.run_steps(3);
        let scroll = harness.ctx.global_style().spacing.scroll;
        let bar = scroll.bar_inner_margin + scroll.bar_width + scroll.bar_outer_margin;
        assert!(
            reading.get() <= edge.get() - bar + 0.5,
            "the reading ends at {:.1}, under a bar that starts at {:.1}",
            reading.get(),
            edge.get() - bar
        );
    }

    /// The height the app bar actually occupies, with whatever is put in it.
    fn bar_height(with_presets: bool) -> f32 {
        let measured = std::rc::Rc::new(std::cell::Cell::new(0.0f32));
        let out = std::rc::Rc::clone(&measured);

        let mut harness = egui_kittest::Harness::new_ui(move |ui| {
            let state = PresetState {
                name: Some("Init"),
                modified: true,
                favourite: false,
                read_only: None,
                save_disabled: None,
            };
            let top = ui.max_rect().top();
            if with_presets {
                AppBar::new("mxm-mono-01").show_with(
                    ui,
                    &LIGHT,
                    |ui| {
                        preset_browser(ui, &LIGHT, &state);
                    },
                    |ui| {
                        let _ = ui.button("Menu");
                    },
                );
            } else {
                AppBar::new("mxm-mono-01").show(ui, &LIGHT, |ui| {
                    let _ = ui.button("Menu");
                });
            }
            // What the panel took out of the space it was given.
            out.set(ui.available_rect_before_wrap().top() - top);
        });
        harness.run();
        measured.get()
    }

    #[test]
    fn the_preset_controls_do_not_make_the_app_bar_taller() {
        // **Reported.** §3.1's bar is a fixed 44 px on a 4 px grid, chosen to leave room for a 32 px
        // pointer target with padding above and below. Slots 2 to 4 have to live inside that, not
        // push it open — a bar that grows when an instrument has presets is a collection that stops
        // looking like one.
        let plain = bar_height(false);
        let with_presets = bar_height(true);
        eprintln!("plain {plain}, with presets {with_presets}");
        assert!(
            (plain - with_presets).abs() < 0.5,
            "the bar is {with_presets} tall with presets and {plain} without"
        );
    }

    /// **The narrowest bar a shipped editor asks for still names every view and still takes a
    /// pointer.** `mxm-para-07`'s 340-point physical minimum exposes 170 logical points at 200 %,
    /// four views wide — the case this was written for on the SH-7 branch, where the bar answered
    /// by shrinking cells to two-letter initials.
    ///
    /// **The bar wraps instead now**, and that is a different answer to the same requirement, so
    /// the requirement is what this holds and the layout is not: the harness is given room for
    /// however many rows the bar asks for, and `overflow` — the bar's own word for *a view I could
    /// not fit without squeezing it below its floor* — is what must stay false.
    #[test]
    fn four_view_navigation_at_the_narrowest_editor_keeps_every_view_reachable_and_named() {
        use std::cell::Cell;

        const WIDTH: f32 = 170.0;
        let views = ["Synth", "Mod", "Voice", "Parameters"];

        let rows = Cell::new(0usize);
        let overflow = Cell::new(true);
        let height = Cell::new(0.0f32);
        let mut harness = egui_kittest::Harness::builder()
            // Tall enough for the bar to wrap as far as it likes; the assertions are on the bar's
            // own geometry, not on this number.
            .with_size(egui::vec2(WIDTH, 240.0))
            .build_ui_state(
                |ui, selected| {
                    let g = ViewBar::new(&views).geometry(ui, WIDTH);
                    rows.set(g.rows);
                    overflow.set(g.overflow);
                    height.set(g.height);
                    ViewBar::new(&views).show(ui, &LIGHT, selected);
                },
                0usize,
            );

        assert!(
            !overflow.get(),
            "at {WIDTH} points the bar reports a view it could not fit: {} rows, {} tall",
            rows.get(),
            height.get()
        );

        let screen = harness.ctx.content_rect();
        for (index, name) in views.into_iter().enumerate() {
            let node = harness.get_by_label(name);
            let rect = node.rect();
            assert!(
                rect.width() >= MIN_TARGET
                    && screen.contains(rect.min)
                    && screen.contains(rect.max),
                "{name} is not a full reachable target in {screen:?}: {rect:?}"
            );
            assert_eq!(
                format!("{:?}", node.accesskit_node().toggled()),
                if index == 0 {
                    "Some(True)"
                } else {
                    "Some(False)"
                },
                "{name} must expose whether it is the current radio-style view"
            );
        }

        harness.get_by_label("Parameters").click();
        harness.run_steps(2);
        assert_eq!(*harness.state(), 3);
        assert_eq!(
            format!(
                "{:?}",
                harness.get_by_label("Synth").accesskit_node().toggled()
            ),
            "Some(False)",
            "the old view must clear its selected state"
        );
        assert_eq!(
            format!(
                "{:?}",
                harness
                    .get_by_label("Parameters")
                    .accesskit_node()
                    .toggled()
            ),
            "Some(True)",
            "the current view must publish selected state"
        );
    }

    use super::{PresetRow, step_preset};

    fn rows<'a>(spec: &'a [(&'a str, bool)]) -> Vec<PresetRow<'a>> {
        spec.iter()
            .map(|(name, ok)| PresetRow {
                name,
                origin: "Factory",
                favourite: false,
                problem: (!ok).then_some("unreadable"),
            })
            .collect()
    }

    #[test]
    fn the_arrows_skip_a_preset_that_cannot_be_loaded() {
        // A broken file is *shown* — omitting it would tell somebody their preset had vanished —
        // but an arrow that stops on it is an arrow that appears to do nothing.
        let spec = [("A", true), ("broken", false), ("C", true)];
        let rows = rows(&spec);

        assert_eq!(step_preset(&rows, Some(0), 1), Some(2), "forwards over it");
        assert_eq!(step_preset(&rows, Some(2), -1), Some(0), "and backwards");
    }

    #[test]
    fn the_arrows_wrap_at_both_ends() {
        let spec = [("A", true), ("B", true)];
        let rows = rows(&spec);

        assert_eq!(step_preset(&rows, Some(1), 1), Some(0));
        assert_eq!(step_preset(&rows, Some(0), -1), Some(1));
    }

    #[test]
    fn stepping_with_nothing_loaded_starts_at_the_first_selectable_row() {
        // The state after Init, and on a fresh instance. An arrow has to do *something* there.
        let spec = [("broken", false), ("B", true)];
        let rows = rows(&spec);
        assert_eq!(step_preset(&rows, None, 1), Some(1));
    }

    #[test]
    fn stepping_a_list_with_nothing_selectable_chooses_nothing() {
        // Every file present and broken. Returning index 0 would load the unloadable.
        let spec = [("broken", false)];
        assert_eq!(step_preset(&rows(&spec), None, 1), None);
        assert!(step_preset(&[], None, 1).is_none());
    }

    use super::*;

    #[test]
    fn the_app_bar_leaves_room_for_a_pointer_target() {
        // §11's 32x32 minimum has to fit inside the bar with its padding, or every control in
        // slots 2 to 6 is undersized. This is the arithmetic that is obvious once written down
        // and invisible when it is not.
        let bar = AppBar::new("mxm-mono-01");
        assert!(
            bar.height >= MIN_TARGET + SPACE_2 * 2.0,
            "an app bar of {} cannot hold a 32 px target with padding",
            bar.height
        );
    }

    #[test]
    fn the_view_bar_is_tall_enough_too() {
        // The bar has to hold a 32 px target and still have space above and below it, or the
        // switch — the one control guaranteed to be used — is the hardest thing to hit.
        const BAR: f32 = MIN_TARGET + SPACE_3;
        const {
            assert!(
                BAR - MIN_TARGET >= SPACE_2 * 2.0,
                "the view bar has no breathing room"
            );
        }
    }

    /// Two columns of cards, one several rows taller than the other, the shorter one's card told
    /// the difference — laid out for `steps` frames, and what the leveller remembered.
    fn level_scene(steps: usize) -> super::Levelled {
        let id = egui::Id::new("mxm-ui-level-test");
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui(move |ui| {
                super::level_columns(ui, id, 2, |column, index, deficit| {
                    let (title, rows) = if index == 0 {
                        ("Tall", 6)
                    } else {
                        ("Short", 1)
                    };
                    ModuleCard::new(title).show(column, &LIGHT, |ui| {
                        for _ in 0..rows {
                            ui.label("a row of something");
                        }
                        super::grow(ui, deficit);
                    });
                });
            });
        harness.run_steps(steps);
        super::levelled(&harness.ctx, id).expect("the layout remembered what it measured")
    }

    /// §3.3: cards side by side end on one line. Measured from where the columns actually ended,
    /// after the frame that applies what the frame before measured.
    #[test]
    fn columns_of_cards_end_on_one_line() {
        let level = level_scene(3);
        assert!(
            level.natural[0] - level.natural[1] > 20.0,
            "the premise: the columns differ by enough to see ({:?})",
            level.natural
        );
        assert!(
            (level.bottoms[0] - level.bottoms[1]).abs() < 0.5,
            "the cards end at {:?}",
            level.bottoms
        );
    }

    /// The padding is subtracted back out of what is remembered, or the shorter column would be
    /// measured as tall as the padding made it, the deficit would vanish, and the two would
    /// alternate forever.
    #[test]
    fn the_levelling_does_not_feed_on_itself() {
        let settled = level_scene(3);
        let later = level_scene(12);
        for (a, b) in settled.natural.iter().zip(&later.natural) {
            assert!(
                (a - b).abs() < 0.5,
                "the natural heights crept: {settled:?} then {later:?}"
            );
        }
        for (a, b) in settled.bottoms.iter().zip(&later.bottoms) {
            assert!(
                (a - b).abs() < 0.5,
                "the bottoms crept: {settled:?} then {later:?}"
            );
        }
    }

    #[test]
    fn a_card_defaults_to_not_bypassable() {
        // mxm-mono-01 has no optional sections, and a disabled control implying otherwise is exactly
        // what `apps/mxm-player/AGENTS.md` warns against for the player's scope decisions.
        let card = ModuleCard::new("Filter");
        assert!(card.enabled.is_none());
    }
}

#[cfg(test)]
mod compact_bar_tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use kittest::Queryable;

    use super::*;

    const PRODUCT: &str = "mxm-test-instrument";

    /// The bar every editor draws: the preset group, and a right-hand group of a fixed-width output
    /// stand-in, the zoom and the theme, in the editors' order. `right_left` is where the right-hand
    /// group begins; `saved` is set when Save preset is chosen.
    fn harness(
        width: f32,
        right_left: Rc<Cell<f32>>,
        saved: Rc<Cell<bool>>,
    ) -> egui_kittest::Harness<'static> {
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(width, 400.0))
            .build_ui(move |ui| {
                crate::theme::apply(ui.ctx());
                crate::typography::apply(ui.ctx());
                let tokens = crate::theme::LIGHT;
                AppBar::new(PRODUCT).show_with(
                    ui,
                    &tokens,
                    |ui| {
                        let state = PresetState {
                            name: Some("Bass"),
                            modified: false,
                            favourite: false,
                            read_only: None,
                            save_disabled: None,
                        };
                        if preset_browser(ui, &tokens, &state).save_as {
                            saved.set(true);
                        }
                    },
                    |ui| {
                        ui.add_sized(Vec2::new(200.0, MIN_TARGET), egui::Label::new("output"));
                        zoom_control(ui);
                        theme_control(ui);
                        right_left.set(ui.min_rect().left());
                    },
                );
            })
    }

    /// **The bar gives things up one step at a time as its window narrows, and never draws over
    /// itself while a step still fits** (design system §3.1). Each piece, once gone, stays gone at
    /// every narrower width; while *Save preset* is still on the bar the `…` menu ends before the
    /// right-hand group begins.
    #[test]
    fn the_app_bar_gives_things_up_one_step_at_a_time_and_never_overlaps() {
        let right_left = Rc::new(Cell::new(0.0));
        let saved = Rc::new(Cell::new(false));
        let mut harness = harness(1200.0, Rc::clone(&right_left), Rc::clone(&saved));
        let mut previous = [true; 4];
        let mut seen_steps = Vec::new();
        for width in (400..=1200).rev().step_by(20) {
            harness.set_size(egui::vec2(width as f32, 400.0));
            harness.run_steps(4);
            let theme = harness.query_by_label_contains("Theme").is_some();
            let wide_name = harness
                .query_by_label("Bass")
                .is_some_and(|name| name.rect().width() > PRESET_NAME_WIDTH_COMPACT + 1.0);
            let product = harness.query_by_label(PRODUCT).is_some();
            let save = harness.query_by_label("Save preset").is_some();
            let now = [theme, wide_name, product, save];
            for (piece, (was, is)) in ["theme", "wide name", "product", "Save"]
                .iter()
                .zip(previous.iter().zip(now))
            {
                assert!(
                    *was || !is,
                    "at {width} the {piece} came back after the bar had given it up"
                );
            }
            // In order: each step keeps what the ones after it give up.
            assert!(
                now.windows(2).all(|pair| pair[0] <= pair[1]),
                "at {width} the bar skipped a step: {now:?}"
            );
            // At every width, below the last step too: the preset group is cut off at its left, so
            // the menu that holds what the bar gave up stays whole. The group is clipped at the
            // gap before the right-hand group — `SPACE_5` and the item spacing egui adds before
            // it — so ending anywhere past that edge is a menu cut short, not merely one drawn
            // over its neighbour.
            let menu = harness.get_by_label("…").rect();
            let edge =
                right_left.get() - SPACE_5 - harness.ctx.global_style().spacing.item_spacing.x;
            assert!(
                menu.right() <= edge + 0.5,
                "at {width} the `…` menu ends at {:.1}, past its group's edge at {edge:.1}",
                menu.right(),
            );
            seen_steps.push(now.iter().filter(|kept| !**kept).count());
            previous = now;
        }
        seen_steps.dedup();
        assert_eq!(seen_steps, [0, 1, 2, 3, 4], "every step is taken, in order");
    }

    /// **A widget keeps its id at every step**, so a focus survives the bar compacting: the
    /// right-hand group's control is focused on a wide bar, the window narrows through every step,
    /// and the accessibility tree — which refuses a focus on a node it does not have — still finds
    /// it, focused.
    #[test]
    fn a_focus_survives_the_bar_compacting() {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 400.0))
            .build_ui(|ui| {
                crate::theme::apply(ui.ctx());
                crate::typography::apply(ui.ctx());
                let tokens = crate::theme::LIGHT;
                AppBar::new(PRODUCT).show_with(
                    ui,
                    &tokens,
                    |ui| {
                        let state = PresetState {
                            name: Some("Bass"),
                            modified: false,
                            favourite: false,
                            read_only: None,
                            save_disabled: None,
                        };
                        preset_browser(ui, &tokens, &state);
                    },
                    |ui| {
                        ui.add_sized(Vec2::new(120.0, MIN_TARGET), egui::Button::new("output"));
                        zoom_control(ui);
                        theme_control(ui);
                    },
                );
            });
        harness.run_steps(4);
        harness.get_by_label("output").focus();
        harness.run_steps(2);
        let focused = harness.ctx.memory(|m| m.focused());
        assert!(focused.is_some(), "the output did not take the focus");
        for width in (400..=1200).rev().step_by(40) {
            harness.set_size(egui::vec2(width as f32, 400.0));
            harness.run_steps(3);
            assert_eq!(
                harness.ctx.memory(|m| m.focused()),
                focused,
                "at {width} the focused control changed its id"
            );
        }
    }

    /// **A bar with no `…` menu never compacts**: `AppBar::show` has no presets, so at any width it
    /// keeps its theme and zoom rather than hiding them with nowhere to put them.
    #[test]
    fn a_bar_without_presets_keeps_its_theme_and_zoom_at_any_width() {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(400.0, 300.0))
            .build_ui(|ui| {
                crate::theme::apply(ui.ctx());
                crate::typography::apply(ui.ctx());
                AppBar::new(PRODUCT).show(ui, &crate::theme::LIGHT, |ui| {
                    zoom_control(ui);
                    theme_control(ui);
                });
            });
        harness.run_steps(4);
        assert!(
            harness.query_by_label_contains("Theme").is_some(),
            "the theme was hidden"
        );
        assert!(
            harness.query_by_label_contains("Scale").is_some(),
            "the zoom was hidden"
        );
        assert!(
            harness.query_by_label(PRODUCT).is_some(),
            "the product name was hidden"
        );
    }

    /// At the last step the `…` menu holds what the bar gave up — *Save preset*, the favourite, the
    /// theme and the scale — and Save still does what it did on the bar: where the last step fits,
    /// and below it, where the preset group is cut off at its left and the menu stays whole.
    #[test]
    fn at_the_last_step_the_menu_holds_what_the_bar_gave_up() {
        for width in [600.0, 400.0] {
            menu_holds_what_the_bar_gave_up(width);
        }
    }

    fn menu_holds_what_the_bar_gave_up(width: f32) {
        let right_left = Rc::new(Cell::new(0.0));
        let saved = Rc::new(Cell::new(false));
        let mut harness = harness(width, right_left, Rc::clone(&saved));
        harness.run_steps(4);
        assert!(harness.query_by_label("Save preset").is_none());
        harness.get_by_label("…").click();
        harness.run_steps(2);
        for label in ["Save preset…", "☆ Favourite", "Theme", "Scale"] {
            assert!(
                harness.query_by_label(label).is_some(),
                "at {width} the menu has no {label}"
            );
        }
        harness.get_by_label("Save preset…").click();
        harness.run_steps(2);
        assert!(
            saved.get(),
            "at {width} Save preset in the menu did not save"
        );
    }

    /// **A product's own bar actions are the last thing the bar gives up**, and the menu then
    /// lists them and reports the one chosen: on a wide bar the product draws them itself and the
    /// menu has none; on a narrow one the product's drawing is skipped, the menu has the rows, a
    /// disabled row cannot be chosen, and choosing one is reported once.
    #[test]
    fn the_products_own_actions_move_into_the_menu_last() {
        use kittest::NodeT;

        let drew = Rc::new(Cell::new(false));
        let chosen = Rc::new(Cell::new(None));
        let bar = |width: f32| {
            let drew = Rc::clone(&drew);
            let chosen = Rc::clone(&chosen);
            egui_kittest::Harness::builder()
                .with_size(egui::vec2(width, 400.0))
                .build_ui(move |ui| {
                    crate::theme::apply(ui.ctx());
                    crate::typography::apply(ui.ctx());
                    let tokens = crate::theme::LIGHT;
                    let items = vec![
                        MenuItem {
                            label: "Freeze".to_owned(),
                            hover: "Freeze the kit".to_owned(),
                            kind: MenuKind::Toggle(false),
                            disabled: None,
                        },
                        MenuItem {
                            label: "Export".to_owned(),
                            hover: "Write it out".to_owned(),
                            kind: MenuKind::Action,
                            disabled: Some("Freeze first".to_owned()),
                        },
                    ];
                    AppBar::new(PRODUCT).show_with(
                        ui,
                        &tokens,
                        |ui| {
                            let state = PresetState {
                                name: Some("Bass"),
                                modified: false,
                                favourite: false,
                                read_only: None,
                                save_disabled: None,
                            };
                            preset_browser(ui, &tokens, &state);
                        },
                        |ui| {
                            ui.add_sized(Vec2::new(200.0, MIN_TARGET), egui::Label::new("output"));
                            let shown = product_actions(ui, items, |ui| {
                                ui.add_sized(
                                    Vec2::new(160.0, MIN_TARGET),
                                    egui::Label::new("pair"),
                                );
                            });
                            drew.set(shown.is_some());
                        },
                    );
                    if let Some(index) = take_product_action(ui.ctx()) {
                        chosen.set(Some(index));
                    }
                })
        };

        let mut wide = bar(1400.0);
        wide.run_steps(4);
        assert!(drew.get(), "a wide bar draws the product's own actions");
        wide.get_by_label("…").click();
        wide.run_steps(2);
        assert!(
            wide.query_by_label("Freeze").is_none(),
            "and the menu has none"
        );

        let mut narrow = bar(560.0);
        narrow.run_steps(4);
        assert!(!drew.get(), "a narrow bar gives them up");
        narrow.get_by_label("…").click();
        narrow.run_steps(2);
        assert!(
            narrow.get_by_label("Export").accesskit_node().is_disabled(),
            "a disabled row stays disabled"
        );
        narrow.get_by_label("Freeze").click();
        narrow.run_steps(1);
        assert_eq!(chosen.get(), Some(0), "the menu reports the row chosen");
        chosen.set(None);
        narrow.run_steps(2);
        assert_eq!(chosen.get(), None, "once");
    }
}
