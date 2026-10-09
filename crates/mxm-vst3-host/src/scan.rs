//! Reading a module's classes without loading it into the host.
//!
//! Loading a VST3 module runs its code, and some take a long time or never return: UVI's Falcon
//! spends 27 seconds to more than a minute in Windows' loader before failing, on a machine where it
//! is installed but not set up. A host that scans its plugin folders in-process freezes on such a
//! module. So the classes are read by a copy of the host itself, started with [`PROBE_ARG`] and
//! the bundle's path: it loads the module, prints what it offers, and exits, and the host waits at
//! most [`PROBE_TIMEOUT`] for it. The host loads the module itself only when a plugin of it is
//! created.
//!
//! A host takes part by calling [`serve_probe`] first thing in `main`. A program that does not (a
//! test binary, a tool) is noticed by its answer lacking the [`HEADER`], and the module is loaded
//! in-process instead, as it would have been.

use crate::module::{ClassInfo, Module};
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The first argument that makes a host's process a probe (see [`serve_probe`]).
pub const PROBE_ARG: &str = "--mxm-vst3-probe";

/// How long a probe may take before its module is taken as one that does not load.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// The probe's first line, by which a host knows the child understood it.
const HEADER: &str = "MXM-VST3-PROBE 1";

/// What a module offers: its vendor and its audio classes.
pub(crate) struct ModuleClasses {
    pub vendor: String,
    pub classes: Vec<ClassInfo>,
}

/// When this process was started as a probe, probes the module named after [`PROBE_ARG`], prints
/// what it offers and exits. Otherwise returns at once. Call it first thing in a host's `main`.
pub fn serve_probe() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new(PROBE_ARG)) {
        return;
    }
    let Some(bundle) = args.next() else {
        std::process::exit(2);
    };
    println!("{HEADER}");
    // SAFETY: this process exists to load the module.
    match unsafe { Module::load(Path::new(&bundle)) } {
        Ok(module) => {
            println!("vendor\t{}", clean(&module.vendor));
            for class in &module.classes {
                let cid: String = class
                    .cid
                    .iter()
                    .map(|&b| format!("{:02X}", b as u8))
                    .collect();
                println!(
                    "class\t{cid}\t{}\t{}\t{}\t{}",
                    clean(&class.name),
                    clean(&class.vendor),
                    clean(&class.version),
                    clean(&class.sub_categories)
                );
            }
        }
        Err(reason) => println!("error\t{}", clean(&reason)),
    }
    use std::io::Write;
    let _ = std::io::stdout().flush();
    // The module is never unloaded: its exit code is no business of the host's.
    std::process::exit(0);
}

/// A field without the tabs and line breaks that separate fields.
fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// The classes of the module at `bundle`, read out of process; `None` when this program does not
/// serve probes, and the module must be loaded here instead.
pub(crate) fn probe(bundle: &Path) -> Option<Result<ModuleClasses, String>> {
    let exe = std::env::current_exe().ok()?;
    let mut command = Command::new(exe);
    command
        .arg(PROBE_ARG)
        .arg(bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: a console host's probe opens no console window.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < PROBE_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Some(Err(format!(
                    "{}: the module did not load within {} seconds",
                    bundle.display(),
                    PROBE_TIMEOUT.as_secs()
                )));
            }
        }
    }
    let text = reader.join().ok()?;
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return None;
    }
    let mut vendor = String::new();
    let mut classes = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["vendor", name] => vendor = (*name).to_owned(),
            ["class", cid, name, class_vendor, version, sub_categories] => {
                let Some(cid) = parse_cid(cid) else {
                    continue;
                };
                classes.push(ClassInfo {
                    cid,
                    name: (*name).to_owned(),
                    vendor: (*class_vendor).to_owned(),
                    version: (*version).to_owned(),
                    sub_categories: (*sub_categories).to_owned(),
                });
            }
            ["error", reason] => return Some(Err((*reason).to_owned())),
            _ => {}
        }
    }
    if classes.is_empty() && vendor.is_empty() {
        // Not even the module's vendor line: the probe died loading it.
        return Some(Err(format!(
            "{}: the module did not load",
            bundle.display()
        )));
    }
    Some(Ok(ModuleClasses { vendor, classes }))
}

fn parse_cid(text: &str) -> Option<vst3::Steinberg::TUID> {
    if text.len() != 32 {
        return None;
    }
    let mut cid = [0 as std::ffi::c_char; 16];
    for (index, byte) in cid.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()? as std::ffi::c_char;
    }
    Some(cid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_id_reads_back_from_its_hex() {
        let cid = vst3::uid(0x0123_4567, 0x89AB_CDEF, 0xFEDC_BA98, 0x7654_3210);
        let text: String = cid.iter().map(|&b| format!("{:02X}", b as u8)).collect();
        assert_eq!(parse_cid(&text), Some(cid));
        assert_eq!(parse_cid("12"), None);
    }

    #[test]
    fn a_program_that_does_not_serve_probes_is_noticed() {
        // This test binary is no host: it answers the probe argument with a test harness's error.
        assert!(probe(Path::new("nowhere.vst3")).is_none());
    }
}
