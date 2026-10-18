//! Each bundled VST3's `moduleinfo.json`, which hosts since VST 3.7.5 read to list a plugin without
//! loading it. Here because nice-plug's bundler doesn't make one; the SDK's own build makes it
//! after linking, by loading the module and asking its factory (`moduleinfotool`), and
//! `mxm_vst3_host::module_info` does the same.
//!
//! Never fatal: a VST3 without the file is listed by loading it, as before. A file that isn't this
//! build's is worse than none, so one that can't be written is removed.

use std::path::{Path, PathBuf};

/// Writes `Contents/Resources/moduleinfo.json` into the `.vst3` of each package bundled that
/// `bundles` names and that the build made, and says what it wrote or why it couldn't.
pub fn write(bundles: &[(String, String)], packages: &[String], bundled: &Path) {
    for (_, bundle) in bundles
        .iter()
        .filter(|(package, _)| packages.contains(package))
    {
        let vst3 = bundled.join(format!("{bundle}.vst3"));
        if !vst3.is_dir() {
            continue;
        }
        match write_one(&vst3) {
            Ok(path) => println!("Wrote {}", path.display()),
            Err(error) => {
                let _ = std::fs::remove_file(vst3.join(mxm_vst3_host::MODULE_INFO_PATH));
                eprintln!("No moduleinfo.json in {}: {error}", vst3.display());
            }
        }
    }
}

fn write_one(vst3: &Path) -> Result<PathBuf, String> {
    // SAFETY: the module is the one this build just made, and loading it runs only its own code,
    // as the SDK's build does when it makes the same file.
    let json = unsafe { mxm_vst3_host::module_info(vst3) }?;
    let path = vst3.join(mxm_vst3_host::MODULE_INFO_PATH);
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("{} can't be made: {error}", folder.display()))?;
    }
    std::fs::write(&path, json).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}
