//! What a drag-resize actually costs `flow::cards`, measured rather than argued.
//!
//! # The two suspects
//!
//! An editor was reported as *choppy while resizing, like a low framerate*, and two attempts at a
//! fix moved scroll areas around without helping. There are two costs those attempts did not touch,
//! and this bench separates them.
//!
//! **1. Every resize frame is drawn twice.** `egui_taffy` recomputes the tree and calls
//! `Context::request_discard` whenever `taffy.dirty(root) || last_size != root_rect.size()`
//! (`egui_taffy-0.14.0/src/lib.rs:632`). The root rect is `Ui::available_rect_before_wrap`, so
//! every frame of a drag changes it and every frame is discarded and run again. The `passes` column
//! is that claim, true or false.
//!
//! **2. Every card is drawn, on screen or not.** `flow::leaf` calls the body closure for all ten
//! cards on every pass; immediate mode has no viewport culling unless you write it, which is what
//! `ScrollArea::show_rows` exists for. At a narrow width ten cards wrap into many rows and two are
//! visible, so this can be the larger cost by far. The `draws` column is that one.
//!
//! Both are multipliers on the same work, so they compound: twice the passes times five times the
//! cards is ten times the necessary layout.
//!
//! # Running it
//!
//! `#[ignore]`d, because it is a measurement and not an assertion; a bench that fails CI on a busy
//! machine teaches nothing. **Release, or the numbers mean nothing.**
//!
//! ```text
//! cargo test -p mxm-ui --release --test flow_resize_bench -- --ignored --nocapture
//! ```
//!
//! No window, no GPU and no plugin: it drives `egui::Context::run_ui` directly, so it is cheap to
//! build and safe to run beside anything else.
//!
//! # What it does not tell you
//!
//! The card bodies here are representative in *shape* — four knob columns and two caption lines —
//! but they paint far less than a real control does. The absolute milliseconds are therefore a
//! **floor**, well under a real editor's, and the ratios between rows are the result.

use std::cell::Cell;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use mxm_ui::flow::{self, Card, Options, Scroll};

/// Ten cards with floors in the range the shipped editors declare (§4.3), and the group shape
/// mono-00 uses: a lone card, then pairs a row break may not fall inside.
const CARDS: &[Card<'static>] = &[
    Card::new("Voice", 248.0),
    Card::new("Modulator 1", 288.0),
    Card::new("Modulator 2", 288.0),
    Card::new("Oscillator 1", 284.0),
    Card::new("Oscillator 2", 284.0),
    Card::new("Mixer", 248.0),
    Card::new("Filter", 264.0),
    Card::new("Envelope (VCF)", 248.0),
    Card::new("Amplifier", 248.0),
    Card::new("Envelope (VCA)", 248.0),
];

const GROUPS: &[&[usize]] = &[&[0], &[1, 2], &[3, 4], &[5], &[6, 7], &[8, 9]];
const CEILING: f32 = 480.0;

/// The window height the sweep holds constant, so only the width is moving. Deliberately shorter
/// than the content gets at narrow widths — that is where culling has anything to skip, and it is
/// the case a tiling manager produces.
const HEIGHT: f32 = 900.0;
/// The drag: from a wide window down to one card wide, one point per frame.
const WIDEST: f32 = 1600.0;
const NARROWEST: f32 = 360.0;
/// Frames drawn before measuring, so the tree exists and nothing is built for the first time.
const WARMUP: usize = 40;

/// One card's body: the shape of a real one, without a real one's paint cost.
fn draw_body(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for _ in 0..4 {
            ui.vertical(|ui| {
                ui.label("Cutoff");
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
                ui.painter()
                    .circle_stroke(rect.center(), 24.0, (1.0, egui::Color32::GRAY));
                ui.label("1.2 kHz");
            });
        }
    });
    ui.label("EXT CV IN: nothing (default)");
}

struct Stats {
    label: &'static str,
    frames: u64,
    passes: u64,
    /// Card bodies actually drawn, summed over the sweep. Ten cards × the passes is the ceiling.
    draws: u64,
    times: Vec<Duration>,
}

impl Stats {
    fn per_frame(&self, total: u64) -> f64 {
        total as f64 / self.frames.max(1) as f64
    }
    fn mean_ms(&self) -> f64 {
        self.times.iter().map(Duration::as_secs_f64).sum::<f64>() / self.times.len().max(1) as f64
            * 1000.0
    }
    fn p95_ms(&self) -> f64 {
        let mut sorted = self.times.clone();
        sorted.sort_unstable();
        let index = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len().saturating_sub(1));
        sorted.get(index).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    }
    fn total_ms(&self) -> f64 {
        self.times.iter().map(Duration::as_secs_f64).sum::<f64>() * 1000.0
    }
}

