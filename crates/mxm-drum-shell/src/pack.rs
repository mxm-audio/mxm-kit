//! Writing a captured kit out as a sample pack: the drum machine's (its plan §4.7, D9), shared since
//! 2026-10-26.
//!
//! Sixteen 24-bit WAV one-shots and a manifest, in a flat folder a tracker or a hardware sampler
//! can import without being told anything. This is the half of Resample that leaves the plugin,
//! and it lives on the plugin side, as no DSP crate touches a file.
//!
//! # What lands in a file
//!
//! A frozen slot is heard as its capture, then its playback controls (Tune and Decay), then
//! Level, Mute, Solo, Pan, Master and their routes. **The pack takes that chain up to and including
//! the playback controls, and stops there.** Those are the one-shot somebody shaped and
//! auditioned; everything after them is monitoring balance belonging to whatever plays the pack
//! next. So every file is at unity gain and centred, a muted slot still exports, and moving a
//! fader cannot change an exported byte. Each plugin renders its one-shots through the reader its
//! frozen kit plays ([`OneShots`]), so a file is the sound rather than a second implementation of
//! it.
//!
//! # Never overwriting, on three platforms
//!
//! Sixteen files and a manifest can fail halfway, which per-file atomicity does not prevent. The
//! pack is written into a uniquely named sibling directory and committed by **one directory
//! rename onto a path that does not exist** — a name already taken simply takes the next one. So
//! there is nothing to roll back, and the rename means the same thing on Windows, Linux and
//! macOS, which a rename over an existing directory would not.

use std::path::{Path, PathBuf};

use mxm_audio_file::{Bits, Target};

use crate::SLOT_COUNT;

/// What a pack is written from: a captured kit, read through the plugin's frozen reader.
pub trait OneShots {
    /// The rate the one-shots are at: the capture's.
    fn sample_rate(&self) -> f32;
    /// Whether `slot` captured nothing (its model Off or unavailable): it writes no file.
    fn is_empty(&self, slot: usize) -> bool;
    /// `slot`'s one-shot: its capture through the frozen reader at full strength, the playback
    /// controls applied, stopping if `cancelled` says to ([`render_one_shot`] is the loop).
    ///
    /// # Errors
    ///
    /// [`PackError::Cancelled`] when it stopped, [`PackError::Allocation`] when its buffer cannot
    /// be had.
    fn one_shot_until(
        &self,
        slot: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, PackError>;
    /// What `slot`'s model is called in a file's name and the manifest: its label.
    fn label(&self, slot: usize) -> &str;
    /// The Kit note `slot` plays, if it has one.
    fn note(&self, slot: usize) -> Option<u8>;
    /// The plugin that wrote the pack, for the manifest (`mxm-drum-machine`).
    fn product(&self) -> &str;
    /// What a recording of this instrument loses, the manifest's last paragraph, after "These are
    /// recordings of the instrument, not the instrument."
    fn losses(&self) -> &str;
}

/// The folder packs are written into, under `data_local`.
///
/// Collection content, so the platform's local data directory rather than the roaming config
/// one — the same rule that puts `mxm/impulses` there, and for the same reason: a pack is audio
/// and audio should not follow a Windows profile around.
#[must_use]
pub fn root_under(data_local: Option<PathBuf>) -> Option<PathBuf> {
    data_local.map(|dir| dir.join("mxm").join("sample-packs"))
}

/// This user's sample-pack folder; `None` where the platform has no local data directory.
///
/// The root is resolved once and injected, exactly as the preset library's and the impulses
/// folder's are, so a test never writes into the folder of whoever ran it.
#[must_use]
pub fn root() -> Option<PathBuf> {
    root_under(dirs::data_local_dir())
}

/// Why a pack could not be written.
#[derive(Debug)]
pub enum PackError {
    /// The platform has no local data directory, so there is nowhere to put a pack.
    NoDestination,
    /// Every slot was `Off`, so there is nothing to export.
    Empty,
    /// Encoding refused the audio, which `mxm-audio-file` does for a non-finite sample.
    Encode(String),
    /// The file system refused.
    Io(std::io::Error),
    /// The plugin is closing, so the export stopped, leaving nothing behind.
    Cancelled,
    /// A one-shot's buffer could not be had: a slot read very slowly is long.
    Allocation { slot: usize, frames: usize },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDestination => write!(f, "this platform has no local data folder to write to"),
            Self::Empty => write!(f, "every slot is Off, so there is nothing to export"),
            Self::Encode(why) => write!(f, "the audio could not be encoded: {why}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Cancelled => write!(f, "the export stopped, as the plugin closed"),
            Self::Allocation { slot, frames } => write!(
                f,
                "slot {}'s one-shot needs {frames} samples, more memory than there is",
                slot + 1
            ),
        }
    }
}

