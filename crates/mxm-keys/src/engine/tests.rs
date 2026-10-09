//! One test per rule, named after it. The keys are the shipped `notes` keymap's:
//!
//! ```text
//! W MOVE      E EXTENT    R VALUE         Tab OUT      Escape BACK
//! A ADD       S SELECT    D DUPLICATE     F or Shift VIEW
//! ```
//!
//! `take` leaves out [`Output::Begin`], which a verb pressed always says; the tests of that rule
//! read everything with `take_all`.
//!
//! ```text
//! Z .         X COARSE    C FINE    V MICRO
//! ```

use super::*;
use crate::key::Direction::{Down, Left, Right, Up};
use crate::keymap::SHIPPED;

/// A keyboard under test: every input a little later than the last, every output kept.
struct Board {
    engine: Engine,
    at: Duration,
    out: Vec<Output>,
}

impl Board {
    fn new() -> Board {
        Board::with(Keymap::shipped("notes").unwrap())
    }

    fn with(keymap: Keymap) -> Board {
        Board {
            engine: Engine::new(keymap),
            at: Duration::ZERO,
            out: Vec::new(),
        }
    }

    /// The `notes` keymap with its settings taken out and `text` added.
    fn keymap(text: &str) -> Board {
        let settings = ["tap-arms", "one-arrow", "timeout"];
        let notes: String = notes_text()
            .lines()
            .filter(|line| !settings.iter().any(|setting| line.starts_with(setting)))
            .map(|line| format!("{line}\n"))
            .collect();
        Board::with(Keymap::parse(&format!("{notes}{text}")).unwrap())
    }

    fn press_with(&mut self, key: Key, mods: Mods) -> &mut Board {
        self.at += Duration::from_millis(10);
        let out = self.engine.press(key, mods, self.at);
        self.out.extend(out);
        self
    }

    fn press(&mut self, key: Key) -> &mut Board {
        self.press_with(key, Mods::NONE)
    }

    fn release(&mut self, key: Key) -> &mut Board {
        let out = self.engine.release(key);
        self.out.extend(out);
        self
    }

    /// Press and release each key in turn: one key down at a time.
    fn tap(&mut self, keys: &[Key]) -> &mut Board {
        for &key in keys {
            self.press(key).release(key);
        }
        self
    }

    fn at(&mut self, seconds: f64) -> &mut Board {
        self.at = Duration::from_secs_f64(seconds);
        self
    }

    fn poll(&mut self) -> &mut Board {
        let out = self.engine.poll(self.at);
        self.out.extend(out);
        self
    }

    /// Every output since the last take, but `Begin`.
    fn take(&mut self) -> Vec<Output> {
        let mut out = self.take_all();
        out.retain(|output| !matches!(output, Output::Begin { .. }));
        out
    }

    fn take_all(&mut self) -> Vec<Output> {
        std::mem::take(&mut self.out)
    }
}

fn notes_text() -> &'static str {
    SHIPPED.iter().find(|(stem, _)| *stem == "notes").unwrap().1
}

fn step(verb: Verb, step: Step, direction: Direction) -> Output {
    Output::Step {
        verb,
        step,
        direction,
    }
}

fn nav(direction: Direction) -> Output {
    Output::Navigate {
        direction,
        coarse: false,
    }
}

fn coarse_nav(direction: Direction) -> Output {
    Output::Navigate {
        direction,
        coarse: true,
    }
}

use crate::keymap::Step::{Coarse, Fine, Micro};
use crate::keymap::Verb::{Duplicate, Extent, Move, Select, Value};
use Key::{Escape, Tab};

#[test]
fn arrows_alone_only_move_the_focus() {
    let mut board = Board::new();
    board.tap(&[Key::Right, Key::Up]);
    assert_eq!(board.take(), [nav(Right), nav(Up)]);
    assert_eq!(board.engine.arrows(), Arrows::Navigate);
}

#[test]
fn held_and_tapped_give_the_same_outputs() {
    let mut held = Board::new();
    held.press(Key::W).press(Key::X).tap(&[Key::Right]);
    held.release(Key::X).release(Key::W);
    let mut tapped = Board::new();
    tapped.tap(&[Key::W, Key::X, Key::Right, Tab]);
    let both = [step(Move, Coarse, Right), Output::Finish];
    assert_eq!(held.take(), both);
    assert_eq!(tapped.take(), both);
}

