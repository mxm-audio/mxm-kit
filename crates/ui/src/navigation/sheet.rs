//! **F1 shows and hides the keys** (the owner, 2026-10-08: the plugins match newDAWn's cheat
//! sheet): a window naming every key of the keyboard language an editor answers to, from the keymap
//! in use, so the sheet never disagrees with the keys. `Escape` closes it too.
//!
//! Only the jobs an editor has are named. MOVE, EXTENT, SELECT, ADD and the rest are newDAWn's, and
//! a key the editor does nothing with stays blank rather than promising something.

use egui::{Context, Key as E, RichText, Ui};
use mxm_keys::{Action, Job, Key, Keymap, Modifier, Step, Verb};

/// The sheet's content, built from a keymap: what the window shows, and what a test reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Sheet {
    pub keymap: String,
    /// The left hand's keys, row by row, with the job each does in an editor ("" for none).
    pub cluster: Vec<Vec<(String, String)>>,
    /// A title, then a key and what it does on each line.
    pub sections: Vec<(&'static str, Vec<(String, &'static str)>)>,
    /// How far one press goes: the step's key, its name, then for a value and for a pitch.
    pub steps: Vec<(String, &'static str, &'static str, &'static str)>,
}

/// The jobs an editor answers to: the ones the cursor and the value keys read.
fn editor_has(job: Job) -> bool {
    matches!(
        job,
        Job::Verb(Verb::Value)
            | Job::Step(_)
            | Job::Action(Action::Delete | Action::Ripple | Action::Open)
            | Job::Out
            | Job::Back
            | Job::View
    )
}

fn label(key: Key) -> String {
    match key {
        Key::Up => "↑".into(),
        Key::Down => "↓".into(),
        Key::Left => "←".into(),
        Key::Right => "→".into(),
        other => other.name().into(),
    }
}

/// The key a job is on, as the keyboard prints it; VIEW names its modifier too.
fn job_key(keymap: &Keymap, job: Job) -> String {
    let mut names: Vec<String> = keymap.keys(job).take(1).map(label).collect();
    if job == Job::View {
        match keymap.view_modifier() {
            Some(Modifier::Shift) => names.push("Shift".into()),
            Some(Modifier::Alt) => names.push("Alt".into()),
            _ => {}
        }
    }
    if names.is_empty() {
        "—".into()
    } else {
        names.join(" or ")
    }
}

/// Every key an editor answers to, from `keymap`.
pub(crate) fn sheet(keymap: &Keymap) -> Sheet {
    const ROWS: [&[Key]; 3] = [
        &[Key::Tab, Key::Q, Key::W, Key::E, Key::R],
        &[Key::CapsLock, Key::A, Key::S, Key::D, Key::F],
        &[Key::Z, Key::X, Key::C, Key::V],
    ];
    let cluster = ROWS
        .iter()
        .map(|row| {
            row.iter()
                .map(|&key| {
                    let job = keymap
                        .job(key)
                        .filter(|&job| editor_has(job))
                        .map(|job| job.name().to_uppercase())
                        .unwrap_or_default();
                    (label(key), job)
                })
                .collect()
        })
        .collect();
    let key = |job| job_key(keymap, job);
    let value = key(Job::Verb(Verb::Value));
    let sizes = format!(
        "{} {} {}",
        key(Job::Step(Step::Coarse)),
        key(Job::Step(Step::Fine)),
        key(Job::Step(Step::Micro))
    );
    let sections = vec![
        (
            "Moving",
            vec![
                (
                    "← ↑ ↓ →".into(),
                    "the next parameter that way; ← → along its row",
                ),
                (
                    format!("{} ← ↑ ↓ →", key(Job::View)),
                    "the next card that way; up from the top ones, the bars above, and back",
                ),
                (
                    key(Job::Action(Action::Open)),
                    "choose the cell the cursor is on, or type a value",
                ),
            ],
        ),
        (
            "Changing a value",
            vec![
                (format!("{sizes} ↑ ↓"), "up or down by that step"),
                (
                    format!("{sizes} ← →"),
                    "to the next round value of that step",
                ),
                (format!("{value} ↑ ↓, ← →"), "the same, a fine step"),
                (key(Job::Action(Action::Delete)), "back to its default"),
                ("Home / End".into(), "its lowest, its highest"),
            ],
        ),
        (
            "Ending a change",
            vec![
                (key(Job::Out), "keep it"),
                (key(Job::Action(Action::Open)), "keep it, open nothing"),
                ("let go".into(), "keep it, when its key was held"),
                (key(Job::Back), "cancel it, a mouse drag too"),
            ],
        ),
        ("This sheet", vec![("F1".into(), "show or hide it")]),
    ];
    let steps = vec![
        (
            key(Job::Step(Step::Fine)),
            "FINE, or none",
            "1 %",
            "a semitone",
        ),
        (key(Job::Step(Step::Coarse)), "COARSE", "10 %", "an octave"),
        (key(Job::Step(Step::Micro)), "MICRO", "0.1 %", "a cent"),
    ];
    Sheet {
        keymap: keymap.name().to_owned(),
        cluster,
        sections,
        steps,
    }
}

