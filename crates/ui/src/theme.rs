//! Semantic theme tokens, design system §5.
//!
//! Components consume these and never embed a raw colour. That is §5's opening rule, and it is
//! what makes an instrument identity accent a one-line change rather than a search-and-replace.
//!
//! This module started in the player, where it fixed a real defect: egui's defaults leave an
//! inactive button with no visible border, so a control only *looked* like a control once the
//! pointer was over it. The design system had already said what the answer was:
//!
//! > Cards use flat surfaces, a one-pixel border, and spacing — not simulated panels — to
//! > establish groups.
//!
//! It moved here at M4a, which is where §13 says it belongs.
//!
//! # Both themes are first-class
//!
//! §5 requires it, so the tokens come in pairs and **nothing is derived** by lightening or
//! darkening the other. [`apply`] writes both sets into the context, so a host that switches at
//! runtime is followed rather than sampled once.
//!
//! # Identity accents
//!
//! §5.3 lets an instrument replace `accent` with one approved hue, which must pass 4.5:1 for text
//! and 3:1 for control boundaries **in both themes**. [`Tokens::with_accent`] is how, and
//! [`contrast`] is how the ratio is measured rather than eyeballed — the design system says
//! measured, and a candidate that fails is discarded rather than adjusted by eye.

use egui::{Color32, Stroke};

/// One theme's tokens: the neutral and interaction set of §5.1, and the modulation set of §5.2.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub canvas: Color32,
    pub surface_1: Color32,
    pub surface_2: Color32,
    pub surface_3: Color32,
    pub border: Color32,
    /// The unfilled part of a knob's arc or a slider's rail.
    ///
    /// **Its own token, and not `surface-2`.** It was `surface-2` — the nested-group fill — which
    /// made it invisible in the two places it most needs to be seen: on a nested surface, where it
    /// *is* the background, and on a white card, where it is a 1.1:1 difference. Reported as
    /// *"the knobs do not show the range of the knob"*. A track has to read against every surface a
    /// control can be drawn on, so it is defined against all of them rather than against one.
    pub track: Color32,
    pub border_strong: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub focus: Color32,
    pub selection: Color32,
    pub text_disabled: Color32,
    pub danger: Color32,
    pub warning: Color32,
    pub success: Color32,

    // §5.2. The same category in both themes, and never the only carrier of meaning: a
    // modulation connection also gets a source label, and a bipolar amount gets a signed value.
    pub mod_envelope: Color32,
    pub mod_lfo: Color32,
    pub mod_key_voice: Color32,
    pub mod_random: Color32,
    pub mod_performance: Color32,
}

impl Tokens {
    /// Replaces `accent`, `accent_hover` and `focus` with an instrument identity hue (§5.3).
    ///
    /// Deliberately does **not** touch the modulation or status colours: §5.3 forbids an identity
    /// accent from replacing them, because their whole value is being the same in every
    /// instrument.
    #[must_use]
    pub const fn with_accent(mut self, accent: Color32, hover: Color32, focus: Color32) -> Self {
        self.accent = accent;
        self.accent_hover = hover;
        self.focus = focus;
        self
    }
}

/// WCAG relative luminance.
fn luminance(colour: Color32) -> f32 {
    let channel = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
}

/// WCAG contrast ratio between two colours, `1.0..=21.0`.
///
/// Public because §5.3 requires an identity accent's ratios to be **measured and recorded** before
/// the accent is chosen. An instrument's brief cites numbers from this function.
#[must_use]
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    let (light, dark) = if x > y { (x, y) } else { (y, x) };
    (light + 0.05) / (dark + 0.05)
}

