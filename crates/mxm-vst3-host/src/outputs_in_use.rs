//! The MXM outputs in use (the kit's `docs/plugin-conventions.md`, *Outputs in use*), VST3 to
//! CLAP: which output buses after the main one something in a plugin sends to now, each with a
//! name for what plays there, so a host can give each its own track as the user routes to it.
//!
//! - **A plugin that offers `IOutputsInUse`** on its component (or its controller), and answers
//!   `getOutputCount` with `kResultOk`, is offered as one with the CLAP extension
//!   `dk.mxm.outputs-in-use/1`. Each bus it names is the port that bus is
//!   (`plugin::output_port_id`); a name is cut to CLAP's room at a character boundary.
//! - **Back to main** (`to_main`): the bus the port is, asked `getToMain`, each VST3 value given
//!   as the CLAP one `params` gives (a stepped parameter's step).
//! - **The handler offers `IOutputsInUseHandler`**: the plugin's `outputsInUseChanged` raises a
//!   flag and asks for a main-thread callback, where the host's `changed` is called.
//!
//! The interfaces are this crate's own copy of the convention's C++ declarations, written as the
//! `vst3` crate writes Steinberg's (its `IInfoListener`), their IDs `DECLARE_CLASS_IID`'s through
//! `vst3::uid`; the CLAP extension's structs are its own copy too, as every host and plugin keeps.

#![allow(non_snake_case)]

use std::ffi::{CStr, c_char, c_void};

use clack_plugin::extensions::prelude::*;
use vst3::Steinberg::Vst::{ParamID, ParamValue, String128};
use vst3::Steinberg::{FUnknown, FUnknownVtbl, TUID, int32, kResultOk, tresult};
use vst3::com_scrape_types::{Class, Construct, Guid, Header, Inherits, Unknown, Wrapper};
use vst3::{ComPtr, Interface};

use crate::strings::from_utf16;

/// The CLAP extension's id.
const OUTPUTS_IN_USE: &CStr = c"dk.mxm.outputs-in-use/1";

/// CLAP's `CLAP_NAME_SIZE`: the room a name is written into.
const NAME_SIZE: usize = 256;

/// The most outputs read from one plugin: far more than any plugin has buses.
const MOST: int32 = 256;

/// A [`TUID`] as the [`Guid`] an [`Interface`] declares.
pub(crate) const fn guid(tuid: TUID) -> Guid {
    let mut guid = [0; 16];
    let mut i = 0;
    while i < 16 {
        guid[i] = tuid[i] as u8;
        i += 1;
    }
    guid
}

/// The plugin's interface, on its component.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IOutputsInUse {
    vtbl: *const IOutputsInUseVtbl,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IOutputsInUseVtbl {
    base: FUnknownVtbl,
    getOutputCount:
        unsafe extern "system" fn(this: *mut IOutputsInUse, count: *mut int32) -> tresult,
    getOutputInUse: unsafe extern "system" fn(
        this: *mut IOutputsInUse,
        index: int32,
        bus_index: *mut int32,
        name: *mut String128,
    ) -> tresult,
    getToMain: unsafe extern "system" fn(
        this: *mut IOutputsInUse,
        bus_index: int32,
        index: int32,
        id: *mut ParamID,
        value: *mut ParamValue,
    ) -> tresult,
}

/// The host's interface, on its `IComponentHandler`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IOutputsInUseHandler {
    vtbl: *const IOutputsInUseHandlerVtbl,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IOutputsInUseHandlerVtbl {
    base: FUnknownVtbl,
    outputsInUseChanged: unsafe extern "system" fn(this: *mut IOutputsInUseHandler) -> tresult,
}

/// What the handler does when the plugin says its outputs in use may have changed.
pub(crate) trait IOutputsInUseHandlerTrait {
    unsafe fn outputsInUseChanged(&self) -> tresult;
}