/// Drags the window from `WIDEST` to `NARROWEST`, one point per frame, recording what each frame
/// cost in passes, card-body draws and wall time.
fn sweep(label: &'static str, options: Options) -> Stats {
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    // The cap a shipped editor runs under. Raising it here would measure a machine none of them is.
    ctx.options_mut(|o| {
        o.max_passes = NonZeroUsize::new(2).expect("2 is not zero");
    });

    let id = egui::Id::new(("flow-resize-bench", label));
    let frame = |width: f32| -> (u64, u64, Duration) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, HEIGHT),
            )),
            ..Default::default()
        };
        let drawn = Cell::new(0_u64);
        let before = ctx.cumulative_pass_nr();
        let start = Instant::now();
        let mut output = ctx.run_ui(input, |ui| {
            let tokens = mxm_ui::theme::tokens(ui.ctx());
            let mut body = |ui: &mut egui::Ui, _index: usize| {
                drawn.set(drawn.get() + 1);
                draw_body(ui);
            };
            flow::cards_with(
                ui,
                tokens,
                id,
                CARDS,
                GROUPS,
                Some(CEILING),
                options,
                &mut body,
            );
        });
        let elapsed = start.elapsed();
        // Textures accumulate across frames otherwise, and that is not what is being measured.
        output.textures_delta.clear();
        (ctx.cumulative_pass_nr() - before, drawn.get(), elapsed)
    };

    for _ in 0..WARMUP {
        frame(WIDEST);
    }

    let steps = (WIDEST - NARROWEST) as usize;
    let mut stats = Stats {
        label,
        frames: 0,
        passes: 0,
        draws: 0,
        times: Vec::with_capacity(steps),
    };
    for step in 0..steps {
        let (passes, draws, elapsed) = frame(WIDEST - step as f32);
        stats.frames += 1;
        stats.passes += passes;
        stats.draws += draws;
        stats.times.push(elapsed);
    }
    stats
}

#[test]
#[ignore = "a measurement, not an assertion: run it by hand with --nocapture"]
fn what_a_resize_costs() {
    let shipped = Options::default();
    let runs = [
        sweep("shipped: vertical, all cards", shipped),
        sweep(
            "the factory's fix: both axes",
            Options {
                scroll: Scroll::Both,
                ..shipped
            },
        ),
        sweep(
            "+ 32pt quantum (fewer relayouts)",
            Options {
                quantum: Some(32.0),
                ..shipped
            },
        ),
        sweep(
            "+ cull offscreen cards",
            Options {
                cull: true,
                ..shipped
            },
        ),
        sweep(
            "+ both: quantum and cull",
            Options {
                quantum: Some(32.0),
                cull: true,
                ..shipped
            },
        ),
    ];

    println!();
    println!(
        "A {}-frame drag from {WIDEST:.0} to {NARROWEST:.0} points in a {HEIGHT:.0}-point window.",
        runs[0].frames
    );
    println!(
        "{} cards. max_passes 2. Card bodies are representative in shape only, so ms is a floor.",
        CARDS.len()
    );
    println!();
    println!(
        "{:<34} {:>7} {:>8} {:>9} {:>8} {:>9}",
        "strategy", "passes", "draws", "mean ms", "p95 ms", "total ms"
    );
    println!("{}", "-".repeat(80));
    for run in &runs {
        println!(
            "{:<34} {:>7.2} {:>8.1} {:>9.3} {:>8.3} {:>9.0}",
            run.label,
            run.per_frame(run.passes),
            run.per_frame(run.draws),
            run.mean_ms(),
            run.p95_ms(),
            run.total_ms()
        );
    }

    let base = &runs[0];
    println!();
    println!("Against the shipped row:");
    for run in runs.iter().skip(1) {
        let ratio = run.total_ms() / base.total_ms().max(f64::MIN_POSITIVE);
        let verdict = if ratio < 0.95 {
            "faster"
        } else if ratio > 1.05 {
            "SLOWER"
        } else {
            "no change"
        };
        println!("  {:<34} {:.2}x  {verdict}", run.label, ratio);
    }
    println!();
    println!("Reading it:");
    println!("  passes  2.00 means every frame of the drag was thrown away and run again.");
    println!("  draws   card bodies per frame. 20.0 is ten cards twice; the floor is what fits.");
    println!("  If 'both axes' is SLOWER, the scrollbars are fighting the layout.");
    println!();
}

/// Lays the cards out at a width narrow enough that most of them fall outside the viewport, and
/// returns where each one landed.
fn settled(options: Options) -> Vec<egui::Rect> {
    let ctx = egui::Context::default();
    mxm_ui::theme::apply(&ctx);
    mxm_ui::typography::apply(&ctx);
    let id = egui::Id::new("flow-cull-equivalence");
    for _ in 0..8 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                let tokens = mxm_ui::theme::tokens(ui.ctx());
                let mut body = |ui: &mut egui::Ui, _index: usize| draw_body(ui);
                flow::cards_with(
                    ui,
                    tokens,
                    id,
                    CARDS,
                    GROUPS,
                    Some(CEILING),
                    options,
                    &mut body,
                );
            },
        );
        output.textures_delta.clear();
    }
    flow::drawn(&ctx, id, CARDS.len())
        .into_iter()
        .map(|rect| rect.expect("every card records where it landed, culled or not"))
        .collect()
}

/// **Culling must not move anything.** A card that is skipped still records where it would have
/// been, so `flow::drawn` — which every editor's layout test reads — says the same thing either
/// way. If this fails, the option is not safe to ship behind a measurement.
#[test]
fn culling_does_not_move_a_card() {
    let drawn = settled(Options::default());
    let culled = settled(Options {
        cull: true,
        ..Options::default()
    });

    for (index, (a, b)) in drawn.iter().zip(&culled).enumerate() {
        let title = CARDS[index].title;
        assert!(
            (a.min.x - b.min.x).abs() < 0.5 && (a.min.y - b.min.y).abs() < 0.5,
            "{title} starts at {:?} drawn and {:?} culled",
            a.min,
            b.min
        );
        assert!(
            (a.width() - b.width()).abs() < 0.5 && (a.height() - b.height()).abs() < 0.5,
            "{title} is {:?} drawn and {:?} culled",
            a.size(),
            b.size()
        );
    }
}
