//! The renderer's guard: renderers are refused, at once, while the host edits; an edit waits for
//! a renderer inside to leave; a renderer never sees half an edit.

mod common;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{HostConfig, Region, RegionSpec, TestExtension};
use mxm_ara::*;

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

/// A region whose start and duration in playback time are both `k`: a renderer that ever reads
/// them unequal has seen half an edit.
fn spec(k: f64) -> RegionSpec {
    RegionSpec {
        start_in_modification: 0.0,
        duration_in_modification: k,
        start_in_playback: k,
        duration_in_playback: k,
        name: Some("with tail".into()),
    }
}

#[test]
fn renderers_are_refused_at_once_during_an_edit() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let try_from_audio_thread = || {
        std::thread::scope(|s| {
            s.spawn(|| {
                let start = Instant::now();
                let got = ext.render_access().is_some();
                (got, start.elapsed())
            })
            .join()
            .unwrap()
        })
    };
    // The control: outside an edit, the renderer gets in.
    assert!(try_from_audio_thread().0);
    doc.begin_editing();
    let (got, took) = try_from_audio_thread();
    assert!(!got, "refused during the edit");
    assert!(
        took < Duration::from_millis(50),
        "without waiting ({took:?})"
    );
    doc.end_editing();
    assert!(try_from_audio_thread().0, "back in after it");
}

#[test]
fn an_edit_waits_for_the_renderer_inside() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    let inside = AtomicBool::new(false);
    let left_at = Mutex::new(None);
    let hold = Duration::from_millis(150);
    let begun_at = std::thread::scope(|s| {
        s.spawn(|| {
            let access = ext.render_access().expect("no edit yet");
            inside.store(true, Ordering::SeqCst);
            std::thread::sleep(hold);
            *left_at.lock().unwrap() = Some(Instant::now());
            drop(access);
        });
        wait_for("the renderer inside", || inside.load(Ordering::SeqCst));
        let start = Instant::now();
        doc.begin_editing();
        let begun = Instant::now();
        doc.end_editing();
        (start, begun)
    });
    let left = left_at.lock().unwrap().expect("the renderer left");
    assert!(
        begun_at.1 >= left,
        "the edit began only after the renderer left"
    );
    // The control: with no renderer inside, an edit begins at once.
    let start = Instant::now();
    doc.begin_editing();
    assert!(start.elapsed() < hold / 2, "{:?}", start.elapsed());
    doc.end_editing();
    let _ = begun_at.0;
}

#[test]
fn a_renderer_never_reads_half_an_edit() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    // SAFETY: used while `ext` lives.
    let seen = unsafe { TestExtension::new(ext.instance()) };
    let (ctx, seq, src, m) = doc.edit(|d| {
        let ctx = d.create_musical_context("song", d_major());
        let seq = d.create_region_sequence("lead", 0, &ctx);
        let src = d.create_audio_source(&source_spec("take-1", 100));
        let m = d.create_audio_modification(&src, "mod-1", "edits");
        (ctx, seq, src, m)
    });
    let mut regions: Vec<Region> = doc.edit(|d| {
        (0..4)
            .map(|_| d.create_playback_region(&m, &seq, &spec(1.0)))
            .collect()
    });
    for r in &regions {
        seen.add_playback_region(r);
    }
    let query = doc.head_and_tail_query();
    let first = regions[0];

    let stop = AtomicBool::new(false);
    let (torn, entered, refused, odd_tails) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    std::thread::scope(|s| {
        s.spawn(|| {
            while !stop.load(Ordering::SeqCst) {
                match ext.render_access() {
                    Some(access) => {
                        for r in access.playback_regions() {
                            let p = r.properties();
                            if p.start_in_playback_time != p.duration_in_playback_time {
                                torn.fetch_add(1, Ordering::SeqCst);
                            }
                            r.data.renders.fetch_add(1, Ordering::Relaxed);
                        }
                        entered.fetch_add(1, Ordering::SeqCst);
                    }
                    None => {
                        refused.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        });
        // Head and tail times, which hosts may ask from the audio thread, mid-edit.
        s.spawn(|| {
            while !stop.load(Ordering::SeqCst) {
                if query.query(&first) != (0.0, 0.5) {
                    odd_tails.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        // At least 300 edits, and on until the renderer has been both in and refused (a busy
        // machine may hold it off the CPU for a while), within 10 s.
        let start = Instant::now();
        let seen_both = || entered.load(Ordering::SeqCst) > 0 && refused.load(Ordering::SeqCst) > 0;
        let mut round = 0;
        while round < 300 || (!seen_both() && start.elapsed() < Duration::from_secs(10)) {
            round += 1;
            let k = round as f64;
            doc.edit(|d| {
                for r in &regions {
                    d.update_playback_region(r, &seq, &spec(k));
                }
            });
            if round % 10 == 0 {
                // A region comes and another goes, through the renderer too.
                let gone = regions.pop().unwrap();
                seen.remove_playback_region(&gone);
                let new = doc.edit(|d| {
                    d.destroy_playback_region(&gone);
                    d.create_playback_region(&m, &seq, &spec(k))
                });
                seen.add_playback_region(&new);
                regions.insert(1, new);
            }
        }
        stop.store(true, Ordering::SeqCst);
    });
    assert_eq!(
        torn.load(Ordering::SeqCst),
        0,
        "a renderer saw half an edit"
    );
    assert_eq!(odd_tails.load(Ordering::SeqCst), 0);
    assert!(entered.load(Ordering::SeqCst) > 0, "the renderer rendered");
    // The control: the guard did keep it out, sometimes.
    assert!(
        refused.load(Ordering::SeqCst) > 0,
        "the renderer was never refused"
    );
    doc.edit(|d| {
        for r in &regions {
            d.destroy_playback_region(r);
        }
        d.destroy_audio_modification(&m);
        d.destroy_audio_source(&src);
        d.destroy_region_sequence(&seq);
        d.destroy_musical_context(&ctx);
    });
    assert!(host.violations().is_empty(), "{:?}", host.violations());
}
