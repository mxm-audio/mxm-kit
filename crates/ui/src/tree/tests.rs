//! Every size rule is held by drawing its control and comparing: one test per control kind and per
//! variant the collection uses, never per card. And every container rule is held by laying out a
//! tree of stated sizes and reading back where each leaf went.

use super::*;
use crate::control::{ParamView, Wave, Wheel};
use egui::{Context, RawInput};

/// Runs `add` in a card body's conditions: the collection's fonts and style, a top-down Ui `width`
/// wide with the card's `SPACE_3` rhythm. Three passes: the weighted cuts bind on the third.
fn in_body<R>(width: f32, mut add: impl FnMut(&mut Ui) -> R) -> R {
    let ctx = Context::default();
    crate::typography::apply(&ctx);
    crate::theme::apply(&ctx);
    ctx.set_theme(egui::Theme::Light);
    let mut result = None;
    for _ in 0..3 {
        let mut output = ctx.run_ui(RawInput::default(), |root| {
            let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 4000.0));
            let mut body = root.new_child(
                UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            body.spacing_mut().item_spacing.y = SPACE_3;
            result = Some(add(&mut body));
        });
        output.textures_delta.clear();
    }
    result.expect("ran")
}

/// Draws one control plainly and returns the size it took, **from where it was placed** to where it
/// ended: the extent a layout has to leave for it. (A knob whose name wraps paints its name 0.56
/// points above its own box, because two wrapped Body rows are 32.0 tall and the box is two row
/// heights, 31.44. That overhang is the knob's and predates the tree; fixing it moves every knob.)
fn drawn(width: f32, draw: impl Fn(&mut Ui)) -> Vec2 {
    in_body(width, |ui| {
        let top = ui.cursor().top();
        let left = ui.cursor().left();
        let rect = ui.scope(|ui| draw(ui)).response.rect;
        vec2(rect.right() - left, rect.bottom() - top)
    })
}

/// Equal to egui's own rounding grid, a thirty-second of a point. Half a point once hid a third of a
/// point in every card's chrome and a quarter in every label line, and both added up down a tall
/// card.
fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0 / 32.0 + 0.001
}

fn same(label: &str, drew: Vec2, computed: Vec2) {
    assert!(
        close(drew.x, computed.x) && close(drew.y, computed.y),
        "{label}: drew {drew:?}, computed {computed:?}"
    );
}

const L: &Tokens = &crate::LIGHT;

#[test]
fn a_knob_is_the_size_it_says_in_every_column() {
    for name in ["Mod frequency", "Timbre", "Complex wave mix"] {
        for column in [0.0, 44.0, 56.0, 72.0, crate::control::KNOB_COLUMN_MIN] {
            for size in [Size::Standard, Size::Compact] {
                let widest = "4400.0 Hz";
                let computed = in_body(400.0, |ui| {
                    control::knob_size(ui, name, widest, size, column)
                });
                let drew = drawn(400.0, |ui| {
                    let text = widest.to_owned();
                    let view = ParamView::new(name, &text, "").widest(widest);
                    let mut value = 0.5;
                    control::knob(
                        ui,
                        L,
                        &view,
                        &mut value,
                        size,
                        column,
                        &mut None,
                        Wheel::Off,
                    );
                });
                same(&format!("{name} at {column} {size:?}"), drew, computed);
            }
        }
    }
}

#[test]
fn a_toggle_is_the_size_it_says_alone_and_in_a_stack() {
    for label in ["Mod high range", "Key", "Complex key tracking"] {
        for shared in [0.0, 180.0] {
            let computed = in_body(400.0, |ui| control::toggle_size(ui, label, shared));
            let drew = drawn(400.0, |ui| {
                control::toggle_stack(ui, shared, |ui| {
                    let mut on = false;
                    control::toggle(ui, L, label, &mut on, false, "");
                });
            });
            same(&format!("{label} in {shared}"), drew, computed);
        }
    }
    let drew = drawn(400.0, |ui| {
        let mut on = false;
        control::toggle_compact(ui, L, "M", "Mute", &mut on, false, "");
    });
    same("compact", drew, control::toggle_compact_size());
    let drew = drawn(400.0, |ui| {
        let mut on = false;
        control::toggle_wave(ui, L, Wave::Square, "Pulse", &mut on, false, "");
    });
    same("picture", drew, control::toggle_wave_size());
    let drew = drawn(400.0, |ui| {
        let mut on = false;
        control::toggle_wave(ui, L, Wave::QuarterNote, "Sync", &mut on, false, "");
    });
    same(
        "quarter note",
        drew,
        control::toggle_wave_size_of(Wave::QuarterNote),
    );
    let square = control::toggle_wave_size_of(Wave::QuarterNote);
    assert_eq!(square.x, square.y, "the quarter note is square");
    assert_eq!(
        square.y,
        control::toggle_wave_size().y,
        "at a wave cell's height"
    );
}

