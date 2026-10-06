use egui::{Context, Id, Pos2, RawInput, Rect, vec2};
use mxm_ui::{
    flow::Card,
    paging::{Category, Item, Key, editor},
};

fn items() -> Vec<Item<'static>> {
    (0..8)
        .map(|i| Item {
            key: Key(i),
            card: Card::new("Module", 100.0).capped(160.0),
            category: if i < 4 {
                Category::Modulators
            } else {
                Category::Generators
            },
            kind: "Module",
        })
        .collect()
}
/// A card body of a fixed size, as the tests before the tree drew one by hand.
fn body(_: &egui::Ui, _: usize) -> mxm_ui::tree::Node<u8> {
    mxm_ui::tree::leaf(
        0,
        mxm_ui::tree::Kind::Custom {
            min_width: 20.0,
            height: mxm_ui::tree::Height::Fixed(75.0),
            fills: false,
        },
    )
}

fn frame(ctx: &Context, width: f32, height: f32) -> editor::Report {
    let mut report = None;
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, height))),
            ..Default::default()
        },
        |ui| {
            report = Some(editor::show(
                ui,
                &mxm_ui::LIGHT,
                &items(),
                &[],
                false,
                &mut body,
                &mut |ui, index, _, _| {
                    ui.scope_builder(
                        egui::UiBuilder::new().id(Id::new(("global", index))),
                        |ui| {
                            let _ = ui.button(format!("Control {index}"));
                        },
                    );
                },
            ));
        },
    );
    output.textures_delta.clear();
    report.unwrap()
}
#[test]
fn cold_start_measures_hidden_cards_and_all_pages_fit_without_scroll() {
    let ctx = Context::default();
    let report = frame(&ctx, 360.0, 330.0);
    assert!(report.plan.pages.len() > 1);
    for i in 0..8 {
        editor::request_card(&ctx, Key(i));
        let report = frame(&ctx, 360.0, 330.0);
        assert!(report.visible.iter().any(|(key, _)| *key == Key(i)));
        assert!(!report.scrolling, "{report:?}");
        for (_, rect) in report.visible {
            assert!(report.viewport.expand(1.0).contains_rect(rect), "{rect:?}");
        }
    }
}
#[test]
fn large_window_merges_to_one_page_without_a_bar() {
    let ctx = Context::default();
    frame(&ctx, 360.0, 330.0);
    let report = frame(&ctx, 1600.0, 1000.0);
    assert_eq!(report.plan.pages.len(), 1);
    assert_eq!(report.plan.bar_height, 0.0);
    assert_eq!(report.visible.len(), 8);
}
#[test]
fn tiny_window_keeps_navigation_and_indivisible_content_reachable() {
    // Too narrow for two cards to a row, so every card is a scrolling page of its own: at 240
    // points two sat side by side on each scrolling page (`a_scrolling_page_takes_what_fits_beside_
    // its_tall_card`), and four tabs fitted the bar.
    let ctx = Context::default();
    let report = frame(&ctx, 200.0, 160.0);
    assert!(report.compact_navigation);
    assert!(report.scrolling);
    editor::request_card(&ctx, Key(7));
    assert_eq!(frame(&ctx, 200.0, 160.0).visible[0].0, Key(7));
}

#[test]
fn held_navigation_and_replacement_wait_and_keep_the_requested_anchor() {
    let ctx = Context::default();
    let before = frame(&ctx, 360.0, 330.0);
    editor::hold(&ctx, true);
    editor::request_card(&ctx, Key(7));
    let held = frame(&ctx, 1600.0, 1000.0);
    assert_eq!(held.plan, before.plan);
    assert_eq!(held.selected, before.selected);
    editor::hold(&ctx, false);
    let released = frame(&ctx, 360.0, 330.0);
    assert!(released.visible.iter().any(|(key, _)| *key == Key(7)));
}

#[test]
fn developer_requests_queue_through_text_ownership_and_invalid_addresses_do_not_clamp() {
    let ctx = Context::default();
    frame(&ctx, 360.0, 330.0);
    let mut view = 0;
    editor::hold(&ctx, true);
    editor::developer_request(&ctx, &mut view, Some(mxm_ui::paging::PARAMETERS));
    assert_eq!(view, 0);
    editor::hold(&ctx, false);
    editor::developer_request(&ctx, &mut view, None);
    assert_eq!(view, mxm_ui::paging::PARAMETERS);
    editor::developer_request(&ctx, &mut view, Some(126));
    assert_eq!(view, mxm_ui::paging::PARAMETERS);
    editor::developer_request(&ctx, &mut view, Some(3));
    assert_eq!(view, 0);
    let report = frame(&ctx, 360.0, 330.0);
    assert!(report.visible.iter().any(|(key, _)| *key == Key(4)));
}