pub const DARK: Tokens = Tokens {
    canvas: Color32::from_rgb(0x10, 0x12, 0x16),
    surface_1: Color32::from_rgb(0x17, 0x1A, 0x20),
    surface_2: Color32::from_rgb(0x1E, 0x22, 0x2A),
    surface_3: Color32::from_rgb(0x27, 0x2C, 0x35),
    track: Color32::from_rgb(0x3E, 0x46, 0x54),
    border: Color32::from_rgb(0x34, 0x3A, 0x45),
    border_strong: Color32::from_rgb(0x4A, 0x52, 0x60),
    text_primary: Color32::from_rgb(0xF4, 0xF5, 0xF7),
    text_secondary: Color32::from_rgb(0xA9, 0xB0, 0xBC),
    accent: Color32::from_rgb(0x4C, 0xC9, 0xD8),
    accent_hover: Color32::from_rgb(0x79, 0xD7, 0xE3),
    focus: Color32::from_rgb(0x92, 0xE4, 0xED),
    selection: Color32::from_rgb(0x25, 0x4B, 0x54),
    text_disabled: Color32::from_rgb(0x6F, 0x76, 0x82),
    danger: Color32::from_rgb(0xFF, 0x70, 0x7A),
    warning: Color32::from_rgb(0xF2, 0xB8, 0x4B),
    success: Color32::from_rgb(0x43, 0xC5, 0x9E),

    mod_envelope: Color32::from_rgb(0xFF, 0x7A, 0x82),
    mod_lfo: Color32::from_rgb(0x56, 0xA8, 0xFF),
    mod_key_voice: Color32::from_rgb(0xA9, 0x8A, 0xFF),
    mod_random: Color32::from_rgb(0xF2, 0xB8, 0x4B),
    mod_performance: Color32::from_rgb(0x43, 0xC5, 0x9E),
};

pub const LIGHT: Tokens = Tokens {
    canvas: Color32::from_rgb(0xF2, 0xF3, 0xF5),
    surface_1: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface_2: Color32::from_rgb(0xE9, 0xEB, 0xEF),
    surface_3: Color32::from_rgb(0xDD, 0xE1, 0xE6),
    track: Color32::from_rgb(0xBF, 0xC6, 0xD0),
    border: Color32::from_rgb(0xC8, 0xCD, 0xD5),
    border_strong: Color32::from_rgb(0xAA, 0xB1, 0xBC),
    text_primary: Color32::from_rgb(0x17, 0x1A, 0x1F),
    text_secondary: Color32::from_rgb(0x5C, 0x64, 0x70),
    accent: Color32::from_rgb(0x24, 0x7F, 0x91),
    accent_hover: Color32::from_rgb(0x17, 0x6B, 0x7B),
    focus: Color32::from_rgb(0x0C, 0x65, 0x75),
    selection: Color32::from_rgb(0xCB, 0xEA, 0xF0),
    text_disabled: Color32::from_rgb(0x8A, 0x92, 0x9D),
    danger: Color32::from_rgb(0xB8, 0x3A, 0x45),
    warning: Color32::from_rgb(0x8A, 0x5A, 0x00),
    success: Color32::from_rgb(0x14, 0x7D, 0x62),

    mod_envelope: Color32::from_rgb(0xC6, 0x4C, 0x55),
    mod_lfo: Color32::from_rgb(0x24, 0x78, 0xC7),
    mod_key_voice: Color32::from_rgb(0x77, 0x50, 0xC9),
    mod_random: Color32::from_rgb(0x9D, 0x6C, 0x00),
    mod_performance: Color32::from_rgb(0x14, 0x7D, 0x62),
};

/// The tokens for the theme a context is currently painting in.
///
/// A design system §5.3 **instrument identity accent**: one approved hue, tuned separately for each
/// theme.
///
/// The values live here rather than in a plugin because `AGENTS.md` is explicit that a consumer
/// needing a colour this crate does not expose adds the token here rather than the literal there —
/// and because §5.3's word is *approved*, which means approval has a home. An instrument selects
/// one; it does not invent one.
///
/// §5.3's requirements are the gate: 4.5:1 against the surface for text, 3:1 for control
/// boundaries, in **both** themes, measured rather than judged, and not a recreation of the source
/// hardware's own colour arrangement.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub dark: Color32,
    pub light: Color32,
}

