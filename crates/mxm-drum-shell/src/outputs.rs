//! Which outputs the slots are routed to, for the host: the MXM CLAP extension
//! `dk.mxm.outputs-in-use/1` and VST3 interface `IOutputsInUse`, through the nice-plug fork's
//! capability (its `PATCHES.md`, *Outputs in use*; the convention is this kit's
//! `docs/plugin-conventions.md`, *Outputs in use*). A plugin returns the same [`HostOutputs`] from
//! `ClapPlugin::outputs_in_use()` and `Vst3Plugin::outputs_in_use()`. A host learns from it which
//! of the sixteen outputs carry a slot, and what each slot is called, so it can give each its own
//! track as the user routes it (the owner, 2026-11-19: "if the drum machine opens an individual
//! out, then it makes a group in the daw").
//!
//! - **An output is in use** while a slot that plays is routed to it. A slot whose model is Off or
//!   unavailable sends nothing, so routing one doesn't count until it plays.
//! - **It is named** after the slots on it, as [`slot_names::shown`] names them (`bd1`, or what the
//!   user typed), two sharing one joined with " / ".
//! - **The routing is the slots' Output parameters**, whatever audio configuration is active: a
//!   host on the stereo configuration, where every slot plays on main, still learns it.
//! - **Back to main is each slot on the output set to `L+R`**, playing or not, so nothing is left
//!   routed there: a host whose track for the output goes sets these, in the same undo step.

use std::sync::Arc;

use mxm_part_routing::Destination;
use nice_plug::prelude::{OutputInUse, OutputsInUse};

use crate::SLOT_COUNT;
use crate::slot_names::{self, DrumSlots};

/// A drum plugin's slots, as the outputs read them: its parameters implement it beside
/// [`DrumSlots`].
pub trait DrumOutputs: DrumSlots {
    /// `slot`'s Output parameter, as `Destination::from_parameter` reads it: 0 is main, N is the
    /// slot's own output N.
    fn output(&self, slot: usize) -> i32;
    /// The ID of `slot`'s Output parameter.
    fn output_id(&self, slot: usize) -> &'static str;
}

/// Auxiliary output `aux`, counted from 0, as [`OutputInUse::output`] names it: its place among
/// the outputs, after the main one, so output N is N. nice-plug makes it the CLAP port id (the drum
/// plugins have no audio input) and the VST3 output bus index.
#[must_use]
pub fn aux_output(aux: usize) -> u32 {
    aux as u32 + 1
}

/// The outputs the host is told about.
pub struct HostOutputs<P>(pub Arc<P>);

impl<P: DrumOutputs + Send + Sync> OutputsInUse for HostOutputs<P> {
    fn outputs(&self) -> Vec<OutputInUse> {
        let slots = &*self.0;
        let shown = slot_names::shown(slots);
        let mut on: [Vec<String>; SLOT_COUNT] = std::array::from_fn(|_| Vec::new());
        for (slot, name) in shown.into_iter().enumerate() {
            let routed = Destination::from_parameter(slots.output(slot), SLOT_COUNT);
            if let (Destination::Auxiliary(aux), Some(name), true) =
                (routed, name, slots.plays(slot))
            {
                on[aux].push(name);
            }
        }
        on.into_iter()
            .enumerate()
            .filter(|(_, names)| !names.is_empty())
            .map(|(aux, names)| OutputInUse {
                output: aux_output(aux),
                name: names.join(" / "),
            })
            .collect()
    }

    /// Each slot's Output, and what its name follows: its model (whether it plays) and MIDI
    /// channel. A typed name comes with the dirty-only GUI transaction, which reads them again.
    fn depends_on(&self) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> =
            (0..SLOT_COUNT).map(|slot| self.0.output_id(slot)).collect();
        for id in self.0.name_dependencies() {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// Each slot routed to `output` set to `L+R`, whether it plays or not.
    fn to_main(&self, output: u32) -> Vec<(&'static str, f32)> {
        let slots = &*self.0;
        let main = Destination::Main.parameter_value() as f32;
        (0..SLOT_COUNT)
            .filter(|&slot| {
                matches!(
                    Destination::from_parameter(slots.output(slot), SLOT_COUNT),
                    Destination::Auxiliary(aux) if aux_output(aux) == output
                )
            })
            .map(|slot| (slots.output_id(slot), main))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot_names::tests::Slots;

    const OUTPUT_IDS: [&str; SLOT_COUNT] = [
        "output_1",
        "output_2",
        "output_3",
        "output_4",
        "output_5",
        "output_6",
        "output_7",
        "output_8",
        "output_9",
        "output_10",
        "output_11",
        "output_12",
        "output_13",
        "output_14",
        "output_15",
        "output_16",
    ];

    impl DrumOutputs for Slots {
        fn output(&self, slot: usize) -> i32 {
            self.outputs[slot]
        }
        fn output_id(&self, slot: usize) -> &'static str {
            OUTPUT_IDS[slot]
        }
    }

    fn told(slots: Slots) -> Vec<(u32, String)> {
        HostOutputs(Arc::new(slots))
            .outputs()
            .into_iter()
            .map(|output| (output.output, output.name))
            .collect()
    }

    #[test]
    fn every_slot_on_main_uses_no_output() {
        assert_eq!(told(Slots::new()), []);
    }

    #[test]
    fn each_slot_on_its_own_output_is_named_on_it() {
        let mut slots = Slots::new();
        slots.outputs = std::array::from_fn(|slot| slot as i32 + 1);
        let told = told(slots);
        assert_eq!(told.len(), SLOT_COUNT);
        assert_eq!(told[0], (1, "bd1".to_owned()));
        assert_eq!(told[2], (3, "sn2".to_owned()));
        assert_eq!(told[15], (16, "tm13".to_owned()));
    }

    #[test]
    fn two_slots_on_one_output_name_both_and_a_typed_name_wins() {
        let mut slots = Slots::new();
        slots.outputs[1] = 2;
        slots.outputs[2] = 2;
        slot_names::set(&slots, 2, "rim");
        assert_eq!(told(slots), [(2, "sn1 / rim".to_owned())]);
    }

    #[test]
    fn a_slot_that_does_not_play_uses_no_output() {
        let mut slots = Slots::new();
        slots.outputs[4] = 5;
        slots.plays[4] = false;
        slots.made_up[4] = None;
        assert_eq!(told(slots), []);
    }

    #[test]
    fn the_outputs_follow_the_outputs_and_the_names_once_each() {
        let slots = HostOutputs(Arc::new(Slots::new()));
        let mut expected = OUTPUT_IDS.to_vec();
        expected.extend(["model_1", "midi_channel_1"]);
        assert_eq!(slots.depends_on(), expected);
    }

    #[test]
    fn back_to_main_sets_every_slot_on_the_output_to_l_r_playing_or_not() {
        let mut slots = Slots::new();
        slots.outputs[1] = 2;
        slots.outputs[2] = 2;
        slots.outputs[4] = 2;
        slots.plays[4] = false;
        slots.outputs[5] = 3;
        let slots = HostOutputs(Arc::new(slots));
        assert_eq!(
            slots.to_main(2),
            [("output_2", 0.0), ("output_3", 0.0), ("output_5", 0.0)]
        );
        assert_eq!(slots.to_main(3), [("output_6", 0.0)]);
        assert_eq!(slots.to_main(1), [], "nothing on output 1");
        assert_eq!(slots.to_main(0), [], "main is no output of its own");
    }
}
