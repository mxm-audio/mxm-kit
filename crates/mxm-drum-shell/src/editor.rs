//! The editor's shared pieces: the app bar's Resample and Export samples (the pair, its rows in the
//! bar's `…` menu, the export's status line), and the slot rows' name box. Each plugin's editor
//! places them; their text, sizes and behaviour are the drum machine's, shared since 2026-10-26.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::Ui;
use mxm_ui::space::{MIN_TARGET, SPACE_5};
use mxm_ui::theme::Tokens;
use nice_plug::prelude::{BoolParam, Param, ParamSetter};

use crate::signals::{Export, Signals};
use crate::slot_names::{self, DrumSlots};

/// The rate a sample pack is written at, whatever the host is running.
///
/// Deliberately fixed, and not the host's: a pack is for a tracker or a groovebox, several of
/// which resample anything that is not 44.1 or 48 on import, and a folder whose rate depends on
/// whichever session happened to export it is a trap. `kit.txt` records it either way.
pub const PACK_SAMPLE_RATE: f32 = 48_000.0;
/// The text on the export half of the app bar's Resample pair.
///
/// Named once because the pair is sized from it: the label used to be shorter than the width the
/// pair reserved, and when it grew the button overflowed its allocation, wrapped the app bar and
/// cost the window 335 pt of height. Measuring the string is what stops that happening again.
pub const EXPORT_LABEL: &str = "Export samples";
/// The export's label while its folder dialog is open.
pub const EXPORT_CHOOSING: &str = "Choosing…";
/// The export's help, on the bar's button and on its row in the `…` menu alike.
pub const EXPORT_HELP: &str = "Choose a folder and write this kit into it as numbered 24-bit WAV one-shots, for a tracker or a sampler. Turn Resample on first, so you hear them before they leave.";
/// Why the export cannot be used, on both forms.
pub const EXPORT_DISABLED: &str = "Turn Resample on first: a pack is the samples you are hearing.";
/// Why the export cannot run at all: the capture thread did not start, on both forms.
pub const EXPORT_UNAVAILABLE: &str =
    "Resample cannot run here: the system would not start the thread that records the kit.";
/// Why the export waits, Resample on and its kit not yet installed, on both forms.
pub const EXPORT_CAPTURING: &str =
    "Resample is still recording the kit: a pack is the samples you are hearing.";
/// The pair's rows in the app bar's `…` menu at the bar's last step ([`menu_items`]).
pub const RESAMPLE_ITEM: usize = 0;
pub const EXPORT_ITEM: usize = 1;
/// The floor for each half of the Resample/Export pair, before either label is measured.
const RESAMPLE_MIN_WIDTH: f32 = 86.0;

/// The egui ids a plugin's export keeps its folder pick and its status under, from its name, so
/// two plugins' editors in one host never share one.
#[derive(Debug, Clone, Copy)]
pub struct Ids {
    folder: egui::Id,
    status: egui::Id,
}

impl Ids {
    /// Ids for `plugin` (its bundle name, `mxm-drum-machine`).
    #[must_use]
    pub fn new(plugin: &str) -> Self {
        Self {
            folder: egui::Id::new(format!("{plugin}-pack-folder")),
            status: egui::Id::new(format!("{plugin}-export-status")),
        }
    }
}

/// What the app bar's Resample and Export are drawn from, a frame at a time.
pub struct Bar<'a> {
    /// The plugin's `resample` parameter.
    pub resample: &'a BoolParam,
    /// Resample's help, on the bar's toggle and its row in the `…` menu alike: what stays live
    /// while frozen is each plugin's.
    pub help: &'static str,
    /// The keyboard cursor's card for the toggle, outside the paging keys.
    pub card: u64,
    /// The plugin's shared signals.
    pub signals: &'a Signals,
    /// Asks the capture thread for a pack at a rate in a folder; `None` while the editor has no
    /// way to it.
    pub export: Option<&'a dyn Fn(f32, PathBuf)>,
    /// The export's ids.
    pub ids: Ids,
}

