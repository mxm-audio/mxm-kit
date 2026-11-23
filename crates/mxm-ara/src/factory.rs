//! The plug-in's static `ARAFactory` and its global entry points.

use std::any::TypeId;
use std::collections::HashMap;
use std::ffi::{CString, c_char};
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::content::ContentType;
use crate::controller::Controller;
use crate::sys;
use crate::util::{c_string, implemented_size, read_field, struct_size};
use crate::validate::{check, clear_assert_function, set_assert_function};

/// The oldest API generation served: ARA 2.0 final (also the oldest on ARM).
pub const LOWEST_API_GENERATION: i32 = sys::kARAAPIGeneration_2_0_Final;
/// The newest the headers define: ARA 2.3 final.
pub const HIGHEST_API_GENERATION: i32 = sys::kARAAPIGeneration_2_3_Final;

/// What the factory tells hosts. All IDs are reverse-domain ASCII; none may ever change meaning.
#[derive(Clone, Copy, Debug)]
pub struct FactoryConfig {
    /// Must equal the plug-in's name in its companion APIs (the VST3 class name, the CLAP name).
    pub plug_in_name: &'static str,
    pub manufacturer_name: &'static str,
    pub information_url: &'static str,
    pub version: &'static str,
    /// Unique and versioned: changes when the archive IDs or capabilities change.
    pub factory_id: &'static str,
    /// The archives this controller writes; changes only when it can no longer read old ones.
    pub document_archive_id: &'static str,
    /// Older archive IDs it still reads, newest first.
    pub compatible_document_archive_ids: &'static [&'static str],
    pub analyzeable_content_types: &'static [ContentType],
    /// `ARAPlaybackTransformationFlags`; 0 for none.
    pub supported_playback_transformation_flags: i32,
    /// Whether `storeAudioSourceToAudioFileChunk` is offered.
    pub supports_storing_audio_file_chunks: bool,
}

impl FactoryConfig {
    /// What ARA requires of a factory's fields, as ARA_Library asserts it; empty when sound.
    pub fn problems(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        let url_id = |s: &str| s.len() > 5 && s.is_ascii();
        if !url_id(self.factory_id) {
            out.push("factory_id must be a reverse-domain ASCII ID");
        }
        if !url_id(self.document_archive_id) {
            out.push("document_archive_id must be a reverse-domain ASCII ID");
        }
        if !self
            .compatible_document_archive_ids
            .iter()
            .all(|s| url_id(s))
        {
            out.push("compatible archive IDs must be reverse-domain ASCII IDs");
        }
        for (s, what) in [
            (self.plug_in_name, "plug_in_name is empty"),
            (self.manufacturer_name, "manufacturer_name is empty"),
            (self.information_url, "information_url is empty"),
            (self.version, "version is empty"),
        ] {
            if s.is_empty() {
                out.push(what);
            }
        }
        let fades = sys::kARAPlaybackTransformationContentBasedFades;
        let flags = self.supported_playback_transformation_flags;
        if flags & fades != 0 && flags & fades != fades {
            out.push("content-based fades must be supported at both ends or neither");
        }
        out
    }
}

/// One plug-in's factory and its global state, leaked: it lives as long as the library.
pub(crate) struct FactoryBox {
    pub(crate) factory: sys::ARAFactory,
    pub(crate) config: FactoryConfig,
    /// The API generation chosen in `initializeARAWithConfiguration`, 0 when uninitialized.
    pub(crate) generation: AtomicI32,
    pub(crate) live_controllers: AtomicUsize,
    pub(crate) assert_address: std::sync::atomic::AtomicPtr<sys::ARAAssertFunction>,
    _strings: Vec<CString>,
    _compatible: Vec<*const c_char>,
    _types: Vec<sys::ARAContentType>,
}

// SAFETY: the raw pointers point into the box's own leaked, never-mutated storage.
unsafe impl Send for FactoryBox {}
unsafe impl Sync for FactoryBox {}

