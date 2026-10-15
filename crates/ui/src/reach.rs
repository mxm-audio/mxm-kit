//! Everything on screen within the keyboard's reach (the owner, 2026-10-07: "EVERYTHING on screen
//! should be reachable and editable by the keyboard"), in newDAWn and the collection's editors.
//!
//! Where a view has no cursor of its own, and in any menu or list that opens, the cursor walks the
//! widgets egui describes to screen readers each frame: the arrows go to the nearest one that way,
//! OPEN presses it as a click does or types in a text field (until Escape or Enter), and VALUE +
//! arrows change a value. A widget is in reach as soon as it is drawn, with nothing to register. A
//! widget drawn by hand says what it is with `Response::widget_info`, or it is left out, as a
//! canvas with a cursor of its own is; one that changes a value in units of its own reads the keys
//! with [`edit`], and DELETE puts one with a default back to it. **A value is typed too**
//! ([`typed`]): OPEN on one, or its right-click menu's *Type a value* ([`type_item`]), opens a text
//! field over it in its own units, Enter keeps it and Escape cancels (newDAWn's keyboard-reach
//! plan, S12). The keys come from `mxm-keys`; the views are the host's, named by any key `K`.
//!
//! **What the pointer sees, the keys see** ([`Tip`]): a widget's tooltip shows beside it while the
//! keyboard's cursor rests there too ([`State::tip`], [`tip_at`]), and a label or a disabled widget
//! with one is a stop for the cursor, [`Kind::Info`], so a warning's list or why a button is off is
//! read without a mouse (newDAWn's owner, 2026-10-15: "everything in newDAWn must be navigatable
//! and editable with the keyboard").

use std::collections::HashMap;
use std::hash::Hash;

use egui::accesskit::{self, ActionRequest, NodeId, Role, TreeId};
use egui::{Context, Event, FullOutput, Id, Order, Pos2, Rect, Ui, WidgetText};
use mxm_keys::{Action as KeyAction, Direction, Output, Step, Verb};

/// What a widget does with the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// OPEN presses it, as a click does.
    Press,
    /// VALUE + arrows change it.
    Value,
    /// OPEN types in it, until Escape or Enter.
    Text,
    /// A label or a disabled widget with a tip: the cursor stops there to show it, and OPEN does
    /// nothing.
    Info,
}

/// A widget on screen, as egui described it on the last frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    pub id: NodeId,
    pub rect: Rect,
    pub kind: Kind,
    /// What it says, for the help.
    pub label: String,
}

/// Where the cursor walks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Region {
    /// The widgets drawn inside the `Ui` with this `Ui::unique_id`: a view, or a bar.
    Inside(Id),
    /// The menu or list open on top, where it is on screen.
    Menu(Rect),
}

/// The menu or list open on top, if one is: a combo box's, a menu's, a right-click menu. Once
/// egui has shown it: in the pass that sizes it, it's drawn unseen where it doesn't stay, over what
/// it was opened from.
fn menu(ctx: &Context) -> Option<Rect> {
    let layer = menu_layer(ctx)?;
    ctx.memory(|memory| {
        memory
            .areas()
            .is_visible(&layer)
            .then(|| memory.area_rect(layer.id))
            .flatten()
    })
}

fn menu_layer(ctx: &Context) -> Option<egui::LayerId> {
    if !egui::Popup::is_any_open(ctx) {
        return None;
    }
    ctx.memory(|memory| memory.areas().top_layer_id(Order::Foreground))
}

/// A hand-drawn value widget's keyboard edit, this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edited {
    /// VALUE + an arrow: the value now, to preview.
    Moving(f64),
    /// The gesture is over: keep this value, as one undo step.
    Done(f64),
    /// The gesture is cancelled: back to the value it started from.
    Cancelled(f64),
}

/// One keyboard press on a hand-drawn value: its step size, +1 or −1, and whether it snaps.
///
/// ↑ ↓ move the value **by** the size; ← → `snap` it **to** the next line of the size that way,
/// the next round value (newDAWn's owner, 2026-10-09: the musical change on the free arrows).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    pub size: Step,
    pub sign: f64,
    pub snap: bool,
}

