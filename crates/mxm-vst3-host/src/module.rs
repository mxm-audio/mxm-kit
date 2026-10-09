//! Loading a VST3 module: finding its library in the bundle, calling its entry function, and
//! reading its factory's classes.
//!
//! The bundle layout and the entry functions are the SDK's ("VST 3 Locations / Format" and
//! `public.sdk/source/vst/hosting/module_*.cpp`): on Windows `Contents/x86_64-win/<name>.vst3`
//! (or a single file, the older form) with optional `InitDll`/`ExitDll`; on macOS a bundle with
//! `bundleEntry(CFBundleRef)`/`bundleExit`; on Linux `Contents/x86_64-linux/<name>.so` with
//! `ModuleEntry(handle)`/`ModuleExit`. All three export `GetPluginFactory`.

use crate::strings::{from_char8, from_utf16};
use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use vst3::Steinberg::{
    FUnknown, IPluginFactory, IPluginFactory2, IPluginFactory2Trait, IPluginFactory3,
    IPluginFactory3Trait, IPluginFactoryTrait, PClassInfo, PClassInfo2, PClassInfoW, PFactoryInfo,
    TUID, kResultOk,
};
use vst3::{ComPtr, Interface};

/// The category of a class that processes audio, as the SDK spells it (`kVstAudioEffectClass`).
const AUDIO_MODULE_CLASS: &str = "Audio Module Class";

/// One audio class a module offers.
#[derive(Clone)]
pub(crate) struct ClassInfo {
    pub cid: TUID,
    pub name: String,
    pub vendor: String,
    pub version: String,
    /// The SDK's `|`-separated subcategories, such as `Fx|Delay` or `Instrument|Synth`.
    pub sub_categories: String,
}

impl ClassInfo {
    /// Whether it is an instrument rather than an effect. A class that is both (`Fx|Instrument`,
    /// as some vocoders say) is an effect: it has an audio input to process.
    pub(crate) fn is_instrument(&self) -> bool {
        let mut parts = self.sub_categories.split('|');
        let first = parts.next().unwrap_or_default();
        first != "Fx"
            && self
                .sub_categories
                .split('|')
                .any(|part| part == "Instrument")
    }
}

/// A loaded module. Shared by every entry and plugin that uses it; released with the last.
pub(crate) struct Module {
    pub vendor: String,
    pub classes: Vec<ClassInfo>,
    factory: Option<ComPtr<IPluginFactory>>,
    host_context_set: AtomicBool,
    /// Last, so it is dropped after the factory is released.
    _binary: platform::Binary,
}

// SAFETY: a factory's methods may be called from any thread the host scans or creates plugins on;
// the SDK's own hosts do so. The module holds nothing else that is thread-bound.
unsafe impl Send for Module {}
// SAFETY: as above; every method takes `&self` and the factory is reference-counted by the plugin.
unsafe impl Sync for Module {}

/// Every module loaded in this process, by its canonical path, while something holds it.
static LOADED: Mutex<Vec<(PathBuf, Weak<Module>)>> = Mutex::new(Vec::new());

impl Module {
    /// The module at `bundle`, loading it unless it is loaded already.
    ///
    /// # Safety
    ///
    /// Loading a module runs its code.
    pub(crate) unsafe fn load(bundle: &Path) -> Result<Arc<Module>, String> {
        // The canonical path is only the key: the library is opened by the path as given, since a
        // plugin that looks for its resources beside itself may not understand a verbatim path.
        let key = bundle
            .canonicalize()
            .unwrap_or_else(|_| bundle.to_path_buf());
        let mut loaded = LOADED.lock().unwrap_or_else(|poison| poison.into_inner());
        loaded.retain(|(_, module)| module.strong_count() > 0);
        if let Some(module) = loaded
            .iter()
            .find(|(path, _)| *path == key)
            .and_then(|(_, module)| module.upgrade())
        {
            return Ok(module);
        }
        // SAFETY: the caller's.
        let module = Arc::new(unsafe { Self::open(bundle) }?);
        loaded.push((key, Arc::downgrade(&module)));
        Ok(module)
    }

