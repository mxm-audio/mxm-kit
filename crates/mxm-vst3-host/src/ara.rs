//! ARA (Celemony's Audio Random Access, SDK 2.3), VST3 to CLAP: a host that reaches ARA the CLAP
//! way reaches a VST3's through this crate, as it reaches the rest of the plugin.
//!
//! - **The factory.** A VST3 module lists ARA as a class of category [`ARA_MAIN_FACTORY_CLASS`],
//!   whose object (`IMainFactory`) gives the `ARAFactory`. The entry offers those classes as
//!   ARA's CLAP factory, `org.ara-audio.ara.factory/2` (`ARACLAP.h`): each one's `ARAFactory`, and
//!   the plugin id of the audio class it belongs to. That is the audio class of the same name, or
//!   the only one when the module has one of each, as the SDK's own loader pairs them
//!   (`ARA_Examples/ExamplesCommon/PlugInHosting/VST3Loader.cpp`). Those audio classes also carry
//!   the feature `ara:supported`.
//! - **The extension.** A component that answers `IPlugInEntryPoint2` is offered ARA's CLAP plugin
//!   extension, `org.ara-audio.ara.pluginextension/2`: `get_factory` is `IPlugInEntryPoint`'s,
//!   `bind_to_document_controller` is `bindToDocumentControllerWithRoles`, refused while active, as
//!   ARA binds before activation.
//!
//! ARA's own types are opaque pointers here: the crate needs no ARA bindings. The VST3 interfaces
//! are this crate's copy of `ARAVST3.h`'s, written as `outputs_in_use.rs` writes its own; the CLAP
//! structs its copy of `ARACLAP.h`'s, as every host and plugin keeps.

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, OnceLock};

use clack_plugin::extensions::prelude::*;
use clack_plugin::factory::{Factory, FactoryImplementation, FactoryWrapper, RawFactoryPointer};
use vst3::Steinberg::Vst::IComponent;
use vst3::Steinberg::{FUnknown, FUnknownVtbl, TUID};
use vst3::com_scrape_types::{Guid, Inherits, Unknown};
use vst3::{ComPtr, Interface};

use crate::entry::ModuleSlot;
use crate::module::ClassInfo;
use crate::outputs_in_use::{guid, interface};
use crate::plugin::Vst3Plugin;

/// The category of an ARA main factory class (`kARAMainFactoryClass`).
pub(crate) const ARA_MAIN_FACTORY_CLASS: &str = "ARA Main Factory Class";

/// ARA's CLAP factory, and the draft id hosts before ARA 2.3 ask for.
const ARA_FACTORY: &CStr = c"org.ara-audio.ara.factory/2";
const ARA_FACTORY_COMPAT: &CStr = c"org.ara-audio.ara.factory.draft/2";
/// ARA's CLAP plugin extension, and its draft id.
const ARA_PLUGIN_EXTENSION: &CStr = c"org.ara-audio.ara.pluginextension/2";
const ARA_PLUGIN_EXTENSION_COMPAT: &CStr = c"org.ara-audio.ara.pluginextension.draft/2";

/// The feature of a CLAP plugin that offers ARA (`CLAP_PLUGIN_FEATURE_ARA_SUPPORTED`).
pub(crate) const ARA_SUPPORTED: &CStr = c"ara:supported";

/// An ARA main factory class a module lists.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AraClass {
    pub cid: TUID,
    pub name: String,
}

/// Which audio class each ARA class belongs to, as `(audio class, ARA class)` indices: the audio
/// class of the same name, or, with one of each, the only one. An ARA class without one is left
/// out, and no audio class is taken twice.
pub(crate) fn pairs(classes: &[ClassInfo], ara: &[AraClass]) -> Vec<(usize, usize)> {
    if classes.len() == 1 && ara.len() == 1 {
        return vec![(0, 0)];
    }
    let mut found: Vec<(usize, usize)> = Vec::new();
    for (index, class) in ara.iter().enumerate() {
        let audio = classes.iter().enumerate().position(|(audio, info)| {
            info.name == class.name && !found.iter().any(|(taken, _)| *taken == audio)
        });
        if let Some(audio) = audio {
            found.push((audio, index));
        }
    }
    found
}

