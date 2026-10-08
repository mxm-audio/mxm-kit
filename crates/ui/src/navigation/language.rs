//! The keyboard language (newDAWn's `docs/keyboard.md`, the collection's law since 2026-10-06), as
//! the cursor reads it in every editor (piloted on mxm-mono-08, rolled out 2026-10-08). The owner
//! decided on 2026-10-07 that the editors convert to it: **the keys work in newDAWn and the
//! collection, other hosts are secondary** and fall back to the mouse where they keep a key from
//! the window.
//!
//! - The arrows go to the next parameter that way inside the card (← → along its row), stopping
//!   on each cell of a segmented control; COARSE + arrows to the next card. VIEW + arrows (and
//!   `Shift` + arrows in the default keymap) leave the cards for the bars above them (the view
//!   bar, then the app bar) and come back, never out of the window: an editor is its own window,
//!   and moving between windows is the window manager's.
//! - VALUE + arrows change the parameter the cursor is on: FINE (or no step key) its fine step,
//!   COARSE and MUSICAL its coarse one, MICRO the finer layer. The change is one gesture, ended by
//!   OUT, by letting go of a held VALUE or by the next command, and BACK cancels it.
//! - DELETE (and RIPPLE) put the parameter back to its default. OPEN (Enter) and BACK (Escape)
//!   stay in egui's queue too, for typing a value and closing what is open; Home and End and every
//!   chord with `Command` or `Alt` are left for the controls and the host. OPEN while a verb is
//!   armed ends its gesture, keeping it, and opens nothing, as in newDAWn (the owner, 2026-10-08:
//!   "if enter is not the end of the selection, what is?"): its Enter stays out of egui's queue.
//! - DUPLICATE, a verb with nothing to copy in an editor, is ended as soon as it's armed, keeping
//!   and cancelling nothing; a value edit it ends is kept, as the next command keeps it.
//! - In a bar, the cursor walks its widgets with [`crate::reach`]: OPEN presses one.

use std::time::Duration;

use egui::{Context, Key as E};
use mxm_keys::{
    Action, Arrows, Direction, Engine, Job, Key, Keymap, Mods, Output, Step as Size, Verb,
};

use super::{Dir, State, Step, ValueKeys, bars};
use crate::control::Press;
use crate::reach;

/// The physical key egui reports, as the language names it.
pub(super) fn to_key(key: E) -> Option<Key> {
    Some(match key {
        E::A => Key::A,
        E::B => Key::B,
        E::C => Key::C,
        E::D => Key::D,
        E::E => Key::E,
        E::F => Key::F,
        E::G => Key::G,
        E::H => Key::H,
        E::I => Key::I,
        E::J => Key::J,
        E::K => Key::K,
        E::L => Key::L,
        E::M => Key::M,
        E::N => Key::N,
        E::O => Key::O,
        E::P => Key::P,
        E::Q => Key::Q,
        E::R => Key::R,
        E::S => Key::S,
        E::T => Key::T,
        E::U => Key::U,
        E::V => Key::V,
        E::W => Key::W,
        E::X => Key::X,
        E::Y => Key::Y,
        E::Z => Key::Z,
        E::ArrowUp => Key::Up,
        E::ArrowDown => Key::Down,
        E::ArrowLeft => Key::Left,
        E::ArrowRight => Key::Right,
        E::Tab => Key::Tab,
        E::Escape => Key::Escape,
        E::Enter => Key::Enter,
        E::Backspace => Key::Backspace,
        E::Delete => Key::Delete,
        _ => return None,
    })
}

fn dir(direction: Direction) -> Dir {
    match direction {
        Direction::Left => Dir::Left,
        Direction::Right => Dir::Right,
        Direction::Up => Dir::Up,
        Direction::Down => Dir::Down,
    }
}

/// Whether a key's events stay in egui's queue as well: OPEN and BACK, for typing a value and
/// closing a menu, which the controls and egui already do.
fn shared(engine: &Engine, key: Key) -> bool {
    matches!(
        engine.keymap().job(key),
        Some(Job::Action(Action::Open) | Job::Back)
    )
}

/// The language's engine, made with the default keymap the first time it is asked for.
pub(super) fn engine(state: &mut State) -> &mut Engine {
    state
        .language
        .get_or_insert_with(|| Engine::new(Keymap::default()))
}

/// Whether the keymap puts BACK on `Escape`, which egui itself uses to end a mouse drag.
pub(super) fn escape_is_back(state: &mut State) -> bool {
    engine(state).keymap().job(Key::Escape) == Some(Job::Back)
}

