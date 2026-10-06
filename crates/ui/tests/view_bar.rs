use std::cell::Cell;
use std::rc::Rc;

use kittest::Queryable;
use mxm_ui::{ViewBar, shell::ViewBarGeometry};

const LABELS: &[&str] = &[
    "Performance",
    "Modulators + Generators",
    "Sequencers",
    "LFO 1",
    "LFO 2",
    "Tone",
    "Effects",
];

#[test]
fn wrapped_tabs_have_uniform_text_sized_targets_and_reserve_their_actual_height() {
    for dark in [false, true] {
        for width in [320.0, 600.0, 1200.0] {
            let geometry = Rc::new(Cell::new(ViewBarGeometry::default()));
            let measured = geometry.clone();
            let content_top = Rc::new(Cell::new(0.0));
            let top = content_top.clone();
            let consumed = Rc::new(Cell::new(0.0));
            let consumed_height = consumed.clone();
            let mut selected = 0;
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(width, 600.0))
                .build_ui(move |ui| {
                    mxm_ui::typography::apply(ui.ctx());
                    ui.ctx().set_theme(if dark {
                        egui::ThemePreference::Dark
                    } else {
                        egui::ThemePreference::Light
                    });
                    let tokens = if dark { mxm_ui::DARK } else { mxm_ui::LIGHT };
                    let bar = ViewBar::new(LABELS);
                    measured.set(bar.geometry(ui, ui.available_width()));
                    let before = ui.available_rect_before_wrap().top();
                    bar.show_paged(ui, &tokens, &mut selected);
                    top.set(ui.available_rect_before_wrap().top());
                    consumed_height.set(top.get() - before);
                });
            harness.run_steps(5);
            let layout = geometry.get();
            assert!(layout.rows >= 1);
            assert!(!layout.overflow);
            assert!(
                (consumed.get() - layout.height).abs() < 1.0,
                "{layout:?} vs {}",
                consumed.get()
            );
            for (index, name) in LABELS.iter().enumerate() {
                let rect = harness.get_by_label(name).rect();
                assert!((rect.width() - layout.cell_width).abs() < 1.0);
                assert!(rect.height() >= mxm_ui::space::MIN_TARGET);
                assert!(rect.right() <= width);
                assert!(rect.bottom() <= content_top.get());
                if index >= layout.columns {
                    assert!(
                        rect.top()
                            > harness
                                .get_by_label(LABELS[index - layout.columns])
                                .rect()
                                .bottom()
                    );
                }
                // Test what is painted too: galleys must remain within the corresponding cell.
                let text = harness
                    .output()
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        if let egui::epaint::Shape::Text(text) = &shape.shape
                            && text.galley.text() == *name
                        {
                            return Some(text.galley.rect.translate(text.pos.to_vec2()));
                        }
                        None
                    })
                    .expect("painted tab label");
                assert!(
                    rect.expand(0.5).contains_rect(text),
                    "{name}: {text:?} outside {rect:?}"
                );
            }
        }
    }
}

#[test]
fn a_wrapped_tab_is_clickable_and_publishes_selection() {
    let selected = Rc::new(Cell::new(0));
    let selection = selected.clone();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(320.0, 600.0))
        .build_ui(move |ui| {
            let mut index = selection.get();
            ViewBar::new(LABELS).show_paged(ui, &mxm_ui::LIGHT, &mut index);
            selection.set(index);
        });
    harness.run();
    harness.get_by_label("Effects").click();
    harness.run();
    assert_eq!(selected.get(), 6);
    harness.get_by_label("LFO 1").focus();
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(
        selected.get(),
        3,
        "keyboard activation uses the same navigation path"
    );
}

#[test]
fn zero_or_one_derived_page_has_neither_widget_nor_bar_space() {
    for labels in [&[][..], &["Effects"][..]] {
        let mut selected = 0;
        let mut harness = egui_kittest::Harness::builder().build_ui(|ui| {
            let before = ui.available_rect_before_wrap();
            let bar = ViewBar::new(labels);
            assert_eq!(bar.geometry(ui, 500.0).height, 0.0);
            assert!(!bar.show_paged(ui, &mxm_ui::LIGHT, &mut selected));
            assert_eq!(before, ui.available_rect_before_wrap());
        });
        harness.run();
        assert!(harness.query_by_label("Effects").is_none());
    }
}

#[test]
fn an_overwide_word_is_reported_not_squeezed_or_split() {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(80.0, 600.0))
        .build_ui(|ui| {
            let geometry = ViewBar::new(LABELS).geometry(ui, ui.available_width());
            assert!(geometry.overflow);
            assert_eq!(geometry.columns, 1);
            assert_eq!(geometry.rows, LABELS.len());
            assert!(geometry.cell_width > 80.0);
        });
    harness.run();
}
