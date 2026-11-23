//! The renderers' guard on model state, as ARA's TestPlugIn guards its graph.
//!
//! A renderer (the audio thread) *enters* only while the guard is open, and never waits: refused,
//! it renders silence for that block. The model thread *closes* the guard around a graph edit and
//! then waits until every renderer inside has left, which is at most one block. So whatever a
//! renderer reads inside never changes under it, and the audio thread never allocates, locks or
//! waits for it.
//!
//! Unlike the TestPlugIn, a refused renderer undoes its own entry count at once; there, a refused
//! `rendererWillAccessModelGraph` leaves the count raised, and the next edit would spin forever.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

pub(crate) struct AccessGuard {
    open: AtomicBool,
    inside: AtomicUsize,
    /// How deep the model thread's closings nest (touched by the model thread only).
    depth: AtomicU32,
}

impl AccessGuard {
    pub(crate) const fn new() -> Self {
        Self {
            open: AtomicBool::new(true),
            inside: AtomicUsize::new(0),
            depth: AtomicU32::new(0),
        }
    }

    /// A renderer's entry: `None` while the model thread holds the guard closed. Lock-free, never
    /// waits, never allocates.
    pub(crate) fn try_enter(&self) -> Option<Entered<'_>> {
        // Dekker-style with sequentially consistent operations: either this renderer sees the
        // guard closed, or the closing thread sees it inside and waits for it.
        self.inside.fetch_add(1, Ordering::SeqCst);
        if self.open.load(Ordering::SeqCst) {
            Some(Entered(self))
        } else {
            self.inside.fetch_sub(1, Ordering::SeqCst);
            None
        }
    }

    /// Closes the guard and waits until no renderer is inside (model thread only). Closings
    /// nest; the guard opens again when the outermost one is matched by [`AccessGuard::open`].
    pub(crate) fn close(&self) {
        if self.depth.fetch_add(1, Ordering::Relaxed) == 0 {
            self.open.store(false, Ordering::SeqCst);
            let mut spins = 0u32;
            while self.inside.load(Ordering::SeqCst) != 0 {
                if spins < 64 {
                    std::hint::spin_loop();
                    spins += 1;
                } else {
                    std::thread::yield_now();
                }
            }
        }
    }

    /// Undoes one [`AccessGuard::close`] (model thread only).
    pub(crate) fn open(&self) {
        let depth = self.depth.load(Ordering::Relaxed);
        debug_assert!(depth > 0, "AccessGuard::open without close");
        if depth == 0 {
            return;
        }
        self.depth.store(depth - 1, Ordering::Relaxed);
        if depth == 1 {
            self.open.store(true, Ordering::SeqCst);
        }
    }

    /// Whether the model thread holds the guard closed (model thread only).
    pub(crate) fn is_closed(&self) -> bool {
        self.depth.load(Ordering::Relaxed) > 0
    }

    /// How many renderers are inside or knocking.
    #[cfg(test)]
    pub(crate) fn inside(&self) -> usize {
        self.inside.load(Ordering::SeqCst)
    }
}

/// A renderer inside the guard; leaving is dropping it.
pub(crate) struct Entered<'a>(&'a AccessGuard);

impl Drop for Entered<'_> {
    fn drop(&mut self) {
        self.0.inside.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_guard_refuses_and_an_open_one_admits() {
        let guard = AccessGuard::new();
        assert!(guard.try_enter().is_some());
        guard.close();
        assert!(guard.try_enter().is_none());
        assert_eq!(
            guard.inside(),
            0,
            "a refused renderer leaves no count behind"
        );
        guard.close();
        guard.open();
        assert!(
            guard.try_enter().is_none(),
            "nested closings keep it closed"
        );
        guard.open();
        assert!(guard.try_enter().is_some());
    }
}
