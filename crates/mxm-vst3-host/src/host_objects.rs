//! The objects a VST3 host hands to plugins: the host application, the messages a component and
//! its controller exchange with their attribute lists, and memory streams for state.
//!
//! Each follows the SDK's own host implementation (`public.sdk/source/vst/hosting/
//! hostclasses.cpp` and `public.sdk/source/common/memorystream.cpp`) in behaviour.

use crate::strings::to_string128;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;
use vst3::Steinberg::IBStream_::IStreamSeekMode_::{kIBSeekCur, kIBSeekEnd, kIBSeekSet};
use vst3::Steinberg::Vst::{
    IAttributeList, IAttributeListTrait, IAudioProcessor, IComponent, IConnectionPoint,
    IEditController, IHostApplication, IHostApplicationTrait, IMessage, IMessageTrait,
    IMidiMapping, IPlugInterfaceSupport, IPlugInterfaceSupportTrait, IUnitInfo, String128, TChar,
};
use vst3::Steinberg::{
    FIDString, IBStream, IBStreamTrait, ISizeableStream, ISizeableStreamTrait, TUID, int32, int64,
    kInvalidArgument, kResultFalse, kResultOk, kResultTrue, tresult, uint32,
};
use vst3::{Class, ComWrapper, Interface};

/// A TUID's bytes, comparable with an interface's IID.
pub(crate) fn guid(tuid: &TUID) -> [u8; 16] {
    tuid.map(|c| c as u8)
}

/// Locks `mutex`, carrying on through a poisoned lock: a panic elsewhere must not take the host's
/// objects down with it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// `IHostApplication`: the host's name, and the objects a plugin may ask it to create.
pub(crate) struct HostApplication {
    name: String,
}

impl HostApplication {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
        }
    }
}

impl Class for HostApplication {
    type Interfaces = (IHostApplication, IPlugInterfaceSupport);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: the plugin passes a writable `String128`.
        unsafe { *name = to_string128(&self.name) };
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: checked non-null; the plugin passes 16-byte IDs and a writable pointer.
        let (cid, iid) = unsafe { (guid(&*cid), guid(&*iid)) };
        let created = if cid == IMessage::IID && iid == IMessage::IID {
            ComWrapper::new(HostMessage::new())
                .to_com_ptr::<IMessage>()
                .map(|message| message.into_raw() as *mut c_void)
        } else if cid == IAttributeList::IID && iid == IAttributeList::IID {
            ComWrapper::new(HostAttributeList::new())
                .to_com_ptr::<IAttributeList>()
                .map(|list| list.into_raw() as *mut c_void)
        } else {
            None
        };
        // SAFETY: `obj` is writable, checked above.
        unsafe { *obj = created.unwrap_or(std::ptr::null_mut()) };
        if created.is_some() {
            kResultOk
        } else {
            kResultFalse
        }
    }
}

impl IPlugInterfaceSupportTrait for HostApplication {
    unsafe fn isPlugInterfaceSupported(&self, iid: *const TUID) -> tresult {
        if iid.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: checked non-null.
        let iid = guid(unsafe { &*iid });
        let supported = [
            IComponent::IID,
            IAudioProcessor::IID,
            IEditController::IID,
            IConnectionPoint::IID,
            IUnitInfo::IID,
            IMidiMapping::IID,
        ];
        if supported.contains(&iid) {
            kResultTrue
        } else {
            kResultFalse
        }
    }
}

/// `IMessage`: an ID and an attribute list, which a component and its controller send each
/// other through their connection points.
pub(crate) struct HostMessage {
    id: Mutex<Option<CString>>,
    attributes: ComWrapper<HostAttributeList>,
}

impl HostMessage {
    fn new() -> Self {
        Self {
            id: Mutex::new(None),
            attributes: ComWrapper::new(HostAttributeList::new()),
        }
    }
}

impl Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> FIDString {
        // The pointer stays valid until the ID is set again or the message is released, which is
        // as long as the SDK's own message keeps it.
        lock(&self.id)
            .as_ref()
            .map(|id| id.as_ptr())
            .unwrap_or(std::ptr::null())
    }

    unsafe fn setMessageID(&self, id: FIDString) {
        *lock(&self.id) = (!id.is_null()).then(|| {
            // SAFETY: a non-null message ID is a NUL-terminated string.
            unsafe { CStr::from_ptr(id) }.to_owned()
        });
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        // No reference is added, as in the SDK: the list lives as long as the message.
        self.attributes
            .as_com_ref::<IAttributeList>()
            .map(|list| list.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }
}

