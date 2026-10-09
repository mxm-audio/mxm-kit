//! VST3 plugins in a CLAP host.
//!
//! MXM Player and newDAWn host CLAP plugins through clack-host. Rather than teach both engines a
//! second plugin format, this crate makes a `.vst3` module **look like a CLAP plugin entry**: a
//! [`Vst3Entry`] is a clack-plugin entry whose factory lists the module's audio classes, and each
//! plugin it creates is a CLAP plugin that drives one VST3 component underneath. A host loads it
//! in-process with clack-host's `PluginEntry::load_from_clack::<Vst3Entry>(path)`, and from there
//! everything it already does for CLAP — the compatibility envelope, quarantine, parameter panels,
//! state, latency compensation, the offline render, editor windows — applies unchanged.
//!
//! # What maps to what
//!
//! | CLAP | VST3 |
//! |---|---|
//! | the entry and its plugin factory | the module (`GetPluginFactory`) and its "Audio Module Class" classes |
//! | plugin id `vst3:<class id>` | the class ID, as the 32 hex digits the SDK prints, the same on every platform |
//! | `init` / `destroy` | `IComponent` and `IEditController` created, initialised, connected; terminated |
//! | `activate` / `deactivate` | `setupProcessing` and `setActive` |
//! | `start_processing` / `stop_processing` / `reset` | `setProcessing` (a reset is off, then on) |
//! | `process` | `IAudioProcessor::process`: parameter changes, note events, the transport |
//! | `params` | `IEditController` parameters; stepped ones as their steps, the rest normalised |
//! | parameter modulation | added to the parameter's value before it reaches the processor |
//! | `state` | the component's and the controller's state, in one blob |
//! | `audio-ports` / `note-ports` | the main audio buses and the first event bus |
//! | `latency` / `tail` | `getLatencySamples` / `getTailSamples` |
//! | `render` | the process mode, realtime or offline |
//! | `gui` | `IPlugView`: embedded, or floating in a window of this crate's |
//!
//! # Threads
//!
//! CLAP's main thread is VST3's UI thread, and CLAP's audio thread its processing thread. Edits a
//! plugin's editor makes (`IComponentHandler::performEdit`) travel to the audio thread through a
//! single-producer queue, become CLAP output events there, and reach the processor in the next
//! block; values the host sets travel back the other way so the editor follows automation. Nothing
//! on the audio thread allocates, locks or calls the edit controller.
//!
//! # Scanning runs no plugin code in the host
//!
//! A [`Vst3Entry`] reads its module's classes in a copy of the host started for it ([`scan`]
//! and [`serve_probe`], which a host calls first thing in `main`), so a module that hangs or asks
//! for a login while loading costs a scan [`PROBE_TIMEOUT`] at most, never the host. The module is
//! loaded into the host when the first plugin of it is created.
//!
//! # Modules stay loaded while used
//!
//! A module is loaded once per process however many entries and plugins use it, and released
//! when the last goes. On Linux its library is never unmapped, as for every plugin library in
//! the collection's hosts (`RTLD_NODELETE`).

// The VST3 headers' enumerations are `int` on Windows and `unsigned int` elsewhere, so a cast that
// is a no-op on one platform is needed on the other.
#![allow(clippy::unnecessary_cast)]

mod component;
mod entry;
mod events;
mod gui;
mod handler;
mod host_objects;
mod module;
mod params;
mod plugin;
#[cfg(target_os = "linux")]
mod run_loop;
mod scan;
mod strings;
mod window;

pub use entry::Vst3Entry;
pub use scan::{PROBE_ARG, PROBE_TIMEOUT, serve_probe};

use std::path::{Path, PathBuf};

/// The file extension of a VST3 bundle, or of a single-file module on Windows.
pub const EXTENSION: &str = "vst3";

/// The prefix of every plugin id this crate gives a VST3 class: `vst3:` and the class ID.
pub const ID_PREFIX: &str = "vst3:";

/// Whether `path` names a VST3 bundle (or a single-file module): its extension is `.vst3`, in any
/// case.
///
/// A host's scanner should not descend into such a folder: inside a Windows bundle the module
/// itself is another `.vst3`, which is the same plugin again.
pub fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
}

/// The folders VST3 plugins are installed in on this platform, as the SDK lists them, with any in
/// `VST3_PATH` first. Bundles may sit in folders below them.
pub fn standard_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(listed) = std::env::var_os("VST3_PATH") {
        folders.extend(std::env::split_paths(&listed));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(common) = std::env::var_os("COMMONPROGRAMFILES") {
            folders.push(PathBuf::from(common).join("VST3"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            folders.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("Common")
                    .join("VST3"),
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(PathBuf::from(home).join("Library/Audio/Plug-Ins/VST3"));
        }
        folders.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        folders.push(PathBuf::from("/Network/Library/Audio/Plug-Ins/VST3"));
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(PathBuf::from(home).join(".vst3"));
        }
        folders.push(PathBuf::from("/usr/lib/vst3"));
        folders.push(PathBuf::from("/usr/local/lib/vst3"));
    }
    folders
}

/// Why the last [`Vst3Entry`] for `bundle` failed to load, which clack's entry error cannot say.
pub fn load_error(bundle: &Path) -> Option<String> {
    entry::load_error(bundle)
}

/// The plugins the module at `bundle` offers, as `(plugin id, name)`, or why it can't be loaded:
/// read out of process when this program serves probes ([`serve_probe`]), loaded here otherwise.
///
/// # Safety
///
/// Loading a module here runs its code, as loading any plugin does.
pub unsafe fn probe(bundle: &Path) -> Result<Vec<(String, String)>, String> {
    let (found, _) = entry::read_classes(bundle)?;
    Ok(found
        .classes
        .iter()
        .map(|class| (entry::plugin_id(&class.cid), class.name.clone()))
        .collect())
}
