//! Replays key presses through the engine and prints what they mean, one output per line.
//!
//! ```text
//! cargo run -p mxm-keys --example replay -- <keymap file> < presses.txt
//! ```
//!
//! Each input line is `press <Key> [shift] [alt] [command]` or `release <Key>`, a millisecond
//! apart. Each output line is one of `navigate <direction>`, `navigate-coarse <direction>`,
//! `view <direction>`, `step <verb> <step> <direction>`, `finish`, `cancel`, `action <action>`,
//! `back` or `raw <Key>[ shift][ alt][ command]`, all in lower case but the keys. A tool that
//! writes a session as presses can check here that the engine reads them as it meant.

use std::io::{self, BufRead, Write};
use std::time::Duration;

use mxm_keys::{Action, Direction, Engine, Key, Keymap, Mods, Output, Step, Verb};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: replay <keymap file> < presses");
    let text = std::fs::read_to_string(&path).expect("the keymap file reads");
    let keymap = Keymap::parse(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
    let mut engine = Engine::new(keymap);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for (index, line) in io::stdin().lock().lines().enumerate() {
        let line = line.expect("stdin reads");
        let mut words = line.split_whitespace();
        let (Some(what), Some(name)) = (words.next(), words.next()) else {
            continue;
        };
        let key =
            Key::from_name(name).unwrap_or_else(|| panic!("line {}: no key {name}", index + 1));
        let mut mods = Mods::NONE;
        for word in words {
            match word {
                "shift" => mods.shift = true,
                "alt" => mods.alt = true,
                "command" => mods.command = true,
                _ => panic!("line {}: no modifier {word}", index + 1),
            }
        }
        let at = Duration::from_millis(index as u64);
        let outputs = match what {
            "press" => engine.press(key, mods, at),
            "release" => engine.release(key),
            _ => panic!("line {}: {what} is not press or release", index + 1),
        };
        for output in outputs {
            writeln!(out, "{}", describe(output)).expect("stdout writes");
        }
    }
}

fn describe(output: Output) -> String {
    match output {
        Output::Navigate { direction, coarse } => {
            let kind = if coarse {
                "navigate-coarse"
            } else {
                "navigate"
            };
            format!("{kind} {}", direction_name(direction))
        }
        Output::View { direction } => format!("view {}", direction_name(direction)),
        Output::Step {
            verb,
            step,
            direction,
        } => format!(
            "step {} {} {}",
            verb_name(verb),
            step_name(step),
            direction_name(direction)
        ),
        Output::Finish => "finish".into(),
        Output::Cancel => "cancel".into(),
        Output::Action(action) => format!("action {}", action_name(action)),
        Output::Back => "back".into(),
        Output::Raw { key, mods } => {
            let mut text = format!("raw {}", key.name());
            for (held, name) in [
                (mods.shift, "shift"),
                (mods.alt, "alt"),
                (mods.command, "command"),
            ] {
                if held {
                    text.push(' ');
                    text.push_str(name);
                }
            }
            text
        }
    }
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    }
}

fn verb_name(verb: Verb) -> &'static str {
    match verb {
        Verb::Move => "move",
        Verb::Extent => "extent",
        Verb::Value => "value",
        Verb::Select => "select",
    }
}

fn step_name(step: Step) -> &'static str {
    match step {
        Step::Coarse => "coarse",
        Step::Fine => "fine",
        Step::Micro => "micro",
        Step::Musical => "musical",
    }
}

fn action_name(action: Action) -> &'static str {
    match action {
        Action::Add => "add",
        Action::Duplicate => "duplicate",
        Action::Delete => "delete",
        Action::Open => "open",
    }
}