/// Why Export cannot be used now, or `None`: the capture thread missing, Resample off, or on with
/// its kit not yet installed (`Signals::frozen`), when the parameters may not be what will play.
#[must_use]
pub fn export_blocked(bar: &Bar<'_>) -> Option<&'static str> {
    if bar.signals.capture_unavailable() {
        Some(EXPORT_UNAVAILABLE)
    } else if !bar.resample.value() {
        Some(EXPORT_DISABLED)
    } else if bar.signals.frozen().is_none() {
        Some(EXPORT_CAPTURING)
    } else {
        None
    }
}

/// How wide each half of the app-bar pair is drawn.
///
/// The widest of the two labels wins, so the two always match and neither ever overflows. It is
/// measured rather than guessed because mxm-kit's `crates/ui` (*The app bar*, in its `AGENTS.md`
/// and `NOTES.md`) requires a bar control to reserve its widest form — an under-reserved one does
/// not just look wrong, it wraps the whole bar.
#[must_use]
pub fn pair_width(ui: &Ui, bar: &Bar<'_>) -> f32 {
    let button = egui::TextStyle::Button.resolve(ui.style());
    let export = ui
        .painter()
        .layout_no_wrap(EXPORT_LABEL.to_owned(), button, egui::Color32::PLACEHOLDER)
        .size()
        .x;
    mxm_ui::control::toggle_min_width(ui, Param::name(bar.resample))
        .max(export + SPACE_5 * 2.0)
        .max(RESAMPLE_MIN_WIDTH)
}

/// The app bar's Resample toggle and Export button, side by side at one width, and the export's
/// status under them.
///
/// The pair opens its own left-to-right region inside the bar's right-to-left group, and reserves
/// a fixed width so nothing downstream jitters as the status text changes. The status is a
/// bar-width label rather than the card's line, because the bar is one row high.
pub fn pair(ui: &mut Ui, tokens: &Tokens, bar: &Bar<'_>, setter: &ParamSetter<'_>) {
    let width = pair_width(ui, bar);
    ui.allocate_ui_with_layout(
        egui::vec2(width * 2.0 + ui.spacing().item_spacing.x, MIN_TARGET),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            mxm_ui::navigation::bar_card(ui, bar.card, |ui| {
                // The pair shares one width: the toggle is at least the Export button's.
                mxm_ui::control::toggle_stack(ui, width, |ui| {
                    ui.scope(|ui| {
                        mxm_params::binding::toggle(
                            ui,
                            tokens,
                            "resample",
                            bar.resample,
                            bar.help,
                            setter,
                        );
                    })
                    .response
                    .rect
                })
            });
            export_button(ui, bar, width);
        },
    );
    export_status(ui, tokens, bar);
}

/// The pair's rows in the app bar's `…` menu, when the bar is too narrow for it: the same names,
/// the same enabled state and the same help as the pair drawn on the bar.
#[must_use]
pub fn menu_items(ctx: &egui::Context, bar: &Bar<'_>) -> Vec<mxm_ui::shell::MenuItem> {
    use mxm_ui::shell::{MenuItem, MenuKind};
    let engaged = bar.resample.value();
    let picking = mxm_ui::offthread::running::<PathBuf>(ctx, bar.ids.folder);
    let mut items = vec![
        MenuItem {
            label: Param::name(bar.resample).to_owned(),
            hover: bar.help.to_owned(),
            kind: MenuKind::Toggle(engaged),
            disabled: None,
        },
        MenuItem {
            label: (if picking {
                EXPORT_CHOOSING
            } else {
                EXPORT_LABEL
            })
            .to_owned(),
            hover: EXPORT_HELP.to_owned(),
            kind: MenuKind::Action,
            disabled: {
                let blocked = export_blocked(bar);
                (blocked.is_some() || bar.export.is_none() || picking)
                    .then(|| blocked.unwrap_or(EXPORT_DISABLED).to_owned())
            },
        },
    ];
    let status = export_status_text(ctx, bar);
    if !status.is_empty() {
        items.push(MenuItem {
            label: status,
            hover: String::new(),
            kind: MenuKind::Note,
            disabled: None,
        });
    }
    items
}

