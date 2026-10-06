//! Measurement rulers for the collection's tests and harnesses. **This crate ships nothing.**
//!
//! Every instrument and effect here is verified by measurement, because audio bugs are not compile
//! errors. Before this crate existed, none of those measurements was implemented once: "magnitude at
//! a frequency" had been written twelve times under one name for three different physical
//! quantities, and two crates asserted one-cent oscillator tuning using a ruler that quantises to
//! nine — a defect a third crate had already found and fixed, in a comment, where the fix could not
//! travel.
//!
//! # What is in here, and what may never be
//!
//! **A ruler, never a verdict.** What lives here is a *named computation*: a definition that can be
//! written down, cited, and scored against a closed form. A transform, a weighting curve, a
//! statistic, a unit conversion.
//!
//! What may never live here is a *decision*: a threshold, a tolerance, a margin, a default epsilon
//! that changes an answer, a function that returns a verdict, or a normalisation law. The two are
//! told apart by one question — **does the result change if somebody changes their mind about what
//! is acceptable?** A flatness figure does not. A "close enough" does.
//!
//! So there is no `assert_no_aliasing` here and there will not be one. A helper that decides whether
//! a number is acceptable hides the number, and hiding the number is how a threshold gets loosened
//! without anybody noticing. Thresholds are an argument about one instrument and they belong in that
//! instrument's test, beside the assertion they justify.
//!
//! Watch for the **default tolerance argument** in review: a helper that returns a number is a
//! ruler, and the same helper with a tolerance parameter is a verdict wearing a parameter's clothing.
//!
//! # Result forms
//!
//! - **A measurement over a buffer never returns NaN**, on any input including silence and at any
//!   sample rate. NaN is the failure mode that propagates silently through a comparison; ±∞ does not.
//! - **Where a non-finite sample poisons the measurement, the result is absence**, never a number —
//!   a DSP failure must not be laundered into one. That covers every accumulation (`rms`, a
//!   correlation, a transform, a pitch) **and `level::peak`, which has no exception**: a maximum is
//!   arithmetically well defined over a buffer holding a NaN, because `f32::max` returns the other
//!   operand, and that is exactly the problem — every call site asks *"is it quiet?"*, and a render
//!   that is half NaN answering **yes** is the worst failure this crate could ship.
//! - **An intentional infinity is a result.** Zero amplitude in decibels is −∞, and a function that
//!   returns it is working. Any floor is applied at the call site, where the argument for it lives.
//! - **An unavailable measurement is absent, not zero.** No pitch in silence, no onset in a signal
//!   that never starts: the result is [`Option`], and a caller that wants a number supplies the
//!   fallback along with the reason for it. A metric that returns `0.0` for *"I could not tell"* is
//!   indistinguishable from one that measured silence, and a test asserting *"below the threshold"*
//!   passes on both.
//! - **Each function declares its valid domain** — the band, the minimum buffer length, the
//!   assumption about periodicity. Used outside its domain a function must still not return NaN, and
//!   is not required to be accurate.
//!
//! # `f32` in, `f64` out
//!
//! The audio path is `f32` and measurement is accumulation over a long buffer, which is exactly the
//! case the root contract reserves `f64` for (the monorepo's; since the split
//! `docs/filters/04-efficiency.md` §4.7 states it). So these functions take what the audio path
//! produces and return what the arithmetic deserves. [`level::peak`] is the exception and returns
//! `f32`, because selecting the largest of a set of `f32`s introduces no error to lose.
//!
//! # The ruler validates itself
//!
//! Unlike `dsp-lab`, this crate has unit tests and they are not optional. A shared ruler that breaks
//! moves every number in the repository at once, silently, in the same direction — which is exactly
//! the failure a relative assertion cannot see. So every function here is scored against a signal
//! whose answer is known by construction, on [`RATES`] and on [`STRESS_RATES`].

pub mod channels;
pub mod convert;
pub mod level;
pub mod observe;
pub mod pitch;
pub mod spectrum;
pub mod stimulus;

/// The rates a measured figure is quoted at, and the set metric **accuracy** is scored on.
///
/// Taken verbatim from the twelve files that already spell this list as their own `RATES`; this
/// crate does not invent a set. A figure measured outside these is still valid, but the accuracy
/// claims in this crate's tests are made here.
pub const RATES: [f64; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];

/// The rates the crate's own **never-NaN** invariant is scored on — not a range to sweep, but the
/// endpoints and inflections the existing suites already assert at.
///
/// The widest is `mxm-grain-fx-dsp`'s validator sweep (a kilohertz to a few hundred), and
/// `mxm-shimmer-dsp` contributes the half-rate points between. Accuracy is **not** claimed here: a
/// function whose definition needs a usable audio band says so, and outside that band owes only the
/// invariant.
pub const STRESS_RATES: [f64; 9] = [
    1_000.0, 8_000.0, 22_050.0, 44_100.0, 48_000.0, 88_200.0, 96_000.0, 192_000.0, 768_000.0,
];