    unsafe fn open(bundle: &Path) -> Result<Module, String> {
        // SAFETY: the caller's.
        let binary = unsafe { platform::Binary::open(bundle) }?;
        // SAFETY: `GetPluginFactory` returns the factory with a reference the caller owns.
        let factory = unsafe { ComPtr::from_raw(binary.factory()?) }
            .ok_or_else(|| format!("{}: the module offers no plugin factory", bundle.display()))?;

        // SAFETY: a zeroed `PFactoryInfo` is a valid out-parameter (plain arrays and an integer).
        let mut info: PFactoryInfo = unsafe { std::mem::zeroed() };
        // SAFETY: `factory` is a live factory and `info` a valid out-parameter.
        let vendor = if unsafe { factory.getFactoryInfo(&mut info) } == kResultOk {
            from_char8(&info.vendor)
        } else {
            String::new()
        };
        // SAFETY: as above.
        let classes = unsafe { read_classes(&factory) };

        Ok(Module {
            vendor,
            classes,
            factory: Some(factory),
            host_context_set: AtomicBool::new(false),
            _binary: binary,
        })
    }

    /// A new instance of class `cid`, as interface `I`.
    ///
    /// # Safety
    ///
    /// Creating an instance runs the module's code; `I` must be an interface the class can have.
    pub(crate) unsafe fn create<I: Interface>(&self, cid: &TUID) -> Option<ComPtr<I>> {
        let factory = self.factory.as_ref()?;
        let mut object: *mut c_void = std::ptr::null_mut();
        // SAFETY: `cid` and `I::IID` are both 16-byte IDs, as `FIDString` points at; `object` is
        // a valid out-parameter.
        let result = unsafe {
            factory.createInstance(cid.as_ptr(), I::IID.as_ptr() as *const c_char, &mut object)
        };
        if result != kResultOk {
            return None;
        }
        // SAFETY: on success the factory hands over one reference to an `I`.
        unsafe { ComPtr::from_raw(object as *mut I) }
    }

