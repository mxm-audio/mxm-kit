//! Audio readers: the right samples on a worker thread, and the reads cancelled and joined when
//! the host disables access or destroys the source.

mod common;

use std::time::Duration;

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{HostConfig, HostEvent};
use mxm_ara::*;

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

fn samples_read(ext: &PlugInExtension<TestController>, id: AudioSourceId) -> Option<Vec<Vec<f32>>> {
    ext.with_controller(|_, g| {
        g.audio_source(id)
            .unwrap()
            .data
            .samples
            .lock()
            .unwrap()
            .clone()
    })
    .unwrap()
}

#[test]
fn a_worker_reads_the_right_samples_in_both_formats() {
    for sixty_four in [false, true] {
        let host = host(HostConfig::default());
        set(Settings {
            sixty_four,
            ..Settings::default()
        });
        let doc = host.create_document("song").unwrap();
        let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
        let spec = source_spec("take-1", 10_007);
        let src = doc.edit(|d| d.create_audio_source(&spec));
        let id = AudioSourceId::from_ref(src.plugin_ref);
        doc.enable_samples_access(&src, true);
        wait_for("the read", || samples_read(&ext, id).is_some());
        let read = samples_read(&ext, id).unwrap();
        assert_eq!(
            read, spec.channels,
            "every sample of every channel, 64-bit {sixty_four}"
        );
        // The control: the comparison sees one sample's shift.
        let mut shifted = spec.channels.clone();
        shifted[1].rotate_left(1);
        assert_ne!(read, shifted);

        let stats = host.source_stats(&src);
        assert_eq!(stats.reads_started, 11, "10 007 frames in chunks of 1000");
        assert!(
            !stats.reading_threads.contains(&std::thread::current().id()),
            "read on a worker, not the model thread"
        );

        // Notifications wait for the host's poll: started first, completed last, then the content.
        doc.enable_samples_access(&src, false);
        assert!(
            host.take_events()
                .iter()
                .all(|e| !matches!(e, HostEvent::AnalysisProgress { .. }))
        );
        doc.notify_model_updates();
        let events = host.take_events();
        let progress: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                HostEvent::AnalysisProgress { state, value, .. } => Some((*state, *value)),
                _ => None,
            })
            .collect();
        assert_eq!(
            progress.first().map(|p| p.0),
            Some(sys::kARAAnalysisProgressStarted),
            "{progress:?}"
        );
        assert_eq!(
            progress.last(),
            Some(&(sys::kARAAnalysisProgressCompleted, 1.0)),
            "{progress:?}"
        );
        assert!(
            progress.windows(2).all(|w| w[0].1 <= w[1].1),
            "progress only grows: {progress:?}"
        );
        assert!(
            matches!(events.last(), Some(HostEvent::AudioSourceContentChanged { source, .. }) if *source == src.host_ref)
        );
        doc.edit(|d| d.destroy_audio_source(&src));
        drop(ext);
        assert!(host.violations().is_empty(), "{:?}", host.violations());
        assert!(host.asserts().is_empty(), "{:?}", host.asserts());
    }
}

#[test]
fn disabling_access_joins_the_read_under_way_before_returning() {
    let host = host(HostConfig {
        read_delay: Duration::from_millis(40),
        ..HostConfig::default()
    });
    set(Settings {
        chunk: 4410,
        ..Settings::default()
    });
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let src = doc.edit(|d| d.create_audio_source(&source_spec("take-1", 44_100)));
    let id = AudioSourceId::from_ref(src.plugin_ref);
    doc.enable_samples_access(&src, true);

    // The control: the host sees a read under way.
    wait_for("a read under way", || {
        host.source_stats(&src).reads_in_flight == 1
    });
    doc.enable_samples_access(&src, false);
    let after = host.source_stats(&src);
    assert_eq!(
        after.reads_in_flight, 0,
        "the read was joined before the call returned"
    );
    assert_eq!(after.live_readers, 0, "and its reader destroyed");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        host.source_stats(&src).reads_started,
        after.reads_started,
        "no read after"
    );
    assert_eq!(samples_read(&ext, id), None, "the worker stopped");
    assert!(host.violations().is_empty(), "{:?}", host.violations());

    // Enabled again, it reads again, from the start.
    doc.enable_samples_access(&src, true);
    wait_for("the read", || samples_read(&ext, id).is_some());
    doc.enable_samples_access(&src, false);
    doc.edit(|d| d.destroy_audio_source(&src));
    assert!(host.violations().is_empty(), "{:?}", host.violations());
}

#[test]
fn destroying_a_source_while_it_is_read_joins_the_read() {
    let host = host(HostConfig {
        read_delay: Duration::from_millis(40),
        ..HostConfig::default()
    });
    set(Settings {
        chunk: 4410,
        ..Settings::default()
    });
    let doc = host.create_document("song").unwrap();
    let src = doc.edit(|d| d.create_audio_source(&source_spec("take-1", 44_100)));
    doc.enable_samples_access(&src, true);
    wait_for("a read under way", || {
        host.source_stats(&src).reads_in_flight == 1
    });
    // ARA doesn't require disabling access before destroying a source.
    doc.edit(|d| d.destroy_audio_source(&src));
    let after = host.source_stats(&src);
    assert_eq!((after.reads_in_flight, after.live_readers), (0, 0));
    assert!(host.violations().is_empty(), "{:?}", host.violations());
}
