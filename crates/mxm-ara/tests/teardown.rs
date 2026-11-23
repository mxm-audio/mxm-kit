//! Both teardown orders: the plug-in instance before the document controller, and after it.

mod common;

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{
    Context, HostConfig, Modification, Region, RegionSpec, Sequence, Source, TestDocument,
    TestExtension,
};
use mxm_ara::*;

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

fn build(doc: &TestDocument<'_>) -> (Context, Sequence, Source, Modification, Region) {
    doc.edit(|d| {
        let ctx = d.create_musical_context("song", d_major());
        let seq = d.create_region_sequence("lead", 0, &ctx);
        let src = d.create_audio_source(&source_spec("take-1", 100));
        let m = d.create_audio_modification(&src, "mod-1", "edits");
        let spec = RegionSpec {
            duration_in_modification: 1.0,
            duration_in_playback: 1.0,
            ..RegionSpec::default()
        };
        let r = d.create_playback_region(&m, &seq, &spec);
        (ctx, seq, src, m, r)
    })
}

fn tear(
    doc: &TestDocument<'_>,
    (ctx, seq, src, m, r): (Context, Sequence, Source, Modification, Region),
) {
    doc.edit(|d| {
        d.destroy_playback_region(&r);
        d.destroy_audio_modification(&m);
        d.destroy_audio_source(&src);
        d.destroy_region_sequence(&seq);
        d.destroy_musical_context(&ctx);
    });
}

#[test]
fn the_instance_may_go_before_the_document_controller() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let objects = build(&doc);
    // SAFETY: used while `ext` lives.
    let seen = unsafe { TestExtension::new(ext.instance()) };
    seen.add_playback_region(&objects.4);
    assert_eq!(
        ext.render_access().map(|a| a.playback_regions().count()),
        Some(1)
    );
    drop(ext);
    assert!(
        take_log().contains(&"unbind".to_string()),
        "the controller hears the instance go"
    );
    tear(&doc, objects);
    doc.destroy();
    assert!(take_log().contains(&"controller dropped".to_string()));
    assert!(host.asserts().is_empty(), "{:?}", host.asserts());
    assert!(host.violations().is_empty(), "{:?}", host.violations());
}

#[test]
fn the_document_controller_may_go_before_the_instance() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let objects = build(&doc);
    // SAFETY: used while `ext` lives.
    let seen = unsafe { TestExtension::new(ext.instance()) };
    seen.add_playback_region(&objects.4);
    // The control: bound to a live document, the renderer gets in.
    assert!(ext.render_access().is_some() && !ext.is_document_destroyed());
    seen.remove_playback_region(&objects.4);
    tear(&doc, objects);
    doc.destroy();
    let log = take_log();
    assert!(log.contains(&"controller dropped".to_string()));
    assert!(!log.contains(&"unbind".to_string()));

    // The instance outlives its document: it renders nothing through ARA and goes quietly.
    assert!(ext.is_document_destroyed());
    assert!(ext.render_access().is_none());
    assert!(ext.with_controller(|_, _| ()).is_none());
    drop(ext);
    assert!(
        take_log().is_empty(),
        "no hook after the controller is gone"
    );
    assert!(host.asserts().is_empty(), "{:?}", host.asserts());
    assert!(host.violations().is_empty(), "{:?}", host.violations());
}