    /// Gives the factory the host's context, once per module (`IPluginFactory3::setHostContext`).
    ///
    /// # Safety
    ///
    /// `context` must be a live `FUnknown` that answers for `IHostApplication`.
    pub(crate) unsafe fn set_host_context(&self, context: *mut FUnknown) {
        if self.host_context_set.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(factory) = self
            .factory
            .as_ref()
            .and_then(|factory| factory.cast::<IPluginFactory3>())
        {
            // SAFETY: the caller's; the factory adds its own reference if it keeps the context.
            unsafe { factory.setHostContext(context) };
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        // The factory goes before the module's exit function runs and its library is released.
        self.factory = None;
    }
}

/// The module's audio classes, read through the newest factory interface it offers, so names come
/// in Unicode where it can.
unsafe fn read_classes(factory: &ComPtr<IPluginFactory>) -> Vec<ClassInfo> {
    let factory3 = factory.cast::<IPluginFactory3>();
    let factory2 = factory.cast::<IPluginFactory2>();
    let mut classes = Vec::new();
    // SAFETY: `factory` is live.
    for index in 0..unsafe { factory.countClasses() } {
        // SAFETY (each read below): a zeroed info struct is a valid out-parameter, and `index` is
        // below the factory's own count.
        let read = if let Some(factory3) = &factory3 {
            let mut info: PClassInfoW = unsafe { std::mem::zeroed() };
            (unsafe { factory3.getClassInfoUnicode(index, &mut info) } == kResultOk).then(|| {
                (
                    from_char8(&info.category),
                    ClassInfo {
                        cid: info.cid,
                        name: from_utf16(&info.name),
                        vendor: from_utf16(&info.vendor),
                        version: from_utf16(&info.version),
                        sub_categories: from_char8(&info.subCategories),
                    },
                )
            })
        } else {
            None
        };
        let read = read.or_else(|| {
            let factory2 = factory2.as_ref()?;
            let mut info: PClassInfo2 = unsafe { std::mem::zeroed() };
            (unsafe { factory2.getClassInfo2(index, &mut info) } == kResultOk).then(|| {
                (
                    from_char8(&info.category),
                    ClassInfo {
                        cid: info.cid,
                        name: from_char8(&info.name),
                        vendor: from_char8(&info.vendor),
                        version: from_char8(&info.version),
                        sub_categories: from_char8(&info.subCategories),
                    },
                )
            })
        });
        let read = read.or_else(|| {
            let mut info: PClassInfo = unsafe { std::mem::zeroed() };
            (unsafe { factory.getClassInfo(index, &mut info) } == kResultOk).then(|| {
                (
                    from_char8(&info.category),
                    ClassInfo {
                        cid: info.cid,
                        name: from_char8(&info.name),
                        vendor: String::new(),
                        version: String::new(),
                        sub_categories: String::new(),
                    },
                )
            })
        });
        if let Some((category, class)) = read
            && category == AUDIO_MODULE_CLASS
        {
            classes.push(class);
        }
    }
    classes
}

/// The library inside a bundle folder: `Contents/<architecture>/<bundle name>`, or failing that
/// the one file there with the platform's extension. A plain file is its own library.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn library_in(bundle: &Path, architectures: &[&str], extension: &str) -> Result<PathBuf, String> {
    if bundle.is_file() {
        return Ok(bundle.to_path_buf());
    }
    let stem = bundle
        .file_stem()
        .ok_or_else(|| format!("{}: not a bundle", bundle.display()))?;
    for architecture in architectures {
        let folder = bundle.join("Contents").join(architecture);
        let named = folder.join(stem).with_extension(extension);
        if named.is_file() {
            return Ok(named);
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case(extension))
            {
                return Ok(path);
            }
        }
    }
    Err(format!(
        "{}: no library for {} in the bundle",
        bundle.display(),
        architectures.join(" or ")
    ))
}

#[cfg(target_os = "windows")]
mod platform {
    use super::library_in;
    use std::path::Path;
    use vst3::Steinberg::IPluginFactory;

    #[cfg(target_arch = "x86_64")]
    const ARCHITECTURES: &[&str] = &["x86_64-win"];
    #[cfg(target_arch = "aarch64")]
    const ARCHITECTURES: &[&str] = &["arm64-win", "arm64x-win", "arm64ec-win"];
    #[cfg(target_arch = "x86")]
    const ARCHITECTURES: &[&str] = &["x86-win"];

    type Exit = unsafe extern "C" fn() -> bool;

    pub(super) struct Binary {
        library: Option<libloading::Library>,
        exit: Option<Exit>,
    }

    impl Binary {
        pub(super) unsafe fn open(bundle: &Path) -> Result<Self, String> {
            let path = library_in(bundle, ARCHITECTURES, "vst3")?;
            // Its own folder first when resolving the libraries it needs, as the SDK's host does.
            // SAFETY: the caller's.
            let library = unsafe {
                libloading::os::windows::Library::load_with_flags(
                    &path,
                    libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
                )
            }
            .map_err(|error| format!("{}: {error}", path.display()))?;
            let library: libloading::Library = library.into();
            // SAFETY: `InitDll` and `ExitDll`, where present, take nothing and return a bool.
            unsafe {
                if let Ok(init) = library.get::<Exit>(b"InitDll\0")
                    && !init()
                {
                    return Err(format!("{}: InitDll failed", path.display()));
                }
                let exit = library.get::<Exit>(b"ExitDll\0").ok().map(|symbol| *symbol);
                Ok(Self {
                    library: Some(library),
                    exit,
                })
            }
        }

