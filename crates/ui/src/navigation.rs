//! The keyboard cursor: a card, a parameter inside it, and the value.
//!
//! It reads the keys through the keyboard language (`navigation/language.rs`): the arrows move
//! parameter to parameter inside the card, COARSE + arrows card to card, VIEW + arrows to the bars
//! above the cards, and VALUE + arrows edit the parameter the cursor is on.
//!
//! # The map is a by-product of drawing, never an authored table
//!
//! Only one editor in the collection declares which parameter sits on which card, and a layout
//! that reflows would make eight more such tables wrong the first time a disclosure opened. So no
//! table: a control [`mark`]s itself as it paints, inside a [`card`] scope the paging renderer
//! opens and an [`at`] scope its plugin's binding opens. The registry is then exactly what is on
//! screen, including what a disclosure just revealed, because it *is* what was drawn.
//!
//! The cursor therefore acts on the **previous** frame's registry, which is how
//! [`crate::flow::drawn`] already works and is not a defect: a frame that had not been drawn yet
//! has no geometry to navigate.
//!
//! # It does not own the value
//!
//! This module moves a cursor and hands egui's focus to whatever it lands on. The value edit is
//! the focused control's own [`crate::control`] keyboard path, and the *size* of a step is the
//! parameter's, carried in [`crate::control::Steps`] by the plugin that owns it. Nothing here
//! knows what a parameter is.

use std::collections::HashMap;

use egui::{Id, Pos2, Rect, Response, Stroke, StrokeKind, Ui};

use crate::control::Press;
use crate::space::{HAIRLINE, RADIUS};
use crate::theme::Tokens;

mod language;
mod sheet;

/// One control, as the frame painted it.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    /// The [`crate::paging::Key`] of the card it was drawn in.
    pub card: u64,
    /// The parameter's permanent id — stable across a reflow, which an index is not.
    pub key: String,
    /// The complete parameter control. Multi-cell controls union every cell here.
    pub rect: Rect,
    /// The primary focus target, so the cursor can hand egui its focus.
    pub id: Id,
    /// Every focusable widget belonging to this parameter. A segmented control has one per cell.
    pub focus_ids: Vec<Id>,
    /// Where each of `focus_ids` was painted: a segmented control's cells, each a stop for the
    /// keyboard language's arrows.
    pub cells: Vec<Rect>,
    /// The cell the pointer took, of `focus_ids`, when `pointed`.
    pub pointed_cell: usize,
    /// The pointer pressed, dragged or clicked this control in the frame that drew it.
    ///
    /// Read from the control's own [`Response`], never from where the press landed: a press on a
    /// popup drawn over a knob belongs to the popup, and only the widget can say it was pressed.
    pub pointed: bool,
}

/// What the cursor is on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cursor {
    pub card: Option<u64>,
    pub key: Option<String>,
}

/// The cursor, plus what it remembers about cards it has left.
///
/// Held by the editor beside `view` and the text-entry buffers rather than in egui's memory:
/// `panel` is a free function over borrowed state precisely so a test can drive it and assert
/// where the cursor went.
#[derive(Clone, Debug, Default)]
pub struct State {
    cursor: Cursor,
    /// The parameter last visited on each card, so re-entering a card resumes rather than resets.
    remembered: HashMap<u64, String>,
    /// Set for the one frame a move happened, so focus is requested then and not every frame.
    moved: bool,
    /// The pointer moved the cursor while something else held the keyboard — an inert frame, a
    /// text field, an open menu — so the focus that move owes the control has not been handed over
    /// yet. The first frame free to hand it over does; a newer cursor move supersedes it.
    owed: bool,
    /// The keyboard language's engine, which the cursor reads keys through ([`language`]).
    language: Option<mxm_keys::Engine>,
    /// The cell of the parameter the cursor is on, of its `focus_ids`: the keyboard language stops
    /// on each cell of a segmented control, and OPEN presses the one the cursor is on.
    cell: usize,
    /// VIEW + arrows left the cards for a bar: which, counted from the top.
    bar: Option<usize>,
    /// A card COARSE + an arrow asked the renderer to show from another page, and the frames left
    /// to wait for it before the cursor stops waiting.
    awaiting: Option<(u64, u8)>,
    /// The cards the app bar draws (`paged_with_bar`): always on screen, never a page.
    bar_cards: Vec<u64>,
    /// VIEW + down just brought the cursor back from the bars to the cards.
    home: bool,
    /// The cursor among a bar's widgets.
    reach: crate::reach::State<usize>,
    /// F1's sheet of the keys is open (`sheet`).
    sheet: bool,
}

impl State {
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    /// The card the cursor is on, for the renderer that draws its outline.
    pub fn card(&self) -> Option<u64> {
        self.cursor.card
    }

    pub fn parameter(&self) -> Option<&str> {
        self.cursor.key.as_deref()
    }

    /// The bar the cursor is in, counted from the top, when VIEW took it out of the cards.
    pub fn bar(&self) -> Option<usize> {
        self.bar
    }

    /// The cell of the parameter the cursor is on: 0, but for a segmented control's other cells.
    pub fn cell(&self) -> usize {
        self.cell
    }

    fn settle(&mut self, spot: &Spot) {
        self.settle_cell(spot, 0);
    }

    fn settle_cell(&mut self, spot: &Spot, cell: usize) {
        self.cursor.card = Some(spot.card);
        self.cursor.key = Some(spot.key.clone());
        self.cell = cell.min(spot.focus_ids.len().saturating_sub(1));
        self.remembered.insert(spot.card, spot.key.clone());
    }
}

fn registry_id() -> Id {
    Id::new("mxm-navigation")
}
fn current_id() -> Id {
    registry_id().with("current")
}
fn last_id() -> Id {
    registry_id().with("last")
}
fn card_id() -> Id {
    registry_id().with("card")
}
fn scope_id() -> Id {
    registry_id().with("scope")
}
fn outline_id() -> Id {
    registry_id().with("outline")
}
fn running_id() -> Id {
    registry_id().with("running")
}
fn target_id() -> Id {
    registry_id().with("target")
}
fn shown_id() -> Id {
    registry_id().with("shown")
}
fn bars_current_id() -> Id {
    registry_id().with("bars current")
}
fn bars_last_id() -> Id {
    registry_id().with("bars last")
}
fn value_keys_id() -> Id {
    registry_id().with("value keys")
}

/// Records a bar drawn above the cards — the app bar, the view bar — so the keyboard language's
/// VIEW can reach its widgets. The shell calls it inside each bar's panel.
pub fn bar(ui: &Ui) {
    let entry = (ui.unique_id(), ui.max_rect());
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<(Id, Rect)>>(bars_current_id())
            .push(entry);
    });
}

/// The bars the last frame drew, from the top.
fn bars(ctx: &egui::Context) -> Vec<(Id, Rect)> {
    let mut bars = ctx.data(|d| {
        d.get_temp::<Vec<(Id, Rect)>>(bars_last_id())
            .unwrap_or_default()
    });
    bars.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
    bars
}

/// What the keyboard language asks of the parameter the cursor is on, this frame: VALUE's steps,
/// the end of the gesture (kept or cancelled), and DELETE's reset to the default.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ValueKeys {
    pub presses: Vec<Press>,
    pub keep: bool,
    pub cancel: bool,
    pub reset: bool,
}

fn publish_value_keys(ctx: &egui::Context, keys: ValueKeys) {
    ctx.data_mut(|d| d.insert_temp(value_keys_id(), keys));
}

/// Takes this frame's value keys, for the control the cursor is on.
pub(crate) fn take_value_keys(ctx: &egui::Context) -> ValueKeys {
    ctx.data_mut(|d| d.remove_temp::<ValueKeys>(value_keys_id()))
        .unwrap_or_default()
}

