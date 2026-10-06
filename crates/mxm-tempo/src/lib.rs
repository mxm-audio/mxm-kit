//! The collection's tempo sync: one ladder of musical divisions, and how a control's position picks
//! one.
//!
//! `plans/plan-tempo-sync-controls.md`, in the private archive. A tempo-syncable time or rate is
//! **one** continuous control with **one** adjacent on/off button (the quarter note). With the
//! button on, the control's own normalised position picks a division from its [`Ladder`], and the
//! control reads that division; the host keeps reading hertz or time. Four plugins carried their
//! own copy of this — three tables that turned out to be slices of one, and the same rounding and
//! reach arithmetic each time — so it lives here once.
//!
//! # What is here, and what is not
//!
//! Musical time only: the sixteen [`Division`]s, a control's [`Ladder`] over them, the host tempo's
//! validity ([`tempo`]), the window of divisions a control's range can hold ([`Ladder::reachable`]),
//! and a cell to publish the tempo in force to an editor ([`TempoCell`]). **Never** a plugin's
//! parameter, range, skew, smoothing or transition law (a delay's Glide, Snap or Fade): those are the
//! plugin's.
//!
//! # A stored position is a contract
//!
//! Presets and automation store the control's normalised position, and the position picks the
//! division. So [`Division::ALL`] is closed: inserting a step between the ends of a shipped [`Span`]
//! moves every stored position in it, and is a migration, not an addition.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::atomic::{AtomicU64, Ordering};

/// One musical division, in the collection's one table, shortest to longest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Division {
    /// 1/64.
    SixtyFourth,
    /// 1/32T.
    ThirtySecondTriplet,
    /// 1/32.
    ThirtySecond,
    /// 1/16T.
    SixteenthTriplet,
    /// 1/16.
    Sixteenth,
    /// 1/8T.
    EighthTriplet,
    /// 1/16. (dotted).
    SixteenthDotted,
    /// 1/8.
    Eighth,
    /// 1/4T.
    QuarterTriplet,
    /// 1/8. (dotted).
    EighthDotted,
    /// 1/4.
    Quarter,
    /// 1/4. (dotted).
    QuarterDotted,
    /// 1/2.
    Half,
    /// 1/1: a whole note, one bar of 4/4.
    Whole,
    /// Two bars of 4/4.
    TwoBars,
    /// Four bars of 4/4.
    FourBars,
}

impl Division {
    /// Every division, shortest to longest. **Closed** — see the crate's documentation.
    pub const ALL: [Self; 16] = [
        Self::SixtyFourth,
        Self::ThirtySecondTriplet,
        Self::ThirtySecond,
        Self::SixteenthTriplet,
        Self::Sixteenth,
        Self::EighthTriplet,
        Self::SixteenthDotted,
        Self::Eighth,
        Self::QuarterTriplet,
        Self::EighthDotted,
        Self::Quarter,
        Self::QuarterDotted,
        Self::Half,
        Self::Whole,
        Self::TwoBars,
        Self::FourBars,
    ];

    /// Its place in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Its length in beats (quarter notes).
    #[must_use]
    pub const fn beats(self) -> f64 {
        match self {
            Self::SixtyFourth => 0.0625,
            Self::ThirtySecondTriplet => 1.0 / 12.0,
            Self::ThirtySecond => 0.125,
            Self::SixteenthTriplet => 1.0 / 6.0,
            Self::Sixteenth => 0.25,
            Self::EighthTriplet => 1.0 / 3.0,
            Self::SixteenthDotted => 0.375,
            Self::Eighth => 0.5,
            Self::QuarterTriplet => 2.0 / 3.0,
            Self::EighthDotted => 0.75,
            Self::Quarter => 1.0,
            Self::QuarterDotted => 1.5,
            Self::Half => 2.0,
            Self::Whole => 4.0,
            Self::TwoBars => 8.0,
            Self::FourBars => 16.0,
        }
    }

