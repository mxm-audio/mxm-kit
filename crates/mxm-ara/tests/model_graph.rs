//! The model graph through the host's edits; roles and binding; the editor view; content both
//! ways; calls ARA calls invalid.

mod common;

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{HostConfig, RegionSpec, TestExtension};
use mxm_ara::*;

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

fn spec(start: f64, name: &str) -> RegionSpec {
    RegionSpec {
        start_in_modification: 0.0,
        duration_in_modification: 1.0,
        start_in_playback: start,
        duration_in_playback: 1.0,
        name: Some(name.into()),
    }
}

#[test]
fn the_graph_follows_the_hosts_edits_and_teardown() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let (ctx, lead, double, src, m, r) = doc.edit(|d| {
        let ctx = d.create_musical_context("song", d_major());
        let lead = d.create_region_sequence("lead", 0, &ctx);
        let double = d.create_region_sequence("double", 1, &ctx);
        let src = d.create_audio_source(&source_spec("take-1", 44_100));
        let m = d.create_audio_modification(&src, "mod-1", "lead edits");
        let r = d.create_playback_region(&m, &lead, &spec(2.0, "verse"));
        (ctx, lead, double, src, m, r)
    });
    let (src_id, m_id, r_id) = (
        AudioSourceId::from_ref(src.plugin_ref),
        AudioModificationId::from_ref(m.plugin_ref),
        PlaybackRegionId::from_ref(r.plugin_ref),
    );
    let (ctx_id, lead_id, double_id) = (
        MusicalContextId::from_ref(ctx.plugin_ref),
        RegionSequenceId::from_ref(lead.plugin_ref),
        RegionSequenceId::from_ref(double.plugin_ref),
    );
    ext.with_controller(|c, g| {
        assert_eq!(g.document().name.as_deref(), Some("song"));
        let s = g.audio_source(src_id).expect("the source");
        assert_eq!(s.persistent_id(), "take-1");
        assert_eq!(
            (
                s.properties().sample_rate,
                s.properties().channel_count,
                s.properties().sample_count
            ),
            (44_100.0, 2, 44_100)
        );
        assert!(!s.is_sample_access_enabled(), "access starts disabled");
        assert_eq!(s.audio_modifications(), [m_id]);
        assert_eq!(
            g.audio_modification(m_id).unwrap().playback_regions(),
            [r_id]
        );
        let region = g.playback_region(r_id).unwrap();
        assert_eq!(
            (region.audio_modification(), region.audio_source()),
            (m_id, src_id)
        );
        assert_eq!(region.properties().start_in_playback_time, 2.0);
        assert_eq!(region.properties().name.as_deref(), Some("verse"));
        assert_eq!(region.region_sequence(), Some(lead_id));
        assert_eq!(g.musical_context_of(r_id), Some(ctx_id));
        assert_eq!(
            g.region_sequences().map(|s| s.id()).collect::<Vec<_>>(),
            [lead_id, double_id]
        );
        assert_eq!(
            g.musical_context(ctx_id).unwrap().region_sequences(),
            [lead_id, double_id]
        );
        assert_eq!(
            c.key.as_ref().map(|k| (k.root, k.root_pitch_class())),
            Some((2, 2)),
            "the song's key, D"
        );
    });

    // Move the region to the other track and later in the song; reorder the tracks.
    doc.edit(|d| {
        d.update_playback_region(&r, &double, &spec(4.0, "verse"));
        d.update_region_sequence(&lead, "lead", 5, &ctx);
        d.update_document_name("song 2");
    });
    ext.with_controller(|_, g| {
        assert_eq!(
            g.playback_region(r_id).unwrap().region_sequence(),
            Some(double_id)
        );
        assert_eq!(
            g.playback_region(r_id)
                .unwrap()
                .properties()
                .start_in_playback_time,
            4.0
        );
        assert!(
            g.region_sequence(lead_id)
                .unwrap()
                .playback_regions()
                .is_empty()
        );
        assert_eq!(
            g.region_sequence(double_id).unwrap().playback_regions(),
            [r_id]
        );
        assert_eq!(
            g.region_sequences().map(|s| s.id()).collect::<Vec<_>>(),
            [double_id, lead_id],
            "sorted by order index"
        );
        assert_eq!(g.document().name.as_deref(), Some("song 2"));
    });

    doc.edit(|d| {
        d.destroy_playback_region(&r);
        d.destroy_audio_modification(&m);
        d.destroy_audio_source(&src);
        d.destroy_region_sequence(&lead);
        d.destroy_region_sequence(&double);
        d.destroy_musical_context(&ctx);
    });
    ext.with_controller(|_, g| {
        assert!(g.audio_source(src_id).is_none() && g.playback_region(r_id).is_none());
        assert_eq!(
            g.audio_sources().count() + g.region_sequences().count() + g.musical_contexts().count(),
            0
        );
    });
    let log = take_log();
    for expected in [
        "begin editing",
        "will end editing, graph exclusive true",
        "did end editing, graph exclusive false",
        "region Some(\"verse\") at 2",
        "region moved to 4",
        "document Some(\"song 2\")",
        "destroy region",
        "destroy modification",
        "destroy source",
        "destroy context",
    ] {
        assert!(
            log.iter().any(|l| l == expected),
            "missing {expected:?} in {log:#?}"
        );
    }
    assert!(host.asserts().is_empty(), "{:?}", host.asserts());
    assert!(host.violations().is_empty(), "{:?}", host.violations());
}