#[test]
fn a_segmented_control_is_the_size_it_says_above_beside_and_shared() {
    for (label, options) in [
        ("Mod type", &["AM", "FM"][..]),
        ("Gate 1 mode", &["VCA", "VCF", "VCA + VCF"][..]),
        ("A label much wider than its two cells", &["A", "B"][..]),
    ] {
        for shared in [0.0, 90.0] {
            let computed = in_body(600.0, |ui| {
                control::segmented_size(ui, label, options, None, shared)
            });
            let drew = drawn(600.0, |ui| {
                control::segmented_stack(ui, shared, |ui| {
                    let mut selected = 0;
                    control::segmented(
                        ui,
                        L,
                        label,
                        options,
                        &mut selected,
                        None,
                        None,
                        &control::described(options),
                    );
                });
            });
            same(&format!("{label} shared {shared}"), drew, computed);
        }
        let beside = Some(Size::Primary);
        let computed = in_body(600.0, |ui| {
            control::segmented_size(ui, label, options, beside, 0.0)
        });
        let drew = drawn(600.0, |ui| {
            let mut selected = 0;
            control::segmented_beside(
                ui,
                L,
                label,
                options,
                &mut selected,
                None,
                None,
                Size::Primary,
                &control::described(options),
            );
        });
        same(&format!("{label} beside"), drew, computed);
    }
}

#[test]
fn a_wave_grid_is_the_size_it_says_labelled_unlabelled_marked_and_beside() {
    let three = [
        (Wave::Triangle, "Triangle"),
        (Wave::Square, "Square"),
        (Wave::RampUp, "Sawtooth"),
    ];
    let six = [
        (Wave::Triangle, "Triangle"),
        (Wave::Square, "Square"),
        (Wave::RampUp, "Saw"),
        (Wave::RampDown, "Ramp"),
        (Wave::Sine, "Sine"),
        (Wave::Noise, "Noise"),
    ];
    for options in [&three[..], &six[..]] {
        let n = options.len();
        let computed = in_body(600.0, |ui| {
            control::waves_size(ui, Some("Mod wave"), n, &[], None)
        });
        let drew = drawn(600.0, |ui| {
            let mut s = 0;
            control::segmented_waves(
                ui,
                L,
                "Mod wave",
                options,
                &mut s,
                None,
                None,
                None,
                &control::described(options),
            );
        });
        same(&format!("{n} labelled"), drew, computed);

        let computed = in_body(600.0, |ui| control::waves_size(ui, None, n, &[], None));
        let drew = drawn(600.0, |ui| {
            let mut s = 0;
            control::segmented_waves_unlabelled(
                ui,
                L,
                "Mod wave",
                options,
                &mut s,
                None,
                None,
                &control::described(options),
            );
        });
        same(&format!("{n} unlabelled"), drew, computed);

        let beside = Some(Size::Standard);
        let computed = in_body(600.0, |ui| {
            control::waves_size(ui, Some("Shape"), n, &[], beside)
        });
        let drew = drawn(600.0, |ui| {
            let mut s = 0;
            control::segmented_waves(
                ui,
                L,
                "Shape",
                options,
                &mut s,
                None,
                None,
                beside,
                &control::described(options),
            );
        });
        same(&format!("{n} beside"), drew, computed);
    }
    let marks = ["−1", "−2", "−2"];
    let computed = in_body(600.0, |ui| {
        control::waves_size(ui, Some("Sub"), 3, &marks, None)
    });
    let drew = drawn(600.0, |ui| {
        let mut s = 0;
        control::segmented_waves_marked(
            ui,
            L,
            "Sub",
            &three,
            &marks,
            &mut s,
            None,
            None,
            None,
            &control::described(&three),
        );
    });
    same("marked", drew, computed);
}

#[test]
fn a_selector_fits_its_floor_on_one_row_in_either_style() {
    let options = ["modulate", "Envelope", "Random 1"];
    for caption in [false, true] {
        let computed = in_body(400.0, |ui| {
            if caption {
                control::selector_caption_size(ui, "Pitch", &options)
            } else {
                control::selector_size(ui, "Pitch", &options)
            }
        });
        let drew = drawn(computed.x, |ui| {
            let mut selected = 0;
            if caption {
                control::selector_caption(
                    ui,
                    L,
                    "Pitch",
                    &options,
                    &mut selected,
                    None,
                    Some(0),
                    "",
                );
            } else {
                control::selector(ui, L, "Pitch", &options, &mut selected, None, Some(0), "");
            }
        });
        assert!(
            close(drew.y, computed.y),
            "caption {caption}: drew {drew:?}, computed {computed:?}"
        );
        assert!(
            drew.x <= computed.x + 0.5,
            "caption {caption}: overflowed {drew:?} > {computed:?}"
        );
    }
}

