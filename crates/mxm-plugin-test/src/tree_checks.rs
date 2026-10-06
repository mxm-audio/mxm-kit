//! Shared headless checks for plugin cards that are `mxm_ui::tree`s — plans/plan-layout-tree.md §4.3.
//! It proves logical geometry and writes pictures for the owner's review. It does not prove native-window, DPI or DAW behaviour.
//!
//! A plugin runs `tree_checks::card` for every card in every state of its structural-state matrix,
//! and `tree_checks::pictures` (ignored) before and after its conversion.

use std::fmt::Debug;
use std::hash::Hash;

use egui::{Align, Context, Layout, Pos2, RawInput, Rect, UiBuilder, Vec2, vec2};
use mxm_ui::tree::Node;

/// A context set up as an editor's is — fonts, theme, light, no animation — plus whatever the
/// plugin's own editor does on open (`setup`: turning the pilot on, say).
pub fn context(setup: &dyn Fn(&Context)) -> Context {
    let ctx = Context::default();
    mxm_ui::typography::apply(&ctx);
    mxm_ui::theme::apply(&ctx);
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.all_styles_mut(|s| s.animation_time = 0.0);
    setup(&ctx);
    ctx
}

/// What one card drew at one width.
#[derive(Debug)]
pub struct Drawn {
    pub card: Rect,
    pub body: Rect,
    pub painted: Rect,
    /// The body's natural height, as the tree states it.
    pub stated: f32,
    /// The body's reserved height: every disclosure open, every `Reserve` at its largest.
    pub reserved: f32,
    /// The card's outer height as the paging renderer plans it: chrome and `reserved`.
    pub room: f32,
    pub content_floor: f32,
    /// Leaves that took room outside the rectangle the tree gave them: key, given, taken.
    pub strays: Vec<(String, Rect, Rect)>,
    /// Leaves whose paint covers another leaf's rectangle: painter, covered, the overlap.
    pub overpaint: Vec<(String, String, Rect)>,
    /// Readings cut off with an ellipsis, as they were painted.
    pub cut: Vec<String>,
}

/// The readings painted cut off with an ellipsis. A reading is drawn in the value style's
/// monospace family, and a knob holds its widest one whole (`crates/ui/AGENTS.md`), so an
/// ellipsis there is a column narrower than its reading. A status line or a file name that
/// shortens to fit is proportional and is left alone: shortening is how those are designed.
fn cut_readings(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text)
                if text.galley.elided
                    && text.galley.job.sections.iter().any(|section| {
                        section.format.font_id.family == egui::FontFamily::Monospace
                    }) =>
            {
                Some(text.galley.text().to_owned())
            }
            _ => None,
        })
        .collect()
}

/// What a shape visibly covers: its bounds cut to its clip. An empty text paints nothing
/// (egui's sliders lay one out), though its bounds are unbounded.
fn visible(clipped: &egui::epaint::ClippedShape) -> Option<Rect> {
    if let egui::Shape::Text(text) = &clipped.shape
        && text.galley.text().is_empty()
    {
        return None;
    }
    let rect = clipped
        .clip_rect
        .intersect(clipped.shape.visual_bounding_rect());
    (rect.is_finite() && rect.is_positive()).then_some(rect)
}

/// The visible bounds of what was painted on `ui`'s layer from shape `from` on.
fn painted_since(ui: &egui::Ui, from: usize) -> Rect {
    ui.ctx().graphics(|layers| {
        layers.get(ui.layer_id()).map_or(Rect::NOTHING, |list| {
            list.all_entries()
                .skip(from)
                .filter_map(visible)
                .fold(Rect::NOTHING, |a, b| a.union(b))
        })
    })
}

fn shape_count(ui: &egui::Ui) -> usize {
    ui.ctx().graphics(|layers| {
        layers
            .get(ui.layer_id())
            .map_or(0, |list| list.all_entries().len())
    })
}