// ARA's VST3 interfaces (`ARAVST3.h`), the host's side: called, never implemented.

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IMainFactory {
    vtbl: *const IMainFactoryVtbl,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IMainFactoryVtbl {
    base: FUnknownVtbl,
    getFactory: unsafe extern "system" fn(this: *mut IMainFactory) -> *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IPlugInEntryPoint {
    vtbl: *const IPlugInEntryPointVtbl,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IPlugInEntryPointVtbl {
    base: FUnknownVtbl,
    getFactory: unsafe extern "system" fn(this: *mut IPlugInEntryPoint) -> *const c_void,
    /// Deprecated in ARA 2: binds every role. Never called here.
    bindToDocumentController: unsafe extern "system" fn(
        this: *mut IPlugInEntryPoint,
        document_controller: *const c_void,
    ) -> *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IPlugInEntryPoint2 {
    vtbl: *const IPlugInEntryPoint2Vtbl,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IPlugInEntryPoint2Vtbl {
    base: FUnknownVtbl,
    bindToDocumentControllerWithRoles: unsafe extern "system" fn(
        this: *mut IPlugInEntryPoint2,
        document_controller: *const c_void,
        known_roles: u32,
        assigned_roles: u32,
    ) -> *const c_void,
}

interface!(
    IMainFactory,
    IMainFactoryVtbl,
    0xDB2A1669,
    0xFAFD42A5,
    0xA82F864F,
    0x7B6872EA
);
interface!(
    IPlugInEntryPoint,
    IPlugInEntryPointVtbl,
    0x12814E54,
    0xA1CE4076,
    0x82B96813,
    0x16950BD6
);
interface!(
    IPlugInEntryPoint2,
    IPlugInEntryPoint2Vtbl,
    0xCD9A5913,
    0xC9EB46D7,
    0x96CA53AD,
    0xD1DB89F5
);

// ARA's CLAP structs (`ARACLAP.h`), as C lays them out.

/// `clap_ara_factory_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct clap_ara_factory {
    pub get_factory_count: Option<unsafe extern "C" fn(factory: *const clap_ara_factory) -> u32>,
    pub get_ara_factory:
        Option<unsafe extern "C" fn(factory: *const clap_ara_factory, index: u32) -> *const c_void>,
    pub get_plugin_id:
        Option<unsafe extern "C" fn(factory: *const clap_ara_factory, index: u32) -> *const c_char>,
}

/// `clap_ara_plugin_extension_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct clap_ara_plugin_extension {
    pub get_factory: Option<unsafe extern "C" fn(plugin: *const clap_plugin) -> *const c_void>,
    pub bind_to_document_controller: Option<
        unsafe extern "C" fn(
            plugin: *const clap_plugin,
            document_controller: *const c_void,
            known_roles: u32,
            assigned_roles: u32,
        ) -> *const c_void,
    >,
}

/// ARA's CLAP factory, as clack finds a factory by its ids.
#[derive(Clone, Copy)]
pub(crate) struct AraFactory<'a>(RawFactoryPointer<'a, clap_ara_factory>);

// SAFETY: the ids are ARACLAP.h's for `clap_ara_factory_t`, which `Raw` lays out.
unsafe impl<'a> Factory<'a> for AraFactory<'a> {
    const IDENTIFIERS: &'static [&'static CStr] = &[ARA_FACTORY, ARA_FACTORY_COMPAT];
    type Raw = clap_ara_factory;

    unsafe fn from_raw(raw: RawFactoryPointer<'a, Self::Raw>) -> Self {
        Self(raw)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
impl AraFactory<'_> {
    /// How many plugins offer ARA.
    pub(crate) fn count(&self) -> u32 {
        let factory = self.0.get();
        // SAFETY: the factory's own function, given its own pointer.
        factory
            .get_factory_count
            .map_or(0, |count| unsafe { count(self.0.as_ptr()) })
    }

    /// Plugin `index`'s `ARAFactory`, null if it has none.
    pub(crate) fn ara_factory(&self, index: u32) -> *const c_void {
        let factory = self.0.get();
        // SAFETY: as above.
        factory
            .get_ara_factory
            .map_or(std::ptr::null(), |get| unsafe {
                get(self.0.as_ptr(), index)
            })
    }

    /// Plugin `index`'s CLAP id.
    pub(crate) fn plugin_id(&self, index: u32) -> Option<&CStr> {
        let factory = self.0.get();
        // SAFETY: as above; the id lives as long as the entry.
        let id = factory
            .get_plugin_id
            .map(|get| unsafe { get(self.0.as_ptr(), index) })?;
        // SAFETY: a non-null id is a NUL-terminated string the factory keeps.
        (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) })
    }
}