#[test]
fn invalid_calls_are_reported_and_refused_without_harm() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let (ctx, src, m) = doc.edit(|d| {
        let ctx = d.create_musical_context("song", d_major());
        let src = d.create_audio_source(&source_spec("take-1", 1000));
        let m = d.create_audio_modification(&src, "mod-1", "edits");
        (ctx, src, m)
    });
    assert!(
        host.asserts().is_empty(),
        "the control: valid calls report nothing"
    );

    // A source with a modification can't go; it stays, whole.
    doc.edit(|d| d.destroy_audio_source(&src));
    assert!(
        host.asserts()
            .iter()
            .any(|(_, d)| d.contains("before its audio modifications")),
        "{:?}",
        host.asserts()
    );
    let src_id = AudioSourceId::from_ref(src.plugin_ref);
    assert!(
        ext.with_controller(|_, g| g.audio_source(src_id).is_some())
            .unwrap()
    );

    // A ref the controller never made isn't followed.
    let fake = mxm_ara::test_host::Source {
        plugin_ref: 0x1000 as sys::ARAAudioSourceRef,
        host_ref: 0,
    };
    doc.edit(|d| d.destroy_audio_source(&fake));
    assert!(
        host.asserts()
            .iter()
            .any(|(_, d)| d == "unknown audio source")
    );

    // An edit outside beginEditing/endEditing is reported, and still made with renderers out.
    let late = doc.create_audio_modification(&src, "mod-late", "late");
    assert!(
        host.asserts()
            .iter()
            .any(|(_, d)| d.contains("outside beginEditing"))
    );
    assert!(
        ext.with_controller(|_, g| g
            .audio_modification(AudioModificationId::from_ref(late.plugin_ref))
            .is_some())
            .unwrap()
    );

    doc.edit(|d| {
        d.destroy_audio_modification(&late);
        d.destroy_audio_modification(&m);
        d.destroy_audio_source(&src);
        d.destroy_musical_context(&ctx);
    });
}

#[test]
fn roles_are_the_hosts_and_an_instance_binds_once() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let playback = sys::kARAPlaybackRendererRole as u32;

    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, playback).unwrap();
    // SAFETY: the instance lives as long as `ext`.
    let seen = unsafe { TestExtension::new(ext.instance()) };
    assert!(seen.has_playback_renderer() && !seen.has_editor_renderer() && !seen.has_editor_view());
    assert_eq!(
        seen.struct_size(),
        std::mem::size_of::<sys::ARAPlugInExtensionInstance>()
    );

    // The control: every role assigned, every interface given.
    let all = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let seen = unsafe { TestExtension::new(all.instance()) };
    assert!(seen.has_playback_renderer() && seen.has_editor_renderer() && seen.has_editor_view());

    // A host that knows no roles gets them all, as ARA_Library gives them.
    let legacy = PlugInExtension::<TestController>::bind(doc.controller_ref(), 0, 0).unwrap();
    assert_eq!(
        legacy.roles(),
        Roles::PLAYBACK_RENDERER | Roles::EDITOR_RENDERER | Roles::EDITOR_VIEW
    );

    // `bind_into` binds an instance once.
    let mut slot = None;
    assert!(!bind_into::<TestController>(&mut slot, doc.controller_ref(), ALL, ALL).is_null());
    assert!(bind_into::<TestController>(&mut slot, doc.controller_ref(), ALL, ALL).is_null());
    assert!(
        host.asserts()
            .iter()
            .any(|(_, d)| d == "plug-in instance bound twice")
    );

    // A controller that isn't this plug-in's is refused.
    assert!(PlugInExtension::<TestController>::bind(0x1000 as *const _, ALL, ALL).is_none());
}

