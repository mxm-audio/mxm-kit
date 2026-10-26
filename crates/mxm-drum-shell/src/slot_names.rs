//! Each slot's name: the drum machine's (the owner, 2026-10-14: "make the software make a 'screen
//! name' like bd1, and then give the user the ability to rename it to anything like bd1 ->
//! subkick"), shared with model drums (the owner, 2026-10-26: "the ability to name the drums", and
//! "the drums should be prenamed in the presets").
//!
//! - **A slot is called what it is**: its model's kind and how many slots before it play that
//!   kind, plus one: `bd1`, `sn1`, `ch1` ([`DrumSlots::made_up_names`], each plugin's codes). So
//!   every kit, and Init, reads as named, with no text in a kit.
//! - **A name typed over it is the song's**: a persistent field, saved with the plugin's state,
//!   which a kit never carries. Loading a kit, or Init, empties every typed name, since the drums
//!   changed ([`clear`], from `Instrument::patch_replaced`); changing one slot's model keeps its
//!   name. An empty name follows the model again, as an empty lane name in newDAWn follows the
//!   instrument.
//! - **The host is told** through CLAP's note-name extension, the nice-plug fork's capability: each
//!   Kit slot's key carries its name, so a host's drum lanes read as the slots do ([`HostNames`]).
//!
//! Every write of the names raises [`DrumSlots::names_written`]; the editor then sends the fork's
//! dirty-only GUI transaction ([`announce`]), which tells the host the song changed and makes the
//! wrapper read the note names again.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use mxm_part_routing::{PartAssignment, claimed_channels};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::{ClapNoteNames, NoteName};

use crate::SLOT_COUNT;

/// The persistent field's key in the plugin's state.
pub const FIELD: &str = "slot-names";

/// The most of a typed name that is kept, in characters (Unicode scalar values, not bytes).
pub const NAME_CHARS: usize = 32;

/// A drum plugin's slots, as the names read them: its parameters implement it.
pub trait DrumSlots {
    /// The Kit's first key, the one slot 1 plays (General MIDI's kick).
    const FIRST_KEY: u8;
    /// The typed names: the plugin's persistent field, under [`FIELD`].
    fn typed_names(&self) -> &RwLock<Vec<String>>;
    /// Raised by every write of the names, and taken by the editor's [`announce`].
    fn names_written(&self) -> &AtomicBool;
    /// Each slot's name from its model's kind (`bd1`): none for a slot whose model is Off or
    /// unavailable.
    fn made_up_names(&self) -> [Option<String>; SLOT_COUNT];
    /// Whether `slot`'s model plays in this build.
    fn plays(&self, slot: usize) -> bool;
    /// `slot`'s MIDI channel parameter, as `PartAssignment::from_parameter` reads it.
    fn midi_channel(&self, slot: usize) -> i32;
    /// The parameter IDs the host's names follow: each slot's model (its made-up name and whether
    /// it plays) and MIDI channel (whether it is a Kit slot, and which channels a Kit key sounds on).
    fn name_dependencies(&self) -> Vec<&'static str>;
}

/// Every slot's name empty: each follows its model.
#[must_use]
pub fn empty() -> Vec<String> {
    vec![String::new(); SLOT_COUNT]
}

/// A typed name as it is kept: without control characters, trimmed, and at most [`NAME_CHARS`]
/// characters.
#[must_use]
pub fn canonical_name(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    let cut: String = clean.trim().chars().take(NAME_CHARS).collect();
    cut.trim_end().to_owned()
}

/// The names as they are kept, from whatever a state held under [`FIELD`]: a list cut or padded to
/// `SLOT_COUNT`, each name [canonical](canonical_name). Nothing there, or anything but a list,
/// is every name empty, and an entry that isn't text is an empty name: the names are a convenience,
/// never worth refusing a song over.
#[must_use]
pub fn canonical(stored: Option<&str>) -> Vec<String> {
    let listed: Vec<String> = stored
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .and_then(|value| match value {
            serde_json::Value::Array(items) => Some(items),
            _ => None,
        })
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(canonical_name).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    (0..SLOT_COUNT)
        .map(|slot| listed.get(slot).cloned().unwrap_or_default())
        .collect()
}

