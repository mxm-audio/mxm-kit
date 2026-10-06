//! Builds plugins from other MXM repositories, at pinned tags, for this repository's tests.
//!
//! MXM Player's tests host mxm-mono-01 as their reference instrument and mxm-bucket-delay as their
//! reference effect; an effect's chain tests feed mxm-mono-01 into it. Since every product lives in
//! its own repository, a test that loads another product's plugin needs that plugin built first.
//! `cargo xtask fetch` does it from `test-bundles.txt` at the workspace root:
//!
//! ```text
//! # repository  tag     profile
//! mxm-mono-01   v0.1.0  release   # -> target/bundled/mxm-mono-01.clap (+ its control map)
//! mxm-mono-01   v0.1.0  debug     # -> target/debug/<library>, nice-plug's allocation guard on
//! ```
//!
//! Each repository is cloned shallowly at its tag into `target/fetched/<repository>-<tag>` and built
//! there with its own `cargo xtask bundle` (release) or `cargo build` (debug), so it builds exactly
//! as its own repository does. A clone already present is reused: a tag does not move.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The organisation every MXM repository lives under.
pub const ORG: &str = "https://github.com/mxm-audio";

/// The file a repository lists the plugins its tests need in.
pub const LIST: &str = "test-bundles.txt";

/// One line of [`LIST`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// The repository, which is also the plugin's package and bundle name.
    pub repo: String,
    pub tag: String,
    pub release: bool,
}

/// Reads [`LIST`]: one `repository tag profile` per line, `#` starting a comment.
pub fn parse(text: &str) -> Result<Vec<Wanted>> {
    let mut out = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [repo, tag, profile] = fields[..] else {
            bail!("{LIST}:{}: expected `repository tag profile`", number + 1);
        };
        let release = match profile {
            "release" => true,
            "debug" => false,
            other => bail!(
                "{LIST}:{}: profile `{other}` is neither release nor debug",
                number + 1
            ),
        };
        out.push(Wanted {
            repo: repo.to_owned(),
            tag: tag.to_owned(),
            release,
        });
    }
    Ok(out)
}

/// Builds every plugin [`LIST`] names into `workspace_root`'s `target/`. Returns what it placed.
pub fn fetch(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    let list = workspace_root.join(LIST);
    let text = std::fs::read_to_string(&list)
        .with_context(|| format!("Could not read {}", list.display()))?;
    let target = workspace_root.join("target");
    let mut placed = Vec::new();
    for wanted in parse(&text)? {
        let checkout = target
            .join("fetched")
            .join(format!("{}-{}", wanted.repo, wanted.tag));
        if !checkout.join("Cargo.toml").exists() {
            run(Command::new("git")
                .args(["clone", "--quiet", "--depth", "1", "--branch", &wanted.tag])
                .arg(format!("{ORG}/{}", wanted.repo))
                .arg(&checkout))?;
        }
        if wanted.release {
            run(Command::new(cargo()).current_dir(&checkout).args([
                "xtask",
                "bundle",
                &wanted.repo,
                "--release",
            ]))?;
            let bundled = target.join("bundled");
            std::fs::create_dir_all(&bundled)?;
            for name in [
                format!("{}.clap", wanted.repo),
                format!("{}.control-map.json", wanted.repo),
            ] {
                let from = checkout.join("target").join("bundled").join(&name);
                if from.exists() {
                    copy(&from, &bundled.join(&name))?;
                    placed.push(bundled.join(&name));
                }
            }
        } else {
            run(Command::new(cargo())
                .current_dir(&checkout)
                .args(["build", "-p", &wanted.repo]))?;
            let library = format!(
                "{}{}{}",
                std::env::consts::DLL_PREFIX,
                wanted.repo.replace('-', "_"),
                std::env::consts::DLL_SUFFIX
            );
            let debug = target.join("debug");
            std::fs::create_dir_all(&debug)?;
            copy(
                &checkout.join("target").join("debug").join(&library),
                &debug.join(&library),
            )?;
            placed.push(debug.join(&library));
        }
    }
    Ok(placed)
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("Could not run {command:?}"))?;
    if !status.success() {
        bail!("{command:?} failed");
    }
    Ok(())
}

/// Copies a file, or a directory (a macOS `.clap` is a bundle directory), replacing what is there.
fn copy(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        if to.exists() {
            std::fs::remove_dir_all(to)?;
        }
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(from, to)
            .with_context(|| format!("Could not copy {} to {}", from.display(), to.display()))?;
    }
    Ok(())
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_reads_repository_tag_and_profile() {
        let wanted =
            parse("# comment\nmxm-mono-01 v0.1.0 release\n\nmxm-para-07 v0.1.0 debug # why\n")
                .unwrap();
        assert_eq!(
            wanted,
            [
                Wanted {
                    repo: "mxm-mono-01".into(),
                    tag: "v0.1.0".into(),
                    release: true
                },
                Wanted {
                    repo: "mxm-para-07".into(),
                    tag: "v0.1.0".into(),
                    release: false
                },
            ]
        );
    }

    #[test]
    fn a_malformed_line_is_refused_with_its_number() {
        let error = parse("mxm-mono-01 v0.1.0\n").unwrap_err().to_string();
        assert!(error.contains(":1:"), "{error}");
        let error = parse("\nmxm-mono-01 v0.1.0 fast\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains(":2:") && error.contains("fast"), "{error}");
    }
}