        pub(super) unsafe fn factory(&self) -> Result<*mut IPluginFactory, String> {
            let library = self.library.as_ref().ok_or("the module is unloaded")?;
            // SAFETY: `GetPluginFactory` takes nothing and returns the factory.
            unsafe {
                let get = library
                    .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(
                        b"GetPluginFactory\0",
                    )
                    .map_err(|error| format!("GetPluginFactory: {error}"))?;
                Ok(get())
            }
        }
    }

    impl Drop for Binary {
        fn drop(&mut self) {
            if let Some(exit) = self.exit {
                // SAFETY: called once, after every object of the module's was released.
                unsafe { exit() };
            }
            self.library = None;
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::library_in;
    use std::ffi::c_void;
    use std::path::Path;
    use vst3::Steinberg::IPluginFactory;

    #[cfg(target_arch = "x86_64")]
    const ARCHITECTURES: &[&str] = &["x86_64-linux"];
    #[cfg(target_arch = "aarch64")]
    const ARCHITECTURES: &[&str] = &["aarch64-linux"];
    #[cfg(target_arch = "x86")]
    const ARCHITECTURES: &[&str] = &["i386-linux", "i686-linux"];
    #[cfg(target_arch = "arm")]
    const ARCHITECTURES: &[&str] = &["armv7l-linux", "armv7a-linux"];

    type Exit = unsafe extern "C" fn() -> bool;

    pub(super) struct Binary {
        library: Option<libloading::Library>,
        exit: Option<Exit>,
    }

    impl Binary {
        pub(super) unsafe fn open(bundle: &Path) -> Result<Self, String> {
            let path = library_in(bundle, ARCHITECTURES, "so")?;
            // Never unmapped, like every plugin library the collection's hosts load: a library
            // built with Rust leaves per-thread destructors behind, and glibc calls them at
            // thread exit (the workspace's contract; MXM Player's `entry.rs`).
            // SAFETY: the caller's.
            let library = unsafe {
                libloading::os::unix::Library::open(
                    Some(&path),
                    libc::RTLD_NOW | libc::RTLD_LOCAL | libc::RTLD_NODELETE,
                )
            }
            .map_err(|error| format!("{}: {error}", path.display()))?;
            let handle = library.into_raw();
            // SAFETY: `handle` came from `into_raw` just above.
            let library: libloading::Library =
                unsafe { libloading::os::unix::Library::from_raw(handle) }.into();
            // SAFETY: `ModuleEntry` takes the library's handle; `ModuleExit` nothing.
            unsafe {
                if let Ok(entry) =
                    library.get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry\0")
                    && !entry(handle)
                {
                    return Err(format!("{}: ModuleEntry failed", path.display()));
                }
                let exit = library
                    .get::<Exit>(b"ModuleExit\0")
                    .ok()
                    .map(|symbol| *symbol);
                Ok(Self {
                    library: Some(library),
                    exit,
                })
            }
        }

        pub(super) unsafe fn factory(&self) -> Result<*mut IPluginFactory, String> {
            let library = self.library.as_ref().ok_or("the module is unloaded")?;
            // SAFETY: `GetPluginFactory` takes nothing and returns the factory.
            unsafe {
                let get = library
                    .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(
                        b"GetPluginFactory\0",
                    )
                    .map_err(|error| format!("GetPluginFactory: {error}"))?;
                Ok(get())
            }
        }
    }

    impl Drop for Binary {
        fn drop(&mut self) {
            if let Some(exit) = self.exit {
                // SAFETY: called once, after every object of the module's was released.
                unsafe { exit() };
            }
            // Mapped for good (RTLD_NODELETE): dropping only releases the handle.
            self.library = None;
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{CStr, c_char, c_void};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use vst3::Steinberg::IPluginFactory;

    type CFTypeRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFURLRef = *const c_void;
    type CFBundleRef = *mut c_void;
    type CFStringRef = *const c_void;
    const UTF8: u32 = 0x0800_0100; // kCFStringEncodingUTF8

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: CFAllocatorRef,
            buffer: *const u8,
            length: isize,
            is_directory: u8,
        ) -> CFURLRef;
        fn CFBundleCreate(allocator: CFAllocatorRef, url: CFURLRef) -> CFBundleRef;
        fn CFBundleLoadExecutable(bundle: CFBundleRef) -> u8;
        fn CFBundleGetFunctionPointerForName(bundle: CFBundleRef, name: CFStringRef)
        -> *mut c_void;
        fn CFStringCreateWithCString(
            allocator: CFAllocatorRef,
            text: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(object: CFTypeRef);
    }

    type Entry = unsafe extern "C" fn(CFBundleRef) -> bool;
    type Exit = unsafe extern "C" fn() -> bool;
    type GetFactory = unsafe extern "system" fn() -> *mut IPluginFactory;

    pub(super) struct Binary {
        bundle: CFBundleRef,
        exit: Option<Exit>,
        get_factory: GetFactory,
    }

    unsafe fn function(bundle: CFBundleRef, names: &[&CStr]) -> *mut c_void {
        for name in names {
            // SAFETY: `name` is NUL-terminated; the CFString is released after the lookup.
            unsafe {
                let text = CFStringCreateWithCString(std::ptr::null(), name.as_ptr(), UTF8);
                if text.is_null() {
                    continue;
                }
                let found = CFBundleGetFunctionPointerForName(bundle, text);
                CFRelease(text);
                if !found.is_null() {
                    return found;
                }
            }
        }
        std::ptr::null_mut()
    }

    impl Binary {
        pub(super) unsafe fn open(path: &Path) -> Result<Self, String> {
            let bytes = path.as_os_str().as_bytes();
            // SAFETY: CoreFoundation calls with valid arguments; every object created is released
            // on every path, the bundle in `Drop`.
            unsafe {
                let url = CFURLCreateFromFileSystemRepresentation(
                    std::ptr::null(),
                    bytes.as_ptr(),
                    bytes.len() as isize,
                    1,
                );
                if url.is_null() {
                    return Err(format!("{}: not a bundle path", path.display()));
                }
                let bundle = CFBundleCreate(std::ptr::null(), url);
                CFRelease(url);
                if bundle.is_null() {
                    return Err(format!("{}: not a bundle", path.display()));
                }
                if CFBundleLoadExecutable(bundle) == 0 {
                    CFRelease(bundle);
                    return Err(format!("{}: its executable did not load", path.display()));
                }
                let get_factory = function(bundle, &[c"GetPluginFactory"]);
                if get_factory.is_null() {
                    CFRelease(bundle);
                    return Err(format!("{}: no GetPluginFactory", path.display()));
                }
                let entry = function(bundle, &[c"bundleEntry", c"BundleEntry"]);
                if !entry.is_null() && !std::mem::transmute::<*mut c_void, Entry>(entry)(bundle) {
                    CFRelease(bundle);
                    return Err(format!("{}: bundleEntry failed", path.display()));
                }
                let exit = function(bundle, &[c"bundleExit", c"BundleExit"]);
                Ok(Self {
                    bundle,
                    exit: (!exit.is_null()).then(|| std::mem::transmute::<*mut c_void, Exit>(exit)),
                    get_factory: std::mem::transmute::<*mut c_void, GetFactory>(get_factory),
                })
            }
        }

        pub(super) unsafe fn factory(&self) -> Result<*mut IPluginFactory, String> {
            // SAFETY: `GetPluginFactory` takes nothing and returns the factory.
            Ok(unsafe { (self.get_factory)() })
        }
    }

    impl Drop for Binary {
        fn drop(&mut self) {
            // SAFETY: called once, after every object of the module's was released. The
            // executable stays loaded, as hosts on macOS leave it.
            unsafe {
                if let Some(exit) = self.exit {
                    exit();
                }
                CFRelease(self.bundle);
            }
        }
    }
}