/// `Plugin::filter_state`'s part, run on every state before it is restored: the names' field
/// rewritten to its canonical text. An older state without it gets it, every name empty, so
/// opening it empties names typed since rather than keeping them. The field then installs exactly
/// what arrived, which the fork's check on persistent fields requires.
pub fn filter(fields: &mut BTreeMap<String, String>) {
    let names = canonical(fields.get(FIELD).map(String::as_str));
    let text = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_owned());
    fields.insert(FIELD.to_owned(), text);
}

fn typed(slots: &impl DrumSlots) -> Vec<String> {
    slots
        .typed_names()
        .read()
        .map(|names| names.clone())
        .unwrap_or_default()
}

/// What each slot is called: the name typed over it, or else the one it is given; none for a slot
/// with nothing typed whose model is Off or unavailable.
#[must_use]
pub fn shown(slots: &impl DrumSlots) -> [Option<String>; SLOT_COUNT] {
    let made_up = slots.made_up_names();
    let typed = typed(slots);
    std::array::from_fn(|slot| match typed.get(slot) {
        Some(name) if !name.is_empty() => Some(name.clone()),
        _ => made_up[slot].clone(),
    })
}

/// The name a slot would have with nothing typed over it.
#[must_use]
pub fn made_up(slots: &impl DrumSlots, slot: usize) -> Option<String> {
    slots.made_up_names()[slot].clone()
}

/// Types a slot's name. It is kept [canonical](canonical_name), and empty when it is the name the
/// slot is given anyway, so it goes on following its model.
pub fn set(slots: &impl DrumSlots, slot: usize, text: &str) {
    let mut name = canonical_name(text);
    if made_up(slots, slot).as_deref() == Some(name.as_str()) {
        name.clear();
    }
    let Ok(mut names) = slots.typed_names().write() else {
        return;
    };
    if names.len() != SLOT_COUNT {
        *names = empty();
    }
    if names[slot] != name {
        names[slot] = name;
        slots.names_written().store(true, Ordering::Release);
    }
}

/// Empties every typed name: a kit load and Init (`Instrument::patch_replaced`).
pub fn clear(slots: &impl DrumSlots) {
    let Ok(mut names) = slots.typed_names().write() else {
        return;
    };
    if names.iter().any(|name| !name.is_empty()) || names.len() != SLOT_COUNT {
        *names = empty();
        slots.names_written().store(true, Ordering::Release);
    }
}

/// The editor's half of every write, once a frame: when the names changed, nice-plug's dirty-only
/// GUI transaction (no parameters, the fields as they are now), which tells the host the song
/// changed and makes the wrapper read the note names again (the fork's `PATCHES.md`, *GUI-authored
/// state dirty* and *Note names*).
pub fn announce(slots: &impl DrumSlots, context: &GuiContext) {
    if slots.names_written().swap(false, Ordering::AcqRel) {
        let mut state = context.get_state();
        state.params.clear();
        context.set_state(state);
    }
}

/// The names the host is told: CLAP note names, through the fork's capability.
pub struct HostNames<P>(pub Arc<P>);