#[test]
fn a_slider_is_the_height_it_says_and_fits_its_floor() {
    for (label, widest) in [("Pitch from Envelope", "+4.00 oct"), ("Level", "-100 %")] {
        for quiet in [false, true] {
            let computed = in_body(400.0, |ui| control::slider_size(ui, label, quiet, widest));
            let drew = drawn(computed.x, |ui| {
                let text = widest.to_owned();
                let mut view = ParamView::new(label, &text, "");
                view.quiet_label = quiet;
                let mut value = 0.5;
                control::slider(ui, L, &view, &mut value, computed.x, &mut None, Wheel::Off);
            });
            assert!(
                close(drew.y, computed.y) && drew.x <= computed.x + 0.5,
                "{label} quiet {quiet}: drew {drew:?}, computed {computed:?}"
            );
        }
    }
    let computed = in_body(400.0, |ui| {
        control::slider_vertical_size(ui, "3", "100 %", 300.0)
    });
    let drew = drawn(400.0, |ui| {
        let text = "100 %".to_owned();
        let view = ParamView::new("3", &text, "");
        let mut value = 0.5;
        control::slider_vertical(
            ui,
            L,
            &view,
            &mut value,
            computed.x,
            300.0,
            &mut None,
            Wheel::Off,
        );
    });
    same("vertical", drew, computed);
}

#[test]
fn plain_egui_widgets_are_the_size_they_say() {
    for label in ["Browse…", "Fire once", "+ Stage"] {
        for min in [Vec2::ZERO, vec2(150.0, 28.0)] {
            let computed = in_body(400.0, |ui| control::button_size(ui, label, min));
            let drew = drawn(400.0, |ui| {
                ui.add(egui::Button::new(label).min_size(min));
            });
            same(&format!("button {label} {min:?}"), drew, computed);
        }
    }
    let computed = in_body(400.0, |ui| control::checkbox_size(ui, "Linked detector"));
    let drew = drawn(400.0, |ui| {
        let mut on = false;
        ui.checkbox(&mut on, "Linked detector");
    });
    same("checkbox", drew, computed);
    for selected in [
        "Fade",
        "Mono to stereo",
        "A much longer interpretation name",
    ] {
        let computed = in_body(400.0, |ui| control::combo_size(ui, selected));
        let drew = drawn(400.0, |ui| {
            egui::ComboBox::from_id_salt("probe")
                .selected_text(selected)
                .show_ui(ui, |_| {});
        });
        same(&format!("combo {selected}"), drew, computed);
    }
    let computed = in_body(400.0, |ui| control::egui_slider_size(ui, "5000 ms", None));
    let drew = drawn(400.0, |ui| {
        let mut v = 5000.0_f32;
        ui.add(egui::Slider::new(&mut v, 0.1..=5000.0).suffix(" ms"));
    });
    same("egui slider", drew, computed);
    // An egui slider's value box follows the value's own text; the caller passes the widest.
    let computed = in_body(400.0, |ui| {
        control::egui_slider_size(ui, "0.50", Some("Size"))
    });
    let drew = drawn(400.0, |ui| {
        let mut v = 0.5_f32;
        ui.add(egui::Slider::new(&mut v, 0.0..=1.0).text("Size"));
    });
    same("egui slider with a label", drew, computed);
    let drew = drawn(400.0, |ui| {
        control::remove_mark(ui, L, 28.0, false, "Remove", false, "");
    });
    same("remove mark", drew, control::remove_mark_size(28.0));
}

#[test]
fn text_is_the_height_it_says_at_every_width_and_font() {
    let text =
        "As a modulation source it keeps sub-audio content; as LPG 2's input it is AC-coupled.";
    for font in [Font::Caption, Font::Body, Font::Sized(11.0)] {
        for width in [120.0, 200.0, 300.0, 600.0] {
            let node: Node<u8> = leaf(
                0,
                Kind::Text {
                    text: text.to_owned(),
                    font,
                    flow: Flow::Wrap,
                },
            );
            let computed = in_body(width, |ui| node.height(ui, width));
            let drew = drawn(width, |ui| {
                let f = font_id(ui, font);
                ui.label(egui::RichText::new(text).font(f));
            });
            assert!(
                close(drew.y, computed),
                "{font:?} at {width}: drew {drew:?}, computed {computed}"
            );
        }
    }
}