#[test]
fn the_keys_can_be_held_and_tapped_in_any_mix_after_the_verb() {
    // Tap MOVE, then hold FINE and press →.
    let mut board = Board::new();
    board
        .tap(&[Key::W])
        .press(Key::C)
        .tap(&[Key::Right])
        .release(Key::C);
    board.tap(&[Tab]);
    assert_eq!(board.take(), [step(Move, Fine, Right), Output::Finish]);
}

#[test]
fn no_step_size_means_fine() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right, Tab]);
    assert_eq!(board.take(), [step(Move, Fine, Right), Output::Finish]);
}

#[test]
fn a_step_size_stays_until_another_is_pressed() {
    // keyboard.md's example: VALUE, COARSE, ↑, ↑, FINE, ↑, OUT.
    let mut board = Board::new();
    board.tap(&[Key::R, Key::X, Key::Up, Key::Up, Key::C, Key::Up, Tab]);
    assert_eq!(
        board.take(),
        [
            step(Value, Coarse, Up),
            step(Value, Coarse, Up),
            step(Value, Fine, Up),
            Output::Finish
        ]
    );
    // Held, letting go of COARSE doesn't take it back either.
    board
        .press(Key::R)
        .press(Key::X)
        .tap(&[Key::Up])
        .release(Key::X);
    board.tap(&[Key::Up]).release(Key::R);
    assert_eq!(
        board.take(),
        [
            step(Value, Coarse, Up),
            step(Value, Coarse, Up),
            Output::Finish
        ]
    );
}

#[test]
fn a_new_gesture_starts_fine() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::X, Key::Right, Tab, Key::W, Key::Right, Tab]);
    assert_eq!(
        board.take(),
        [
            step(Move, Coarse, Right),
            Output::Finish,
            step(Move, Fine, Right),
            Output::Finish
        ]
    );
}

#[test]
fn an_armed_verb_stays_armed() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right, Key::Right, Key::Right]);
    assert_eq!(board.take(), [step(Move, Fine, Right); 3]);
    assert_eq!(
        board.engine.arrows(),
        Arrows::Edit {
            verb: Move,
            step: Fine
        }
    );
}

#[test]
fn out_finishes_and_keeps_and_pressed_again_does_nothing() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right, Tab]);
    assert_eq!(board.take(), [step(Move, Fine, Right), Output::Finish]);
    board.tap(&[Tab, Tab, Key::Right]);
    assert_eq!(board.take(), [nav(Right)]);
}

#[test]
fn tapping_the_verb_again_finishes() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right, Key::W, Key::Right]);
    assert_eq!(
        board.take(),
        [step(Move, Fine, Right), Output::Finish, nav(Right)]
    );
}

#[test]
fn the_next_command_finishes_and_goes_on() {
    // keyboard.md's example: SELECT, →, →, MOVE, → selects two more items, then moves them.
    let mut board = Board::new();
    board.tap(&[Key::S, Key::Right, Key::Right, Key::W, Key::Right]);
    assert_eq!(
        board.take(),
        [
            step(Select, Fine, Right),
            step(Select, Fine, Right),
            Output::Finish,
            step(Move, Fine, Right)
        ]
    );
}

#[test]
fn an_action_finishes_the_gesture_then_does_its_job() {
    let mut board = Board::new();
    board.tap(&[Key::E, Key::Right, Key::A]);
    assert_eq!(
        board.take(),
        [
            step(Extent, Fine, Right),
            Output::Finish,
            Output::Action(Action::Add)
        ]
    );
}

#[test]
fn the_quick_gesture_in_sequence() {
    // keyboard.md: DUPLICATE, COARSE, ↑, FINE, →, OUT, six taps, one key down at a time.
    let mut board = Board::new();
    board.tap(&[Key::D, Key::X, Key::Up, Key::C, Key::Right, Tab]);
    assert_eq!(
        board.take(),
        [
            step(Duplicate, Coarse, Up),
            step(Duplicate, Fine, Right),
            Output::Finish
        ]
    );
}

