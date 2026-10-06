//! The opening size: **set the window to quarter 4K, then hug every page it plans.**
//!
//! The owner's rule, 2026-09-09, and it is the whole rule. Lay the panel out at the quarter-4K
//! budget of design system §4.2 — the most room an editor may ask for, so it shows as many modules
//! as it ever will — then take away the slack every page leaves between what it drew and the edges
//! of the window (the owner, 2026-09-24: not the first page alone). What is left is what the editor
//! opens at.
//!
//! **It replaces a number somebody chose while building the panel.** Every reference size in the
//! collection used to be one, which is how mxm-mono-01 came to open on two pages with a band of bare
//! canvas under the last row while all six of its cards would have fitted.

/// The quarter-4K budget, design system §4.2. A window may not open larger than this.
pub const BUDGET: egui::Vec2 = egui::vec2(1920.0, 1080.0);

/// What one layout at `size` drew: how many pages it planned, and the window that would hug
/// every one of them.
///
/// The hugged window is `size` less the slack every page leaves between its content and the
/// viewport edges, so everything above and left of the cards — app bar, view bar, gutters — is
/// carried along untouched. **Every page, not the opening one** (the owner, 2026-09-24,
/// `plans/plan-layout-tree.md` §10, in the private archive): a window hugged to its first page is
/// as small as that page happens to be, and mxm-para-07 opened at a third of the budget with seven
/// tabs because its first page was one short category. `None` when nothing was planned at that
/// size.
///
/// `reveal` opens whatever the editor keeps behind a disclosure, because §4.2 asks for the
/// opening size to be judged with disclosures open: a window sized to a closed expander throws
/// its own controls off the bottom the moment somebody opens one.
pub fn hug(
    size: egui::Vec2,
    reveal: &dyn Fn(&egui::Context),
    panel: &mut impl FnMut(&mut egui::Ui),
) -> Option<(usize, egui::Vec2)> {
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    };
    // Four frames: the fonts settle, the pager plans, then paints the plan it settled on.
    // `reveal` is reapplied each frame, because a disclosure request is consumed by the card
    // that answers it.
    let mut frames = |count: usize| {
        for _ in 0..count {
            reveal(&ctx);
            let mut output = ctx.run_ui(input.clone(), &mut *panel);
            output.textures_delta.clear();
        }
    };
    frames(4);
    let report = mxm_ui::paging::editor::report(&ctx)?;
    if report.plan.pages.is_empty() || report.visible.is_empty() {
        return None;
    }
    let drawn = |report: &mxm_ui::paging::editor::Report| {
        report
            .visible
            .iter()
            .fold(egui::Rect::NOTHING, |all, (_, rect)| all.union(*rect))
    };
    let mut content = drawn(&report);
    for page in report.plan.pages.iter().skip(1) {
        mxm_ui::paging::editor::request_card(&ctx, page.cards[0]);
        frames(2);
        let shown = mxm_ui::paging::editor::report(&ctx)?;
        content = content.union(drawn(&shown));
    }
    // **A guard of `GUARD` points is left**, so the plan a window was hugged to survives a
    // hair of measurement difference: hugged to the point, mxm-creative-sampler's second row
    // fitted with 0.4 of a point to spare, and a card a point taller elsewhere split its pages.
    let slack = report.viewport.max - content.max - egui::Vec2::splat(GUARD);
    Some((
        report.plan.pages.len(),
        egui::vec2(
            (size.x - slack.x.max(0.0)).ceil(),
            (size.y - slack.y.max(0.0)).ceil(),
        ),
    ))
}

/// The room a hugged opening size leaves beyond what its pages draw, in points on each axis.
pub const GUARD: f32 = 2.0;

/// The opening size: the budget, hugged.
///
/// Hugging rewraps — a narrower window packs its rows differently — so the answer is the fixed
/// point rather than one shrink. Two or three passes reach it; a shrink that adds a page has
/// gone too far and the previous size stands.
pub fn derive(
    reveal: &dyn Fn(&egui::Context),
    panel: &mut impl FnMut(&mut egui::Ui),
) -> Option<egui::Vec2> {
    let (pages, mut size) = hug(BUDGET, reveal, panel)?;
    for _ in 0..4 {
        let Some((planned, hugged)) = hug(size, reveal, panel) else {
            return Some(size);
        };
        if planned > pages || hugged.x > size.x || hugged.y > size.y {
            return Some(size);
        }
        if hugged == size {
            break;
        }
        size = hugged;
    }
    Some(size)
}