/// Rows and stacks are arithmetic on their children, and a filler takes the rest.
#[test]
fn rows_add_up_stacks_take_the_widest_and_fillers_take_what_is_left() {
    let fixed = |w: f32, h: f32| Kind::Custom {
        min_width: w,
        height: Height::Fixed(h),
        fills: false,
    };
    let filler = |w: f32, h: f32| Kind::Custom {
        min_width: w,
        height: Height::Fixed(h),
        fills: true,
    };
    let tree: Node<u8> = stack(vec![
        row(vec![
            leaf(0, fixed(100.0, 50.0)),
            leaf(1, fixed(60.0, 80.0)),
        ]),
        leaf(2, filler(40.0, 20.0)),
        group(vec![
            leaf(3, fixed(120.0, 28.0)),
            leaf(4, fixed(30.0, 28.0)),
        ]),
    ]);
    let (min, height, rects) = layout(&tree, 300.0);
    assert!(close(min, 100.0 + GAP + 60.0));
    assert!(close(
        height,
        80.0 + GAP + 20.0 + GAP + (28.0 * 2.0 + SPACE_2 + 2.0 * GROUP_INSET)
    ));
    assert!(close(rects[&1].left(), 100.0 + GAP), "{:?}", rects[&1]);
    assert!(
        close(rects[&2].width(), 300.0),
        "the filler takes the stack's width"
    );
    assert!(
        close(rects[&3].left(), GROUP_INSET),
        "inside the group's inset"
    );
    assert!(
        close(rects[&4].width(), 30.0),
        "a non-filler keeps its own width"
    );
}

fn layout(tree: &Node<u8>, width: f32) -> (f32, f32, std::collections::HashMap<u8, Rect>) {
    in_body(width, |ui| {
        let mut rects = std::collections::HashMap::new();
        let min = tree.min_width(ui);
        let height = tree.height(ui, tree.drawn_width(ui, width));
        let origin = ui.cursor().min.to_vec2();
        show(ui, L, tree, |_, key, rect| {
            rects.insert(*key, rect.translate(-origin));
        });
        (min, height, rects)
    })
}

fn boxed(w: f32, h: f32) -> Kind {
    Kind::Custom {
        min_width: w,
        height: Height::Fixed(h),
        fills: false,
    }
}

#[test]
fn columns_are_equal_centred_and_capped_like_ui_columns() {
    // Two 40-wide children in columns of 76 each, gaps included, as mono-08's knob rows cap them.
    let tree: Node<u8> = Node::Columns {
        gap: 8.0,
        column: Some(76.0),
        stretch: false,
        children: vec![leaf(0, boxed(40.0, 10.0)), leaf(1, boxed(40.0, 30.0))],
    };
    // Offered less than the cap, the columns share what there is: the cap is not a floor.
    let (min, _, squeezed) = layout(&tree, 100.0);
    assert!(
        close(min, 40.0 * 2.0 + 8.0),
        "the cap is not a floor: {min}"
    );
    assert!(
        close(squeezed[&1].center().x, 46.0 + 8.0 + 23.0),
        "{:?}",
        squeezed[&1]
    );
    let (_, height, rects) = layout(&tree, 400.0);
    assert!(close(height, 30.0));
    let column = (152.0 - 8.0) / 2.0;
    assert!(close(rects[&0].center().x, column / 2.0), "{:?}", rects[&0]);
    assert!(
        close(rects[&1].center().x, column + 8.0 + column / 2.0),
        "{:?}",
        rects[&1]
    );

    let stretched: Node<u8> = Node::Columns {
        gap: 8.0,
        column: None,
        stretch: true,
        children: vec![leaf(0, boxed(40.0, 10.0)), leaf(1, boxed(40.0, 10.0))],
    };
    let (_, _, rects) = layout(&stretched, 300.0);
    assert!(
        close(rects[&1].center().x, 146.0 + 8.0 + 73.0),
        "{:?}",
        rects[&1]
    );
}

#[test]
fn a_grid_puts_as_many_on_a_line_as_the_width_holds() {
    let tree: Node<u8> = Node::Grid {
        gap: 8.0,
        row_gap: 16.0,
        column: 78.0,
        children: (0..5).map(|i| leaf(i, boxed(40.0, 20.0))).collect(),
    };
    let (_, wide, _) = layout(&tree, 400.0);
    assert!(close(wide, 20.0), "five fit on one line at 400: {wide}");
    let (_, narrow, rects) = layout(&tree, 240.0);
    assert!(
        close(narrow, 20.0 + 16.0 + 20.0),
        "three and two at 240: {narrow}"
    );
    assert!(close(rects[&3].top(), 36.0));
}

#[test]
fn a_wrap_breaks_where_the_next_would_not_fit() {
    let tree: Node<u8> = Node::Wrap {
        gap: 8.0,
        row_gap: 4.0,
        children: (0..4).map(|i| leaf(i, boxed(60.0, 28.0))).collect(),
    };
    let (_, height, rects) = layout(&tree, 140.0);
    assert!(close(height, 28.0 * 2.0 + 4.0));
    assert!(close(rects[&2].left(), 0.0) && close(rects[&2].top(), 32.0));
}