#[test]
fn raising_one_note_then_the_next() {
    // keyboard.md: MOVE, ↑, OUT, →, MOVE, ↑, OUT.
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Up, Tab, Key::Right, Key::W, Key::Up, Tab]);
    assert_eq!(
        board.take(),
        [
            step(Move, Fine, Up),
            Output::Finish,
            nav(Right),
            step(Move, Fine, Up),
            Output::Finish
        ]
    );
}

#[test]
fn shift_with_add_takes_away() {
    let mut board = Board::new();
    board.press_with(Key::A, Mods::SHIFT).release(Key::A);
    board.tap(&[Key::A]);
    assert_eq!(
        board.take(),
        [Output::Action(Action::Remove), Output::Action(Action::Add)]
    );
}

#[test]
fn back_cancels_the_whole_gesture() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right, Key::Right, Escape, Key::Right]);
    assert_eq!(
        board.take(),
        [
            step(Move, Fine, Right),
            step(Move, Fine, Right),
            Output::Cancel,
            nav(Right)
        ]
    );
}

#[test]
fn back_with_nothing_armed_goes_back_a_level() {
    let mut board = Board::new();
    board.tap(&[Escape]);
    assert_eq!(board.take(), [Output::Back]);
}

#[test]
fn command_z_cancels_a_gesture_and_is_otherwise_undo() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right]);
    board.press_with(Key::Z, Mods::COMMAND).release(Key::Z);
    assert_eq!(board.take(), [step(Move, Fine, Right), Output::Cancel]);
    let undo = Output::Raw {
        key: Key::Z,
        mods: Mods::COMMAND,
    };
    board.press_with(Key::Z, Mods::COMMAND).release(Key::Z);
    assert_eq!(board.take(), [undo]);
    // An armed verb that hasn't stepped has nothing to revert, so the undo is the panel's.
    board.tap(&[Key::W]);
    board.press_with(Key::Z, Mods::COMMAND).release(Key::Z);
    assert_eq!(board.take(), [undo]);
    assert_eq!(board.engine.arrows(), Arrows::Navigate);
}

#[test]
fn letting_go_of_a_held_verb_ends_it_like_out() {
    let mut board = Board::new();
    board.press(Key::W).tap(&[Key::Right]).release(Key::W);
    board.tap(&[Key::Right]);
    assert_eq!(
        board.take(),
        [step(Move, Fine, Right), Output::Finish, nav(Right)]
    );
}

#[test]
fn changing_your_mind_arms_the_verb() {
    let mut board = Board::new();
    board.press(Key::W).release(Key::W);
    assert_eq!(
        board.engine.arrows(),
        Arrows::Edit {
            verb: Move,
            step: Fine
        }
    );
    board.tap(&[Tab]);
    assert_eq!(board.engine.arrows(), Arrows::Navigate);
}

#[test]
fn a_gesture_without_steps_ends_without_a_word() {
    let mut board = Board::new();
    board.tap(&[Key::W, Tab, Key::W, Escape, Key::W, Key::W]);
    assert_eq!(board.take(), []);
    assert_eq!(board.engine.arrows(), Arrows::Navigate);
}

#[test]
fn coarse_held_goes_one_level_up() {
    let mut board = Board::new();
    board
        .press(Key::X)
        .tap(&[Key::Up, Key::Down])
        .release(Key::X);
    board.tap(&[Key::Up]);
    assert_eq!(board.take(), [coarse_nav(Up), coarse_nav(Down), nav(Up)]);
}

#[test]
fn a_tapped_coarse_lasts_one_arrow() {
    let mut board = Board::new();
    board.tap(&[Key::X]);
    assert_eq!(board.engine.arrows(), Arrows::Coarse);
    board.tap(&[Key::Right, Key::Right]);
    assert_eq!(board.take(), [coarse_nav(Right), nav(Right)]);
}

#[test]
fn back_or_out_forgets_a_tapped_coarse() {
    let mut board = Board::new();
    board.tap(&[Key::X, Escape, Key::X, Tab, Key::Left]);
    assert_eq!(board.take(), [nav(Left)]);
}