/// Draws one card as the paging renderer does — `ModuleCard` around the tree `build` returns, in a
/// column `width` wide and as tall as the room the renderer plans for it (chrome and the tree's
/// reserved height) — in a fresh editor context, and reads back the third pass: the weighted
/// font cuts bind there.
pub fn draw<K: Hash + Debug>(
    setup: &dyn Fn(&Context),
    title: &str,
    width: f32,
    build: &dyn Fn(&egui::Ui) -> Node<K>,
    paint: &mut dyn FnMut(&mut egui::Ui, &K, Rect),
) -> Drawn {
    let ctx = context(setup);
    let mut last = None;
    for _ in 0..3 {
        let mut result = None;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(3000.0, 6000.0))),
                ..Default::default()
            },
            |ui| {
                let tokens = mxm_ui::LIGHT;
                let mut column = ui.new_child(
                    UiBuilder::new()
                        .max_rect(Rect::from_min_size(Pos2::ZERO, vec2(width, 6000.0)))
                        .layout(Layout::top_down(Align::Min)),
                );
                let top = column.cursor().top();
                // The paging renderer's chrome, in the panel's style, as it measures it.
                let chrome = mxm_ui::shell::card_height(&column, "", 0.0);
                let mut strays = Vec::new();
                let mut painted = Vec::new();
                let (body, stated, reserved, content_floor) =
                    mxm_ui::ModuleCard::new(title).show(&mut column, &tokens, |ui| {
                        let tree = build(ui);
                        let inner = tree.drawn_width(ui, ui.available_width());
                        let stated = tree.height(ui, inner);
                        let reserved = tree.reserved_height(ui, inner);
                        let content_floor = mxm_ui::tree::card_floor(ui, title, &tree);
                        // As `paging::editor` sizes a card: its room reserved before drawing.
                        let room = chrome + reserved;
                        let bottom = top + room - mxm_ui::space::SPACE_5 - mxm_ui::space::HAIRLINE;
                        ui.set_min_height((bottom - ui.cursor().top()).max(0.0));
                        let body = mxm_ui::tree::show_observed(
                            ui,
                            &tokens,
                            &tree,
                            &mut |ui, key, rect| {
                                let from = shape_count(ui);
                                paint(ui, key, rect);
                                painted.push((format!("{key:?}"), rect, painted_since(ui, from)));
                            },
                            &mut |key, given, took| {
                                // The knob's name overhangs its box by 0.56 points (crates/ui
                                // AGENTS.md); anything beyond three quarters of a point is real.
                                if took.is_positive() && !given.expand(0.75).contains_rect(took) {
                                    strays.push((format!("{key:?}"), given, took));
                                }
                            },
                        );
                        (body, stated, reserved, content_floor)
                    });
                // Paint may reach past a leaf's own room — a slider's handle at its ends, a
                // stroke, a marker — into a gap or the card's padding, as it always has; it
                // must never reach another leaf's.
                let mut overpaint = Vec::new();
                for (key, _, drew) in &painted {
                    for (other, given, _) in &painted {
                        let overlap = drew.intersect(*given);
                        if other != key && overlap.width() > 0.75 && overlap.height() > 0.75 {
                            overpaint.push((key.clone(), other.clone(), overlap));
                        }
                    }
                }
                result = Some((
                    column.min_rect(),
                    body,
                    stated,
                    reserved,
                    chrome + reserved,
                    content_floor,
                    strays,
                    overpaint,
                ));
            },
        );
        output.textures_delta.clear();
        let cut = cut_readings(&output.shapes);
        let painted = output
            .shapes
            .iter()
            .filter_map(visible)
            .fold(Rect::NOTHING, |a, b| a.union(b));
        let (card, body, stated, reserved, room, content_floor, strays, overpaint) =
            result.expect("drawn");
        last = Some(Drawn {
            card,
            body,
            painted,
            stated,
            reserved,
            room,
            content_floor,
            strays,
            overpaint,
            cut,
        });
    }
    last.expect("three passes")
}

