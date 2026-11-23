//! The factory as a host reads it, and the API generations it accepts.

mod common;

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{HostConfig, TestHost};
use mxm_ara::*;

#[test]
fn the_factory_tells_the_host_what_its_config_says() {
    let host = TestHost::of::<TestController>(HostConfig::default());
    let info = host.factory_info();
    assert_eq!(info.plug_in_name, CONFIG.plug_in_name);
    assert_eq!(info.manufacturer_name, CONFIG.manufacturer_name);
    assert_eq!(info.information_url, CONFIG.information_url);
    assert_eq!(info.version, CONFIG.version);
    assert_eq!(info.factory_id, CONFIG.factory_id);
    assert_eq!(info.document_archive_id, CONFIG.document_archive_id);
    assert_eq!(
        info.compatible_document_archive_ids,
        vec!["dk.mxm.test-ara.archive.0".to_string()]
    );
    assert_eq!(
        info.analyzeable_content_types,
        vec![sys::kARAContentTypeNotes]
    );
    assert_eq!(
        info.supported_playback_transformation_flags,
        sys::kARAPlaybackTransformationNoChanges
    );
    assert_eq!(info.supports_storing_audio_file_chunks, Some(false));
    // The whole 2.3 struct is implemented: its size reaches the last field.
    assert_eq!(info.struct_size, std::mem::size_of::<sys::ARAFactory>());
    assert_eq!(
        (info.lowest_api_generation, info.highest_api_generation),
        (
            sys::kARAAPIGeneration_2_0_Final,
            sys::kARAAPIGeneration_2_3_Final
        )
    );
    // The same pointer every time, as the companion APIs require.
    assert_eq!(factory::<TestController>(), factory::<TestController>());
    assert!(CONFIG.problems().is_empty(), "{:?}", CONFIG.problems());
}

#[test]
fn a_config_ara_would_reject_is_named() {
    let bad = FactoryConfig {
        factory_id: "x",
        plug_in_name: "",
        supported_playback_transformation_flags:
            sys::kARAPlaybackTransformationContentBasedFadeAtHead,
        ..CONFIG
    };
    let problems = bad.problems();
    assert_eq!(problems.len(), 3, "{problems:?}");
}

#[test]
fn every_supported_generation_initializes_and_others_are_refused() {
    for generation in [
        sys::kARAAPIGeneration_2_0_Final,
        sys::kARAAPIGeneration_2_X_Draft,
        sys::kARAAPIGeneration_2_3_Final,
    ] {
        let host = TestHost::of::<TestController>(HostConfig::default());
        host.initialize(generation);
        assert_eq!(api_generation::<TestController>(), Some(generation));
        let doc = host
            .create_document("song")
            .expect("a document at a supported generation");
        assert_eq!(doc.get_factory(), factory::<TestController>());
        drop(doc);
        assert!(take_log().contains(&format!("new Some(\"song\") generation {generation}")));
        assert!(host.asserts().is_empty(), "{:?}", host.asserts());
    }
    // ARA 2.0 draft (and ARA 1) are older than this library serves: no controller.
    {
        let host = TestHost::of::<TestController>(HostConfig::default());
        host.initialize(sys::kARAAPIGeneration_2_0_Draft);
        assert_eq!(api_generation::<TestController>(), None);
        assert!(host.create_document("song").is_none());
        assert!(
            host.asserts()
                .iter()
                .any(|(c, _)| *c == sys::kARAAssertInvalidArgument),
            "the refusal is reported"
        );
    }
    // So is a document before initialization.
    let host = TestHost::of::<TestController>(HostConfig::default());
    assert!(host.create_document("song").is_none());
}