#[test]
fn the_other_steps_alone_do_nothing() {
    let mut board = Board::new();
    board.tap(&[Key::C, Key::Left]);
    assert_eq!(board.take(), [nav(Left)]);
    // FINE forgets a tapped MICRO.
    board.tap(&[Key::V, Key::C, Key::Left]);
    assert_eq!(board.take(), [nav(Left)]);
}

#[test]
fn micro_alone_goes_one_level_down_held_or_tapped() {
    let mut board = Board::new();
    board.press(Key::V).tap(&[Key::Left]).release(Key::V);
    assert_eq!(board.take(), [Output::Within { direction: Left }]);
    board.tap(&[Key::V]);
    assert_eq!(board.engine.arrows(), Arrows::Micro);
    board.tap(&[Key::Right, Key::Right]);
    assert_eq!(
        board.take(),
        [Output::Within { direction: Right }, nav(Right)]
    );
    // Held with a verb, it's the verb's step, as before.
    board.press(Key::V).press(Key::W).tap(&[Key::Right]);
    board.release(Key::W).release(Key::V);
    assert_eq!(board.take(), [step(Move, Micro, Right), Output::Finish]);
}

#[test]
fn a_step_held_before_the_verb_counts() {
    // Held like the M8, the order the keys go down in doesn't matter.
    let mut board = Board::new();
    board.press(Key::X).press(Key::W).tap(&[Key::Right]);
    board.release(Key::W).release(Key::X);
    board.tap(&[Key::Right]);
    assert_eq!(
        board.take(),
        [step(Move, Coarse, Right), Output::Finish, nav(Right)]
    );
}

#[test]
fn view_on_shift_moves_between_views_and_finishes_the_gesture_first() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right]);
    board
        .press_with(Key::Right, Mods::SHIFT)
        .release(Key::Right);
    assert_eq!(
        board.take(),
        [
            step(Move, Fine, Right),
            Output::Finish,
            Output::View { direction: Right }
        ]
    );
}

#[test]
fn view_on_a_key_is_held_or_tapped_like_coarse() {
    let mut board =
        Board::with(Keymap::parse(&notes_text().replace("view = F, Shift", "view = F")).unwrap());
    board.press(Key::F).tap(&[Key::Right]).release(Key::F);
    board.tap(&[Key::F, Key::Left, Key::Left]);
    assert_eq!(
        board.take(),
        [
            Output::View { direction: Right },
            Output::View { direction: Left },
            nav(Left)
        ]
    );
    // Without VIEW on Shift, Shift + an arrow is the panel's.
    board.press_with(Key::Up, Mods::SHIFT).release(Key::Up);
    assert_eq!(
        board.take(),
        [Output::Raw {
            key: Key::Up,
            mods: Mods::SHIFT
        }]
    );
}

#[test]
fn other_keys_go_to_the_panel_and_finish_the_gesture_first() {
    let mut board = Board::new();
    board.tap(&[Key::K]);
    assert_eq!(
        board.take(),
        [Output::Raw {
            key: Key::K,
            mods: Mods::NONE
        }]
    );
    board.tap(&[Key::W, Key::Right, Key::Space]);
    assert_eq!(
        board.take(),
        [
            step(Move, Fine, Right),
            Output::Finish,
            Output::Raw {
                key: Key::Space,
                mods: Mods::NONE
            }
        ]
    );
}

#[test]
fn standard_shortcuts_reach_the_panel_in_every_shipped_keymap() {
    let redo = Mods {
        shift: true,
        ..Mods::COMMAND
    };
    for (stem, text) in SHIPPED {
        let mut board = Board::with(Keymap::parse(text).unwrap());
        for key in [Key::Z, Key::X, Key::C, Key::V, Key::S, Key::A, Key::Y] {
            for mods in [Mods::COMMAND, redo] {
                board.press_with(key, mods).release(key);
                assert_eq!(board.take(), [Output::Raw { key, mods }], "{stem}");
            }
        }
    }
}

#[test]
fn the_systems_repeat_is_ignored_but_a_held_arrow_repeats() {
    let mut board = Board::new();
    board.press(Key::W).press(Key::W);
    board
        .press(Key::Right)
        .press(Key::Right)
        .release(Key::Right);
    board.release(Key::W);
    board.press(Key::A).press(Key::A).release(Key::A);
    assert_eq!(
        board.take(),
        [
            step(Move, Fine, Right),
            step(Move, Fine, Right),
            Output::Finish,
            Output::Action(Action::Add)
        ]
    );
}

