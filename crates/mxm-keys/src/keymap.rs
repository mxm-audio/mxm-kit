//! The language's jobs, and the keymap file that puts them on keys.
//!
//! A keymap is plain text, one `name = value` line each; `#` starts a comment:
//!
//! ```text
//! name = Verbs on the top row
//! move = W
//! delete = Delete Backspace
//! view = F, Shift
//! out =               # left unbound
//! tap-arms = yes
//! timeout = off       # or seconds: 4, 2.5
//! ```
//!
//! A job takes any number of keys, and a key does one job. The arrows aren't bound: they are the
//! right hand's, and every job is used with them. There is no way to write a chord with `Command`,
//! so no keymap can take a standard shortcut (`Ctrl/Cmd+Z`, `X`, `C`, `V`, `S`) away. `Shift` and
//! `Alt` are reported only as held, never pressed, so only VIEW, which works while held with the
//! arrows, can be put on one.

use std::fmt;
use std::time::Duration;

use crate::key::{COUNT, Key};

/// Held, or tapped and armed, a verb gives the arrows a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verb {
    /// Position: in time, in pitch, or a point's value.
    Move,
    /// Size, length, duration.
    Extent,
    /// The item's main value: a parameter, a note's velocity, a chord.
    Value,
    /// The selection, by extending it.
    Select,
    /// A copy of the selection, which the arrows take where it goes and the gesture's end puts
    /// there (the owner, 2026-10-08). Its gesture is a change from the start: the copy.
    Duplicate,
}

/// How far one arrow press goes in a gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Step {
    /// Large. Without a verb, the arrows go one structural level up instead.
    Coarse,
    /// The normal step, and the one a gesture starts with.
    Fine,
    /// Tiny.
    Micro,
    /// The next meaningful value instead of a number.
    Musical,
}

/// A job done at once, on the focused item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Add,
    /// Takes away what ADD gives. Shift with ADD's key does it, so it needs no key of its own:
    /// Z gives, Shift+Z takes away (the owner, 2026-10-07).
    Remove,
    Delete,
    /// Deletes and closes the gap: what came after moves up to where it began.
    Ripple,
    /// Opens the item: goes in.
    Open,
}

/// Every job a key can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Job {
    Verb(Verb),
    Step(Step),
    Action(Action),
    /// Finishes whatever is armed and keeps it. Pressed again, it does nothing.
    Out,
    /// Cancels a gesture, or goes back a level.
    Back,
    /// With the arrows: the focus to the neighbouring view.
    View,
}

impl Job {
    /// Every job, in the order a keymap file lists them.
    pub const ALL: [Job; 17] = [
        Job::Verb(Verb::Move),
        Job::Verb(Verb::Extent),
        Job::Verb(Verb::Value),
        Job::Verb(Verb::Select),
        Job::Verb(Verb::Duplicate),
        Job::Step(Step::Coarse),
        Job::Step(Step::Fine),
        Job::Step(Step::Micro),
        Job::Step(Step::Musical),
        Job::Action(Action::Add),
        Job::Action(Action::Remove),
        Job::Action(Action::Delete),
        Job::Action(Action::Ripple),
        Job::Action(Action::Open),
        Job::Out,
        Job::Back,
        Job::View,
    ];

    /// The job's name in a keymap file.
    pub const fn name(self) -> &'static str {
        match self {
            Job::Verb(Verb::Move) => "move",
            Job::Verb(Verb::Extent) => "extent",
            Job::Verb(Verb::Value) => "value",
            Job::Verb(Verb::Select) => "select",
            Job::Verb(Verb::Duplicate) => "duplicate",
            Job::Step(Step::Coarse) => "coarse",
            Job::Step(Step::Fine) => "fine",
            Job::Step(Step::Micro) => "micro",
            Job::Step(Step::Musical) => "musical",
            Job::Action(Action::Add) => "add",
            Job::Action(Action::Remove) => "remove",
            Job::Action(Action::Delete) => "delete",
            Job::Action(Action::Ripple) => "ripple",
            Job::Action(Action::Open) => "open",
            Job::Out => "out",
            Job::Back => "back",
            Job::View => "view",
        }
    }

    /// The job this name in a keymap file means, ignoring case.
    pub fn from_name(name: &str) -> Option<Job> {
        Job::ALL
            .into_iter()
            .find(|job| job.name().eq_ignore_ascii_case(name))
    }
}

/// A modifier VIEW can be held on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Modifier {
    Shift,
    Alt,
}

/// The personal choices a keymap file holds. Nothing has to be set for held and tapped gestures to
/// both work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Tapping a verb, or COARSE or VIEW, arms it. Off, they work only while held.
    pub tap_arms: bool,
    /// A tapped verb ends after one arrow press, instead of staying armed until it's finished.
    pub one_arrow: bool,
    /// A tapped verb that has waited this long for a key finishes on its own. Off by default,
    /// because a timeout punishes slow, deliberate work.
    pub timeout: Option<Duration>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            tap_arms: true,
            one_arrow: false,
            timeout: None,
        }
    }
}