/// Whether the cursor is currently *shown*, as opposed to merely positioned.
///
/// **Where the keyboard is and whether to draw it are two facts, and this crate used to have
/// one.** The cursor lands on the first control the moment an editor first paints, so every panel
/// in the collection opened with a card ringed and a knob focused that nobody had asked for, and
/// the ring stayed put through an entire session of mouse work. The owner reported it on the
/// grain-fx panel (2026-09-11): *"this should not start with a border ... the border should only
/// appear when the user starts keyboard navigation and editing."*
///
/// So the position is kept and only the paint is withheld. [`run`] hides it on any pointer press
/// and shows it on any keyboard gesture, which is the web's `:focus-visible` rule and what §11
/// means by *keyboard focus is always visible*: visible to whoever is using the keyboard.
///
/// **The position survives being hidden, and that is the point of separating them.** A knob turned
/// with the mouse is still the cursor's parameter, so the very next arrow press edits *that* value
/// and reveals the cursor already sitting on it — no keystroke is spent arriving, which is the
/// whole speed of the feature.
pub fn shown(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(shown_id()).unwrap_or(false))
}

/// Hides the cursor. Called on a pointer press, wherever the press landed.
fn conceal(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(shown_id(), false));
}

/// Shows the cursor. Called when a key that drives it is pressed.
fn reveal(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(shown_id(), true));
}

/// Whether a cursor is driving this context, set by [`run`] and read by [`crate::control`].
///
/// Where a cursor runs, the keyboard language owns the arrows: they move the cursor, and VALUE +
/// an arrow edits the value. Where none does — the player, a cardless surface such as Parameters
/// — a bare arrow edits the focused control itself, because taking that away and giving nothing
/// back would leave those surfaces with no keyboard editing at all.
pub fn running(ui: &Ui) -> bool {
    ui.ctx()
        .data(|d| d.get_temp::<bool>(running_id()).unwrap_or(false))
}

/// Whether the control being painted is the cursor's parameter target.
///
/// Keyboard behavior cannot depend solely on egui focus. A custom painted control can remain the
/// navigation target while none of its generated responses retains native focus between frames.
pub fn keyboard_target(ui: &Ui) -> bool {
    let (Some(card), Some(key)) = (
        ui.ctx().data(|d| d.get_temp::<u64>(card_id())),
        ui.ctx().data(|d| d.get_temp::<String>(scope_id())),
    ) else {
        return false;
    };
    ui.ctx()
        .data(|d| d.get_temp::<(u64, String)>(target_id()))
        .is_some_and(|target| target.0 == card && target.1 == key)
}

/// Opens the card a control is being drawn in. The paging renderer calls this; a plugin does not.
///
/// Nested cards do not exist (§3.3 nests *groups*, one level, and a group is not a card), so this
/// sets rather than pushes.
pub fn card<R>(ui: &mut Ui, key: u64, body: impl FnOnce(&mut Ui) -> R) -> R {
    ui.ctx().data_mut(|d| d.insert_temp(card_id(), key));
    let out = body(ui);
    ui.ctx().data_mut(|d| d.remove::<u64>(card_id()));
    out
}

/// Opens a card drawn in the app bar, above the paging renderer: design system §3.1 puts the master
/// output there. It is [`card`] plus a record of where it was painted, because the renderer's
/// report, which gives every other card its geometry, cannot see the bar. `body` returns the
/// painted rectangle. [`paged_with_bar`] reads it back next frame. `key` must not collide with a
/// paging card key.
pub fn bar_card(ui: &mut Ui, key: u64, body: impl FnOnce(&mut Ui) -> Rect) {
    let rect = card(ui, key, body);
    ui.ctx().data_mut(|d| d.insert_temp(bar_rect_id(key), rect));
}

/// Takes a bar card the bar did not draw this frame out of the cursor's sequence — one the app
/// bar moved into its `…` menu (`shell::product_actions`). Its rectangle is last frame's
/// otherwise, and the cursor would stop on a card that is no longer there.
pub fn bar_card_absent(ctx: &egui::Context, key: u64) {
    ctx.data_mut(|d| d.remove::<Rect>(bar_rect_id(key)));
}

fn bar_rect_id(key: u64) -> Id {
    registry_id().with(("bar", key))
}

/// Names the parameter whose control is about to be drawn.
///
/// The plugin's binding opens this, because the permanent id is the plugin's to know. Without it a
/// control paints exactly as before and simply does not join the registry — which is what keeps
/// this rollout per-editor rather than all-or-nothing.
pub fn at<R>(ui: &mut Ui, key: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    ui.ctx()
        .data_mut(|d| d.insert_temp(scope_id(), key.to_owned()));
    let out = body(ui);
    ui.ctx().data_mut(|d| d.remove::<String>(scope_id()));
    out
}

/// Draws an editor-only control inside a parameter's scope without joining the registry.
///
/// A picker that chooses *which* parameter a control edits is not itself a parameter and has no
/// permanent id, but it is drawn inside the owning parameter's [`at`] scope — mxm-mono-08's routing
/// slider carries its source menu on its own name line. Left alone it would register as a second
/// cell of that parameter and then answer the keys meant for the parameter. This closes the scope
/// for its body: nothing inside marks, and [`keyboard_target`] is false there.
pub fn aside<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let held = ui.ctx().data(|d| d.get_temp::<String>(scope_id()));
    ui.ctx().data_mut(|d| d.remove::<String>(scope_id()));
    let out = body(ui);
    if let Some(key) = held {
        ui.ctx().data_mut(|d| d.insert_temp(scope_id(), key));
    }
    out
}

/// Records a focusable control at its painted rectangle. Called by the controls in
/// [`crate::control`], once each, on the response that carries their focus.
///
/// **It also records whether the pointer took the control this frame** — pressed it, dragged it or
/// clicked it — and [`run`] then moves the cursor there. egui gives a painted control no keyboard
/// focus when it is clicked or dragged (only its text and drag-value widgets ask for it), so a
/// cursor that followed focus alone stayed on the previous parameter, hidden, and the next arrow
/// edited that one instead of the knob just turned. The owner's report, 2026-09-23: *"When I click
/// on a parameter, or move it with the mouse it should immediately be possible to edit that
/// parameter with the arrow keys."*
///
/// Silently does nothing outside a [`card`] and an [`at`] scope, and during the paging renderer's
/// measurement pass — that pass draws into a separate, inputless context and its geometry is not
/// on screen.
pub fn mark(ui: &Ui, response: &Response, rect: Rect) {
    let pointed = response.is_pointer_button_down_on() || response.clicked();
    register(ui, response.id, rect, pointed);
}

/// [`mark`], for a control a press must not select: `control::remove_mark`, whose press deletes
/// the row it sits on, so the cursor would be left on a parameter that is gone.
pub fn mark_unclaimed(ui: &Ui, id: Id, rect: Rect) {
    register(ui, id, rect, false);
}

fn register(ui: &Ui, id: Id, rect: Rect, pointed: bool) {
    let (Some(card), Some(key)) = (
        ui.ctx().data(|d| d.get_temp::<u64>(card_id())),
        ui.ctx().data(|d| d.get_temp::<String>(scope_id())),
    ) else {
        return;
    };
    ui.ctx().data_mut(|d| {
        let spots = d.get_temp_mut_or_default::<Vec<Spot>>(current_id());
        if let Some(spot) = spots
            .iter_mut()
            .find(|spot| spot.key == key && spot.card == card)
        {
            // A segmented parameter paints one response per cell. It is one navigation target:
            // keep the first cell as the target, but use the union for geometry and remember every
            // cell so clicking any of them can move the cursor to this parameter.
            spot.rect = spot.rect.union(rect);
            if !spot.focus_ids.contains(&id) {
                spot.focus_ids.push(id);
                spot.cells.push(rect);
            }
            if pointed {
                spot.pointed = true;
                spot.pointed_cell = spot.focus_ids.iter().position(|&f| f == id).unwrap_or(0);
            }
            return;
        }
        spots.push(Spot {
            card,
            key,
            rect,
            id,
            focus_ids: vec![id],
            cells: vec![rect],
            pointed,
            pointed_cell: 0,
        });
    });
}

