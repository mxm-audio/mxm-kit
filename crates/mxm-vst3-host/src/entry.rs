//! The CLAP entry a VST3 module is offered as, and its plugin factory.

use crate::module::{ClassInfo, Module};
use crate::plugin::{self, MainThread, Shared, Vst3Plugin};
use crate::scan::{self, ModuleClasses};
use crate::strings::class_id_text;
use clack_plugin::entry::prelude::*;
use clack_plugin::plugin::features;
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use vst3::Steinberg::TUID;

/// Why the last entry for each bundle failed to load, for [`crate::load_error`].
static ERRORS: Mutex<Vec<(PathBuf, String)>> = Mutex::new(Vec::new());

fn remember_error(bundle: &Path, reason: Option<String>) {
    let mut errors = ERRORS.lock().unwrap_or_else(|poison| poison.into_inner());
    errors.retain(|(held, _)| held != bundle);
    if let Some(reason) = reason {
        errors.push((bundle.to_path_buf(), reason));
    }
}

pub(crate) fn load_error(bundle: &Path) -> Option<String> {
    let errors = ERRORS.lock().unwrap_or_else(|poison| poison.into_inner());
    errors
        .iter()
        .find(|(held, _)| held == bundle)
        .map(|(_, reason)| reason.clone())
}

/// What a module offers, out of process when the host serves probes ([`scan`]), and the module
/// itself when it had to be loaded here to find out.
pub(crate) fn read_classes(bundle: &Path) -> Result<(ModuleClasses, Option<Arc<Module>>), String> {
    match scan::probe(bundle) {
        Some(found) => found.map(|classes| (classes, None)),
        None => {
            // SAFETY: loading a plugin is what the host asked for.
            let module = unsafe { Module::load(bundle) }?;
            let classes = ModuleClasses {
                vendor: module.vendor.clone(),
                classes: module.classes.clone(),
            };
            Ok((classes, Some(module)))
        }
    }
}

/// The plugin id of a VST3 class: `vst3:` and its class ID, the same on every platform.
pub(crate) fn plugin_id(cid: &TUID) -> String {
    format!("{}{}", crate::ID_PREFIX, class_id_text(cid))
}

/// A `.vst3` module as a CLAP plugin entry. Load it with clack-host:
///
/// ```ignore
/// let path = CString::new(bundle.to_str().unwrap())?;
/// let entry = PluginEntry::load_from_clack::<mxm_vst3_host::Vst3Entry>(&path)?;
/// ```
///
/// The path is the bundle's (or the single file's). Its factory offers every audio class the
/// module has, as [`crate::ID_PREFIX`] and the class ID. The classes are read out of process when
/// the host serves probes ([`crate::serve_probe`]), and the module itself is loaded into the host
/// only when a plugin of it is created. Why an entry failed is [`crate::load_error`].
pub struct Vst3Entry {
    factory: PluginFactoryWrapper<Vst3Factory>,
}

impl Entry for Vst3Entry {
    fn new(bundle_path: Option<&CStr>) -> Result<Self, EntryLoadError> {
        let path = bundle_path
            .and_then(|path| path.to_str().ok())
            .map(PathBuf::from)
            .ok_or(EntryLoadError)?;
        let (found, loaded) = match read_classes(&path) {
            Ok(found) => {
                remember_error(&path, None);
                found
            }
            Err(reason) => {
                remember_error(&path, Some(reason));
                return Err(EntryLoadError);
            }
        };
        let descriptors = found
            .classes
            .iter()
            .map(|class| descriptor(class, &found.vendor))
            .collect();
        let module = OnceLock::new();
        if let Some(loaded) = loaded {
            let _ = module.set(Ok(loaded));
        }
        Ok(Self {
            factory: PluginFactoryWrapper::new(Vst3Factory {
                bundle: path,
                classes: found.classes,
                descriptors,
                module,
            }),
        })
    }

    fn declare_factories<'a>(&'a self, builder: &mut EntryFactories<'a>) {
        builder.register_factory(&self.factory);
    }
}

pub(crate) struct Vst3Factory {
    bundle: PathBuf,
    classes: Vec<ClassInfo>,
    descriptors: Vec<PluginDescriptor>,
    /// The module, loaded when the first plugin is created.
    module: OnceLock<Result<Arc<Module>, String>>,
}

impl PluginFactoryImpl for Vst3Factory {
    fn plugin_count(&self) -> u32 {
        self.descriptors.len() as u32
    }

    fn plugin_descriptor(&self, index: u32) -> Option<&PluginDescriptor> {
        self.descriptors.get(index as usize)
    }

    fn create_plugin<'a>(
        &'a self,
        host_info: HostInfo<'a>,
        plugin_id: &CStr,
    ) -> Option<PluginInstance<'a>> {
        let index = self
            .descriptors
            .iter()
            .position(|descriptor| descriptor.id() == Some(plugin_id))?;
        let module = self
            .module
            // SAFETY: creating a plugin is what the host asked for.
            .get_or_init(|| unsafe { Module::load(&self.bundle) })
            .as_ref()
            .map_err(|reason| remember_error(&self.bundle, Some(reason.clone())))
            .ok()?
            .clone();
        let class = &self.classes[index];
        Some(PluginInstance::new_with_initializer::<Vst3Plugin, _>(
            host_info,
            &self.descriptors[index],
            move |host| {
                let (shared, parts) = plugin::create(&host, module, class)?;
                Ok((shared, move |shared: &'a Shared<'a>| {
                    Ok(MainThread::new(host, shared, parts))
                }))
            },
        ))
    }
}

/// A class's CLAP descriptor: its name, vendor and version, and features from its subcategories.
fn descriptor(class: &ClassInfo, module_vendor: &str) -> PluginDescriptor {
    let vendor = if class.vendor.is_empty() {
        module_vendor
    } else {
        &class.vendor
    };
    let parts: Vec<&str> = class.sub_categories.split('|').collect();
    let has = |part: &str| parts.contains(&part);
    let mut found: Vec<&'static CStr> = Vec::new();
    if class.is_instrument() {
        found.push(features::INSTRUMENT);
        for (part, feature) in [
            ("Synth", features::SYNTHESIZER),
            ("Sampler", features::SAMPLER),
            ("Drum", features::DRUM),
        ] {
            if has(part) {
                found.push(feature);
            }
        }
    } else {
        found.push(features::AUDIO_EFFECT);
        for (part, feature) in [
            ("Analyzer", features::ANALYZER),
            ("Delay", features::DELAY),
            ("Distortion", features::DISTORTION),
            ("Dynamics", features::COMPRESSOR),
            ("EQ", features::EQUALIZER),
            ("Filter", features::FILTER),
            ("Mastering", features::MASTERING),
            ("Pitch Shift", features::PITCH_SHIFTER),
            ("Restoration", features::RESTORATION),
            ("Reverb", features::REVERB),
            ("Tools", features::UTILITY),
        ] {
            if has(part) {
                found.push(feature);
            }
        }
    }
    for (part, feature) in [
        ("Mono", features::MONO),
        ("Stereo", features::STEREO),
        ("Surround", features::SURROUND),
    ] {
        if has(part) {
            found.push(feature);
        }
    }
    PluginDescriptor::new(&plugin_id(&class.cid), &class.name)
        .with_vendor(vendor)
        .with_version(&class.version)
        .with_description("VST3")
        .with_features(found)
}