/// Every §4.3 check for one card in one state. `floor` is the floor the plugin gives the paging
/// renderer for this card: the larger of its content floor and any usability minimum it declares.
pub fn card<K: Hash + Debug>(
    setup: &dyn Fn(&Context),
    state: &str,
    title: &str,
    floor: f32,
    build: &dyn Fn(&egui::Ui) -> Node<K>,
    paint: &mut dyn FnMut(&mut egui::Ui, &K, Rect),
) {
    let at_floor = draw(setup, title, floor, build, paint);
    let content = at_floor.content_floor;
    let who = format!("{title} ({state})");
    // `MXM_FLOORS=1 cargo test -p <plugin> --lib tree_checks -- --nocapture` lists every card's
    // numbers, for a conversion note or a review.
    if std::env::var_os("MXM_FLOORS").is_some() {
        eprintln!(
            "floor\t{who}\tcontent {content:.1}\tfloor {floor:.1}\theight {:.1}",
            at_floor.card.height()
        );
    }

    // The floor holds the content, and nothing paints outside the card at it.
    assert!(
        floor + 0.5 >= content,
        "{who}: floor {floor:.1} is below its content floor {content:.1}"
    );
    assert!(
        at_floor.card.width() <= floor + 0.5,
        "{who}: drawn {:.1} wide at its floor {floor:.1}",
        at_floor.card.width()
    );
    assert!(
        at_floor.card.expand(0.5).contains_rect(at_floor.painted),
        "{who}: painted {:?} outside its card {:?} at its floor",
        at_floor.painted,
        at_floor.card
    );

    // The content floor is exact: the card is that wide at it, and offered eight points less it
    // stays that wide — overflowing the narrower box rather than squeezing.
    for offered in [content, content - 8.0] {
        let drawn = draw(setup, title, offered, build, paint);
        assert!(
            (drawn.card.width() - content).abs() < 0.5,
            "{who}: offered {offered:.1}, drawn {:.1} wide; its content floor is {content:.1}",
            drawn.card.width()
        );
    }

    // The stated height is the drawn height, the card is exactly the room the paging renderer
    // plans for it, and every leaf takes and paints only its own rectangle.
    for width in [floor, floor + 60.0] {
        let drawn = draw(setup, title, width, build, paint);
        assert!(
            drawn.cut.is_empty(),
            "{who} at {width:.1}: readings cut off with an ellipsis: {:?}",
            drawn.cut
        );
        assert!(
            (drawn.body.height() - drawn.stated).abs() < 0.5,
            "{who} at {width:.1}: the tree said {:.1}, the body took {:.1}",
            drawn.stated,
            drawn.body.height()
        );
        assert!(
            drawn.stated <= drawn.reserved + 0.01,
            "{who} at {width:.1}: it shows {:.1}, more than the {:.1} it reserves",
            drawn.stated,
            drawn.reserved
        );
        assert!(
            (drawn.card.height() - drawn.room).abs() < 0.5,
            "{who} at {width:.1}: planned {:.1} tall, the card took {:.1}",
            drawn.room,
            drawn.card.height()
        );
        assert!(
            drawn.card.expand(0.5).contains_rect(drawn.painted),
            "{who} at {width:.1}: painted {:?} outside its card {:?}",
            drawn.painted,
            drawn.card
        );
        assert!(
            drawn.strays.is_empty(),
            "{who} at {width:.1}: leaves took room outside their own: {:?}",
            drawn.strays
        );
        assert!(
            drawn.overpaint.is_empty(),
            "{who} at {width:.1}: leaves painted over another's room: {:?}",
            drawn.overpaint
        );
    }
}

/// Every page of a panel at `size`, light and dark, to `dir` as `light-page-N.png` and
/// `dark-page-N.png` — the owner's review material, before and after a conversion. `panel`
/// draws the whole editor surface, so it needs nothing of the tree.
pub fn pictures(
    setup: &dyn Fn(&Context),
    size: Vec2,
    dir: &std::path::Path,
    panel: &mut dyn FnMut(&mut egui::Ui),
) {
    std::fs::create_dir_all(dir).expect("picture directory");
    for (name, theme) in [
        ("light", egui::ThemePreference::Light),
        ("dark", egui::ThemePreference::Dark),
    ] {
        // kittest frames an app with an 8-point outer margin; a host gives the editor its window
        // edge to edge, so the panel draws over the whole screen. Inside the margin it had 16
        // points less each way, and a page the window holds split in the pictures.
        // **The collection's fonts before the first frame**, as an editor's `build` applies them:
        // a first frame in egui's own fonts plans other pages, and the pager's hysteresis can
        // keep that plan at an opening size hugged to under a point of slack. Once, not every
        // frame, which would keep the weighted cuts from binding.
        let fonts = std::cell::Cell::new(false);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(size)
            .wgpu()
            .build_ui(|ui| {
                if !fonts.replace(true) {
                    mxm_ui::typography::apply(ui.ctx());
                    mxm_ui::theme::apply(ui.ctx());
                    ui.ctx().set_theme(theme);
                }
                let screen = ui.ctx().content_rect();
                ui.scope_builder(egui::UiBuilder::new().max_rect(screen), |ui| panel(ui));
            });
        harness.ctx.set_theme(theme);
        harness.ctx.all_styles_mut(|s| s.animation_time = 0.0);
        setup(&harness.ctx);
        for _ in 0..4 {
            harness.step();
        }
        let pages = mxm_ui::paging::editor::report(&harness.ctx)
            .map(|report| report.plan.pages.clone())
            .unwrap_or_default();
        let count = pages.len().max(1);
        for page in 0..count {
            if let Some(key) = pages.get(page).and_then(|p| p.cards.first()) {
                mxm_ui::paging::editor::request_card(&harness.ctx, *key);
                for _ in 0..4 {
                    harness.step();
                }
            }
            let image = harness.render().expect("wgpu renders");
            image
                .save(dir.join(format!("{name}-page-{}.png", page + 1)))
                .expect("picture written");
        }
    }
}