    /// What a synced control reads: `1/8`, `1/4T`, `1/8.`, `2 bars`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SixtyFourth => "1/64",
            Self::ThirtySecondTriplet => "1/32T",
            Self::ThirtySecond => "1/32",
            Self::SixteenthTriplet => "1/16T",
            Self::Sixteenth => "1/16",
            Self::EighthTriplet => "1/8T",
            Self::SixteenthDotted => "1/16.",
            Self::Eighth => "1/8",
            Self::QuarterTriplet => "1/4T",
            Self::EighthDotted => "1/8.",
            Self::Quarter => "1/4",
            Self::QuarterDotted => "1/4.",
            Self::Half => "1/2",
            Self::Whole => "1/1",
            Self::TwoBars => "2 bars",
            Self::FourBars => "4 bars",
        }
    }

    /// A division from its [`label`](Self::label), trimmed and case-insensitive — what a synced
    /// control accepts as typed text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        Self::ALL
            .into_iter()
            .find(|division| division.label().eq_ignore_ascii_case(text))
    }

    /// Its length in seconds at `bpm`. A tempo is expected to have passed [`tempo`].
    #[must_use]
    pub fn seconds(self, bpm: f64) -> f64 {
        60.0 / bpm * self.beats()
    }

    /// Once per division, in hertz, at `bpm`.
    #[must_use]
    pub fn hz(self, bpm: f64) -> f64 {
        1.0 / self.seconds(bpm)
    }
}

/// A contiguous run of [`Division::ALL`]: the divisions one control offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    shortest: Division,
    longest: Division,
}

impl Span {
    /// Every LFO's span, 1/32 to four bars: the drum machine's approved pilot ladder.
    pub const LFO: Self = Self::new(Division::ThirtySecond, Division::FourBars);

    /// The divisions from `shortest` to `longest`, both included.
    ///
    /// # Panics
    ///
    /// If `shortest` is longer than `longest` — at compile time for a `const`.
    #[must_use]
    pub const fn new(shortest: Division, longest: Division) -> Self {
        assert!(
            shortest.index() <= longest.index(),
            "a span runs from its shortest division to its longest"
        );
        Self { shortest, longest }
    }

    /// Its shortest division.
    #[must_use]
    pub const fn shortest(self) -> Division {
        self.shortest
    }

    /// Its longest division.
    #[must_use]
    pub const fn longest(self) -> Division {
        self.longest
    }

    /// How many divisions it holds.
    #[must_use]
    pub const fn len(self) -> usize {
        self.longest.index() - self.shortest.index() + 1
    }

    /// Never: a span holds at least one division. Here for the `len` convention.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        false
    }

    /// Whether `division` is one of its.
    #[must_use]
    pub const fn contains(self, division: Division) -> bool {
        self.shortest.index() <= division.index() && division.index() <= self.longest.index()
    }

    /// Its divisions, shortest to longest.
    #[must_use]
    pub fn divisions(self) -> &'static [Division] {
        &Division::ALL[self.shortest.index()..=self.longest.index()]
    }
}

/// Which end of a control's travel holds the longest division.
///
/// **A synced control moves the way it moves free.** A time grows toward the top, so its longest
/// division is at the top; a rate grows toward the top, so its fastest — its shortest division — is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// A delay, a pre-delay, a clock's period: the top is the longest division.
    Time,
    /// An LFO, a clock's rate, a grain rate: the top is the fastest division.
    Rate,
}

/// A control's divisions, and which way its travel runs through them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ladder {
    /// The divisions the control offers.
    pub span: Span,
    /// Which end holds the longest.
    pub direction: Direction,
}

impl Ladder {
    /// A ladder over `span`, run in `direction`.
    #[must_use]
    pub const fn new(span: Span, direction: Direction) -> Self {
        Self { span, direction }
    }