impl<P: DrumSlots + Send + Sync> ClapNoteNames for HostNames<P> {
    /// Each Kit slot whose model plays, on its key and the plugin's one note port, on every channel a
    /// Kit key sounds on: any (`-1`) while no slot claims a channel, otherwise each unclaimed one.
    /// A chromatic slot plays every key on its channel, so it names none.
    fn names(&self) -> Vec<NoteName> {
        let slots = &*self.0;
        let assignments: [PartAssignment; SLOT_COUNT] =
            std::array::from_fn(|slot| PartAssignment::from_parameter(slots.midi_channel(slot)));
        let claimed = claimed_channels(&assignments);
        let channels: Vec<i16> = if claimed == 0 {
            vec![-1]
        } else {
            (0..16_i16)
                .filter(|channel| claimed & (1 << channel) == 0)
                .collect()
        };
        let shown = shown(slots);
        let mut names = Vec::new();
        for (slot, name) in shown.into_iter().enumerate() {
            let plays = slots.plays(slot);
            let Some(name) =
                name.filter(|_| plays && assignments[slot] == PartAssignment::FixedNote)
            else {
                continue;
            };
            let key = i16::from(P::FIRST_KEY) + slot as i16;
            names.extend(channels.iter().map(|&channel| NoteName {
                name: name.clone(),
                port: 0,
                key,
                channel,
            }));
        }
        names
    }

    fn depends_on(&self) -> Vec<&'static str> {
        self.0.name_dependencies()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Sixteen slots as the names read them: what a plugin's parameters answer, set by hand.
    pub(crate) struct Slots {
        pub names: RwLock<Vec<String>>,
        pub written: AtomicBool,
        pub made_up: [Option<String>; SLOT_COUNT],
        pub plays: [bool; SLOT_COUNT],
        pub channels: [i32; SLOT_COUNT],
    }

    impl Slots {
        /// A kick, two snares and thirteen toms, every one playing, every one a Kit slot.
        pub(crate) fn new() -> Self {
            let made_up = std::array::from_fn(|slot| {
                Some(match slot {
                    0 => "bd1".to_owned(),
                    1 => "sn1".to_owned(),
                    2 => "sn2".to_owned(),
                    n => format!("tm{}", n - 2),
                })
            });
            Self {
                names: RwLock::new(empty()),
                written: AtomicBool::new(false),
                made_up,
                plays: [true; SLOT_COUNT],
                channels: [0; SLOT_COUNT],
            }
        }
    }

