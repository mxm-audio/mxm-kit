//! Everything on screen within the keyboard's reach (the owner, 2026-10-07: "EVERYTHING on screen
//! should be reachable and editable by the keyboard"), in newDAWn and the collection's editors.
//!
//! Where a view has no cursor of its own, and in any menu or list that opens, the cursor walks the
//! widgets egui describes to screen readers each frame: the arrows go to the nearest one that way,
//! OPEN presses it as a click does or types in a text field (until Escape or Enter), and VALUE +
//! arrows change a value. A widget is in reach as soon as it is drawn, with nothing to register. A
//! widget drawn by hand says what it is with `Response::widget_info`, or it is left out, as a
//! canvas with a cursor of its own is; one that changes a value in units of its own reads the keys
//! with [`edit`]. The keys come from `mxm-keys`; the views are the host's, named by any key `K`.

use std::collections::HashMap;
use std::hash::Hash;

use egui::accesskit::{self, ActionRequest, NodeId, Role, TreeId};
use egui::{Context, Event, FullOutput, Id, Order, Pos2, Rect, Ui};
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

/// The menu or list open on top, if one is: a combo box's, a menu's, a right-click menu.
fn menu(ctx: &Context) -> Option<Rect> {
    let layer = menu_layer(ctx)?;
    ctx.memory(|memory| memory.area_rect(layer.id))
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

/// A hand-drawn value widget's keyboard edits, in its own units. Call it every frame with the
/// widget's id and its value: `step` takes a value, a step size and +1 or −1, and gives the next
/// value. Returns the value to show, the edit's while one is going on, and what changed now.
pub fn edit(
    ui: &Ui,
    id: Id,
    current: f64,
    step: impl Fn(f64, Step, f64) -> f64,
) -> (f64, Option<Edited>) {
    let ctx = ui.ctx();
    let node = id.accesskit_id();
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Takes>(Id::new(TAKES))
            .0
            .push(node)
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
            EditEvent::Step(size, sign) => {
                let next = step(value.unwrap_or(current), size, sign);
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

/// A widget's right-click menu, which the menu key (Shift+F10) opens too, below it: for a
/// widget the cursor is on, or where `keys` says when a view with a cursor of its own asks.
pub fn context_menu(response: &egui::Response, keys: Option<Rect>, add: impl FnOnce(&mut Ui)) {
    let ctx = &response.ctx;
    let id = response.id.with("menu from the keys");
    let node = response.id.accesskit_id();
    let asked = ctx.data_mut(|data| {
        let key = Id::new(MENU_FOR);
        let asked = data.get_temp::<NodeId>(key) == Some(node);
        if asked {
            data.remove::<NodeId>(key);
        }
        asked
    });
    let anchor = keys.or(asked.then_some(response.rect));
    if anchor.is_some() {
        egui::Popup::open_id(ctx, id);
    }
    if egui::Popup::is_id_open(ctx, id) {
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

/// The hand-drawn value widgets that read their edits, as drawn on a frame.
#[derive(Clone, Default)]
struct Takes(Vec<NodeId>);

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditEvent {
    Step(Step, f64),
    Keep,
    Cancel,
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
        for (id, node) in &update.nodes {
            for child in node.children() {
                self.parents.insert(*child, *id);
            }
            if let Some(widget) = widget(*id, node) {
                self.widgets.push(widget);
            }
        }
    }
}

/// A node the keyboard can use, or None: a label, a container, a canvas.
fn widget(id: NodeId, node: &accesskit::Node) -> Option<Widget> {
    if node.is_disabled() {
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

/// How many of a value widget's own steps one press makes, for widgets that take no edits.
fn steps(size: Step) -> usize {
    match size {
        Step::Coarse => 10,
        Step::Fine | Step::Micro | Step::Musical => 1,
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
    takes: Vec<NodeId>,
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
            takes: Vec::new(),
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
            .unwrap_or_default()
            .0;
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

    /// The menu key: the right-click menu of the widget the cursor is on, in a region.
    pub fn open_menu(&mut self, view: K, region: Region) {
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        if let Some(widget) = self.cursor(&ctx, view, region) {
            ctx.data_mut(|data| data.insert_temp(Id::new(MENU_FOR), widget.id));
            ctx.request_repaint();
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
                if self.takes.contains(&cursor.id) {
                    edit_event(ctx, cursor.id, EditEvent::Step(size, sign));
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
            Output::Finish => edit_event(ctx, cursor.id, EditEvent::Keep),
            Output::Cancel => edit_event(ctx, cursor.id, EditEvent::Cancel),
            _ => return false,
        }
        true
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
}