    /// The division a control's normalised `position` picks: its travel cut into the span's steps
    /// and rounded to the nearest — the rounding every copy of this used. A position outside 0..1 is
    /// clamped; one that is not a number picks position 0's division.
    #[must_use]
    pub fn pick(self, position: f32) -> Division {
        let position = if position.is_nan() {
            0.0
        } else {
            position.clamp(0.0, 1.0)
        };
        let last = self.span.len() - 1;
        let step = (position * last as f32).round() as usize;
        self.at_step(step.min(last))
    }

    /// The position that picks `division` — the inverse of [`pick`](Self::pick), for preset designs
    /// and tests that mean a division rather than a number.
    ///
    /// # Panics
    ///
    /// If `division` is not in the ladder's span.
    #[must_use]
    pub const fn position(self, division: Division) -> f32 {
        assert!(
            self.span.contains(division),
            "the division is not on this ladder"
        );
        let last = self.span.len() - 1;
        if last == 0 {
            return 0.0;
        }
        let from_shortest = division.index() - self.span.shortest.index();
        let step = match self.direction {
            Direction::Time => from_shortest,
            Direction::Rate => last - from_shortest,
        };
        step as f32 / last as f32
    }

    /// The division at `step` from the bottom of the travel.
    fn at_step(self, step: usize) -> Division {
        let index = match self.direction {
            Direction::Time => self.span.shortest.index() + step,
            Direction::Rate => self.span.longest.index() - step,
        };
        Division::ALL[index]
    }

    /// The divisions of the span a control running from `lo` to `hi` can hold at `bpm` — `lo` and
    /// `hi` in the ladder's unit, seconds for [`Direction::Time`] and hertz for [`Direction::Rate`].
    ///
    /// A window, because the span is ordered by length and both bounds are lengths. When nothing
    /// fits — a tempo so extreme that every division falls outside the range, or a range narrow
    /// enough to fall between two — the nearest one is offered rather than none, nearest by ratio,
    /// which is how durations compare: a control that refuses to sync is worse than one that syncs to
    /// the closest thing it can hold (the delays' law, which this is).
    #[must_use]
    pub fn reachable(self, bpm: f64, lo: f64, hi: f64) -> Span {
        let (shortest_s, longest_s) = match self.direction {
            Direction::Time => (lo, hi),
            Direction::Rate => (1.0 / hi, 1.0 / lo),
        };
        let divisions = self.span.divisions();
        let seconds = |i: usize| divisions[i].seconds(bpm);
        let last = divisions.len() - 1;
        let first_fitting = (0..=last).find(|&i| seconds(i) >= shortest_s);
        let last_fitting = (0..=last).rev().find(|&i| seconds(i) <= longest_s);
        let (from, to) = match (first_fitting, last_fitting) {
            // The ordinary case.
            (Some(from), Some(to)) if from <= to => (from, to),
            // `from > to`: the range falls between two divisions, `to` shorter than it and `from`
            // longer. The nearer by ratio is offered; a tie goes to the shorter.
            (Some(from), Some(to)) => {
                let short_by = shortest_s / seconds(to);
                let long_by = seconds(from) / longest_s;
                let nearer = if short_by <= long_by { to } else { from };
                (nearer, nearer)
            }
            // Every division is longer than the range holds: the shortest is the closest.
            (Some(_), None) => (0, 0),
            // Every division is shorter than the range holds: the longest is the closest.
            (None, _) => (last, last),
        };
        Span::new(divisions[from], divisions[to])
    }

    /// The division `position` picks, **clamped** into what the control can reach at `bpm` —
    /// clamped rather than rescaled, so a position means the same division at every tempo it is
    /// reachable at, and the travel goes flat at an end instead.
    #[must_use]
    pub fn division(self, position: f32, bpm: f64, lo: f64, hi: f64) -> Division {
        let asked = self.pick(position).index();
        let window = self.reachable(bpm, lo, hi);
        Division::ALL[asked.clamp(window.shortest.index(), window.longest.index())]
    }

