//! The spacing grid, design system §4.1.
//!
//! All values are **logical pixels** before host DPI scaling, so they scale with the editor rather
//! than being recomputed per zoom level. §4.1's rule is a 4 px grid for component geometry and an
//! 8 px rhythm for layout; arbitrary values are allowed only for optical centring and data
//! visualization.
//!
//! Named constants rather than literals because the grid is the thing that makes two instruments
//! by two different hands look like one collection. A stray `7.0` in a card is invisible in review
//! and obvious side by side.

/// Reset.
pub const SPACE_0: f32 = 0.0;
/// Optical adjustment only — not layout.
pub const SPACE_1: f32 = 2.0;
/// Tight internal spacing.
pub const SPACE_2: f32 = 4.0;
/// Related controls.
pub const SPACE_3: f32 = 8.0;
/// Component padding.
pub const SPACE_4: f32 = 12.0;
/// Card padding and small gutters.
pub const SPACE_5: f32 = 16.0;
/// Section separation.
pub const SPACE_6: f32 = 24.0;
/// Major section separation.
pub const SPACE_7: f32 = 32.0;
/// Sparse display spacing.
pub const SPACE_8: f32 = 48.0;

/// The starting design size, §4.2; each editor measures its own default within the fit budget.
pub const REFERENCE: (f32, f32) = (1200.0, 760.0);

// Resize minima belong to each editor's measured one-card floor (§4.3), not a shared constant.

/// The height every rectangular control is drawn at, and the floor under every pointer target.
///
/// It was 32, which is a number this project chose rather than one it could cite, and at 32 every
/// button, selector, segment and slider track in the collection was a third taller than it needed
/// to be. The owner asked for the density back (2026-09-07), against a reference interface whose
/// controls sit near this size. 24×24 is WCAG 2.2 AA's Target Size (Minimum), SC 2.5.8, so the
/// floor is now a standard rather than a preference; §4.2 keeps 40×40 as the preferred target for
/// anything a performer reaches for while playing.
///
/// **28, not the 24 floor itself.** It was set to the floor exactly, and once Inter replaced the
/// toolkit's default face the type filled the controls: the owner asked for the widgets back a few
/// points rather than the type down, which is the right way round — a smaller word in a bigger box
/// is a worse trade than the four points. So the collection *draws* at 28 and §4.2's minimum stays
/// 24, which is what a floor is for.
pub const MIN_TARGET: f32 = 28.0;

/// A one-pixel border. §3.3: cards establish groups with a flat surface, a hairline and spacing,
/// never a simulated panel.
pub const HAIRLINE: f32 = 1.0;

/// Corner rounding for cards and controls. One value, so nothing drifts.
pub const RADIUS: u8 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_is_actually_a_grid() {
        // §4.1: component geometry on the 4 px grid. `space-1` is exempt by its own definition —
        // it exists for optical adjustment, which is the case the grid cannot serve.
        for (name, value) in [
            ("space-2", SPACE_2),
            ("space-3", SPACE_3),
            ("space-4", SPACE_4),
            ("space-5", SPACE_5),
            ("space-6", SPACE_6),
            ("space-7", SPACE_7),
            ("space-8", SPACE_8),
        ] {
            assert_eq!(value % 4.0, 0.0, "{name} is off the 4 px grid");
        }
    }
}