/// `mxm-mono-03`'s. Measured against `surface-1` and `surface-2` in both themes:
/// **12.09 / 11.06** dark, **6.11 / 5.12** light.
///
/// Chosen over the other passing candidates because it sits about 85 degrees of hue from the
/// nearest reserved colour (`success`), where orchid and coral sit about 20 from `mod-key-voice`
/// and `danger` — and a control boundary that reads as a warning is a real cost.
pub const ACID_LIME: Identity = Identity {
    dark: Color32::from_rgb(0xB6, 0xE8, 0x2B),
    light: Color32::from_rgb(0x4F, 0x6B, 0x00),
};

/// `mxm-poly-06`'s. Measured against `surface-1` and `surface-2` in both themes:
/// **7.38 / 6.75** dark, **7.19 / 6.03** light.
///
/// Chosen from what the wheel had left: lime is `mxm-mono-03`'s, orchid and coral are on
/// `mxm-mono-01`'s candidate list, and blue, red, amber, green, violet and teal are the modulation
/// and status colours. Rose sits about 20° from `danger` — the same distance orchid and coral sit
/// from their neighbours — and further from every modulation hue than any other candidate that
/// passed. Recorded in `docs/briefs/mxm-poly-06.md` §7 with the alternatives measured.
pub const ROSE: Identity = Identity {
    dark: Color32::from_rgb(0xFF, 0x7E, 0xB3),
    light: Color32::from_rgb(0xA8, 0x13, 0x5E),
};

/// `mxm-mono-00`'s. Measured against `surface-1` and `surface-2` in both themes:
/// **7.69 / 7.03** dark, **6.82 / 5.72** light.
///
/// Chosen for hue separation above all: this instrument's accent marks **active matrix
/// connections**, and a connection from an LFO must not be the LFO's own blue. Copper sits about
/// 20° from `danger` and from `warning`, the distance the other briefs accepted on a crowded
/// wheel, and is the darkest of the warm candidates, so the furthest from `mxm-mono-01`'s coral.
/// Recorded in `docs/briefs/mxm-mono-00.md` §7 with the alternatives measured.
pub const COPPER: Identity = Identity {
    dark: Color32::from_rgb(0xE8, 0x9A, 0x6B),
    light: Color32::from_rgb(0x8A, 0x4A, 0x1C),
};

/// mxm-mono-02's identity accent: orchid.
///
/// Chosen from what the wheel had left after lime, rose and copper — and after the collection's
/// default accent, which `mxm-mono-01` wears and which is itself an aqua. Orchid sits about 39°
/// from `mod-key-voice`'s violet on one side and from rose on the other, the widest gap left on
/// the wheel, and cold where the two warm instruments' accents are warm. Recorded in
/// `docs/briefs/mxm-mono-02.md` §7 with the alternatives measured, including the aqua that was
/// taken first and withdrawn for the collision.
pub const ORCHID: Identity = Identity {
    dark: Color32::from_rgb(0xE0, 0x7B, 0xEA),
    light: Color32::from_rgb(0x8A, 0x2E, 0x9C),
};

/// `mxm-para-07`'s identity accent: leaf green.
///
/// Measured against `surface-1` and `surface-2` in both themes: **8.83 / 8.07** dark,
/// **6.60 / 5.53** light. It marks structure and selection only; modulation and status retain
/// their collection-wide colours. The values and the hue-separation decision are recorded in
/// `docs/briefs/mxm-para-07.md` §7.
pub const LEAF_GREEN: Identity = Identity {
    dark: Color32::from_rgb(0x63, 0xCF, 0x63),
    light: Color32::from_rgb(0x2F, 0x69, 0x2F),
};

impl Tokens {
    /// This set with an instrument's identity accent in place of the collection default.
    ///
    /// **Replaces `accent` and `accent_hover` only.** §5.3 permits replacing the accent and forbids
    /// replacing the fixed modulation and status colours; `focus` stays the system's, because a
    /// focus ring is an accessibility affordance that should look the same everywhere rather than
    /// per instrument.
    #[must_use]
    pub const fn with_identity(mut self, identity: Identity, dark_mode: bool) -> Self {
        let accent = if dark_mode {
            identity.dark
        } else {
            identity.light
        };
        self.accent = accent;
        self.accent_hover = accent;
        self
    }
}