impl FactoryBox {
    fn new<C: Controller>() -> Self {
        let config = C::FACTORY;
        let mut strings = Vec::new();
        let mut keep = |s: &str| {
            let c = c_string(s);
            let p = c.as_ptr();
            strings.push(c);
            p
        };
        let plug_in_name = keep(config.plug_in_name);
        let manufacturer = keep(config.manufacturer_name);
        let url = keep(config.information_url);
        let version = keep(config.version);
        let factory_id = keep(config.factory_id);
        let archive_id = keep(config.document_archive_id);
        let compatible: Vec<*const c_char> = config
            .compatible_document_archive_ids
            .iter()
            .map(|s| keep(s))
            .collect();
        let types: Vec<sys::ARAContentType> = config
            .analyzeable_content_types
            .iter()
            .map(|t| t.raw())
            .collect();
        let factory = sys::ARAFactory {
            structSize: implemented_size!(sys::ARAFactory, supportsStoringAudioFileChunks),
            lowestSupportedApiGeneration: LOWEST_API_GENERATION,
            highestSupportedApiGeneration: HIGHEST_API_GENERATION,
            factoryID: factory_id,
            initializeARAWithConfiguration: Some(initialize::<C>),
            uninitializeARA: Some(uninitialize::<C>),
            plugInName: plug_in_name,
            manufacturerName: manufacturer,
            informationURL: url,
            version,
            createDocumentControllerWithDocument: Some(
                crate::document::create_document_controller::<C>,
            ),
            documentArchiveID: archive_id,
            compatibleDocumentArchiveIDsCount: compatible.len(),
            compatibleDocumentArchiveIDs: if compatible.is_empty() {
                std::ptr::null()
            } else {
                compatible.as_ptr()
            },
            analyzeableContentTypesCount: types.len(),
            analyzeableContentTypes: if types.is_empty() {
                std::ptr::null()
            } else {
                types.as_ptr()
            },
            supportedPlaybackTransformationFlags: config.supported_playback_transformation_flags,
            supportsStoringAudioFileChunks: crate::util::ara_bool(
                config.supports_storing_audio_file_chunks,
            ),
        };
        // The Vecs' heap buffers don't move when the Vecs move into the box.
        FactoryBox {
            factory,
            config,
            generation: AtomicI32::new(0),
            live_controllers: AtomicUsize::new(0),
            assert_address: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
            _strings: strings,
            _compatible: compatible,
            _types: types,
        }
    }
}

/// The factory of controller `C`, made on first use and kept for the library's life.
pub(crate) fn factory_box<C: Controller>() -> &'static FactoryBox {
    static FACTORIES: OnceLock<Mutex<HashMap<TypeId, &'static FactoryBox>>> = OnceLock::new();
    let mut map = FACTORIES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    map.entry(TypeId::of::<C>())
        .or_insert_with(|| Box::leak(Box::new(FactoryBox::new::<C>())))
}

/// The plug-in's `const ARAFactory*`: the same pointer every time, valid while the library is
/// loaded. What nice-plug's `AraHooks::factory` returns.
pub fn factory<C: Controller>() -> *const sys::ARAFactory {
    &raw const factory_box::<C>().factory
}

/// The API generation the host initialized ARA with, `None` before `initializeARAWithConfiguration`.
pub fn api_generation<C: Controller>() -> Option<i32> {
    let g = factory_box::<C>().generation.load(Ordering::SeqCst);
    (g != 0).then_some(g)
}

unsafe extern "C" fn initialize<C: Controller>(config: *const sys::ARAInterfaceConfiguration) {
    let fb = factory_box::<C>();
    type I = sys::ARAInterfaceConfiguration;
    // SAFETY: non-null and size checked before any field is read.
    let (desired, assert_address) = unsafe {
        if !check!(
            !config.is_null() && struct_size(config) >= implemented_size!(I, assertFunctionAddress),
            Argument,
            config,
            c"ARAInterfaceConfiguration missing or too small"
        ) {
            return;
        }
        (
            read_field!(config, I, desiredApiGeneration).unwrap_or(0),
            read_field!(config, I, assertFunctionAddress).unwrap_or(std::ptr::null_mut()),
        )
    };
    // The host's asserts first, so it hears a refusal of this very call.
    fb.assert_address.store(assert_address, Ordering::SeqCst);
    set_assert_function(assert_address);
    check!(
        fb.generation.load(Ordering::SeqCst) == 0,
        State,
        config,
        c"ARA initialized twice"
    );
    if !check!(
        (LOWEST_API_GENERATION..=HIGHEST_API_GENERATION).contains(&desired),
        Argument,
        config,
        c"desiredApiGeneration not supported"
    ) {
        fb.generation.store(0, Ordering::SeqCst);
        return;
    }
    fb.generation.store(desired, Ordering::SeqCst);
}

unsafe extern "C" fn uninitialize<C: Controller>() {
    let fb = factory_box::<C>();
    check!(
        fb.generation.load(Ordering::SeqCst) != 0,
        State,
        std::ptr::null::<u8>(),
        c"uninitializeARA without initialize"
    );
    check!(
        fb.live_controllers.load(Ordering::SeqCst) == 0,
        State,
        std::ptr::null::<u8>(),
        c"uninitializeARA with document controllers alive"
    );
    clear_assert_function(
        fb.assert_address
            .swap(std::ptr::null_mut(), Ordering::SeqCst),
    );
    fb.generation.store(0, Ordering::SeqCst);
}
