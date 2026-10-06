//! The contracts `plans/plan-modulation-routing.md` §10 requires of the routing layer.
//!
//! Every test here corresponds to a named invariant or to a defect the plan's review loop found, and
//! several exist because an earlier design failed them. Where that is so the test says which, because
//! a test whose reason is unrecorded is a test somebody deletes while tidying.

use mxm_modulation::{Compacted, SourceFrame, product, product_with_tops, sum, sum_split};

const N: usize = 6;

fn frame_with(values: [f32; N]) -> SourceFrame<N> {
    let mut frame = SourceFrame::<N>::new();
    frame.begin_sample();
    for (source, value) in values.iter().enumerate() {
        frame.write(source, *value);
    }
    frame
}

fn live(present: [bool; N]) -> Compacted<N> {
    let mut live = Compacted::<N>::new();
    live.build(&present);
    live
}

// ---------------------------------------------------------------------------
// Removing a route
// ---------------------------------------------------------------------------

/// **The owner's requirement, and the half that gets forgotten**: a source can be taken off a
/// target, and taking it off must actually silence it.
#[test]
fn an_absent_route_contributes_nothing_however_large_its_amount() {
    let frame = frame_with([1.0; N]);
    let amounts = [1.0; N];

    let all = sum(&frame, &live([true; N]), &amounts, f32::INFINITY);
    assert!(
        (all - 6.0).abs() < 1e-6,
        "six full routes should sum to six"
    );

    let none = sum(&frame, &live([false; N]), &amounts, f32::INFINITY);
    assert_eq!(
        none, 0.0,
        "an absent route must contribute nothing whatever its amount holds"
    );
}

/// Removal is **one write and reversible**: the amount is not zeroed, so re-adding the source brings
/// back the depth the player last set. Design system §8.7's undo, with no undo stack.
#[test]
fn re_adding_a_removed_route_restores_its_depth() {
    let frame = frame_with([0.5; N]);
    let amounts = [0.8, 0.0, 0.0, 0.0, 0.0, 0.0];

    let before = sum(
        &frame,
        &live([true, false, false, false, false, false]),
        &amounts,
        f32::INFINITY,
    );
    let removed = sum(&frame, &live([false; N]), &amounts, f32::INFINITY);
    let after = sum(
        &frame,
        &live([true, false, false, false, false, false]),
        &amounts,
        f32::INFINITY,
    );

    assert!(before > 0.0, "the route should sound before removal");
    assert_eq!(removed, 0.0, "and nothing while removed");
    assert_eq!(
        before, after,
        "re-adding must restore the same depth: the amount is state the removal does not touch"
    );
}

/// **A present route at zero depth is still a route.** Deriving topology from a smoothed amount was
/// rejected in review because a smoother crossing zero would drop the route mid-ramp and click.
#[test]
fn a_present_route_at_zero_depth_is_still_present() {
    let mut live = Compacted::<N>::new();
    live.build(&[true, false, false, false, false, false]);
    assert_eq!(live.len(), 1, "presence is what counts, not the amount");
    assert_eq!(live.sources(), &[0]);
}

/// Removing one route never moves another's position, which is what makes a removal one write with
/// no renumbering behind it.
#[test]
fn removal_does_not_reorder_the_survivors() {
    assert_eq!(
        live([true, true, true, false, false, false]).sources(),
        &[0, 1, 2]
    );
    assert_eq!(
        live([true, false, true, false, false, false]).sources(),
        &[0, 2],
        "dropping the middle route leaves the others where they were"
    );
}

// ---------------------------------------------------------------------------
// The unit delay
// ---------------------------------------------------------------------------

/// A forward route reads this sample; a backward route reads last sample. That is the whole of what
/// makes a player-made cycle evaluable.
#[test]
fn a_written_source_reads_current_and_an_unwritten_one_reads_previous() {
    let mut frame = SourceFrame::<N>::new();
    frame.begin_sample();
    frame.write(0, 0.25);
    assert_eq!(frame.read(0), 0.25, "written this sample");
    assert_eq!(frame.read(1), 0.0, "not yet written, so last sample");

    frame.write(1, 0.5);
    frame.begin_sample();
    frame.write(0, -0.25);
    assert_eq!(frame.read(0), -0.25);
    assert_eq!(
        frame.read(1),
        0.5,
        "last sample's value survives into this one"
    );
}