/// The registry as the previous frame left it.
pub fn spots(ctx: &egui::Context) -> Vec<Spot> {
    ctx.data(|d| d.get_temp::<Vec<Spot>>(last_id()).unwrap_or_default())
}

/// A move the keyboard language asked for. VALUE's presses are not here: they go to the control
/// the cursor is on, as [`ValueKeys`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Card(Dir),
    /// The keyboard language's bare arrow: the next parameter that way in the card, ← → only along
    /// its row.
    Any(Dir),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    const fn forward(self) -> bool {
        matches!(self, Self::Right | Self::Down)
    }
}

/// Moves the cursor, and hands egui's focus to where it lands.
///
/// `order` is every card key in the paging plan's category-first order, flattened across pages;
/// `cards` is the report's exact visible `(key, rectangle)` list. A page is where a card happens to
/// sit at this width, not a boundary the cursor respects. Crossing one returns the card that must be
/// shown, and the caller asks the paging renderer for it.
///
/// `inert` suspends the whole layer — while the preset browser is open, while a value is being
/// typed, or whenever another surface owns the keyboard.
#[must_use]
pub fn run(
    ctx: &egui::Context,
    state: &mut State,
    order: &[u64],
    cards: &[(u64, Rect)],
    inert: bool,
) -> Option<u64> {
    // Roll the registry: what the last frame drew becomes what this frame navigates.
    ctx.data_mut(|d| {
        let current = d.get_temp::<Vec<Spot>>(current_id()).unwrap_or_default();
        if !current.is_empty() {
            d.insert_temp(last_id(), current);
        }
        d.insert_temp(current_id(), Vec::<Spot>::new());
        let bars = d
            .get_temp::<Vec<(Id, Rect)>>(bars_current_id())
            .unwrap_or_default();
        d.insert_temp(bars_last_id(), bars);
        d.insert_temp(bars_current_id(), Vec::<(Id, Rect)>::new());
    });
    state.reach.begin(ctx);

    // Declared even while inert: the editor still has a cursor, it is merely suspended, and the
    // controls must not fall back to their cursorless arrow handling for one frame because the
    // preset browser happens to be open.
    ctx.data_mut(|d| d.insert_temp(running_id(), true));

    // **A pointer press puts the cursor away, wherever it landed, and it is hidden before the
    // inert check on purpose.** A click into the preset browser is still somebody reaching for the
    // mouse, and leaving a stale ring behind the browser is the very thing the owner reported.
    if ctx.input(|input| input.pointer.any_pressed()) {
        conceal(ctx);
    }

    state.moved = false;
    let spots = spots(ctx);

    // **Follow the pointer.** A control the pointer pressed, dragged or clicked in the last frame
    // is where the keyboard is now, so the next VALUE press edits the knob just turned rather than
    // the parameter the cursor was left on. egui never focuses a painted control for a click, so
    // this cannot come from the focus-follow below; the registry records it instead (see [`mark`]).
    //
    // **Before the inert return, and position only.** A gesture whose next frame is inert — a value
    // opened for typing, the preset browser in front — would otherwise be lost, because the control
    // is not redrawn while its entry is open. The focus the move owes is handed over later, on the
    // first frame nothing else holds the keyboard.
    let pointed = spots.iter().find(|spot| spot.pointed);
    if let Some(spot) = pointed {
        if state.cursor.card != Some(spot.card)
            || state.cursor.key.as_deref() != Some(&spot.key)
            || state.cell != spot.pointed_cell
        {
            state.settle_cell(spot, spot.pointed_cell);
        }
        let focused = ctx.memory(|m| m.focused());
        state.owed = !focused.is_some_and(|id| spot.focus_ids.contains(&id));
    }

    // **A text field or an open popup owns the keyboard**, exactly as the editor's own `inert`
    // surfaces do: a long selector's search field takes the arrows and the letters as text, and
    // any open list walks its rows with the arrows. The cursor neither reveals nor takes a key
    // while either is up, and the pointer's owed focus waits for them to close.
    //
    // `Popup::is_any_open` and not `Context::any_popup_open`: this runs before anything is drawn,
    // and the context's answer is built from the popups drawn so far in this pass — none yet.
    // egui's memory keeps a menu's open state from frame to frame.
    let inert = inert || ctx.text_edit_focused() || egui::Popup::is_any_open(ctx);

    // **BACK during a mouse drag cancels it, before the inert return** (`crate::drag`): egui has
    // already ended a drag on `Escape` this frame, and a surface holding the keyboard — a focused
    // canvas — must not keep that drag's value either.
    let back_spent = language::escape_is_back(state) && crate::drag::notice_escape(ctx);

    // **F1 shows and hides the keys** (`sheet`), whatever holds the keyboard, and `Escape` closes
    // the sheet before anything else hears it.
    let mut open = state.sheet;
    sheet::keys(ctx, &mut open);
    sheet::show(ctx, &mut open, language::engine(state).keymap());
    state.sheet = open;
    if inert {
        if let Some(engine) = state.language.as_mut() {
            let _ = engine.interrupt();
        }
        clear_target(ctx);
        return None;
    }

    // A key brings it back — *after* the inert check, because while the browser is open or a
    // value is being typed the arrows are that surface's and say nothing about this cursor. The
    // keyboard language reveals it as it reads its keys.

    if spots.is_empty() {
        clear_target(ctx);
        return None;
    }

    // Follow egui's focus: a registered cell that took focus some other way moves the cursor to
    // it, so the two never disagree about where the keyboard is. **Not in a frame the pointer
    // claimed a control**: egui gives up focus on a click but not on a drag, so the knob the cursor
    // was on can still hold it while another is dragged, and following it would undo the
    // pointer's move.
    let focused = ctx.memory(|m| m.focused());
    if pointed.is_none()
        && let Some(id) = focused
        && let Some(spot) = spots.iter().find(|s| s.focus_ids.contains(&id))
    {
        let cell = spot.focus_ids.iter().position(|&f| f == id).unwrap_or(0);
        if state.cursor.card != Some(spot.card)
            || state.cursor.key.as_deref() != Some(&spot.key)
            || state.cell != cell
        {
            state.settle_cell(spot, cell);
        }
    }

    // Hand over the focus a pointer move owed, now that nothing else holds the keyboard.
    if state.owed {
        state.owed = false;
        state.moved = true;
    }

    let steps = language::read(ctx, state, back_spent);

    // In a bar, the cards keep their place but give up the keyboard, and the bar's own cursor is
    // outlined over it while the keyboard is in use.
    if let Some(bar) = state.bar {
        clear_target(ctx);
        let region = bars(ctx)
            .get(bar)
            .map(|&(ui, _)| crate::reach::Region::Inside(ui));
        state.reach.show(bar, region);
        if shown(ctx)
            && let Some((widget, _)) = &state.reach.now
        {
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                registry_id().with("bar cursor"),
            ))
            .rect_stroke(
                widget.rect.expand(2.0),
                RADIUS,
                Stroke::new(2.0, ctx.global_style().visuals.selection.stroke.color),
                StrokeKind::Outside,
            );
        }
        return None;
    }

    // **The page changed under the cursor** — a tab chosen from the view bar, or with the mouse —
    // so its card is not drawn any more: it goes to the first card of the page shown, where it was
    // last on it. Not while a card COARSE asked for is on its way (the owner, 2026-10-07: back from
    // the tabs the cursor was on nothing, and COARSE + an arrow started from a card off screen).
    if let Some((card, left)) = state.awaiting {
        let arrived = cards.iter().any(|(key, _)| *key == card);
        state.awaiting = (!arrived && left > 0).then_some((card, left - 1));
    }
    let home = std::mem::take(&mut state.home);
    if state.awaiting.is_none()
        && let Some(card) = state.cursor.card
        && (!cards.iter().any(|(key, _)| *key == card) || (home && state.bar_cards.contains(&card)))
        && let Some(first) = order.iter().find(|key| {
            !state.bar_cards.contains(key) && cards.iter().any(|(visible, _)| visible == *key)
        })
    {
        enter(state, &spots, *first);
    }

    // A cursor that has never landed starts on the first thing drawn, so the first keystroke is
    // never spent arriving.
    if state.cursor.key.is_none() {
        // On the page's first card rather than a parameter in the app bar, which VIEW reaches (the
        // owner, 2026-10-07: it started on Output).
        let first = spots
            .iter()
            .find(|spot| !state.bar_cards.contains(&spot.card))
            .or(spots.first());
        if let Some(first) = first {
            state.settle(first);
            state.moved = true;
        }
        if steps.is_empty() {
            focus(ctx, state, &spots);
            publish_target(ctx, state);
            return None;
        }
    }

    let mut show = None;
    for step in steps {
        match step {
            Step::Card(dir) => {
                if let Some(wanted) = move_card(state, &spots, order, cards, dir) {
                    show = Some(wanted);
                    state.awaiting = Some((wanted, 4));
                }
            }
            Step::Any(dir) => move_any(state, &spots, dir),
        }
    }
    focus(ctx, state, &spots);
    publish_target(ctx, state);
    show
}

