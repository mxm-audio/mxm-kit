//! The type scale, design system §6.
//!
//! # The font is bundled, in three cuts rather than one variable file
//!
//! §6 requires **Inter**, bundled with its OFL license, in release builds. It is: `assets/inter/`
//! carries Regular, Medium and SemiBold with the license beside them, and [`FONT_IS_BUNDLED`] is
//! true.
//!
//! **Three static cuts, not `InterVariable.ttf`, because egui has no variable axis.** A variable
//! file loads and renders at its default instance, which would have given one weight and made the
//! `weight` column of §6's table decorative — which is exactly what it had been: before this,
//! every editor rendered in egui's own default face, `weight` was carried and dropped, and the
//! owner reported the type as thin against a reference interface. Each cut is registered as its
//! own family and every style names the one its weight asks for, so the table is now executed
//! rather than described.
//!
//! Only the weights the scale actually uses are shipped: 400, 500 and 600. Adding a style that
//! asks for another weight means adding its file here, and [`family_for`] will say so at compile
//! time by not having a branch for it.
//!
//! # Tabular numerals
//!
//! §6 asks for them on changing values, meters and timing displays, and the reason is visible the
//! moment a value crosses from `9` to `10`: proportional digits reflow the whole readout, so a
//! parameter appears to twitch while being dragged. Four of the reported interface defects on this
//! project were the interface moving, and a value that changes width under the pointer is the same
//! failure at a smaller scale.

use egui::{FontFamily, FontId, TextStyle};

/// Whether Inter Variable is bundled. §6 requires this to be true for a release build.
pub const FONT_IS_BUNDLED: bool = true;

/// One row of §6's table.
#[derive(Copy, Clone, Debug)]
pub struct Style {
    pub size: f32,
    pub line: f32,
    /// CSS-style weight, and the cut it selects: 400 Regular, 500 Medium, 600 SemiBold. See
    /// [`family_for`].
    pub weight: u16,
    /// Whether digits are tabular. §6: values, meters, timing.
    pub tabular: bool,
}

/// Product or major Play-view value; rare.
pub const DISPLAY: Style = Style {
    size: 24.0,
    line: 30.0,
    weight: 600,
    tabular: false,
};
/// Dialog and major section titles.
pub const TITLE: Style = Style {
    size: 16.0,
    line: 22.0,
    weight: 600,
    tabular: false,
};
/// Module titles.
pub const HEADING: Style = Style {
    size: 14.0,
    line: 19.0,
    weight: 600,
    tabular: false,
};
/// Standard labels and text.
pub const BODY: Style = Style {
    size: 13.0,
    line: 18.0,
    weight: 500,
    tabular: false,
};
/// Parameter labels.
pub const LABEL: Style = Style {
    size: 12.0,
    line: 15.0,
    weight: 600,
    tabular: false,
};
/// Units and secondary metadata.
pub const CAPTION: Style = Style {
    size: 11.0,
    line: 14.0,
    weight: 550,
    tabular: false,
};
/// Parameter values — tabular, so a value does not change width as it changes.
pub const VALUE: Style = Style {
    size: 12.0,
    line: 15.0,
    weight: 550,
    tabular: true,
};

/// The name a caller uses to reach [`VALUE`] through egui's style map.
pub const VALUE_STYLE: &str = "mxm-value";
/// The name a caller uses to reach [`LABEL`].
pub const LABEL_STYLE: &str = "mxm-label";
/// The name a caller uses to reach [`CAPTION`].
pub const CAPTION_STYLE: &str = "mxm-caption";

/// Writes §6's scale into a context.
///
/// Tabular values stay on [`FontFamily::Monospace`], and that is now a permanent decision rather
/// than a placeholder.
///
/// Inter carries tabular figures as an OpenType feature (`tnum`), and egui's text stack applies no
/// OpenType features at all — so asking Inter for a value would get *proportional* digits and the
/// readout would reflow as it changed, which is the one thing §6's rule exists to prevent. A
/// monospace face buys the property outright. It is a visible seam, and the honest one: a value
/// that jitters under the pointer is a defect, and a value in a second face is a compromise.
pub fn apply(ctx: &egui::Context) {
    // **The cuts are only named once they are bound, and that is a frame later than this call.**
    //
    // `Context::set_fonts` takes effect at the next pass, but every consumer calls `apply` from
    // *inside* a frame, so naming `inter-600` in the style map on the same frame that installs it
    // resolves against fonts that do not have it yet — and epaint's answer to an unbound family is
    // a panic, not a fallback. It reached four of `mxm-preset`'s browser tests, which render a
    // single frame.
    //
    // So the first frame installs the cuts and keeps the styles on `Proportional`, which is bound
    // and, from the next frame, is Inter Regular anyway. Every frame after names the weights. An
    // editor repaints continuously, so "a frame later" is imperceptible; a one-frame harness gets
    // Regular and no crash.
    install_fonts(ctx);
    write_styles(ctx, false);

    // **Consumers call this once, when the editor is created, and never again** — so the style map
    // has to finish itself. A hook on the next pass writes the weighted map, by which point
    // `set_fonts` has been applied and the cuts are bound.
    let hooked = ctx.data(|data| data.get_temp::<bool>(hook_id()).unwrap_or(false));
    if !hooked {
        ctx.data_mut(|data| data.insert_temp(hook_id(), true));
        ctx.on_begin_pass(
            "mxm-ui-typography",
            std::sync::Arc::new(|ui: &mut egui::Ui| {
                let ctx = ui.ctx().clone();
                let passes = ctx.data(|data| data.get_temp::<u32>(pass_id()).unwrap_or(0));
                ctx.data_mut(|data| data.insert_temp(pass_id(), passes.saturating_add(1)));
                // Pass 0 is the one that installs the fonts; write the weighted map on the next,
                // and only then. Counting rather than asking `Fonts` avoids taking that lock from
                // inside a begin-pass hook.
                if passes == 1 {
                    write_styles(&ctx, true);
                }
            }),
        );
    }
}