/// A hand-drawn value widget's keyboard edits, in its own units. Call it every frame with the
/// widget's id, its value and its default, if it has one: `step` takes a value and a [`Change`],
/// and gives the next value. DELETE (or RIPPLE) puts it back to its default, one undo step, as in
/// the editors (design system §11). Returns the value to show, the edit's while one is going on,
/// and what changed now.
pub fn edit(
    ui: &Ui,
    id: Id,
    current: f64,
    default: Option<f64>,
    step: impl Fn(f64, Change) -> f64,
) -> (f64, Option<Edited>) {
    let ctx = ui.ctx();
    let node = id.accesskit_id();
    ctx.data_mut(|data| {
        let takes = data.get_temp_mut_or_default::<Takes>(Id::new(TAKES));
        takes.values.push(node);
        if default.is_some() {
            takes.defaults.push(node);
        }
    });
    let events = ctx
        .data(|data| data.get_temp::<Pending>(Id::new(PENDING)))
        .filter(|pending| pending.target == Some(node))
        .map(|pending| pending.events)
        .unwrap_or_default();
    let key = Id::new(EDITING).with(id);
    let mut value: Option<f64> = ctx.data(|data| data.get_temp(key));
    let mut edited = None;
    for event in events {
        match event {
            EditEvent::Change(change) => {
                let next = step(value.unwrap_or(current), change);
                value = Some(next);
                edited = Some(Edited::Moving(next));
            }
            EditEvent::Keep => {
                if let Some(kept) = value.take() {
                    edited = Some(Edited::Done(kept));
                }
            }
            EditEvent::Cancel => {
                if value.take().is_some() {
                    edited = Some(Edited::Cancelled(current));
                }
            }
            // The default, kept at once; nothing when it's there already and no edit is going on.
            EditEvent::Reset => {
                if let Some(default) = default
                    && (value.take().is_some() || current != default)
                {
                    edited = Some(Edited::Done(default));
                }
            }
            // [`typed`]'s.
            EditEvent::Type => {}
        }
    }
    ctx.data_mut(|data| {
        if let Some(value) = value {
            data.insert_temp(key, value);
        } else {
            data.remove::<f64>(key);
        }
    });
    let shown = match edited {
        Some(Edited::Done(kept)) => kept,
        _ => value.unwrap_or(current),
    };
    (shown, edited)
}

/// A widget's right-click menu, which MENU opens too, below it: for a widget the cursor is on, or
/// where `keys` says when a view with a cursor of its own asks.
///
/// A request lasts the pass it was made in and the next, since a host may read the keys before or
/// after drawing; one no menu took by then is dropped, so a widget that draws a menu only later
/// never opens one unasked.
pub fn context_menu(response: &egui::Response, keys: Option<Rect>, add: impl FnOnce(&mut Ui)) {
    let ctx = &response.ctx;
    let id = response.id.with("menu from the keys");
    let node = response.id.accesskit_id();
    let now = ctx.cumulative_pass_nr();
    let asked = ctx.data_mut(|data| {
        let key = Id::new(MENU_FOR);
        let Some((asked_for, pass)) = data.get_temp::<(NodeId, u64)>(key) else {
            return false;
        };
        let current = pass + 1 >= now;
        if !current || asked_for == node {
            data.remove::<(NodeId, u64)>(key);
        }
        current && asked_for == node
    });
    let anchor = keys.or(asked.then_some(response.rect));
    if anchor.is_some() {
        egui::Popup::open_id(ctx, id);
    }
    if egui::Popup::is_id_open(ctx, id) {
        ctx.data_mut(|data| data.insert_temp(Id::new(CONTEXT_MENU), id));
        let rect = anchor
            .or_else(|| ctx.data(|data| data.get_temp::<Rect>(id)))
            .unwrap_or(response.rect);
        ctx.data_mut(|data| data.insert_temp(id, rect));
        egui::Popup::new(id, ctx.clone(), rect, response.layer_id)
            .kind(egui::PopupKind::Menu)
            .open_memory(None)
            .show(add);
    } else {
        response.context_menu(add);
    }
}

const MENU_FOR: &str = "newdawn reach: the widget whose menu the keys asked for";
const TAKES: &str = "newdawn reach: widgets that take edits";
const PENDING: &str = "newdawn reach: this frame's edits";
const EDITING: &str = "newdawn reach: the value being edited";
const TIP: &str = "newdawn reach: the tip where the keyboard's cursor is";
const TYPING: &str = "newdawn reach: the value being typed";
const CONTEXT_MENU: &str = "newdawn reach: the right-click menu open";

/// Whether a [`context_menu`] is open: the editors walk it with the keys (`navigation`), as
/// newDAWn walks every menu.
pub(crate) fn context_menu_open(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp::<Id>(Id::new(CONTEXT_MENU)))
        .is_some_and(|id| egui::Popup::is_id_open(ctx, id))
}