/// One plugin of the module that offers ARA.
struct AraPlugin {
    cid: TUID,
    plugin_id: CString,
    /// Its `IMainFactory`, made when its `ARAFactory` is first asked for, and the `ARAFactory`,
    /// valid while the object lives: until the entry goes, as ARACLAP.h asks.
    main: OnceLock<Option<MainFactory>>,
}

struct MainFactory {
    _object: ComPtr<IMainFactory>,
    factory: *const c_void,
}

// SAFETY: the object is the module's, which may be used from any thread (`module.rs`); the
// `ARAFactory` it gives is a static the host only reads.
unsafe impl Send for MainFactory {}
// SAFETY: as above.
unsafe impl Sync for MainFactory {}

/// The entry's ARA factory, over the same module as its plugin factory.
pub(crate) struct Vst3AraFactory {
    module: Arc<ModuleSlot>,
    plugins: Vec<AraPlugin>,
}

impl Vst3AraFactory {
    fn ara_factory(&self, index: u32) -> *const c_void {
        let Some(plugin) = self.plugins.get(index as usize) else {
            return std::ptr::null();
        };
        plugin
            .main
            .get_or_init(|| {
                let module = self.module.get()?;
                // SAFETY: the class is one the module lists as an ARA main factory, whose object
                // answers `IMainFactory`.
                let object = unsafe { module.create::<IMainFactory>(&plugin.cid) }?;
                let ptr = object.as_ptr();
                // SAFETY: the object's own function, through its own vtable.
                let factory = unsafe { ((*(*ptr).vtbl).getFactory)(ptr) };
                (!factory.is_null()).then_some(MainFactory {
                    _object: object,
                    factory,
                })
            })
            .as_ref()
            .map_or(std::ptr::null(), |main| main.factory)
    }
}

/// [`Vst3AraFactory`] as the CLAP struct clack registers.
pub(crate) struct AraFactoryWrapper(FactoryWrapper<clap_ara_factory, Vst3AraFactory>);

impl AraFactoryWrapper {
    /// The ARA factory of a module's ARA classes, `None` when it has none that pairs with an audio
    /// class. `ids` are the plugin ids of `classes`, the module's audio classes.
    pub(crate) fn new(
        module: Arc<ModuleSlot>,
        classes: &[ClassInfo],
        ids: &[String],
        ara: &[AraClass],
    ) -> Option<Self> {
        let plugins: Vec<AraPlugin> = pairs(classes, ara)
            .into_iter()
            .filter_map(|(audio, index)| {
                Some(AraPlugin {
                    cid: ara[index].cid,
                    plugin_id: CString::new(ids[audio].as_str()).ok()?,
                    main: OnceLock::new(),
                })
            })
            .collect();
        (!plugins.is_empty()).then(|| {
            Self(FactoryWrapper::new(
                clap_ara_factory {
                    get_factory_count: Some(get_factory_count),
                    get_ara_factory: Some(get_ara_factory),
                    get_plugin_id: Some(get_plugin_id),
                },
                Vst3AraFactory { module, plugins },
            ))
        })
    }
}

