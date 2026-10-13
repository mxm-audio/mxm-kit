//! What every plugin's DSP→editor channel shares: the peak meter with its clip latch, and the
//! developer channel's request slots.
//!
//! A plugin's `telemetry.rs` is the **only** channel from its audio thread to its editor — atomics,
//! written once per block, read whenever the editor looks, never a lock the audio thread could wait
//! on (`docs/plugin-conventions.md`). Most of each one is the plugin's own: an envelope, a modulator,
//! stage levels. These two parts were the same thing written again in every plugin, and the copies
//! had drifted — five policies for what a non-finite peak does to one clip light — so each plugin's
//! `Telemetry` holds them as fields instead (the architecture audit's O10).
//!
//! Zero dependencies and MSRV 1.87, so taking it raises nobody's floor.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

/// A peak level and its clip latch, written by the audio thread and read by the editor.
///
/// **One policy, for every plugin:**
///
/// - **A finite peak is max-combined by magnitude, and reading resets it.** Overwriting each block
///   would lose a transient that landed between two frames; combining means the level is always
///   *the loudest since you last looked*.
/// - **A peak at or above full scale latches the clip**, and **only the user clears it** — a meter
///   that quietly forgets it clipped is worse than no meter (design system §5.4).
/// - **A non-finite peak latches the clip and is never stored as the level.** A broken render shows
///   as clipped, never as a number: `+inf` as a level would pin the meter, and NaN would poison
///   every comparison after it.
#[derive(Debug)]
pub struct PeakMeter {
    /// The loudest finite magnitude since the editor last read it, as `f32` bits.
    level: AtomicU32,
    /// Latched on clip or on a non-finite peak; cleared only by [`PeakMeter::clear_clip`].
    clipped: AtomicBool,
}

impl Default for PeakMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl PeakMeter {
    /// Silent and unclipped.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            level: AtomicU32::new(0),
            clipped: AtomicBool::new(false),
        }
    }

    // ---- audio thread ----

    /// Publishes a block's peak: max-combined by magnitude, the clip latched at full scale, and a
    /// non-finite peak latching the clip without touching the level.
    pub fn publish(&self, peak: f32) {
        if !peak.is_finite() {
            self.clipped.store(true, Ordering::Relaxed);
            return;
        }
        let magnitude = peak.abs();
        if magnitude >= 1.0 {
            self.clipped.store(true, Ordering::Relaxed);
        }
        let mut current = self.level.load(Ordering::Relaxed);
        while f32::from_bits(current) < magnitude {
            match self.level.compare_exchange_weak(
                current,
                magnitude.to_bits(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(seen) => current = seen,
            }
        }
    }

    /// Latches the clip for a reason the peak does not show — a DSP that reports clipping itself
    /// (`mxm-fx-curve`'s stages). The meter's own rule still applies to every published peak.
    pub fn latch_clip(&self) {
        self.clipped.store(true, Ordering::Relaxed);
    }

    // ---- editor thread ----

    /// The loudest finite magnitude since this was last called, **and resets** it.
    pub fn take(&self) -> f32 {
        f32::from_bits(self.level.swap(0, Ordering::Relaxed))
    }

    /// Whether the clip is latched.
    pub fn clipped(&self) -> bool {
        self.clipped.load(Ordering::Relaxed)
    }

    /// Clears the clip: the user's act, never a timeout.
    pub fn clear_clip(&self) {
        self.clipped.store(false, Ordering::Relaxed);
    }
}

/// Nothing requested on a slot.
const NO_REQUEST: u8 = u8::MAX;

/// The developer channel's requests of the editor (`docs/plugin-conventions.md`, *A developer
/// channel in every editor*): a view to show, the expander open or closed, the preset browser open
/// or closed, and a theme. The plugin's MIDI handler stores each; the editor takes each **once**, at
/// the top of its panel. The DSP reads none of them.
///
/// A view or a theme is clamped below the no-request sentinel, so no value can be lost for a
/// request; a CC value never reaches it anyway.
#[derive(Debug)]
pub struct DevRequests {
    view: AtomicU8,
    disclosure: AtomicU8,
    browser: AtomicU8,
    theme: AtomicU8,
}

impl Default for DevRequests {
    fn default() -> Self {
        Self::new()
    }
}