enum Attribute {
    Int(i64),
    Float(f64),
    /// UTF-16, without the terminating NUL.
    Text(Vec<u16>),
    Binary(Vec<u8>),
}

/// `IAttributeList`: named values of four kinds.
pub(crate) struct HostAttributeList {
    values: Mutex<HashMap<CString, Attribute>>,
}

impl HostAttributeList {
    fn new() -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
        }
    }

    /// Stores `value` under `id`, or refuses a null ID.
    unsafe fn set(&self, id: *const c_char, value: Attribute) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: a non-null attribute ID is a NUL-terminated string.
        let id = unsafe { CStr::from_ptr(id) }.to_owned();
        lock(&self.values).insert(id, value);
        kResultOk
    }

    /// Runs `read` on the value under `id`, or answers `kResultFalse` when there is none.
    unsafe fn get(&self, id: *const c_char, read: impl FnOnce(&Attribute) -> tresult) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: a non-null attribute ID is a NUL-terminated string.
        let id = unsafe { CStr::from_ptr(id) };
        lock(&self.values).get(id).map_or(kResultFalse, read)
    }
}

impl Class for HostAttributeList {
    type Interfaces = (IAttributeList,);
}

impl IAttributeListTrait for HostAttributeList {
    unsafe fn setInt(&self, id: *const c_char, value: int64) -> tresult {
        // SAFETY: the plugin's ID, checked in `set`.
        unsafe { self.set(id, Attribute::Int(value)) }
    }

    unsafe fn getInt(&self, id: *const c_char, value: *mut int64) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: the plugin's ID; `value` is writable, checked above.
        unsafe {
            self.get(id, |attribute| match attribute {
                Attribute::Int(int) => {
                    *value = *int;
                    kResultOk
                }
                _ => kResultFalse,
            })
        }
    }

    unsafe fn setFloat(&self, id: *const c_char, value: f64) -> tresult {
        // SAFETY: the plugin's ID, checked in `set`.
        unsafe { self.set(id, Attribute::Float(value)) }
    }

    unsafe fn getFloat(&self, id: *const c_char, value: *mut f64) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: the plugin's ID; `value` is writable, checked above.
        unsafe {
            self.get(id, |attribute| match attribute {
                Attribute::Float(float) => {
                    *value = *float;
                    kResultOk
                }
                _ => kResultFalse,
            })
        }
    }

    unsafe fn setString(&self, id: *const c_char, string: *const TChar) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let mut text = Vec::new();
        // SAFETY: the plugin's string is NUL-terminated UTF-16.
        unsafe {
            let mut cursor = string;
            while *cursor != 0 {
                text.push(*cursor);
                cursor = cursor.add(1);
            }
            self.set(id, Attribute::Text(text))
        }
    }

    unsafe fn getString(
        &self,
        id: *const c_char,
        string: *mut TChar,
        size_in_bytes: uint32,
    ) -> tresult {
        let capacity = size_in_bytes as usize / std::mem::size_of::<TChar>();
        if string.is_null() || capacity == 0 {
            return kInvalidArgument;
        }
        // SAFETY: the plugin's ID; `string` holds `size_in_bytes` bytes.
        unsafe {
            self.get(id, |attribute| match attribute {
                Attribute::Text(text) => {
                    let length = text.len().min(capacity - 1);
                    std::ptr::copy_nonoverlapping(text.as_ptr(), string, length);
                    *string.add(length) = 0;
                    kResultOk
                }
                _ => kResultFalse,
            })
        }
    }

    unsafe fn setBinary(
        &self,
        id: *const c_char,
        data: *const c_void,
        size_in_bytes: uint32,
    ) -> tresult {
        let bytes = if data.is_null() || size_in_bytes == 0 {
            Vec::new()
        } else {
            // SAFETY: the plugin's buffer holds `size_in_bytes` bytes.
            unsafe { std::slice::from_raw_parts(data as *const u8, size_in_bytes as usize) }
                .to_vec()
        };
        // SAFETY: the plugin's ID, checked in `set`.
        unsafe { self.set(id, Attribute::Binary(bytes)) }
    }

    unsafe fn getBinary(
        &self,
        id: *const c_char,
        data: *mut *const c_void,
        size_in_bytes: *mut uint32,
    ) -> tresult {
        if data.is_null() || size_in_bytes.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: the plugin's ID; both out-parameters checked. The pointer stays valid while the
        // attribute is unchanged, as the SDK's does.
        unsafe {
            self.get(id, |attribute| match attribute {
                Attribute::Binary(bytes) => {
                    *data = bytes.as_ptr() as *const c_void;
                    *size_in_bytes = bytes.len() as uint32;
                    kResultOk
                }
                _ => kResultFalse,
            })
        }
    }
}