// SAFETY: the wrapper's raw struct is a complete `clap_ara_factory_t`, its functions below.
unsafe impl<'a> FactoryImplementation<'a> for AraFactoryWrapper {
    type Factory = AraFactory<'a>;
    type Wrapped = Vst3AraFactory;

    fn wrapper(&self) -> &FactoryWrapper<clap_ara_factory, Vst3AraFactory> {
        &self.0
    }
}

unsafe extern "C" fn get_factory_count(factory: *const clap_ara_factory) -> u32 {
    // SAFETY: the host passes the pointer the entry gave it, this wrapper's.
    unsafe {
        FactoryWrapper::<clap_ara_factory, Vst3AraFactory>::handle(factory, |factory| {
            Ok(factory.plugins.len() as u32)
        })
    }
    .unwrap_or(0)
}

unsafe extern "C" fn get_ara_factory(
    factory: *const clap_ara_factory,
    index: u32,
) -> *const c_void {
    // SAFETY: as above.
    unsafe {
        FactoryWrapper::<clap_ara_factory, Vst3AraFactory>::handle(factory, |factory| {
            Ok(factory.ara_factory(index))
        })
    }
    .unwrap_or(std::ptr::null())
}

unsafe extern "C" fn get_plugin_id(factory: *const clap_ara_factory, index: u32) -> *const c_char {
    // SAFETY: as above.
    unsafe {
        FactoryWrapper::<clap_ara_factory, Vst3AraFactory>::handle(factory, |factory| {
            Ok(factory
                .plugins
                .get(index as usize)
                .map_or(std::ptr::null(), |plugin| plugin.plugin_id.as_ptr()))
        })
    }
    .unwrap_or(std::ptr::null())
}

/// A component's ARA entry points: `IPlugInEntryPoint2` to bind, `IPlugInEntryPoint` for the
/// factory.
pub(crate) struct EntryPoints {
    entry: Option<ComPtr<IPlugInEntryPoint>>,
    entry2: ComPtr<IPlugInEntryPoint2>,
}

/// The component's ARA entry points, `None` when it doesn't answer `IPlugInEntryPoint2` (no ARA,
/// or only ARA 1's binding of every role, which this crate doesn't offer).
pub(crate) fn entry_points(component: &ComPtr<IComponent>) -> Option<EntryPoints> {
    Some(EntryPoints {
        entry2: component.cast::<IPlugInEntryPoint2>()?,
        entry: component.cast::<IPlugInEntryPoint>(),
    })
}

impl EntryPoints {
    /// The component's `ARAFactory`, null if it doesn't say.
    fn factory(&self) -> *const c_void {
        let Some(entry) = &self.entry else {
            return std::ptr::null();
        };
        let ptr = entry.as_ptr();
        // SAFETY: the component's own function, through its own vtable.
        unsafe { ((*(*ptr).vtbl).getFactory)(ptr) }
    }

    /// Binds the component to `document_controller` with `assigned` of the `known` roles.
    ///
    /// # Safety
    ///
    /// On the main thread, before the plugin is first activated, `document_controller` a live
    /// `ARADocumentControllerRef` of this plugin's factory.
    unsafe fn bind(
        &self,
        document_controller: *const c_void,
        known: u32,
        assigned: u32,
    ) -> *const c_void {
        let ptr = self.entry2.as_ptr();
        // SAFETY: the caller's; the component's own function, through its own vtable.
        unsafe {
            ((*(*ptr).vtbl).bindToDocumentControllerWithRoles)(
                ptr,
                document_controller,
                known,
                assigned,
            )
        }
    }
}