/// The hand-drawn value widgets that read their edits, as drawn on a frame, and those of them
/// that have a default.
#[derive(Clone, Debug, Default)]
struct Takes {
    values: Vec<NodeId>,
    defaults: Vec<NodeId>,
    /// Those that can be typed ([`typed`]): OPEN opens their field rather than clicking them.
    typed: Vec<NodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditEvent {
    Change(Change),
    Keep,
    Cancel,
    Reset,
    /// OPEN on a value that can be typed.
    Type,
}

/// A value's text field while it's open: what's typed (the value as it was, until the field has
/// drawn once) and whether it still has to take the keyboard.
#[derive(Clone, Debug, Default)]
struct Typing {
    text: Option<String>,
    focus: bool,
}

/// Typing a value (newDAWn's keyboard-reach plan, S12): OPEN on a value widget the cursor is on,
/// or its right-click menu's *Type a value* ([`type_item`]), opens a text field over it, holding
/// the value as `text` writes it in the widget's units. Enter returns what was typed, for the
/// widget to read and keep as one undo step; Escape, or a click elsewhere, closes it with nothing.
/// What the widget can't read changes nothing, as in the editors. Call it every frame after the
/// widget is drawn, with its response.
pub fn typed(ui: &Ui, response: &egui::Response, text: impl FnOnce() -> String) -> Option<String> {
    let ctx = ui.ctx();
    let node = response.id.accesskit_id();
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Takes>(Id::new(TAKES))
            .typed
            .push(node);
    });
    let key = Id::new(TYPING).with(response.id);
    let asked = ctx
        .data(|data| data.get_temp::<Pending>(Id::new(PENDING)))
        .filter(|pending| pending.target == Some(node))
        .is_some_and(|pending| pending.events.contains(&EditEvent::Type));
    let mut typing = if asked {
        Typing {
            text: None,
            focus: true,
        }
    } else {
        ctx.data(|data| data.get_temp::<Typing>(key))?
    };
    let shown = typing.text.get_or_insert_with(text);
    let output = egui::Area::new(key.with("field"))
        .order(Order::Foreground)
        .fixed_pos(response.rect.left_top())
        .show(ctx, |ui| {
            egui::TextEdit::singleline(shown)
                .id(key.with("text"))
                .desired_width(response.rect.width().max(64.0))
                .show(ui)
        })
        .inner;
    let field = if std::mem::take(&mut typing.focus) {
        take_keyboard(ctx, output)
    } else {
        output.response.response
    };
    if field.lost_focus() {
        ctx.data_mut(|data| data.remove::<Typing>(key));
        return ui
            .input(|input| input.key_pressed(egui::Key::Enter))
            .then(|| typing.text.unwrap_or_default());
    }
    ctx.data_mut(|data| data.insert_temp(key, typing));
    None
}

/// A text field just opened over a value takes the keyboard, its text all selected, so what's
/// typed replaces the value. The editors' direct entry does the same.
pub(crate) fn take_keyboard(
    ctx: &Context,
    mut output: egui::text_edit::TextEditOutput,
) -> egui::Response {
    let chars = output.galley.text().chars().count();
    output
        .state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(chars),
        )));
    let response = output.response.response;
    output.state.store(ctx, response.id);
    response.request_focus();
    response
}

/// *Type a value*, for a value's right-click menu ([`context_menu`]): opens [`typed`]'s field over
/// the widget `id`, as OPEN does from the keys.
pub fn type_item(ui: &mut Ui, id: Id) {
    if ui.button("Type a value").clicked() {
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                Id::new(TYPING).with(id),
                Typing {
                    text: None,
                    focus: true,
                },
            );
        });
        ui.close();
    }
}

/// A widget's tooltip, which the keyboard's cursor shows too.
pub trait Tip {
    /// `on_hover_text`, or `on_disabled_hover_text` on a disabled widget, and the same text beside
    /// the widget while the keyboard's cursor is on it. A label or a disabled widget with one is a
    /// stop for the cursor ([`Kind::Info`]).
    fn tip(self, text: impl Into<WidgetText>) -> Self;
}

impl Tip for egui::Response {
    fn tip(self, text: impl Into<WidgetText>) -> Self {
        let text = text.into();
        // Only where a cursor walks: elsewhere nothing would ever clear them.
        if let Some(seen) = self.ctx.plugin_opt::<Seen>() {
            seen.lock()
                .drawing
                .insert(self.id.accesskit_id(), text.text().to_owned());
        }
        if self.enabled() {
            self.on_hover_text(text)
        } else {
            self.on_disabled_hover_text(text)
        }
    }
}

/// Shows a tip beside `rect`, as a tooltip shows beside the pointer: for a view with a cursor of
/// its own, where its cursor is, and for [`State::tip`]. Below, or above when `rect` is in the
/// lower half of the screen; never taking the pointer.
pub fn tip_at(ctx: &Context, rect: Rect, text: &str) {
    let lower = rect.center().y > ctx.content_rect().center().y;
    let (pivot, at) = if lower {
        (
            egui::Align2::LEFT_BOTTOM,
            rect.left_top() - egui::vec2(0.0, 4.0),
        )
    } else {
        (
            egui::Align2::LEFT_TOP,
            rect.left_bottom() + egui::vec2(0.0, 4.0),
        )
    };
    egui::Area::new(Id::new(TIP))
        .order(Order::Tooltip)
        .pivot(pivot)
        .fixed_pos(at)
        .interactable(false)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.label(text);
            });
        });
}

/// This frame's edits, for the widget the cursor is on.
#[derive(Clone, Default)]
struct Pending {
    target: Option<NodeId>,
    events: Vec<EditEvent>,
}