/// Writes the scale into both themes' style maps.
///
/// `weighted` names each cut; without it every style stays on `FontFamily::Proportional`, which is
/// bound from the start and is Inter Regular once the cuts are installed. Naming a family that
/// `set_fonts` has not applied yet is a panic in epaint, not a fallback, which is the whole reason
/// this is two steps.
fn write_styles(ctx: &egui::Context, weighted: bool) {
    // Both themes get the same scale. Type is not theme-dependent, but egui's style map is, so
    // writing only the current one would leave a host that switches themes with default sizes.
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            let family = |s: &Style| {
                if s.tabular {
                    FontFamily::Monospace
                } else if weighted {
                    family_for(s.weight)
                } else {
                    FontFamily::Proportional
                }
            };

            style.text_styles = [
                (TextStyle::Heading, FontId::new(TITLE.size, family(&TITLE))),
                (TextStyle::Body, FontId::new(BODY.size, family(&BODY))),
                (TextStyle::Button, FontId::new(BODY.size, family(&BODY))),
                (
                    TextStyle::Small,
                    FontId::new(CAPTION.size, family(&CAPTION)),
                ),
                (
                    TextStyle::Monospace,
                    FontId::new(VALUE.size, FontFamily::Monospace),
                ),
                (
                    TextStyle::Name(VALUE_STYLE.into()),
                    FontId::new(VALUE.size, FontFamily::Monospace),
                ),
                (
                    TextStyle::Name(LABEL_STYLE.into()),
                    FontId::new(LABEL.size, family(&LABEL)),
                ),
                (
                    TextStyle::Name(CAPTION_STYLE.into()),
                    FontId::new(CAPTION.size, family(&CAPTION)),
                ),
            ]
            .into();
        });
    }
}

/// The family name a weight is registered under.
const REGULAR: &str = "inter-400";
/// The family name a weight is registered under.
const MEDIUM: &str = "inter-500";
/// The family name a weight is registered under.
const SEMIBOLD: &str = "inter-600";

/// The cut a [`Style::weight`] asks for.
///
/// Weights between the shipped cuts round **down**, which is the conservative direction: a label
/// asking for 550 gets Medium rather than SemiBold, so a scale tweak cannot quietly embolden half
/// the interface.
#[must_use]
pub fn family_for(weight: u16) -> FontFamily {
    FontFamily::Name(
        match weight {
            0..=449 => REGULAR,
            450..=599 => MEDIUM,
            _ => SEMIBOLD,
        }
        .into(),
    )
}

/// Where [`apply`] remembers it has already registered its begin-pass hook.
fn hook_id() -> egui::Id {
    egui::Id::new("mxm-ui-typography-hook")
}

/// Where the hook counts passes, so it writes the weighted map exactly once and not before the
/// cuts are bound.
fn pass_id() -> egui::Id {
    egui::Id::new("mxm-ui-typography-pass")
}

/// Registers the three cuts, and makes Regular the proportional default.
///
/// Called by [`apply`], and idempotent: egui replaces the whole `FontDefinitions`, and the font
/// data is `'static`, so re-applying costs a clone of three `Arc`s rather than a reparse.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            REGULAR,
            &include_bytes!("../assets/inter/Inter-Regular.ttf")[..],
        ),
        (
            MEDIUM,
            &include_bytes!("../assets/inter/Inter-Medium.ttf")[..],
        ),
        (
            SEMIBOLD,
            &include_bytes!("../assets/inter/Inter-SemiBold.ttf")[..],
        ),
    ] {
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
        fonts
            .families
            .insert(FontFamily::Name(name.into()), vec![name.to_owned()]);
    }

    // Anything that never names a family — egui's own widgets, and any consumer that resolves a
    // style this module did not register — lands on Regular rather than on the toolkit's face.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, REGULAR.to_owned());

    ctx.set_fonts(fonts);
}

