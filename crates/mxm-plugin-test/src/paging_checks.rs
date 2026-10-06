//! Shared headless checks for real plugin panels.
//! This proves logical geometry and reachability, not native-window/DPI or DAW operation.
//! Each plugin runs the checks that apply to it; one it does not call is not a defect.

pub fn all_rects(ctx: &egui::Context, count: usize) -> Vec<egui::Rect> {
    let report = mxm_ui::paging::editor::report(ctx).expect("paging report");
    (0..count)
        .map(|i| {
            report
                .visible
                .iter()
                .find(|(key, _)| key.0 == i as u64)
                .unwrap_or_else(|| {
                    panic!("card {i} absent; use a tall component canvas or request its page")
                })
                .1
        })
        .collect()
}

pub fn verify(
    items: &[mxm_ui::paging::Item<'_>],
    sizes: &[egui::Vec2],
    mut panel: impl FnMut(&mut egui::Ui),
) {
    use mxm_ui::paging::editor::{report, request_card};
    for theme in [egui::ThemePreference::Light, egui::ThemePreference::Dark] {
        for &physical_size in sizes {
            for scale in [1.0, 2.0] {
                let size = physical_size / scale;
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                mxm_ui::typography::apply(&ctx);
                ctx.set_theme(theme);
                mxm_ui::theme::apply(&ctx);
                ctx.all_styles_mut(|s| s.animation_time = 0.0);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                for item in items {
                    request_card(&ctx, item.key);
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(input.clone(), &mut panel);
                        output.textures_delta.clear();
                    }
                    let report = report(&ctx).expect("production panel uses dynamic paging");
                    assert!(
                        report.visible.iter().any(|(k, _)| *k == item.key),
                        "{} unreachable at {size}",
                        item.card.title
                    );
                    let mut expected: Vec<_> = items.iter().collect();
                    expected.sort_by_key(|i| i.category);
                    assert_eq!(
                        report
                            .plan
                            .pages
                            .iter()
                            .flat_map(|p| p.cards.iter().copied())
                            .collect::<Vec<_>>(),
                        expected.iter().map(|i| i.key).collect::<Vec<_>>(),
                        "category-first order/coverage"
                    );
                    assert_eq!(ctx.pixels_per_point(), scale, "paging must not choose zoom");
                    if scale == 1.0 && size.x >= 700.0 && size.y >= 760.0 {
                        assert!(
                            !report.scrolling,
                            "{} scrolls at {size}: {report:?}",
                            item.card.title
                        );
                    }
                    for (key, rect) in &report.visible {
                        let card = items.iter().find(|i| i.key == *key).unwrap().card;
                        assert!(
                            rect.width() >= card.floor - 0.75,
                            "{} below floor: {rect:?}",
                            card.title
                        );
                        // The expected items are measured at 1×; at another scale text rounds
                        // to physical pixels, and a card exactly its floor — the ceiling since
                        // `plans/plan-editor-standard.md` A1, in the private archive — can
                        // measure a point wider. The editor measures its floor in the context
                        // it draws in, so it agrees with itself; only this comparison crosses
                        // scales.
                        let rounding = if scale == 1.0 { 0.75 } else { 1.5 };
                        assert!(
                            rect.width() <= card.ceiling.unwrap_or(f32::INFINITY) + rounding,
                            "{} above ceiling at {size} ×{scale}: {:.2} wide against {:?}",
                            card.title,
                            rect.width(),
                            card.ceiling
                        );
                        if !report.scrolling {
                            assert!(
                                report.viewport.expand(0.75).contains_rect(*rect),
                                "{} clips at {size}: {rect:?} outside {:?}",
                                card.title,
                                report.viewport
                            );
                        }
                    }
                    for (i, (_, a)) in report.visible.iter().enumerate() {
                        for (_, b) in report.visible.iter().skip(i + 1) {
                            let intersection = a.intersect(*b);
                            assert!(
                                intersection.width() <= 0.75 || intersection.height() <= 0.75,
                                "overlapping cards {a:?} {b:?}"
                            );
                            if (a.top() - b.top()).abs() < 0.75 {
                                assert!(
                                    (a.bottom() - b.bottom()).abs() < 0.75,
                                    "unequal row bottoms: {a:?} {b:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