// SAFETY: the interfaces are the component's, which CLAP's main thread drives; `Shared` is
// `Sync` only because each method is called on the thread its CLAP counterpart is.
unsafe impl Send for EntryPoints {}
// SAFETY: as above.
unsafe impl Sync for EntryPoints {}

/// ARA's CLAP plugin extension, offered by a plugin whose component has [`EntryPoints`].
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct PluginAra(RawExtension<PluginExtensionSide, clap_ara_plugin_extension>);

// SAFETY: the ids are ARACLAP.h's for `clap_ara_plugin_extension_t`, which the raw struct lays
// out.
unsafe impl Extension for PluginAra {
    const IDENTIFIERS: &[&CStr] = &[ARA_PLUGIN_EXTENSION, ARA_PLUGIN_EXTENSION_COMPAT];
    type ExtensionSide = PluginExtensionSide;

    unsafe fn from_raw(raw: RawExtension<Self::ExtensionSide>) -> Self {
        // SAFETY: the caller upholds that the pointer is of this extension's type.
        Self(unsafe { raw.cast() })
    }
}

// SAFETY: the raw struct is a complete `clap_ara_plugin_extension_t`, its functions below.
unsafe impl ExtensionImplementation<Vst3Plugin> for PluginAra {
    const IMPLEMENTATION: RawExtensionImplementation =
        RawExtensionImplementation::new(&clap_ara_plugin_extension {
            get_factory: Some(plugin_factory),
            bind_to_document_controller: Some(bind_to_document_controller),
        });
}

unsafe extern "C" fn plugin_factory(plugin: *const clap_plugin) -> *const c_void {
    // SAFETY: the host passes this plugin's pointer; only `Shared` is read, which is `Sync`.
    unsafe {
        PluginWrapper::<Vst3Plugin>::handle(plugin, |plugin| {
            Ok(plugin
                .shared()
                .ara
                .as_ref()
                .map_or(std::ptr::null(), EntryPoints::factory))
        })
    }
    .unwrap_or(std::ptr::null())
}