/// F1 toggles the sheet and `Escape` closes it; both are taken. Runs before the inert check, so it
/// answers whatever holds the keyboard. Returns whether `Escape` was spent closing it.
pub(super) fn keys(ctx: &Context, open: &mut bool) -> bool {
    ctx.input_mut(|input| {
        if input.consume_key(egui::Modifiers::NONE, E::F1) {
            *open = !*open;
        }
        *open && input.consume_key(egui::Modifiers::NONE, E::Escape) && {
            *open = false;
            true
        }
    })
}

/// Draws the sheet over the editor while it is open; its close button closes it.
pub(super) fn show(ctx: &Context, open: &mut bool, keymap: &Keymap) {
    if !*open {
        return;
    }
    let sheet = sheet(keymap);
    egui::Window::new("Keys")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 48.0))
        .max_height((ctx.content_rect().height() - 96.0).max(160.0))
        .vscroll(true)
        .show(ctx, |ui| draw(ui, &sheet));
}

fn draw(ui: &mut Ui, sheet: &Sheet) {
    let dim = ui.visuals().weak_text_color();
    ui.label(RichText::new(&sheet.keymap).color(dim));
    ui.add_space(4.0);
    for (index, row) in sheet.cluster.iter().enumerate() {
        ui.horizontal(|ui| {
            // Staggered, as on a keyboard.
            ui.add_space([0.0, 10.0, 34.0][index.min(2)]);
            for (key, job) in row {
                let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                let fill = if job.is_empty() {
                    ui.visuals().panel_fill
                } else {
                    ui.visuals().faint_bg_color
                };
                egui::Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(5, 2))
                    .show(ui, |ui| {
                        ui.set_min_width(64.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(key).monospace().size(10.0).color(dim));
                            ui.label(RichText::new(job).size(10.0).strong());
                        });
                    });
            }
        });
    }
    ui.add_space(8.0);
    for (title, hints) in &sheet.sections {
        ui.label(RichText::new(*title).strong());
        egui::Grid::new(*title)
            .num_columns(2)
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                for (keys, does) in hints {
                    ui.label(RichText::new(keys).monospace());
                    ui.label(RichText::new(*does).color(dim));
                    ui.end_row();
                }
            });
        ui.add_space(6.0);
    }
    ui.label(RichText::new("How far one press goes").strong());
    egui::Grid::new("steps")
        .num_columns(4)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.label("");
            ui.label("");
            ui.label(RichText::new("a value").color(dim));
            ui.label(RichText::new("a pitch").color(dim));
            ui.end_row();
            for (key, step, value, pitch) in &sheet.steps {
                ui.label(RichText::new(key).monospace());
                ui.label(RichText::new(*step).strong());
                ui.label(RichText::new(*value).color(dim));
                ui.label(RichText::new(*pitch).color(dim));
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sheet names the keys the default keymap puts each job on, and nothing an editor lacks.
    #[test]
    fn the_sheet_names_the_keymaps_keys_and_only_what_an_editor_has() {
        let sheet = sheet(&Keymap::default());
        let cluster: Vec<&(String, String)> = sheet.cluster.iter().flatten().collect();
        let job_of = |key: &str| {
            cluster
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, job)| job.as_str())
        };
        assert_eq!(job_of("E"), Some("VALUE"));
        assert_eq!(job_of("S"), Some("COARSE"));
        assert_eq!(job_of("F"), Some("MICRO"));
        assert_eq!(job_of("Tab"), Some("OUT"));
        assert_eq!(job_of("C"), Some("VIEW"));
        // MOVE (A) and ADD (V) are newDAWn's: blank here.
        assert_eq!(job_of("A"), Some(""));
        assert_eq!(job_of("V"), Some(""));
        let moving = &sheet.sections[0].1;
        assert!(moving.iter().any(|(keys, _)| keys == "C or Shift ← ↑ ↓ →"));
        let changing = &sheet.sections[1].1;
        assert!(changing.iter().any(|(keys, _)| keys == "S D F ↑ ↓"));
        assert!(changing.iter().any(|(keys, _)| keys == "S D F ← →"));
    }

    /// F1 opens and closes the sheet, `Escape` closes it, and neither reaches anything else.
    #[test]
    fn f1_toggles_the_sheet_and_escape_closes_it() {
        let ctx = egui::Context::default();
        let press = |key| egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut open = false;
        let mut spent = false;
        let frame = |input, open: &mut bool, spent: &mut bool| {
            let mut output = ctx.run_ui(input, |ui| {
                *spent = keys(ui.ctx(), open);
                show(ui.ctx(), open, &Keymap::default());
            });
            output.textures_delta.clear();
        };
        frame(press(E::F1), &mut open, &mut spent);
        assert!(open, "F1 opens it");
        frame(press(E::Escape), &mut open, &mut spent);
        assert!(!open && spent, "Escape closes it, and is spent on that");
        frame(press(E::F1), &mut open, &mut spent);
        frame(press(E::F1), &mut open, &mut spent);
        assert!(!open, "F1 closes it again");
        frame(press(E::Escape), &mut open, &mut spent);
        assert!(!spent, "a closed sheet leaves Escape alone");
    }
}