/// Which key does which job, and the settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Keymap {
    name: String,
    jobs: [Option<Job>; COUNT],
    view_modifier: Option<Modifier>,
    settings: Settings,
}

/// A keymap file that doesn't read, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The line, from 1.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

/// The keymaps the kit ships, as `(file stem, text)`, the default first: starting points to try,
/// tuned by practice.
pub const SHIPPED: [(&str, &str); 3] = [
    ("study", include_str!("../keymaps/study.keymap")),
    ("notes", include_str!("../keymaps/notes.keymap")),
    ("home-row", include_str!("../keymaps/home-row.keymap")),
];

/// The default keymap's stem: the one the key study proposed (newDAWn's `docs/key-study.md`).
pub const DEFAULT: &str = "study";

impl Keymap {
    /// A keymap with no job on any key.
    pub fn empty() -> Keymap {
        Keymap {
            name: String::new(),
            jobs: [None; COUNT],
            view_modifier: None,
            settings: Settings::default(),
        }
    }

    /// Reads a keymap file.
    pub fn parse(text: &str) -> Result<Keymap, Error> {
        let mut keymap = Keymap::empty();
        // The line each job was bound on, so binding it twice can say where it was first.
        let mut bound_on: Vec<(Job, usize)> = Vec::new();
        let mut set_on: Vec<(&str, usize)> = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let error = |message: String| Error { line, message };
            let content = raw.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            let Some((name, value)) = content.split_once('=') else {
                return Err(error(format!("`{content}` is not `name = value`")));
            };
            let (name, value) = (name.trim(), value.trim());
            if let Some(job) = Job::from_name(name) {
                if let Some(&(_, first)) = bound_on.iter().find(|(bound, _)| *bound == job) {
                    return Err(error(format!("{name} is already bound, on line {first}")));
                }
                bound_on.push((job, line));
                keymap.bind(job, value).map_err(error)?;
                continue;
            }
            let name = name.to_ascii_lowercase();
            let setting = match name.as_str() {
                "name" => "name",
                "tap-arms" => "tap-arms",
                "one-arrow" => "one-arrow",
                "timeout" => "timeout",
                _ => return Err(error(format!("`{name}` is not a job or a setting"))),
            };
            if let Some(&(_, first)) = set_on.iter().find(|(set, _)| *set == setting) {
                return Err(error(format!("{setting} is already set, on line {first}")));
            }
            set_on.push((setting, line));
            match setting {
                "name" => keymap.name = value.to_string(),
                "tap-arms" => keymap.settings.tap_arms = yes_or_no(value).map_err(error)?,
                "one-arrow" => keymap.settings.one_arrow = yes_or_no(value).map_err(error)?,
                _ => keymap.settings.timeout = timeout(value).map_err(error)?,
            }
        }
        Ok(keymap)
    }

    fn bind(&mut self, job: Job, value: &str) -> Result<(), String> {
        for word in value
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
        {
            let modifier = if word.eq_ignore_ascii_case("Shift") {
                Some(Modifier::Shift)
            } else if word.eq_ignore_ascii_case("Alt") {
                Some(Modifier::Alt)
            } else {
                None
            };
            if let Some(modifier) = modifier {
                if job != Job::View {
                    return Err(format!(
                        "only view can be on {word}: a modifier is only ever held, never pressed"
                    ));
                }
                self.view_modifier = Some(modifier);
                continue;
            }
            let Some(key) = Key::from_name(word) else {
                return Err(format!("`{word}` is not a key"));
            };
            if key.direction().is_some() {
                return Err(format!(
                    "{word} can't be bound: the arrows are the right hand's, used with every job"
                ));
            }
            if let Some(other) = self.jobs[key.index()] {
                return Err(format!("{} is already {}", key.name(), other.name()));
            }
            self.jobs[key.index()] = Some(job);
        }
        Ok(())
    }

    /// The keymap's name, for choosing between several.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The job a key does, if any.
    pub fn job(&self, key: Key) -> Option<Job> {
        self.jobs[key.index()]
    }

    /// The keys that do a job, to show on screen.
    pub fn keys(&self, job: Job) -> impl Iterator<Item = Key> + '_ {
        Key::ALL
            .iter()
            .copied()
            .filter(move |key| self.job(*key) == Some(job))
    }

    /// The modifier VIEW is held on, if it is on one.
    pub fn view_modifier(&self) -> Option<Modifier> {
        self.view_modifier
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// A shipped keymap by its file stem.
    pub fn shipped(stem: &str) -> Option<Keymap> {
        let (_, text) = SHIPPED.iter().find(|(shipped, _)| *shipped == stem)?;
        Some(Keymap::parse(text).expect("the shipped keymaps read"))
    }
}

