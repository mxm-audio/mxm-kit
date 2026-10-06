//! The collection's modulation routing, as the part that is genuinely shared.
//!
//! `plans/plan-modulation-routing.md` §4.1, in the private archive. Four things live here and **no
//! instrument's voice**: the source frame and its unit delay, route compaction, the combination
//! laws, and the boundedness contract that keeps a player-made feedback loop finite.
//!
//! # What a route is
//!
//! A route is a **(target, source) pair** carrying a *presence* and an *amount*. There is no source
//! selector: a target has one slot per source, so a slot *is* a source and there is nothing to
//! choose. Two consequences this crate exists to enforce:
//!
//! - **Presence is what the DSP reads.** An absent pair contributes nothing *whatever its amount
//!   holds*, which is what makes removing a route one parameter write, and what makes re-adding it
//!   restore the depth the player last set. [`Compacted::build`] is the only route to an amount and
//!   it admits present pairs only.
//! - **An amount of zero is not absence.** A present route at zero depth is a route and stays one.
//!   Deriving topology from a smoothed amount would drop a route mid-ramp and click.
//!
//! # What is not here
//!
//! Every source's generation, every target's application, each instrument's evaluation order, its
//! unit scale, and the detectors that are not plain CV — `mxm-mono-00`'s audio-row crossfade, its
//! gate threshold and its sync edge stay with it until a second instrument needs the same one.
//!
//! The seam: **this crate owns *when a value is readable* and *which routes are live*; the
//! instrument owns *what the value means* and *how it is applied*.**

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "conformance")]
pub mod conformance;
pub mod standard;

/// A sample's worth of source values, with the unit delay that makes a cycle finite.
///
/// Written three times in this repository before it was written once here — `mxm-mono-00`'s
/// `Columns`, `mxm-mono-08`'s `CvFrame`, and `mxm-mono-pr1`'s one-field degenerate case. The rule
/// is the same in all three: **read this sample's value if this sample has already produced it,
/// otherwise last sample's.**
///
/// That is what lets a player route a target back to something that reads it. A *forward* route —
/// one whose source comes earlier in the instrument's declared order — has no delay at all; a
/// *backward* route is one sample late, which at audio rate is a comb and is a deviation each
/// instrument declares for itself.
///
/// # Scope
///
/// One frame per scope. A monophonic instrument has one; a polyphonic instrument has a global frame
/// and one per voice, because with six keys down there is no single "the key". A voice frame ceases
/// with its voice, which is what stops a latched note source outliving the note that set it.
/// # Opening a sample is O(1), not O(sources)
///
/// **An unwritten source's `current` slot already holds last sample's value**, because nothing has
/// overwritten it yet — so the unit-delay read is `current[source]` with no branch and no
/// per-sample bookkeeping at all. What the frame still owes is the *explicit* `previous`, and that
/// is saved lazily by the first `write` of each sample rather than by copying the whole frame.
///
/// So a frame with a thousand sources and two routed does two sources' work, which is what the
/// compacted sum below has always done and what this half of the crate did not. The stamp is what
/// replaced clearing an N-slot array every sample: `u64` at 48 kHz runs out in about twelve
/// million years, so it never wraps, and initialising the stamps to `u64::MAX` means a write
/// before the first `begin_sample` cannot look like one that already happened.
#[derive(Debug, Clone)]
pub struct SourceFrame<const N: usize> {
    current: [f32; N],
    /// Only meaningful for a source whose `stamp` is this sample's; otherwise `current` is
    /// already last sample's value.
    previous: [f32; N],
    /// The sample each source was last written in.
    stamp: [u64; N],
    /// Counts samples. Never reset to a value a stale stamp could equal.
    sample: u64,
}