impl From<std::io::Error> for PackError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// What a finished pack contains, for the editor to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// Where the pack landed.
    pub directory: PathBuf,
    /// How many one-shots it holds.
    pub files: usize,
    /// How many samples clipped, summed over the pack.
    ///
    /// Reported rather than prevented: `mxm-audio-file` counts them and a silently clipped pack
    /// is worse than one that says so. Nothing is normalised — a drum's brief forbids a hidden
    /// limiter in the audio path, and a file is no different.
    pub clipped: usize,
}

/// How often a one-shot's render asks whether to stop: read slowly, one reads its capture out up
/// to sixteen times as long.
pub const CANCEL_CHECK_FRAMES: usize = 4_096;

/// The loop every one-shot is rendered by: up to `bound` frames from `frame` (a frozen reader's
/// next sample, `None` once its hit has ended), its buffer reserved first and fallibly, as an
/// infallible allocation that fails aborts the host, and `cancelled` asked every
/// [`CANCEL_CHECK_FRAMES`].
///
/// # Errors
///
/// [`PackError::Allocation`] for `slot` when the buffer cannot be had, [`PackError::Cancelled`]
/// when it stopped.
pub fn render_one_shot(
    slot: usize,
    bound: usize,
    cancelled: &dyn Fn() -> bool,
    mut frame: impl FnMut() -> Option<f32>,
) -> Result<Vec<f32>, PackError> {
    let mut out = Vec::new();
    out.try_reserve_exact(bound)
        .map_err(|_| PackError::Allocation {
            slot,
            frames: bound,
        })?;
    while out.len() < bound {
        if out.len() % CANCEL_CHECK_FRAMES == 0 && cancelled() {
            return Err(PackError::Cancelled);
        }
        let Some(sample) = frame() else {
            break;
        };
        out.push(sample);
    }
    Ok(out)
}

/// A file-system-safe name for a model's public label, or a kit's name.
#[must_use]
pub fn slug(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut last_dash = true;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    // Windows reserves these for its devices, in any case, so no folder or file may be called one;
    // a pack is named the same on all three platforms, so the name moves aside everywhere.
    let device = matches!(out.as_str(), "con" | "prn" | "aux" | "nul")
        || (out.len() == 4
            && (out.starts_with("com") || out.starts_with("lpt"))
            && out.as_bytes()[3].is_ascii_digit());
    if device {
        out.push_str("-kit");
    }
    if out.is_empty() {
        "slot".to_owned()
    } else {
        out
    }
}

/// The manifest that ships beside the audio.
///
/// Plain text, because the thing reading it next is a person deciding which file is the kick.
#[must_use]
pub fn manifest(kit: &str, shots: &impl OneShots, names: &[Option<String>; SLOT_COUNT]) -> String {
    let mut out = String::new();
    out.push_str(&format!("{kit}\n"));
    out.push_str(&format!("Exported from {}.\n\n", shots.product()));
    out.push_str(&format!(
        "Sample rate: {} Hz, 24-bit WAV\n\n",
        shots.sample_rate() as u32
    ));
    for (slot, name) in names.iter().enumerate() {
        let Some(name) = name.as_ref() else {
            continue;
        };
        let label = shots.label(slot);
        // Every slot in the Kit range has a note; the fallback is only so a future slot without
        // one cannot break the manifest.
        let note = shots
            .note(slot)
            .map_or_else(|| "-".to_owned(), |note| note.to_string());
        out.push_str(&format!(
            "{name}  slot {:02}  note {note}  {label}\n",
            slot + 1
        ));
    }
    out.push_str("\nThese are recordings of the instrument, not the instrument. ");
    out.push_str(shots.losses());
    out
}

/// Writes a pack, and returns where it landed.
///
/// **Never call this from an audio thread**: it renders, allocates and touches the file system.
///
/// # Errors
///
/// [`PackError`] when there is no destination, nothing to export, the audio cannot be encoded, or
/// the file system refuses. A failure leaves no partial pack at the destination, because the
/// staging directory is only renamed into place once every file is written.
pub fn write(root: &Path, kit: &str, shots: &impl OneShots) -> Result<Written, PackError> {
    write_until(root, kit, shots, &|| false)
}

