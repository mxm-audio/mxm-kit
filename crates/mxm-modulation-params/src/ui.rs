//! A target's routes, drawn as the owner asked: the live ones stacked, and `‹ modulate ›` beneath.
//!
//! `plans/plan-modulation-routing.md` §8a, in the private archive. The rows are derived from
//! **parameter values alone** — a present pair is a row, and the sources not yet present are what
//! the menu offers — so a preset fully determines what the panel shows and there is no editor-only
//! routing state to invalidate a layout cache on.
//!
//! # Why the source is a label and not a menu
//!
//! Under the owner's ruling a target has one slot per source, so a row *is* its source and there is
//! nothing to re-point. That is what removes a whole class of stored-value hazard, and it is why
//! a row names its source rather than offering `‹ LFO ›`.
//!
//! # The target said once, each row only its source
//!
//! The target's own line, *Cutoff ‹ modulate ›*, is the title of the box its routes are drawn in, in
//! the body style and primary ink, and each row inside is named by its source alone, in the caption
//! style and secondary ink: *LFO*, *Envelope*, *Key* (the owner, 2026-09-24, piloted on mxm-mono-08
//! and every editor's since). With nothing routed the line stands alone, unboxed; with every source
//! routed it keeps the name without the menu. Every row used to repeat the whole *Cutoff from LFO*,
//! and those long names — *Glide speed from Mod oscillator* — were what set the width of every card
//! that carried a route, and what ran into the reading beside them. The full name is still each
//! row's accessible name and tooltip, and what a host shows.
//!
//! # A route slider has a minimum
//!
//! Short names would otherwise let a card shrink until its slider is too short to aim (the owner,
//! 2026-09-24). [`TRACK_MIN`] holds every row's track at six pointer targets, and a row given less
//! runs past its card's edge, which is what an editor's floor measurement finds. A row's reading
//! never draws over its source (`mxm_ui::control::slider`).
//!
//! # What the group encloses
//!
//! **The target's line and the rows.** A target with nothing routed draws no border at all — an
//! empty box is a frame around nothing, and it read as a control that had failed to load rather than
//! as an invitation.
//!
//! # Removing a route
//!
//! Each row ends in a **remove**, not a switch. A row exists *because* its pair is present, so a
//! toggle reading `On` beside it said what the row already said and spent fifty points saying it —
//! the owner's finding on the pilot. `mxm_ui::control::remove_mark` is what a presence looks like
//! when its own visibility is the state: the cross alone, unframed, because the slider it follows
//! is already a framed thing and a second frame reads as another item rather than as an action on
//! the one before it. It is bottom-aligned onto the slider's track — centred in the row, it floated
//! above the track, because a slider is a name/value line stacked over the track it names.
//!
//! It clears that pair's presence — **one parameter write**, with the amount left alone so re-adding
//! restores the depth. [`crate::remove`] is the single place that happens.

use egui::Ui;
use mxm_preset::{ErasedParam, StepLaw};
use mxm_ui::control::{self, ControlOutcome, Next, NextValue, ParamView, Press, Steps, Wheel};
use mxm_ui::space::{MIN_TARGET, SPACE_2};

/// The shortest a route's slider track is drawn: six pointer targets, enough travel to aim a depth, so a card holding a route is never narrower than this
/// plus the remove beside it.
pub const TRACK_MIN: f32 = 6.0 * MIN_TARGET;
use mxm_ui::theme::Tokens;
use nice_plug::prelude::ParamSetter;

use crate::Route;

/// What a frame of [`stack`] did, so the caller can react without re-reading every parameter.
#[derive(Default, Debug, Clone, Copy)]
pub struct StackOutcome {
    /// A route was added or removed, so the card's height changed and a paging cache is stale.
    ///
    /// **Not an invalidation request.** Presence is a parameter, so the paging cache's revision
    /// already moves on its own; this is for a caller that wants to know, not one that must be told.
    pub topology_changed: bool,
    /// Some amount was edited this frame.
    pub amount_changed: bool,
}

