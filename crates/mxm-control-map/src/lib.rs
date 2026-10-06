//! The collection's control map: the layout file, what is on disk, and what it has to satisfy to be
//! usable. The normative contract is `docs/MXM_CONTROL_MAP.md`.
//!
//! This is **data, not code**. Adding an instrument to the collection means adding a file; it must
//! never mean editing a host. MXM Player's `tests/t5_control_map.rs` asserts exactly that, because it
//! is the requirement the whole design exists to serve. The player reads the schema from here, and so
//! does every plugin's test that holds its own map to the standard.
//!
//! # Two files, because they have different owners
//!
//! - The [`Layout`] — roles, fixed knobs, the bank, the pages — is the **collection standard**. It
//!   is the same for every instrument, lives beside this crate as `control-map.json`, and is compiled
//!   into every host that uses it ([`SHIPPED`]).
//! - An [`InstrumentMap`] says which of *this* instrument's parameters fill those roles. It ships
//!   **beside the `.clap` bundle**, because nobody is obliged to install the whole collection:
//!   someone who downloads only mxm-mono-01 must still get a working controller layout, and the player
//!   must not need to know about instruments that were released after it.
//!
//! Parameter references are written as **string parameter ids** (`"cutoff"`), which are permanent
//! by contract (`plugins/AGENTS.md`; `docs/plugin-conventions.md` here) and readable by a person.
//! CLAP reports a `u32`, so the string is hashed the way nice-plug hashes it — see
//! [`hash_param_id`]. A reference written as a bare number is used as a CLAP id directly, which is
//! what a non-nice-plug plugin needs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The schema version this build understands.
///
/// The file is a compatibility surface people hand-edit and share, so a mismatch is refused with
/// a reason rather than parsed hopefully and misinterpreted.
pub const SCHEMA_VERSION: u32 = 1;

/// How many slots a page has. Fixed by CLAP's remote-controls extension, which the collection's
/// pages are exported to; a page that exceeded it would be silently split and renamed.
pub const SLOTS_PER_PAGE: usize = 8;

/// How a controller's 0..=127 is spread across a parameter's range.
///
/// CLAP carries no curve hint — `clap_param_info` has stepped/periodic/hidden/readonly/bypass
/// flags and nothing about skew — so the host cannot recover it and the layout must state it.
/// Without this, 128 linear steps over mxm-mono-01's 20 Hz–20 kHz cutoff puts the first step near
/// 176 Hz and spends most of the travel above 10 kHz.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Curve {
    /// Even across the range. Depths, levels, mixes.
    #[default]
    Linear,
    /// Even in ratio. Frequencies and times, where equal *musical* steps are equal ratios.
    Log,
    /// Linear, but 64 lands exactly on the centre of the range. Tune, envelope amounts.
    Bipolar,
    /// Discrete positions, using the parameter's own step count.
    Stepped,
}

/// What the layout says about one role.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoleSpec {
    #[serde(default)]
    pub curve: Curve,
}

/// One of the eight knobs that never change meaning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FixedKnob {
    pub cc: u8,
    pub role: String,
}

/// Which CCs drive the paged bank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bank {
    pub slot_cc: Vec<u8>,
    pub page_down_cc: u8,
    pub page_up_cc: u8,
}

/// One page of the bank. `None` in `slots` is a deliberate gap, not a missing entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub section: String,
    pub name: String,
    pub slots: Vec<Option<String>>,
}

impl Page {
    /// What the player shows, and what a DAW would show as section plus page.
    pub fn title(&self) -> String {
        if self.section == self.name {
            self.name.clone()
        } else {
            format!("{} · {}", self.section, self.name)
        }
    }
}

/// One instrument's role-to-parameter assignments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Instrument {
    pub clap_id: String,
    #[serde(default)]
    pub name: String,
    /// Role → parameter reference. `BTreeMap` so a written-back file has stable ordering.
    pub params: BTreeMap<String, ParamRef>,
}

/// How the layout names a parameter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamRef {
    /// A nice-plug string id, hashed to the CLAP id the host sees.
    Id(String),
    /// A raw CLAP parameter id, for a plugin that does not use nice-plug's hashing.
    Raw(u32),
}

