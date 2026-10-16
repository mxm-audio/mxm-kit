//! A plugin's VST3 identity, held to its CLAP one: the class ID is the CLAP id's hash, and the
//! subcategories follow the CLAP features by one table.
//!
//! The plugin writes its VST3 class ID out as sixteen bytes, because a host keys every saved
//! project on it and the shipped code should hold the permanent value, not a hash function. The
//! check is what makes the choice mechanical: [`vst3_class_id`] of the plugin's `CLAP_ID`. A rename
//! that moves the CLAP id fails it, which forces the same decision the CLAP id already needs.

use nice_plug::prelude::{AudioIOLayout, ClapFeature, ClapPlugin, Vst3Plugin, Vst3SubCategory};

/// The VST3 class ID an MXM plugin takes: the 128-bit FNV-1a hash of its CLAP id, big-endian.
/// FNV-1a is Fowler, Noll and Vo's; the offset basis and prime are the published 128-bit ones.
pub const fn vst3_class_id(clap_id: &str) -> [u8; 16] {
    let bytes = clap_id.as_bytes();
    let mut hash: u128 = 0x6c62272e07bb014262b821756295c58d;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u128;
        hash = hash.wrapping_mul(0x0000000001000000000000000000013b);
        i += 1;
    }
    hash.to_be_bytes()
}

/// Panics unless `P::VST3_CLASS_ID` is [`vst3_class_id`] of `P::CLAP_ID`. Its message prints the
/// bytes to write.
pub fn the_vst3_class_id_is_the_clap_ids_hash<P: ClapPlugin + Vst3Plugin>() {
    let wanted = vst3_class_id(P::CLAP_ID);
    assert_eq!(
        P::VST3_CLASS_ID,
        wanted,
        "{}'s VST3_CLASS_ID is not its CLAP id's hash; write {}",
        P::CLAP_ID,
        bytes_literal(&wanted)
    );
}

/// The VST3 subcategory a CLAP feature stands for, or `None` for one with no row (and for `Mono`
/// and `Stereo`, which the layouts decide instead).
pub fn vst3_subcategory(feature: &ClapFeature) -> Option<Vst3SubCategory> {
    Some(match feature {
        ClapFeature::Instrument => Vst3SubCategory::Instrument,
        ClapFeature::AudioEffect => Vst3SubCategory::Fx,
        ClapFeature::Synthesizer => Vst3SubCategory::Synth,
        ClapFeature::Sampler => Vst3SubCategory::Sampler,
        ClapFeature::Drum | ClapFeature::DrumMachine => Vst3SubCategory::Drum,
        ClapFeature::Delay => Vst3SubCategory::Delay,
        ClapFeature::Reverb => Vst3SubCategory::Reverb,
        ClapFeature::Chorus | ClapFeature::Flanger | ClapFeature::Phaser => {
            Vst3SubCategory::Modulation
        }
        ClapFeature::Distortion => Vst3SubCategory::Distortion,
        ClapFeature::Equalizer => Vst3SubCategory::Eq,
        ClapFeature::Compressor | ClapFeature::Limiter => Vst3SubCategory::Dynamics,
        ClapFeature::Filter => Vst3SubCategory::Filter,
        _ => return None,
    })
}

/// The subcategories a plugin with these CLAP features and layouts declares: each feature's
/// [`vst3_subcategory`] in the features' order, without duplicates, then `Mono` or `Stereo` when
/// every layout is that shape and only then (VST3 reads either as "only this").
pub fn vst3_subcategories(
    features: &[ClapFeature],
    layouts: &[AudioIOLayout],
) -> Vec<&'static str> {
    let mut wanted: Vec<&'static str> = Vec::new();
    for category in features.iter().filter_map(vst3_subcategory) {
        if !wanted.contains(&category.as_str()) {
            wanted.push(category.as_str());
        }
    }
    let every = |channels: u32| {
        !layouts.is_empty()
            && layouts.iter().all(|layout| {
                layout.main_output_channels.map(|c| c.get()) == Some(channels)
                    && layout
                        .main_input_channels
                        .is_none_or(|c| c.get() == channels)
            })
    };
    if every(1) {
        wanted.push(Vst3SubCategory::Mono.as_str());
    } else if every(2) {
        wanted.push(Vst3SubCategory::Stereo.as_str());
    }
    wanted
}