/// For the rare control that needs a token `style` cannot deliver through `Visuals` - egui's
/// selected-button path, for instance, swaps the fill and the text but never the border, so a
/// control wanting SS 7.2's full selected treatment (fill *and* accent border) must paint the
/// border itself.
pub fn tokens(ctx: &egui::Context) -> &'static Tokens {
    match ctx.theme() {
        egui::Theme::Dark => &DARK,
        egui::Theme::Light => &LIGHT,
    }
}

/// The environment variable that overrides the theme an editor opens in.
pub const THEME_ENV: &str = "MXM_EDITOR_THEME";

/// The theme an editor should open in.
///
/// **Light unless told otherwise, and the fallback is the whole reason this exists.** egui
/// defaults to [`egui::ThemePreference::System`] and resolves it against the system theme the
/// *integration* reports — egui-baseview reports none, so System silently resolves to Dark and the
/// editor opened dark beside a player following a light desktop. Every editor therefore chose
/// Light explicitly, in eight copies of one line.
///
/// This is that line, with two ways to change it, in the order they are consulted:
///
/// 1. **`MXM_EDITOR_THEME=dark|light|system`**, read once when the editor opens. It exists because
///    the dark theme had **no way to be reached at all** — not from a menu, not from a host, not
///    from a script — which made a dark screenshot of a plugin impossible to take and the dark
///    half of §5 impossible to review. It wins, and it is never written back: a capture run must
///    not change what the person at the machine chose.
/// 2. **What the person chose** in [`crate::shell::theme_control`], from [`stored`].
///
/// Light when neither says otherwise, for the fallback reason above.
///
/// An unrecognised value is Light rather than a panic. A plugin runs inside somebody else's DAW,
/// and a typo in an environment variable is not worth taking the host down for.
pub fn preference() -> egui::ThemePreference {
    match std::env::var(THEME_ENV) {
        Ok(value) => from_name(&value),
        Err(_) => stored().unwrap_or(egui::ThemePreference::Light),
    }
}

/// A theme by index — **0 light, 1 dark, 2 system**, the order [`crate::shell::theme_control`]
/// offers them in, and `None` for anything else.
///
/// The developer channel's spelling: a control change carries a number, and CC 119 already means
/// "an index into the thing the control lists". One function so a plugin, a test and a screenshot
/// script cannot disagree about which number is which theme.
#[must_use]
pub fn from_index(index: u8) -> Option<egui::ThemePreference> {
    match index {
        0 => Some(egui::ThemePreference::Light),
        1 => Some(egui::ThemePreference::Dark),
        2 => Some(egui::ThemePreference::System),
        _ => None,
    }
}

/// What the control calls each choice. §10 names the three, and every interface in the collection
/// uses these words for them.
#[must_use]
pub fn label_of(preference: egui::ThemePreference) -> &'static str {
    match preference {
        egui::ThemePreference::Dark => "Dark",
        egui::ThemePreference::Light => "Light",
        egui::ThemePreference::System => "System",
    }
}

/// How a theme is written down — in the file, and in `MXM_EDITOR_THEME`. [`from_name`] reads it
/// back, and the player spells its own saved choice the same way.
#[must_use]
pub fn name_of(preference: egui::ThemePreference) -> &'static str {
    match preference {
        egui::ThemePreference::Dark => "dark",
        egui::ThemePreference::Light => "light",
        egui::ThemePreference::System => "system",
    }
}

