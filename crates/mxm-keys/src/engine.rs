//! The gesture engine: physical keys in, finished gestures out.
//!
//! Every gesture can be pressed two ways at once, and they give the same outputs:
//!
//! - **held**: hold MOVE, hold COARSE, press →;
//! - **in sequence**: tap MOVE, tap COARSE, press →. A tapped verb stays *armed* until the gesture
//!   ends, so only one key is ever down.
//!
//! The rules, which the tests name one by one:
//!
//! - Arrows on their own only move the focus. COARSE with them goes one level up, and VIEW to the
//!   neighbouring view. Tapped, either lasts for one arrow.
//! - The verb comes first. With no step size it steps FINE, and a step size stays until another
//!   one is pressed. A step key held when the verb is pressed counts, so a chord can be pressed in
//!   any order.
//! - A gesture ends with OUT, by tapping its verb again, or with the next command, which then does
//!   its own job; letting go of a held verb ends it the same way. BACK, or `Command+Z`, cancels it.
//! - Arming a verb is said, [`Output::Begin`], after the end of the gesture it ends, so the order
//!   tells which gesture each end belongs to.
//! - A gesture's steps are one change: one [`Output::Finish`] or [`Output::Cancel`] follows them.
//!   A gesture with no steps ends without a word, but DUPLICATE's: it is a change from the start,
//!   the copy, so its end is always said.
//! - Shift with ADD's key is REMOVE: ADD gives, Shift + ADD takes away.
//! - A key held down is pressed once: the operating system's repeat is ignored, except on the
//!   arrows, where holding one repeats it.
//!
//! The engine never knows what a gesture changes. The host turns its outputs into commands for the
//! focused item, and any key that isn't the language's goes to the panel as [`Output::Raw`].

use std::ops::Deref;
use std::time::Duration;

use crate::key::{Direction, Key, Mods};
use crate::keymap::{Action, Job, Keymap, Modifier, Step, Verb};

/// What a key press, a release or the passing of time means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Move the focus to the nearest item that way; `coarse`, one structural level up.
    Navigate { direction: Direction, coarse: bool },
    /// MICRO with bare arrows: one level down, within the focused item. In an editor, its start
    /// edge (←) or its end edge (→), which MOVE then moves on its own.
    Within { direction: Direction },
    /// Move the focus to the neighbouring view.
    View { direction: Direction },
    /// A verb was armed: a gesture begins. It comes after the end of the gesture it ends.
    Begin { verb: Verb },
    /// One arrow press of a gesture.
    Step {
        verb: Verb,
        step: Step,
        direction: Direction,
    },
    /// The gesture is over: keep what its steps changed, as one undo step.
    Finish,
    /// The gesture is over: revert everything its steps changed.
    Cancel,
    /// A job done at once, on the focused item.
    Action(Action),
    /// BACK with no gesture to cancel: go back a level.
    Back,
    /// A key that isn't the language's, such as a standard shortcut. The focused panel may use it.
    Raw { key: Key, mods: Mods },
}

/// The outputs of one input, in order. There are never more than three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outputs {
    items: [Output; 3],
    len: usize,
}

impl Outputs {
    fn new() -> Outputs {
        Outputs {
            items: [Output::Finish; 3],
            len: 0,
        }
    }

    fn push(&mut self, output: Output) {
        self.items[self.len] = output;
        self.len += 1;
    }
}

impl Deref for Outputs {
    type Target = [Output];

    fn deref(&self) -> &[Output] {
        &self.items[..self.len]
    }
}

impl IntoIterator for Outputs {
    type Item = Output;
    type IntoIter = std::iter::Take<std::array::IntoIter<Output, 3>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter().take(self.len)
    }
}

/// What the arrows do now: what the arrow map shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrows {
    Navigate,
    /// One structural level up.
    Coarse,
    /// One level down, within the focused item.
    Micro,
    View,
    /// A gesture's verb and step, *MOVE · COARSE*.
    Edit {
        verb: Verb,
        step: Step,
    },
}

#[derive(Clone, Copy, Debug)]
struct Gesture {
    verb: Verb,
    key: Key,
    step: Step,
    /// The verb's key is down.
    held: bool,
    /// An arrow was pressed while the verb's key was down, so letting go ends the gesture.
    used_while_held: bool,
    /// A step was sent, so the end of the gesture is said; DUPLICATE's from the start.
    stepped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    Coarse,
    Micro,
    View,
}

/// COARSE, MICRO or VIEW without a verb, on a key: held for its arrows, or tapped for the next
/// one.
#[derive(Clone, Copy, Debug)]
struct Navigation {
    layer: Layer,
    key: Key,
    held: bool,
    used_while_held: bool,
}

