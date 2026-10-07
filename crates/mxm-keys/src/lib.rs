//! The shared keyboard language: physical keys in, finished gestures out.
//!
//! The left hand says *what*, the right hand *where*. Navigate with the arrows; then choose a verb
//! (MOVE, EXTENT, VALUE, SELECT), optionally a step size (COARSE, FINE, MICRO, MUSICAL), and a
//! direction. Actions (ADD, DUPLICATE, DELETE, RIPPLE, OPEN) work on their own, OUT finishes whatever is
//! armed, and BACK cancels it. A key means the same in every view and every instrument; only the
//! object it acts on changes.
//!
//! - [`key`]: keys as physical positions, the arrows, and the modifiers.
//! - [`keymap`]: the jobs, and the plain-text keymap that puts them on keys.
//! - [`engine`]: the gestures, held or tapped in sequence, and what they mean.
//!
//! An adapter feeds the [`Engine`] presses and releases from whatever toolkit draws the window and
//! turns its [`Output`]s into commands. Nothing here knows what a part, a note or a parameter is.

pub mod engine;
pub mod key;
pub mod keymap;

pub use engine::{Arrows, Engine, Output, Outputs};
pub use key::{Direction, Key, Mods};
pub use keymap::{Action, Job, Keymap, Modifier, Settings, Step, Verb};