/// Where an editor's theme choice is remembered.
///
/// **Not in the plugin's state**, which is where presets and a host's saved project live: §10 is
/// explicit that a preset must not silently change the editor's theme, and state is exactly how it
/// would. It goes beside the collection's presets instead — `<config>/mxm/editor.json` — and it is
/// **one file for every MXM editor**, because somebody who chose dark chose it for the collection
/// and not for one instrument.
///
/// `None` when the platform reports no config directory. Rare and real, and every caller here
/// treats it as "nothing saved" rather than as a failure worth telling anyone about.
#[must_use]
pub fn state_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|dir| dir.join("mxm").join("editor.json"))
}

/// The saved choice, or `None` when there is no file, no directory, or nothing readable in it.
#[must_use]
pub fn stored() -> Option<egui::ThemePreference> {
    stored_at(&state_path()?)
}

/// [`stored`], from a named file.
///
/// Split out for the reason [`from_name`] is: a test that had to write the real config directory
/// would be a test that depends on the machine it runs on.
#[must_use]
pub fn stored_at(path: &std::path::Path) -> Option<egui::ThemePreference> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(from_name(value.get("theme")?.as_str()?))
}

/// Remembers a choice for the next editor to open. Errors are returned, never raised.
pub fn store(preference: egui::ThemePreference) -> Result<(), String> {
    let path = state_path().ok_or("this platform reports no configuration directory")?;
    store_at(&path, preference)
}