impl<const N: usize> Default for SourceFrame<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> SourceFrame<N> {
    /// A frame with every source at zero and nothing written.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: [0.0; N],
            previous: [0.0; N],
            stamp: [u64::MAX; N],
            sample: 0,
        }
    }

    /// Opens a sample: this sample's values become last sample's, and nothing is written yet.
    ///
    /// **One increment**, whatever `N` is. It used to copy `current` into `previous` and clear an
    /// N-slot written array, which is work proportional to how many sources an instrument *has*
    /// rather than to how many are routed — the one place in this crate that was.
    #[inline]
    pub fn begin_sample(&mut self) {
        self.sample = self.sample.wrapping_add(1);
    }

    /// Publishes a source's value for this sample, **bounded to unit magnitude**.
    ///
    /// The bound is not decoration and not the caller's option: it is what stops a multiplier cycle
    /// growing. A product of values bounded by one is bounded by one, so no loop through a
    /// [`product`] can run away — where a unit delay alone would only make the runaway *well
    /// defined*. A module working in a wider domain scales into this one before publishing, which
    /// is what `mxm-mono-00`'s ten-volt columns already do and what `mxm-mono-pr1`'s clamped bus
    /// exists to do.
    ///
    /// A non-finite value publishes as zero rather than poisoning every route that reads it.
    #[inline]
    pub fn write(&mut self, source: usize, value: f32) {
        let bounded = if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        // The first write of a sample is what saves last sample's value, so nothing is copied for a
        // source nobody publishes.
        if self.stamp[source] != self.sample {
            self.previous[source] = self.current[source];
            self.stamp[source] = self.sample;
        }
        self.current[source] = bounded;
    }

    /// This sample's value if it has been published, otherwise last sample's — the unit delay.
    ///
    /// **One load, no branch.** An unwritten source's slot still holds last sample's value, so the
    /// two cases the doc comment names are the same read; the branch that used to choose between
    /// them was answering a question whose answers were identical.
    #[inline]
    #[must_use]
    pub fn read(&self, source: usize) -> f32 {
        self.current[source]
    }

    /// Last sample's value, whatever this sample has done.
    #[inline]
    #[must_use]
    pub fn previous(&self, source: usize) -> f32 {
        if self.stamp[source] == self.sample {
            self.previous[source]
        } else {
            self.current[source]
        }
    }

    /// Whether this sample has published that source yet.
    #[inline]
    #[must_use]
    pub fn written(&self, source: usize) -> bool {
        self.stamp[source] == self.sample
    }

    /// Clears **one** source's history, as if it had never been written.
    ///
    /// For an instrument that skips publishing a source nothing reads. While a source goes unread
    /// its slot keeps whatever it held the last time something did read it — so the moment a route
    /// is added, a *backward* route from that source reads a value that may be from a different
    /// phrase entirely, for exactly one sample. Clearing the slot as the route arrives makes that
    /// first sample a deterministic zero instead.
    ///
    /// **It is the topology transition that owes this, not the frame.** A caller that publishes
    /// every source unconditionally never needs it; one that gates publication on *is anything
    /// reading this* does, and the cost of the gate is this call at the interval boundary.
    pub fn clear(&mut self, source: usize) {
        self.current[source] = 0.0;
        self.previous[source] = 0.0;
        self.stamp[source] = u64::MAX;
    }

    /// Clears both halves, leaving no tail.
    ///
    /// `previous` is state, so a reset clearing only `current` would let one render leak a sample
    /// into the next. `mxm-mono-pr1`'s `reset_removes_the_delayed_backward_source` is the test that
    /// says so.
    pub fn reset(&mut self) {
        self.current = [0.0; N];
        self.previous = [0.0; N];
        // Back to "nothing has ever been written", which `u64::MAX` says and no sample count can
        // reach. Setting the stamps to zero and the counter to zero would say the opposite.
        self.stamp = [u64::MAX; N];
        self.sample = 0;
    }
}

/// The live routes into one target, in source order, rebuilt when topology changes.
///
/// **Topology is discrete and changes only on a parameter event**, so this is built once per
/// processing interval rather than per sample — which is the whole of the efficiency design. The
/// per-sample loop then runs over the *present* routes rather than over every source the instrument
/// declares.
///
/// Nothing is ever reordered: a route's position is its source's, so removing one never moves
/// another. That is what makes a removal one parameter write with no renumbering behind it.
#[derive(Debug, Clone)]
pub struct Compacted<const N: usize> {
    sources: [usize; N],
    len: usize,
}

impl<const N: usize> Default for Compacted<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Compacted<N> {
    /// An empty list — a target with no routes.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sources: [0; N],
            len: 0,
        }
    }

    /// Rebuilds from a target's presences. Allocation-free, so it is safe in `process()`.
    ///
    /// **Presence alone decides**, never an amount: a present route at zero depth is still a route,
    /// and an absent one contributes nothing whatever its amount holds.
    pub fn build(&mut self, present: &[bool; N]) {
        self.len = 0;
        for (source, &live) in present.iter().enumerate() {
            if live {
                self.sources[self.len] = source;
                self.len += 1;
            }
        }
    }

    /// The present sources, in source order.
    #[inline]
    #[must_use]
    pub fn sources(&self) -> &[usize] {
        &self.sources[..self.len]
    }

    /// How many routes are live into this target.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the target has no routes at all.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Sums a target's live routes, in the target's own domain, bounded by the caller.
///
/// `amounts` is indexed by source, so an absent pair's amount is never read — which is what
/// *removing a route* means at this layer. The `bound` is the target's, not this crate's:
/// `mxm-mono-08` clamps its CV sum, `mxm-mono-pr1` clamped its bus, and a cutoff target sums
/// octaves. [`f32::INFINITY`] is legal and means the caller bounds it elsewhere.
///
/// Every route is at unit scale here; [`sum_scaled`] is the form that carries one.
#[inline]
#[must_use]
pub fn sum<const N: usize>(
    frame: &SourceFrame<N>,
    live: &Compacted<N>,
    amounts: &[f32; N],
    bound: f32,
) -> f32 {
    sum_scaled(frame, live, amounts, &[1.0; N], bound)
}