#[test]
fn a_share_gives_every_toggle_and_every_cell_the_widest() {
    let tree: Node<u8> = share(
        Share::Toggles,
        stack(vec![
            row(vec![
                leaf(
                    0,
                    Kind::Toggle {
                        label: "Key".into(),
                    },
                ),
                leaf(1, boxed(12.0, 12.0)),
            ]),
            row(vec![
                leaf(
                    2,
                    Kind::Toggle {
                        label: "Complex key tracking".into(),
                    },
                ),
                leaf(3, boxed(12.0, 12.0)),
            ]),
        ]),
    );
    let (_, _, rects) = layout(&tree, 400.0);
    assert!(
        close(rects[&0].width(), rects[&2].width()),
        "{:?} {:?}",
        rects[&0],
        rects[&2]
    );
    assert!(close(rects[&1].left(), rects[&3].left()));

    let cells: Node<u8> = share(
        Share::Cells,
        stack(vec![
            leaf(
                0,
                Kind::Segmented {
                    label: "Sync".into(),
                    options: vec!["Free".into(), "Sync".into()],
                    beside: None,
                },
            ),
            leaf(
                1,
                Kind::Segmented {
                    label: "Change".into(),
                    options: vec!["Repitch".into(), "Fade".into(), "Jump".into()],
                    beside: None,
                },
            ),
        ]),
    );
    let (drew_first, drew_second) = in_body(600.0, |ui| {
        let mut widths = Vec::new();
        show(ui, L, &cells, |ui, key, _| {
            let options: &[&str] = if *key == 0 {
                &["Free", "Sync"]
            } else {
                &["Repitch", "Fade", "Jump"]
            };
            let mut s = 0;
            let r = ui.scope(|ui| {
                control::segmented(
                    ui,
                    L,
                    "x",
                    options,
                    &mut s,
                    None,
                    None,
                    &control::described(options),
                )
            });
            widths.push(r.response.rect.width());
        });
        (widths[0], widths[1])
    });
    let (_, _, rects) = layout(&cells, 600.0);
    assert!(close(drew_first, rects[&0].width()) && close(drew_second, rects[&1].width()));
    assert!(close(
        (drew_first - HAIRLINE) / 2.0,
        (drew_second - 2.0 * HAIRLINE) / 3.0
    ));
}

#[test]
fn a_reserve_is_as_large_as_its_largest_alternative_and_draws_one() {
    let tree: Node<u8> = stack(vec![
        reserve(leaf(0, boxed(50.0, 20.0)), vec![leaf(9, boxed(80.0, 60.0))]),
        leaf(1, boxed(10.0, 10.0)),
    ]);
    let (min, height, rects) = layout(&tree, 400.0);
    assert!(
        close(min, 80.0),
        "the widest alternative is the floor: {min}"
    );
    assert!(
        close(height, 20.0 + GAP + 10.0),
        "drawn as it stands: {height}"
    );
    let reserved = in_body(400.0, |ui| tree.reserved_height(ui, 400.0));
    assert!(
        close(reserved, 60.0 + GAP + 10.0),
        "reserved at its largest: {reserved}"
    );
    assert!(
        !rects.contains_key(&9),
        "an alternative is reserved, never drawn"
    );
    // What follows sits under what is shown: the reserved room is the card's foot, not a gap.
    assert!(close(rects[&1].top(), 20.0 + GAP));
}

#[test]
fn pads_and_anchors_move_a_child_by_their_rule() {
    let tree: Node<u8> = row(vec![
        leaf(0, boxed(40.0, 10.0)),
        beside_knob(Size::Standard, leaf(1, boxed(40.0, 28.0))),
        pad_all(5.0, 7.0, 3.0, leaf(2, boxed(10.0, 10.0))),
    ]);
    let (min, height, rects) = layout(&tree, 400.0);
    let drop = in_body(400.0, |ui| control::knob_line_drop(ui, Size::Standard));
    assert!(close(rects[&1].top(), drop));
    assert!(close(rects[&2].left(), 40.0 + GAP + 40.0 + GAP + 7.0) && close(rects[&2].top(), 5.0));
    assert!(close(min, 40.0 + GAP + 40.0 + GAP + 17.0));
    assert!(close(height, drop + 28.0));
}

