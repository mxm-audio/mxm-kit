//! Programming errors on either side of the API, reported as ARA reports them.
//!
//! ARA separates programming errors from runtime errors: a call the API says is invalid is
//! reported through the global assert function the host gave `initializeARAWithConfiguration`
//! (when it gave one), and then handled gracefully (refused or ignored), never by crashing or
//! panicking across the C boundary. Every report also counts in [`api_violations`].

use std::ffi::{CStr, c_void};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use crate::sys;

static VIOLATIONS: AtomicUsize = AtomicUsize::new(0);
static ASSERT_FUNCTION: AtomicPtr<sys::ARAAssertFunction> = AtomicPtr::new(std::ptr::null_mut());

/// How many invalid calls (by the host, or by the plug-in through this crate) have been reported
/// since the library was loaded.
pub fn api_violations() -> usize {
    VIOLATIONS.load(Ordering::SeqCst)
}

/// Kinds of programming error, as `ARAAssertCategory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Argument,
    State,
    Thread,
}

impl Category {
    fn raw(self) -> sys::ARAAssertCategory {
        match self {
            Category::Argument => sys::kARAAssertInvalidArgument,
            Category::State => sys::kARAAssertInvalidState,
            Category::Thread => sys::kARAAssertInvalidThread,
        }
    }
}

/// Reports a programming error: counted, and passed to the host's assert function if it set one.
pub(crate) fn report(category: Category, argument: *const c_void, diagnosis: &'static CStr) {
    VIOLATIONS.fetch_add(1, Ordering::SeqCst);
    let address = ASSERT_FUNCTION.load(Ordering::SeqCst);
    if address.is_null() {
        return;
    }
    // SAFETY: the host guarantees the address stays valid until `uninitializeARA`, when it is
    // cleared; it may hold NULL to switch asserts off.
    if let Some(assert) = unsafe { address.read() } {
        // SAFETY: a host-provided C function taking these arguments.
        unsafe { assert(category.raw(), argument, diagnosis.as_ptr()) };
    }
}

/// Checks a condition the API requires, reporting it when it fails; returns the condition.
macro_rules! check {
    ($cond:expr, $cat:ident, $arg:expr, $msg:literal) => {{
        let ok: bool = $cond;
        if !ok {
            $crate::validate::report(
                $crate::validate::Category::$cat,
                ($arg) as *const _ as *const ::core::ffi::c_void,
                $msg,
            );
        }
        ok
    }};
}
pub(crate) use check;

pub(crate) fn set_assert_function(address: *mut sys::ARAAssertFunction) {
    ASSERT_FUNCTION.store(address, Ordering::SeqCst);
}

pub(crate) fn clear_assert_function(address: *mut sys::ARAAssertFunction) {
    let _ = ASSERT_FUNCTION.compare_exchange(
        address,
        std::ptr::null_mut(),
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}