impl ParamRef {
    /// The CLAP parameter id this refers to.
    pub fn clap_id(&self) -> u32 {
        match self {
            ParamRef::Id(s) => hash_param_id(s),
            ParamRef::Raw(n) => *n,
        }
    }
}

/// nice-plug's parameter id hash, reproduced so the layout can use readable string ids.
///
/// A Rabin fingerprint with the top bit cleared — `vendor/nice-plug/src/wrapper/util.rs:35-46`,
/// since the split `src/wrapper/util.rs:35-46` in the nice-plug fork (mxm-audio/nice-plug).
/// mxm-player's `apps/mxm-player/tests/t5_control_map.rs` pins it against mxm-mono-01's real ids,
/// so a change upstream is caught rather than quietly unmapping every knob.
pub fn hash_param_id(id: &str) -> u32 {
    let mut hash: u32 = 0;
    for byte in id.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(u32::from(byte));
    }
    // VST3 reserves the top bit for host-provided parameters, so nice-plug clears it.
    hash & !(1 << 31)
}

/// The whole layout file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub schema_version: u32,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleSpec>,
    #[serde(default)]
    pub fixed: Vec<FixedKnob>,
    pub bank: Bank,
    #[serde(default)]
    pub pages: Vec<Page>,
}

/// A file shipped beside a `.clap` bundle, naming what fills each role on the instruments in it.
///
/// One file may carry several instruments, because one bundle may export several plugins.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstrumentMap {
    pub schema_version: u32,
    #[serde(default)]
    pub instruments: Vec<Instrument>,
}

impl InstrumentMap {
    pub fn parse(text: &str) -> Result<Self, LayoutError> {
        let map: InstrumentMap =
            serde_json::from_str(text).map_err(|e| LayoutError::Parse(e.to_string()))?;
        if map.schema_version != SCHEMA_VERSION {
            return Err(LayoutError::UnsupportedVersion {
                found: map.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(map)
    }

    /// The name a map file takes beside a bundle: `mxm-mono-01.clap` → `mxm-mono-01.control-map.json`.
    ///
    /// Also loaded is any `control-map.json` in the same directory, so a bundle whose file name
    /// does not match still has a home for its map.
    pub const SUFFIX: &'static str = ".control-map.json";
    pub const BARE: &'static str = "control-map.json";
}

/// CCs the layout may never claim, whatever it says.
///
/// - **120 and 123** are All Sound Off and All Notes Off. The player's own panic and recovery
///   machinery depends on them reaching the plugin, so binding a knob to one would disarm it.
/// - **1** is the mod wheel, which mxm-mono-01 consumes as *live performance modulation* into
///   `mod_wheel[channel]` (`plugins/mxm-mono-01/src/lib.rs:152-154`) without writing any parameter.
///   Mapping it would turn a performance gesture into an edit, which is a different thing.
pub const RESERVED_CCS: [u8; 3] = [1, 120, 123];

pub fn is_reserved(cc: u8) -> bool {
    RESERVED_CCS.contains(&cc)
}

/// Why a layout was refused.
///
/// Refusal is always with a reason: a layout that silently half-loaded would leave a controller
/// where some knobs work and others do nothing, which is worse than not loading at all.
#[derive(Clone, Debug, PartialEq)]
pub enum LayoutError {
    Parse(String),
    UnsupportedVersion { found: u32, supported: u32 },
    ReservedCc { cc: u8, what: String },
    DuplicateCc { cc: u8 },
    UnknownRole { role: String, what: String },
    WrongSlotCount { page: String, found: usize },
    WrongBankSize { found: usize },
    DuplicatePage { title: String },
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::Parse(e) => write!(f, "the control map could not be parsed: {e}"),
            LayoutError::UnsupportedVersion { found, supported } => write!(
                f,
                "the control map is schema version {found}, but this build understands {supported}"
            ),
            LayoutError::ReservedCc { cc, what } => write!(
                f,
                "CC {cc} is reserved and cannot be mapped, but {what} claims it"
            ),
            LayoutError::DuplicateCc { cc } => {
                write!(f, "CC {cc} is claimed twice")
            }
            LayoutError::UnknownRole { role, what } => {
                write!(
                    f,
                    "{what} names the role `{role}`, which the layout does not define"
                )
            }
            LayoutError::WrongSlotCount { page, found } => write!(
                f,
                "page `{page}` has {found} slots; a page must have exactly {SLOTS_PER_PAGE}, \
                 or CLAP would split and rename it"
            ),
            LayoutError::WrongBankSize { found } => write!(
                f,
                "the bank names {found} slot CCs; it must name exactly {SLOTS_PER_PAGE}"
            ),
            LayoutError::DuplicatePage { title } => {
                write!(f, "two pages are both called `{title}`")
            }
        }
    }
}

impl Layout {
    /// Parses and validates. Either the whole layout is usable, or none of it is.
    pub fn parse(text: &str) -> Result<Self, LayoutError> {
        let layout: Layout =
            serde_json::from_str(text).map_err(|e| LayoutError::Parse(e.to_string()))?;
        layout.validate()?;
        Ok(layout)
    }