/// Keeps each frame's widgets from egui's description of the screen, and hands egui the requests
/// it can only take before a frame begins.
#[derive(Default)]
struct Seen {
    widgets: Vec<Widget>,
    /// Each node's parent, to tell which `Ui` a widget is in.
    parents: HashMap<NodeId, NodeId>,
    /// A text field to type in, from the next frame.
    focus: Option<NodeId>,
    /// The widgets' tips, as described on the last frame.
    tips: HashMap<NodeId, String>,
    /// The tips given this frame, so far.
    drawing: HashMap<NodeId, String>,
}

impl egui::Plugin for Seen {
    fn debug_name(&self) -> &'static str {
        "newdawn reach"
    }

    fn input_hook(&mut self, _ctx: &Context, input: &mut egui::RawInput) {
        if let Some(target) = self.focus.take() {
            input.events.push(request(target, accesskit::Action::Focus));
        }
    }

    fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
        let Some(update) = &output.platform_output.accesskit_update else {
            return;
        };
        self.widgets.clear();
        self.parents.clear();
        let before = std::mem::replace(&mut self.tips, std::mem::take(&mut self.drawing));
        for (id, node) in &update.nodes {
            for child in node.children() {
                self.parents.insert(*child, *id);
            }
            // Tipped this frame and the one before: a popup's first frame is a sizing pass, drawn
            // invisible, where every widget is disabled, and a menu's tipped items are no stops
            // then (`keys_typed_with_menu_in_one_frame_act_in_the_menu`, newDAWn).
            let tipped = self.tips.contains_key(id) && before.contains_key(id);
            if let Some(widget) = widget(*id, node, tipped) {
                self.widgets.push(widget);
            }
        }
    }
}

/// A node the keyboard can use, or None: a label, a container, a canvas. A label or a disabled
/// widget is one only with a tip to show (`tipped`).
fn widget(id: NodeId, node: &accesskit::Node, tipped: bool) -> Option<Widget> {
    if node.is_disabled() && !tipped {
        return None;
    }
    let bounds = node.bounds()?;
    let rect = Rect::from_min_max(
        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
    );
    if !rect.is_positive() {
        return None;
    }
    let kind = match node.role() {
        _ if node.is_disabled() => Kind::Info,
        Role::Label if tipped => Kind::Info,
        Role::TextInput | Role::MultilineTextInput | Role::SearchInput => Kind::Text,
        Role::SpinButton | Role::Slider => Kind::Value,
        // Labels sense clicks to select their text; the rest are frames, scroll bars, canvases.
        Role::Label
        | Role::Unknown
        | Role::GenericContainer
        | Role::ScrollBar
        | Role::Splitter
        | Role::Window
        | Role::Pane => return None,
        _ if node.supports_action(accesskit::Action::Click) => Kind::Press,
        _ => return None,
    };
    let label = node
        .label()
        .or(node.value())
        .unwrap_or_default()
        .trim()
        .to_owned();
    Some(Widget {
        id,
        rect,
        kind,
        label,
    })
}

fn request(target: NodeId, action: accesskit::Action) -> Event {
    Event::AccessKitActionRequest(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: target,
        data: None,
    })
}

/// Asks the widget to do something this frame, as a screen reader would.
fn ask(ctx: &Context, target: NodeId, action: accesskit::Action) {
    ctx.input_mut(|input| input.events.push(request(target, action)));
}

/// How many of a value widget's own steps one press makes, for widgets that take no edits; they
/// can't snap, so ← → step as ↑ ↓ do.
fn steps(size: Step) -> usize {
    match size {
        Step::Coarse => 10,
        Step::Fine | Step::Micro => 1,
    }
}

/// The keyboard's cursor among the widgets: one place in each of the host's views `K`, and one in
/// an open menu.
#[derive(Clone, Debug)]
pub struct State<K> {
    ctx: Option<Context>,
    at: HashMap<K, NodeId>,
    menu: Option<NodeId>,
    /// The hand-drawn widgets that read their edits, from the last frame.
    takes: Takes,
    /// The widget the cursor is on now, and whether it's in a menu: for the outline and the help.
    pub now: Option<(Widget, bool)>,
    /// A menu an item was just pressed in. egui closes a menu only on a click of the mouse, so
    /// the cursor closes it, unless the item opened another menu on top.
    pressed_in: Option<egui::LayerId>,
}

impl<K> Default for State<K> {
    fn default() -> Self {
        State {
            ctx: None,
            at: HashMap::new(),
            menu: None,
            takes: Takes::default(),
            now: None,
            pressed_in: None,
        }
    }
}

impl<K: Copy + Eq + Hash> State<K> {
    /// At the start of a frame, before the keys: egui describes the screen from now on, and the
    /// last frame's edits are over.
    pub fn begin(&mut self, ctx: &Context) {
        self.ctx = Some(ctx.clone());
        if ctx.plugin_opt::<Seen>().is_none() {
            ctx.add_plugin(Seen::default());
        }
        ctx.enable_accesskit();
        self.takes = ctx
            .data_mut(|data| data.remove_temp::<Takes>(Id::new(TAKES)))
            .unwrap_or_default();
        ctx.data_mut(|data| data.remove::<Pending>(Id::new(PENDING)));
    }

