//! A module's `moduleinfo.json` (VST 3.7.5 and later): what its factory says about itself and its
//! classes, written into the bundle so a host can list the plugin without loading it.
//!
//! The keys, their order and which are left out follow the SDK's `moduleinfotool`
//! (`public.sdk/source/vst/moduleinfo/moduleinfocreator.cpp`, `createModuleInfo` and
//! `outputJson`), and class IDs are its `UID::toString` ([`class_id_text`]). Plain JSON, which the
//! SDK's JSON5 parser reads. For build tools only: making it runs the module's code in this
//! process, which a host's scan never does.

use crate::module::{ClassRecord, FactoryRecord, Module};
use crate::strings::class_id_text;
use std::fmt::Write;
use std::path::Path;
use vst3::Steinberg::PFactoryInfo_::FactoryFlags_::{
    kClassesDiscardable, kComponentNonDiscardable, kUnicode,
};

/// Where the file goes, inside the bundle.
pub const MODULE_INFO_PATH: &str = "Contents/Resources/moduleinfo.json";

/// The `moduleinfo.json` of the bundle at `bundle`, from its factory's own answers.
///
/// Its `Name` is the bundle's, without the extension, as the SDK's tool names it. Its `Version`,
/// which that tool is told on its command line, is the version the factory gives its first audio
/// class: a nice-plug plugin's `VERSION`.
///
/// # Safety
///
/// Loading the module runs its code, in this process.
pub unsafe fn module_info(bundle: &Path) -> Result<String, String> {
    // SAFETY: the caller's.
    let module = unsafe { Module::load(bundle) }?;
    let name = bundle
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let version = module
        .classes
        .first()
        .map(|class| class.version.as_str())
        .unwrap_or_default();
    Ok(render(&name, version, &module.about, &module.records))
}

/// The file, from what was read. The classes are left out when the factory says they may change
/// (`kClassesDiscardable`), as the SDK's tool leaves them out.
fn render(name: &str, version: &str, about: &FactoryRecord, records: &[ClassRecord]) -> String {
    let flag = |flag: i32| about.flags & flag != 0;
    let mut out = String::new();
    out.push_str("{\n");
    let _ = writeln!(out, "  \"Name\": {},", text(name));
    let _ = writeln!(out, "  \"Version\": {},", text(version));
    out.push_str("  \"Factory Info\": {\n");
    let _ = writeln!(out, "    \"Vendor\": {},", text(&about.vendor));
    let _ = writeln!(out, "    \"URL\": {},", text(&about.url));
    let _ = writeln!(out, "    \"E-Mail\": {},", text(&about.email));
    out.push_str("    \"Flags\": {\n");
    let _ = writeln!(out, "      \"Unicode\": {},", flag(kUnicode as i32));
    let _ = writeln!(
        out,
        "      \"Classes Discardable\": {},",
        flag(kClassesDiscardable as i32)
    );
    let _ = writeln!(
        out,
        "      \"Component Non Discardable\": {}",
        flag(kComponentNonDiscardable as i32)
    );
    out.push_str("    }\n  },\n");
    out.push_str("  \"Classes\": [");
    let listed: &[ClassRecord] = if flag(kClassesDiscardable as i32) {
        &[]
    } else {
        records
    };
    for (index, record) in listed.iter().enumerate() {
        out.push_str(if index == 0 { "\n" } else { ",\n" });
        out.push_str("    {\n");
        let _ = writeln!(out, "      \"CID\": {},", text(&class_id_text(&record.cid)));
        let _ = writeln!(out, "      \"Category\": {},", text(&record.category));
        let _ = writeln!(out, "      \"Name\": {},", text(&record.name));
        let _ = writeln!(out, "      \"Vendor\": {},", text(&record.vendor));
        let _ = writeln!(out, "      \"Version\": {},", text(&record.version));
        let _ = writeln!(out, "      \"SDKVersion\": {},", text(&record.sdk_version));
        let sub_categories: Vec<String> = record
            .sub_categories
            .split('|')
            .filter(|part| !part.is_empty())
            .map(text)
            .collect();
        if !sub_categories.is_empty() {
            let _ = writeln!(
                out,
                "      \"Sub Categories\": [{}],",
                sub_categories.join(", ")
            );
        }
        let _ = writeln!(out, "      \"Class Flags\": {},", record.flags);
        let _ = writeln!(out, "      \"Cardinality\": {},", record.cardinality);
        // The SDK's tool lists the screenshots in `Contents/Resources/Snapshots`; there are none.
        out.push_str("      \"Snapshots\": []\n");
        out.push_str("    }");
    }
    out.push_str(if listed.is_empty() { "]\n" } else { "\n  ]\n" });
    out.push_str("}\n");
    out
}

/// `value` as a JSON string.
fn text(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> ClassRecord {
        // A class ID whose text is `5C2F4F956A769870329BBEED54F37F2F` on every platform: the bytes
        // as the platform's SDK stores them.
        let words = [0x5C2F_4F95u32, 0x6A76_9870, 0x329B_BEED, 0x54F3_7F2F];
        let mut cid = [0 as std::ffi::c_char; 16];
        for (index, word) in words.iter().enumerate() {
            let bytes = if cfg!(target_os = "windows") && index == 0 {
                word.to_le_bytes()
            } else if cfg!(target_os = "windows") && index == 1 {
                let [a, b, c, d] = word.to_be_bytes();
                [b, a, d, c]
            } else {
                word.to_be_bytes()
            };
            for (offset, byte) in bytes.iter().enumerate() {
                cid[index * 4 + offset] = *byte as std::ffi::c_char;
            }
        }
        ClassRecord {
            cid,
            category: "Audio Module Class".into(),
            cardinality: 0x7FFF_FFFF,
            name: "mxm-mono-00".into(),
            flags: 1,
            sub_categories: "Instrument|Synth".into(),
            vendor: "mxm".into(),
            version: "0.1.0".into(),
            sdk_version: "VST 3.8.0".into(),
        }
    }

    #[test]
    fn the_file_is_the_sdk_tools_keys_in_its_order() {
        let about = FactoryRecord {
            vendor: "mxm".into(),
            url: "https://mxm.dk".into(),
            email: "info@\"mxm\".dk".into(),
            flags: kUnicode as i32,
        };
        let written = render("mxm-mono-00", "0.1.0", &about, &[record()]);
        assert_eq!(
            written,
            r#"{
  "Name": "mxm-mono-00",
  "Version": "0.1.0",
  "Factory Info": {
    "Vendor": "mxm",
    "URL": "https://mxm.dk",
    "E-Mail": "info@\"mxm\".dk",
    "Flags": {
      "Unicode": true,
      "Classes Discardable": false,
      "Component Non Discardable": false
    }
  },
  "Classes": [
    {
      "CID": "5C2F4F956A769870329BBEED54F37F2F",
      "Category": "Audio Module Class",
      "Name": "mxm-mono-00",
      "Vendor": "mxm",
      "Version": "0.1.0",
      "SDKVersion": "VST 3.8.0",
      "Sub Categories": ["Instrument", "Synth"],
      "Class Flags": 1,
      "Cardinality": 2147483647,
      "Snapshots": []
    }
  ]
}
"#
        );

        let discardable = FactoryRecord {
            flags: (kUnicode | kClassesDiscardable) as i32,
            ..about
        };
        let written = render("mxm-mono-00", "0.1.0", &discardable, &[record()]);
        assert!(written.ends_with("  \"Classes\": []\n}\n"), "{written}");
    }
}