#[test]
fn a_mouse_click_finishes_and_keeps() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right]);
    let out = board.engine.interrupt();
    assert_eq!(*out, [Output::Finish]);
    assert_eq!(board.engine.arrows(), Arrows::Navigate);
}

#[test]
fn losing_the_keyboard_forgets_the_keys_held() {
    let mut board = Board::new();
    board.press(Key::W).tap(&[Key::Right]);
    let out = board.engine.lost_focus();
    assert_eq!(*out, [Output::Finish]);
    // W's release never came; pressed again, it is a new press, not the system's repeat.
    board.take();
    board.press(Key::W).tap(&[Key::Right]).release(Key::W);
    assert_eq!(board.take(), [step(Move, Fine, Right), Output::Finish]);
}

#[test]
fn switching_keymaps_finishes_first() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right]);
    let home_row = Keymap::shipped("home-row").unwrap();
    let out = board.engine.set_keymap(home_row);
    assert_eq!(*out, [Output::Finish]);
    board.take();
    board.tap(&[Key::S, Key::Left, Tab]);
    assert_eq!(board.take(), [step(Move, Fine, Left), Output::Finish]);
}

#[test]
fn with_tap_arms_off_only_a_held_verb_works() {
    let mut board = Board::keymap("tap-arms = no");
    board.tap(&[Key::W, Key::Right, Key::X, Key::Right]);
    assert_eq!(board.take(), [nav(Right), nav(Right)]);
    board.press(Key::W).tap(&[Key::Right]).release(Key::W);
    assert_eq!(board.take(), [step(Move, Fine, Right), Output::Finish]);
}

#[test]
fn with_one_arrow_a_tapped_verb_ends_after_one_step() {
    let mut board = Board::keymap("one-arrow = yes");
    board.tap(&[Key::W, Key::Right, Key::Right]);
    assert_eq!(
        board.take(),
        [step(Move, Fine, Right), Output::Finish, nav(Right)]
    );
    // Held, the verb lasts as long as it's held.
    board
        .press(Key::W)
        .tap(&[Key::Right, Key::Right])
        .release(Key::W);
    assert_eq!(
        board.take(),
        [
            step(Move, Fine, Right),
            step(Move, Fine, Right),
            Output::Finish
        ]
    );
}

#[test]
fn a_timeout_finishes_a_tapped_verb_left_waiting() {
    let mut board = Board::keymap("timeout = 2");
    board.at(0.0).tap(&[Key::W]).at(1.0).tap(&[Key::Right]);
    board.at(2.9).poll();
    assert_eq!(board.take(), [step(Move, Fine, Right)]);
    board.at(3.1).poll();
    assert_eq!(board.take(), [Output::Finish]);
    board.tap(&[Key::Right]);
    assert_eq!(board.take(), [nav(Right)]);
    // A held verb isn't waiting.
    board
        .at(10.0)
        .press(Key::W)
        .at(20.0)
        .poll()
        .tap(&[Key::Right]);
    assert_eq!(board.take(), [step(Move, Fine, Right)]);
}

#[test]
fn arming_a_verb_is_said_after_the_end_of_the_gesture_it_ends() {
    let mut board = Board::new();
    board.tap(&[Key::W, Key::Right, Key::E]);
    assert_eq!(
        board.take_all(),
        [
            Output::Begin { verb: Move },
            step(Move, Fine, Right),
            Output::Finish,
            Output::Begin { verb: Extent }
        ]
    );
}

#[test]
fn duplicate_is_a_change_from_the_start_so_its_end_is_always_said() {
    let mut board = Board::new();
    // OUT, the key again, OPEN and BACK, with no arrow taken.
    board.tap(&[Key::D, Tab]);
    assert_eq!(
        board.take_all(),
        [Output::Begin { verb: Duplicate }, Output::Finish]
    );
    board.tap(&[Key::D, Key::D]);
    assert_eq!(
        board.take_all(),
        [Output::Begin { verb: Duplicate }, Output::Finish]
    );
    board.tap(&[Key::D, Key::Enter]);
    assert_eq!(
        board.take_all(),
        [
            Output::Begin { verb: Duplicate },
            Output::Finish,
            Output::Action(Action::Open)
        ]
    );
    board.tap(&[Key::D, Key::Escape]);
    assert_eq!(
        board.take_all(),
        [Output::Begin { verb: Duplicate }, Output::Cancel]
    );
    // Other verbs still end without a word when nothing was stepped.
    board.tap(&[Key::W, Tab]);
    assert_eq!(board.take_all(), [Output::Begin { verb: Move }]);
}