impl Default for Keymap {
    /// The shipped default, [`DEFAULT`].
    fn default() -> Keymap {
        Keymap::shipped(DEFAULT).expect("the default keymap ships")
    }
}

fn yes_or_no(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => Err(format!("`{value}` is not yes or no")),
    }
}

fn timeout(value: &str) -> Result<Option<Duration>, String> {
    if value.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    match value.parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds > 0.0 => {
            Ok(Some(Duration::from_secs_f64(seconds)))
        }
        _ => Err(format!("`{value}` is not off or a number of seconds")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(text: &str) -> Error {
        Keymap::parse(text).expect_err("the keymap is refused")
    }

    #[test]
    fn every_job_name_reads_back() {
        for job in Job::ALL {
            assert_eq!(Job::from_name(job.name()), Some(job));
        }
    }

    #[test]
    fn the_default_is_the_key_studys() {
        let keymap = Keymap::default();
        assert_eq!(SHIPPED[0].0, DEFAULT);
        assert_eq!(keymap.job(Key::E), Some(Job::Verb(Verb::Move)));
        assert_eq!(keymap.job(Key::S), Some(Job::Step(Step::Coarse)));
        assert_eq!(keymap.job(Key::R), Some(Job::Step(Step::Musical)));
        assert_eq!(keymap.job(Key::A), Some(Job::Verb(Verb::Value)));
        assert_eq!(keymap.job(Key::W), Some(Job::Verb(Verb::Duplicate)));
        assert_eq!(keymap.job(Key::X), Some(Job::Verb(Verb::Extent)));
        assert_eq!(keymap.job(Key::V), Some(Job::Action(Action::Add)));
        assert_eq!(keymap.job(Key::Z), None);
        assert_eq!(Keymap::shipped("none"), None);
    }

    #[test]
    fn the_shipped_keymaps_read_and_bind_every_job() {
        for (stem, text) in SHIPPED {
            let keymap = Keymap::parse(text).unwrap_or_else(|error| panic!("{stem}: {error}"));
            assert!(!keymap.name().is_empty(), "{stem} has a name");
            for job in Job::ALL {
                // REMOVE is Shift with ADD's key; a keymap may give it a key of its own too.
                let bound = keymap.keys(job).next().is_some()
                    || (job == Job::View && keymap.view_modifier().is_some())
                    || job == Job::Action(Action::Remove);
                assert!(bound, "{stem} binds {}", job.name());
            }
            assert_eq!(keymap.settings(), Settings::default(), "{stem}'s settings");
        }
    }

    #[test]
    fn a_file_binds_keys_and_reads_settings() {
        let keymap = Keymap::parse(
            "# a comment\n\
             name = Test\n\
             MOVE = w   # case doesn't matter\n\
             delete = Delete, Backspace\n\
             out =\n\
             view = Alt\n\
             tap-arms = no\n\
             one-arrow = yes\n\
             timeout = 2.5\n",
        )
        .unwrap();
        assert_eq!(keymap.name(), "Test");
        assert_eq!(keymap.job(Key::W), Some(Job::Verb(Verb::Move)));
        let deletes: Vec<Key> = keymap.keys(Job::Action(Action::Delete)).collect();
        assert_eq!(deletes, [Key::Backspace, Key::Delete]);
        assert_eq!(keymap.keys(Job::Out).count(), 0);
        assert_eq!(keymap.view_modifier(), Some(Modifier::Alt));
        assert_eq!(
            keymap.settings(),
            Settings {
                tap_arms: false,
                one_arrow: true,
                timeout: Some(Duration::from_millis(2500)),
            }
        );
    }

    #[test]
    fn mistakes_are_refused_with_their_line() {
        assert_eq!(
            refusal("move = W\nfly = F"),
            Error {
                line: 2,
                message: "`fly` is not a job or a setting".into()
            }
        );
        assert_eq!(refusal("move = Wq").message, "`Wq` is not a key");
        assert_eq!(refusal("move = W\nadd = W").message, "W is already move");
        assert_eq!(
            refusal("move = W\n\nmove = E").message,
            "move is already bound, on line 1"
        );
        assert!(refusal("move = Left").message.contains("right hand"));
        assert!(refusal("move = Shift").message.contains("only view"));
        assert_eq!(
            refusal("tap-arms = maybe").message,
            "`maybe` is not yes or no"
        );
        assert_eq!(
            refusal("timeout = -1").message,
            "`-1` is not off or a number of seconds"
        );
        assert_eq!(refusal("timeout = off\ntimeout = 3").line, 2);
        assert_eq!(
            refusal("just words").message,
            "`just words` is not `name = value`"
        );
    }
}