/// `Unknown`, `Interface` and `Inherits<FUnknown>` for an interface whose vtable begins with
/// `FUnknown`'s, as the generated bindings implement them.
macro_rules! interface {
    ($interface:ident, $vtbl:ident, $a:literal, $b:literal, $c:literal, $d:literal) => {
        // SAFETY: a pointer to the vtable, as COM lays out every interface.
        unsafe impl Send for $interface {}
        // SAFETY: as above.
        unsafe impl Sync for $interface {}

        impl Unknown for $interface {
            unsafe fn query_interface(this: *mut Self, iid: &Guid) -> Option<*mut c_void> {
                // SAFETY: its vtable begins with `FUnknown`'s.
                unsafe { FUnknown::query_interface(this.cast(), iid) }
            }

            unsafe fn add_ref(this: *mut Self) -> usize {
                // SAFETY: as above.
                unsafe { FUnknown::add_ref(this.cast()) }
            }

            unsafe fn release(this: *mut Self) -> usize {
                // SAFETY: as above.
                unsafe { FUnknown::release(this.cast()) }
            }
        }

        // SAFETY: the layout is a pointer to `$vtbl`, which begins with `FUnknownVtbl`.
        unsafe impl Interface for $interface {
            type Vtbl = $vtbl;
            const IID: Guid = guid(vst3::uid($a, $b, $c, $d));

            fn inherits(iid: &Guid) -> bool {
                iid == &Self::IID || FUnknown::inherits(iid)
            }
        }

        // SAFETY: `$vtbl` begins with `FUnknownVtbl`.
        unsafe impl Inherits<FUnknown> for $interface {}
    };
}

/// Also for ARA's interfaces (`ara.rs`).
pub(crate) use interface;

interface!(
    IOutputsInUse,
    IOutputsInUseVtbl,
    0x19786A5F,
    0xE141689D,
    0x5D908823,
    0xCE588BF6
);
interface!(
    IOutputsInUseHandler,
    IOutputsInUseHandlerVtbl,
    0x30EA7F50,
    0x4E3D0C7E,
    0x498B4086,
    0xC95A6C81
);

impl IOutputsInUseHandler {
    const fn make_vtbl<C, W, const OFFSET: isize>() -> IOutputsInUseHandlerVtbl
    where
        C: IOutputsInUseHandlerTrait + Class,
        W: Wrapper<C>,
    {
        unsafe extern "system" fn outputsInUseChanged<C, W, const OFFSET: isize>(
            this: *mut IOutputsInUseHandler,
        ) -> tresult
        where
            C: IOutputsInUseHandlerTrait + Class,
            W: Wrapper<C>,
        {
            // SAFETY: called through the vtable `W` made for this object, at `OFFSET`.
            unsafe {
                let header = (this as *mut u8).offset(-OFFSET) as *mut Header<C>;
                (*W::data_from_header(header)).outputsInUseChanged()
            }
        }

        IOutputsInUseHandlerVtbl {
            base: FUnknown::make_vtbl::<C, W, OFFSET>(),
            outputsInUseChanged: outputsInUseChanged::<C, W, OFFSET>,
        }
    }
}

// SAFETY: `OBJ` points to a vtable of this interface's layout, made for `C`.
unsafe impl<C, W, const OFFSET: isize> Construct<C, W, OFFSET> for IOutputsInUseHandler
where
    C: IOutputsInUseHandlerTrait + Class,
    W: Wrapper<C>,
{
    const OBJ: Self = IOutputsInUseHandler {
        vtbl: &Self::make_vtbl::<C, W, OFFSET>(),
    };
}

/// The plugin's outputs in use: each output bus and a name for what plays there. `None` when it
/// doesn't say (`kNotImplemented`, or any answer but `kResultOk`).
///
/// # Safety
///
/// On the main thread, `plugin` a live object's.
pub(crate) unsafe fn read(plugin: &ComPtr<IOutputsInUse>) -> Option<Vec<(u32, String)>> {
    let ptr = plugin.as_ptr();
    let mut count = 0;
    // SAFETY: the caller's; the plugin's own function, through its own vtable.
    if unsafe { ((*(*ptr).vtbl).getOutputCount)(ptr, &mut count) } != kResultOk {
        return None;
    }
    let outputs = (0..count.clamp(0, MOST))
        .filter_map(|index| {
            let mut bus = -1;
            let mut name: String128 = [0; 128];
            // SAFETY: as above; `name` is the `String128` it writes into.
            let got = unsafe { ((*(*ptr).vtbl).getOutputInUse)(ptr, index, &mut bus, &mut name) };
            // The last unit is always a NUL, whatever the plugin wrote.
            name[127] = 0;
            let end = name.iter().position(|&unit| unit == 0).unwrap_or(127);
            let bus = u32::try_from(bus).ok()?;
            (got == kResultOk).then(|| (bus, from_utf16(&name[..end])))
        })
        .collect();
    Some(outputs)
}