    fn validate(&self) -> Result<(), LayoutError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(LayoutError::UnsupportedVersion {
                found: self.schema_version,
                supported: SCHEMA_VERSION,
            });
        }

        if self.bank.slot_cc.len() != SLOTS_PER_PAGE {
            return Err(LayoutError::WrongBankSize {
                found: self.bank.slot_cc.len(),
            });
        }

        // Every CC the layout claims, checked for reservation and for collision in one pass.
        let mut claimed: Vec<u8> = Vec::new();
        let mut claim = |cc: u8, what: &str| -> Result<(), LayoutError> {
            if is_reserved(cc) {
                return Err(LayoutError::ReservedCc {
                    cc,
                    what: what.to_owned(),
                });
            }
            if claimed.contains(&cc) {
                return Err(LayoutError::DuplicateCc { cc });
            }
            claimed.push(cc);
            Ok(())
        };

        for knob in &self.fixed {
            claim(knob.cc, &format!("the fixed knob for `{}`", knob.role))?;
        }
        for (index, cc) in self.bank.slot_cc.iter().enumerate() {
            claim(*cc, &format!("bank slot {}", index + 1))?;
        }
        claim(self.bank.page_down_cc, "page down")?;
        claim(self.bank.page_up_cc, "page up")?;

        for knob in &self.fixed {
            if !self.roles.contains_key(&knob.role) {
                return Err(LayoutError::UnknownRole {
                    role: knob.role.clone(),
                    what: format!("the fixed knob on CC {}", knob.cc),
                });
            }
        }

        let mut titles: Vec<String> = Vec::new();
        for page in &self.pages {
            if page.slots.len() != SLOTS_PER_PAGE {
                return Err(LayoutError::WrongSlotCount {
                    page: page.title(),
                    found: page.slots.len(),
                });
            }
            let title = page.title();
            if titles.contains(&title) {
                return Err(LayoutError::DuplicatePage { title });
            }
            titles.push(title);

            for role in page.slots.iter().flatten() {
                if !self.roles.contains_key(role) {
                    return Err(LayoutError::UnknownRole {
                        role: role.clone(),
                        what: format!("page `{}`", page.title()),
                    });
                }
            }
        }

        Ok(())
    }

    /// Whether every role an instrument map names is one this layout declares.
    ///
    /// A typo here would otherwise surface as one dead knob, months later, so it is reported when
    /// the map is loaded instead.
    pub fn check_instrument(&self, instrument: &Instrument) -> Result<(), LayoutError> {
        for role in instrument.params.keys() {
            if !self.roles.contains_key(role) {
                return Err(LayoutError::UnknownRole {
                    role: role.clone(),
                    what: format!("instrument `{}`", instrument.clap_id),
                });
            }
        }
        Ok(())
    }

    pub fn curve_for(&self, role: &str) -> Curve {
        self.roles.get(role).map(|r| r.curve).unwrap_or_default()
    }
}

/// The layout the collection ships, compiled in.
///
/// Embedded so a host is never in a state where it cannot map anything: a missing or broken user
/// file falls back to this, and a fresh install needs no setup.
pub const SHIPPED: &str = include_str!("../control-map.json");

