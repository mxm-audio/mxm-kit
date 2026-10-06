//! Keys as physical positions, the arrows, and the modifiers an adapter reports with them.

macro_rules! keys {
    ($($key:ident => $name:literal,)*) => {
        /// A key by where it sits, named after its legend on a US QWERTY keyboard.
        ///
        /// A keymap names positions, not letters, so a layout still works on AZERTY or Dvorak: `W`
        /// is the key above `S`, whatever is printed on it. Showing the user's own legend is the
        /// adapter's job. The modifiers are not keys here: they arrive as [`Mods`] with each press.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Key {
            $($key,)*
        }

        impl Key {
            /// Every key, in declaration order.
            pub const ALL: &'static [Key] = &[$(Key::$key,)*];

            /// The key's name in a keymap file.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Key::$key => $name,)*
                }
            }
        }
    };
}

keys! {
    A => "A", B => "B", C => "C", D => "D", E => "E", F => "F", G => "G", H => "H", I => "I",
    J => "J", K => "K", L => "L", M => "M", N => "N", O => "O", P => "P", Q => "Q", R => "R",
    S => "S", T => "T", U => "U", V => "V", W => "W", X => "X", Y => "Y", Z => "Z",
    Digit0 => "0", Digit1 => "1", Digit2 => "2", Digit3 => "3", Digit4 => "4",
    Digit5 => "5", Digit6 => "6", Digit7 => "7", Digit8 => "8", Digit9 => "9",
    Up => "Up", Down => "Down", Left => "Left", Right => "Right",
    Tab => "Tab", CapsLock => "CapsLock", Escape => "Escape", Enter => "Enter", Space => "Space",
    Backspace => "Backspace", Delete => "Delete", Insert => "Insert", Home => "Home", End => "End",
    PageUp => "PageUp", PageDown => "PageDown",
    Minus => "Minus", Equals => "Equals", BracketLeft => "BracketLeft",
    BracketRight => "BracketRight", Backslash => "Backslash", Semicolon => "Semicolon",
    Quote => "Quote", Backquote => "Backquote", Comma => "Comma", Period => "Period",
    Slash => "Slash",
    F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6",
    F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
}

/// How many keys there are; a key's place in [`Key::ALL`] is below it.
pub(crate) const COUNT: usize = Key::ALL.len();

// The engine keeps the keys held down as one bit each.
const _: () = assert!(COUNT <= 128);

impl Key {
    /// The key whose [`name`](Key::name) this is, ignoring case.
    pub fn from_name(name: &str) -> Option<Key> {
        Key::ALL
            .iter()
            .copied()
            .find(|key| key.name().eq_ignore_ascii_case(name))
    }

    /// The direction of an arrow key.
    pub const fn direction(self) -> Option<Direction> {
        match self {
            Key::Up => Some(Direction::Up),
            Key::Down => Some(Direction::Down),
            Key::Left => Some(Direction::Left),
            Key::Right => Some(Direction::Right),
            _ => None,
        }
    }

    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// Where an arrow points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// The arrow key that points this way.
    pub const fn key(self) -> Key {
        match self {
            Direction::Up => Key::Up,
            Direction::Down => Key::Down,
            Direction::Left => Key::Left,
            Direction::Right => Key::Right,
        }
    }
}

/// The modifiers held when a key was pressed.
///
/// `command` is the platform's shortcut modifier: `Ctrl` on Windows and Linux, `Cmd` on macOS. A
/// key pressed with it is a standard shortcut and never one of the language's jobs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        shift: false,
        alt: false,
        command: false,
    };
    pub const SHIFT: Mods = Mods {
        shift: true,
        alt: false,
        command: false,
    };
    pub const ALT: Mods = Mods {
        shift: false,
        alt: true,
        command: false,
    };
    pub const COMMAND: Mods = Mods {
        shift: false,
        alt: false,
        command: true,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_reads_back_as_its_key() {
        for &key in Key::ALL {
            assert_eq!(Key::from_name(key.name()), Some(key));
            assert_eq!(Key::from_name(&key.name().to_lowercase()), Some(key));
        }
        assert_eq!(Key::from_name("Esc"), None);
    }

    #[test]
    fn a_key_is_its_place_in_the_list() {
        for (index, &key) in Key::ALL.iter().enumerate() {
            assert_eq!(key.index(), index);
        }
    }

    #[test]
    fn only_the_arrows_have_a_direction() {
        for &key in Key::ALL {
            if let Some(direction) = key.direction() {
                assert_eq!(direction.key(), key);
            }
        }
        let arrows = Key::ALL.iter().filter(|key| key.direction().is_some());
        assert_eq!(arrows.count(), 4);
    }
}