fn export_button(ui: &mut Ui, bar: &Bar<'_>, width: f32) {
    // **"Export samples", not "Export pack"** (owner, 2026-09-22, and their original wording).
    // The preset browser's footer already has an `Export…` that packs *presets* into a bank file,
    // a few centimetres away in the same window; two buttons reading "Export" that do unrelated
    // things is a trap. "Samples" is the word that separates them.
    let blocked = export_blocked(bar);
    let picking = mxm_ui::offthread::running::<PathBuf>(ui.ctx(), bar.ids.folder);
    let button = ui
        .add_enabled(
            blocked.is_none() && bar.export.is_some() && !picking,
            egui::Button::new(if picking {
                EXPORT_CHOOSING
            } else {
                EXPORT_LABEL
            })
            .min_size(egui::vec2(width, MIN_TARGET)),
        )
        .on_hover_text(EXPORT_HELP)
        .on_disabled_hover_text(blocked.unwrap_or(EXPORT_DISABLED));
    if button.clicked() {
        start_export(ui.ctx(), bar);
    }
}

/// Asks where to write the pack, off the frame: the bar's button and the `…` menu's row
/// ([`EXPORT_ITEM`]).
///
/// A pack's natural destination is removable media — an SD card for a tracker — so the export asks
/// where (owner, 2026-09-21). The dialog is **blocking**, and blocking an editor frame hangs the
/// host, so `mxm_ui::offthread` runs it on its own thread and [`collect_export`] takes the answer on
/// a later frame. That is the pattern the sampler and the convolution already use for their file
/// picks; a *folder* pick is the collection's first, and its Linux portal path is unverified.
pub fn start_export(ctx: &egui::Context, bar: &Bar<'_>) {
    let start = crate::pack::root();
    mxm_ui::offthread::start(ctx, bar.ids.folder, move || {
        let mut dialog = rfd::FileDialog::new().set_title("Write the sample pack into");
        // Opening on the default pack folder makes "the usual place" one more click rather
        // than a navigation, without taking the choice away.
        if let Some(start) = start.filter(|path| path.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        dialog.pick_folder()
    });
}

/// Hands a chosen folder to the capture thread, once a frame.
pub fn collect_export(ctx: &egui::Context, bar: &Bar<'_>) {
    if let Some(chosen) = mxm_ui::offthread::take::<PathBuf>(ctx, bar.ids.folder)
        && let Some(export) = bar.export
    {
        export(PACK_SAMPLE_RATE, chosen);
    }
}

fn export_status(ui: &mut Ui, tokens: &Tokens, bar: &Bar<'_>) {
    let text = export_status_text(ui.ctx(), bar);
    ui.label(egui::RichText::new(text).color(tokens.text_secondary));
}

/// The last export's outcome in words, kept across frames once it arrives.
#[must_use]
pub fn export_status_text(ctx: &egui::Context, bar: &Bar<'_>) -> String {
    if let Some(outcome) = bar.signals.take_export() {
        ctx.memory_mut(|memory| memory.data.insert_temp(bar.ids.status, outcome));
    }
    let status = ctx.memory(|memory| memory.data.get_temp::<Export>(bar.ids.status));
    match status {
        Some(Export::Written { files, clipped: 0 }) => format!("Wrote {files} samples"),
        Some(Export::Written { files, clipped }) => {
            format!("Wrote {files} samples, {clipped} clipped")
        }
        Some(Export::NoDestination) => "No folder to write to on this system".to_owned(),
        Some(Export::Failed(why)) => format!("Could not write: {why}"),
        // Reserved rather than absent: a line that appears only after an export would move the
        // card's floor, which the paging tests measure.
        None => String::new(),
    }
}

/// The name box's width: about eight letters of the button's text, measured, so a name like
/// `subkick` shows whole and a longer one is cut (its hover shows it all).
#[must_use]
pub fn name_width(ui: &Ui) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let letters = ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap("oooooooo".to_owned(), font, egui::Color32::PLACEHOLDER)
            .rect
            .width()
    });
    letters + 2.0 * ui.spacing().button_padding.x
}