/// Draws one target's routes and the affordance that adds another.
///
/// # Two names, for the same reason a knob has two
///
/// `target` is the destination's **canonical** name — what the parameter is called, what a host's
/// automation list shows, what a screen reader announces. `panel` is what is **painted**, and it
/// may drop a prefix the card around it already carries: *"Pitch"* inside a card titled
/// *Oscillator 2*, where the parameter is *"Oscillator 2 pitch"*. That is design system §7.1's rule for
/// `panel_label`, and a routing group needs it more than a knob does: its title is
/// `<panel> from`, and a module-qualified target there is width every card with a route pays.
/// `mxm-mono-00` paid a hundred points a card for the long form.
///
/// A caller with nothing to drop passes the same string twice. **`panel` is not an alias**: it
/// never changes with the source, and the canonical name is what accessibility and tooltips read.
///
/// `target` names the destination — *"Cutoff"*. The group opens with *"Cutoff from"* and each row
/// under it reads only its source — *"LFO"* — one line per route, which is the whole of the layout
/// budget argument. **The target names the menu too**, on its own line beside `‹ modulate ›`: a card
/// carrying two stacks would otherwise offer two identical unlabelled menus, and the player could
/// not tell which control either one reached before using it.
///
/// It sizes itself from the `ui` it is given rather than taking a width, because the group's own
/// margin is not knowable to the caller and a row that guesses it runs its removal toggle past the
/// card's edge. `text_entry` is the editor's shared type-a-value buffer, as every other control
/// takes.
pub fn stack(
    ui: &mut Ui,
    tokens: &Tokens,
    target: &str,
    panel: &str,
    routes: &[Route<'_>],
    text_entry: &mut Option<String>,
    setter: &ParamSetter<'_>,
) -> StackOutcome {
    stack_with_law(
        ui,
        tokens,
        target,
        panel,
        routes,
        text_entry,
        setter,
        &|_| StepLaw::Own,
    )
}

/// [`stack`], with the keyboard stepping each route's amount by a law its plugin declares.
///
/// **A second entry point rather than a changed one**, as `control::segmented_beside` is: most
/// targets are not a pitch and pass nothing. A target that is — mxm-mono-08's pitch routes, read
/// in octaves — hands each route its [`StepLaw`] here, computed from the same reach its reading
/// multiplies by, so an arrow lands on a semitone or an octave rather than a hundredth of the
/// range. Per route rather than per target, because a route's reach is its *(target, source)*
/// pair's.
#[allow(clippy::too_many_arguments)]
pub fn stack_with_law(
    ui: &mut Ui,
    tokens: &Tokens,
    target: &str,
    panel: &str,
    routes: &[Route<'_>],
    text_entry: &mut Option<String>,
    setter: &ParamSetter<'_>,
    law: &dyn Fn(&Route<'_>) -> StepLaw,
) -> StackOutcome {
    let mut outcome = StackOutcome::default();

    // Read before the rows are drawn, because a removal inside the loop changes the answer and the
    // spacing below has to match the border that was actually painted.
    let enclosed = routes.iter().any(Route::is_present);
    // The menu offers exactly the sources this target does not yet have. When every source is
    // present there is nothing to add and the affordance is absent rather than empty.
    let absent: Vec<&Route<'_>> = routes.iter().filter(|r| !r.is_present()).collect();

    // **The target's line is the group's one title** (the owner, 2026-09-24). The line naming the
    // target and offering `‹ modulate ›` is the title of the box its routes are drawn in, and each
    // row under it is named by its source alone. It used to sit under the box while the box carried
    // a title of its own — *Level from* over *Envelope*, and *Level ‹ modulate ›* beneath — two
    // titles for one thing.
    if enclosed {
        mxm_ui::shell::group(ui, tokens, |ui| {
            if target_line(ui, tokens, target, panel, &absent, setter) {
                outcome.topology_changed = true;
            }
            for route in routes.iter().filter(|r| r.is_present()) {
                if row(ui, tokens, target, route, law(route), text_entry, setter) {
                    outcome.amount_changed = true;
                }
            }
        });
    } else if !absent.is_empty() && target_line(ui, tokens, target, panel, &absent, setter) {
        outcome.topology_changed = true;
    }
    outcome
}

/// The target's line: its painted name, and `‹ modulate ›` offering the sources it does not yet
/// carry. With every source present there is nothing to offer and the name stands alone, in the
/// same style and height so the line does not move. Returns whether a source was added.
///
/// It is the title of the box its routes sit in, so it is the stronger word — the body style and
/// primary ink — and each source under it the lesser (the owner, 2026-09-24: the weights were
/// *"backwards"*).
fn target_line(
    ui: &mut Ui,
    tokens: &Tokens,
    target: &str,
    panel: &str,
    absent: &[&Route<'_>],
    setter: &ParamSetter<'_>,
) -> bool {
    if absent.is_empty() {
        ui.horizontal(|ui| {
            ui.set_min_height(MIN_TARGET);
            ui.label(egui::RichText::new(panel).color(tokens.text_primary));
        });
        return false;
    }
    let labels: Vec<&str> = std::iter::once("modulate")
        .chain(absent.iter().map(|r| r.source))
        .collect();
    let mut chosen = 0usize;
    let description = format!("Add a modulation source to {target}");
    let changed = control::selector(
        ui,
        tokens,
        panel,
        &labels,
        &mut chosen,
        None,
        Some(0),
        &description,
    );
    if changed && chosen > 0 {
        crate::add(absent[chosen - 1], setter);
        return true;
    }
    false
}

/// What [`stack`] occupies in `ui`, without drawing it — for `mxm_ui::tree`. `x` is the narrowest
/// it may be given, `y` its height as the patch stands.
///
/// **The narrowest is the stack with every route revealed**: the widest row any of its sources
/// could draw, present or not, at its widest reading, inside the group's inset — or the target's
/// line, if that is wider. A route is one click away, and a card that widened when one was added
/// would reflow the panel under the pointer. The height is what is drawn now. A row is named by its
/// source in the caption style, its reading beside the name and never over it, over a track of at
/// least [`TRACK_MIN`]; the target's line is the group's first row.
#[must_use]
pub fn stack_size(ui: &Ui, panel: &str, routes: &[Route<'_>]) -> egui::Vec2 {
    let spacing = ui.spacing().item_spacing;
    let inset = 2.0 * mxm_ui::tree::GROUP_INSET;
    let row = |route: &Route<'_>| {
        let widest = control::widest_value(|n| route.amount.format(n as f32));
        let slider = control::slider_size(ui, route.source, true, &widest);
        egui::Vec2::new(
            slider.x.max(TRACK_MIN) + spacing.x + control::REMOVE_SIZE,
            slider.y.max(control::REMOVE_SIZE),
        )
    };
    let line = control::selector_size(ui, panel, &["modulate"]);
    let widest_row = routes.iter().map(|r| row(r).x).fold(0.0, f32::max);
    let width = (widest_row.max(line.x) + inset).max(line.x);

    let rows: Vec<f32> = routes
        .iter()
        .filter(|route| route.is_present())
        .map(|route| row(route).y)
        .collect();
    let absent = rows.len() < routes.len();
    let rows_height = rows.iter().sum::<f32>() + SPACE_2 * rows.len().saturating_sub(1) as f32;
    let height = if !rows.is_empty() {
        // The target's line opens the group, `SPACE_2` above the first row.
        inset + line.y + SPACE_2 + rows_height
    } else if absent {
        line.y
    } else {
        0.0
    };
    egui::Vec2::new(width, height)
}

/// One live route: its amount, painted as its source and named in full for accessibility and the
/// host, with a remove beside it.
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    tokens: &Tokens,
    target: &str,
    route: &Route<'_>,
    law: StepLaw,
    text_entry: &mut Option<String>,
    setter: &ParamSetter<'_>,
) -> bool {
    let stepper = Amount {
        amount: route.amount,
        law,
    };
    let name = format!("{target} from {}", route.source);
    // The group's title above the rows already says `<panel>`, so the row paints its source alone.
    let label = route.source;
    let text = route.amount.text();
    let (fine_up, fine_down, coarse_up, coarse_down) = route.amount.stepping();
    let view = ParamView {
        name: &name,
        label,
        text: &text,
        // A route amount is drawn stacked in a card, never inline in the app bar.
        widest: "",
        description: &format!(
            "How much {} moves {target}. Signed: below the centre inverts it.",
            route.source
        ),
        default: f64::from(route.amount.default_normalised()),
        // Every route amount is signed, so the centre is "no modulation" and a half-filled track
        // would read as "half on" when it means "off".
        bipolar: true,
        read_only: false,
        modulation: f64::from(route.amount.modulation()),
        marked: route.amount.modulation() != 0.0,
        steps: Steps {
            fine_up,
            fine_down,
            coarse_up,
            coarse_down,
        },
        // A route amount keeps the parameter's own step unless its plugin declared a law: its
        // reading is a scale of the target's unit that only the plugin's routes table knows, so
        // only the plugin can say a semitone is a twelfth of its octave. mxm-mono-08's pitch
        // routes do (the owner, 2026-09-23); the rest keep the owner's earlier exclusion.
        next: (law != StepLaw::Own).then_some(Next(&stepper)),
        // The row sits under its target's title, so its source is the lesser word.
        quiet_label: true,
    };

    let mut normalised = f64::from(route.amount.normalised());
    let mut removed = false;
    let marked = route.present.modulation() != 0.0;

    let outcome: ControlOutcome = ui
        .horizontal(|ui| {
            // **The remove's width is reserved before the slider takes the rest.** A slider lays
            // its value out right-aligned across the whole width it is given, so left to itself it
            // claims the entire row and pushes the button past the card's border — which is what it
            // did.
            let gap = ui.spacing().item_spacing.x;
            let track = (ui.available_width() - control::REMOVE_SIZE - gap).max(TRACK_MIN);

            let out = ui
                .scope(|ui| {
                    // The cap as well as the track's own width: the name line above the track is
                    // laid out against the ui's width rather than against the number passed in.
                    ui.set_max_width(track);
                    mxm_ui::navigation::at(ui, route.amount_id, |ui| {
                        control::slider(
                            ui,
                            tokens,
                            &view,
                            &mut normalised,
                            track,
                            text_entry,
                            Wheel::FocusOrModifier,
                        )
                    })
                })
                .inner;

            // The presence parameter, drawn as what it is: the row's own existence. Clearing it
            // removes the route, which is one parameter write and leaves the amount alone. It
            // carries `marked` for the same reason every other control does — presence is
            // automatable and per-step modulatable, so a host moving it must not leave the control
            // looking inert.
            // **Bottom-aligned, so the cross lands on the track rather than floating above it.** A
            // slider stacks its name/value line over its track, so it is taller than the cross
            // beside it, and a row centres what it holds: the cross sat half a name line high. The
            // track is the last `REMOVE_SIZE` of the slider's column and the cross is allocated the
            // same square, so giving the cross the row's full height and laying it out bottom-up
            // puts the two boxes exactly on each other.
            //
            // The height is measured rather than derived: it is the name line plus the track plus
            // the gap between them, and the first of those is whatever the font says it is. The
            // slider is already placed by this point, so `min_rect` is that answer. Not
            // `ui.with_layout`, which takes *all* the remaining height as its initial size — with a
            // bottom cross-alignment that pushed the row to the bottom of the scroll area and
            // measured a card 16 560 points tall.
            let row = ui.min_rect().height().max(control::REMOVE_SIZE);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(control::REMOVE_SIZE, row),
                egui::Layout::bottom_up(egui::Align::Center),
                |ui| {
                    mxm_ui::navigation::at(ui, route.present_id, |ui| {
                        if control::remove_mark(
                            ui,
                            tokens,
                            control::REMOVE_SIZE,
                            true,
                            &format!("Remove {} from {target}", route.source),
                            marked,
                            "The depth stays, so adding the source back restores it.",
                        ) {
                            crate::remove(route, setter);
                            removed = true;
                        }
                    });
                },
            );
            out
        })
        .inner;

    if removed {
        return false;
    }
    if outcome.gesture_started {
        route.amount.begin(setter);
    }
    if outcome.changed {
        route.amount.set(setter, normalised as f32);
    }
    if outcome.gesture_ended {
        route.amount.end(setter);
    }
    outcome.changed
}

/// A route amount's keyboard law, asked once per press. See [`stack_with_law`].
struct Amount<'a> {
    amount: &'a dyn ErasedParam,
    law: StepLaw,
}

impl NextValue for Amount<'_> {
    fn next_value(&self, normalised: f64, press: Press) -> f64 {
        self.amount.step_from(normalised, press, self.law)
    }
}
