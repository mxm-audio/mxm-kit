//! **What a player reads on hover speaks to the player** (design system §7.6; the owner,
//! 2026-09-27: *the text MUST be user focused, not a leak from internal technical discussions*, and,
//! of mxm-mono-00's Tone, *what is a plug-out and why is it mentioned in this plugin?*).
//!
//! Every editor's sentences are string literals in its editor sources. [`speaks_to_the_player`]
//! reads one plugin's and fails on the vocabulary that leaked before: the hardware and its history,
//! the circuit, and the code's own words. A sentence says what a control does to the sound.
//! [`host_description_speaks_to_the_player`] does the same for the line a host's plugin browser
//! shows.
//!
//! **The word lists are the contract**: extend them when a new leak is found, never loosen them to
//! pass. Developer documentation — briefs, READMEs, `Cargo.toml` descriptions — may name the
//! originals; it is not what a player reads.

use std::path::{Path, PathBuf};

/// Words and phrases that tell the player about the machine, its history or the code rather than
/// the sound. Lower case; matched inside each sentence, lower-cased.
pub const LEAKS: &[&str] = &[
    "the hardware",
    "hardware's",
    "the machine",
    "machine's own",
    "plug-out",
    "plugout",
    "the original",
    "as on the",
    "circuit",
    "schematic",
    "research",
    "trimmer",
    "diode ladder",
    "bucket brigade",
    "chip",
    " cv",
    "dsp",
    "as fitted",
    "exact bypass",
    "bit-exact",
    "costs no cpu",
    "costs no processing",
    "costs nothing",
    "nonlinearity",
    "decorrelated",
    "antiphase",
    "metadata",
    "unity",
];

/// Makers' and models' names — the root *Naming* rule (mxm-kit's `docs/collection-rules.md`) keeps
/// them out of what a product calls itself — and the code's own words. **Not the technique**: *bucket-brigade*, *circuit-modelled* and
/// *diode-ladder* are what sets a plugin apart, and a browser is where a player learns it (the
/// owner, 2026-09-27: *you removed the unique selling point*).
pub const HOST_LEAKS: &[&str] = &[
    "roland",
    "juno",
    "jupiter",
    "system-100",
    "moog",
    "korg",
    "buchla",
    "oberheim",
    "sequential",
    "prophet",
    "yamaha",
    "tb-303",
    "tr-808",
    "tr-909",
    "sh-101",
    "mc-202",
    "the circuit's",
    "memoryless",
    "topology",
    "scheduler",
];

/// Panics on any sentence in the plugin's editor sources that talks about the machine or the code
/// rather than the sound, or when the scan reads no sentence at all — which means it is not finding
/// the editor. Pass `env!("CARGO_MANIFEST_DIR")`.
pub fn speaks_to_the_player(manifest_dir: &str) {
    let plugin = Path::new(manifest_dir);
    let mut leaks = Vec::new();
    let mut read = 0;
    for path in editor_sources(plugin) {
        let text = std::fs::read_to_string(&path).expect("a source file");
        // Tests say what they like to a developer.
        let text = match text
            .find("#[cfg(test)]\nmod ")
            .or(text.find("#[cfg(test)]\r\nmod "))
        {
            Some(end) => &text[..end],
            None => &text[..],
        };
        for (line, sentence) in literals(text) {
            if !is_sentence(&sentence) {
                continue;
            }
            read += 1;
            let lower = sentence.to_lowercase();
            for leak in LEAKS.iter().filter(|leak| lower.contains(*leak)) {
                leaks.push(format!(
                    "{}:{line}: \"{leak}\" in {sentence:?}",
                    path.strip_prefix(plugin).unwrap_or(&path).display()
                ));
            }
        }
    }
    assert!(
        read > 0,
        "read no sentences in {}: the scan is not finding the editor",
        plugin.display()
    );
    assert!(
        leaks.is_empty(),
        "hover text that talks about the machine or the code, not the sound:\n{}",
        leaks.join("\n")
    );
}

/// **The line a host shows in its plugin browser speaks to the player** (the owner, 2026-09-27):
/// what the plugin is and what sets it apart — its technique included — never a maker's or model's
/// name, or the code's own words. Reads the `CLAP_DESCRIPTION` in the plugin's `src/lib.rs`. Pass
/// `env!("CARGO_MANIFEST_DIR")`.
pub fn host_description_speaks_to_the_player(manifest_dir: &str) {
    let lib = Path::new(manifest_dir).join("src/lib.rs");
    let text = std::fs::read_to_string(&lib).unwrap_or_else(|e| panic!("{}: {e}", lib.display()));
    let at = text
        .find("const CLAP_DESCRIPTION")
        .unwrap_or_else(|| panic!("{} has no CLAP_DESCRIPTION", lib.display()));
    let (_, description) = literals(&text[at..])
        .into_iter()
        .next()
        .expect("the description's literal");
    let lower = description.to_lowercase();
    let leaks: Vec<_> = HOST_LEAKS
        .iter()
        .filter(|leak| lower.contains(*leak))
        .collect();
    assert!(
        leaks.is_empty(),
        "the host description talks about the machine or the code ({leaks:?}): {description:?}"
    );
}

/// A plugin's editor sources: the files that draw what the player sees.
fn editor_sources(plugin: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![plugin.join("src")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = path.to_string_lossy().replace('\\', "/");
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if text.ends_with(".rs")
                && (text.contains("/editor")
                    || matches!(&*name, "sections.rs" | "canvas.rs" | "visuals.rs"))
            {
                out.push(path);
            }
        }
    }
    out
}

/// The string literals of `text` outside comments, decoded, with the line each starts on.
fn literals(text: &str) -> Vec<(usize, String)> {
    let bytes: Vec<char> = text.chars().collect();
    let (mut out, mut i, mut line) = (Vec::new(), 0, 1);
    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        if c == '\n' {
            line += 1;
        } else if c == '/' && next == Some('/') {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '\'' && (bytes.get(i + 2) == Some(&'\'') || next == Some('\\')) {
            i += if next == Some('\\') { 4 } else { 3 };
            while i < bytes.len() && bytes[i - 1] != '\'' {
                i += 1;
            }
            continue;
        } else if c == '"' {
            let (start, mut s) = (line, String::new());
            i += 1;
            while i < bytes.len() && bytes[i] != '"' {
                if bytes[i] == '\\' {
                    match bytes.get(i + 1) {
                        Some('\r' | '\n') => {
                            i += 1;
                            while i < bytes.len() && bytes[i].is_whitespace() {
                                if bytes[i] == '\n' {
                                    line += 1;
                                }
                                i += 1;
                            }
                            continue;
                        }
                        Some(escaped) => s.push(*escaped),
                        None => {}
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == '\n' {
                    line += 1;
                }
                s.push(bytes[i]);
                i += 1;
            }
            out.push((start, s));
        }
        i += 1;
    }
    out
}

/// A literal that reads as a sentence a player sees, rather than an id, a format or a message to a
/// developer.
fn is_sentence(s: &str) -> bool {
    let s = s.trim();
    s.len() >= 18
        && s.contains(' ')
        && !s.contains('{')
        && s.starts_with(|c: char| c.is_uppercase())
        && s.ends_with(['.', ')', '?', '!'])
}