#[test]
fn a_held_duplicate_let_go_after_an_arrow_ends_and_with_none_stays_armed() {
    let mut board = Board::new();
    board.press(Key::D).tap(&[Key::Right]).release(Key::D);
    assert_eq!(board.take(), [step(Duplicate, Fine, Right), Output::Finish]);
    // Held and let go with no arrow: a tap, still armed.
    board.press(Key::D).release(Key::D);
    assert_eq!(board.take(), []);
    board.tap(&[Tab]);
    assert_eq!(board.take(), [Output::Finish]);
}

#[test]
fn duplicate_ends_under_the_settings_as_any_verb_with_its_end_said() {
    // A timeout ends a tapped DUPLICATE left waiting, as OUT does.
    let mut board = Board::keymap("timeout = 2");
    board.at(0.0).tap(&[Key::D]).at(3.0).poll();
    assert_eq!(board.take(), [Output::Finish]);
    // One arrow moves it and ends it.
    let mut board = Board::keymap("one-arrow = yes");
    board.tap(&[Key::D, Key::Right]);
    assert_eq!(board.take(), [step(Duplicate, Fine, Right), Output::Finish]);
    // With taps not arming, it works while held, and letting go ends it, arrow or not.
    let mut board = Board::keymap("tap-arms = no");
    board.press(Key::D).release(Key::D);
    assert_eq!(board.take(), [Output::Finish]);
}

#[test]
fn there_is_no_timeout_unless_one_is_set() {
    let mut board = Board::new();
    board.tap(&[Key::W]).at(1000.0).poll();
    assert_eq!(
        board.engine.arrows(),
        Arrows::Edit {
            verb: Move,
            step: Fine
        }
    );
}

#[test]
fn on_a_value_a_step_key_is_value_at_its_size() {
    let mut board = Board::new();
    board.engine.set_on_value(true);
    // Held: COARSE down, ↑ ↑, up; the gesture ends as a held verb's does.
    board.press(Key::X).tap(&[Key::Up, Key::Up]).release(Key::X);
    assert_eq!(
        board.take_all(),
        [
            Output::Begin { verb: Value },
            step(Value, Coarse, Up),
            step(Value, Coarse, Up),
            Output::Finish
        ]
    );
    // Tapped: MICRO, ←, OUT keeps; ← → are the value's too, for the host to snap.
    board.tap(&[Key::V, Key::Left, Tab]);
    assert_eq!(
        board.take_all(),
        [
            Output::Begin { verb: Value },
            step(Value, Micro, Left),
            Output::Finish
        ]
    );
    // Tapped again, the step key finishes it, as a verb's does; BACK cancels.
    board.tap(&[Key::C, Key::Up, Key::C]);
    assert_eq!(board.take(), [step(Value, Fine, Up), Output::Finish]);
    board.tap(&[Key::C, Key::Down, Escape]);
    assert_eq!(board.take(), [step(Value, Fine, Down), Output::Cancel]);
    // Another step key changes the size, the gesture going on.
    board.tap(&[Key::C, Key::Up, Key::X, Key::Up, Tab]);
    assert_eq!(
        board.take(),
        [
            step(Value, Fine, Up),
            step(Value, Coarse, Up),
            Output::Finish
        ]
    );
}

#[test]
fn off_a_value_the_step_keys_move_as_before() {
    let mut board = Board::new();
    board.engine.set_on_value(true);
    board.engine.set_on_value(false);
    board.tap(&[Key::X, Key::Right, Key::V, Key::Left, Key::C, Key::Up]);
    assert_eq!(
        board.take_all(),
        [
            coarse_nav(Right),
            Output::Within { direction: Left },
            nav(Up)
        ]
    );
}
