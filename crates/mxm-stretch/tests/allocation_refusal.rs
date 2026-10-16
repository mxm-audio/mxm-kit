//! **A stretch refuses a buffer it can't reserve instead of aborting its host.** A host stretches off
//! its audio thread, and an infallible allocation that fails there takes the whole program down.
//!
//! **Its own test binary, because it installs a global allocator**: this one refuses, on an armed
//! thread, the allocation of at least [`LARGE`] bytes whose turn it is. Each buffer the stretch sizes
//! by its input or its output is refused in turn, and each must come back as `Error::NoMemory`; one
//! that isn't reserved fallibly aborts this binary, which is what the test looks like when it fails.

use mxm_stretch::{Error, stretch};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::AtomicBool;

/// Larger than anything the stretch sizes by its rate alone (the period track's lag buffers hold a
/// few hundred values at 48 kHz), smaller than everything it sizes by a second of input.
const LARGE: usize = 4_096;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static SEEN: Cell<usize> = const { Cell::new(0) };
    static REFUSE_AT: Cell<Option<usize>> = const { Cell::new(None) };
    static REFUSED: Cell<bool> = const { Cell::new(false) };
}

/// Counts a large allocation on an armed thread and says whether to refuse it.
fn refuse(size: usize) -> bool {
    if size < LARGE || !ARMED.try_with(Cell::get).unwrap_or(false) {
        return false;
    }
    let index = SEEN.get();
    SEEN.set(index + 1);
    let refused = REFUSE_AT.get() == Some(index);
    if refused {
        REFUSED.set(true);
    }
    refused
}

struct Refusing;

// SAFETY: every call either returns null, which `GlobalAlloc` permits, or forwards to `System` with
// the caller's own arguments.
unsafe impl GlobalAlloc for Refusing {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if refuse(layout.size()) {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if refuse(layout.size()) {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc_zeroed(layout) }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if refuse(new_size) {
            std::ptr::null_mut()
        } else {
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }
}

#[global_allocator]
static ALLOCATOR: Refusing = Refusing;

#[test]
fn every_buffer_sized_by_the_input_or_the_output_is_refused_as_no_memory() {
    let input: Vec<f32> = (0..48_000)
        .map(|n| (0.5 * (std::f64::consts::TAU * 220.0 * f64::from(n) / 48_000.0).sin()) as f32)
        .collect();
    let channels: [&[f32]; 2] = [&input, &input];
    let cancel = AtomicBool::new(false);
    // Once unarmed, so what's built once for every stretch (the sinc kernel) is built.
    let whole = stretch(&channels, 57_600, 48_000, &cancel).unwrap();

    let mut refusals = 0;
    for index in 0.. {
        SEEN.set(0);
        REFUSED.set(false);
        REFUSE_AT.set(Some(index));
        ARMED.set(true);
        let result = stretch(&channels, 57_600, 48_000, &cancel);
        ARMED.set(false);
        if !REFUSED.get() {
            assert_eq!(result.unwrap(), whole, "with nothing refused");
            break;
        }
        assert_eq!(result, Err(Error::NoMemory), "allocation {index} refused");
        refusals += 1;
    }
    // The mono mix, the decimated copy, the window and both output channels, at least.
    assert!(
        refusals >= 5,
        "only {refusals} buffers were large enough to refuse"
    );
}

/// Runs `stretch` once with the allocation of at least [`LARGE`] bytes numbered `index` refused,
/// and says whether one was.
fn refusing(
    index: usize,
    run: impl FnOnce() -> Result<Vec<Vec<f32>>, Error>,
) -> (bool, Result<Vec<Vec<f32>>, Error>) {
    SEEN.set(0);
    REFUSED.set(false);
    REFUSE_AT.set(Some(index));
    ARMED.set(true);
    let result = run();
    ARMED.set(false);
    (REFUSED.get(), result)
}

#[test]
fn the_list_of_channels_is_refused_as_no_memory_too() {
    // Enough channels that the list of them is a large allocation itself.
    let channel: Vec<f32> = (0..9_000).map(|n| (n as f32 * 0.01).sin() * 0.5).collect();
    let channels: Vec<&[f32]> = (0..256).map(|_| channel.as_slice()).collect();
    let cancel = AtomicBool::new(false);
    // No frames, fewer, and the same: the stretch builds its list of channels the same way.
    for frames in [0, 100, 9_000] {
        let whole = stretch(&channels, frames, 48_000, &cancel).unwrap();
        let mut refusals = 0;
        for index in 0.. {
            let (refused, result) = refusing(index, || stretch(&channels, frames, 48_000, &cancel));
            if !refused {
                assert_eq!(result.unwrap(), whole, "{frames} frames, nothing refused");
                break;
            }
            assert_eq!(
                result,
                Err(Error::NoMemory),
                "{frames} frames, allocation {index}"
            );
            refusals += 1;
        }
        assert!(refusals >= 1, "{frames} frames: the list was never refused");
    }
}