    /// The widgets in a region, from the last frame.
    fn widgets(ctx: &Context, region: Region) -> Vec<Widget> {
        let seen = ctx.plugin::<Seen>();
        let seen = seen.lock();
        seen.widgets
            .iter()
            .filter(|widget| match region {
                Region::Inside(ui) => inside(&seen.parents, widget.id, ui.accesskit_id()),
                Region::Menu(rect) => rect.contains(widget.rect.center()),
            })
            .cloned()
            .collect()
    }

    /// The menu or list open on top, if one is: a combo box's, a menu's, a right-click menu.
    pub fn menu(&self) -> Option<Rect> {
        self.ctx.as_ref().and_then(menu)
    }

    /// Whether a menu is open with something in it the keys can walk: one asked for is drawn and
    /// its items seen, which takes a frame or two after the ask. A host holds the keys typed after
    /// MENU until it is (newDAWn's `DawApp::keys`), so they act in the menu, as typed slowly.
    pub fn menu_ready(&self) -> bool {
        let Some(ctx) = &self.ctx else {
            return false;
        };
        menu(ctx).is_some_and(|rect| !Self::widgets(ctx, Region::Menu(rect)).is_empty())
    }

    /// MENU: the right-click menu of the widget the cursor is on, in a region.
    pub fn open_menu(&mut self, view: K, region: Region) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        if let Some(widget) = self.cursor(&ctx, view, region) {
            ask_menu(&ctx, widget.id);
        }
    }

    /// Closes the menus and lists that are open.
    pub fn close_menus(&self) {
        if let Some(ctx) = &self.ctx {
            egui::Popup::close_all(ctx);
        }
    }

    /// The widget the cursor is on in a region: the one it was on, if that's still there, or
    /// else the first, top left.
    fn cursor(&self, ctx: &Context, view: K, region: Region) -> Option<Widget> {
        let widgets = Self::widgets(ctx, region);
        let at = match region {
            Region::Menu(_) => self.menu,
            Region::Inside(_) => self.at.get(&view).copied(),
        };
        at.and_then(|id| widgets.iter().find(|widget| widget.id == id).cloned())
            .or_else(|| {
                widgets.into_iter().min_by(|a, b| {
                    (a.rect.top(), a.rect.left())
                        .partial_cmp(&(b.rect.top(), b.rect.left()))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })
    }

    fn put(&mut self, view: K, region: Region, id: NodeId) {
        match region {
            Region::Menu(_) => self.menu = Some(id),
            Region::Inside(_) => {
                self.at.insert(view, id);
            }
        }
    }

    /// The keys, for the widgets in a region. Returns false for the ones it has no use for.
    pub fn key(&mut self, view: K, region: Region, output: Output) -> bool {
        let Some(ctx) = self.ctx.clone() else {
            return false;
        };
        let ctx = &ctx;
        if !matches!(region, Region::Menu(_)) {
            self.menu = None;
        }
        // MENU in an open menu closes it, as BACK does, whatever it holds: even one with nothing
        // the cursor can stand on.
        if let (Region::Menu(_), Output::Action(KeyAction::Menu)) = (region, output) {
            egui::Popup::close_all(ctx);
            return true;
        }
        let Some(cursor) = self.cursor(ctx, view, region) else {
            return false;
        };
        self.put(view, region, cursor.id);
        match output {
            Output::Navigate { direction, .. } => {
                let widgets = Self::widgets(ctx, region);
                if let Some(next) = nearest(&cursor, &widgets, direction) {
                    self.put(view, region, next.id);
                    ask(ctx, next.id, accesskit::Action::ScrollIntoView);
                }
            }
            Output::Action(KeyAction::Open) => match cursor.kind {
                // A value that can be typed opens its field (S12).
                Kind::Value if self.takes.typed.contains(&cursor.id) => {
                    edit_event(ctx, cursor.id, EditEvent::Type);
                }
                Kind::Text => {
                    ctx.plugin::<Seen>().lock().focus = Some(cursor.id);
                    ctx.request_repaint();
                }
                Kind::Press | Kind::Value => {
                    ask(ctx, cursor.id, accesskit::Action::Click);
                    if let Region::Menu(_) = region {
                        self.pressed_in = menu_layer(ctx);
                    }
                }
                Kind::Info => {}
            },
            Output::Step {
                verb: Verb::Value,
                step: size,
                direction,
            } if cursor.kind == Kind::Value => {
                let sign = match direction {
                    Direction::Up | Direction::Right => 1.0,
                    Direction::Down | Direction::Left => -1.0,
                };
                if self.takes.values.contains(&cursor.id) {
                    let change = Change {
                        size,
                        sign,
                        snap: matches!(direction, Direction::Left | Direction::Right),
                    };
                    edit_event(ctx, cursor.id, EditEvent::Change(change));
                } else {
                    let action = if sign > 0.0 {
                        accesskit::Action::Increment
                    } else {
                        accesskit::Action::Decrement
                    };
                    for _ in 0..steps(size) {
                        ask(ctx, cursor.id, action);
                    }
                }
            }
            // MENU (the owner, 2026-10-14): the menu of the widget the cursor is on. (In an open
            // menu it closed it, above.)
            Output::Action(KeyAction::Menu) => ask_menu(ctx, cursor.id),
            // DELETE (and RIPPLE) put a value back to its default (design system §11).
            Output::Action(KeyAction::Delete | KeyAction::Ripple)
                if self.takes.defaults.contains(&cursor.id) =>
            {
                edit_event(ctx, cursor.id, EditEvent::Reset);
            }
            Output::Finish => edit_event(ctx, cursor.id, EditEvent::Keep),
            Output::Cancel => edit_event(ctx, cursor.id, EditEvent::Cancel),
            _ => return false,
        }
        true
    }

    /// Whether the cursor in `region` is on a value, with no menu open: there a step key alone is
    /// VALUE at its size (the host says so to the engine, `mxm_keys::Engine::set_on_value`, before
    /// each key; newDAWn's owner, 2026-10-09: on every value). Where the cursor is now, after this
    /// frame's keys before it moved it, so keys read together do what they'd do typed slowly. A
    /// widget that only takes AccessKit's steps can't snap, so there ← → step as ↑ ↓ do.
    pub fn on_value_in(&self, view: K, region: Region) -> bool {
        let Some(ctx) = &self.ctx else {
            return false;
        };
        if menu(ctx).is_some() {
            return false;
        }
        self.cursor(ctx, view, region)
            .is_some_and(|widget| widget.kind == Kind::Value)
    }

    /// The tip of the widget the cursor is on, if it has one, and where that widget is: for the
    /// host to show with [`tip_at`] while the keyboard is in use.
    pub fn tip(&self) -> Option<(Rect, String)> {
        let ctx = self.ctx.as_ref()?;
        let (widget, _) = self.now.as_ref()?;
        let text = ctx
            .plugin_opt::<Seen>()?
            .lock()
            .tips
            .get(&widget.id)
            .cloned()?;
        Some((widget.rect, text))
    }

    /// Where the cursor is now, for the outline and the help: in the menu if one is open, or
    /// else in the view's region.
    pub fn show(&mut self, view: K, region: Option<Region>) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        let ctx = &ctx;
        if let Some(layer) = self.pressed_in.take()
            && menu_layer(ctx) == Some(layer)
        {
            egui::Popup::close_all(ctx);
        }
        self.now = match (menu(ctx), region) {
            (Some(rect), _) => self
                .cursor(ctx, view, Region::Menu(rect))
                .map(|widget| (widget, true)),
            (None, Some(region)) => {
                self.menu = None;
                self.cursor(ctx, view, region).map(|widget| (widget, false))
            }
            (None, None) => {
                self.menu = None;
                None
            }
        };
    }
}

