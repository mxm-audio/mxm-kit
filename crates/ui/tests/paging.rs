use mxm_ui::flow::{self, Card};
use mxm_ui::paging::{
    self, Budget, Category as C, Error, Item, Key, Measurement, Outcome, Plan, State,
};

fn item(key: u64, category: C, kind: &'static str) -> Item<'static> {
    Item {
        key: Key(key),
        card: Card::new(kind, 100.0),
        category,
        kind,
    }
}
fn budget(height: f32) -> Budget {
    Budget {
        width: 100.0,
        height,
        ceiling: Some(300.0),
    }
}
fn ready(result: Result<Outcome, Error>) -> Plan {
    match result.unwrap() {
        Outcome::Ready(plan) => plan,
        other => panic!("{other:?}"),
    }
}
fn solve(items: &[Item<'_>], height: f32) -> Plan {
    ready(paging::plan(
        items,
        &[],
        budget(height),
        |_| Some(100.0),
        |_| 32.0,
    ))
}
fn keys(plan: &Plan) -> Vec<Vec<u64>> {
    plan.pages
        .iter()
        .map(|page| page.cards.iter().map(|key| key.0).collect())
        .collect()
}

#[test]
fn empty_and_single_page_have_no_bar() {
    let empty = ready(paging::plan(
        &[],
        &[],
        budget(100.0),
        |_| panic!(),
        |_| panic!(),
    ));
    assert_eq!(empty, Plan::default());
    let one = solve(&[item(7, C::Effects, "Delay")], 100.0);
    assert_eq!(keys(&one), [vec![7]]);
    assert_eq!(one.bar_height, 0.0);
}

#[test]
fn category_order_is_stable_and_every_card_occurs_exactly_once() {
    let items = [
        item(8, C::Effects, "Chorus"),
        item(2, C::Generators, "Oscillator"),
        item(4, C::Performance, "Voice"),
        item(1, C::Generators, "Oscillator"),
        item(9, C::Modulators, "Envelope"),
    ];
    for height in [140.0, 250.0, 400.0, 800.0] {
        let plan = solve(&items, height);
        assert_eq!(
            keys(&plan).into_iter().flatten().collect::<Vec<_>>(),
            [4, 9, 2, 1, 8]
        );
    }
}

#[test]
fn adjacent_whole_categories_merge_before_a_category_splits() {
    let items = [
        item(1, C::Performance, "Voice"),
        item(2, C::Modulators, "LFO"),
        item(3, C::Generators, "Oscillator"),
        item(4, C::Generators, "Oscillator"),
    ];
    let plan = solve(&items, 280.0);
    assert_eq!(keys(&plan), [vec![1, 2], vec![3, 4]]);
    assert_eq!(plan.pages[0].label, "Performance + Modulators");
    assert_eq!(plan.pages[1].label, "Generators");
}

/// One card per row, `n` rows to a page, plus the view bar the tests reserve.
fn rows(n: usize) -> f32 {
    n as f32 * 100.0 + (n - 1) as f32 * flow::GAP + 32.0
}

/// The owner's ruling on mxm-para-07, 2026-09-24: a first page holding one short category, because
/// the next category fitted a page of its own, is not the hug rule. Kept whole, these five cards are
/// three pages — Performance alone, the Modulators, Generators alone — where filling each page in
/// order makes two.
#[test]
fn a_whole_category_never_costs_a_page() {
    let items = [
        item(1, C::Performance, "Voice"),
        item(2, C::Modulators, "LFO"),
        item(3, C::Modulators, "LFO"),
        item(4, C::Modulators, "LFO"),
        item(5, C::Generators, "Oscillator"),
    ];
    let plan = solve(&items, rows(3));
    assert_eq!(keys(&plan), [vec![1, 2, 3], vec![4, 5]]);
    assert_eq!(plan.pages[0].label, "Performance + Modulators");
    assert_eq!(plan.pages[1].label, "Modulators + Generators");
}

/// Where filling saves no page, categories stay whole: the same number of pages, cut on category
/// lines.
#[test]
fn categories_stay_whole_where_filling_saves_nothing() {
    let items = [
        item(1, C::Performance, "Voice"),
        item(2, C::Performance, "Voice"),
        item(3, C::Modulators, "LFO"),
        item(4, C::Modulators, "LFO"),
    ];
    let plan = solve(&items, rows(3));
    assert_eq!(keys(&plan), [vec![1, 2], vec![3, 4]]);
    assert_eq!(plan.pages[0].label, "Performance");
    assert_eq!(plan.pages[1].label, "Modulators");
}

#[test]
fn split_categories_use_kinds_and_repeated_kinds_have_distinct_labels() {
    let items: Vec<_> = (0..4).map(|i| item(i, C::Modulators, "LFO")).collect();
    let plan = solve(&items, 250.0);
    assert_eq!(keys(&plan), [vec![0, 1], vec![2, 3]]);
    assert_eq!(
        plan.pages
            .iter()
            .map(|p| p.label.as_str())
            .collect::<Vec<_>>(),
        ["LFO 1", "LFO 2"]
    );
    let mixed = [
        item(0, C::Modulators, "LFO"),
        item(1, C::Modulators, "Envelope"),
        item(2, C::Modulators, "LFO"),
    ];
    assert_eq!(keys(&solve(&mixed, 140.0)), [vec![0], vec![1], vec![2]]);
}

#[test]
fn preferred_groups_survive_until_width_or_height_makes_them_impossible() {
    let items = [
        item(0, C::Generators, "Oscillator"),
        item(1, C::Generators, "Oscillator"),
        item(2, C::Generators, "Oscillator"),
    ];
    let groups: &[&[Key]] = &[&[Key(1), Key(2)]];
    let grouped = ready(paging::plan(
        &items,
        groups,
        budget(250.0),
        |_| Some(100.0),
        |_| 32.0,
    ));
    assert_eq!(keys(&grouped), [vec![0], vec![1, 2]]);
    let split = ready(paging::plan(
        &items,
        groups,
        budget(140.0),
        |_| Some(100.0),
        |_| 32.0,
    ));
    assert_eq!(keys(&split), [vec![0], vec![1], vec![2]]);
    assert!(split.pages.iter().all(|page| !page.overflow));
}

/// **Two cards each taller than a page share one when they sit side by side** (the owner,
/// 2026-09-28: mxm-mono-00's two oscillators had a tab each in a short window). The page scrolls
/// either way; the second card beside the first adds no row, so it costs no tab — even a little
/// taller than the first, as mono-00's Oscillator 2 is. A card that would add a row still starts a
/// page of its own.
#[test]
fn a_scrolling_page_takes_what_fits_beside_its_tall_card() {
    let items = [
        item(0, C::Generators, "Oscillator"),
        item(1, C::Generators, "Oscillator"),
        item(2, C::Tone, "Filter"),
    ];
    let mut wide = budget(140.0);
    wide.width = 210.0;
    let plan = ready(paging::plan(
        &items,
        &[],
        wide,
        |r| {
            Some(match r.key {
                Key(0) => 500.0,
                Key(1) => 520.0,
                _ => 100.0,
            })
        },
        |_| 32.0,
    ));
    // Both oscillators, 100 wide, fit one 210-wide row, 500 and 520 tall; the filter would need a
    // second row.
    assert_eq!(keys(&plan), [vec![0, 1], vec![2]]);
    assert!(plan.pages[0].overflow);
    assert!(!plan.pages[1].overflow);
}

#[test]
fn indivisible_overflow_is_explicit_not_lost_or_clipped() {
    let items = [item(0, C::Effects, "Delay"), item(1, C::Effects, "Reverb")];
    let plan = ready(paging::plan(
        &items,
        &[],
        budget(140.0),
        |r| Some(if r.key == Key(0) { 500.0 } else { 100.0 }),
        |_| 32.0,
    ));
    assert_eq!(keys(&plan), [vec![0], vec![1]]);
    assert!(plan.pages[0].overflow);
    assert!(!plan.pages[1].overflow);
    let mut narrow = budget(1000.0);
    narrow.width = 50.0;
    let plan = ready(paging::plan(
        &items[..1],
        &[],
        narrow,
        |_| Some(100.0),
        |_| panic!(),
    ));
    assert!(plan.pages[0].overflow);
    assert_eq!(plan.bar_height, 0.0);
}

#[test]
fn missing_heights_request_hidden_cards_and_candidate_widths_instead_of_assuming_fit() {
    let items = [
        item(0, C::Generators, "Oscillator"),
        item(1, C::Generators, "Oscillator"),
    ];
    let bounds = Budget {
        width: 250.0,
        ..budget(140.0)
    };
    let mut measurements: Vec<(Measurement, f32)> = Vec::new();
    let mut requests = Vec::new();
    let mut result = None;
    for _ in 0..10 {
        match paging::plan(
            &items,
            &[],
            bounds,
            |r| measurements.iter().find(|(m, _)| *m == r).map(|(_, h)| *h),
            |_| 32.0,
        )
        .unwrap()
        {
            Outcome::Pending(missing) => {
                assert!(!missing.is_empty());
                for request in missing {
                    requests.push(request);
                    // Width-sensitive real content: a narrow card takes more lines.
                    measurements.push((request, if request.width < 200.0 { 160.0 } else { 100.0 }));
                }
            }
            Outcome::Ready(plan) => {
                result = Some(plan);
                break;
            }
        }
    }
    let plan = result.expect("measurement pipeline converges");
    assert_eq!(keys(&plan), [vec![0], vec![1]]);
    assert!(requests.iter().any(|r| r.width < 200.0));
    assert!(requests.iter().any(|r| r.width == 250.0));
    for key in [Key(0), Key(1)] {
        assert!(requests.iter().any(|r| r.key == key));
    }
}

#[test]
fn bar_reservation_never_shrinks_during_a_solve_even_when_more_tabs_are_shorter() {
    let items: Vec<_> = (0..5).map(|i| item(i, C::Modulators, "LFO")).collect();
    let plan = ready(paging::plan(
        &items,
        &[],
        budget(330.0),
        |_| Some(80.0),
        |labels| {
            if labels.len() == 2 { 100.0 } else { 40.0 }
        },
    ));
    assert_eq!(keys(&plan), [vec![0, 1], vec![2, 3], vec![4]]);
    assert_eq!(plan.bar_height, 100.0);
    assert!(plan.pages.iter().all(|page| !page.overflow));
}

#[test]
fn exhausted_navigation_and_bad_measurements_are_errors_not_loops() {
    let items = [item(0, C::Tone, "Filter"), item(1, C::Effects, "Chorus")];
    assert_eq!(
        paging::plan(&items, &[], budget(140.0), |_| Some(100.0), |_| 140.0),
        Err(Error::NavigationExhausted)
    );
    assert_eq!(
        paging::plan(&items, &[], budget(140.0), |_| Some(f32::NAN), |_| 32.0),
        Err(Error::InvalidMeasurement)
    );
    assert_eq!(
        paging::plan(&items, &[], budget(140.0), |_| Some(100.0), |_| -1.0),
        Err(Error::InvalidBarHeight)
    );
    assert_eq!(
        paging::plan(
            &items,
            &[],
            budget(f32::INFINITY),
            |_| Some(100.0),
            |_| 32.0
        ),
        Err(Error::InvalidBudget)
    );
}

#[test]
fn contradictory_metadata_is_rejected_before_any_measurement() {
    let items = [
        item(0, C::Tone, "Filter"),
        item(1, C::Effects, "Chorus"),
        item(2, C::Tone, "Amplifier"),
    ];
    for group in [
        &[Key(0), Key(1)][..],
        &[Key(2), Key(0)],
        &[Key(0), Key(0)],
        &[Key(99)],
    ] {
        assert_eq!(
            paging::plan(&items, &[group], budget(100.0), |_| panic!(), |_| panic!()),
            Err(Error::InvalidGroup)
        );
    }
    assert_eq!(
        paging::plan(
            &[items[0], items[0]],
            &[],
            budget(100.0),
            |_| panic!(),
            |_| panic!()
        ),
        Err(Error::DuplicateKey)
    );
}

#[test]
fn selected_card_survives_split_merge_insertion_and_removal() {
    let items: Vec<_> = (0..4).map(|i| item(i, C::Modulators, "LFO")).collect();
    let mut state = State::default();
    state.update(Outcome::Ready(solve(&items, 250.0)), false);
    state.select(1); // anchor 2, NOT page index 1
    state.update(Outcome::Ready(solve(&items, 1000.0)), false);
    assert_eq!(state.selected_page(), 0);
    state.update(Outcome::Ready(solve(&items, 140.0)), false);
    assert_eq!(state.selected_page(), 2);
    let mut added = vec![item(9, C::Performance, "Voice")];
    added.extend(items.clone());
    state.update(Outcome::Ready(solve(&added, 140.0)), false);
    assert_eq!(state.selected_page(), 3);
    added.retain(|item| item.key != Key(2));
    state.update(Outcome::Ready(solve(&added, 140.0)), false);
    assert_eq!(state.plan.pages[state.selected_page()].cards, [Key(3)]);
}

#[test]
fn a_gesture_defers_replacement_and_incomplete_measurement_never_erases_the_page() {
    let items: Vec<_> = (0..3).map(|i| item(i, C::Modulators, "LFO")).collect();
    let mut state = State::default();
    let original = solve(&items, 1000.0);
    state.update(Outcome::Ready(original.clone()), false);
    state.update(Outcome::Ready(solve(&items, 140.0)), true);
    assert_eq!(state.plan, original);
    state.resume();
    assert_eq!(state.plan.pages.len(), 3);
    state.update(Outcome::Ready(original), true);
    state.update(Outcome::Pending(vec![]), true);
    state.resume(); // must not install the now-stale deferred plan
    assert_eq!(state.plan.pages.len(), 3);
}

#[test]
fn height_sweep_has_a_dead_band_and_splits_before_overflow() {
    let items = [item(0, C::Tone, "Filter"), item(1, C::Effects, "Chorus")];
    let mut state = State::default();
    state.update(Outcome::Ready(solve(&items, 200.0)), false);
    let mut counts = Vec::new();
    for height in [
        207.0, 208.0, 209.0, 208.0, 210.0, 224.0, 210.0, 208.0, 207.0,
    ] {
        let current_fits = if state.plan.pages.len() == 1 {
            height >= 208.0
        } else {
            height >= 132.0
        };
        // Chosen fixture margin, not a claim about a real editor's measured threshold.
        let candidate = solve(&items, height - if current_fits { 16.0 } else { 0.0 });
        state.update_with_hysteresis(Outcome::Ready(candidate), current_fits, false);
        counts.push(state.plan.pages.len());
    }
    assert_eq!(counts, [2, 2, 2, 2, 2, 1, 1, 1, 2]);
}

#[test]
fn hysteresis_cannot_swallow_inventory_changes_or_keep_a_stale_overflow_flag() {
    let mut state = State::default();
    let items = [item(0, C::Tone, "Filter"), item(1, C::Effects, "Chorus")];
    state.update(Outcome::Ready(solve(&items[..1], 90.0)), false);
    assert!(state.plan.pages[0].overflow);
    state.update_with_hysteresis(Outcome::Ready(solve(&items[..1], 140.0)), true, false);
    assert!(!state.plan.pages[0].overflow);
    state.update_with_hysteresis(Outcome::Ready(solve(&items, 140.0)), true, false);
    assert_eq!(keys(&state.plan), [vec![0], vec![1]]);
}

#[test]
fn candidate_row_widths_match_the_rendered_flow_and_readback_rejects_stale_width() {
    let cards = [
        Card::new("A", 100.0),
        Card::new("B", 150.0),
        Card::new("C", 120.0),
    ];
    for width in [258.0, 400.0, 600.0] {
        let ctx = egui::Context::default();
        let id = egui::Id::new("paging-measurement");
        let groups: &[&[usize]] = &[&[0], &[1], &[2]];
        for _ in 0..6 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    flow::cards_with(
                        ui,
                        &mxm_ui::LIGHT,
                        id,
                        &cards,
                        groups,
                        Some(250.0),
                        flow::Options {
                            scroll: flow::Scroll::None,
                            ..Default::default()
                        },
                        &mut |ui, index| {
                            ui.allocate_space(egui::vec2(10.0, 30.0 + 20.0 * index as f32));
                        },
                    );
                },
            );
            output.textures_delta.clear();
        }
        let drawn = flow::drawn(&ctx, id, cards.len());
        for row in flow::pack(&cards, groups, width) {
            let widths = flow::row_widths(&cards, &row, width, Some(250.0));
            for (index, predicted) in row.into_iter().zip(widths) {
                let rect = drawn[index].unwrap();
                assert!(
                    (rect.width() - predicted).abs() <= 1.0,
                    "{width}: {rect:?} vs {predicted}"
                );
                // The non-paged flow draws through egui_taffy, which rounds to whole points; its
                // readback is keyed by the width it drew, which `row_widths` now plans exactly.
                let natural = flow::natural_height(&ctx, id, index, rect.width())
                    .expect("measured at this width");
                assert!(natural <= rect.height() + 0.01);
                if width == 600.0 && index == 0 {
                    assert!(
                        natural + 30.0 < rect.height(),
                        "row stretch must not feed into paging height"
                    );
                }
                assert!(flow::natural_height(&ctx, id, index, rect.width() + 10.0).is_none());
                flow::invalidate_measurements(&ctx, id, index + 1);
                assert!(flow::natural_height(&ctx, id, index, predicted).is_none());
                assert_eq!(
                    flow::drawn(&ctx, id, cards.len()),
                    drawn,
                    "invalidation must not change visible identity/geometry"
                );
            }
        }
    }
}

/// Computed floors are fractional. A row of them that fills its budget sums to it only up to float
/// rounding, and read as not fitting it put every card on a page of its own at a height that holds
/// them all — mxm-mono-00's Voice alone on a first page, mxm-mono-03 one card a page.
#[test]
fn fractional_floors_that_fill_the_width_stay_on_one_page() {
    for floor in [299.0625_f32, 304.6875, 312.3333, 1.0 / 3.0 * 900.0] {
        let items: Vec<_> = (0..3)
            .map(|i| Item {
                key: Key(i),
                card: Card::new("Card", floor),
                category: C::Tone,
                kind: "Card",
            })
            .collect();
        for width in [
            3.0 * floor + 2.0 * flow::GAP,
            3.0 * floor + 2.0 * flow::GAP + 0.3,
        ] {
            let plan = ready(paging::plan(
                &items,
                &[],
                Budget {
                    width,
                    height: 20_000.0,
                    ceiling: None,
                },
                |_| Some(100.0),
                |_| 32.0,
            ));
            assert_eq!(keys(&plan), vec![vec![0, 1, 2]], "floor {floor} at {width}");
        }
    }
}