/// A switch beside a knob stands `switch_gap` from the knob's circle — with room, where the column
/// is the knob column and the switch stands against it, and at the row's floor, where the column is
/// the knob's own width — on the circle's line, never in the knob's room, and what follows it keeps
/// its gap. After anything but a knob it stands `switch_gap` from that.
#[test]
fn a_switch_beside_a_knob_stands_its_gap_from_the_circle_at_every_width() {
    let knob = || {
        leaf(
            0,
            Kind::Knob {
                name: "Rate".to_owned(),
                widest: "20.0 Hz".to_owned(),
                size: Size::Standard,
                column: 0.0,
            },
        )
    };
    let rows: Vec<Node<u8>> = in_body(900.0, |ui| {
        vec![
            row(vec![
                knob_row(ui, vec![(Size::Standard, knob())]),
                switch_beside_knob(Size::Standard, leaf(1, boxed(28.0, 28.0))),
                leaf(2, boxed(10.0, 10.0)),
            ]),
            row(vec![
                knob(),
                switch_beside_knob(Size::Standard, leaf(1, boxed(28.0, 28.0))),
                leaf(2, boxed(10.0, 10.0)),
            ]),
        ]
    });
    let drop = in_body(900.0, |ui| control::knob_line_drop(ui, Size::Standard));
    let gap = switch_gap(Size::Standard);
    assert!(close(gap, 18.0), "half the 34 it was: {gap}");
    for tree in &rows {
        let floor = in_body(900.0, |ui| tree.min_width(ui));
        for width in [900.0, floor] {
            let (_, _, rects) = layout(tree, width);
            let circle = rects[&0].center().x + Size::Standard.diameter() / 2.0;
            assert!(
                close(rects[&1].left() - circle, gap),
                "the switch is {} from the circle at {width}, not {gap}",
                rects[&1].left() - circle
            );
            assert!(
                rects[&1].left() >= rects[&0].right() - 0.01,
                "never in the knob's room at {width}"
            );
            assert!(close(rects[&1].top(), drop), "on the circle's line");
            assert!(close(rects[&2].left(), rects[&1].right() + GAP));
            assert!(rects[&2].right() <= width + 0.5, "inside {width}");
        }
        let (_, _, rects) = layout(tree, floor);
        assert!(close(rects[&2].right(), floor), "the floor is hugged");
    }
    let tree: Node<u8> = row(vec![
        leaf(0, boxed(40.0, 10.0)),
        switch_beside_knob(Size::Standard, leaf(1, boxed(28.0, 28.0))),
    ]);
    let (min, _, rects) = layout(&tree, 400.0);
    assert!(close(rects[&1].left(), 40.0 + gap));
    assert!(close(min, 40.0 + gap + 28.0));
}

#[test]
fn a_row_height_leaf_takes_the_rows_height() {
    let tree: Node<u8> = row(vec![
        leaf(0, boxed(40.0, 94.0)),
        leaf(
            1,
            Kind::Custom {
                min_width: 100.0,
                height: Height::Row(88.0),
                fills: true,
            },
        ),
    ]);
    let (_, height, rects) = layout(&tree, 400.0);
    assert!(close(height, 94.0) && close(rects[&1].height(), 94.0));
    assert!(close(rects[&1].width(), 400.0 - 40.0 - GAP));
}

/// A disclosure drawn by the tree is the height the shared helper draws when open, and reserves it
/// when closed.
#[test]
fn a_disclosure_reserves_its_open_body() {
    let body = || -> Node<u8> { leaf(0, boxed(100.0, 50.0)) };
    let open = in_body(400.0, |ui| {
        ui.data_mut(|d| d.insert_temp(crate::shell::disclosure_id("Advanced"), true));
        let top = ui.cursor().top();
        let r = ui
            .scope(|ui| {
                crate::shell::disclosure(ui, L, "Advanced", "", |ui| {
                    ui.allocate_exact_size(vec2(100.0, 50.0), Sense::hover());
                });
            })
            .response
            .rect;
        r.bottom() - top
    });
    for state in [true, false] {
        let (natural, reserved) = in_body(400.0, |ui| {
            ui.data_mut(|d| d.insert_temp(crate::shell::disclosure_id("Advanced"), state));
            let tree = disclosure(ui.ctx(), "Advanced", "", body());
            let width = tree.drawn_width(ui, 400.0);
            let h = tree.height(ui, width);
            let drawn = show(ui, L, &tree, |_, _, _| {}).height();
            assert!(close(h, drawn), "open {state}: stated {h}, drew {drawn}");
            (h, tree.reserved_height(ui, width))
        });
        assert!(
            close(reserved, open),
            "open {state}: reserved {reserved}, helper {open}"
        );
        if state {
            assert!(close(natural, open), "open: tree {natural}, helper {open}");
        } else {
            assert!(
                natural < open,
                "closed, it draws its header alone: {natural}"
            );
        }
    }
}