fn publish_target(ctx: &egui::Context, state: &State) {
    ctx.data_mut(
        |data| match (state.cursor.card, state.cursor.key.as_ref()) {
            (Some(card), Some(key)) => {
                data.insert_temp(target_id(), (card, key.clone()));
            }
            _ => data.remove::<(u64, String)>(target_id()),
        },
    );
}

fn clear_target(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<(u64, String)>(target_id()));
}

/// Stops this context's cursor layer and gives the controls back their cursorless keyboard
/// behavior.
///
/// Used by cardless surfaces such as the developer Parameters list. Clearing the paint registry is
/// deliberate: retaining the last musician page would let invisible cards consume its arrows.
pub fn stop(ctx: &egui::Context) {
    // The reveal flag is deliberately *not* cleared, and is kept current here: a cardless surface
    // is where somebody clicks around for a while, and the card page they return to must not light
    // up because this surface stopped watching the mouse.
    if ctx.input(|input| input.pointer.any_pressed()) {
        conceal(ctx);
    }
    ctx.data_mut(|d| {
        d.remove::<bool>(running_id());
        d.remove::<Vec<Spot>>(current_id());
        d.remove::<Vec<Spot>>(last_id());
        d.remove::<(u64, String)>(target_id());
        d.remove::<u64>(outline_id());
        d.remove::<Vec<(Id, Rect)>>(bars_current_id());
        d.remove::<Vec<(Id, Rect)>>(bars_last_id());
    });
}

/// Gives egui the focus the cursor claims, and locks its own arrow travel out of it.
///
/// Only on a frame the cursor moved: requesting focus every frame would fight anything else that
/// legitimately takes it, and the lock filter is what stops a bare arrow being spent twice.
fn focus(ctx: &egui::Context, state: &State, spots: &[Spot]) {
    let Some(spot) = current_spot(state, spots) else {
        return;
    };
    let id = spot.focus_ids.get(state.cell).copied().unwrap_or(spot.id);
    ctx.memory_mut(|m| {
        if state.moved {
            m.request_focus(id);
        }
        m.set_focus_lock_filter(
            id,
            egui::EventFilter {
                tab: false,
                // `true` means the focused widget has exclusive access, so egui does not also
                // move its own focus ring after the navigation layer consumes or delegates it.
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: false,
            },
        );
    });
}

fn current_spot<'s>(state: &State, spots: &'s [Spot]) -> Option<&'s Spot> {
    let key = state.cursor.key.as_deref()?;
    let card = state.cursor.card?;
    spots.iter().find(|s| s.card == card && s.key == key)
}

/// Moves to a card painted in the requested direction.
///
/// Geometry comes from the paging renderer's exact card rectangles, not the union of whichever
/// controls happen to be inside one. At a page edge there is no geometry for the next page, so the
/// adjacent card in the paging plan is the bridge: right/down move forward, left/up move backward.
fn move_card(
    state: &mut State,
    spots: &[Spot],
    order: &[u64],
    cards: &[(u64, Rect)],
    dir: Dir,
) -> Option<u64> {
    let current = state.cursor.card?;
    let here = cards
        .iter()
        .find(|(key, _)| *key == current)
        .map(|(_, rect)| *rect);
    let geometric = here.and_then(|here| {
        directional(
            here,
            cards
                .iter()
                .filter(|(key, _)| *key != current)
                .map(|(key, rect)| (*key, *rect)),
            dir,
        )
    });

    // Geometry exists only for the selected page. At an edge, the adjacent canonical card is the
    // bridge to another page; do not use that fallback when it is visible in the wrong direction.
    // From a card in the app bar there is no page to bridge to: it is on every page.
    let bridge = !state.bar_cards.contains(&current);
    let target = geometric.or_else(|| {
        let target = sequence_target(order, current, dir).filter(|_| bridge)?;
        (!cards.iter().any(|(key, _)| *key == target)).then_some(target)
    })?;

    enter(state, spots, target);
    // A card the cursor moved to that this frame did not draw is on another page; the caller asks
    // for it, and the parameter settles when it arrives.
    (!cards.iter().any(|(key, _)| *key == target)).then_some(target)
}

