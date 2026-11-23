//! FFI helpers: ARA's versioned struct sizes, strings, and which thread is calling.

use std::ffi::{CStr, CString, c_char};

/// The `structSize` of a versioned ARA struct implemented up to and including `$field`: ARA's
/// `ARA_IMPLEMENTED_STRUCT_SIZE`. Never `size_of`, which grows with newer headers.
macro_rules! implemented_size {
    ($S:ty, $field:ident) => {{
        const fn pointee_size<T>(_: *const T) -> usize {
            ::core::mem::size_of::<T>()
        }
        let uninit = ::core::mem::MaybeUninit::<$S>::uninit();
        let base = uninit.as_ptr();
        // SAFETY: `addr_of!` computes the field's address inside `uninit` without reading it.
        #[allow(unused_unsafe)]
        let field = unsafe { ::core::ptr::addr_of!((*base).$field) };
        ::core::mem::offset_of!($S, $field) + pointee_size(field)
    }};
}
pub(crate) use implemented_size;

/// Reads `$field` of the versioned struct at `$ptr` if its `structSize` covers it: `None` for an
/// older partner's smaller struct, whose memory past `structSize` may not even exist.
///
/// Unsafe to use: `$ptr` must point to a readable struct whose `structSize` bytes exist.
macro_rules! read_field {
    ($ptr:expr, $S:ty, $field:ident) => {{
        let p: *const $S = $ptr;
        let size: usize = ::core::ptr::addr_of!((*p).structSize).read_unaligned();
        if size >= $crate::util::implemented_size!($S, $field) {
            Some(::core::ptr::addr_of!((*p).$field).read_unaligned())
        } else {
            None
        }
    }};
}
pub(crate) use read_field;

/// Reads a versioned struct's `structSize`.
///
/// # Safety
/// `ptr` must be non-null and point to at least a `structSize` field.
pub(crate) unsafe fn struct_size<S>(ptr: *const S) -> usize {
    // SAFETY: every ARA versioned struct starts with `ARASize structSize` (caller's contract).
    unsafe { ptr.cast::<usize>().read_unaligned() }
}

/// An optional ARA string, copied (invalid UTF-8 replaced).
///
/// # Safety
/// `p` must be null or a NUL-terminated string valid for the call.
pub(crate) unsafe fn opt_string(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        // SAFETY: non-null and NUL-terminated (caller's contract).
        Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }
}

/// A string for the other side: interior NULs, which C cannot carry, are dropped.
pub(crate) fn c_string(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

thread_local! {
    static THREAD_MARK: u8 = const { 0 };
}

/// An identity for the calling thread that costs nothing: the address of a thread-local byte.
/// Unlike `thread::current()`, it never allocates, so the audio thread may ask.
pub(crate) fn thread_mark() -> usize {
    THREAD_MARK.with(|m| std::ptr::from_ref(m) as usize)
}

/// Turns a bool into an `ARABool`.
pub(crate) fn ara_bool(b: bool) -> crate::sys::ARABool {
    if b {
        crate::sys::kARATrue
    } else {
        crate::sys::kARAFalse
    }
}