/// Panics unless `P::VST3_SUBCATEGORIES` is exactly [`vst3_subcategories`] of the plugin's
/// `CLAP_FEATURES` and `AUDIO_IO_LAYOUTS`, in that order, and begins with a main category.
pub fn the_vst3_categories_follow_the_clap_features<P: ClapPlugin + Vst3Plugin>() {
    let declared: Vec<&str> = P::VST3_SUBCATEGORIES.iter().map(|c| c.as_str()).collect();
    let wanted = vst3_subcategories(P::CLAP_FEATURES, P::AUDIO_IO_LAYOUTS);
    assert_eq!(
        declared,
        wanted,
        "{}'s VST3_SUBCATEGORIES don't follow its CLAP_FEATURES and AUDIO_IO_LAYOUTS",
        P::CLAP_ID
    );
    let main = [
        Vst3SubCategory::Fx.as_str(),
        Vst3SubCategory::Instrument.as_str(),
    ];
    assert!(
        declared.first().is_some_and(|first| main.contains(first)),
        "{}'s VST3 subcategories begin with {:?}, not Fx or Instrument",
        P::CLAP_ID,
        declared.first()
    );
}

fn bytes_literal(bytes: &[u8; 16]) -> String {
    let listed: Vec<String> = bytes.iter().map(|b| format!("0x{b:02x}")).collect();
    format!("[{}]", listed.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn layout(input: Option<u32>, output: u32) -> AudioIOLayout {
        AudioIOLayout {
            main_input_channels: input.and_then(NonZeroU32::new),
            main_output_channels: NonZeroU32::new(output),
            ..AudioIOLayout::const_default()
        }
    }

    /// mxm-mono-00's, the first one shipped: the id Bitwig and newDAWn registered for it
    /// (`vst3:5C2F4F956A769870329BBEED54F37F2F`, 2026-10-16). It never changes.
    #[test]
    fn the_hash_gives_the_class_id_mono_00_shipped_with() {
        assert_eq!(
            vst3_class_id("dk.mxm.mxm-mono-00"),
            [
                0x5c, 0x2f, 0x4f, 0x95, 0x6a, 0x76, 0x98, 0x70, 0x32, 0x9b, 0xbe, 0xed, 0x54, 0xf3,
                0x7f, 0x2f
            ]
        );
    }

    #[test]
    fn an_instrument_playing_mono_and_stereo_names_neither() {
        let features = [
            ClapFeature::Instrument,
            ClapFeature::Synthesizer,
            ClapFeature::Stereo,
            ClapFeature::Mono,
        ];
        let layouts = [layout(None, 2), layout(None, 1)];
        assert_eq!(
            vst3_subcategories(&features, &layouts),
            ["Instrument", "Synth"]
        );
    }

    #[test]
    fn a_stereo_only_effect_says_stereo_and_a_mono_only_one_mono() {
        let features = [
            ClapFeature::AudioEffect,
            ClapFeature::Chorus,
            ClapFeature::Stereo,
        ];
        assert_eq!(
            vst3_subcategories(&features, &[layout(Some(2), 2)]),
            ["Fx", "Modulation", "Stereo"]
        );
        assert_eq!(
            vst3_subcategories(&features, &[layout(Some(1), 1)]),
            ["Fx", "Modulation", "Mono"]
        );
        // Mono in, stereo out is neither shape.
        assert_eq!(
            vst3_subcategories(&features, &[layout(Some(1), 2)]),
            ["Fx", "Modulation"]
        );
    }

    #[test]
    fn features_naming_one_category_list_it_once_and_those_with_no_row_add_nothing() {
        let features = [
            ClapFeature::AudioEffect,
            ClapFeature::MultiEffects,
            ClapFeature::Compressor,
            ClapFeature::Limiter,
            ClapFeature::Distortion,
        ];
        let layouts = [layout(Some(1), 1), layout(Some(2), 2)];
        assert_eq!(
            vst3_subcategories(&features, &layouts),
            ["Fx", "Dynamics", "Distortion"]
        );
    }
}
