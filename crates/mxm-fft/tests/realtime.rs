//! Executing a plan allocates and frees nothing, in every kind, direction, precision and layout, at
//! the audio thread's sizes and the analyser's.

use mxm_fft::{Complex, Planner};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct TrackingAllocator;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static OPERATIONS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACKING.with(|tracking| {
            if tracking.get() {
                OPERATIONS.with(|operations| operations.set(operations.get() + 1));
            }
        });
        // SAFETY: this allocator delegates the exact layout to the process allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        TRACKING.with(|tracking| {
            if tracking.get() {
                OPERATIONS.with(|operations| operations.set(operations.get() + 1));
            }
        });
        // SAFETY: `pointer` and `layout` came from this delegate.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn untracked<R>(run: impl FnOnce() -> R) -> (R, usize) {
    OPERATIONS.with(|operations| operations.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    let result = run();
    TRACKING.with(|tracking| tracking.set(false));
    (result, OPERATIONS.with(Cell::get))
}

#[test]
fn executing_a_plan_allocates_and_frees_nothing() {
    for n in [128, 4_096, 6_000, 16_384] {
        let mut planner = Planner::<f32>::new();
        let complex = planner.complex(n);
        let real = planner.real(n);
        let mut planner64 = Planner::<f64>::new();
        let complex64 = planner64.complex(n);
        let real64 = planner64.real(n);

        let mut buffer = vec![Complex::new(0.25f32, -0.5); n];
        let mut scratch = complex.make_scratch();
        let mut signal = vec![0.5f32; n];
        let mut spectrum = vec![Complex::new(0.0f32, 0.0); real.spectrum_len()];
        let mut real_scratch = real.make_scratch();
        let mut buffer64 = vec![Complex::new(0.25f64, -0.5); n];
        let mut scratch64 = complex64.make_scratch();
        let mut signal64 = vec![0.5f64; n];
        let mut spectrum64 = vec![Complex::new(0.0f64, 0.0); real64.spectrum_len()];
        let mut real_scratch64 = real64.make_scratch();
        let (mut re, mut im) = (vec![0.1f64; n], vec![0.2f64; n]);
        let mut work = vec![Complex::new(0.0f64, 0.0); n];

        let ((), operations) = untracked(|| {
            for _ in 0..3 {
                complex.forward(&mut buffer, &mut scratch).unwrap();
                complex.inverse(&mut buffer, &mut scratch).unwrap();
                real.forward(&mut signal, &mut spectrum, &mut real_scratch)
                    .unwrap();
                real.inverse(&mut spectrum, &mut signal, &mut real_scratch)
                    .unwrap();
                complex64.forward(&mut buffer64, &mut scratch64).unwrap();
                complex64.inverse(&mut buffer64, &mut scratch64).unwrap();
                real64
                    .forward(&mut signal64, &mut spectrum64, &mut real_scratch64)
                    .unwrap();
                real64
                    .inverse(&mut spectrum64, &mut signal64, &mut real_scratch64)
                    .unwrap();
                complex64
                    .forward_split(&mut re, &mut im, &mut work, &mut scratch64)
                    .unwrap();
                complex64
                    .inverse_split(&mut re, &mut im, &mut work, &mut scratch64)
                    .unwrap();
            }
        });
        assert_eq!(operations, 0, "{n} points");
        assert!(signal.iter().all(|x| (x - 0.5).abs() < 1e-4));
    }
}

#[test]
fn a_refused_buffer_allocates_nothing_either() {
    let fft = Planner::<f32>::new().complex(64);
    let mut short = vec![Complex::new(0.0f32, 0.0); 10];
    let mut scratch = fft.make_scratch();
    let (result, operations) = untracked(|| fft.forward(&mut short, &mut scratch));
    assert!(result.is_err());
    assert_eq!(operations, 0);
}