unsafe extern "C" fn bind_to_document_controller(
    plugin: *const clap_plugin,
    document_controller: *const c_void,
    known_roles: u32,
    assigned_roles: u32,
) -> *const c_void {
    if document_controller.is_null() {
        return std::ptr::null();
    }
    // SAFETY: the host passes this plugin's pointer, on the main thread as ARACLAP.h asks.
    unsafe {
        PluginWrapper::<Vst3Plugin>::handle(plugin, |plugin| {
            // ARA binds before the first activation; a bind while active is the host's error.
            if plugin.is_active() || plugin.shared().is_active() {
                return Ok(std::ptr::null());
            }
            Ok(plugin
                .shared()
                .ara
                .as_ref()
                .map_or(std::ptr::null(), |ara| {
                    ara.bind(document_controller, known_roles, assigned_roles)
                }))
        })
    }
    .unwrap_or(std::ptr::null())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(name: &str) -> ClassInfo {
        ClassInfo {
            cid: [0; 16],
            name: name.to_owned(),
            vendor: String::new(),
            version: String::new(),
            sub_categories: "Fx".to_owned(),
        }
    }

    fn ara(name: &str, first: u8) -> AraClass {
        let mut cid = [0; 16];
        cid[0] = first as c_char;
        AraClass {
            cid,
            name: name.to_owned(),
        }
    }

    #[test]
    fn one_of_each_pairs_whatever_the_names() {
        assert_eq!(pairs(&[class("Pitch")], &[ara("Pitch ARA", 1)]), [(0, 0)]);
    }

    #[test]
    fn several_pair_by_name_and_an_unpaired_ara_class_is_left_out() {
        let classes = [class("A"), class("B"), class("C")];
        let found = pairs(&classes, &[ara("C", 1), ara("Z", 2), ara("A", 3)]);
        assert_eq!(found, [(2, 0), (0, 2)]);
    }

    #[test]
    fn no_audio_class_is_taken_twice() {
        let classes = [class("A"), class("B")];
        assert_eq!(pairs(&classes, &[ara("A", 1), ara("A", 2)]), [(0, 0)]);
    }

    #[test]
    fn a_module_without_ara_has_no_pairs() {
        assert!(pairs(&[class("A")], &[]).is_empty());
    }

    /// The SDK's `ARATestPlugIn.vst3`, built where the workspace builds the ARA SDK
    /// (`$MXM_BUILD_ROOT/ara-sdk`), if it is there.
    fn test_plugin() -> Option<std::path::PathBuf> {
        let root = std::env::var_os("MXM_BUILD_ROOT")?;
        let path = std::path::Path::new(&root).join("ara-sdk/build/bin/Release/ARATestPlugIn.vst3");
        path.exists().then_some(path)
    }

    struct TestHost;

    impl clack_host::prelude::HostHandlers for TestHost {
        type Shared<'a> = ();
        type MainThread<'a> = ();
        type AudioProcessor<'a> = ();
    }

    #[test]
    #[ignore = "needs the ARA SDK's ARATestPlugIn.vst3 under $MXM_BUILD_ROOT/ara-sdk"]
    fn the_sdks_test_plugin_offers_its_ara_factory_and_extension() {
        use clack_host::prelude::*;
        let Some(path) = test_plugin() else {
            eprintln!("skipped: no ARATestPlugIn.vst3 under $MXM_BUILD_ROOT/ara-sdk");
            return;
        };
        let path = CString::new(path.to_str().expect("a UTF-8 path")).expect("a path without NUL");
        let entry = PluginEntry::load_from_clack::<crate::Vst3Entry>(&path).expect("the module");
        let descriptor = entry
            .get_plugin_factory()
            .expect("its plugin factory")
            .plugin_descriptors()
            .next()
            .expect("its audio class");
        let id = descriptor.id().expect("an id").to_owned();
        assert!(
            descriptor
                .features()
                .any(|feature| feature == ARA_SUPPORTED)
        );

        let ara = entry.get_factory::<AraFactory>().expect("ARA's factory");
        assert_eq!(ara.count(), 1);
        assert_eq!(ara.plugin_id(0), Some(id.as_c_str()));
        let factory = ara.ara_factory(0);
        assert!(!factory.is_null());
        // `ARAFactory::structSize`, its first field: more than the generations before `factoryID`.
        // SAFETY: a non-null `ARAFactory` is a struct that begins with its size.
        assert!(unsafe { factory.cast::<usize>().read_unaligned() } > 16);
        assert_eq!(ara.ara_factory(0), factory, "one object, one ARAFactory");
        assert!(ara.ara_factory(1).is_null());

        let host = HostInfo::new("mxm-vst3-host tests", "mxm", "https://mxm.dk", "0").unwrap();
        let instance = PluginInstance::<TestHost>::new(|_| (), |_| (), &entry, &id, &host)
            .expect("the plugin");
        let handle = instance.plugin_shared_handle();
        let extension: PluginAra = handle.get_extension().expect("ARA's plugin extension");
        let raw = handle.use_extension(&extension.0);
        // SAFETY: the plugin's own function, given its own pointer.
        let own = unsafe { (raw.get_factory.expect("get_factory"))(handle.as_raw()) };
        assert_eq!(
            own, factory,
            "ARACLAP.h: the instance's ARAFactory is the factory's"
        );
        // SAFETY: as above; a null document controller is refused before the component sees it.
        let bound = unsafe {
            (raw.bind_to_document_controller.expect("bind"))(
                handle.as_raw(),
                std::ptr::null(),
                7,
                7,
            )
        };
        assert!(bound.is_null());
    }
}