/// `IBStream` over memory: what a component or controller writes its state into, and reads it
/// back from.
pub(crate) struct MemoryStream {
    /// The bytes, and the position the next read or write starts at.
    inner: Mutex<(Vec<u8>, usize)>,
}

impl MemoryStream {
    pub(crate) fn new() -> Self {
        Self::from_bytes(Vec::new())
    }

    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            inner: Mutex::new((bytes, 0)),
        }
    }

    /// Everything written, whatever the position.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        lock(&self.inner).0.clone()
    }

    /// Back to the start, to be read again.
    pub(crate) fn rewind(&self) {
        lock(&self.inner).1 = 0;
    }

    /// The stream as the `IBStream` a plugin takes; valid while `stream` is.
    pub(crate) fn as_ptr(stream: &ComWrapper<MemoryStream>) -> *mut IBStream {
        stream
            .as_com_ref::<IBStream>()
            .map(|stream| stream.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream, ISizeableStream);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_read: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let mut inner = lock(&self.inner);
        let (bytes, position) = &mut *inner;
        let available = bytes.len().saturating_sub(*position);
        let count = available.min(num_bytes as usize);
        // SAFETY: `buffer` holds `num_bytes` bytes and `count` is no more.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr().add(*position), buffer as *mut u8, count);
        }
        *position += count;
        if !num_bytes_read.is_null() {
            // SAFETY: checked non-null.
            unsafe { *num_bytes_read = count as int32 };
        }
        kResultOk
    }

    unsafe fn write(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_written: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let mut inner = lock(&self.inner);
        let (bytes, position) = &mut *inner;
        let count = num_bytes as usize;
        let end = *position + count;
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        // SAFETY: `buffer` holds `num_bytes` bytes, and `bytes` was grown to take them.
        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer as *const u8,
                bytes.as_mut_ptr().add(*position),
                count,
            );
        }
        *position = end;
        if !num_bytes_written.is_null() {
            // SAFETY: checked non-null.
            unsafe { *num_bytes_written = count as int32 };
        }
        kResultOk
    }

    unsafe fn seek(&self, pos: int64, mode: int32, result: *mut int64) -> tresult {
        let mut inner = lock(&self.inner);
        let (bytes, position) = &mut *inner;
        let mode = mode as vst3::Steinberg::IBStream_::IStreamSeekMode;
        let base = if mode == kIBSeekSet {
            0
        } else if mode == kIBSeekCur {
            *position as i64
        } else if mode == kIBSeekEnd {
            bytes.len() as i64
        } else {
            return kInvalidArgument;
        };
        let Some(target) = base.checked_add(pos).filter(|&target| target >= 0) else {
            return kInvalidArgument;
        };
        // Beyond the end is allowed: a later write fills the gap with zeros.
        *position = target as usize;
        if !result.is_null() {
            // SAFETY: checked non-null.
            unsafe { *result = target };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: checked non-null.
        unsafe { *pos = lock(&self.inner).1 as int64 };
        kResultOk
    }
}

impl ISizeableStreamTrait for MemoryStream {
    unsafe fn getStreamSize(&self, size: *mut int64) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: checked non-null.
        unsafe { *size = lock(&self.inner).0.len() as int64 };
        kResultOk
    }

    unsafe fn setStreamSize(&self, size: int64) -> tresult {
        if size < 0 {
            return kInvalidArgument;
        }
        lock(&self.inner).0.resize(size as usize, 0);
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stream_reads_back_what_was_written_and_seeks() {
        let stream = ComWrapper::new(MemoryStream::new());
        let ptr = MemoryStream::as_ptr(&stream);
        let stream_ref = unsafe { vst3::ComRef::from_raw(ptr) }.unwrap();
        let mut data = *b"hello world";
        let mut written = 0;
        unsafe { stream_ref.write(data.as_mut_ptr() as *mut c_void, 11, &mut written) };
        assert_eq!(written, 11);
        let mut at = 0;
        unsafe { stream_ref.seek(6, kIBSeekSet as int32, &mut at) };
        assert_eq!(at, 6);
        let mut out = [0u8; 16];
        let mut read = 0;
        unsafe { stream_ref.read(out.as_mut_ptr() as *mut c_void, 16, &mut read) };
        assert_eq!(&out[..read as usize], b"world");
        assert_eq!(stream.bytes(), b"hello world");
    }
}