/// One of the parameter values that send what plays on output bus `bus` back to main: its id and
/// normalised value. `None` past the last.
///
/// # Safety
///
/// On the main thread, `plugin` a live object's.
pub(crate) unsafe fn to_main(
    plugin: &ComPtr<IOutputsInUse>,
    bus: u32,
    index: u32,
) -> Option<(ParamID, ParamValue)> {
    let (Ok(bus), Ok(index)) = (int32::try_from(bus), int32::try_from(index)) else {
        return None;
    };
    let ptr = plugin.as_ptr();
    let (mut id, mut value) = (0, 0.0);
    // SAFETY: the caller's; the plugin's own function, through its own vtable.
    let got = unsafe { ((*(*ptr).vtbl).getToMain)(ptr, bus, index, &mut id, &mut value) };
    (got == kResultOk && value.is_finite()).then_some((id, value))
}

/// Copies `name` into `dest` as NUL-terminated UTF-8, cut at a character boundary when it doesn't
/// fit.
fn copy_name(dest: &mut [u8], name: &str) {
    let Some(room) = dest.len().checked_sub(1) else {
        return;
    };
    let mut end = name.len().min(room);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    dest[..end].copy_from_slice(&name.as_bytes()[..end]);
    dest[end] = 0;
}

/// The CLAP extension's plugin side, as C lays it out.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawPluginOutputsInUse {
    count: Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> u32>,
    get: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            index: u32,
            port_id: *mut u32,
            name: *mut c_char,
        ) -> bool,
    >,
    to_main: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            port_id: u32,
            index: u32,
            param_id: *mut u32,
            value: *mut f64,
        ) -> bool,
    >,
}

/// The CLAP extension's host side, as C lays it out.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawHostOutputsInUse {
    changed: Option<unsafe extern "C" fn(host: *const c_void)>,
}

/// The CLAP extension's plugin side, which a plugin that offers `IOutputsInUse` registers.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct PluginOutputsInUse(RawExtension<PluginExtensionSide, RawPluginOutputsInUse>);

// SAFETY: `RawPluginOutputsInUse` is `repr(C)` and laid out as the extension's plugin struct.
unsafe impl Extension for PluginOutputsInUse {
    const IDENTIFIERS: &[&CStr] = &[OUTPUTS_IN_USE];
    type ExtensionSide = PluginExtensionSide;

    unsafe fn from_raw(raw: RawExtension<Self::ExtensionSide>) -> Self {
        // SAFETY: the caller upholds that the pointer is of this extension's type.
        Self(unsafe { raw.cast() })
    }
}

/// The CLAP extension's host side.
#[derive(Clone, Copy)]
pub(crate) struct HostOutputsInUse(RawExtension<HostExtensionSide, RawHostOutputsInUse>);

// SAFETY: `RawHostOutputsInUse` is `repr(C)` and laid out as the extension's host struct.
unsafe impl Extension for HostOutputsInUse {
    const IDENTIFIERS: &[&CStr] = &[OUTPUTS_IN_USE];
    type ExtensionSide = HostExtensionSide;

    unsafe fn from_raw(raw: RawExtension<Self::ExtensionSide>) -> Self {
        // SAFETY: the caller upholds that the pointer is of this extension's type.
        Self(unsafe { raw.cast() })
    }
}

impl HostOutputsInUse {
    /// Tells the host the outputs in use may have changed, on the main thread.
    pub(crate) fn changed(&self, host: &mut HostMainThreadHandle) {
        if let Some(changed) = host.use_extension(&self.0).changed {
            // SAFETY: the host's own function, given its own pointer, on the main thread.
            unsafe { changed(std::ptr::from_ref(host.as_raw()).cast::<c_void>()) }
        }
    }
}

/// What the plugin answers on the main thread.
pub(crate) trait PluginOutputsInUseImpl {
    /// Reads the outputs in use, which [`get`][Self::get] then serves: how many.
    fn count(&mut self) -> u32;
    /// One of those [`count`][Self::count] read: its CLAP port id and its name.
    fn get(&mut self, index: u32) -> Option<(u32, String)>;
    /// One of the parameter values that send what plays on CLAP port `port` back to main: its
    /// CLAP id and value. `None` past the last.
    fn to_main(&mut self, port: u32, index: u32) -> Option<(u32, f64)>;
}