/// [`store`], to a named file.
///
/// **Whatever else is in the file survives**, so a later piece of interface state added beside this
/// one is not erased by an editor that predates it. Written to a temporary file and renamed, so an
/// interrupted write cannot leave half a file where a whole one was.
pub fn store_at(path: &std::path::Path, preference: egui::ThemePreference) -> Result<(), String> {
    let mut value = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    value["theme"] = serde_json::Value::String(name_of(preference).to_owned());

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&temp, text).map_err(|e| format!("{}: {e}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parses a theme name, case- and whitespace-insensitively. Anything else is Light.
///
/// Split from [`preference`] so it can be tested without touching the process environment, which
/// is global and would make the tests order-dependent under a threaded runner.
pub fn from_name(name: &str) -> egui::ThemePreference {
    match name.trim().to_ascii_lowercase().as_str() {
        "dark" => egui::ThemePreference::Dark,
        "system" => egui::ThemePreference::System,
        _ => egui::ThemePreference::Light,
    }
}

/// Applies the tokens to a context, matching the theme it is already in.
pub fn apply(ctx: &egui::Context) {
    // Both themes get their own set, so a host that switches at runtime is followed rather than
    // sampled once. Neither is derived from the other.
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let dark = theme == egui::Theme::Dark;
        let tokens = if dark { &DARK } else { &LIGHT };
        let mut visuals = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        style(&mut visuals, tokens);
        ctx.set_visuals_of(theme, visuals);
    }
}

/// Writes one theme's tokens into a `Visuals`.
fn style(visuals: &mut egui::Visuals, tokens: &Tokens) {
    visuals.panel_fill = tokens.surface_1;
    visuals.window_fill = tokens.surface_1;
    visuals.extreme_bg_color = tokens.canvas;
    visuals.faint_bg_color = tokens.surface_2;
    visuals.override_text_color = Some(tokens.text_primary);
    visuals.hyperlink_color = tokens.accent;
    visuals.selection.bg_fill = tokens.selection;
    visuals.selection.stroke = Stroke::new(1.0, tokens.accent);
    visuals.window_stroke = Stroke::new(1.0, tokens.border);

    // **The fix.** `inactive` is what an untouched control looks like, and egui's default leaves it
    // with no border — so a button read as a label until hovered.
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = tokens.surface_1;
    widgets.noninteractive.weak_bg_fill = tokens.surface_1;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, tokens.border);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, tokens.text_secondary);

    widgets.inactive.bg_fill = tokens.surface_2;
    widgets.inactive.weak_bg_fill = tokens.surface_2;
    widgets.inactive.bg_stroke = Stroke::new(1.0, tokens.border);
    widgets.inactive.fg_stroke = Stroke::new(1.0, tokens.text_primary);

    widgets.hovered.bg_fill = tokens.surface_3;
    widgets.hovered.weak_bg_fill = tokens.surface_3;
    widgets.hovered.bg_stroke = Stroke::new(1.0, tokens.border_strong);
    widgets.hovered.fg_stroke = Stroke::new(1.0, tokens.text_primary);

    widgets.active.bg_fill = tokens.surface_3;
    widgets.active.weak_bg_fill = tokens.surface_3;
    widgets.active.bg_stroke = Stroke::new(1.0, tokens.accent);
    widgets.active.fg_stroke = Stroke::new(1.0, tokens.text_primary);

    // Selected controls carry the accent as a border as well as a fill: §7.2 requires a toggle to
    // show state through fill **and** another treatment, never hue alone.
    widgets.open.bg_fill = tokens.surface_3;
    widgets.open.weak_bg_fill = tokens.surface_3;
    widgets.open.bg_stroke = Stroke::new(1.0, tokens.border_strong);
    widgets.open.fg_stroke = Stroke::new(1.0, tokens.text_primary);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of this test's own, in the temp directory, named so two tests running at once cannot
    /// meet. The real config directory is never touched: a test that wrote it would change the
    /// theme of every editor on the machine that ran it.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mxm-ui-theme-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("editor.json")
    }

    #[test]
    fn a_stored_theme_comes_back() {
        let path = scratch("round-trip");
        assert_eq!(
            stored_at(&path),
            None,
            "nothing is stored before anything is"
        );

        store_at(&path, egui::ThemePreference::Dark).expect("the file is written");
        assert_eq!(stored_at(&path), Some(egui::ThemePreference::Dark));

        store_at(&path, egui::ThemePreference::System).expect("and rewritten");
        assert_eq!(stored_at(&path), Some(egui::ThemePreference::System));
    }

    #[test]
    fn storing_a_theme_keeps_whatever_else_the_file_holds() {
        // The next piece of interface state to be saved beside this one must survive an editor
        // that predates it, or the two will erase each other on alternate launches.
        let path = scratch("other-keys");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"something-later": 7}"#).unwrap();

        store_at(&path, egui::ThemePreference::Light).expect("the file is written");

        let text = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["something-later"], 7);
        assert_eq!(value["theme"], "light");
    }

    #[test]
    fn a_damaged_theme_file_reads_as_nothing_stored() {
        // It is a preference file that a person can open in an editor. Refusing to start, or
        // panicking inside a DAW, is never the answer to a stray brace.
        let path = scratch("damaged");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(stored_at(&path), None);

        std::fs::write(&path, r#"{"theme": 3}"#).unwrap();
        assert_eq!(
            stored_at(&path),
            None,
            "a theme that is not a name is not a theme"
        );
    }

    #[test]
    fn the_developer_channel_spells_the_themes_by_index() {
        // CC 116 carries these numbers, and `plugins/AGENTS.md` publishes them (in full, this
        // repository's `docs/plugin-conventions.md`). The order is the control's own: Light, Dark,
        // System.
        assert_eq!(from_index(0), Some(egui::ThemePreference::Light));
        assert_eq!(from_index(1), Some(egui::ThemePreference::Dark));
        assert_eq!(from_index(2), Some(egui::ThemePreference::System));
        assert_eq!(from_index(3), None);
        assert_eq!(from_index(127), None);
    }

    #[test]
    fn a_name_survives_being_written_down_and_read_back() {
        for preference in [
            egui::ThemePreference::Light,
            egui::ThemePreference::Dark,
            egui::ThemePreference::System,
        ] {
            assert_eq!(from_name(name_of(preference)), preference);
        }
    }

    #[test]
    fn an_unset_or_unrecognised_theme_is_light_rather_than_a_panic() {
        // Light, not System: egui resolves System against a system theme egui-baseview never
        // reports, and the fallback for that is Dark. An editor must not open dark by accident.
        assert_eq!(from_name(""), egui::ThemePreference::Light);
        assert_eq!(from_name("darkk"), egui::ThemePreference::Light);
        assert_eq!(from_name("1"), egui::ThemePreference::Light);
    }

    #[test]
    fn a_theme_name_is_read_however_it_was_typed() {
        // Environment variables get typed by hand and pasted from shells that keep the newline.
        for name in [
            "dark", "Dark", "DARK", " dark ", "dark
",
        ] {
            assert_eq!(
                from_name(name),
                egui::ThemePreference::Dark,
                "`{name:?}` should select Dark"
            );
        }
        assert_eq!(from_name("light"), egui::ThemePreference::Light);
        assert_eq!(from_name("System"), egui::ThemePreference::System);
    }

    #[test]
    fn a_border_is_visible_against_the_surface_it_sits_on() {
        // The actual complaint: a control that only looks like a control on hover. A border needs
        // to be *seen*, so this measures it rather than trusting the hex.
        for (name, tokens) in [("dark", &DARK), ("light", &LIGHT)] {
            let ratio = contrast(tokens.border, tokens.surface_2);
            assert!(
                ratio > 1.15,
                "{name}: border on surface-2 is only {ratio:.2}:1"
            );
        }
    }

    #[test]
    fn hovering_strengthens_the_border_rather_than_introducing_one() {
        // Hover should be a change of degree. It used to be the difference between no border and a
        // border, which is what made an untouched control read as a label.
        for (name, tokens) in [("dark", &DARK), ("light", &LIGHT)] {
            let resting = contrast(tokens.border, tokens.surface_2);
            let hovered = contrast(tokens.border_strong, tokens.surface_3);
            assert!(
                hovered > resting,
                "{name}: hover ({hovered:.2}) should be stronger than rest ({resting:.2})"
            );
        }
    }

    #[test]
    fn body_text_meets_wcag_aa_on_its_surfaces() {
        // The design system says `text-primary` and `text-secondary` meet AA. Measured, not
        // assumed — 4.5:1 for body text.
        for (name, tokens) in [("dark", &DARK), ("light", &LIGHT)] {
            for (label, surface) in [
                ("surface-1", tokens.surface_1),
                ("surface-2", tokens.surface_2),
            ] {
                let ratio = contrast(tokens.text_primary, surface);
                assert!(
                    ratio >= 4.5,
                    "{name}: text-primary on {label} is {ratio:.2}:1"
                );
            }
        }
    }

    #[test]
    fn secondary_text_meets_wcag_aa_too() {
        for (name, tokens) in [("dark", &DARK), ("light", &LIGHT)] {
            let ratio = contrast(tokens.text_secondary, tokens.surface_1);
            assert!(
                ratio >= 4.5,
                "{name}: text-secondary on surface-1 is {ratio:.2}:1"
            );
        }
    }

    #[test]
    fn status_colours_meet_aa_on_their_surfaces() {
        // §5.1 puts danger, warning and success in the same table as body text. A warning nobody
        // can read is worse than no warning, and light-theme amber is the classic way to get this
        // wrong — which is why `warning` is `#8A5A00` in light rather than the dark theme's amber.
        for (name, tokens) in [("dark", &DARK), ("light", &LIGHT)] {
            for (label, colour) in [
                ("danger", tokens.danger),
                ("warning", tokens.warning),
                ("success", tokens.success),
            ] {
                let ratio = contrast(colour, tokens.surface_1);
                assert!(ratio >= 4.5, "{name}: {label} on surface-1 is {ratio:.2}:1");
            }
        }
    }

    /// Hue, in turns, from an sRGB triple. Enough for "are these two different colours".
    fn hue(colour: Color32) -> f32 {
        let (r, g, b) = (
            f32::from(colour.r()) / 255.0,
            f32::from(colour.g()) / 255.0,
            f32::from(colour.b()) / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        if delta <= f32::EPSILON {
            return 0.0;
        }
        let h = if max == r {
            ((g - b) / delta).rem_euclid(6.0)
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        (h / 6.0).rem_euclid(1.0)
    }

    #[test]
    fn modulation_sources_separate_by_hue_not_by_lightness() {
        // This test was written to assert luminance separation and **failed**, which turned out to
        // be the useful result: §5.2's five sources sit within 0.034 relative luminance of each
        // other in dark (`mod-envelope` 0.368, `mod-lfo` 0.372, `mod-key-voice` 0.338). They are
        // equal-weight categories, so equal lightness is deliberate — but it means greyscale and
        // severe colour-vision deficiency cannot tell them apart.
        //
        // So §5.2's *"colour never acts alone. A modulation connection also uses a source label or
        // icon"* is not a nicety layered on top of a palette that would survive without it. It is
        // load-bearing, and any code that draws a modulation indicator without its label is a bug.
        //
        // What the palette does guarantee is hue separation, and that is what this measures.
        for (name, tokens) in [("dark", &DARK), ("light", &LIGHT)] {
            let sources = [
                ("envelope", tokens.mod_envelope),
                ("lfo", tokens.mod_lfo),
                ("key-voice", tokens.mod_key_voice),
                ("random", tokens.mod_random),
                ("performance", tokens.mod_performance),
            ];
            for (i, (a_name, a)) in sources.iter().enumerate() {
                for (b_name, b) in &sources[i + 1..] {
                    let raw = (hue(*a) - hue(*b)).abs();
                    let separation = raw.min(1.0 - raw);
                    assert!(
                        separation >= 0.05,
                        "{name}: {a_name} and {b_name} are {separation:.3} turns apart"
                    );
                }
            }
        }
    }

    #[test]
    fn an_identity_accent_is_measured_before_it_is_taken() {
        // §5.3's own thresholds, applied to the default accent so the harness that will judge
        // mxm-mono-01's identity hue is proven on a value we already trust.
        for (name, tokens) in [("dark", &DARK), ("light", &LIGHT)] {
            assert!(
                contrast(tokens.accent, tokens.surface_1) >= 3.0,
                "{name}: accent fails the 3:1 control-boundary threshold"
            );
        }
    }

    #[test]
    fn with_accent_leaves_the_fixed_colours_alone() {
        // §5.3: an identity accent must not replace modulation or status colours.
        let custom = DARK.with_accent(
            Color32::from_rgb(0xC0, 0x8A, 0xFF),
            Color32::from_rgb(0xD3, 0xAA, 0xFF),
            Color32::from_rgb(0xE0, 0xC4, 0xFF),
        );
        assert_eq!(custom.mod_lfo, DARK.mod_lfo);
        assert_eq!(custom.danger, DARK.danger);
        assert_ne!(custom.accent, DARK.accent);
    }

    #[test]
    fn leaf_green_meets_its_brief_and_changes_identity_only() {
        for (name, base, accent) in [
            ("dark", DARK, LEAF_GREEN.dark),
            ("light", LIGHT, LEAF_GREEN.light),
        ] {
            for (surface_name, surface) in
                [("surface-1", base.surface_1), ("surface-2", base.surface_2)]
            {
                let ratio = contrast(accent, surface);
                assert!(
                    ratio >= 4.5,
                    "{name}: leaf green on {surface_name} is only {ratio:.2}:1"
                );
            }
            let identified = base.with_identity(LEAF_GREEN, name == "dark");
            assert_eq!(identified.accent, accent);
            assert_eq!(identified.mod_envelope, base.mod_envelope);
            assert_eq!(identified.mod_lfo, base.mod_lfo);
            assert_eq!(identified.mod_key_voice, base.mod_key_voice);
            assert_eq!(identified.mod_random, base.mod_random);
            assert_eq!(identified.mod_performance, base.mod_performance);
            assert_eq!(identified.danger, base.danger);
            assert_eq!(identified.warning, base.warning);
            assert_eq!(identified.success, base.success);
        }
    }

    #[test]
    fn the_two_themes_are_genuinely_different_sets() {
        // Not one derived from the other by inversion: the design system treats both as
        // first-class, and a derived palette drifts out of contrast in one of them.
        assert_ne!(DARK.accent, LIGHT.accent);
        assert_ne!(DARK.border, LIGHT.border);
    }
}