/// Asks for the menu of the widget with the egui id `id`: an editor's MENU on the control its
/// cursor is on (`navigation`).
pub(crate) fn ask_menu_for(ctx: &Context, id: Id) {
    ask_menu(ctx, id.accesskit_id());
}

/// Asks for the menu of the widget `target`, for [`context_menu`] to open in this pass or the next.
fn ask_menu(ctx: &Context, target: NodeId) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| data.insert_temp(Id::new(MENU_FOR), (target, pass)));
    ctx.request_repaint();
}

fn edit_event(ctx: &Context, target: NodeId, event: EditEvent) {
    ctx.data_mut(|data| {
        let pending = data.get_temp_mut_or_default::<Pending>(Id::new(PENDING));
        if pending.target != Some(target) {
            *pending = Pending {
                target: Some(target),
                events: Vec::new(),
            };
        }
        pending.events.push(event);
    });
}

/// Whether a node is drawn inside the `Ui` with this node id.
fn inside(parents: &HashMap<NodeId, NodeId>, node: NodeId, ui: NodeId) -> bool {
    let mut at = node;
    for _ in 0..64 {
        match parents.get(&at) {
            Some(&parent) if parent == ui => return true,
            Some(&parent) => at = parent,
            None => return false,
        }
    }
    false
}