#[test]
fn regions_reach_renderers_and_the_view_and_leave_them_when_destroyed() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    // SAFETY: the instance lives as long as `ext`.
    let seen = unsafe { TestExtension::new(ext.instance()) };
    let (ctx, seq, src, m, a, b) = doc.edit(|d| {
        let ctx = d.create_musical_context("song", d_major());
        let seq = d.create_region_sequence("lead", 0, &ctx);
        let src = d.create_audio_source(&source_spec("take-1", 1000));
        let m = d.create_audio_modification(&src, "mod-1", "edits");
        let a = d.create_playback_region(&m, &seq, &spec(0.0, "a"));
        let b = d.create_playback_region(&m, &seq, &spec(1.0, "with tail"));
        (ctx, seq, src, m, a, b)
    });
    seen.add_playback_region(&a);
    seen.add_playback_region(&b);
    seen.add_editor_region(&a);
    seen.add_editor_sequence(&seq);
    // The controller sees what the editor renderer was given, as it is given (the log put back
    // for the counts below).
    let so_far = take_log();
    assert!(
        so_far.contains(&"editor renderer Some((1, 0))".to_string()),
        "{so_far:?}"
    );
    assert!(
        so_far.contains(&"editor renderer Some((1, 1))".to_string()),
        "{so_far:?}"
    );
    for line in so_far {
        log(line);
    }
    seen.notify_selection(&[b, a], &[seq], Some((0.5, 1.0)));
    seen.hide_region_sequences(&[seq]);
    let names = |ext: &PlugInExtension<TestController>| -> Vec<String> {
        let access = ext.render_access().expect("no edit under way");
        access
            .playback_regions()
            .map(|r| r.properties().name.clone().unwrap())
            .collect()
    };
    assert_eq!(names(&ext), ["a", "with tail"]);
    let (a_id, b_id) = (
        PlaybackRegionId::from_ref(a.plugin_ref),
        PlaybackRegionId::from_ref(b.plugin_ref),
    );
    assert_eq!(
        ext.selection().playback_regions,
        [b_id, a_id],
        "in the host's order of importance"
    );
    assert_eq!(
        ext.selection().time_range,
        Some(TimeRange {
            start: 0.5,
            duration: 1.0
        })
    );
    assert_eq!(
        ext.hidden_region_sequences(),
        [RegionSequenceId::from_ref(seq.plugin_ref)]
    );
    assert_eq!(doc.head_and_tail_time(&b), (0.0, 0.5));
    assert_eq!(doc.head_and_tail_time(&a), (0.0, 0.0));

    seen.remove_playback_region(&b);
    assert_eq!(names(&ext), ["a"]);
    // Destroying a region a renderer still has is the host's error; it leaves every list anyway.
    doc.edit(|d| d.destroy_playback_region(&a));
    assert!(
        host.asserts()
            .iter()
            .any(|(_, d)| d.contains("a renderer still has it"))
    );
    assert_eq!(names(&ext), Vec::<String>::new());
    assert_eq!(
        ext.selection().playback_regions,
        [b_id],
        "a destroyed region leaves the selection"
    );
    doc.edit(|d| {
        d.destroy_playback_region(&b);
        d.destroy_audio_modification(&m);
        d.destroy_audio_source(&src);
        d.destroy_region_sequence(&seq);
        d.destroy_musical_context(&ctx);
    });
    assert!(ext.hidden_region_sequences().is_empty());
    assert!(
        ext.selection().playback_regions.is_empty() && ext.selection().region_sequences.is_empty()
    );
    let log = take_log();
    assert_eq!(log.iter().filter(|l| *l == "renderer +region").count(), 2);
    assert_eq!(log.iter().filter(|l| *l == "renderer -region").count(), 1);
    assert!(log.contains(&"selection 2 regions".to_string()));
}

#[test]
fn the_song_key_is_read_where_ara_allows_it() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ctx = doc.edit(|d| {
        let ctx = d.create_musical_context("song", d_major());
        let src = d.create_audio_source(&source_spec("take-1", 10));
        d.destroy_audio_source(&src);
        ctx
    });
    let log = take_log();
    assert!(
        log.contains(&"context keys Some(1) tempo Some(2)".to_string()),
        "{log:#?}"
    );
    // The control: the same read in a call about an audio source is refused.
    assert!(
        log.contains(&"source take-1, key readable here false".to_string()),
        "{log:#?}"
    );

    let mut e_major = d_major();
    e_major.keys[0].root = 4;
    host.set_context_content(&ctx, e_major);
    doc.edit(|d| d.update_musical_context_content(&ctx));
    assert!(take_log().contains(&"context content updated, key root Some(4)".to_string()));
    doc.edit(|d| d.destroy_musical_context(&ctx));
    assert!(host.violations().is_empty(), "{:?}", host.violations());
}

#[test]
fn content_the_plug_in_offers_is_read_by_the_host() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let src = doc.edit(|d| d.create_audio_source(&source_spec("take-1", 44_100)));
    // The control: before the analysis, no notes.
    assert_eq!(doc.read_audio_source_content::<Note>(&src), None);
    doc.enable_samples_access(&src, true);
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let id = AudioSourceId::from_ref(src.plugin_ref);
    wait_for("the analysis", || {
        ext.with_controller(|_, g| {
            g.audio_source(id)
                .unwrap()
                .data
                .samples
                .lock()
                .unwrap()
                .is_some()
        })
        .unwrap()
    });
    let notes = doc
        .read_audio_source_content::<Note>(&src)
        .expect("notes once analysed");
    assert_eq!(notes.len(), 1);
    assert_eq!((notes[0].pitch_number, notes[0].note_duration), (69, 1.0));
    // A type the controller doesn't offer.
    assert_eq!(doc.read_audio_source_content::<TempoEntry>(&src), None);
    doc.enable_samples_access(&src, false);
    doc.edit(|d| d.destroy_audio_source(&src));
    assert!(host.asserts().is_empty(), "{:?}", host.asserts());
}