    impl DrumSlots for Slots {
        const FIRST_KEY: u8 = 36;
        fn typed_names(&self) -> &RwLock<Vec<String>> {
            &self.names
        }
        fn names_written(&self) -> &AtomicBool {
            &self.written
        }
        fn made_up_names(&self) -> [Option<String>; SLOT_COUNT] {
            self.made_up.clone()
        }
        fn plays(&self, slot: usize) -> bool {
            self.plays[slot]
        }
        fn midi_channel(&self, slot: usize) -> i32 {
            self.channels[slot]
        }
        fn name_dependencies(&self) -> Vec<&'static str> {
            vec!["model_1", "midi_channel_1"]
        }
    }

    fn written(slots: &Slots) -> bool {
        slots.written.swap(false, Ordering::AcqRel)
    }

    #[test]
    fn a_typed_name_wins_and_an_empty_or_the_given_one_follows_the_model() {
        let mut slots = Slots::new();
        set(&slots, 0, "  subkick ");
        assert!(written(&slots));
        assert_eq!(shown(&slots)[0].as_deref(), Some("subkick"));
        assert_eq!(slots.names.read().unwrap()[0], "subkick");

        // Changing the slot's model keeps its name.
        slots.made_up[0] = Some("sn3".to_owned());
        assert_eq!(shown(&slots)[0].as_deref(), Some("subkick"));

        set(&slots, 0, "");
        assert!(written(&slots));
        assert_eq!(
            shown(&slots)[0].as_deref(),
            Some("sn3"),
            "the model's again"
        );

        // Typing the name it is given keeps nothing, so it goes on following the model.
        set(&slots, 1, "sn1");
        assert_eq!(slots.names.read().unwrap()[1], "");
        set(&slots, 1, "sn1");
        assert!(!written(&slots), "nothing changed, nothing to tell");
    }

    #[test]
    fn clearing_empties_every_name_and_tells_only_when_one_was_typed() {
        let slots = Slots::new();
        clear(&slots);
        assert!(!written(&slots), "nothing typed, nothing to tell");
        set(&slots, 4, "crack");
        written(&slots);
        clear(&slots);
        assert!(written(&slots));
        assert_eq!(*slots.names.read().unwrap(), empty());
    }

    #[test]
    fn names_are_kept_canonical() {
        assert_eq!(canonical_name("  sub\u{7}kick\n "), "subkick");
        let long = "ø".repeat(40);
        assert_eq!(
            canonical_name(&long).chars().count(),
            NAME_CHARS,
            "cut in characters, not bytes"
        );
        assert_eq!(canonical(None), empty(), "an older state");
        assert_eq!(canonical(Some("{\"a\": 1}")), empty(), "not a list");
        assert_eq!(canonical(Some("not json")), empty());
        let short = canonical(Some("[\"kick\", 4, \" snare \"]"));
        assert_eq!(short.len(), SLOT_COUNT);
        assert_eq!(&short[..3], ["kick", "", "snare"]);
        let many: Vec<String> = (0..40).map(|n| format!("x{n}")).collect();
        let long_list = canonical(Some(&serde_json::to_string(&many).unwrap()));
        assert_eq!(long_list.len(), SLOT_COUNT);
        assert_eq!(long_list[15], "x15");
    }

    #[test]
    fn a_state_without_the_field_gets_it_empty() {
        let mut fields = BTreeMap::new();
        filter(&mut fields);
        assert_eq!(
            fields[FIELD],
            serde_json::to_string(&empty()).unwrap(),
            "an older song empties the names"
        );
        fields.insert(FIELD.to_owned(), "[\" kick \"]".to_owned());
        filter(&mut fields);
        let mut want = empty();
        want[0] = "kick".to_owned();
        assert_eq!(fields[FIELD], serde_json::to_string(&want).unwrap());
    }

    #[test]
    fn the_host_hears_each_kit_slot_on_its_key() {
        let names = HostNames(Arc::new(Slots::new())).names();
        let heard: Vec<(String, i16, i16, i16)> = names
            .into_iter()
            .map(|name| (name.name, name.port, name.key, name.channel))
            .collect();
        let expected: Vec<(String, i16, i16, i16)> = Slots::new()
            .made_up
            .iter()
            .enumerate()
            .map(|(slot, name)| (name.clone().unwrap(), 0, 36 + slot as i16, -1))
            .collect();
        assert_eq!(heard, expected);
    }

    #[test]
    fn a_claimed_channel_takes_its_slot_out_and_names_the_rest_on_the_others() {
        let mut slots = Slots::new();
        // The last slot plays every note on Ch 2 (CLAP channel 1).
        slots.channels[15] = 2;
        set(&slots, 0, "subkick");
        let names = HostNames(Arc::new(slots)).names();
        assert!(
            names.iter().all(|name| name.key != 51),
            "chromatic: no key of its own"
        );
        assert!(
            names.iter().all(|name| name.channel != 1),
            "nor a Kit key on its channel"
        );
        let kick: Vec<i16> = names
            .iter()
            .filter(|name| name.key == 36)
            .map(|name| name.channel)
            .collect();
        assert_eq!(kick, [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        assert!(
            names
                .iter()
                .filter(|name| name.key == 36)
                .all(|name| name.name == "subkick")
        );
    }

    #[test]
    fn a_slot_that_does_not_play_names_nothing_to_the_host() {
        let mut slots = Slots::new();
        slots.plays[3] = false;
        slots.made_up[3] = None;
        set(&slots, 3, "ghost");
        let slots = Arc::new(slots);
        let names = HostNames(slots.clone()).names();
        assert!(names.iter().all(|name| name.key != 39), "Off plays nothing");
        assert_eq!(
            shown(&*slots)[3].as_deref(),
            Some("ghost"),
            "but the box keeps it"
        );
    }
}
