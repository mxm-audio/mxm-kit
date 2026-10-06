//! Regression tests for the pager review at e644c96.
//! The first three assertions failed before the repairs; see plans/plan-dynamic-view-paging.md §11.
use egui::{Context, Event, Pos2, RawInput, Rect, vec2};
use mxm_ui::{
    flow::Card,
    paging::{Category, Item, Key, editor},
};

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

fn frame(
    ctx: &Context,
    size: egui::Vec2,
    items: &[Item<'_>],
    events: Vec<Event>,
) -> editor::Report {
    let mut report = None;
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| {
            report = Some(editor::show(
                ui,
                &mxm_ui::LIGHT,
                items,
                &[],
                false,
                &mut body,
                &mut |ui, _, _, rect| {
                    ui.allocate_space(rect.size());
                },
            ));
        },
    );
    // There is no renderer here to upload the font atlas to, and `epaint` panics if a
    // `TexturesDelta` is dropped with work still in it. Discarding the output silently was fine
    // until a frame actually rasterised a new glyph, and then it failed in a way that named epaint
    // rather than this harness. The plugin editors' own headless checks clear it the same way.
    output.textures_delta.clear();
    report.unwrap()
}

#[test]
fn overflowing_card_can_actually_scroll_to_its_right_edge() {
    let ctx = Context::default();
    let items = [Item {
        key: Key(0),
        card: Card::new("Wide", 400.0),
        category: Category::Tone,
        kind: "Filter",
    }];
    let size = vec2(200.0, 300.0);
    for _ in 0..4 {
        frame(&ctx, size, &items, vec![]);
    }
    let before = frame(&ctx, size, &items, vec![]);
    assert!(before.scrolling);
    for _ in 0..30 {
        frame(
            &ctx,
            size,
            &items,
            vec![
                Event::PointerMoved(egui::pos2(100.0, 80.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    delta: vec2(-100.0, 0.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    let after = frame(&ctx, size, &items, vec![]);
    assert!(
        after.visible[0].1.right() <= after.viewport.right() + 1.0,
        "right edge cannot be reached: before={before:?}, after={after:?}"
    );
}

#[test]
fn compact_navigation_next_stays_inside_a_zoomed_minimum_workspace() {
    use egui_kittest::kittest::Queryable;
    let items: Vec<_> = (0..8)
        .map(|i| Item {
            key: Key(i),
            card: Card::new("Module", 100.0).capped(160.0),
            category: Category::Tone,
            kind: "Module",
        })
        .collect();
    let workspace = std::cell::Cell::new(Rect::NOTHING);
    let mut harness = egui_kittest::Harness::builder()
        .with_size(vec2(184.0, 144.0))
        .build_ui(|ui| {
            mxm_ui::typography::apply(ui.ctx());
            mxm_ui::theme::apply(ui.ctx());
            workspace.set(ui.available_rect_before_wrap());
            editor::show(
                ui,
                &mxm_ui::LIGHT,
                &items,
                &[],
                false,
                &mut body,
                &mut |ui, _, _, rect| {
                    ui.allocate_space(rect.size());
                },
            );
        });
    harness.run();
    let next = harness.get_by_label("Next").rect();
    let viewport = workspace.get();
    assert_eq!(viewport.width(), 168.0); // 400 physical / 2x zoom, less 32 points of gutters.
    assert!(
        next.left() >= viewport.left() && next.right() <= viewport.right(),
        "Next outside workspace {viewport:?}: {next:?}"
    );
    assert!(
        next.height() >= 24.0,
        "Compact navigation violates the 24-point pointer floor: {next:?}"
    );
}

#[test]
fn compact_navigation_wraps_with_stable_targets_and_pointer_keyboard_activation() {
    use egui_kittest::kittest::Queryable;
    for dark in [false, true] {
        for width in [152.0, 168.0, 240.0, 360.0] {
            // Wider than half of any width here, so no two share a row: twelve scrolling pages.
            // At 100 points two or three sat side by side on each scrolling page
            // (`a_scrolling_page_takes_what_fits_beside_its_tall_card`), and there were fewer.
            let items: Vec<_> = (0..12)
                .map(|i| Item {
                    key: Key(i),
                    card: Card::new("Module", 180.0).capped(180.0),
                    category: Category::Tone,
                    kind: "Module",
                })
                .collect();
            let workspace = std::cell::Cell::new(Rect::NOTHING);
            let mut harness = egui_kittest::Harness::builder()
                .with_size(vec2(width + 16.0, 144.0))
                .build_ui(|ui| {
                    mxm_ui::typography::apply(ui.ctx());
                    mxm_ui::theme::apply(ui.ctx());
                    ui.ctx().set_theme(if dark {
                        egui::ThemePreference::Dark
                    } else {
                        egui::ThemePreference::Light
                    });
                    workspace.set(ui.available_rect_before_wrap());
                    editor::show(
                        ui,
                        if dark { &mxm_ui::DARK } else { &mxm_ui::LIGHT },
                        &items,
                        &[],
                        false,
                        &mut body,
                        &mut |ui, _, _, rect| {
                            ui.allocate_space(rect.size());
                        },
                    );
                });
            harness.run();
            let next = harness.get_by_label("Next").rect();
            let previous = harness.get_by_label("Previous").rect();
            for selected in 0..12 {
                let report = editor::report(&harness.ctx).unwrap();
                assert!(report.compact_navigation);
                assert_eq!(report.selected, selected);
                assert!(
                    (report.viewport.top() - workspace.get().top() - report.plan.bar_height).abs()
                        < 0.5,
                    "{report:?}"
                );
                for (name, rect) in [("Next", next), ("Previous", previous)] {
                    assert_eq!(
                        harness.get_by_label(name).rect(),
                        rect,
                        "target moved at page {selected}"
                    );
                    assert!(workspace.get().contains_rect(rect), "{width}: {rect:?}");
                    assert!(rect.height() >= mxm_ui::space::MIN_TARGET);
                }
                for shape in &harness.output().shapes {
                    if let egui::epaint::Shape::Text(text) = &shape.shape {
                        let label = text.galley.text();
                        if label == "Previous" || label == "Next" || label.starts_with("Page ") {
                            assert_eq!(text.galley.rows.len(), 1);
                            assert!(
                                workspace
                                    .get()
                                    .contains_rect(text.galley.rect.translate(text.pos.to_vec2()))
                            );
                        }
                    }
                }
                if selected < 11 {
                    harness.get_by_label("Next").click();
                    harness.run();
                }
            }
            harness.get_by_label("Previous").focus();
            harness.run();
            harness.key_press(egui::Key::Enter);
            harness.run();
            assert_eq!(editor::report(&harness.ctx).unwrap().selected, 10);
        }
    }
}

#[test]
fn height_sweep_keeps_every_non_overflow_page_inside_its_viewport() {
    let items: Vec<_> = (0..8)
        .map(|i| Item {
            key: Key(i),
            card: Card::new("Module", 160.0).capped(160.0),
            category: if i < 4 {
                Category::Modulators
            } else {
                Category::Generators
            },
            kind: "Oscillator module",
        })
        .collect();
    for width in [240.0, 360.0, 500.0, 700.0] {
        let ctx = Context::default();
        for height in (100..700).chain((100..700).rev()) {
            let report = frame(&ctx, vec2(width, height as f32), &items, vec![]);
            if !report.plan.pages[report.selected].overflow {
                assert!(
                    !report.scrolling,
                    "width={width}, height={height}: {report:?}"
                );
                for (_, rect) in &report.visible {
                    assert!(
                        report.viewport.expand(1.0).contains_rect(*rect),
                        "width={width}, height={height}: {report:?}"
                    );
                }
            }
        }
    }
}