    /// `division`'s value in the ladder's unit at `bpm`: seconds for a time, hertz for a rate.
    #[must_use]
    pub fn value(self, division: Division, bpm: f64) -> f64 {
        match self.direction {
            Direction::Time => division.seconds(bpm),
            Direction::Rate => division.hz(bpm),
        }
    }

    /// **What a control is while synced**, in its unit, or `None` for its free value: `None` when
    /// sync is off or the host gave no usable tempo. The value is the reachable division's, and a
    /// guard clamp keeps it inside the control's range.
    #[must_use]
    pub fn resolve(
        self,
        on: bool,
        host_tempo: Option<f64>,
        position: f32,
        lo: f64,
        hi: f64,
    ) -> Option<f64> {
        if !on {
            return None;
        }
        let bpm = tempo(host_tempo)?;
        let division = self.division(position, bpm, lo, hi);
        Some(self.value(division, bpm).clamp(lo, hi))
    }

    /// The division a synced control **reads**: the one in force with a tempo, and `None` without
    /// one — the free value stands then, and the control reads that (the plan's *no tempo* rule).
    #[must_use]
    pub fn shown(
        self,
        position: f32,
        host_tempo: Option<f64>,
        lo: f64,
        hi: f64,
    ) -> Option<Division> {
        let bpm = tempo(host_tempo)?;
        Some(self.division(position, bpm, lo, hi))
    }
}

/// The host's tempo if it is one: finite and above zero. `None`, zero, negative and non-finite
/// tempos are no tempo, and a synced control falls back to its free value.
#[must_use]
pub fn tempo(host: Option<f64>) -> Option<f64> {
    host.filter(|bpm| bpm.is_finite() && *bpm > 0.0)
}

/// The tempo in force, published by the audio thread and read by the editor, so a synced control can
/// read the division actually sounding — or its free value when there is no tempo.
#[derive(Debug, Default)]
pub struct TempoCell(AtomicU64);

impl TempoCell {
    /// No tempo yet.
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Stores the host's tempo, or none: anything [`tempo`] rejects is stored as none.
    pub fn publish(&self, host: Option<f64>) {
        let bits = tempo(host).map_or(0, f64::to_bits);
        self.0.store(bits, Ordering::Relaxed);
    }