fn enter(state: &mut State, spots: &[Spot], card: u64) {
    state.cursor.card = Some(card);
    state.moved = true;
    let resumed = state
        .remembered
        .get(&card)
        .filter(|key| spots.iter().any(|s| s.card == card && s.key == **key))
        .cloned();
    // Else the card's top-left parameter, as it is read, rather than whichever drew first.
    state.cursor.key = resumed.or_else(|| {
        spots
            .iter()
            .filter(|s| s.card == card)
            .min_by(|a, b| {
                (a.rect.top(), a.rect.left())
                    .partial_cmp(&(b.rect.top(), b.rect.left()))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.key.clone())
    });
    state.cell = 0;
    if let Some(key) = state.cursor.key.clone() {
        state.remembered.insert(card, key);
    }
}

/// The keyboard language's bare arrow: parameter to parameter **inside the card**, as in a synth
/// editor; COARSE + an arrow goes card to card. ← → stay on the parameter's row and stop at its
/// end, ↑ ↓ go to the nearest row that way and the parameter most in line there. (The owner,
/// 2026-10-07, of the first pilot, whose arrows went to the nearest parameter on any card: "There
/// is no logic to the order ... sometimes the lr keys jumps up or down to cards.")
fn move_any(state: &mut State, spots: &[Spot], dir: Dir) {
    let (Some(card), Some(key)) = (state.cursor.card, state.cursor.key.as_deref()) else {
        return;
    };
    let Some(from) = spots
        .iter()
        .find(|spot| spot.card == card && spot.key == key)
    else {
        return;
    };
    // Every stop in the card: a parameter, or each cell of a segmented one (the owner, 2026-10-07:
    // "It fits what I see on the screen"), whose OPEN then presses that cell.
    let stops = spots
        .iter()
        .filter(|spot| spot.card == card)
        .flat_map(|spot| {
            let cells: Vec<Rect> = if spot.cells.len() > 1 {
                spot.cells.clone()
            } else {
                vec![spot.rect]
            };
            cells
                .into_iter()
                .enumerate()
                .map(move |(cell, rect)| (spot, cell, rect))
        });
    let here = if from.cells.len() > 1 {
        from.cells.get(state.cell).copied().unwrap_or(from.rect)
    } else {
        from.rect
    };
    let horizontal = matches!(dir, Dir::Left | Dir::Right);
    let row = |rect: Rect| here.bottom() > rect.top() + 0.5 && rect.bottom() > here.top() + 0.5;
    let current = state.cell;
    let target = directional(
        here,
        stops
            .filter(|(spot, cell, _)| !(spot.key == key && *cell == current))
            .filter(|(_, _, rect)| !horizontal || row(*rect))
            .map(|(spot, cell, rect)| ((spot, cell), rect)),
        dir,
    );
    if let Some((spot, cell)) = target {
        state.settle_cell(spot, cell);
        state.moved = true;
    }
}

fn sequence_target(order: &[u64], current: u64, dir: Dir) -> Option<u64> {
    let at = order.iter().position(|key| *key == current)?;
    let next = if dir.forward() {
        at.checked_add(1).filter(|next| *next < order.len())?
    } else {
        at.checked_sub(1)?
    };
    Some(order[next])
}

/// The nearest target in the requested half-plane.
///
/// Primary-axis distance sorts first, so Down reaches the next row before a better-aligned card
/// several rows away. Perpendicular distance breaks ties within that row or column.
fn directional<T>(from: Rect, candidates: impl Iterator<Item = (T, Rect)>, dir: Dir) -> Option<T> {
    let here = from.center();
    let mut best: Option<((u8, f32, f32), T)> = None;
    for (target, rect) in candidates {
        let there: Pos2 = rect.center();
        let horizontal = matches!(dir, Dir::Left | Dir::Right);
        let (signed_primary, secondary, band) = match dir {
            Dir::Left => (here.x - there.x, (there.y - here.y).abs(), 0.5),
            Dir::Right => (there.x - here.x, (there.y - here.y).abs(), 0.5),
            Dir::Up => (
                here.y - there.y,
                (there.x - here.x).abs(),
                from.height().max(rect.height()) * 0.5,
            ),
            Dir::Down => (
                there.y - here.y,
                (there.x - here.x).abs(),
                from.height().max(rect.height()) * 0.5,
            ),
        };
        if signed_primary <= band {
            continue;
        }
        // Horizontal movement stays in an overlapping visual row when one exists. Without this,
        // a card hundreds of points lower whose centre was one point nearer in x beat the card
        // visibly beside the cursor.
        let same_row = from.bottom() > rect.top() + 0.5 && rect.bottom() > from.top() + 0.5;
        let cost = (u8::from(horizontal && !same_row), signed_primary, secondary);
        if best.as_ref().is_none_or(|(prior, _)| cost < *prior) {
            best = Some((cost, target));
        }
    }
    best.map(|(_, target)| target)
}

/// Runs the cursor over a [`crate::paging::editor`] surface, in the plan's own order.
///
/// Every editor derives the same three things from the last frame's report — the plan's flattened
/// category-first card order, the visible cards' exact rectangles, and the request that brings an
/// off-page card into view — and the pilot's own review caught the one mistake there is to make:
/// walking authored order, which the renderer has already re-sorted. Deriving it in each editor is
/// that mistake once per editor, so it is derived here instead.
///
/// **It needs no card list from the caller**, and asking for one would be ceremony for an
/// unreachable path: a spot is only registered inside a [`card`] scope, which only
/// [`crate::paging::editor::show`] opens, and `show` stores its report at the end of the same
/// frame. So a non-empty registry implies a report, and where there is no report [`run`] has
/// nothing to move anyway.
///
/// `inert` is the paging renderer's own `hold` condition: while the preset browser is open or a
/// value is being typed, the arrows belong to that surface.
pub fn paged(ctx: &egui::Context, state: &mut State, inert: bool) {
    paged_with_bar(ctx, state, inert, &[]);
}

/// [`paged`] for an editor that also draws parameters in the app bar through [`bar_card`].
///
/// The bar cards come first in the cursor's sequence, so a card step off the top of the first card
/// lands on them rather than wrapping to the end of the plan. They use last frame's painted
/// geometry, and they are never a page to turn to: they are always on screen, and asking the
/// renderer for one would request a card it has never heard of.
pub fn paged_with_bar(ctx: &egui::Context, state: &mut State, inert: bool, bar: &[u64]) {
    let (mut order, mut cards) = match crate::paging::editor::report(ctx) {
        Some(report) => (
            report
                .plan
                .pages
                .iter()
                .flat_map(|page| page.cards.iter().map(|key| key.0))
                .collect::<Vec<_>>(),
            report
                .visible
                .into_iter()
                .map(|(key, rect)| (key.0, rect))
                .collect::<Vec<_>>(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    state.bar_cards = bar.to_vec();
    for (index, &key) in bar.iter().enumerate() {
        order.insert(index, key);
        if let Some(rect) = ctx.data(|d| d.get_temp::<Rect>(bar_rect_id(key))) {
            cards.push((key, rect));
        }
    }
    if let Some(wanted) = run(ctx, state, &order, &cards, inert)
        && !bar.contains(&wanted)
    {
        crate::paging::editor::request_card(ctx, crate::paging::Key(wanted));
    }
    outline(ctx, state.card().filter(|_| state.bar.is_none()));
}

/// Tells the renderer which card outline to paint. Set by the editor, read by the paging renderer.
///
/// **The reveal rule is applied here rather than at the call sites**, because there is more than
/// one: [`paged_with_bar`], which [`paged`] delegates to, and any editor that drives [`run`] itself. A rule enforced once per caller is
/// a rule that is one new editor away from being forgotten, and the editor asking for an outline
/// has no business knowing whether the mouse was the last thing touched.
pub fn outline(ctx: &egui::Context, card: Option<u64>) {
    let card = card.filter(|_| shown(ctx));
    ctx.data_mut(|d| match card {
        Some(card) => {
            d.insert_temp(outline_id(), card);
        }
        None => d.remove::<u64>(outline_id()),
    });
}

/// Paints the card cursor, if this is the card it is on.
///
/// **Painted, never laid out**, and drawn outside the card's own rectangle in the same way
/// `control`'s focus ring is drawn outside a control's: a cursor that occupied space would move
/// every card beside it the moment it arrived, which §7.1's no-resize rule forbids.
///
/// Fill *and* border, so it is not hue alone (§15) and stays distinct from a bypassed card's
/// reduced emphasis and from the focus ring inside it.
pub fn paint_card(ui: &Ui, tokens: &Tokens, card: u64, rect: Rect) {
    if ui.ctx().data(|d| d.get_temp::<u64>(outline_id())) != Some(card) {
        return;
    }
    ui.painter().rect_stroke(
        rect.expand(2.0),
        RADIUS as f32 + 2.0,
        Stroke::new(2.0 * HAIRLINE, tokens.accent),
        StrokeKind::Outside,
    );
}

/// The egui key the default keymap puts a job on, if it binds one: what a test presses for the
/// job, so a remap of the default keymap changes no test (`mxm_plugin_test::keyboard_checks`).
pub fn default_key(job: mxm_keys::Job) -> Option<egui::Key> {
    let bound = mxm_keys::Keymap::default().keys(job).next()?;
    egui::Key::ALL
        .iter()
        .copied()
        .find(|key| language::to_key(*key) == Some(bound))
}

/// [`default_key`], for this crate's tests.
#[cfg(test)]
pub(crate) fn key_of(job: mxm_keys::Job) -> egui::Key {
    default_key(job).unwrap_or_else(|| panic!("the default keymap binds {}", job.name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Key;

    // Tests press jobs, on the keys the default keymap gives them (`key_of`).
    const VALUE: mxm_keys::Job = mxm_keys::Job::Verb(mxm_keys::Verb::Value);
    const DUPLICATE: mxm_keys::Job = mxm_keys::Job::Verb(mxm_keys::Verb::Duplicate);
    const COARSE: mxm_keys::Job = mxm_keys::Job::Step(mxm_keys::Step::Coarse);
    const OPEN: mxm_keys::Job = mxm_keys::Job::Action(mxm_keys::Action::Open);
    const VIEW: mxm_keys::Job = mxm_keys::Job::View;

    fn spot(card: u64, key: &str, x: f32, y: f32) -> Spot {
        let id = Id::new((card, key));
        let rect = Rect::from_min_size(Pos2::new(x, y), egui::vec2(40.0, 40.0));
        Spot {
            card,
            key: key.to_owned(),
            rect,
            id,
            focus_ids: vec![id],
            cells: vec![rect],
            pointed: false,
            pointed_cell: 0,
        }
    }

    fn cards(spots: &[Spot]) -> Vec<(u64, Rect)> {
        spots.iter().map(|spot| (spot.card, spot.rect)).collect()
    }

    #[test]
    fn a_bar_card_opens_its_card_scope_and_records_where_it_was_painted() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            bar_card(ui, 64, |ui| {
                assert_eq!(ui.ctx().data(|d| d.get_temp::<u64>(card_id())), Some(64));
                ui.allocate_exact_size(egui::vec2(96.0, 20.0), egui::Sense::hover())
                    .0
            });
            assert_eq!(ui.ctx().data(|d| d.get_temp::<u64>(card_id())), None);
        });
        output.textures_delta.clear();
        assert!(ctx.data(|d| d.get_temp::<Rect>(bar_rect_id(64))).is_some());
        assert!(ctx.data(|d| d.get_temp::<Rect>(bar_rect_id(65))).is_none());
    }

    #[test]
    fn a_multi_cell_parameter_is_one_complete_spot_with_every_focus_id() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            card(ui, 7, |ui| {
                at(ui, "mode", |ui| {
                    mark_unclaimed(
                        ui,
                        Id::new("first"),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(20.0, 20.0)),
                    );
                    mark_unclaimed(
                        ui,
                        Id::new("second"),
                        Rect::from_min_max(Pos2::new(20.0, 0.0), Pos2::new(40.0, 20.0)),
                    );
                });
            });
        });
        output.textures_delta.clear();

        let registered = ctx.data(|data| data.get_temp::<Vec<Spot>>(current_id()).unwrap());
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].rect.width(), 40.0);
        assert_eq!(registered[0].focus_ids.len(), 2);
    }

    #[test]
    fn arrows_follow_painted_direction_instead_of_wrapping_reading_order() {
        let spots = vec![
            spot(0, "a", 0.0, 0.0),
            spot(0, "b", 60.0, 0.0),
            spot(0, "c", 0.0, 100.0),
        ];
        let mut state = State::default();
        state.settle(&spots[0]);

        move_any(&mut state, &spots, Dir::Right);
        assert_eq!(state.parameter(), Some("b"));

        move_any(&mut state, &spots, Dir::Right);
        assert_eq!(
            state.parameter(),
            Some("b"),
            "Right must not wrap to a control painted down and left"
        );

        move_any(&mut state, &spots, Dir::Down);
        assert_eq!(state.parameter(), Some("c"));
        move_any(&mut state, &spots, Dir::Up);
        assert_eq!(state.parameter(), Some("a"));
    }

    /// One frame of an editor: the last frame's spots, `run`, and the outline the renderer reads.
    fn frame(
        ctx: &egui::Context,
        state: &mut State,
        spots: &[Spot],
        input: egui::RawInput,
    ) -> Option<u64> {
        let mut output = ctx.run_ui(input, |ui| {
            ui.ctx()
                .data_mut(|d| d.insert_temp(last_id(), spots.to_vec()));
            let order: Vec<u64> = cards(spots).into_iter().map(|(key, _)| key).collect();
            let _ = run(ui.ctx(), state, &order, &cards(spots), false);
            outline(ui.ctx(), state.card());
        });
        output.textures_delta.clear();
        ctx.data(|d| d.get_temp::<u64>(outline_id()))
    }

    fn press(key: Key, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        }
    }

    fn click(pos: Pos2) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        }
    }

    /// **The owner's report of 2026-09-11**: a panel opened with a card already ringed, and the
    /// ring stayed through a whole session of mouse work. Position and paint are two facts now,
    /// and this asserts both halves of the split at once — nothing is painted until somebody
    /// reaches for the keyboard, and the cursor is nevertheless already placed when they do, so
    /// the first press edits rather than arrives.
    #[test]
    fn no_border_until_the_keyboard_is_used_and_none_after_a_click() {
        let ctx = egui::Context::default();
        let spots = vec![spot(0, "a", 0.0, 0.0), spot(0, "b", 60.0, 0.0)];
        let mut state = State::default();

        assert_eq!(
            frame(&ctx, &mut state, &spots, egui::RawInput::default()),
            None,
            "an editor must not open with a border nobody asked for"
        );
        assert!(!shown(&ctx));
        assert_eq!(
            state.parameter(),
            Some("a"),
            "the cursor is placed even while it is invisible, or the first press is spent arriving"
        );

        assert_eq!(
            frame(
                &ctx,
                &mut state,
                &spots,
                press(Key::ArrowUp, egui::Modifiers::NONE)
            ),
            Some(0),
            "a bare arrow is the language's navigation and reveals the cursor"
        );
        assert!(shown(&ctx));

        // Move it, so what survives the click is a place the cursor was driven to.
        frame(
            &ctx,
            &mut state,
            &spots,
            press(Key::ArrowRight, egui::Modifiers::NONE),
        );
        assert_eq!(state.parameter(), Some("b"));

        assert_eq!(
            frame(&ctx, &mut state, &spots, click(Pos2::new(500.0, 500.0))),
            None,
            "a click anywhere puts the border away"
        );
        assert!(!shown(&ctx));
        assert_eq!(
            state.parameter(),
            Some("b"),
            "hidden is not lost: the mouse user's parameter is still the arrows' target"
        );

        assert_eq!(
            frame(
                &ctx,
                &mut state,
                &spots,
                press(Key::ArrowUp, egui::Modifiers::NONE)
            ),
            Some(0),
            "and the next keypress brings it back where it was"
        );
        assert_eq!(state.parameter(), Some("b"));
    }

    /// `Escape` closes and cancels; it is not navigation, and a panel must not light up because
    /// somebody dismissed a menu.
    #[test]
    fn escape_is_not_a_reveal() {
        let ctx = egui::Context::default();
        let spots = vec![spot(0, "a", 0.0, 0.0)];
        let mut state = State::default();
        frame(&ctx, &mut state, &spots, egui::RawInput::default());
        assert_eq!(
            frame(
                &ctx,
                &mut state,
                &spots,
                press(Key::Escape, egui::Modifiers::NONE)
            ),
            None
        );
        assert!(!shown(&ctx));
    }

    #[test]
    fn a_card_uses_geometry_and_reports_the_adjacent_card_on_another_page() {
        let spots = vec![spot(0, "a", 0.0, 0.0), spot(1, "b", 60.0, 0.0)];
        let visible = cards(&spots);
        let order = [0_u64, 1, 2];
        let mut state = State::default();
        state.settle(&spots[0]);

        assert_eq!(
            move_card(&mut state, &spots, &order, &visible, Dir::Right),
            None
        );
        assert_eq!(state.card(), Some(1));

        let show = move_card(&mut state, &spots, &order, &visible, Dir::Right);
        assert_eq!(
            show,
            Some(2),
            "a card this frame did not draw must be asked for"
        );
        assert_eq!(state.card(), Some(2));
    }

    #[test]
    fn down_chooses_the_next_row_before_a_better_aligned_later_row() {
        let spots = vec![
            spot(0, "a", 0.0, 0.0),
            spot(1, "b", 80.0, 100.0),
            spot(2, "c", 0.0, 200.0),
        ];
        let visible = cards(&spots);
        let mut state = State::default();
        state.settle(&spots[0]);

        move_card(&mut state, &spots, &[0, 1, 2], &visible, Dir::Down);
        assert_eq!(state.card(), Some(1), "the immediately adjacent row wins");
    }

    /// Leaving a card and coming back resumes where it was left, so a keystroke is never spent
    /// re-aiming at the control just used.
    #[test]
    fn a_card_remembers_the_parameter_it_was_left_on() {
        let spots = vec![
            spot(0, "a", 0.0, 0.0),
            spot(0, "b", 60.0, 0.0),
            spot(1, "c", 120.0, 0.0),
        ];
        let visible = vec![(0, spots[0].rect.union(spots[1].rect)), (1, spots[2].rect)];
        let order = [0_u64, 1];
        let mut state = State::default();
        state.settle(&spots[0]);
        move_any(&mut state, &spots, Dir::Right);
        assert_eq!(state.parameter(), Some("b"));

        move_card(&mut state, &spots, &order, &visible, Dir::Right);
        assert_eq!(state.parameter(), Some("c"));

        move_card(&mut state, &spots, &order, &visible, Dir::Left);
        assert_eq!(
            state.parameter(),
            Some("b"),
            "resumed, not reset to the first"
        );
    }

    /// [`frame`] with the editor's own `inert` condition, for a frame another surface owns.
    fn frame_inert(ctx: &egui::Context, state: &mut State, spots: &[Spot], input: egui::RawInput) {
        let mut output = ctx.run_ui(input, |ui| {
            ui.ctx()
                .data_mut(|d| d.insert_temp(last_id(), spots.to_vec()));
            let order: Vec<u64> = cards(spots).into_iter().map(|(key, _)| key).collect();
            let _ = run(ui.ctx(), state, &order, &cards(spots), true);
        });
        output.textures_delta.clear();
    }

    fn pointed(mut spot: Spot) -> Spot {
        spot.pointed = true;
        spot
    }

    /// **The owner's report of 2026-09-23**: a knob turned with the mouse did not become the
    /// arrows' target, because egui never focuses a painted control for a click and the cursor
    /// only followed focus. The pointer now moves the cursor itself, and hands the control focus
    /// as a cursor move would.
    #[test]
    fn a_pointer_press_moves_the_cursor_and_hands_the_control_focus() {
        let ctx = egui::Context::default();
        let (a, b) = (spot(0, "a", 0.0, 0.0), spot(0, "b", 60.0, 0.0));
        let mut state = State::default();
        frame(
            &ctx,
            &mut state,
            &[a.clone(), b.clone()],
            egui::RawInput::default(),
        );
        assert_eq!(state.parameter(), Some("a"));

        frame(
            &ctx,
            &mut state,
            &[a.clone(), pointed(b.clone())],
            egui::RawInput::default(),
        );
        assert_eq!(state.parameter(), Some("b"), "the pointer chose b");
        assert_eq!(ctx.memory(|m| m.focused()), Some(b.id));
        assert!(!shown(&ctx), "the pointer does not reveal the cursor");
    }

    /// A drag does not take focus from the knob the cursor was on — egui only surrenders focus
    /// on a click — so following focus in the same frame would undo the pointer's move.
    #[test]
    fn a_drag_is_not_undone_by_the_control_that_still_holds_focus() {
        let ctx = egui::Context::default();
        let (a, b) = (spot(0, "a", 0.0, 0.0), spot(0, "b", 60.0, 0.0));
        let mut state = State::default();
        frame(
            &ctx,
            &mut state,
            &[a.clone(), b.clone()],
            egui::RawInput::default(),
        );
        ctx.memory_mut(|m| m.request_focus(a.id));

        frame(
            &ctx,
            &mut state,
            &[a.clone(), pointed(b.clone())],
            egui::RawInput::default(),
        );
        assert_eq!(state.parameter(), Some("b"));
    }

    /// A press whose next frame is inert — a value opened for typing, the preset browser in front —
    /// still moves the cursor, and the focus it owes arrives on the first frame that is free.
    #[test]
    fn a_pointer_press_survives_an_inert_frame_and_its_focus_arrives_after() {
        let ctx = egui::Context::default();
        let (a, b) = (spot(0, "a", 0.0, 0.0), spot(0, "b", 60.0, 0.0));
        let mut state = State::default();
        frame(
            &ctx,
            &mut state,
            &[a.clone(), b.clone()],
            egui::RawInput::default(),
        );

        frame_inert(
            &ctx,
            &mut state,
            &[a.clone(), pointed(b.clone())],
            egui::RawInput::default(),
        );
        assert_eq!(state.parameter(), Some("b"), "position moves while inert");
        assert_ne!(ctx.memory(|m| m.focused()), Some(b.id), "but focus waits");

        frame(
            &ctx,
            &mut state,
            &[a.clone(), b.clone()],
            egui::RawInput::default(),
        );
        assert_eq!(ctx.memory(|m| m.focused()), Some(b.id), "and arrives");
    }

    /// The first frame's press lands where it was aimed, not on the first control drawn.
    #[test]
    fn a_pointer_press_on_the_first_frame_is_where_the_cursor_lands() {
        let ctx = egui::Context::default();
        let (a, b) = (spot(0, "a", 0.0, 0.0), spot(0, "b", 60.0, 0.0));
        let mut state = State::default();
        frame(
            &ctx,
            &mut state,
            &[a, pointed(b)],
            egui::RawInput::default(),
        );
        assert_eq!(state.parameter(), Some("b"));
    }

    /// An open menu walks its own rows with the arrows, and a search field takes the arrows and the
    /// letters as text: while either is up the cursor neither moves nor reveals.
    #[test]
    fn an_open_popup_keeps_the_keyboard_from_the_cursor() {
        let ctx = egui::Context::default();
        let spots = vec![spot(0, "a", 0.0, 0.0), spot(0, "b", 60.0, 0.0)];
        let mut state = State::default();
        frame(&ctx, &mut state, &spots, egui::RawInput::default());
        egui::Popup::open_id(&ctx, Id::new("a menu"));

        frame(
            &ctx,
            &mut state,
            &spots,
            press(Key::ArrowRight, egui::Modifiers::NONE),
        );
        assert_eq!(state.parameter(), Some("a"), "the menu's keys are its own");
        assert!(!shown(&ctx), "and the cursor does not appear behind it");

        egui::Popup::close_all(&ctx);
        frame(
            &ctx,
            &mut state,
            &spots,
            press(Key::ArrowRight, egui::Modifiers::NONE),
        );
        assert_eq!(
            state.parameter(),
            Some("b"),
            "closed, the cursor has them back"
        );
    }

    #[test]
    fn a_focused_text_field_keeps_the_keyboard_from_the_cursor() {
        let ctx = egui::Context::default();
        let spots = vec![spot(0, "a", 0.0, 0.0), spot(0, "b", 60.0, 0.0)];
        let mut state = State::default();
        let mut text = String::new();
        // Draw a text field and give it focus, then keep drawing it so it stays focused.
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let field = ui.text_edit_singleline(&mut text);
                field.request_focus();
            });
            output.textures_delta.clear();
        }
        assert!(ctx.text_edit_focused());

        let mut output = ctx.run_ui(press(Key::ArrowRight, egui::Modifiers::NONE), |ui| {
            ui.ctx()
                .data_mut(|d| d.insert_temp(last_id(), spots.clone()));
            let order: Vec<u64> = cards(&spots).into_iter().map(|(key, _)| key).collect();
            let _ = run(ui.ctx(), &mut state, &order, &cards(&spots), false);
            let _ = ui.text_edit_singleline(&mut text);
        });
        output.textures_delta.clear();
        assert_ne!(state.parameter(), Some("b"), "the field's keys are its own");
        assert!(!shown(&ctx));
    }

    /// **The keyboard language**: VIEW + ↑ leaves the cards for the bar
    /// above them, where OPEN presses the widget the cursor is on, and VIEW + ↓ comes back.
    #[test]
    fn under_the_language_view_reaches_the_bar_where_open_presses() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        let mut pressed = false;
        let tap = |key| {
            [true, false].map(|down| egui::Event::Key {
                key,
                physical_key: None,
                pressed: down,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        };
        let frame = |state: &mut State, events: Vec<egui::Event>, pressed: &mut bool| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let cards = [(
                    0,
                    Rect::from_min_size(Pos2::new(0.0, 60.0), egui::vec2(400.0, 240.0)),
                )];
                let _ = run(ui.ctx(), state, &[0], &cards, false);
                egui::Panel::top("a bar").show(ui, |ui| {
                    bar(ui);
                    if ui.button("Presets").clicked() {
                        *pressed = true;
                    }
                });
                egui::CentralPanel::default().show(ui, |ui| {
                    card(ui, 0, |ui| {
                        at(ui, "a", |ui| {
                            let response = ui.button("a");
                            mark(ui, &response, response.rect);
                        });
                    });
                });
            });
            output.textures_delta.clear();
        };
        for _ in 0..3 {
            frame(&mut state, Vec::new(), &mut pressed);
        }
        let view_up: Vec<_> = tap(key_of(VIEW))
            .into_iter()
            .chain(tap(Key::ArrowUp))
            .collect();
        frame(&mut state, view_up, &mut pressed);
        assert_eq!(state.bar(), Some(0), "VIEW + up is the bar");
        frame(&mut state, Vec::new(), &mut pressed);
        frame(&mut state, tap(key_of(OPEN)).to_vec(), &mut pressed);
        assert!(pressed, "OPEN pressed the bar's button");
        let view_down: Vec<_> = tap(key_of(VIEW))
            .into_iter()
            .chain(tap(Key::ArrowDown))
            .collect();
        frame(&mut state, view_down, &mut pressed);
        assert_eq!(state.bar(), None, "VIEW + down is the cards again");
    }

    /// **OPEN while a verb is armed ends its gesture, keeping it, and opens nothing**, as in
    /// newDAWn (the owner, 2026-10-08): its Enter isn't left for a control to start typing a
    /// value. With nothing armed, Enter is left for the control again.
    #[test]
    fn under_the_language_open_keeps_an_armed_edit_and_types_nothing() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let read_with = |state: &mut State, events: Vec<egui::Event>| {
            let mut left = false;
            let mut keys = ValueKeys::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    let _ = language::read(ui.ctx(), state, false);
                    left = ui.input(|input| {
                        input.events.iter().any(|event| {
                            matches!(
                                event,
                                egui::Event::Key {
                                    key,
                                    pressed: true,
                                    ..
                                } if *key == key_of(OPEN)
                            )
                        })
                    });
                    keys = take_value_keys(ui.ctx());
                },
            );
            output.textures_delta.clear();
            (left, keys)
        };
        // VALUE, →: armed and stepped. Enter keeps the edit, and no control sees it.
        let value_right = vec![
            key(key_of(VALUE), true),
            key(key_of(VALUE), false),
            key(Key::ArrowRight, true),
            key(Key::ArrowRight, false),
        ];
        let _ = read_with(&mut state, value_right);
        let enter = || vec![key(key_of(OPEN), true), key(key_of(OPEN), false)];
        let (left, keys) = read_with(&mut state, enter());
        assert!(keys.keep, "the edit is kept");
        assert!(!left, "no value is typed");
        let (left, _) = read_with(&mut state, enter());
        assert!(left, "with nothing armed, Enter is the control's");
    }

    /// **DUPLICATE has nothing to copy in an editor**: a press that arms it is ended at once, so
    /// the arrows after it move the cursor, and a value edit it ends is kept, not cancelled.
    #[test]
    fn under_the_language_duplicate_ends_at_once_and_keeps_a_value_edit() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let read_with = |state: &mut State, events: Vec<egui::Event>| {
            let mut keys = ValueKeys::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    let _ = language::read(ui.ctx(), state, false);
                    keys = take_value_keys(ui.ctx());
                },
            );
            output.textures_delta.clear();
            keys
        };
        // VALUE, →, then DUPLICATE: the edit is kept, nothing cancelled,
        // and nothing is left armed.
        let _ = read_with(
            &mut state,
            vec![
                key(key_of(VALUE), true),
                key(key_of(VALUE), false),
                key(Key::ArrowRight, true),
                key(Key::ArrowRight, false),
            ],
        );
        let keys = read_with(
            &mut state,
            vec![key(key_of(DUPLICATE), true), key(key_of(DUPLICATE), false)],
        );
        assert!(keys.keep && !keys.cancel, "the value edit is kept");
        assert_eq!(
            language::engine(&mut state).arrows(),
            mxm_keys::Arrows::Navigate,
            "nothing is left armed"
        );
    }

    /// **The keyboard language's bare arrows keep to the card**, and ← → to the row: at the end
    /// of either they stop, and COARSE + an arrow goes to the next card.
    #[test]
    fn under_the_language_the_arrows_keep_to_the_card_and_the_row() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        // Card 1: a row of two, and one below them; card 2 to the right, level with the first row.
        let spots = vec![
            spot(1, "a", 0.0, 0.0),
            spot(1, "b", 50.0, 0.0),
            spot(1, "c", 0.0, 60.0),
            spot(2, "d", 120.0, 0.0),
            spot(2, "e", 120.0, 70.0),
        ];
        let cards = |state: &State| (state.card(), state.parameter().map(str::to_owned));
        frame(&ctx, &mut state, &spots, egui::RawInput::default());
        assert_eq!(cards(&state), (Some(1), Some("a".into())));
        let arrow = |key| press(key, egui::Modifiers::NONE);
        frame(&ctx, &mut state, &spots, arrow(Key::ArrowRight));
        assert_eq!(cards(&state), (Some(1), Some("b".into())));
        frame(&ctx, &mut state, &spots, arrow(Key::ArrowRight));
        assert_eq!(
            cards(&state),
            (Some(1), Some("b".into())),
            "the row ends; the card doesn't change"
        );
        frame(&ctx, &mut state, &spots, arrow(Key::ArrowDown));
        assert_eq!(
            cards(&state),
            (Some(1), Some("c".into())),
            "down is the next row, most in line"
        );
        frame(&ctx, &mut state, &spots, arrow(Key::ArrowRight));
        assert_eq!(
            cards(&state),
            (Some(1), Some("c".into())),
            "right never drops to another row"
        );
        frame(&ctx, &mut state, &spots, arrow(Key::ArrowDown));
        assert_eq!(cards(&state), (Some(1), Some("c".into())), "the card ends");
        // COARSE + right is the next card.
        frame(
            &ctx,
            &mut state,
            &spots,
            egui::RawInput {
                events: vec![
                    egui::Event::Key {
                        key: key_of(COARSE),
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::Key {
                        key: key_of(COARSE),
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::Key {
                        key: Key::ArrowRight,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
        );
        assert_eq!(state.card(), Some(2), "COARSE + right is the next card");
    }

    /// **The page changes under the cursor** (a tab chosen, with the keys or the mouse): the
    /// cursor goes to the first card of the page shown, and COARSE + an arrow starts from there.
    #[test]
    fn under_the_language_a_new_page_takes_the_cursor_to_its_first_card() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        let first_page = vec![spot(1, "a", 0.0, 0.0), spot(2, "b", 100.0, 0.0)];
        let second_page = vec![spot(3, "c", 0.0, 0.0), spot(4, "d", 100.0, 0.0)];
        let order = [1, 2, 3, 4];
        let page = |ctx: &egui::Context, state: &mut State, spots: &[Spot], input| {
            let mut output = ctx.run_ui(input, |ui| {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(last_id(), spots.to_vec()));
                let _ = run(ui.ctx(), state, &order, &cards(spots), false);
            });
            output.textures_delta.clear();
        };
        page(&ctx, &mut state, &first_page, egui::RawInput::default());
        assert_eq!(state.card(), Some(1));
        // The second page is shown: the cursor follows it.
        page(&ctx, &mut state, &second_page, egui::RawInput::default());
        assert_eq!((state.card(), state.parameter()), (Some(3), Some("c")));
        // COARSE + right from there is the next card on this page.
        let coarse_right = egui::RawInput {
            events: [
                (key_of(COARSE), true),
                (key_of(COARSE), false),
                (Key::ArrowRight, true),
            ]
            .into_iter()
            .map(|(key, pressed)| egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect(),
            ..Default::default()
        };
        page(&ctx, &mut state, &second_page, coarse_right);
        assert_eq!(state.card(), Some(4));
    }
}