/// egui's own collapsing header opens its body over `animation_time`. The tree's follows the same
/// openness, and its stated height with it: part-open, nothing under the header shows below that
/// height, and open it is the reserved height.
#[test]
fn a_collapsing_body_opens_over_egui_s_animation_inside_its_stated_height() {
    let ctx = Context::default();
    crate::typography::apply(&ctx);
    crate::theme::apply(&ctx);
    ctx.all_styles_mut(|s| s.animation_time = 1.0);
    // At `time`: the stated and reserved heights, the tree's top, and the leaf's clip if it drew.
    let frame = |time: f64| {
        let mut result = None;
        let mut output = ctx.run_ui(
            RawInput {
                time: Some(time),
                ..Default::default()
            },
            |root| {
                let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 4000.0));
                let mut body = root.new_child(
                    UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::top_down(Align::Min)),
                );
                body.spacing_mut().item_spacing.y = SPACE_3;
                let tree = collapsing(body.ctx(), "Bender", false, leaf(0u8, boxed(100.0, 50.0)));
                let width = tree.drawn_width(&body, 400.0);
                let stated = tree.height(&body, width);
                let reserved = tree.reserved_height(&body, width);
                let top = body.cursor().top();
                let mut clip = None;
                show(&mut body, L, &tree, |ui, _, _| clip = Some(ui.clip_rect()));
                result = Some((stated, reserved, top, clip));
            },
        );
        output.textures_delta.clear();
        result.expect("ran")
    };
    let (closed, reserved, _, clip) = frame(0.0);
    assert!(clip.is_none(), "closed, the body is not drawn");
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        &ctx,
        collapsing_id("Bender"),
        false,
    );
    state.set_open(true);
    state.store(&ctx);
    frame(0.0);
    let (part, _, top, clip) = frame(0.5);
    assert!(
        closed < part && part < reserved,
        "half-way, {part} lies between closed {closed} and open {reserved}"
    );
    let clip = clip.expect("part-open, the body is drawn");
    assert!(
        clip.bottom() <= top + part + 0.001,
        "part-open, the body shows down to {} and the tree stated {}",
        clip.bottom(),
        top + part
    );
    let (open, _, top, clip) = frame(2.0);
    assert!(close(open, reserved), "open {open}, reserved {reserved}");
    assert!(
        clip.expect("open, the body is drawn").bottom() > top + open,
        "open, the body is not clipped"
    );
}

#[test]
fn a_collapsing_header_is_the_size_it_says() {
    for caption in [false, true] {
        let (computed, drew) = in_body(400.0, |ui| {
            let tree: Node<u8> =
                collapsing(ui.ctx(), "Bender", caption, leaf(0, boxed(10.0, 10.0)));
            let Node::Disclosure { header, .. } = &tree else {
                unreachable!()
            };
            let size = vec2(header_width(ui, header), header_height(ui, header));
            let rect = Rect::from_min_size(ui.cursor().min, vec2(400.0, 400.0));
            let (_, used) = draw_header(ui, L, header, false, rect, Scope::default());
            (
                size,
                vec2(used.right() - rect.left(), used.bottom() - rect.top()),
            )
        });
        same(&format!("collapsing, caption {caption}"), drew, computed);
    }
}

#[test]
fn a_cards_height_is_its_chrome_around_its_body() {
    let (computed, drew) = in_body(400.0, |ui| {
        let computed = crate::shell::card_height(ui, "Mod oscillator", 123.0);
        let top = ui.cursor().top();
        crate::ModuleCard::new("Mod oscillator").show(ui, L, |ui| {
            ui.allocate_exact_size(vec2(100.0, 123.0), Sense::hover());
        });
        (computed, ui.min_rect().bottom() - top)
    });
    // Exact, not within half a point: a chrome a third of a point tall moved every card's bottom
    // edge by a pixel before this was tightened.
    assert!(
        (computed - drew).abs() < 0.01,
        "computed {computed}, drew {drew}"
    );
}

#[test]
fn a_centre_puts_its_child_in_the_middle_of_the_width_offered() {
    let tree: Node<u8> = stack(vec![
        center(leaf(0, boxed(40.0, 10.0))),
        leaf(
            1,
            Kind::Custom {
                min_width: 20.0,
                height: Height::Fixed(10.0),
                fills: true,
            },
        ),
    ]);
    let (min, _, rects) = layout(&tree, 200.0);
    assert!(close(min, 40.0));
    assert!(close(rects[&0].center().x, 100.0), "{:?}", rects[&0]);
}

#[test]
fn capped_columns_in_a_row_leave_their_spare_room_empty() {
    let tree: Node<u8> = row(vec![
        Node::Columns {
            gap: 8.0,
            column: Some(76.0),
            stretch: false,
            children: vec![leaf(0, boxed(40.0, 10.0)), leaf(1, boxed(40.0, 10.0))],
        },
        leaf(2, boxed(30.0, 10.0)),
    ]);
    let (_, _, rects) = layout(&tree, 600.0);
    assert!(close(rects[&2].left(), 152.0 + GAP), "{:?}", rects[&2]);
}

