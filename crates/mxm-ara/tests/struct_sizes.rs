//! ARA's versioned structs: an older host's smaller structs are read, and its interfaces called,
//! only as far as their `structSize` goes.

mod common;

use std::ffi::CString;

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{HostConfig, HostEvent, RegionSpec, TestDocument};
use mxm_ara::*;

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

fn document_data_sent(config: HostConfig) -> bool {
    let host = host(config);
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    ext.with_controller(|c, _| c.notifier.document_data_changed());
    doc.notify_model_updates();
    let sent = host.take_events().contains(&HostEvent::DocumentDataChanged);
    assert!(host.violations().is_empty(), "{:?}", host.violations());
    sent
}

#[test]
fn an_ara_2_0_hosts_model_update_controller_is_called_only_as_far_as_it_goes() {
    // The control: a 2.3 host hears that the document's data changed.
    assert!(document_data_sent(HostConfig::default()));
    // A 2.0 host's struct ends before `notifyDocumentDataChanged`; its slot (filled in by the
    // test host all the same) is never called.
    assert!(!document_data_sent(HostConfig::ara_2_0()));
}

#[test]
fn an_older_hosts_smaller_region_properties_are_read_only_as_far_as_they_go() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let (ctx, seq, src, m) = doc.edit(|d| {
        let ctx = d.create_musical_context("song", d_major());
        let seq = d.create_region_sequence("lead", 0, &ctx);
        let src = d.create_audio_source(&source_spec("take-1", 100));
        let m = d.create_audio_modification(&src, "mod-1", "edits");
        (ctx, seq, src, m)
    });
    // Past ARA 1's struct (it ends at `musicalContextRef`) lie a sequence and a name the host
    // never meant to send.
    let stale = CString::new("STALE").unwrap();
    let spec = RegionSpec {
        start_in_playback: 3.0,
        duration_in_playback: 1.0,
        duration_in_modification: 1.0,
        ..RegionSpec::default()
    };
    let mut p = TestDocument::region_properties(&spec, &seq, Some(&stale));
    p.structSize = sys::kARAPlaybackRegionPropertiesMinSize as usize;
    let small = doc.edit(|d| d.create_playback_region_with(&m, &p));
    // The control: the same struct at its full size carries them.
    p.structSize = std::mem::size_of::<sys::ARAPlaybackRegionProperties>();
    let full = doc.edit(|d| d.create_playback_region_with(&m, &p));

    let read = |r: &mxm_ara::test_host::Region| {
        let id = PlaybackRegionId::from_ref(r.plugin_ref);
        ext.with_controller(|_, g| {
            let p = g.playback_region(id).unwrap().properties();
            (p.start_in_playback_time, p.name.clone(), p.region_sequence)
        })
        .unwrap()
    };
    let seq_id = RegionSequenceId::from_ref(seq.plugin_ref);
    assert_eq!(read(&small), (3.0, None, None));
    assert_eq!(read(&full), (3.0, Some("STALE".into()), Some(seq_id)));
    // ARA 2 requires the sequence, so the small struct is reported, once.
    let missing = host
        .asserts()
        .iter()
        .filter(|(_, d)| d.contains("without a region sequence"))
        .count();
    assert_eq!(missing, 1, "{:?}", host.asserts());

    doc.edit(|d| {
        d.destroy_playback_region(&small);
        d.destroy_playback_region(&full);
        d.destroy_audio_modification(&m);
        d.destroy_audio_source(&src);
        d.destroy_region_sequence(&seq);
        d.destroy_musical_context(&ctx);
    });
}

fn restore_reports_a_foreign_archive_id(config: HostConfig) -> bool {
    let host = host(config);
    let doc = host.create_document("song").unwrap();
    let src = doc.edit(|d| d.create_audio_source(&source_spec("take-1", 10)));
    let archive = doc.store(None).unwrap();
    assert!(doc.edit(|d| d.restore(&archive, None)));
    doc.edit(|d| d.destroy_audio_source(&src));
    host.asserts()
        .iter()
        .any(|(_, d)| d.contains("document archive ID this factory doesn't read"))
}

#[test]
fn the_archive_id_is_checked_only_where_the_host_offers_it() {
    let foreign = HostConfig {
        document_archive_id: Some("com.example.other.archive".into()),
        ..HostConfig::default()
    };
    // The control: a host that offers `getDocumentArchiveID` and names a foreign ID is reported.
    assert!(restore_reports_a_foreign_archive_id(foreign.clone()));
    // A host whose struct predates it is never asked.
    assert!(!restore_reports_a_foreign_archive_id(
        foreign.without_archive_id()
    ));
    // Our own and our compatible IDs are fine.
    assert!(!restore_reports_a_foreign_archive_id(HostConfig::default()));
    let compatible = HostConfig {
        document_archive_id: Some("dk.mxm.test-ara.archive.0".into()),
        ..HostConfig::default()
    };
    assert!(!restore_reports_a_foreign_archive_id(compatible));
}