    /// The tempo last published, if there was one.
    #[must_use]
    pub fn get(&self) -> Option<f64> {
        match self.0.load(Ordering::Relaxed) {
            0 => None,
            bits => Some(f64::from_bits(bits)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Ladder = Ladder::new(
        Span::new(Division::ThirtySecond, Division::Half),
        Direction::Time,
    );
    const LFO: Ladder = Ladder::new(Span::LFO, Direction::Rate);

    #[test]
    fn the_table_rises_strictly_and_its_labels_are_unique() {
        for pair in Division::ALL.windows(2) {
            assert!(pair[0].beats() < pair[1].beats(), "{pair:?}");
        }
        for (i, a) in Division::ALL.iter().enumerate() {
            assert_eq!(a.index(), i);
            for b in &Division::ALL[i + 1..] {
                assert_ne!(a.label(), b.label());
            }
        }
    }

    /// **The three tables this replaced are contiguous slices of it**, in their own order, so every
    /// position they stored still picks the division it picked. Transcribed from the old code.
    #[test]
    fn the_legacy_tables_are_contiguous_slices() {
        let bucket = [
            "1/32", "1/16T", "1/16", "1/8T", "1/16.", "1/8", "1/4T", "1/8.", "1/4", "1/4.", "1/2",
        ];
        let fx_delay = [
            "1/64", "1/32T", "1/32", "1/16T", "1/16", "1/8T", "1/16.", "1/8", "1/4T", "1/8.",
            "1/4", "1/4.", "1/2", "1/1",
        ];
        // Knob bottom (slow) to top (fast).
        let drum = [
            "4 bars", "2 bars", "1/1", "1/2", "1/4.", "1/4", "1/8.", "1/4T", "1/8", "1/16.",
            "1/8T", "1/16", "1/16T", "1/32",
        ];
        let cases = [
            (DELAY_BUCKET, &bucket[..]),
            (
                Ladder::new(
                    Span::new(Division::SixtyFourth, Division::Whole),
                    Direction::Time,
                ),
                &fx_delay[..],
            ),
            (LFO, &drum[..]),
        ];
        for (ladder, old) in cases {
            let last = old.len() - 1;
            assert_eq!(ladder.span.len(), old.len());
            // Every old step, and a fine sweep of positions, pick what the old code picked.
            for step in 0..=1000 {
                let position = step as f32 / 1000.0;
                let old_pick = old[(position * last as f32).round() as usize];
                assert_eq!(ladder.pick(position).label(), old_pick, "{position}");
            }
        }
    }

    const DELAY_BUCKET: Ladder = Ladder::new(
        Span::new(Division::ThirtySecond, Division::Half),
        Direction::Time,
    );

    #[test]
    fn a_synced_control_moves_the_way_it_moves_free() {
        // A time's top is its longest division; a rate's top is its fastest.
        assert_eq!(DELAY.pick(0.0), Division::ThirtySecond);
        assert_eq!(DELAY.pick(1.0), Division::Half);
        assert_eq!(LFO.pick(0.0), Division::FourBars);
        assert_eq!(LFO.pick(1.0), Division::ThirtySecond);
        // And monotonically, reaching every division on the way.
        for ladder in [DELAY, LFO] {
            let mut seen = Vec::new();
            let mut previous = ladder.pick(0.0).beats();
            for step in 0..=1000 {
                let division = ladder.pick(step as f32 / 1000.0);
                let beats = division.beats();
                match ladder.direction {
                    Direction::Time => assert!(beats >= previous),
                    Direction::Rate => assert!(beats <= previous),
                }
                previous = beats;
                if !seen.contains(&division) {
                    seen.push(division);
                }
            }
            assert_eq!(seen.len(), ladder.span.len());
        }
    }

    #[test]
    fn out_of_range_and_nan_positions_are_held() {
        assert_eq!(DELAY.pick(-3.0), Division::ThirtySecond);
        assert_eq!(DELAY.pick(7.0), Division::Half);
        assert_eq!(DELAY.pick(f32::NAN), Division::ThirtySecond);
        assert_eq!(LFO.pick(f32::NAN), Division::FourBars);
    }

    #[test]
    fn a_position_picks_its_own_division() {
        const QUARTER: f32 = LFO.position(Division::Quarter);
        assert_eq!(LFO.pick(QUARTER), Division::Quarter);
        for ladder in [DELAY, LFO] {
            for &division in ladder.span.divisions() {
                assert_eq!(ladder.pick(ladder.position(division)), division);
            }
        }
    }

    #[test]
    fn divisions_are_their_beats_at_the_tempo() {
        assert!((Division::Quarter.seconds(120.0) - 0.5).abs() < 1e-12);
        assert!((Division::Quarter.hz(120.0) - 2.0).abs() < 1e-12);
        assert!((Division::SixtyFourth.seconds(0.5) - 7.5).abs() < 1e-12);
        assert!((Division::FourBars.seconds(120.0) - 8.0).abs() < 1e-12);
    }

    #[test]
    fn only_a_finite_positive_tempo_is_a_tempo() {
        assert_eq!(tempo(Some(120.0)), Some(120.0));
        for bad in [
            None,
            Some(0.0),
            Some(-1.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            assert_eq!(tempo(bad), None, "{bad:?}");
        }
    }

    /// Ported from the delays' `reachable` tests: the ordinary window, clamping without rescaling in
    /// the middle, and the nearest division when nothing fits.
    #[test]
    fn a_control_reaches_what_its_range_holds_and_clamps_the_rest() {
        // A range between two divisions offers the nearer, on either side of the gap: at 60 bpm an
        // eighth triplet is 333 ms and a dotted sixteenth 375 ms.
        let near_the_triplet = DELAY.reachable(60.0, 0.34, 0.34);
        assert_eq!(near_the_triplet.divisions(), [Division::EighthTriplet]);
        let near_the_dotted = DELAY.reachable(60.0, 0.37, 0.37);
        assert_eq!(near_the_dotted.divisions(), [Division::SixteenthDotted]);

        // A delay of 20 ms to 1 s at 120: 1/32 (62.5 ms) to 1/2 (1 s) all fit.
        let window = DELAY.reachable(120.0, 0.02, 1.0);
        assert_eq!(window, DELAY.span);
        // At 60 the half note is 2 s: the window ends at 1/4 (1 s).
        let window = DELAY.reachable(60.0, 0.02, 1.0);
        assert_eq!(window.longest(), Division::Quarter);
        // The top of the travel clamps to it; the middle is not rescaled.
        assert_eq!(DELAY.division(1.0, 60.0, 0.02, 1.0), Division::Quarter);
        assert_eq!(
            DELAY.division(DELAY.position(Division::Eighth), 60.0, 0.02, 1.0),
            Division::Eighth
        );
        // Too slow for anything: the shortest. Too fast for anything: the longest.
        assert_eq!(
            DELAY.reachable(0.1, 0.02, 1.0),
            Span::new(Division::ThirtySecond, Division::ThirtySecond)
        );
        assert_eq!(
            DELAY.reachable(1.0e6, 0.02, 1.0),
            Span::new(Division::Half, Division::Half)
        );
        // A rate's range converts: an LFO of 0.15 to 25 Hz cannot hold four bars at 120 (0.125 Hz).
        assert_eq!(
            LFO.reachable(120.0, 0.15, 25.0).longest(),
            Division::TwoBars
        );
        assert_eq!(LFO.division(0.0, 120.0, 0.15, 25.0), Division::TwoBars);
    }

    #[test]
    fn resolve_is_the_division_in_force_or_the_free_value() {
        assert_eq!(LFO.resolve(false, Some(120.0), 1.0, 0.05, 30.0), None);
        assert_eq!(LFO.resolve(true, None, 1.0, 0.05, 30.0), None);
        assert_eq!(LFO.resolve(true, Some(f64::NAN), 1.0, 0.05, 30.0), None);
        // The top of an LFO's travel is a thirty-second note: sixteen a second at 120.
        let hz = LFO.resolve(true, Some(120.0), 1.0, 0.05, 30.0).unwrap();
        assert!((hz - 16.0).abs() < 1e-9);
        // A delay's top at 120 is a half note, a second.
        let s = DELAY.resolve(true, Some(120.0), 1.0, 0.02, 1.0).unwrap();
        assert!((s - 1.0).abs() < 1e-12);
        // Shown: the division with a tempo, none without.
        assert_eq!(
            LFO.shown(1.0, Some(120.0), 0.05, 30.0),
            Some(Division::ThirtySecond)
        );
        assert_eq!(LFO.shown(1.0, None, 0.05, 30.0), None);
    }

    #[test]
    fn a_label_reads_back() {
        for division in Division::ALL {
            assert_eq!(Division::parse(division.label()), Some(division));
        }
        assert_eq!(Division::parse("  1/4t "), Some(Division::QuarterTriplet));
        assert_eq!(Division::parse("2 BARS"), Some(Division::TwoBars));
        assert_eq!(Division::parse("1/3"), None);
        assert_eq!(Division::parse("250 ms"), None);
    }

    #[test]
    fn the_cell_carries_a_tempo_or_none() {
        let cell = TempoCell::new();
        assert_eq!(cell.get(), None);
        cell.publish(Some(98.5));
        assert_eq!(cell.get(), Some(98.5));
        cell.publish(Some(f64::NAN));
        assert_eq!(cell.get(), None);
        cell.publish(Some(140.0));
        cell.publish(None);
        assert_eq!(cell.get(), None);
    }
}