/// The collection's knob row with room to spare draws every column exactly the knob column — the
/// cap counts the gaps — and offered its floor, shrinks only to its widest knob.
#[test]
fn a_knob_row_draws_the_knob_column_with_room_and_its_widest_knob_at_its_floor() {
    let knob = |key: u8, name: &str| {
        leaf(
            key,
            Kind::Knob {
                name: name.to_owned(),
                widest: "50 %".to_owned(),
                size: Size::Standard,
                column: 0.0,
            },
        )
    };
    let row = in_body(900.0, |ui| {
        knob_row(
            ui,
            (0..6)
                .map(|key| (Size::Standard, knob(key, "Tap")))
                .collect(),
        )
    });
    let (_, _, rects) = layout(&row, 900.0);
    for key in 0..6u8 {
        assert!(
            close(rects[&key].width(), crate::control::KNOB_COLUMN_MIN),
            "column {key} is {} with room, not the knob column",
            rects[&key].width()
        );
    }
    let floor = in_body(900.0, |ui| row.min_width(ui));
    let (_, _, rects) = layout(&row, floor);
    let widest = in_body(900.0, |ui| {
        crate::control::knob_size(ui, "Tap", "50 %", Size::Standard, 0.0).x
    });
    assert!(
        close(rects[&0].width(), widest),
        "at its floor a column is its widest knob, {widest}: {:?}",
        rects[&0]
    );
}

/// A knob that shows its value holds its widest reading whole: the value is one line and would
/// otherwise be cut short at the card's floor.
#[test]
fn a_knob_holds_its_widest_reading_on_one_line() {
    let (reading, size) = in_body(400.0, |ui| {
        (
            {
                let font = crate::typography::value_style(ui.style()).resolve(ui.style());
                ui.painter()
                    .layout_no_wrap("0.060 s/oct".to_owned(), font, egui::Color32::PLACEHOLDER)
                    .size()
                    .x
            },
            crate::control::knob_size(ui, "Glide", "0.060 s/oct", Size::Standard, 0.0),
        )
    });
    assert!(
        size.x >= reading,
        "{size:?} is narrower than its reading, {reading}"
    );
}

/// A slider's reading never draws over its name, and its size says so: drawn at its floor it takes
/// exactly that, name and reading side by side.
#[test]
fn a_slider_keeps_its_reading_off_its_name_at_its_floor() {
    for (label, widest) in [("Envelope", "-4.00 oct"), ("Random 1", "+100 %")] {
        let computed = in_body(400.0, |ui| control::slider_size(ui, label, true, widest));
        let drew = drawn(computed.x, |ui| {
            let text = widest.to_owned();
            let mut view = ParamView::new(label, &text, "");
            view.quiet_label = true;
            let mut value = 0.5;
            control::slider(ui, L, &view, &mut value, computed.x, &mut None, Wheel::Off);
        });
        same(&format!("slider {label}"), drew, computed);
    }
}

/// The tree's collapsing header is where egui's own puts its row and its body: the same header size,
/// the body an indent in and an item spacing below — so a converted Bender or Setup did not move.
#[test]
fn a_collapsing_header_matches_egui_s_own() {
    let (egui_header, egui_body, tree_header, tree_body) = in_body(400.0, |ui| {
        let origin = ui.cursor().min;
        let id = egui::Id::new("probe-collapsing");
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            ui.make_persistent_id(id),
            true,
        )
        .store(ui.ctx());
        let mut body = Rect::NOTHING;
        let response = egui::CollapsingHeader::new("Bender")
            .id_salt(id)
            .default_open(true)
            .show(ui, |ui| {
                body = ui.allocate_exact_size(vec2(100.0, 50.0), Sense::hover()).0;
            });
        let header = response.header_response.rect.translate(-origin.to_vec2());
        let body = body.translate(-origin.to_vec2());
        let tree: Node<u8> = Node::Disclosure {
            header: Header::Collapsing {
                title: "Bender".into(),
                caption: false,
            },
            openness: 1.0,
            body: Box::new(leaf(0, boxed(100.0, 50.0))),
        };
        let Node::Disclosure { header: h, .. } = &tree else {
            unreachable!()
        };
        let size = vec2(header_width(ui, h), header_height(ui, h));
        let offset = body_offset(ui, h);
        (
            header.size(),
            body.min.to_vec2(),
            size,
            vec2(offset.x, size.y + offset.y),
        )
    });
    same("header", egui_header, tree_header);
    same("body", egui_body, tree_body);
}