// SAFETY: `RawPluginOutputsInUse` is the extension's plugin struct.
unsafe impl<P> ExtensionImplementation<P> for PluginOutputsInUse
where
    for<'a> P: Plugin<MainThread<'a>: PluginOutputsInUseImpl>,
{
    const IMPLEMENTATION: RawExtensionImplementation =
        RawExtensionImplementation::new(&RawPluginOutputsInUse {
            count: Some(count::<P>),
            get: Some(get::<P>),
            to_main: Some(to_main_value::<P>),
        });
}

unsafe extern "C" fn count<P>(plugin: *const clap_plugin) -> u32
where
    for<'a> P: Plugin<MainThread<'a>: PluginOutputsInUseImpl>,
{
    // SAFETY: the pointer is this plugin's, called on the main thread.
    unsafe {
        PluginWrapper::<P>::handle(plugin, |plugin| Ok(plugin.main_thread().as_mut().count()))
    }
    .unwrap_or(0)
}

unsafe extern "C" fn get<P>(
    plugin: *const clap_plugin,
    index: u32,
    port_id: *mut u32,
    name: *mut c_char,
) -> bool
where
    for<'a> P: Plugin<MainThread<'a>: PluginOutputsInUseImpl>,
{
    if port_id.is_null() || name.is_null() {
        return false;
    }
    // SAFETY: the pointer is this plugin's, called on the main thread; `port_id` and `name` are
    // the host's, `name` holding `NAME_SIZE` bytes.
    unsafe {
        PluginWrapper::<P>::handle(plugin, |plugin| {
            let Some((port, text)) = plugin.main_thread().as_mut().get(index) else {
                return Ok(false);
            };
            *port_id = port;
            copy_name(
                std::slice::from_raw_parts_mut(name.cast::<u8>(), NAME_SIZE),
                &text,
            );
            Ok(true)
        })
    }
    .unwrap_or(false)
}

unsafe extern "C" fn to_main_value<P>(
    plugin: *const clap_plugin,
    port_id: u32,
    index: u32,
    param_id: *mut u32,
    value: *mut f64,
) -> bool
where
    for<'a> P: Plugin<MainThread<'a>: PluginOutputsInUseImpl>,
{
    if param_id.is_null() || value.is_null() {
        return false;
    }
    // SAFETY: the pointer is this plugin's, called on the main thread; `param_id` and `value` are
    // the host's.
    unsafe {
        PluginWrapper::<P>::handle(plugin, |plugin| {
            let Some((id, to)) = plugin.main_thread().as_mut().to_main(port_id, index) else {
                return Ok(false);
            };
            *param_id = id;
            *value = to;
            Ok(true)
        })
    }
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_cut_at_a_character_boundary() {
        // "Kick" and a two-byte "é" fill six bytes, the NUL a seventh.
        let mut dest = [0xFF; 7];
        copy_name(&mut dest, "Kickéé");
        assert_eq!(CStr::from_bytes_until_nul(&dest).unwrap(), c"Kické");
        let mut dest = [0xFF; 6];
        copy_name(&mut dest, "Kické");
        assert_eq!(CStr::from_bytes_until_nul(&dest).unwrap(), c"Kick");
        copy_name(&mut [], "Kick");
    }

    #[test]
    fn the_ids_are_the_conventions() {
        // `DECLARE_CLASS_IID (IOutputsInUse, 0x19786A5F, …)`, as nice-plug declares it: COM's byte
        // order on Windows, big-endian elsewhere.
        let first: [u8; 4] = if cfg!(windows) {
            [0x5F, 0x6A, 0x78, 0x19]
        } else {
            [0x19, 0x78, 0x6A, 0x5F]
        };
        assert_eq!(IOutputsInUse::IID[..4], first);
        assert_eq!(
            IOutputsInUse::IID[8..],
            [0x5D, 0x90, 0x88, 0x23, 0xCE, 0x58, 0x8B, 0xF6]
        );
        assert_eq!(
            IOutputsInUseHandler::IID[8..],
            [0x49, 0x8B, 0x40, 0x86, 0xC9, 0x5A, 0x6C, 0x81]
        );
    }
}