/// The gesture engine. See the [module](self) for the rules.
#[derive(Clone, Debug)]
pub struct Engine {
    keymap: Keymap,
    /// The keys down, one bit each by [`Key::index`].
    down: u128,
    gesture: Option<Gesture>,
    navigation: Option<Navigation>,
    /// The step key pressed last, which a verb pressed while it's still down starts with.
    last_step: Option<(Key, Step)>,
    /// A key whose press ended a gesture, so its release does nothing more.
    spent: Option<Key>,
    /// When the last key was pressed, for the timeout.
    last_press: Option<Duration>,
}

impl Engine {
    pub fn new(keymap: Keymap) -> Engine {
        Engine {
            keymap,
            down: 0,
            gesture: None,
            navigation: None,
            last_step: None,
            spent: None,
            last_press: None,
        }
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Switches keymaps, finishing anything armed first.
    pub fn set_keymap(&mut self, keymap: Keymap) -> Outputs {
        let out = self.interrupt();
        self.keymap = keymap;
        self.last_step = None;
        out
    }

    /// What the arrows do now.
    pub fn arrows(&self) -> Arrows {
        if let Some(gesture) = self.gesture {
            return Arrows::Edit {
                verb: gesture.verb,
                step: gesture.step,
            };
        }
        match self.navigation.map(|navigation| navigation.layer) {
            Some(Layer::Coarse) => Arrows::Coarse,
            Some(Layer::Micro) => Arrows::Micro,
            Some(Layer::View) => Arrows::View,
            None => Arrows::Navigate,
        }
    }

    /// A key went down, or the operating system repeated one that is down. `at` is any clock
    /// that only goes forward.
    pub fn press(&mut self, key: Key, mods: Mods, at: Duration) -> Outputs {
        let mut out = Outputs::new();
        self.expire(at, &mut out);
        self.last_press = Some(at);
        let repeat = self.is_down(key);
        self.down |= 1 << key.index();
        if let Some(direction) = key.direction() {
            self.arrow(direction, mods, &mut out);
            return out;
        }
        if repeat {
            return out;
        }
        if mods.command || mods.alt {
            let undo = mods.command && !mods.alt && !mods.shift && key == Key::Z;
            if undo && self.gesture.is_some_and(|gesture| gesture.stepped) {
                self.end(false, &mut out);
                self.spent = Some(key);
            } else {
                self.raw(key, mods, &mut out);
            }
            return out;
        }
        match self.keymap.job(key) {
            None => self.raw(key, mods, &mut out),
            Some(Job::Verb(verb)) => self.verb(key, verb, &mut out),
            Some(Job::Step(step)) => self.step(key, step),
            Some(Job::Action(action)) => {
                self.end(true, &mut out);
                self.navigation = None;
                let action = if mods.shift && action == Action::Add {
                    Action::Remove
                } else {
                    action
                };
                out.push(Output::Action(action));
            }
            Some(Job::Out) => {
                self.end(true, &mut out);
                self.navigation = None;
            }
            Some(Job::Back) => {
                if self.gesture.is_some() {
                    self.end(false, &mut out);
                } else if self.navigation.take().is_none() {
                    out.push(Output::Back);
                }
            }
            Some(Job::View) => {
                self.end(true, &mut out);
                self.navigation = Some(Navigation {
                    layer: Layer::View,
                    key,
                    held: true,
                    used_while_held: false,
                });
            }
        }
        out
    }

    /// A key came up.
    pub fn release(&mut self, key: Key) -> Outputs {
        let mut out = Outputs::new();
        if !self.is_down(key) {
            return out;
        }
        self.down &= !(1 << key.index());
        if self.spent == Some(key) {
            self.spent = None;
            return out;
        }
        let tap_arms = self.keymap.settings().tap_arms;
        if let Some(gesture) = &mut self.gesture {
            if gesture.key == key && gesture.held {
                if gesture.used_while_held || !tap_arms {
                    self.end(true, &mut out);
                } else {
                    // A tap, or a change of mind: either way the verb is now armed.
                    gesture.held = false;
                }
                return out;
            }
        }
        if let Some(navigation) = &mut self.navigation {
            if navigation.key == key && navigation.held {
                if navigation.used_while_held || !tap_arms {
                    self.navigation = None;
                } else {
                    navigation.held = false;
                }
            }
        }
        out
    }

    /// Lets time pass, for the timeout; call it now and then, so the arrow map catches up.
    pub fn poll(&mut self, at: Duration) -> Outputs {
        let mut out = Outputs::new();
        self.expire(at, &mut out);
        out
    }

    /// A pointer press, text entry, or a focus change the keyboard didn't make: finish anything
    /// armed and keep it.
    pub fn interrupt(&mut self) -> Outputs {
        let mut out = Outputs::new();
        self.end(true, &mut out);
        self.navigation = None;
        out
    }

    /// The window lost the keyboard: finish anything armed, and forget the keys held down, whose
    /// releases won't arrive.
    pub fn lost_focus(&mut self) -> Outputs {
        let out = self.interrupt();
        self.down = 0;
        self.spent = None;
        self.last_step = None;
        out
    }

    fn is_down(&self, key: Key) -> bool {
        self.down & (1 << key.index()) != 0
    }

    fn arrow(&mut self, direction: Direction, mods: Mods, out: &mut Outputs) {
        let view_held = match self.keymap.view_modifier() {
            Some(Modifier::Shift) => mods.shift,
            Some(Modifier::Alt) => mods.alt,
            None => false,
        };
        if view_held && !mods.command {
            self.end(true, out);
            self.navigation = None;
            out.push(Output::View { direction });
            return;
        }
        if mods.shift || mods.alt || mods.command {
            self.raw(direction.key(), mods, out);
            return;
        }
        if let Some(gesture) = &mut self.gesture {
            gesture.stepped = true;
            gesture.used_while_held |= gesture.held;
            out.push(Output::Step {
                verb: gesture.verb,
                step: gesture.step,
                direction,
            });
            if !gesture.held && self.keymap.settings().one_arrow {
                self.end(true, out);
            }
            return;
        }
        if let Some(navigation) = &mut self.navigation {
            out.push(match navigation.layer {
                Layer::Coarse => Output::Navigate {
                    direction,
                    coarse: true,
                },
                Layer::Micro => Output::Within { direction },
                Layer::View => Output::View { direction },
            });
            if navigation.held {
                navigation.used_while_held = true;
            } else {
                self.navigation = None;
            }
            return;
        }
        out.push(Output::Navigate {
            direction,
            coarse: false,
        });
    }

    fn verb(&mut self, key: Key, verb: Verb, out: &mut Outputs) {
        if let Some(gesture) = self.gesture {
            self.end(true, out);
            if gesture.key == key && !gesture.held {
                // Tapping the armed verb again finishes it.
                self.spent = Some(key);
                return;
            }
        }
        let step = match self.last_step {
            Some((step_key, step)) if self.is_down(step_key) => step,
            _ => Step::Fine,
        };
        // A COARSE held for navigation is now the verb's step instead.
        self.navigation = None;
        self.gesture = Some(Gesture {
            verb,
            key,
            step,
            held: true,
            used_while_held: false,
            stepped: verb == Verb::Duplicate,
        });
        out.push(Output::Begin { verb });
    }

    fn step(&mut self, key: Key, step: Step) {
        self.last_step = Some((key, step));
        if let Some(gesture) = &mut self.gesture {
            gesture.step = step;
            return;
        }
        // COARSE and MICRO with nothing armed move the focus a level up or down; FINE and
        // MUSICAL have nothing to size, and forget either.
        let layer = match step {
            Step::Coarse => Some(Layer::Coarse),
            Step::Micro => Some(Layer::Micro),
            Step::Fine | Step::Musical => None,
        };
        if self
            .navigation
            .is_some_and(|navigation| navigation.layer == Layer::View)
        {
            return;
        }
        self.navigation = layer.map(|layer| Navigation {
            layer,
            key,
            held: true,
            used_while_held: false,
        });
    }

    /// A key the language doesn't use: the next command, so anything armed finishes first.
    fn raw(&mut self, key: Key, mods: Mods, out: &mut Outputs) {
        self.end(true, out);
        if self.navigation.is_some_and(|navigation| !navigation.held) {
            self.navigation = None;
        }
        out.push(Output::Raw { key, mods });
    }

    fn end(&mut self, keep: bool, out: &mut Outputs) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        if gesture.stepped {
            out.push(if keep { Output::Finish } else { Output::Cancel });
        }
    }

    /// Finishes a tapped verb, and forgets a tapped COARSE, MICRO or VIEW, once the timeout has
    /// passed.
    fn expire(&mut self, at: Duration, out: &mut Outputs) {
        let (Some(timeout), Some(last)) = (self.keymap.settings().timeout, self.last_press) else {
            return;
        };
        if at.saturating_sub(last) < timeout {
            return;
        }
        if self.gesture.is_some_and(|gesture| !gesture.held) {
            self.end(true, out);
        }
        if self.navigation.is_some_and(|navigation| !navigation.held) {
            self.navigation = None;
        }
    }
}

#[cfg(test)]
mod tests;