/// [`write`], stopping between one file and the next if `cancelled` says to, the staging removed:
/// the form for the capture thread, which the plugin's teardown joins, so a pack to slow or
/// removable storage cannot hold a closing host for its whole length.
///
/// # Errors
///
/// As [`write`], plus [`PackError::Cancelled`].
pub fn write_until(
    root: &Path,
    kit: &str,
    shots: &impl OneShots,
    cancelled: &dyn Fn() -> bool,
) -> Result<Written, PackError> {
    let rate = shots.sample_rate() as u32;
    if (0..SLOT_COUNT).all(|slot| shots.is_empty(slot)) {
        return Err(PackError::Empty);
    }

    let staging = claim_staging(root, kit)?;

    // **One slot at a time**: rendered, encoded and written before the next is rendered, as a
    // one-shot read slowly can be long, and sixteen held at once hundreds of megabytes.
    let mut clipped = 0;
    let mut names: [Option<String>; SLOT_COUNT] = std::array::from_fn(|_| None);
    let written = (|| -> Result<usize, PackError> {
        let mut files = 0;
        for (slot, entry) in names.iter_mut().enumerate() {
            let samples = shots.one_shot_until(slot, cancelled)?;
            if samples.is_empty() {
                continue;
            }
            let (bytes, encoded) =
                mxm_audio_file::encode(&samples, 1, rate, Target::Wav(Bits::TwentyFour))
                    .map_err(|error| PackError::Encode(format!("{error:?}")))?;
            drop(samples);
            let name = format!("{:02}-{}.wav", slot + 1, slug(shots.label(slot)));
            write_file_until(&staging.join(&name), &bytes, cancelled)?;
            clipped += encoded.clipped;
            *entry = Some(name);
            files += 1;
        }
        let text = manifest(kit, shots, &names);
        std::fs::write(staging.join("kit.txt"), text)?;
        Ok(files)
    })();

    let files = match written {
        Ok(files) => files,
        Err(error) => {
            // Nothing reached the destination, so there is nothing to roll back — only the
            // staging directory to clean up.
            let _ = std::fs::remove_dir_all(&staging);
            return Err(error);
        }
    };

    if cancelled() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(PackError::Cancelled);
    }
    match commit(root, kit, &staging) {
        Ok(directory) => Ok(Written {
            directory,
            files,
            clipped,
        }),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

/// How much of a file is written between two looks at whether to stop.
const WRITE_CHUNK_BYTES: usize = 1 << 20;

/// Writes `bytes` as the file at `path` a mebibyte at a time, stopping if `cancelled` says to: a
/// pack's destination is often removable media, slow to write, and the plugin's teardown waits on
/// the capture thread. The file is in the staging directory, which a failure removes whole.
fn write_file_until(
    path: &Path,
    bytes: &[u8],
    cancelled: &dyn Fn() -> bool,
) -> Result<(), PackError> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    for chunk in bytes.chunks(WRITE_CHUNK_BYTES) {
        if cancelled() {
            return Err(PackError::Cancelled);
        }
        file.write_all(chunk)?;
    }
    Ok(())
}

/// Makes this write's own staging directory beside the packs, named so no other write's can be: its
/// process, its thread and a count, claimed by `create_dir`, which refuses one that exists. Two
/// instances exporting the same kit at once each write their own.
fn claim_staging(root: &Path, kit: &str) -> Result<PathBuf, PackError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static WRITES: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(root)?;
    let base = slug(kit);
    for _ in 0..1_000 {
        let staging = root.join(format!(
            ".{base}.{}-{:?}-{}.incomplete",
            std::process::id(),
            std::thread::current().id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&staging) {
            Ok(()) => return Ok(staging),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(PackError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no staging directory could be claimed",
    )))
}

