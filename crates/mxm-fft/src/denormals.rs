//! A scoped hardware flush of subnormal numbers, around one transform.
//!
//! A third-party transform cannot flush per butterfly as the collection's own radix-2 copies did,
//! so while a transform runs the floating-point unit treats subnormals as zero:
//!
//! - **x86_64**: the MXCSR flush-to-zero (bit 15) and denormals-are-zero (bit 6) bits;
//! - **aarch64**: the FPCR flush-to-zero bit (bit 24);
//! - **any other target**: nothing. A caller's own flush of its stored state is then the only
//!   layer, which it keeps everywhere anyway (the archive's `plans/plan-mxm-fft.md` §3).
//!
//! The state before is restored when the guard drops, so the caller's floating-point environment is
//! never changed. nice-plug's own guard is compiled out in this workspace (its
//! `unsafe_flush_denormals` feature is off), so this one does not lean on it.
//!
//! The register is the thread's, so a guard stays on the thread that made it: it is neither `Send`
//! nor `Sync`.
//!
//! ```
//! let _flush = mxm_fft::denormals::Guard::new();
//! ```
//!
//! ```compile_fail
//! fn send<T: Send>() {}
//! send::<mxm_fft::denormals::Guard>();
//! ```
//!
//! ```compile_fail
//! fn sync<T: Sync>() {}
//! sync::<mxm_fft::denormals::Guard>();
//! ```

use std::marker::PhantomData;

/// Flushes subnormals to zero until dropped, then restores what was set before.
pub struct Guard {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    previous: Control,
    /// Keeps the guard on its thread, whose register it restores.
    thread: PhantomData<*const ()>,
}

#[cfg(target_arch = "x86_64")]
type Control = u32;
#[cfg(target_arch = "aarch64")]
type Control = u64;

#[cfg(target_arch = "x86_64")]
const FLUSH: Control = (1 << 15) | (1 << 6);
#[cfg(target_arch = "aarch64")]
const FLUSH: Control = 1 << 24;

#[cfg(target_arch = "x86_64")]
fn read() -> Control {
    let mut csr: Control = 0;
    // SAFETY: `stmxcsr` stores the 32-bit MXCSR register to the given address, which is a live,
    // aligned `u32` owned by this frame. It changes no flags and touches no stack.
    unsafe {
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags));
    }
    csr
}

#[cfg(target_arch = "x86_64")]
fn write(csr: Control) {
    // SAFETY: `ldmxcsr` loads MXCSR from a live, aligned `u32`. Only the two flush bits differ from
    // what the thread had; no rounding mode or exception mask changes.
    unsafe {
        std::arch::asm!(
            "ldmxcsr [{}]",
            in(reg) &csr,
            options(nostack, preserves_flags, readonly)
        );
    }
}

#[cfg(target_arch = "aarch64")]
fn read() -> Control {
    let fpcr: Control;
    // SAFETY: reading FPCR is unprivileged on every aarch64 OS the collection targets.
    unsafe {
        std::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags));
    }
    fpcr
}

#[cfg(target_arch = "aarch64")]
fn write(fpcr: Control) {
    // SAFETY: writing FPCR is unprivileged; only the flush-to-zero bit differs from what was read.
    unsafe {
        std::arch::asm!("msr fpcr, {}", in(reg) fpcr, options(nomem, nostack, preserves_flags));
    }
}

impl Guard {
    pub fn new() -> Self {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        {
            let previous = read();
            if previous & FLUSH != FLUSH {
                write(previous | FLUSH);
            }
            Self {
                previous,
                thread: PhantomData,
            }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Self {
                thread: PhantomData,
            }
        }
    }

    /// Whether this target flushes in hardware while a guard lives.
    pub const fn flushes() -> bool {
        cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
    }
}

impl Default for Guard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        if self.previous & FLUSH != FLUSH {
            write(self.previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;

    fn halve_the_smallest_normal() -> f32 {
        black_box(f32::MIN_POSITIVE) * black_box(0.5)
    }

    #[test]
    fn a_guard_flushes_and_dropping_it_restores_the_state() {
        let outside = halve_the_smallest_normal();
        assert!(
            outside > 0.0 && outside < f32::MIN_POSITIVE,
            "subnormal outside a guard"
        );
        {
            let _guard = Guard::new();
            let inside = halve_the_smallest_normal();
            if Guard::flushes() {
                assert_eq!(inside, 0.0, "flushed inside");
            }
            {
                let _nested = Guard::new();
                assert_eq!(halve_the_smallest_normal(), inside);
            }
            assert_eq!(
                halve_the_smallest_normal(),
                inside,
                "a nested guard keeps the outer's"
            );
        }
        assert_eq!(halve_the_smallest_normal(), outside, "restored afterwards");
    }
}