/// The language's keys this frame, read before any control is drawn: the moves for the cursor,
/// and the value keys published for the parameter it is on.
///
/// `back_spent`: this frame's BACK already cancelled a mouse drag ([`escape_is_back`] and
/// `crate::drag::notice_escape`, before the inert return), so it does nothing else.
pub(super) fn read(ctx: &Context, state: &mut State, back_spent: bool) -> Vec<Step> {
    let mut back_spent = back_spent;
    let engine = state
        .language
        .get_or_insert_with(|| Engine::new(Keymap::default()));
    let at = Duration::from_secs_f64(ctx.input(|input| input.time).max(0.0));
    let mut outputs = Vec::new();
    let mut taken = false;
    ctx.input_mut(|input| {
        if input.pointer.any_pressed() {
            outputs.extend(engine.interrupt());
        }
        input.events.retain(|event| {
            let egui::Event::Key {
                key,
                physical_key,
                pressed,
                modifiers,
                ..
            } = event
            else {
                return true;
            };
            // Chords with Command or Alt are the controls' and the host's.
            if modifiers.command || modifiers.alt {
                return true;
            }
            let Some(key) = to_key(physical_key.unwrap_or(*key)) else {
                return true;
            };
            let ours = key.direction().is_some() || engine.keymap().job(key).is_some();
            if !ours {
                return true;
            }
            let mods = Mods {
                shift: modifiers.shift,
                alt: false,
                command: false,
            };
            // OPEN with a verb armed: the engine ends the gesture, keeping it; nothing opens.
            let armed_open = *pressed
                && engine.keymap().job(key) == Some(Job::Action(Action::Open))
                && matches!(engine.arrows(), Arrows::Edit { .. });
            if *pressed {
                let read = engine.press(key, mods, at);
                outputs.extend(read.into_iter().filter(|output| {
                    !(armed_open && *output == Output::Action(Action::Open))
                        && !matches!(output, Output::Begin { .. })
                }));
                // An editor has nothing to duplicate: a press that arms DUPLICATE is ended at
                // once, and the end that makes is dropped, so it keeps or cancels nothing.
                if matches!(
                    engine.arrows(),
                    Arrows::Edit {
                        verb: Verb::Duplicate,
                        ..
                    }
                ) {
                    let _ = engine.interrupt();
                }
            } else {
                outputs.extend(engine.release(key));
            }
            let keep = shared(engine, key) && !armed_open;
            taken |= !keep;
            keep
        });
    });
    // Tab and the arrows are the language's here: egui has already read them for moving its own
    // focus, which would take the cursor off its parameter (OUT is Tab), so that's undone.
    if taken {
        ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
    }
    outputs.extend(engine.poll(at));
    // Any of the language's keys reveal the cursor but BACK alone: it closes and cancels, and a
    // panel must not light up because somebody dismissed a menu (`escape_is_not_a_reveal`).
    if outputs
        .iter()
        .any(|output| !matches!(output, Output::Cancel | Output::Back))
    {
        super::reveal(ctx);
    }

    let mut steps = Vec::new();
    let mut keys = ValueKeys::default();
    for output in outputs {
        // BACK that ended a mouse drag, or ends one now, is spent on it.
        if matches!(output, Output::Cancel | Output::Back)
            && (std::mem::take(&mut back_spent) || crate::drag::cancel(ctx))
        {
            continue;
        }
        // VIEW moves between the cards and the bars above them.
        if let Output::View { direction } = output {
            view(ctx, state, direction);
            continue;
        }
        if let Some(bar) = state.bar {
            let ui = bars(ctx).get(bar).map(|&(ui, _)| ui);
            if let Some(ui) = ui {
                state.reach.key(bar, reach::Region::Inside(ui), output);
            }
            continue;
        }
        match output {
            Output::Navigate {
                direction,
                coarse: true,
            } => steps.push(Step::Card(dir(direction))),
            Output::Navigate { direction, .. } => steps.push(Step::Any(dir(direction))),
            Output::Step {
                verb: Verb::Value,
                step,
                direction,
            } => keys.presses.push(Press {
                up: matches!(direction, Direction::Up | Direction::Right),
                coarse: matches!(step, Size::Coarse | Size::Musical),
                finer: step == Size::Micro,
            }),
            Output::Finish => keys.keep = true,
            Output::Cancel => keys.cancel = true,
            Output::Action(Action::Delete | Action::Ripple) => keys.reset = true,
            _ => {}
        }
    }
    if keys != ValueKeys::default() {
        super::publish_value_keys(ctx, keys);
    }
    steps
}

/// VIEW + ↑ from the cards goes to the lowest bar above them, and on up; ↓ comes back down, and
/// from the lowest bar to the cards. Sideways, and past the top, there is nowhere to go.
fn view(ctx: &Context, state: &mut State, direction: Direction) {
    let count = bars(ctx).len();
    let before = state.bar;
    state.bar = match (direction, state.bar) {
        (Direction::Up, None) if count > 0 => Some(count - 1),
        (Direction::Up, Some(bar)) => Some(bar.saturating_sub(1)),
        (Direction::Down, Some(bar)) if bar + 1 < count => Some(bar + 1),
        (Direction::Down, Some(_)) => None,
        (_, bar) => bar,
    };
    match (before, state.bar) {
        // Into the bars: the parameter the cursor leaves gives up egui's focus, so its ring goes.
        (None, Some(_)) => ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        }),
        // Back to the cards: the cursor's parameter takes the focus again, on the page shown.
        (Some(_), None) => {
            state.home = true;
            state.moved = true;
        }
        _ => {}
    }
}