/// **The editor opens at the budget, hugged** — the owner's rule, 2026-09-09.
///
/// Cheap enough to stand: two or three panel layouts, no sweep. A card added or a floor raised
/// fails here with the size it should be, rather than leaving the window wasting space.
pub fn is_the_budget_hugged(
    size: egui::Vec2,
    reveal: &dyn Fn(&egui::Context),
    panel: &mut impl FnMut(&mut egui::Ui),
) {
    assert!(
        size.x <= BUDGET.x && size.y <= BUDGET.y,
        "the opening size {size:?} is outside the quarter-4K budget {BUDGET:?}"
    );
    let derived = derive(reveal, panel).expect("the panel plans a page at the budget");
    // **A gutter's worth of tolerance, not an exact match.** The rule is that the window hugs
    // its modules, and a few points either way is a font metric or a reflow rather than a
    // decision — nobody opens an editor and keeps its size anyway. What this catches is real
    // waste, of the kind mxm-mono-01 opened with: a band of bare canvas under the last row.
    const SLACK: f32 = 24.0;
    assert!(
        (derived.x - size.x).abs() <= SLACK && (derived.y - size.y).abs() <= SLACK,
        "the opening size is {size:?}, but the budget hugged is {derived:?}"
    );
}

/// How tall the app bar is: its words are the text painted above this line.
const BAR_HEIGHT: f32 = 44.0;

/// What is wrong with the app bar in a window of `size`, or `None` when its `…` menu is drawn
/// whole and no two of its words are drawn over each other.
pub fn bar_fault(size: egui::Vec2, panel: &mut impl FnMut(&mut egui::Ui)) -> Option<String> {
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
    let input = egui::RawInput {
        screen_rect: Some(screen),
        ..Default::default()
    };
    // Four frames: the fonts settle, and the bar takes its step from the widths it measured on
    // the frame before.
    let mut shapes = Vec::new();
    for _ in 0..4 {
        let mut output = ctx.run_ui(input.clone(), &mut *panel);
        output.textures_delta.clear();
        shapes = output.shapes;
    }
    let words: Vec<(String, egui::Rect, egui::Rect)> = shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => {
                let word = text.galley.text().trim().to_owned();
                let rect = text.visual_bounding_rect();
                (!word.is_empty() && rect.top() < BAR_HEIGHT).then_some((
                    word,
                    rect,
                    clipped.clip_rect.intersect(screen),
                ))
            }
            _ => None,
        })
        .collect();
    let menu_whole = words
        .iter()
        .any(|(word, rect, clip)| word == "…" && clip.expand(0.5).contains_rect(*rect));
    if !menu_whole {
        return Some("its `…` menu is cut off or gone".to_owned());
    }
    for (i, (a, ra, ca)) in words.iter().enumerate() {
        for (b, rb, cb) in words.iter().skip(i + 1) {
            let overlap = ra.intersect(*ca).intersect(rb.intersect(*cb));
            if overlap.width() > 1.0 && overlap.height() > 1.0 {
                return Some(format!("`{a}` is drawn over `{b}`"));
            }
        }
    }
    None
}

/// **The app bar holds in the narrowest window** (the owner, 2026-09-26): its `…` menu whole —
/// design system §3.1's promise, and the door to the presets, the theme and the zoom once the
/// bar has compacted — and nothing drawn over anything else, at `minimum` and in steps above it
/// across the bar's last compact steps.
///
/// §4.3's minimum is one card wide, and a card can be narrower than the bar's last step: then
/// the minimum is the bar's instead. A failure names the narrowest width the bar holds from,
/// which is the minimum to take.
pub fn bar_holds_from_the_minimum(minimum: egui::Vec2, panel: &mut impl FnMut(&mut egui::Ui)) {
    const ABOVE: u32 = 160;
    for extra in (0..=ABOVE).step_by(8) {
        let width = minimum.x + extra as f32;
        if let Some(fault) = bar_fault(egui::vec2(width, minimum.y), panel) {
            let holds = (width.ceil() as u32..width.ceil() as u32 + 1200)
                .step_by(2)
                .find(|&from| {
                    (0..=ABOVE).step_by(8).all(|above| {
                        bar_fault(egui::vec2((from + above) as f32, minimum.y), panel).is_none()
                    })
                });
            panic!(
                "{width} points wide, the app bar fails: {fault}. It holds from {holds:?} \
                 points wide, which is the minimum to take"
            );
        }
    }
}