impl DevRequests {
    /// Nothing requested.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            view: AtomicU8::new(NO_REQUEST),
            disclosure: AtomicU8::new(NO_REQUEST),
            browser: AtomicU8::new(NO_REQUEST),
            theme: AtomicU8::new(NO_REQUEST),
        }
    }

    /// Asks for a view: a developer category address, or the Parameters address, never a derived
    /// tab index.
    pub fn request_view(&self, view: u8) {
        self.view.store(view.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    /// The view asked for since the editor last looked, if any.
    pub fn take_view(&self) -> Option<usize> {
        take(&self.view).map(usize::from)
    }

    /// Asks for the expander open or closed.
    pub fn request_disclosure(&self, open: bool) {
        self.disclosure.store(u8::from(open), Ordering::Relaxed);
    }

    /// Whether the expander was asked open or closed since the editor last looked, if it was.
    pub fn take_disclosure(&self) -> Option<bool> {
        take(&self.disclosure).map(|open| open != 0)
    }

    /// Asks for the preset browser open or closed.
    pub fn request_browser(&self, open: bool) {
        self.browser.store(u8::from(open), Ordering::Relaxed);
    }

    /// Whether the browser was asked open or closed since the editor last looked, if it was.
    pub fn take_browser(&self) -> Option<bool> {
        take(&self.browser).map(|open| open != 0)
    }

    /// Asks for a theme by index, as the editor's theme control reads it.
    pub fn request_theme(&self, theme: u8) {
        self.theme
            .store(theme.min(NO_REQUEST - 1), Ordering::Relaxed);
    }

    /// The theme asked for since the editor last looked, if any.
    pub fn take_theme(&self) -> Option<u8> {
        take(&self.theme)
    }
}

/// A slot's request, once: the slot is reset to nothing as it is read.
fn take(slot: &AtomicU8) -> Option<u8> {
    match slot.swap(NO_REQUEST, Ordering::Relaxed) {
        NO_REQUEST => None,
        value => Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peak_is_combined_not_overwritten_and_reset_on_read() {
        let meter = PeakMeter::new();
        meter.publish(0.4);
        meter.publish(0.9);
        meter.publish(0.2);
        assert_eq!(meter.take(), 0.9, "the loudest of the three, not the last");
        assert_eq!(meter.take(), 0.0, "and reading resets it");
    }

    #[test]
    fn a_negative_peak_counts_by_its_magnitude() {
        let meter = PeakMeter::new();
        meter.publish(-0.7);
        assert_eq!(meter.take(), 0.7);
        meter.publish(-1.0);
        assert!(meter.clipped(), "full scale either way");
    }

    #[test]
    fn a_clip_latches_through_quiet_blocks_until_cleared() {
        let meter = PeakMeter::new();
        meter.publish(1.0);
        for _ in 0..100 {
            meter.publish(0.1);
        }
        assert!(
            meter.clipped(),
            "a meter that forgets is worse than no meter"
        );
        let _ = meter.take();
        assert!(meter.clipped(), "reading the level does not clear the clip");
        meter.clear_clip();
        assert!(!meter.clipped());
    }

    #[test]
    fn a_peak_just_below_full_scale_does_not_clip() {
        let meter = PeakMeter::new();
        meter.publish(1.0_f32.next_down());
        assert!(!meter.clipped());
    }

    /// **A non-finite peak latches the clip and leaves the level as it was** — as the first thing
    /// published, and after a finite peak.
    #[test]
    fn a_non_finite_peak_latches_the_clip_and_is_never_the_level() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let first = PeakMeter::new();
            first.publish(bad);
            assert!(first.clipped(), "{bad} first latches");
            assert_eq!(first.take(), 0.0, "{bad} first is not a level");

            let after = PeakMeter::new();
            after.publish(0.5);
            after.publish(bad);
            assert!(after.clipped(), "{bad} after a level latches");
            assert_eq!(after.take(), 0.5, "{bad} leaves the level as it was");

            let later = PeakMeter::new();
            later.publish(bad);
            later.publish(0.3);
            assert_eq!(
                later.take(),
                0.3,
                "a finite peak after {bad} is still combined"
            );
        }
    }

    #[test]
    fn a_reported_clip_latches_without_a_peak() {
        let meter = PeakMeter::new();
        meter.latch_clip();
        assert!(meter.clipped());
        assert_eq!(meter.take(), 0.0);
    }

    #[test]
    fn each_request_is_taken_once() {
        let dev = DevRequests::new();
        assert_eq!(dev.take_view(), None, "nothing is asked of a fresh channel");
        dev.request_view(3);
        dev.request_disclosure(true);
        dev.request_browser(false);
        dev.request_theme(1);
        assert_eq!(dev.take_view(), Some(3));
        assert_eq!(dev.take_disclosure(), Some(true));
        assert_eq!(dev.take_browser(), Some(false));
        assert_eq!(dev.take_theme(), Some(1));
        assert_eq!(
            (
                dev.take_view(),
                dev.take_disclosure(),
                dev.take_browser(),
                dev.take_theme()
            ),
            (None, None, None, None),
            "each was taken once"
        );
    }

    #[test]
    fn the_last_request_before_a_look_wins() {
        let dev = DevRequests::new();
        dev.request_view(1);
        dev.request_view(127);
        assert_eq!(dev.take_view(), Some(127));
    }

    #[test]
    fn a_view_or_theme_at_the_sentinel_is_clamped_below_it_rather_than_lost() {
        let dev = DevRequests::new();
        dev.request_view(u8::MAX);
        assert_eq!(dev.take_view(), Some(usize::from(u8::MAX - 1)));
        dev.request_theme(u8::MAX);
        assert_eq!(dev.take_theme(), Some(u8::MAX - 1));
    }
}