/// Renames a finished staging directory to the first free pack name: the kit's, then `-1`, `-2`, …
/// **An existing pack is never overwritten**, and the name is taken by the rename itself, which
/// refuses a directory that exists (Windows) or holds anything (Linux, macOS), so a pack another
/// write committed first only moves this one to the next name. One rename within one file system,
/// so a destination holds a whole pack or nothing, on all three.
fn commit(root: &Path, kit: &str, staging: &Path) -> Result<PathBuf, PackError> {
    let base = slug(kit);
    for attempt in 0..1_000 {
        let name = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        let directory = root.join(&name);
        if directory.exists() {
            continue;
        }
        match std::fs::rename(staging, &directory) {
            Ok(()) => return Ok(directory),
            // Taken between the look and the rename: the next name.
            Err(_) if directory.exists() => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(PackError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "a thousand packs already share this name",
    )))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A folder for one test's packs, under the build's own folder (never the real pack folder, and
    /// never on C:), empty.
    pub(crate) fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-packs")
            .join(format!(
                "drum-shell-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch root");
        dir
    }

    /// A kit as a pack reads one: a ramp a slot, a kick in slot 1 and a hat in slot 2, the rest
    /// Off, rendered through [`render_one_shot`] as a plugin's frozen reader is.
    pub(crate) struct Shots {
        pub lengths: [usize; SLOT_COUNT],
    }

    impl Shots {
        pub(crate) fn two() -> Self {
            let mut lengths = [0; SLOT_COUNT];
            lengths[0] = 4_800;
            lengths[1] = 2_400;
            Self { lengths }
        }
    }

    impl OneShots for Shots {
        fn sample_rate(&self) -> f32 {
            48_000.0
        }
        fn is_empty(&self, slot: usize) -> bool {
            self.lengths[slot] == 0
        }
        fn one_shot_until(
            &self,
            slot: usize,
            cancelled: &dyn Fn() -> bool,
        ) -> Result<Vec<f32>, PackError> {
            let length = self.lengths[slot];
            let mut at = 0;
            render_one_shot(slot, length, cancelled, || {
                at += 1;
                (at <= length).then(|| 0.5 * (1.0 - at as f32 / length as f32))
            })
        }
        fn label(&self, slot: usize) -> &str {
            match slot {
                0 => "Test Kick",
                1 => "Test Hi-hat closed",
                _ => "Off",
            }
        }
        fn note(&self, slot: usize) -> Option<u8> {
            Some(36 + slot as u8)
        }
        fn product(&self) -> &str {
            "mxm-test-drums"
        }
        fn losses(&self) -> &str {
            "Each one is a single hit.\n"
        }
    }

    fn files_in(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(directory)
            .expect("the pack")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_pack_holds_one_file_per_sounding_slot_and_a_manifest() {
        let root = scratch("files");
        let written = write(&root, "Test Kit", &Shots::two()).expect("a pack");
        assert_eq!(written.files, 2, "two loaded slots, two files");
        assert_eq!(
            files_in(&written.directory),
            vec![
                "01-test-kick.wav".to_owned(),
                "02-test-hi-hat-closed.wav".to_owned(),
                "kit.txt".to_owned(),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_existing_pack_is_never_overwritten_and_nothing_partial_is_left() {
        let root = scratch("twice");
        let shots = Shots::two();
        let first = write(&root, "Test Kit", &shots).expect("a pack");
        let second = write(&root, "Test Kit", &shots).expect("a second pack");
        assert_ne!(first.directory, second.directory, "the name was taken");
        assert!(
            first.directory.join("kit.txt").exists(),
            "and the first is untouched"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&root)
            .expect("the root")
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().to_string_lossy().into_owned();
                name.contains("incomplete").then_some(name)
            })
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging was cleaned up: {leftovers:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two writes of one kit at once each claim their own staging and commit a whole pack of their
    /// own: neither mixes into the other, and nothing is left staged (model drums' review,
    /// 2026-10-26).
    #[test]
    fn two_exports_at_once_write_two_whole_packs() {
        let root = scratch("together");
        let shots = Shots::two();
        let written: Vec<Written> = std::thread::scope(|scope| {
            let writers: Vec<_> = (0..2)
                .map(|_| {
                    let (root, shots) = (&root, &shots);
                    scope.spawn(move || write(root, "Same Kit", shots).expect("a pack"))
                })
                .collect();
            writers
                .into_iter()
                .map(|w| w.join().expect("joins"))
                .collect()
        });
        assert_ne!(written[0].directory, written[1].directory);
        for pack in &written {
            let files = std::fs::read_dir(&pack.directory)
                .expect("the pack")
                .count();
            assert_eq!(
                files, 3,
                "two one-shots and the manifest in {:?}",
                pack.directory
            );
        }
        let staged = std::fs::read_dir(&root)
            .expect("the root")
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().contains("incomplete"))
            })
            .count();
        assert_eq!(staged, 0, "nothing left staged");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A one-shot's render asked to stop, stops; and one that cannot have its buffer says so
    /// rather than aborting.
    #[test]
    fn a_one_shot_stops_when_asked_and_refuses_a_buffer_it_cannot_have() {
        let shots = Shots::two();
        assert!(matches!(
            shots.one_shot_until(0, &|| true),
            Err(PackError::Cancelled)
        ));
        assert_eq!(
            shots.one_shot_until(0, &|| false).expect("a shot").len(),
            4_800
        );
        assert!(matches!(
            render_one_shot(3, usize::MAX, &|| false, || Some(0.0)),
            Err(PackError::Allocation { slot: 3, .. })
        ));
    }

    /// A file is written a mebibyte at a time, the cancel asked before each: asked to stop after
    /// the first, it stops there, one mebibyte on the disk.
    #[test]
    fn a_file_stops_between_its_mebibytes() {
        let root = scratch("chunks");
        let path = root.join("three.bin");
        let bytes = vec![7_u8; 3 * WRITE_CHUNK_BYTES];
        let asked = std::cell::Cell::new(0);
        let cancelled = || {
            asked.set(asked.get() + 1);
            asked.get() > 1
        };
        assert!(matches!(
            write_file_until(&path, &bytes, &cancelled),
            Err(PackError::Cancelled)
        ));
        assert_eq!(
            asked.get(),
            2,
            "asked before the first mebibyte and the second"
        );
        let written = std::fs::metadata(&path).expect("the file").len();
        assert_eq!(
            written, WRITE_CHUNK_BYTES as u64,
            "one mebibyte, then it stopped"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A write the teardown cancels part-way, one file staged, leaves nothing: no pack, nothing
    /// staged.
    #[test]
    fn a_cancelled_write_leaves_nothing_behind() {
        let root = scratch("cancelled");
        // Cancelled once a file is staged: as its bytes are being written, part-way through.
        let staged = || {
            std::fs::read_dir(&root)
                .into_iter()
                .flatten()
                .flatten()
                .any(|entry| {
                    std::fs::read_dir(entry.path()).is_ok_and(|mut files| files.next().is_some())
                })
        };
        let cancelled = || staged();
        assert!(matches!(
            write_until(&root, "Kit", &Shots::two(), &cancelled),
            Err(PackError::Cancelled)
        ));
        assert_eq!(
            std::fs::read_dir(&root).expect("the root").count(),
            0,
            "nothing left"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_kit_is_refused_rather_than_written() {
        let root = scratch("empty");
        let shots = Shots {
            lengths: [0; SLOT_COUNT],
        };
        assert!(matches!(
            write(&root, "Empty", &shots),
            Err(PackError::Empty)
        ));
        assert_eq!(
            std::fs::read_dir(&root).expect("the root").count(),
            0,
            "nothing staged for nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_manifest_names_every_file_and_says_what_a_pack_is() {
        let mut names: [Option<String>; SLOT_COUNT] = std::array::from_fn(|_| None);
        names[0] = Some("01-test-kick.wav".to_owned());
        let text = manifest("Test Kit", &Shots::two(), &names);
        assert_eq!(
            text,
            "Test Kit\nExported from mxm-test-drums.\n\nSample rate: 48000 Hz, 24-bit WAV\n\n\
             01-test-kick.wav  slot 01  note 36  Test Kick\n\
             \nThese are recordings of the instrument, not the instrument. Each one is a single \
             hit.\n"
        );
    }

    #[test]
    fn a_slug_is_file_system_safe() {
        assert_eq!(slug("Big Red Hi-hat closed"), "big-red-hi-hat-closed");
        assert_eq!(slug("  ...  "), "slot");
        // Windows' device names, in any case, move aside; a name that only starts with one does not.
        assert_eq!(slug("CON"), "con-kit");
        assert_eq!(slug("Aux"), "aux-kit");
        assert_eq!(slug("lpt9"), "lpt9-kit");
        assert_eq!(slug("Console"), "console");
    }

    #[test]
    fn the_folder_is_mxm_sample_packs_under_the_local_data_directory() {
        let base = PathBuf::from("data-local");
        assert_eq!(
            root_under(Some(base.clone())),
            Some(base.join("mxm").join("sample-packs"))
        );
        assert_eq!(root_under(None), None);
    }
}