/// `reset` clears both halves. Clearing only `current` would let one render leak a sample into the
/// next, which is `mxm-mono-pr1`'s recorded lesson.
#[test]
fn reset_leaves_no_tail_in_either_half() {
    let mut frame = SourceFrame::<N>::new();
    frame.begin_sample();
    frame.write(0, 1.0);
    frame.begin_sample();
    frame.reset();
    assert_eq!(frame.read(0), 0.0);
    assert_eq!(
        frame.previous(0),
        0.0,
        "previous is state and must be cleared too"
    );
}

// ---------------------------------------------------------------------------
// Boundedness — the review finding the unit delay alone did not answer
// ---------------------------------------------------------------------------

/// **A unit delay makes a cycle well-defined, not stable.** Publication bounds every source, so a
/// product cannot grow — the finding that a multiplier chain with loop gain above one would run away
/// geometrically.
#[test]
fn publication_bounds_every_source() {
    let mut frame = SourceFrame::<N>::new();
    frame.begin_sample();
    frame.write(0, 8.0);
    frame.write(1, -8.0);
    assert_eq!(
        frame.read(0),
        1.0,
        "bounded at publication, not at the far end"
    );
    assert_eq!(frame.read(1), -1.0);
}

/// A feedback loop cannot grow, and **publication is the only thing stopping it** here.
///
/// Deliberately routed through [`sum`] with an unbounded target rather than through [`product`]: a
/// product clamps its own output, so a multiplier loop would stay bounded even if publication did
/// not — which makes it useless as evidence for the claim. An earlier version of this test did
/// exactly that and passed with the bound removed. This one goes red without it.
#[test]
fn a_feedback_loop_stays_bounded_because_publication_bounds_it() {
    let mut frame = SourceFrame::<N>::new();
    let present = live([true, true, false, false, false, false]);
    // Loop gain well above unity: without a bound at publication this diverges geometrically.
    let amounts = [3.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let mut out = 0.1f32;

    for _ in 0..10_000 {
        frame.begin_sample();
        frame.write(0, out);
        frame.write(1, 0.1);
        out = sum(&frame, &present, &amounts, f32::INFINITY);
        assert!(
            out.is_finite() && out.abs() <= 8.0,
            "the loop grew past what publication should allow: {out}"
        );
    }
}

/// A non-finite value never enters the graph, so one bad source cannot poison every route reading it.
#[test]
fn a_non_finite_source_publishes_as_zero() {
    let mut frame = SourceFrame::<N>::new();
    frame.begin_sample();
    frame.write(0, f32::NAN);
    frame.write(1, f32::INFINITY);
    assert_eq!(frame.read(0), 0.0);
    assert_eq!(frame.read(1), 0.0);
    let total = sum(&frame, &live([true; N]), &[1.0; N], f32::INFINITY);
    assert!(total.is_finite(), "and the sum stays finite");
}

/// The target's bound is the caller's, not this crate's — `mxm-mono-08` clamps its CV sum where a
/// cutoff target sums octaves.
#[test]
fn the_sum_respects_the_targets_own_bound() {
    let frame = frame_with([1.0; N]);
    let bounded = sum(&frame, &live([true; N]), &[1.0; N], 2.5);
    assert_eq!(
        bounded, 2.5,
        "six routes of one, clamped to the target's bound"
    );
}

// ---------------------------------------------------------------------------
// The multiplier's neutral
// ---------------------------------------------------------------------------

/// **Nothing present means one, not zero.** A multiplier nobody has wired changes nothing, which is
/// what makes it safe to route one before setting it up.
#[test]
fn an_unwired_multiplier_is_neutral() {
    let frame = frame_with([0.0; N]);
    assert_eq!(product(&frame, &live([false; N]), &[1.0; N]), 1.0);
}

/// A zero amount means neutral, exactly as a zero amount means nothing everywhere else. Turning a
/// factor down must not silence the product.
#[test]
fn a_zero_amount_factor_is_neutral_rather_than_silencing() {
    let frame = frame_with([0.0; N]); // a source at zero would zero a naive product
    let amounts = [0.0; N];
    assert_eq!(
        product(&frame, &live([true; N]), &amounts),
        1.0,
        "a factor at zero depth must be neutral, not annihilating"
    );
}

/// At full depth the factor is the source, which is what makes the module a multiplier at all.
#[test]
fn a_full_amount_factor_is_the_source() {
    let frame = frame_with([0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
    let amounts = [1.0; N];
    let out = product(
        &frame,
        &live([true, true, false, false, false, false]),
        &amounts,
    );
    assert!((out - 0.25).abs() < 1e-6, "0.5 * 0.5, got {out}");
}

// ---------------------------------------------------------------------------
// Silence, and independence from how the host splits blocks
// ---------------------------------------------------------------------------

/// Silence in gives exact zero out — the denormal-flush test in the collection's usual form.
#[test]
fn silence_in_is_exactly_zero_out() {
    let frame = frame_with([0.0; N]);
    assert_eq!(sum(&frame, &live([true; N]), &[1.0; N], f32::INFINITY), 0.0);
}

/// **Block-partition invariance.** Compaction happens per processing interval, and the audio must
/// not depend on where those intervals fall. This is the property that killed the plan's original
/// per-block smoother trick, so it is tested rather than argued.
#[test]
fn rendering_is_independent_of_how_the_span_is_split() {
    fn render(splits: &[usize]) -> Vec<f32> {
        let mut frame = SourceFrame::<N>::new();
        let mut live = Compacted::<N>::new();
        let amounts = [0.7, 0.3, 0.0, 0.0, 0.0, 0.0];
        let mut out = Vec::new();
        let mut n = 0usize;
        for &len in splits {
            // Once per interval, exactly as the plugin will do it.
            live.build(&[true, true, false, false, false, false]);
            for _ in 0..len {
                frame.begin_sample();
                frame.write(0, (n as f32 * 0.01).sin());
                frame.write(1, out.last().copied().unwrap_or(0.0));
                out.push(sum(&frame, &live, &amounts, f32::INFINITY));
                n += 1;
            }
        }
        out
    }

    let whole = render(&[512]);
    let split = render(&[64, 64, 1, 127, 256]);
    assert_eq!(whole.len(), split.len());
    assert_eq!(
        whole, split,
        "the same span split differently must render identically, feedback route included"
    );
}

/// Two fresh graphs render identically: nothing here carries hidden state between instances.
#[test]
fn a_fresh_graph_is_deterministic() {
    fn render() -> Vec<f32> {
        let mut frame = SourceFrame::<N>::new();
        let live = live([true, false, false, false, false, false]);
        (0..256)
            .map(|n| {
                frame.begin_sample();
                frame.write(0, (n as f32 * 0.05).sin());
                sum(&frame, &live, &[0.9; N], f32::INFINITY)
            })
            .collect()
    }
    assert_eq!(render(), render());
}

/// No NaN or inf out of a sweep of extreme amounts and sources, which is the sweep any-to-any makes
/// necessary: it reaches pairs no designer ever chose.
#[test]
fn no_pair_produces_a_non_finite_value() {
    let extremes = [-1.0f32, -0.5, 0.0, 0.5, 1.0];
    let amounts_of = [-4.0f32, -1.0, 0.0, 1.0, 4.0];
    for &v in &extremes {
        for &a in &amounts_of {
            let frame = frame_with([v; N]);
            let amounts = [a; N];
            let present = live([true; N]);
            let s = sum(&frame, &present, &amounts, 4.0);
            let p = product(&frame, &present, &amounts);
            assert!(s.is_finite(), "sum went non-finite at v={v} a={a}");
            assert!(p.is_finite(), "product went non-finite at v={v} a={a}");
            assert!(p.abs() <= 1.0, "product left its bound at v={v} a={a}: {p}");
        }
    }
}

/// **A source that starts being read must not read a value from a previous phrase.**
///
/// `mxm-mono-pr1` paid for this one. An instrument that skips publishing a source nothing reads
/// leaves that slot holding whatever it held the last time something did — so adding a *backward*
/// route to a running voice reads that stale value for exactly one sample, and how stale it is
/// depends on how long the source went unread. [`SourceFrame::clear`] at the topology transition
/// makes the first sample a deterministic zero instead.
///
/// Falsified before trusted: without the `clear`, the first read below is `0.9`, not `0.0`.
#[test]
fn clearing_a_slot_stops_a_newly_read_source_returning_an_ancient_value() {
    let mut frame: SourceFrame<2> = SourceFrame::new();

    // A long-ago phrase in which something did read this source.
    frame.begin_sample();
    frame.write(0, 0.9);
    for _ in 0..10_000 {
        frame.begin_sample();
    }

    // Nothing published it for ten thousand samples, and the slot still holds the old value.
    assert_eq!(frame.read(0), 0.9, "the slot keeps what it last held");

    // A route arrives. The transition clears the slot, so the first backward read is zero.
    frame.clear(0);
    frame.begin_sample();
    assert_eq!(frame.read(0), 0.0);
    assert_eq!(frame.previous(0), 0.0);

    // And the slot behaves normally from there.
    frame.write(0, 0.4);
    assert_eq!(frame.read(0), 0.4);
    frame.begin_sample();
    assert_eq!(frame.read(0), 0.4, "the unit delay still holds afterwards");

    // Clearing one source leaves its neighbour alone.
    frame.write(1, -0.6);
    frame.clear(0);
    assert_eq!(frame.read(1), -0.6);
}

// ---------------------------------------------------------------------------
// The split sum and the multiplier with tops (`plans/plan-modulation-standard.md`)
// ---------------------------------------------------------------------------

/// **While nothing added is live, the split sum is the machine's uniform sum to the bit** — the
/// empty case included — and an added route adds its own scaled term.
///
/// Falsified before trusted: scaling the uniform half per route instead of after its sum moves the
/// last bits of the first comparison.
#[test]
fn the_split_sum_is_the_uniform_sum_while_nothing_added_is_live() {
    let frame = frame_with([0.3, -0.7, 0.123_456, 0.9, 0.0, 0.5]);
    let amounts = [0.77, 0.31, -0.49, 1.0, 0.0, 0.25];
    let uniform = live([true, true, true, false, false, false]);
    let none = live([false; N]);
    let scale = 8.0 * 24.0;
    let split = sum_split(
        &frame,
        &uniform,
        &amounts,
        scale,
        &none,
        &[12.0 * 8.0; N],
        64.0,
    );
    assert_eq!(
        split.to_bits(),
        (sum(&frame, &uniform, &amounts, 64.0) * scale).to_bits()
    );
    assert_eq!(
        sum_split(&frame, &none, &amounts, scale, &none, &[1.0; N], 64.0).to_bits(),
        0.0f32.to_bits()
    );

    let added = live([false, false, false, true, false, false]);
    let with = sum_split(&frame, &uniform, &amounts, scale, &added, &[12.0; N], 64.0);
    assert_eq!(with, split + (1.0 * 0.9) * 12.0);
}

/// **A top of one is the old multiplier; a top of zero passes a full-velocity note through.** The
/// standard Velocity is `v − 1`, so at full velocity its factor is one whatever the amount.
#[test]
fn a_factor_is_neutral_at_its_sources_top() {
    let mut frame = SourceFrame::<N>::new();
    frame.begin_sample();
    frame.write(0, 0.5 / 8.0);
    frame.write(1, 0.0);
    let factors = live([true, true, false, false, false, false]);
    let amounts = [1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let mut tops = [1.0; N];
    tops[1] = 0.0;
    assert_eq!(
        product_with_tops(&frame, &factors, &amounts, &tops, 8.0),
        0.5,
        "a velocity at full passes the other factor through"
    );
    frame.write(1, -0.5 / 8.0);
    assert_eq!(
        product_with_tops(&frame, &factors, &amounts, &tops, 8.0),
        0.25,
        "half velocity halves it"
    );
    assert_eq!(
        product_with_tops(&frame, &live([false; N]), &amounts, &tops, 8.0),
        1.0,
        "nothing present is neutral"
    );
}
