//! Nothing on the render path allocates or frees: entering the guard, reading the regions, being
//! refused during an edit, and the head and tail query. An allocator counts what the watched
//! thread does.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{HostConfig, RegionSpec, TestExtension};
use mxm_ara::*;

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static WATCHED: Cell<bool> = const { Cell::new(false) };
}

fn count() {
    if WATCHED.with(Cell::get) {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
    }
}

// SAFETY: forwards to the system allocator; counting touches only an atomic and a const
// thread-local without a destructor.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count();
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// How many allocations and frees `f` made on this thread.
fn allocations_in(f: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.load(Ordering::SeqCst);
    WATCHED.with(|w| w.set(true));
    f();
    WATCHED.with(|w| w.set(false));
    ALLOCATIONS.load(Ordering::SeqCst) - before
}

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

#[test]
fn the_render_path_neither_allocates_nor_frees() {
    let host = host(HostConfig::default());
    let doc = host.create_document("song").unwrap();
    let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
    // SAFETY: used while `ext` lives.
    let seen = unsafe { TestExtension::new(ext.instance()) };
    let (ctx, seq, src, m, r) = doc.edit(|d| {
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
    });
    seen.add_playback_region(&r);
    let query = doc.head_and_tail_query();
    // Warm up thread-locals the first time round.
    let _ = ext.render_access();
    let mut sum = 0.0;
    let rendering = allocations_in(|| {
        for _ in 0..100 {
            let access = ext.render_access().expect("no edit under way");
            for region in access.playback_regions() {
                sum += region.properties().start_in_playback_time
                    + region.properties().duration_in_playback_time;
                region.data.renders.fetch_add(1, Ordering::Relaxed);
            }
            drop(access);
            let (head, tail) = query.query(&r);
            sum += head + tail;
        }
    });
    assert_eq!(rendering, 0, "the render path allocated or freed");
    assert_eq!(sum, 100.0);
    doc.begin_editing();
    let refused = allocations_in(|| assert!(ext.render_access().is_none()));
    doc.end_editing();
    assert_eq!(refused, 0);
    // The control: the counter sees an allocation.
    assert_eq!(
        allocations_in(|| drop(std::hint::black_box(vec![1u8; 16]))),
        2,
        "one allocation, one free"
    );

    seen.remove_playback_region(&r);
    doc.edit(|d| {
        d.destroy_playback_region(&r);
        d.destroy_audio_modification(&m);
        d.destroy_audio_source(&src);
        d.destroy_region_sequence(&seq);
        d.destroy_musical_context(&ctx);
    });
}