/// An editor whose cards are trees: the renderer takes each card's height from its tree and draws
/// the tree, so a visible card is exactly its tree's height plus its chrome, and a card whose tree
/// grows re-plans the page at once — no revision, no hidden draw.
#[test]
fn tree_cards_are_laid_out_at_the_height_their_trees_state() {
    use mxm_ui::tree::{self, Height, Kind};
    let ctx = Context::default();
    let tall = std::cell::Cell::new(false);
    let frame = |ctx: &Context| {
        let mut report = None;
        let mut expected = Vec::new();
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                let body = |index: usize| -> tree::Node<u64> {
                    let h = if index == 2 && tall.get() {
                        400.0
                    } else {
                        60.0 + index as f32 * 10.0
                    };
                    tree::stack(vec![tree::leaf(
                        index as u64,
                        Kind::Custom {
                            min_width: 80.0,
                            height: Height::Fixed(h),
                            fills: false,
                        },
                    )])
                };
                let chrome = mxm_ui::shell::card_height(ui, "", 0.0);
                expected = (0..8).map(|i| chrome + body(i).height(ui, 80.0)).collect();
                report = Some(editor::show(
                    ui,
                    &mxm_ui::LIGHT,
                    &items(),
                    &[],
                    false,
                    &mut |_, index| body(index),
                    &mut |ui, _, _, rect| {
                        ui.allocate_space(rect.size());
                    },
                ));
            },
        );
        output.textures_delta.clear();
        (report.unwrap(), expected)
    };
    for _ in 0..3 {
        frame(&ctx);
    }
    let (report, expected) = frame(&ctx);
    // Cards in a row end level, so each is at least its own stated height and the row is its tallest.
    for (key, rect) in &report.visible {
        let own = expected[key.0 as usize];
        assert!(
            rect.height() + 0.5 >= own,
            "card {key:?} is {rect:?}, its tree says {own}"
        );
    }
    let before = report.plan.clone();
    tall.set(true);
    frame(&ctx);
    let (after, _) = frame(&ctx);
    assert!(
        after
            .visible
            .iter()
            .any(|(k, r)| k.0 == 2 && r.height() > 400.0)
            || after.plan != before,
        "a tree that grew re-planned without a revision"
    );
}

/// While an interaction holds the page nothing is re-planned, but the cards are still this frame's
/// trees: one that grows mid-gesture — its floor moving with it, so no earlier sample matches the
/// width it is drawn at — is drawn at its new height, never at a stale sample or at nothing.
#[test]
fn a_held_page_draws_its_cards_at_this_frame_s_trees() {
    use mxm_ui::tree::{self, Height, Kind};
    let ctx = Context::default();
    let grown = std::cell::Cell::new(false);
    let frame = |ctx: &Context| {
        let mut result = None;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                let big = grown.get();
                let items: Vec<_> = items()
                    .into_iter()
                    .enumerate()
                    .map(|(i, mut item)| {
                        if i == 2 && big {
                            item.card = Card::new("Module", 130.0).capped(160.0);
                        }
                        item
                    })
                    .collect();
                let body = |index: usize| -> tree::Node<u64> {
                    let h = if index == 2 && big { 300.0 } else { 60.0 };
                    tree::leaf(
                        index as u64,
                        Kind::Custom {
                            min_width: 80.0,
                            height: Height::Fixed(h),
                            fills: false,
                        },
                    )
                };
                let chrome = mxm_ui::shell::card_height(ui, "", 0.0);
                let report = editor::show(
                    ui,
                    &mxm_ui::LIGHT,
                    &items,
                    &[],
                    false,
                    &mut |_, index| body(index),
                    &mut |ui, _, _, rect| {
                        ui.allocate_space(rect.size());
                    },
                );
                result = Some((report, chrome));
            },
        );
        output.textures_delta.clear();
        result.unwrap()
    };
    for _ in 0..3 {
        frame(&ctx);
    }
    editor::hold(&ctx, true);
    let (before, _) = frame(&ctx);
    grown.set(true);
    let (held, chrome) = frame(&ctx);
    editor::hold(&ctx, false);
    assert_eq!(held.plan, before.plan, "a held page is not re-planned");
    let card = held
        .visible
        .iter()
        .find(|(key, _)| key.0 == 2)
        .expect("card 2 is on the page")
        .1;
    assert!(
        card.height() + 0.5 >= chrome + 300.0,
        "the grown card is drawn {card:?}; its tree says {}",
        chrome + 300.0
    );
    // Its row is as tall as it: nothing on the row below is drawn over.
    for (i, (a, ra)) in held.visible.iter().enumerate() {
        for (b, rb) in &held.visible[i + 1..] {
            assert!(
                !ra.shrink(0.5).intersects(rb.shrink(0.5)),
                "cards {a:?} {ra:?} and {b:?} {rb:?} overlap"
            );
        }
    }
    assert!(
        held.visible.iter().any(|(_, r)| r.top() > card.bottom()),
        "a second row is on the page, below the grown card"
    );
}