/// [`sum`], with a **per-route full scale** — the reach an amount of one buys on that pair.
///
/// # Why the scale is per route and applied last
///
/// **Per route** because a machine's own wiring is not uniform: `mxm-mono-01`'s filter envelope
/// reaches six octaves and its filter LFO four, from the same cutoff. `plans/plan-modulation-routing.md`
/// §5 takes the conservative form — a route the instrument itself wires keeps the scale it always
/// had — so no stored value changes meaning and no shipped sound moves. A route a *player* adds
/// takes the target's declared scale, which is that same table's default column.
///
/// **Applied last**, as a third multiply rather than folded into the amount, because
/// `(amount × source) × scale` and `(amount × scale) × source` do not round alike, and the first is
/// literally the instruction sequence these voices executed before they had routing. §6.2 puts it
/// plainly: folding it would move every pinned golden digest for one instruction per route. An
/// earlier revision of this crate folded it, and the digests moved.
#[inline]
#[must_use]
pub fn sum_scaled<const N: usize>(
    frame: &SourceFrame<N>,
    live: &Compacted<N>,
    amounts: &[f32; N],
    scales: &[f32; N],
    bound: f32,
) -> f32 {
    let mut total = 0.0;
    for &source in live.sources() {
        total += (amounts[source] * frame.read(source)) * scales[source];
    }
    if total.is_finite() {
        total.clamp(-bound, bound)
    } else {
        0.0
    }
}

/// A target whose **machine routes share one scale applied after their sum**, and whose routes a
/// player adds take their own — `plans/plan-modulation-standard.md`'s split sum (in the private
/// archive).
///
/// A machine that summed its sources on a bus and scaled the bus once (`mxm-mono-pr1`, `mxm-para-07`,
/// `mxm-mono-00`) keeps that instruction sequence exactly — `sum(uniform) × uniform_scale` — for the
/// paths it had, which is what its pinned digests hold. A path it never had takes the standard reach
/// through `added_scales`, per route and applied last as in [`sum_scaled`]. **While nothing added is
/// live the result is the uniform sum to the bit**, the empty case included; each half is bounded by
/// `bound` in its own domain.
#[inline]
#[must_use]
pub fn sum_split<const N: usize>(
    frame: &SourceFrame<N>,
    uniform: &Compacted<N>,
    amounts: &[f32; N],
    uniform_scale: f32,
    added: &Compacted<N>,
    added_scales: &[f32; N],
    bound: f32,
) -> f32 {
    let machine = sum(frame, uniform, amounts, bound) * uniform_scale;
    if added.is_empty() {
        return machine;
    }
    let extra = sum_scaled(frame, added, amounts, added_scales, bound);
    if uniform.is_empty() {
        extra
    } else {
        machine + extra
    }
}

/// A multiplier module in a **scaled frame**, each factor neutral at its source's **top**:
/// `Π (1 + amount × (read × unscale − top))`.
///
/// `unscale` undoes the frame unit before the law, so the `1` a factor blends toward is one *raw*
/// unit (`crates/mxm-modulation/AGENTS.md`, *`product` is not scale-invariant*). The **top** is the
/// value at which a source leaves the product alone at full amount: one for a unipolar source, and
/// **zero for the standard Velocity**, which is `v − 1` — so a full-velocity note passes the other
/// factors through, as the raw velocity at one did. For a top of one this is exactly the law
/// `mxm-mono-pr1` and `mxm-para-07` carried as their own.
///
/// **Not bounded here**: the caller publishes the result back through [`SourceFrame::write`], and that
/// bound is what keeps a cycle through the multiplier finite.
#[inline]
#[must_use]
pub fn product_with_tops<const N: usize>(
    frame: &SourceFrame<N>,
    live: &Compacted<N>,
    amounts: &[f32; N],
    tops: &[f32; N],
    unscale: f32,
) -> f32 {
    let mut total = 1.0f32;
    for &source in live.sources() {
        let raw = frame.read(source) * unscale;
        total *= 1.0 + amounts[source] * (raw - tops[source]);
    }
    total
}

/// Multiplies a target's live routes, **with one as the neutral element**.
///
/// This is the multiplier module: a target whose law is product and whose result is published as a
/// source, which is how one source scales another — *the LFO through the mod wheel* — without a
/// per-route modifier. Two properties it must have, and this is where they live:
///
/// - **Nothing present means one, not zero.** A multiplier nobody has wired changes nothing.
/// - **An amount blends its factor between neutral and the source**, so a zero amount means neutral
///   exactly as a zero amount means nothing everywhere else. Turning a factor down must not silence
///   the product.
///
/// Every input is already bounded to unit magnitude by [`SourceFrame::write`], so the product is
/// too, and a cycle through a multiplier cannot grow.
#[inline]
#[must_use]
pub fn product<const N: usize>(
    frame: &SourceFrame<N>,
    live: &Compacted<N>,
    amounts: &[f32; N],
) -> f32 {
    let mut total = 1.0f32;
    for &source in live.sources() {
        let factor = 1.0 + amounts[source] * (frame.read(source) - 1.0);
        total *= factor;
    }
    if total.is_finite() {
        total.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}