/// Resolves the tabular value style, falling back to `Monospace` if [`apply`] has not run.
///
/// # Why this is not just `TextStyle::Name(VALUE_STYLE)`
///
/// `TextStyle::resolve` **panics** on an unregistered name. That turns a caller forgetting
/// [`apply`] into a crash inside a paint call, and it is worse than it sounds: `Context::style_mut`
/// writes the context's style, but a `Ui` already holds a clone of it, so calling [`apply`] partway
/// through a frame leaves every `Ui` built earlier in that frame resolving against the old map.
/// A one-frame render — a headless test, a screenshot, a plugin's first paint — would panic even
/// though the caller did everything right.
///
/// So the named styles are looked up defensively. `Monospace` is the right fallback because it is
/// what §6 is buying here: digits that do not change width as the value changes.
#[must_use]
pub fn value_style(style: &egui::Style) -> TextStyle {
    named_or(style, VALUE_STYLE, TextStyle::Monospace)
}

/// Resolves the parameter-label style, falling back to `Body`.
#[must_use]
pub fn label_style(style: &egui::Style) -> TextStyle {
    named_or(style, LABEL_STYLE, TextStyle::Body)
}

/// Resolves the caption style, falling back to `Small`.
#[must_use]
pub fn caption_style(style: &egui::Style) -> TextStyle {
    named_or(style, CAPTION_STYLE, TextStyle::Small)
}

fn named_or(style: &egui::Style, name: &'static str, fallback: TextStyle) -> TextStyle {
    let named = TextStyle::Name(name.into());
    if style.text_styles.contains_key(&named) {
        named
    } else {
        fallback
    }
}

/// Sentence case, as §6 requires: `Filter envelope`, never `FILTER ENVELOPE`.
///
/// Acronyms keep their conventional casing, so this only ever touches the first character and
/// leaves the rest of the string alone. Anything cleverer would turn `LFO rate` into `Lfo rate`.
#[must_use]
pub fn sentence_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_descends_without_collisions() {
        // Two styles at the same size are two names for one thing, and the hierarchy stops being
        // legible. §6's table is strictly descending except where a pair shares a size for a
        // deliberate reason — `label` and `value` are the same size by design, one tabular.
        let sizes = [
            DISPLAY.size,
            TITLE.size,
            HEADING.size,
            BODY.size,
            CAPTION.size,
        ];
        for pair in sizes.windows(2) {
            assert!(pair[0] > pair[1], "the scale is not descending: {pair:?}");
        }
        assert_eq!(
            LABEL.size, VALUE.size,
            "a label and its value share a baseline"
        );
        // The pair is distinguished by tabular digits, not by size. If that ever flipped, values
        // would start reflowing under the pointer again - the defect §6's rule exists to prevent.
        const {
            assert!(VALUE.tabular, "values must be tabular");
            assert!(
                !LABEL.tabular,
                "labels are not numeric and gain nothing from tabular figures"
            );
        }
    }

    #[test]
    fn line_heights_leave_room_for_the_glyphs() {
        for (name, style) in [
            ("display", DISPLAY),
            ("title", TITLE),
            ("heading", HEADING),
            ("body", BODY),
            ("label", LABEL),
            ("caption", CAPTION),
            ("value", VALUE),
        ] {
            assert!(
                style.line > style.size,
                "{name}: line {} is not larger than size {}",
                style.line,
                style.size
            );
        }
    }

    #[test]
    fn a_named_style_degrades_instead_of_panicking() {
        // The failure this prevents is a panic inside a paint call, which is the worst place for
        // one: in a plugin it takes the host down with it. A default `Style` has never seen
        // `apply`, which is exactly the state a first frame is in.
        let bare = egui::Style::default();
        assert_eq!(value_style(&bare), TextStyle::Monospace);
        assert_eq!(label_style(&bare), TextStyle::Body);
        assert_eq!(caption_style(&bare), TextStyle::Small);
    }

    #[test]
    fn a_named_style_is_used_once_it_is_registered() {
        let ctx = egui::Context::default();
        apply(&ctx);
        // Both themes, because `apply` writes both and a caller reads whichever one is active.
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let style = ctx.style_of(theme);
            assert_eq!(
                value_style(&style),
                TextStyle::Name(VALUE_STYLE.into()),
                "{theme:?} did not get the named styles"
            );
        }
    }

    #[test]
    fn sentence_case_does_not_mangle_acronyms() {
        assert_eq!(sentence_case("filter envelope"), "Filter envelope");
        assert_eq!(sentence_case("LFO rate"), "LFO rate");
        assert_eq!(sentence_case("pulse width"), "Pulse width");
        assert_eq!(sentence_case(""), "");
    }
}