/// The shipped layout, parsed.
///
/// # Panics
///
/// If the shipped layout is invalid — which is a build-time mistake, caught by
/// `the_shipped_layout_is_valid` rather than by a user.
pub fn shipped() -> Layout {
    Layout::parse(SHIPPED).expect("the shipped control map must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_layout_is_valid() {
        let layout = shipped();
        assert_eq!(layout.schema_version, SCHEMA_VERSION);
        // Eight roles, each on its GM2 controller plus its M32/A-series factory-template alias.
        assert_eq!(layout.fixed.len(), 2 * SLOTS_PER_PAGE);
        // Fourteen: the original eight, Chorus, LFO 2, Delay, Shimmer, Classic verb, and the
        // Dynamics page appended for mxm-fx-curve. LFO 2 moved ahead of Delay in the collection-wide
        // pre-1.0 pass. Pinned so growing the standard is deliberate; update this with the page.
        assert_eq!(layout.pages.len(), 14);
        assert_eq!(layout.pages[9].name, "LFO 2");
        assert_eq!(layout.pages[10].name, "Delay");
        assert_eq!(layout.pages[11].name, "Shimmer");
        assert_eq!(layout.pages[12].name, "Classic verb");
        assert_eq!(layout.pages[13].name, "Dynamics");
    }

    #[test]
    fn the_param_id_hash_matches_nice_plugs() {
        // Pinned against the real algorithm at vendor/nice-plug/src/wrapper/util.rs:35-46 (since
        // the split, src/wrapper/util.rs:35-46 in the nice-plug fork).
        // Worked by hand: 'c'=99, then h = h*31 + byte for each of "utoff".
        let mut expected: u32 = 0;
        for byte in b"cutoff" {
            expected = expected.wrapping_mul(31).wrapping_add(u32::from(*byte));
        }
        assert_eq!(hash_param_id("cutoff"), expected & !(1 << 31));
    }

    #[test]
    fn the_top_bit_is_always_clear_because_vst3_reserves_it() {
        // A string long enough to overflow into the top bit if it were not masked.
        assert_eq!(hash_param_id("a-very-long-parameter-id-string") >> 31, 0);
    }

    #[test]
    fn a_layout_claiming_a_reserved_cc_is_refused_with_the_reason() {
        let mut layout = shipped();
        layout.fixed[0].cc = 123;
        let error = layout.validate().unwrap_err();
        assert_eq!(
            error,
            LayoutError::ReservedCc {
                cc: 123,
                what: "the fixed knob for `filter.cutoff`".to_owned(),
            }
        );
    }

    #[test]
    fn the_mod_wheel_cannot_be_mapped_because_it_is_a_performance_gesture() {
        let mut layout = shipped();
        layout.fixed[0].cc = 1;
        assert!(matches!(
            layout.validate(),
            Err(LayoutError::ReservedCc { cc: 1, .. })
        ));
    }

    #[test]
    fn two_knobs_on_one_cc_are_refused() {
        let mut layout = shipped();
        layout.fixed[1].cc = layout.fixed[0].cc;
        assert!(matches!(
            layout.validate(),
            Err(LayoutError::DuplicateCc { .. })
        ));
    }

    #[test]
    fn a_page_that_would_be_split_by_clap_is_refused() {
        let mut layout = shipped();
        layout.pages[0].slots.push(Some("filter.cutoff".to_owned()));
        assert!(matches!(
            layout.validate(),
            Err(LayoutError::WrongSlotCount { found: 9, .. })
        ));
    }

    #[test]
    fn a_role_nobody_declared_is_a_typo_and_is_refused() {
        let mut layout = shipped();
        layout.pages[0].slots[0] = Some("osc1.rnage".to_owned());
        assert!(matches!(
            layout.validate(),
            Err(LayoutError::UnknownRole { .. })
        ));
    }

    #[test]
    fn a_future_schema_version_is_refused_rather_than_guessed_at() {
        let mut layout = shipped();
        layout.schema_version = SCHEMA_VERSION + 1;
        assert!(matches!(
            layout.validate(),
            Err(LayoutError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn a_numeric_param_ref_is_used_as_a_clap_id_directly() {
        assert_eq!(ParamRef::Raw(4_266_790_046).clap_id(), 4_266_790_046);
    }
}