/// A slot's name box, first in its row: its name (its number when it has none), a cursor stop at
/// `key`; double-click or OPEN types a name, Enter keeps it, Escape (BACK) cancels. `entries` is
/// the editor's typed-text state, `None` while a box is closed. Returns the closed box's response,
/// or `None` while it is being typed in.
pub fn name_box(
    ui: &mut Ui,
    slots: &impl DrumSlots,
    key: &'static str,
    index: usize,
    entries: &mut HashMap<&'static str, Option<String>>,
    text_editing: bool,
) -> Option<egui::Response> {
    let size = egui::vec2(name_width(ui), MIN_TARGET);
    if let Some(Some(buffer)) = entries.get_mut(key) {
        let id = ui.make_persistent_id(key);
        let response = ui.add_sized(
            size,
            egui::TextEdit::singleline(buffer)
                .id(id)
                .char_limit(slot_names::NAME_CHARS),
        );
        // The frame it opens in, it takes the keyboard with the whole name selected, so typing
        // replaces it.
        if ui
            .ctx()
            .data_mut(|d| d.remove_temp::<bool>(id.with("opened")))
            .is_some()
        {
            response.request_focus();
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            let end = egui::text::CCursor::new(buffer.chars().count());
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    end,
                )));
            state.store(ui.ctx(), id);
        } else if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            entries.insert(key, None);
        } else if response.lost_focus() {
            let typed = buffer.clone();
            slot_names::set(slots, index, &typed);
            entries.insert(key, None);
        }
        return None;
    }

    let shown = slot_names::shown(slots)[index].clone();
    let text = shown.clone().unwrap_or_else(|| format!("{:02}", index + 1));
    let response = mxm_ui::navigation::at(ui, key, |ui| {
        let response = ui
            .add_enabled_ui(!text_editing, |ui| {
                ui.add_sized(size, egui::Button::new(text).frame(false).truncate())
            })
            .inner;
        mxm_ui::navigation::mark(ui, &response, response.rect);
        response
    });
    let response = response.on_hover_text(name_hover(slots, index, shown.as_deref()));
    let open =
        response.double_clicked() || (!text_editing && mxm_ui::navigation::take_type(ui, key));
    if open && !text_editing {
        let id = ui.make_persistent_id(key);
        ui.ctx()
            .data_mut(|d| d.insert_temp(id.with("opened"), true));
        entries.insert(key, Some(shown.unwrap_or_default()));
    }
    Some(response)
}

/// The name box's hover: the whole name, the slot, and its note or channel.
#[must_use]
pub fn name_hover<S: DrumSlots>(slots: &S, index: usize, shown: Option<&str>) -> String {
    let slot = format!("Slot {:02}", index + 1);
    let plays = match mxm_part_routing::PartAssignment::from_parameter(slots.midi_channel(index)) {
        mxm_part_routing::PartAssignment::FixedNote => {
            format!("note {}", u16::from(S::FIRST_KEY) + index as u16)
        }
        mxm_part_routing::PartAssignment::Channel(channel) => {
            format!("every note on Ch {}", channel + 1)
        }
    };
    let name = shown.map_or_else(String::new, |name| format!("{name}\n"));
    format!("{name}{slot}, {plays}. Double-click to name it.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot_names::tests::Slots;
    use nice_plug::params::InternalParamMut;

    /// Export waits for the kit heard and says why: off, still recording, ready, and a capture
    /// thread that never started.
    #[test]
    fn export_waits_for_the_kit_heard_and_says_why() {
        let resample = BoolParam::new("Resample", false);
        let signals = Signals::new();
        let bar = |resample: &BoolParam| -> Option<&'static str> {
            export_blocked(&Bar {
                resample,
                help: "",
                card: 65,
                signals: &signals,
                export: None,
                ids: Ids::new("mxm-test-drums"),
            })
        };
        assert_eq!(bar(&resample), Some(EXPORT_DISABLED));
        // SAFETY: exclusive test ownership; no process or GUI thread exists.
        unsafe { resample._internal_set_plain_value(true) };
        assert_eq!(bar(&resample), Some(EXPORT_CAPTURING));
        signals.set_frozen(Some(1));
        assert_eq!(bar(&resample), None);
        signals.set_capture_unavailable(true);
        assert_eq!(bar(&resample), Some(EXPORT_UNAVAILABLE));
    }

    #[test]
    fn the_hover_says_the_name_the_slot_and_what_it_plays() {
        let mut slots = Slots::new();
        assert_eq!(
            name_hover(&slots, 2, Some("sn2")),
            "sn2\nSlot 03, note 38. Double-click to name it."
        );
        slots.channels[2] = 2;
        assert_eq!(
            name_hover(&slots, 2, None),
            "Slot 03, every note on Ch 2. Double-click to name it."
        );
    }

    #[test]
    fn two_plugins_keep_their_export_apart() {
        let (a, b) = (Ids::new("mxm-drum-machine"), Ids::new("mxm-model-drums"));
        assert_ne!(a.folder, b.folder);
        assert_ne!(a.status, b.status);
        assert_ne!(a.folder, a.status);
    }
}