/// The widget nearest the cursor that way, preferring one in line with it: one that overlaps it
/// across the way is in line, however wide each is.
fn nearest<'a>(from: &Widget, widgets: &'a [Widget], direction: Direction) -> Option<&'a Widget> {
    let centre = from.rect.center();
    let gap = |a: egui::Rangef, b: egui::Rangef| (b.min - a.max).max(a.min - b.max).max(0.0);
    widgets
        .iter()
        .filter(|widget| widget.id != from.id)
        .filter_map(|widget| {
            let delta = widget.rect.center() - centre;
            let (along, across) = match direction {
                Direction::Left => (-delta.x, gap(from.rect.y_range(), widget.rect.y_range())),
                Direction::Right => (delta.x, gap(from.rect.y_range(), widget.rect.y_range())),
                Direction::Up => (-delta.y, gap(from.rect.x_range(), widget.rect.x_range())),
                Direction::Down => (delta.y, gap(from.rect.x_range(), widget.rect.x_range())),
            };
            (along > 1.0).then_some((along + 3.0 * across, widget))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, widget)| widget)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(id: u64, x: f32, y: f32) -> Widget {
        Widget {
            id: NodeId(id),
            rect: Rect::from_min_size(Pos2::new(x, y), egui::vec2(20.0, 20.0)),
            kind: Kind::Press,
            label: String::new(),
        }
    }

    #[test]
    fn down_a_menu_goes_to_the_next_item_however_wide() {
        let item = |id, width: f32, y: f32| Widget {
            rect: Rect::from_min_size(Pos2::new(0.0, y), egui::vec2(width, 18.0)),
            ..at(id, 0.0, 0.0)
        };
        let menu = [item(1, 90.0, 0.0), item(2, 60.0, 20.0), item(3, 85.0, 40.0)];
        assert_eq!(
            nearest(&menu[0], &menu, Direction::Down).map(|w| w.id.0),
            Some(2)
        );
    }

    #[test]
    fn the_arrows_go_to_the_nearest_widget_that_way_in_line_first() {
        // A row of three, and one below the middle.
        let widgets = [
            at(1, 0.0, 0.0),
            at(2, 40.0, 0.0),
            at(3, 80.0, 0.0),
            at(4, 40.0, 40.0),
        ];
        let next =
            |from: usize, direction| nearest(&widgets[from], &widgets, direction).map(|w| w.id.0);
        assert_eq!(next(0, Direction::Right), Some(2));
        assert_eq!(next(1, Direction::Right), Some(3));
        assert_eq!(next(2, Direction::Right), None);
        assert_eq!(next(0, Direction::Down), Some(4));
        assert_eq!(next(3, Direction::Up), Some(2));
    }

    /// A request for a menu lasts its pass and the next. A widget that had none then, and draws
    /// one later, opens nothing; one drawn while the request is current opens.
    #[test]
    fn a_menu_asked_for_opens_only_while_the_request_is_current() {
        use std::cell::Cell;
        use std::rc::Rc;
        let draws_menu = Rc::new(Cell::new(false));
        let ask_now = Rc::new(Cell::new(true));
        let opened = Rc::new(Cell::new(false));
        let (menu, ask, open) = (draws_menu.clone(), ask_now.clone(), opened.clone());
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(200.0, 120.0))
            .build_ui(move |ui| {
                let button = ui.button("Kick");
                if ask.replace(false) {
                    ask_menu(ui.ctx(), button.id.accesskit_id());
                }
                if menu.get() {
                    context_menu(&button, None, |ui| {
                        ui.label("Rename");
                    });
                }
                open.set(egui::Popup::is_any_open(ui.ctx()));
            });
        // Asked, but nothing draws a menu for a few passes: the request goes.
        harness.run_steps(3);
        draws_menu.set(true);
        harness.run_steps(2);
        assert!(!opened.get(), "a stale request opened a menu");

        // Asked while the widget draws one: it opens.
        ask_now.set(true);
        harness.run_steps(2);
        assert!(opened.get());
    }

    /// The value [`value_board`] draws, and the keys queued for it.
    type Board = (
        egui_kittest::Harness<'static>,
        std::rc::Rc<std::cell::Cell<f64>>,
        std::rc::Rc<std::cell::RefCell<Vec<Output>>>,
    );

    /// A hand-drawn value with a default, read with [`edit`], and the keys pressed on it: each
    /// frame takes the presses queued for it, and keeps what the edit kept.
    fn value_board(default: Option<f64>) -> Board {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        let state = Rc::new(RefCell::new(State::<u8>::default()));
        let value = Rc::new(Cell::new(5.0));
        let presses: Rc<RefCell<Vec<Output>>> = Rc::default();
        let (value_, presses_) = (value.clone(), presses.clone());
        let harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(200.0, 120.0))
            .build_ui(move |ui| {
                let mut state = state.borrow_mut();
                state.begin(ui.ctx());
                let region = Region::Inside(ui.unique_id());
                for output in presses_.borrow_mut().drain(..) {
                    state.key(0, region, output);
                }
                let (_, response) =
                    ui.allocate_exact_size(egui::vec2(60.0, 20.0), egui::Sense::click_and_drag());
                response.widget_info(|| egui::WidgetInfo::slider(true, value_.get(), "Level"));
                let (_, edited) = edit(ui, response.id, value_.get(), default, |at, change| {
                    at + change.sign
                });
                if let Some(Edited::Done(kept)) = edited {
                    value_.set(kept);
                }
            });
        (harness, value, presses)
    }

    /// DELETE, and RIPPLE, put a value with a default back to it at once; one without keeps its
    /// value (design system §11).
    #[test]
    fn delete_puts_a_value_back_to_its_default() {
        let (mut harness, value, presses) = value_board(Some(0.0));
        harness.run_steps(2);
        presses.borrow_mut().push(Output::Action(KeyAction::Delete));
        harness.run_steps(2);
        assert_eq!(value.get(), 0.0);
        value.set(3.0);
        harness.run_steps(1);
        presses.borrow_mut().push(Output::Action(KeyAction::Ripple));
        harness.run_steps(2);
        assert_eq!(value.get(), 0.0);

        let (mut harness, value, presses) = value_board(None);
        harness.run_steps(2);
        presses.borrow_mut().push(Output::Action(KeyAction::Delete));
        harness.run_steps(2);
        assert_eq!(value.get(), 5.0, "no default, nothing to go back to");
    }

    /// Every value is one for the step keys, a `DragValue` too, though it takes only AccessKit's
    /// steps (newDAWn's owner, 2026-10-09: a step key alone on every value).
    #[test]
    fn a_drag_value_is_a_value() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        let state = Rc::new(RefCell::new(State::<u8>::default()));
        let on_value = Rc::new(Cell::new(false));
        let (state_, on_value_) = (state.clone(), on_value.clone());
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(200.0, 120.0))
            .build_ui(move |ui| {
                let mut state = state_.borrow_mut();
                state.begin(ui.ctx());
                on_value_.set(state.on_value_in(0, Region::Inside(ui.unique_id())));
                let mut step = 4;
                ui.add(egui::DragValue::new(&mut step));
            });
        harness.run_steps(3);
        assert!(on_value.get());
    }

    /// A label or a disabled widget with a tip is a stop for the cursor, which shows the tip; one
    /// without stays out, and a widget without a tip shows none.
    #[test]
    fn the_cursor_stops_on_what_has_a_tip_and_shows_it_there_only() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let state = Rc::new(RefCell::new(State::<u8>::default()));
        let region: Rc<RefCell<Option<Region>>> = Rc::default();
        let (state_, region_) = (state.clone(), region.clone());
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(300.0, 160.0))
            .build_ui(move |ui| {
                let mut state = state_.borrow_mut();
                state.begin(ui.ctx());
                *region_.borrow_mut() = Some(Region::Inside(ui.unique_id()));
                ui.label("⚠ 2 plugins left out").tip(
                    "Kick: no audio out
Hat: crashed",
                );
                ui.label("Just words");
                ui.add_enabled(false, egui::Button::new("Loop"))
                    .tip("Set a loop first");
                let _ = ui.button("Play");
                state.show(0, *region_.borrow());
            });
        harness.run_steps(3);
        let region = region.borrow().unwrap();
        let ctx = harness.ctx.clone();
        let widgets = State::<u8>::widgets(&ctx, region);
        let kinds: Vec<(&str, Kind)> = widgets
            .iter()
            .map(|widget| (widget.label.as_str(), widget.kind))
            .collect();
        assert!(
            kinds.contains(&("⚠ 2 plugins left out", Kind::Info)),
            "{kinds:?}"
        );
        assert!(kinds.contains(&("Loop", Kind::Info)), "{kinds:?}");
        assert!(kinds.contains(&("Play", Kind::Press)), "{kinds:?}");
        assert!(
            !kinds.iter().any(|(label, _)| *label == "Just words"),
            "{kinds:?}"
        );

        // The cursor starts top left, on the warning: its tip is shown.
        let tip = state.borrow().tip().map(|(_, text)| text);
        assert_eq!(
            tip.as_deref(),
            Some(
                "Kick: no audio out
Hat: crashed"
            )
        );
        // Down to the button without one: none.
        let play = widgets
            .iter()
            .find(|widget| widget.label == "Play")
            .unwrap()
            .id;
        state.borrow_mut().put(0, region, play);
        harness.run_steps(1);
        assert_eq!(state.borrow().tip(), None);
    }

    /// MENU closes an open menu even when nothing in it can be chosen, as BACK does.
    #[test]
    fn menu_closes_an_open_menu_with_nothing_to_choose() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        let state = Rc::new(RefCell::new(State::<u8>::default()));
        let (open_it, press, opened) = (
            Rc::new(Cell::new(true)),
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(false)),
        );
        let (state_, open_, press_, opened_) = (
            state.clone(),
            open_it.clone(),
            press.clone(),
            opened.clone(),
        );
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(200.0, 120.0))
            .build_ui(move |ui| {
                let mut state = state_.borrow_mut();
                state.begin(ui.ctx());
                if press_.replace(false)
                    && let Some(rect) = state.menu()
                {
                    assert!(state.key(0, Region::Menu(rect), Output::Action(KeyAction::Menu)));
                }
                let button = ui.button("Kick");
                let keys = open_.replace(false).then_some(button.rect);
                context_menu(&button, keys, |ui| {
                    ui.add_enabled(false, egui::Button::new("Paste here"));
                });
                opened_.set(egui::Popup::is_any_open(ui.ctx()));
            });
        harness.run_steps(2);
        assert!(opened.get(), "the premise: the menu is open");
        press.set(true);
        harness.run_steps(2);
        assert!(!opened.get(), "MENU closed it");
    }
}
