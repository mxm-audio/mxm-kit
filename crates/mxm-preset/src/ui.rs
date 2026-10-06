//! The app bar's preset controls, §3.1 slots 2 to 4, the browser they open, and what they do.
//!
//! `mxm-ui` draws the bar's controls and the browser's panes and reports what a person did; this
//! module is the caller that knows about files, banks, parameters and hosts. It was the same code
//! in every editor, and now it is one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use egui::Ui;
use mxm_ui::browser::{BrowserAction, BrowserRow, BrowserState, Pane, PaneRow};
use mxm_ui::space::{HAIRLINE, MIN_TARGET, RADIUS, SPACE_2, SPACE_3, SPACE_4, SPACE_5};
use mxm_ui::theme::Tokens;
use nice_plug::prelude::ParamSetter;

use crate::identity::mark_loaded_with_baseline;
use crate::{
    Bank, Category, Entry, ImportRefused, Instrument, Library, Origin, Preset, favourite_key,
    is_favourite, loaded, mark_loaded, mark_none, open_folder, read_favourites, snapshot,
    write_favourites,
};

/// What the preset controls need between frames.
///
/// **The library is built once**, not on every frame: `dirs::config_dir()` is a syscall, and — more
/// to the point — holding the root in a field is what lets a test drive it somewhere harmless.
pub struct PresetUi {
    library: Library,
    /// Rebuilt when something changes it, not every frame: listing means reading a directory, and
    /// doing that at frame rate inside a DAW is a syscall per frame for a list that rarely moves.
    entries: Vec<Entry>,
    /// The banks the entries came from, listed with them.
    banks: Vec<Bank>,
    /// An open Save As, Rename, New bank or Export, and what is being typed into it.
    naming: Option<Naming>,
    /// The last thing that went wrong, shown until something else happens.
    problem: Option<String>,
    /// The last thing that went right and is worth saying — where an export went.
    notice: Option<String>,
    /// Names the person has starred. **Per-person state, beside the presets rather than inside
    /// them** — a preset copied to another machine must not carry somebody else's stars.
    favourites: BTreeSet<String>,
    /// The browser under the bar, and what it is filtering by.
    browser_open: bool,
    /// Which of its panes the keyboard is in.
    pane_focus: Pane,
    /// Scroll the browser's list to the loaded preset on the next frame — after the arrows moved.
    scroll_pending: bool,
    /// `None` is every bank.
    bank_filter: Option<Origin>,
    category_filter: CategoryFilter,
    search: String,
    /// A question the browser is asking, over its list.
    picker: Option<Picker>,
    /// Where the browser was drawn last frame, for the naming row to sit inside it.
    overlay: Option<egui::Rect>,
}

/// A preset operation handed to an opt-in consumer whose durable content must be prepared before
/// any part of the patch becomes visible.
#[derive(Clone, Debug, PartialEq)]
pub enum DeferredPresetUiRequest {
    Load {
        name: String,
        origin: Origin,
        preset: Preset,
    },
    Init,
}

/// Per-frame adapter for an opt-in deferred preset consumer.
///
/// The shared browser still owns navigation, files and presentation. The consumer owns only the
/// asynchronous preparation/publication transaction. While one is pending Save and Save As are
/// disabled, but another selection or Init remains available and may supersede it.
pub struct DeferredPresetUi<'a> {
    pub pending: bool,
    pub request: &'a mut dyn FnMut(DeferredPresetUiRequest) -> Result<(), String>,
}

/// Why an export of nothing but presets found in a folder writes nothing.
const FOUND_NOT_EXPORTED: &str = "presets found in a folder are not exported: they name a file there, so share the folder instead";

/// The categories pane's rows: everything, the starred, one category, or one folder a found
/// factory preset came from ([`crate::Found`]).
#[derive(Clone, Debug, PartialEq, Eq)]
enum CategoryFilter {
    All,
    Favourites,
    Category(Category),
    Group(String),
}

/// A question with a few answers, over the browser's list.
enum Picker {
    /// Which bank file in the banks folder to import.
    Import(Vec<PathBuf>),
    /// A bank of that name is already here: replace it, or keep it.
    Replace(PathBuf, String),
}

/// A name being typed, and what will be done with it.
struct Naming {
    kind: NamingKind,
    name: String,
    /// Save As asks for one; Rename keeps the preset's own.
    category: Category,
    /// Where a saved file goes: the loaded preset's bank, or the user's own presets.
    origin: Origin,
    /// Set when the name would replace an existing preset. **Asks rather than overwriting or
    /// silently suffixing**: a preset is something somebody made, and losing it to a name clash is
    /// not a rounding error.
    collides: bool,
    /// Give the field focus on its first frame, so typing goes into it and not the search box.
    focus: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum NamingKind {
    SaveAs,
    Rename,
    NewBank,
    Export,
}

impl NamingKind {
    const fn prompt(self) -> &'static str {
        match self {
            NamingKind::SaveAs => "Save as",
            NamingKind::Rename => "Rename to",
            NamingKind::NewBank => "New bank",
            NamingKind::Export => "Export as",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            NamingKind::SaveAs | NamingKind::Rename => "Save",
            NamingKind::NewBank => "Make",
            NamingKind::Export => "Export",
        }
    }
}

impl PresetUi {
    /// Rooted in the real config directory, under the instrument's own id.
    pub fn new(instrument: &dyn Instrument) -> Self {
        Self::at(Library::at_config_dir(instrument.clap_id()), instrument)
    }

    /// Rooted wherever the caller says — **a test must never reach the real config directory**, and
    /// `Library::at(None)` is a library that reads and writes nothing at all.
    pub fn at(library: Library, instrument: &dyn Instrument) -> Self {
        let entries = library.list(instrument);
        let banks = library.banks();
        let favourites = read_favourites(&library);
        Self {
            library,
            entries,
            banks,
            naming: None,
            problem: None,
            notice: None,
            favourites,
            browser_open: false,
            pane_focus: Pane::Presets,
            scroll_pending: false,
            bank_filter: None,
            category_filter: CategoryFilter::All,
            search: String::new(),
            picker: None,
            overlay: None,
        }
    }

    fn refresh(&mut self, instrument: &dyn Instrument) {
        self.entries = self.library.list(instrument);
        self.banks = self.library.banks();
    }

    /// Indices into the entries, **starred first**.
    ///
    /// A stable partition rather than a sort: within each group the library's own order is kept —
    /// factory, then user, then each bank, by name — so starring one preset moves that one and
    /// leaves the rest where they were.
    pub fn order(&self) -> Vec<usize> {
        self.starred_first((0..self.entries.len()).collect())
    }

    fn starred_first(&self, indices: Vec<usize>) -> Vec<usize> {
        let (starred, rest): (Vec<usize>, Vec<usize>) = indices.into_iter().partition(|index| {
            let entry = &self.entries[*index];
            is_favourite(&self.favourites, &entry.name, &entry.origin)
        });
        starred.into_iter().chain(rest).collect()
    }

    /// The entries the browser's filters let through, starred first — what the list shows and
    /// what the bar's arrows step through.
    pub fn visible(&self) -> Vec<usize> {
        let indices = (0..self.entries.len())
            .filter(|index| self.passes(&self.entries[*index], true, true))
            .collect();
        self.starred_first(indices)
    }

    /// Whether an entry passes the filters — the bank's and the category's each only when asked,
    /// so a pane can count what the *other* pane's choice leaves.
    fn passes(&self, entry: &Entry, by_bank: bool, by_category: bool) -> bool {
        if by_bank
            && let Some(bank) = &self.bank_filter
            && &entry.origin != bank
        {
            return false;
        }
        if by_category && !self.in_category(entry, &self.category_filter) {
            return false;
        }
        if !self.search.trim().is_empty() {
            let needle = self.search.trim().to_lowercase();
            if !entry.name.to_lowercase().contains(&needle) {
                return false;
            }
        }
        true
    }

    /// What an export of the visible list writes, and how many presets found in a folder it leaves
    /// out ([`Entry::found`]): those name a file where they were found, and a bank has to load on a
    /// machine without that folder.
    fn exportable(&self) -> (Vec<Preset>, usize) {
        let visible = self.visible();
        let left_out = visible
            .iter()
            .filter(|index| self.entries[**index].found)
            .count();
        let chosen = visible
            .iter()
            .map(|index| &self.entries[*index])
            .filter(|entry| !entry.found)
            .filter_map(|entry| entry.preset.clone())
            .collect();
        (chosen, left_out)
    }

    /// Whether an entry is in one of the categories pane's rows. **A preset from a folder is filed
    /// under its folder only**, not also under its [`Category`], so a hundred rooms do not all
    /// land in *FX* as well.
    fn in_category(&self, entry: &Entry, filter: &CategoryFilter) -> bool {
        match filter {
            CategoryFilter::All => true,
            CategoryFilter::Favourites => {
                is_favourite(&self.favourites, &entry.name, &entry.origin)
            }
            CategoryFilter::Category(category) => {
                entry.group.is_none()
                    && entry
                        .preset
                        .as_ref()
                        .is_some_and(|preset| preset.category == *category)
            }
            CategoryFilter::Group(group) => entry.group.as_deref() == Some(group.as_str()),
        }
    }

    /// The banks pane's rows: everything, the factory, then each bank as the library lists them.
    fn bank_rows(&self) -> Vec<Option<Origin>> {
        let mut rows = vec![None, Some(Origin::Factory)];
        rows.extend(self.banks.iter().map(|bank| Some(bank.origin.clone())));
        rows
    }

    /// The categories pane's rows: everything, the starred, each category that has a preset in the
    /// selected bank, then each folder found presets came from, in name order.
    fn category_rows(&self) -> Vec<CategoryFilter> {
        let mut rows = vec![CategoryFilter::All, CategoryFilter::Favourites];
        for category in Category::ALL {
            let row = CategoryFilter::Category(category);
            let present = self
                .entries
                .iter()
                .any(|entry| self.passes(entry, true, false) && self.in_category(entry, &row));
            if present {
                rows.push(row);
            }
        }
        let groups: std::collections::BTreeSet<&str> = self
            .entries
            .iter()
            .filter(|entry| self.passes(entry, true, false))
            .filter_map(|entry| entry.group.as_deref())
            .collect();
        rows.extend(
            groups
                .into_iter()
                .map(|group| CategoryFilter::Group(group.to_owned())),
        );
        rows
    }

    /// Everything the browser lists, in the library's order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The starred names, to change — a test's way of starring without a bar to click.
    pub fn favourites_mut(&mut self) -> &mut BTreeSet<String> {
        &mut self.favourites
    }

    /// Opens Save As with `name` filled in, as the bar's button does.
    pub fn open_save_as(&mut self, name: &str) {
        self.naming = Some(Naming {
            kind: NamingKind::SaveAs,
            name: name.to_owned(),
            category: Category::Uncategorised,
            origin: Origin::User,
            collides: false,
            focus: true,
        });
    }

    /// Whether one of this widget's own surfaces has the keyboard.
    ///
    /// **The editors used to spell this out**, each naming the two states an open browser and an
    /// open naming row happen to be in — twelve panels reciting the internals of a widget that
    /// knows them. The sampler made that thirteen. What a panel actually asks is whether the preset UI is holding the keyboard,
    /// because that is what suspends the paging renderer and the keyboard cursor together.
    /// A value being typed into a *control* is the editor's own business and is not this.
    pub fn holds_the_keyboard(&self) -> bool {
        self.is_naming() || self.is_browser_open()
    }

    /// Whether Save As, Rename, New bank or Export is open.
    pub fn is_naming(&self) -> bool {
        self.naming.is_some()
    }

    /// Opens or closes the browser, as the name in the bar does — and as the developer channel
    /// does, so a script can look at it without a mouse.
    pub fn set_browser_open(&mut self, open: bool) {
        if open && !self.browser_open {
            self.pane_focus = Pane::Presets;
        }
        self.browser_open = open;
        if !open {
            self.picker = None;
        }
    }

    pub fn is_browser_open(&self) -> bool {
        self.browser_open
    }

    /// The category of the loaded preset, if one is loaded and readable.
    fn category_of_loaded(&self, instrument: &dyn Instrument) -> Category {
        let loaded = loaded(instrument);
        self.entries
            .iter()
            .find(|entry| {
                Some(entry.name.as_str()) == loaded.name() && Some(&entry.origin) == loaded.origin()
            })
            .and_then(|entry| entry.preset.as_ref())
            .map_or(Category::Uncategorised, |preset| preset.category)
    }

    /// Where a Save As from the current patch goes: the loaded preset's bank, else the bank the
    /// browser is looking at, else the user's own.
    fn origin_for_save_as(&self, instrument: &dyn Instrument) -> Origin {
        match loaded(instrument).origin() {
            Some(Origin::Bank(bank)) => Origin::Bank(bank.clone()),
            _ => match &self.bank_filter {
                Some(Origin::Bank(bank)) => Origin::Bank(bank.clone()),
                _ => Origin::User,
            },
        }
    }

    /// Applies the entry at `index`, synchronously for ordinary consumers or through the complete
    /// opt-in transaction for a consumer with durable background work.
    fn load_entry(
        &mut self,
        instrument: &dyn Instrument,
        setter: &ParamSetter<'_>,
        index: usize,
        deferred: Option<&mut DeferredPresetUi<'_>>,
    ) {
        let entry = self.entries[index].clone();
        if let Some(preset) = entry.preset.as_ref() {
            if let Some(deferred) = deferred {
                self.problem = (deferred.request)(DeferredPresetUiRequest::Load {
                    name: entry.name,
                    origin: entry.origin,
                    preset: preset.clone(),
                })
                .err();
            } else {
                let (applied, problems, baseline) = apply_preset_checked_from_with_baseline(
                    instrument,
                    setter,
                    preset,
                    Some(&entry.origin),
                );
                if let (true, Some(baseline)) = (applied, baseline) {
                    mark_loaded_with_baseline(
                        instrument,
                        &entry.name,
                        entry.origin.clone(),
                        baseline,
                    );
                }
                // **Reported, not dropped.** `plugins/AGENTS.md` (`docs/plugin-conventions.md`): an
                // unknown id is reported and skipped, a missing one keeps its value — and the person
                // loading the preset is the one who should hear about it.
                self.problem = (!problems.is_empty()).then(|| problems.join("; "));
            }
            self.notice = None;
        }
    }

    /// Which row of `visible` is the loaded preset, by name **and** origin, because a factory and a
    /// user preset can share a name and stepping past one must not land on the other.
    fn current_row(&self, visible: &[usize], instrument: &dyn Instrument) -> Option<usize> {
        let loaded = loaded(instrument);
        visible.iter().position(|index| {
            let entry = &self.entries[*index];
            Some(entry.name.as_str()) == loaded.name() && Some(&entry.origin) == loaded.origin()
        })
    }

    /// The bar's arrows and the browser's arrow keys: the previous or next selectable row.
    fn step(
        &mut self,
        instrument: &dyn Instrument,
        setter: &ParamSetter<'_>,
        by: i32,
        deferred: Option<&mut DeferredPresetUi<'_>>,
    ) {
        let visible = self.visible();
        let rows: Vec<mxm_ui::shell::PresetRow<'_>> = visible
            .iter()
            .map(|index| {
                let entry = &self.entries[*index];
                mxm_ui::shell::PresetRow {
                    name: &entry.name,
                    origin: entry.origin.label(),
                    favourite: false,
                    problem: entry.problem.as_deref(),
                }
            })
            .collect();
        let current = self.current_row(&visible, instrument);
        if let Some(row) = mxm_ui::shell::step_preset(&rows, current, by) {
            let index = visible[row];
            self.load_entry(instrument, setter, index, deferred);
            self.scroll_pending = true;
        }
    }
}

/// §3.1 slots 2 to 4 for an ordinary synchronous preset consumer.
pub fn preset_row(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    presets: &mut PresetUi,
) {
    preset_row_impl(ui, tokens, instrument, setter, presets, None);
}

/// §3.1 slots 2 to 4 for a consumer that prepares durable content asynchronously.
pub fn preset_row_deferred(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    presets: &mut PresetUi,
    deferred: &mut DeferredPresetUi<'_>,
) {
    preset_row_impl(ui, tokens, instrument, setter, presets, Some(deferred));
}

fn preset_row_impl(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    presets: &mut PresetUi,
    mut deferred: Option<&mut DeferredPresetUi<'_>>,
) {
    let loaded = loaded(instrument);
    // A factory preset cannot be written to at any price, so Save offers Save As instead and says
    // why here rather than letting the button do nothing.
    let read_only = presets.library.why_read_only().or_else(|| {
        loaded
            .origin()
            .is_some_and(|origin| !origin.is_writable())
            .then_some("a factory preset cannot be changed; Save writes a copy of your own")
    });

    let state = mxm_ui::shell::PresetState {
        name: loaded.name(),
        modified: loaded.is_modified(),
        favourite: match (loaded.name(), loaded.origin()) {
            (Some(name), Some(origin)) => is_favourite(&presets.favourites, name, origin),
            _ => false,
        },
        read_only,
        save_disabled: deferred
            .as_ref()
            .is_some_and(|deferred| deferred.pending)
            .then_some("wait for the pending preset change before saving"),
    };

    let action = mxm_ui::shell::preset_browser(ui, tokens, &state);
    if !action.any() {
        return;
    }

    if action.browse {
        let open = !presets.browser_open;
        presets.set_browser_open(open);
    }

    if let Some(by) = action.step {
        // **Through the filtered list**, so with a bank and a category chosen the arrows mean
        // something again.
        presets.step(instrument, setter, by, deferred.as_deref_mut());
    }

    if action.init {
        if let Some(deferred) = deferred {
            presets.problem = (deferred.request)(DeferredPresetUiRequest::Init).err();
        } else {
            init_patch(instrument, setter);
            // **Cleared, not marked modified**: there is nothing left that it is a modification of.
            mark_none(instrument);
            presets.problem = None;
        }
    }

    if action.favourite
        && let (Some(name), Some(origin)) = (loaded.name(), loaded.origin())
    {
        // Keyed by origin and name; a plain name from before banks is unstarred the same way.
        let key = favourite_key(name, origin);
        let was_starred = presets.favourites.remove(&key)
            | (!matches!(origin, Origin::Bank(_)) && presets.favourites.remove(name));
        if !was_starred {
            presets.favourites.insert(key);
        }
        write_favourites(&presets.library, &presets.favourites);
    }

    if action.save
        && let (Some(name), Some(origin)) = (loaded.name(), loaded.origin())
    {
        let category = presets.category_of_loaded(instrument);
        let origin = origin.clone();
        save_preset(instrument, presets, name, category, &origin);
    }

    if action.save_as || action.rename {
        let origin = if action.rename {
            loaded.origin().cloned().unwrap_or(Origin::User)
        } else {
            presets.origin_for_save_as(instrument)
        };
        presets.naming = Some(Naming {
            kind: if action.rename {
                NamingKind::Rename
            } else {
                NamingKind::SaveAs
            },
            name: loaded.name().unwrap_or_default().to_owned(),
            category: presets.category_of_loaded(instrument),
            origin,
            collides: false,
            focus: true,
        });
    }

    if action.delete
        && let (Some(name), Some(origin)) = (loaded.name(), loaded.origin())
    {
        match presets.library.delete(name, origin) {
            Ok(()) => {
                // **The sound is left exactly as it is**, with the identity cleared. Reverting to
                // Init or loading the next preset would throw away what somebody is listening to as
                // a side effect of tidying up.
                mark_none(instrument);
                presets.problem = None;
                presets.refresh(instrument);
            }
            Err(why) => presets.problem = Some(why),
        }
    }
}

/// What floats under the bar for an ordinary synchronous preset consumer.
pub fn overlays(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    presets: &mut PresetUi,
) {
    overlays_impl(ui, tokens, instrument, setter, presets, None);
}

/// What floats under the bar for a consumer with a complete deferred preset transaction.
pub fn overlays_deferred(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    presets: &mut PresetUi,
    deferred: &mut DeferredPresetUi<'_>,
) {
    overlays_impl(ui, tokens, instrument, setter, presets, Some(deferred));
}

fn overlays_impl(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    presets: &mut PresetUi,
    deferred: Option<&mut DeferredPresetUi<'_>>,
) {
    let deferred_pending = deferred.as_ref().is_some_and(|deferred| deferred.pending);
    if presets.browser_open {
        browser(ui, tokens, instrument, setter, presets, deferred);
    } else {
        presets.overlay = None;
    }
    naming_row(ui, tokens, instrument, presets, deferred_pending);
}

/// The three-pane browser as an overlay over the view area, the app bar left in sight.
fn browser(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    presets: &mut PresetUi,
    mut deferred: Option<&mut DeferredPresetUi<'_>>,
) {
    let rect = mxm_ui::browser::overlay_rect(ui.max_rect(), ui.cursor().top() + SPACE_2);
    presets.overlay = Some(rect);

    // Everything the panes show, built before the frame so the widget borrows only what it draws.
    let bank_rows = presets.bank_rows();
    let bank_labels: Vec<String> = bank_rows
        .iter()
        .map(|row| match row {
            None => "All".to_owned(),
            Some(Origin::Factory) => "Factory".to_owned(),
            Some(origin) => presets
                .banks
                .iter()
                .find(|bank| &bank.origin == origin)
                .map_or_else(|| origin.label().to_owned(), |bank| bank.name.clone()),
        })
        .collect();
    let bank_counts: Vec<usize> = bank_rows
        .iter()
        .map(|row| {
            presets
                .entries
                .iter()
                .filter(|entry| row.as_ref().is_none_or(|origin| &entry.origin == origin))
                .filter(|entry| presets.passes(entry, false, true))
                .count()
        })
        .collect();
    let bank_problems: Vec<Option<String>> = bank_rows
        .iter()
        .map(|row| {
            row.as_ref().and_then(|origin| {
                presets
                    .banks
                    .iter()
                    .find(|bank| &bank.origin == origin)
                    .and_then(|bank| bank.problem.clone())
            })
        })
        .collect();
    let bank_pane: Vec<PaneRow<'_>> = bank_labels
        .iter()
        .zip(&bank_counts)
        .zip(&bank_problems)
        .map(|((label, count), problem)| PaneRow {
            label,
            count: *count,
            problem: problem.as_deref(),
        })
        .collect();
    let bank_selected = bank_rows
        .iter()
        .position(|row| row == &presets.bank_filter)
        .unwrap_or(0);

    let category_rows = presets.category_rows();
    let category_pane: Vec<PaneRow<'_>> = category_rows
        .iter()
        .map(|filter| {
            let label = match filter {
                CategoryFilter::All => "All",
                CategoryFilter::Favourites => "Favourites",
                CategoryFilter::Category(category) => category.label(),
                CategoryFilter::Group(group) => group.as_str(),
            };
            let count = presets
                .entries
                .iter()
                .filter(|entry| presets.passes(entry, true, false))
                .filter(|entry| presets.in_category(entry, filter))
                .count();
            PaneRow {
                label,
                count,
                problem: None,
            }
        })
        .collect();
    let category_selected = category_rows
        .iter()
        .position(|row| *row == presets.category_filter)
        .unwrap_or(0);

    let visible = presets.visible();
    let details: Vec<String> = visible
        .iter()
        .map(|index| {
            let entry = &presets.entries[*index];
            match (entry.preset.as_ref(), entry.group.as_deref()) {
                (Some(_), Some(group)) => format!("{} · {group}", entry.origin.label()),
                (Some(preset), None) => {
                    format!("{} · {}", entry.origin.label(), preset.category.label())
                }
                (None, _) => entry.origin.label().to_owned(),
            }
        })
        .collect();
    let preset_rows: Vec<BrowserRow<'_>> = visible
        .iter()
        .zip(&details)
        .map(|(index, detail)| {
            let entry = &presets.entries[*index];
            BrowserRow {
                name: &entry.name,
                detail,
                favourite: is_favourite(&presets.favourites, &entry.name, &entry.origin),
                problem: entry.problem.as_deref(),
            }
        })
        .collect();
    let selected = presets.current_row(&visible, instrument);

    let picker_choices: Option<(String, Vec<String>)> = match &presets.picker {
        None => None,
        Some(Picker::Import(files)) => Some((
            "Which bank file to import?".to_owned(),
            files
                .iter()
                .map(|path| {
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
                .collect(),
        )),
        Some(Picker::Replace(_, name)) => Some((
            format!("A bank called {name:?} is already here."),
            vec![
                "Replace it with the file".to_owned(),
                "Keep the bank that is here".to_owned(),
            ],
        )),
    };
    let status = presets.problem.clone().or_else(|| presets.notice.clone());
    let importable = presets.library.importable().len();
    let can_write = presets.library.why_read_only().is_none();
    // The keyboard lives in the search box while nothing else is being typed into.
    let focus_search = presets.naming.is_none() && presets.picker.is_none();
    let scroll_to_selected = presets.scroll_pending;
    presets.scroll_pending = false;

    let mut action = BrowserAction::default();
    egui::Area::new(egui::Id::new("mxm-preset-browser"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(tokens.surface_1)
                .stroke(egui::Stroke::new(HAIRLINE, tokens.border))
                .corner_radius(RADIUS)
                .inner_margin(egui::Margin::same(SPACE_4 as i8))
                .show(ui, |ui| {
                    let inner = rect.size() - egui::vec2(2.0 * SPACE_4, 2.0 * SPACE_4);
                    ui.set_min_size(inner);
                    ui.set_max_size(inner);
                    let mut state = BrowserState {
                        focus: presets.pane_focus,
                        banks: &bank_pane,
                        bank: bank_selected,
                        categories: &category_pane,
                        category: category_selected,
                        presets: &preset_rows,
                        selected,
                        search: &mut presets.search,
                        focus_search,
                        scroll_to_selected,
                        importable,
                        can_write,
                        picker: picker_choices
                            .as_ref()
                            .map(|(title, choices)| (title.as_str(), choices.as_slice())),
                        status: status.as_deref(),
                    };
                    action = mxm_ui::browser::panes(ui, tokens, &mut state);
                });
        });

    // A click on the content around the browser closes it; the bar above it does not, so its own
    // controls keep working while the browser is up.
    let pressed_outside = ui.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| !rect.contains(pos) && pos.y >= rect.top())
    });
    if pressed_outside {
        action.close = true;
    }

    if !action.any() {
        return;
    }
    presets.notice = None;

    if let Some(pane) = action.focus {
        presets.pane_focus = pane;
    }
    if let Some(row) = action.bank {
        presets.bank_filter = bank_rows.get(row).cloned().flatten();
    }
    if let Some(row) = action.category {
        presets.category_filter = category_rows
            .get(row)
            .cloned()
            .unwrap_or(CategoryFilter::All);
    }
    if let Some(row) = action.load
        && let Some(index) = visible.get(row)
    {
        presets.load_entry(instrument, setter, *index, deferred.as_deref_mut());
    }
    if let Some(by) = action.step {
        presets.step(instrument, setter, by, deferred.as_deref_mut());
    }
    if action.import {
        let files = presets.library.importable();
        if files.is_empty() {
            presets.notice = Some(
                "No bank files in the banks folder. Open folder, and put a .mxmbank.json there."
                    .to_owned(),
            );
        } else {
            presets.picker = Some(Picker::Import(files));
        }
    }
    if let Some(choice) = action.pick {
        match presets.picker.take() {
            Some(Picker::Import(files)) => {
                if let Some(path) = files.get(choice) {
                    import(presets, instrument, path.clone(), false);
                }
            }
            Some(Picker::Replace(path, _)) if choice == 0 => {
                import(presets, instrument, path, true);
            }
            Some(Picker::Replace(..)) | None => {}
        }
    }
    if action.cancel_pick {
        presets.picker = None;
    }
    if action.export {
        let name = match (&presets.bank_filter, &presets.category_filter) {
            (Some(Origin::Bank(bank)), CategoryFilter::All) => bank.clone(),
            (Some(Origin::User), CategoryFilter::All) => "My presets".to_owned(),
            (Some(Origin::Factory), CategoryFilter::All) => "Factory".to_owned(),
            (_, CategoryFilter::Category(category)) => category.label().to_owned(),
            (_, CategoryFilter::Group(group)) => group.clone(),
            (_, CategoryFilter::Favourites) => "Favourites".to_owned(),
            (None, CategoryFilter::All) => "All presets".to_owned(),
        };
        presets.naming = Some(Naming {
            kind: NamingKind::Export,
            name,
            category: Category::Uncategorised,
            origin: Origin::User,
            collides: false,
            focus: true,
        });
    }
    if action.new_bank {
        presets.naming = Some(Naming {
            kind: NamingKind::NewBank,
            name: String::new(),
            category: Category::Uncategorised,
            origin: Origin::User,
            collides: false,
            focus: true,
        });
    }
    if action.open_folder {
        match presets.library.banks_dir() {
            Some(dir) => open_folder(dir),
            None => {
                presets.problem = Some(
                    "this system has no config directory, so there is no banks folder".to_owned(),
                );
            }
        }
    }
    if action.enter {
        // **Done.** What is loaded stays — or, with nothing from this list loaded yet, the first
        // match is, so typing a name and pressing Enter is enough — and the browser closes.
        if presets.current_row(&visible, instrument).is_none()
            && let Some(first) = visible
                .iter()
                .copied()
                .find(|index| presets.entries[*index].preset.is_some())
        {
            presets.load_entry(instrument, setter, first, deferred);
        }
        presets.set_browser_open(false);
    }
    if action.close {
        presets.set_browser_open(false);
    }
}

/// Imports one bank file, asking before replacing one of the same name.
fn import(presets: &mut PresetUi, instrument: &dyn Instrument, path: PathBuf, replace: bool) {
    match presets
        .library
        .import_bank(&path, instrument.clap_id(), replace)
    {
        Ok(bank) => {
            presets.refresh(instrument);
            let count = presets
                .entries
                .iter()
                .filter(|entry| entry.origin == bank.origin)
                .count();
            presets.bank_filter = Some(bank.origin);
            presets.category_filter = CategoryFilter::All;
            presets.problem = None;
            presets.notice = Some(format!("Imported {:?}: {count} presets", bank.name));
        }
        Err(ImportRefused::Exists(name)) => {
            presets.picker = Some(Picker::Replace(path, name));
        }
        Err(why) => presets.problem = Some(why.to_string()),
    }
}

/// Save As, Rename, New bank and Export — the name, and for Save As the category — and the last
/// problem, floating under the bar; inside the browser when it is open.
///
/// **Floating, never in the flow.** The window is a fixed size and the layout was measured without
/// this row, so a row injected between the bar and the view bar had nowhere to come from: it
/// squeezed itself and the Save and Cancel buttons drew cropped. Every other transient surface
/// already floats — the browser, the menus — and this one follows, over the content.
fn naming_row(
    ui: &mut Ui,
    tokens: &Tokens,
    instrument: &dyn Instrument,
    presets: &mut PresetUi,
    deferred_pending: bool,
) {
    let position = match presets.overlay {
        // Inside the browser, above its actions: the panes' last rows are covered while a name is
        // typed, which is the one moment they are not being read.
        Some(overlay) => egui::pos2(
            overlay.left() + SPACE_4,
            overlay.bottom() - 2.0 * (MIN_TARGET + SPACE_4) - SPACE_3,
        ),
        None => egui::pos2(SPACE_5, ui.cursor().top() + SPACE_2),
    };
    let anchored = |ui: &Ui, add: &mut dyn FnMut(&mut Ui)| {
        egui::Area::new(egui::Id::new("mxm-preset-naming-row"))
            .order(egui::Order::Tooltip)
            .fixed_pos(position)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(tokens.surface_1)
                    .stroke(egui::Stroke::new(HAIRLINE, tokens.border))
                    .corner_radius(RADIUS)
                    .inner_margin(egui::Margin::symmetric(SPACE_3 as i8, SPACE_2 as i8))
                    .show(ui, |ui| ui.horizontal(|ui| add(ui)));
            });
    };

    let Some(naming) = presets.naming.as_mut() else {
        // The browser shows the problem along its foot; closed, it floats here.
        if presets.overlay.is_none()
            && let Some(problem) = presets.problem.clone()
        {
            anchored(ui, &mut |ui| {
                ui.colored_label(tokens.danger, &problem);
            });
        }
        return;
    };

    let mut commit = false;
    let mut cancel = false;
    let identity_write_disabled =
        deferred_pending && matches!(naming.kind, NamingKind::SaveAs | NamingKind::Rename);

    anchored(ui, &mut |ui| {
        ui.label(naming.kind.prompt());
        let field = ui.add(egui::TextEdit::singleline(&mut naming.name).desired_width(180.0));
        if naming.focus {
            field.request_focus();
            naming.focus = false;
        }
        if field.changed() {
            naming.collides = false;
        }
        // **A sound is saved under a category**, chosen here: the one word that lets a browser
        // find it again among many, whichever bank it ends up in. Rename does not ask — the
        // preset keeps its own.
        if naming.kind == NamingKind::SaveAs {
            ui.label("Category");
            egui::ComboBox::from_id_salt("mxm-preset-category")
                .selected_text(naming.category.label())
                .show_ui(ui, |ui| {
                    for category in Category::ALL {
                        ui.selectable_value(&mut naming.category, category, category.label());
                    }
                });
        }
        commit |= !identity_write_disabled
            && field.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let commit_button = ui.add_enabled(
            !identity_write_disabled,
            egui::Button::new(naming.kind.verb()),
        );
        let commit_button = if identity_write_disabled {
            commit_button.on_disabled_hover_text("wait for the pending preset change before saving")
        } else {
            commit_button
        };
        commit |= commit_button.clicked();
        cancel |= ui.button("Cancel").clicked();
        cancel |= ui.input(|i| i.key_pressed(egui::Key::Escape));
        if naming.collides {
            ui.colored_label(tokens.warning, "A preset of that name exists.");
            commit |= ui
                .add_enabled(!identity_write_disabled, egui::Button::new("Replace it"))
                .clicked();
        }
    });

    if cancel {
        presets.naming = None;
        return;
    }
    if !commit {
        return;
    }

    let Some(naming) = presets.naming.take() else {
        return;
    };
    let name = naming.name.trim().to_owned();
    if name.is_empty() {
        return;
    }

    match naming.kind {
        NamingKind::SaveAs | NamingKind::Rename => {
            // The collision check happens once: the second press of the button is the answer to it.
            if !naming.collides && presets.library.preset_exists(&name, &naming.origin) {
                presets.naming = Some(Naming {
                    collides: true,
                    name,
                    ..naming
                });
                return;
            }
            if naming.kind == NamingKind::SaveAs {
                save_preset(instrument, presets, &name, naming.category, &naming.origin);
            } else {
                let loaded = loaded(instrument);
                let from =
                    Preset::capture(loaded.name().unwrap_or(&name), naming.category, instrument);
                match presets.library.rename(&from, &name, &naming.origin) {
                    Ok(_) => {
                        mark_loaded(instrument, &name, naming.origin.clone());
                        presets.problem = None;
                        presets.refresh(instrument);
                    }
                    Err(why) => presets.problem = Some(why),
                }
            }
        }
        NamingKind::NewBank => match presets.library.new_bank(&name, "", instrument.clap_id()) {
            Ok(origin) => {
                presets.refresh(instrument);
                presets.bank_filter = Some(origin);
                presets.category_filter = CategoryFilter::All;
                presets.problem = None;
                presets.notice = Some(format!(
                    "Made the bank {name:?}; Save As puts a sound in it"
                ));
            }
            Err(why) => presets.problem = Some(why),
        },
        NamingKind::Export => {
            let (chosen, left_out) = presets.exportable();
            if chosen.is_empty() && left_out > 0 {
                presets.problem = Some(FOUND_NOT_EXPORTED.to_owned());
                return;
            }
            let comment = match (&presets.bank_filter, &presets.category_filter) {
                (None, CategoryFilter::All) => "every preset".to_owned(),
                (bank, filter) => format!(
                    "{}{}",
                    bank.as_ref()
                        .map_or(String::new(), |origin| format!("{} · ", origin.label())),
                    match filter {
                        CategoryFilter::All => "all categories".to_owned(),
                        CategoryFilter::Favourites => "favourites".to_owned(),
                        CategoryFilter::Category(category) => category.label().to_owned(),
                        CategoryFilter::Group(group) => group.clone(),
                    }
                ),
            };
            match presets
                .library
                .export_bank(&name, "", &comment, instrument.clap_id(), &chosen)
            {
                Ok(path) => {
                    presets.problem = None;
                    presets.notice = Some(if left_out == 0 {
                        format!("Exported {} presets to {}", chosen.len(), path.display())
                    } else {
                        format!(
                            "Exported {} presets to {}; {left_out} from a folder were left out",
                            chosen.len(),
                            path.display()
                        )
                    });
                }
                Err(why) => presets.problem = Some(why),
            }
        }
    }
}

/// Captures the patch under `name` as a `category`, writes it where `origin` keeps its files, and
/// makes it the loaded preset.
fn save_preset(
    instrument: &dyn Instrument,
    presets: &mut PresetUi,
    name: &str,
    category: Category,
    origin: &Origin,
) {
    let preset = Preset::capture(name, category, instrument);
    match presets.library.save(&preset, origin) {
        Ok(_) => {
            // The baseline is recaptured here, so a save makes the patch clean.
            mark_loaded(instrument, name, origin.clone());
            presets.problem = None;
            presets.refresh(instrument);
        }
        Err(why) => presets.problem = Some(why),
    }
}

/// Writes a whole preset, one bracketed parameter at a time, and returns what it could not do.
///
/// **The target set is built first and each parameter written once.** Applying Init and then the
/// preset would send two gestures per parameter, and a host would record every one of them.
/// Anything the preset could not supply is reported rather than guessed at, and a parameter it does
/// not mention keeps its current value.
pub fn apply_preset(
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    preset: &Preset,
) -> Vec<String> {
    apply_preset_checked(instrument, setter, preset).1
}

/// Apply a preset and distinguish a rejected durable payload from non-fatal missing/unknown
/// parameters. A caller must not mark a rejected file as loaded.
pub fn apply_preset_checked(
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    preset: &Preset,
) -> (bool, Vec<String>) {
    apply_preset_checked_from(instrument, setter, preset, None)
}

/// Apply with trusted library provenance. An origin-less call remains strict so a JSON file cannot
/// promote itself to factory status; only the compiled-in library entry supplies `Factory` here.
pub fn apply_preset_checked_from(
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    preset: &Preset,
    origin: Option<&Origin>,
) -> (bool, Vec<String>) {
    let (applied, problems, _) =
        apply_preset_checked_from_with_baseline(instrument, setter, preset, origin);
    (applied, problems)
}

/// Applies a preset while retaining the canonical values its queued host gestures intend to set.
///
/// CLAP may apply GUI-authored parameter events on a later process or flush call. Reading the live
/// atomics immediately after emitting the gestures therefore captures the patch being replaced,
/// not the preset being loaded.
fn apply_preset_checked_from_with_baseline(
    instrument: &dyn Instrument,
    setter: &ParamSetter<'_>,
    preset: &Preset,
    origin: Option<&Origin>,
) -> (bool, Vec<String>, Option<BTreeMap<String, f32>>) {
    let (writes, mut problems) = preset.resolve_from(instrument, origin);
    let mut baseline = snapshot(instrument);
    for (id, parameter, target) in &writes {
        baseline.insert((*id).to_owned(), parameter.canonical_normalised(*target));
    }
    if let Err(problem) = instrument.validate_preset_state(preset.state.as_ref()) {
        problems.push(problem);
        return (false, problems, None);
    }
    if let Err(problem) = instrument.apply_preset_state(preset.state.as_ref()) {
        problems.push(problem);
        return (false, problems, None);
    }
    for (_, param, value) in writes {
        param.begin(setter);
        param.set(setter, value);
        param.end(setter);
    }
    // The durable half of the sound becomes audible only now, with the patch that describes it
    // already written. See `Instrument::commit_preset_state`.
    instrument.commit_preset_state();
    (true, problems, Some(baseline))
}

/// Init writes persisted parameters and, for an explicit opt-in, the instrument's authored default
/// model — never sounding notes or live controller state. A held note keeps sounding and its
/// envelope simply follows the new times.
///
/// Durable Init content is prepared before the gestures and committed after them, exactly like a
/// factory preset. Source-preserving instruments return no Init content and retain their source.
/// **Every parameter write is a complete gesture.** An Init that opens gestures and closes none
/// latches every automation lane in the host.
pub fn init_patch(instrument: &dyn Instrument, setter: &ParamSetter<'_>) {
    let state = instrument.init_preset_state();
    if let Some(state) = state.as_ref()
        && (instrument.validate_preset_state(Some(state)).is_err()
            || instrument.apply_preset_state(Some(state)).is_err())
    {
        return;
    }
    for (id, param) in instrument.parameters() {
        if instrument.is_source_owned(id) || instrument.is_instance_setting(id) {
            continue;
        }
        let default = param.default_normalised();
        param.begin(setter);
        param.set(setter, default);
        param.end(setter);
    }
    if state.is_some() {
        instrument.commit_preset_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestInstrument;
    use nice_plug::params::internals::ParamPtr;
    use nice_plug::prelude::{PluginApi, PluginState};

    /// A host that accepts every gesture and does nothing with it.
    struct NoHost;

    impl nice_plug::context::gui::GuiContextInner for NoHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {}
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    struct CountingHost(std::sync::atomic::AtomicUsize);

    impl nice_plug::context::gui::GuiContextInner for CountingHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    /// A CLAP-like host that applies GUI parameter output only when processing or flushing later.
    #[derive(Default)]
    struct QueuedHost(std::sync::Mutex<Vec<(ParamPtr, f32)>>);

    impl QueuedHost {
        fn flush(&self) {
            for (param, normalised) in self.0.lock().unwrap().drain(..) {
                // SAFETY: this test owns the instrument and flushes on its only thread.
                unsafe { param._internal_set_normalized_value(normalised) };
            }
        }
    }

    impl nice_plug::context::gui::GuiContextInner for QueuedHost {
        fn plugin_api(&self) -> PluginApi {
            PluginApi::Clap
        }
        unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
        unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
            self.0.lock().unwrap().push((param, normalized));
        }
        unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
        fn get_state(&self) -> PluginState {
            PluginState {
                version: String::new(),
                params: Default::default(),
                fields: Default::default(),
            }
        }
        fn set_state(&self, _state: PluginState) {}
    }

    #[test]
    fn a_host_that_applies_preset_gestures_later_still_leaves_the_preset_clean() {
        let instrument = TestInstrument::new();
        let host = QueuedHost::default();
        let setter = ParamSetter::new(&host);
        let mut presets = PresetUi::at(Library::at(None), &instrument);
        let brass = presets
            .entries
            .iter()
            .position(|entry| entry.name == "Brass")
            .expect("factory Brass preset");

        presets.load_entry(&instrument, &setter, brass, None);
        assert_eq!(
            instrument.cutoff.value(),
            0.5,
            "the host has not flushed yet"
        );
        host.flush();

        assert!((instrument.cutoff.value() - 0.7).abs() < f32::EPSILON);
        assert!(instrument.sync.value());
        assert_eq!(
            loaded(&instrument),
            crate::Loaded::Clean {
                name: "Brass".to_owned(),
                origin: Origin::Factory,
            }
        );
    }

    #[test]
    fn starring_a_preset_puts_it_first_and_leaves_the_rest_where_they_were() {
        // **What the star is for.** Reported as: *"I can star a preset, but what is it used for?"*
        // — and it was used for nothing. A control that records a preference nothing acts on is a
        // control that does nothing dressed as one that does something.
        let instrument = TestInstrument::new();
        let mut presets = PresetUi::at(Library::at(None), &instrument);

        let plain: Vec<String> = presets
            .order()
            .iter()
            .map(|i| presets.entries[*i].name.clone())
            .collect();
        assert_eq!(plain[0], "Init", "the premise: Init leads the factory list");

        presets.favourites_mut().insert("Brass".to_owned());
        let starred: Vec<String> = presets
            .order()
            .iter()
            .map(|i| presets.entries[*i].name.clone())
            .collect();

        assert_eq!(starred[0], "Brass", "a starred preset goes to the top");
        let rest: Vec<&String> = starred[1..].iter().collect();
        let expected: Vec<&String> = plain.iter().filter(|n| *n != "Brass").collect();
        assert_eq!(rest, expected, "everything else keeps its order");
    }

    /// **Presets found in a folder are factory presets filed under their folder** (the owner,
    /// 2026-09-28: mxm-fx-convolution's impulse files are its factory set, and their folders its
    /// categories): listed after the compiled set as Factory, a row per folder after the
    /// categories, a folder's row showing exactly its presets, and none of them counted under its
    /// own `Category` as well. One found outside any folder is filed by its category as before.
    #[test]
    fn found_presets_are_factory_presets_filed_under_their_folders() {
        let instrument = TestInstrument::new();
        let found = |name: &str, group: Option<&str>| {
            let mut preset = Preset::init(&instrument);
            preset.name = name.to_owned();
            preset.category = Category::Bass;
            crate::Found {
                preset,
                group: group.map(str::to_owned),
            }
        };
        let library = Library::at(None).with_found(vec![
            found("Cinema", Some("Venues")),
            found("Cellar", Some("Rooms")),
            found("Loose", None),
        ]);
        let mut presets = PresetUi::at(library, &instrument);
        let listed: Vec<(&str, &Origin, Option<&str>)> = presets
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), &entry.origin, entry.group.as_deref()))
            .collect();
        assert_eq!(
            &listed[3..],
            [
                ("Cinema", &Origin::Factory, Some("Venues")),
                ("Cellar", &Origin::Factory, Some("Rooms")),
                ("Loose", &Origin::Factory, None),
            ],
            "after Init, Sub bass and Brass"
        );
        assert_eq!(
            presets.category_rows(),
            vec![
                CategoryFilter::All,
                CategoryFilter::Favourites,
                CategoryFilter::Category(Category::Bass),
                CategoryFilter::Category(Category::Brass),
                CategoryFilter::Category(Category::Template),
                CategoryFilter::Group("Rooms".to_owned()),
                CategoryFilter::Group("Venues".to_owned()),
            ]
        );
        let visible = |presets: &PresetUi| -> Vec<String> {
            presets
                .visible()
                .iter()
                .map(|i| presets.entries[*i].name.clone())
                .collect()
        };
        presets.category_filter = CategoryFilter::Group("Rooms".to_owned());
        assert_eq!(visible(&presets), ["Cellar"]);
        presets.category_filter = CategoryFilter::Category(Category::Bass);
        assert_eq!(
            visible(&presets),
            ["Sub bass", "Loose"],
            "the rooms are not in Bass too"
        );

        // Exporting leaves them out: they name a file where they were found, and a bank has to load
        // on a machine without that folder.
        presets.category_filter = CategoryFilter::All;
        let (chosen, left_out) = presets.exportable();
        let names: Vec<&str> = chosen.iter().map(|preset| preset.name.as_str()).collect();
        assert_eq!(names, ["Init", "Sub bass", "Brass"]);
        assert_eq!(left_out, 3);
    }

    #[test]
    fn the_filters_narrow_what_is_visible_and_the_panes_count_what_the_other_leaves() {
        let instrument = TestInstrument::new();
        let mut presets = PresetUi::at(Library::at(None), &instrument);
        assert_eq!(presets.visible().len(), 3, "Init, Sub bass, Brass");

        presets.category_filter = CategoryFilter::Category(Category::Bass);
        let names: Vec<&str> = presets
            .visible()
            .iter()
            .map(|i| presets.entries[*i].name.as_str())
            .collect();
        assert_eq!(names, ["Sub bass"]);

        // The categories pane lists only what the bank has, after All and Favourites, in the
        // list's own order; the banks pane is everything, the factory, then each bank.
        presets.category_filter = CategoryFilter::All;
        assert_eq!(
            presets.category_rows(),
            vec![
                CategoryFilter::All,
                CategoryFilter::Favourites,
                CategoryFilter::Category(Category::Bass),
                CategoryFilter::Category(Category::Brass),
                CategoryFilter::Category(Category::Template),
            ]
        );
        assert_eq!(
            presets.bank_rows(),
            vec![None, Some(Origin::Factory), Some(Origin::User)]
        );

        // A search narrows by name, whatever the panes say.
        presets.search = "sub".to_owned();
        let names: Vec<&str> = presets
            .visible()
            .iter()
            .map(|i| presets.entries[*i].name.as_str())
            .collect();
        assert_eq!(names, ["Sub bass"]);
    }

    #[test]
    fn a_preset_missing_a_parameter_reports_it_when_applied() {
        // The problems `resolve` reports were once dropped on the floor by two editors. A preset
        // missing a parameter loads what it has and says what it lacks.
        let instrument = TestInstrument::new();
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let mut preset = Preset::capture("Partial", Category::Uncategorised, &instrument);
        preset.params.remove("cutoff");
        let problems = apply_preset(&instrument, &setter, &preset);
        assert!(
            problems.iter().any(|p| p.contains("cutoff")),
            "the missing parameter is named: {problems:?}"
        );
    }

    #[test]
    fn existing_parameter_only_apply_and_init_remain_immediately_synchronous() {
        let instrument = TestInstrument::new();
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = ParamSetter::new(&host);
        let preset = Preset::capture("Immediate", Category::Uncategorised, &instrument);
        assert!(apply_preset_checked(&instrument, &setter, &preset).0);
        assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 9);
        init_patch(&instrument, &setter);
        assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 18);
    }

    #[test]
    fn unexpected_durable_content_is_rejected_before_parameter_writes() {
        let instrument = TestInstrument::new();
        let host = CountingHost(std::sync::atomic::AtomicUsize::new(0));
        let setter = ParamSetter::new(&host);
        let mut preset = Preset::capture("Foreign content", Category::Uncategorised, &instrument);
        preset.params.get_mut("cutoff").unwrap().v = 0.1;
        preset.state = Some(serde_json::json!({ "audio": "not for this synth" }));
        let (applied, problems) = apply_preset_checked(&instrument, &setter, &preset);
        assert!(!applied);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("does not accept"))
        );
        assert_eq!(host.0.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    /// An asset instrument that records the order the seam calls it in.
    struct Ordered {
        inner: TestInstrument,
        log: std::cell::RefCell<Vec<&'static str>>,
    }

    impl Instrument for Ordered {
        fn clap_id(&self) -> &'static str {
            self.inner.clap_id()
        }
        fn parameters(&self) -> Vec<(&'static str, &dyn crate::ErasedParam)> {
            self.inner.parameters()
        }
        fn identity(&self) -> &std::sync::RwLock<crate::PresetIdentity> {
            self.inner.identity()
        }
        fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
            self.inner.factory_files()
        }
        fn validate_preset_state(&self, _state: Option<&serde_json::Value>) -> Result<(), String> {
            Ok(())
        }
        fn apply_preset_state(&self, _state: Option<&serde_json::Value>) -> Result<(), String> {
            self.log.borrow_mut().push("prepare");
            Ok(())
        }
        fn commit_preset_state(&self) {
            self.log.borrow_mut().push("commit");
        }
    }

    /// **The commit barrier.** An asset instrument prepares its audio before any gesture — so a
    /// rejection costs nothing — and is told to make it audible only after the whole patch has
    /// been written. Publishing in `apply_preset_state` would let a host render the new content
    /// through the old parameters.
    #[test]
    fn durable_content_is_prepared_before_the_gestures_and_committed_after_them() {
        let instrument = Ordered {
            inner: TestInstrument::new(),
            log: std::cell::RefCell::new(Vec::new()),
        };
        let host = NoHost;
        let setter = ParamSetter::new(&host);
        let preset = Preset::capture("Sampled", Category::Uncategorised, &instrument);
        instrument.log.borrow_mut().clear();
        let (applied, problems) = apply_preset_checked(&instrument, &setter, &preset);
        assert!(applied, "{problems:?}");
        assert_eq!(*instrument.log.borrow(), ["prepare", "commit"]);

        // A recipe that carries no content of its own is still committed, so an instrument that
        // staged something earlier is not left holding it.
        let mut recipe = preset.clone();
        recipe.state = None;
        instrument.log.borrow_mut().clear();
        assert!(apply_preset_checked(&instrument, &setter, &recipe).0);
        assert_eq!(*instrument.log.borrow(), ["prepare", "commit"]);
    }

    /// The bar and what floats under it, laid out in the accessibility harness.
    fn harness(
        presets: std::rc::Rc<std::cell::RefCell<PresetUi>>,
        instrument: std::rc::Rc<TestInstrument>,
    ) -> egui_kittest::Harness<'static> {
        egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .build_ui(move |ui| {
                mxm_ui::theme::apply(ui.ctx());
                mxm_ui::typography::apply(ui.ctx());
                let tokens = mxm_ui::LIGHT;
                let host = NoHost;
                let setter = ParamSetter::new(&host);
                let mut presets = presets.borrow_mut();
                mxm_ui::AppBar::new("test").show_with(
                    ui,
                    &tokens,
                    |ui| preset_row(ui, &tokens, instrument.as_ref(), &setter, &mut presets),
                    |_ui| {},
                );
                overlays(ui, &tokens, instrument.as_ref(), &setter, &mut presets);
            })
    }

    #[test]
    fn deferred_app_bar_routes_selection_and_init_but_suppresses_save_while_pending() {
        use kittest::Queryable;

        let instrument = std::rc::Rc::new(TestInstrument::new());
        let presets = std::rc::Rc::new(std::cell::RefCell::new(PresetUi::at(
            Library::at(None),
            instrument.as_ref(),
        )));
        let requests = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let harness_presets = std::rc::Rc::clone(&presets);
        let harness_instrument = std::rc::Rc::clone(&instrument);
        let harness_requests = std::rc::Rc::clone(&requests);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(900.0, 600.0))
            .build_ui(move |ui| {
                mxm_ui::theme::apply(ui.ctx());
                mxm_ui::typography::apply(ui.ctx());
                let tokens = mxm_ui::LIGHT;
                let host = NoHost;
                let setter = ParamSetter::new(&host);
                let mut presets = harness_presets.borrow_mut();
                let requests = std::rc::Rc::clone(&harness_requests);
                let mut request = move |request| {
                    requests.borrow_mut().push(request);
                    Ok(())
                };
                let mut deferred = DeferredPresetUi {
                    pending: true,
                    request: &mut request,
                };
                mxm_ui::AppBar::new("test").show_with(
                    ui,
                    &tokens,
                    |ui| {
                        preset_row_deferred(
                            ui,
                            &tokens,
                            harness_instrument.as_ref(),
                            &setter,
                            &mut presets,
                            &mut deferred,
                        );
                    },
                    |_ui| {},
                );
                overlays_deferred(
                    ui,
                    &tokens,
                    harness_instrument.as_ref(),
                    &setter,
                    &mut presets,
                    &mut deferred,
                );
            });
        harness.run_steps(3);

        // Save As may already be open when another action begins deferred work. Every visible Save
        // action must remain inert until that work leaves Pending; otherwise it captures half-old
        // parameters/content and replaces the transaction's identity.
        presets.borrow_mut().open_save_as("Pending copy");
        harness.run_steps(2);
        // One "Save" now: the naming row's commit. The app bar's is "Save preset" since the
        // owner's 2026-09-22 ruling, and both are pressed here because both must stay inert.
        assert_eq!(harness.query_all_by_label("Save").count(), 1);
        for save in harness.query_all_by_label("Save") {
            save.click();
        }
        harness.get_by_label("Save preset").click();
        harness.run_steps(2);
        assert!(
            presets.borrow().is_naming(),
            "an already-open Save As committed while deferred work was pending"
        );
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!presets.borrow().is_naming());

        harness.get_by_label("Save preset").click();
        harness.run_steps(2);
        assert!(
            !presets.borrow().is_naming(),
            "a disabled Save preset still opened the naming row"
        );
        assert!(requests.borrow().is_empty());

        harness.get_by_label("›").click();
        harness.run_steps(2);
        assert!(matches!(
            requests.borrow().as_slice(),
            [DeferredPresetUiRequest::Load { .. }]
        ));

        harness.get_by_label("…").click();
        harness.run_steps(2);
        harness.get_by_label("Init patch").click();
        harness.run_steps(2);
        let requests = requests.borrow();
        assert!(
            matches!(
                requests.as_slice(),
                [
                    DeferredPresetUiRequest::Load { .. },
                    DeferredPresetUiRequest::Init
                ]
            ),
            "requests: {requests:#?}"
        );
    }

    /// Save As asks for the name **and the category** — the owner's rule (2026-09-04): a sound is
    /// categorised when it is saved, on every instrument.
    #[test]
    fn save_as_asks_for_a_name_and_a_category() {
        use kittest::Queryable;

        let instrument = std::rc::Rc::new(TestInstrument::new());
        let presets = std::rc::Rc::new(std::cell::RefCell::new(PresetUi::at(
            Library::at(None),
            instrument.as_ref(),
        )));
        presets.borrow_mut().open_save_as("New sound");
        let mut harness = harness(
            std::rc::Rc::clone(&presets),
            std::rc::Rc::clone(&instrument),
        );
        harness.run_steps(3);

        assert!(
            harness.query_by_label("Save as").is_some(),
            "the row is open and says what it is for"
        );
        assert!(
            harness.query_by_label("Category").is_some(),
            "and offers the category beside the name"
        );
        assert!(presets.borrow().is_naming());
    }

    /// The name in the bar opens the browser; a category row narrows the list; Escape closes it.
    /// All from the accessibility tree, which is what a script — or the developer channel — sees.
    #[test]
    fn the_browser_opens_from_the_name_and_a_category_narrows_the_list() {
        use kittest::Queryable;

        let instrument = std::rc::Rc::new(TestInstrument::new());
        let presets = std::rc::Rc::new(std::cell::RefCell::new(PresetUi::at(
            Library::at(None),
            instrument.as_ref(),
        )));
        let mut harness = harness(
            std::rc::Rc::clone(&presets),
            std::rc::Rc::clone(&instrument),
        );
        harness.run_steps(2);
        assert!(
            harness.query_by_label("Banks").is_none(),
            "closed until asked"
        );

        harness.get_by_label("No preset").click();
        harness.run_steps(2);
        assert!(presets.borrow().is_browser_open());
        for label in ["Banks", "Categories", "Factory", "My presets", "Sub bass"] {
            assert!(
                harness.query_by_label(label).is_some(),
                "{label} is missing"
            );
        }
        // "Brass" is a category row and a preset: two nodes.
        assert_eq!(harness.query_all_by_label("Brass").count(), 2);

        harness.get_by_label("Bass").click();
        harness.run_steps(2);
        assert!(harness.query_by_label("Sub bass").is_some(), "a bass stays");
        assert_eq!(
            harness.query_all_by_label("Brass").count(),
            1,
            "the brass preset is gone; its category row stays"
        );

        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!presets.borrow().is_browser_open(), "Escape closes it");
    }

    /// The owner's ask (2026-09-04): the arrow keys navigate the presets — loading as they go,
    /// so stepping is auditioning — and Enter closes. From anywhere in the browser.
    #[test]
    fn the_arrows_step_the_presets_and_enter_closes() {
        let instrument = std::rc::Rc::new(TestInstrument::new());
        let presets = std::rc::Rc::new(std::cell::RefCell::new(PresetUi::at(
            Library::at(None),
            instrument.as_ref(),
        )));
        presets.borrow_mut().set_browser_open(true);
        let mut harness = harness(
            std::rc::Rc::clone(&presets),
            std::rc::Rc::clone(&instrument),
        );
        harness.run_steps(2);
        assert_eq!(
            loaded(instrument.as_ref()).name(),
            None,
            "the premise: nothing loaded"
        );

        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        assert_eq!(
            loaded(instrument.as_ref()).name(),
            Some("Init"),
            "down from nothing is the first"
        );
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        assert_eq!(loaded(instrument.as_ref()).name(), Some("Sub bass"));
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(2);
        assert_eq!(loaded(instrument.as_ref()).name(), Some("Init"));
        assert!(presets.borrow().is_browser_open(), "stepping keeps it open");

        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert!(!presets.borrow().is_browser_open(), "Enter closes it");
        assert_eq!(
            loaded(instrument.as_ref()).name(),
            Some("Init"),
            "and keeps what was chosen"
        );
    }

    /// The owner's ask (2026-09-04): left and right reach the banks and the categories. Left into
    /// the categories, down twice to Bass; right back to the presets, down loads the one bass.
    #[test]
    fn left_and_right_move_between_the_panes_and_up_and_down_choose_within_one() {
        let instrument = std::rc::Rc::new(TestInstrument::new());
        let presets = std::rc::Rc::new(std::cell::RefCell::new(PresetUi::at(
            Library::at(None),
            instrument.as_ref(),
        )));
        presets.borrow_mut().set_browser_open(true);
        let mut harness = harness(
            std::rc::Rc::clone(&presets),
            std::rc::Rc::clone(&instrument),
        );
        harness.run_steps(2);

        harness.key_press(egui::Key::ArrowLeft);
        harness.run_steps(2);
        assert_eq!(presets.borrow().pane_focus, Pane::Categories);
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        assert_eq!(presets.borrow().category_filter, CategoryFilter::Favourites);
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        assert_eq!(
            presets.borrow().category_filter,
            CategoryFilter::Category(Category::Bass)
        );
        assert_eq!(presets.borrow().visible().len(), 1, "one bass in the list");

        harness.key_press(egui::Key::ArrowLeft);
        harness.run_steps(2);
        assert_eq!(presets.borrow().pane_focus, Pane::Banks);
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        assert_eq!(presets.borrow().bank_filter, Some(Origin::Factory));

        harness.key_press(egui::Key::ArrowRight);
        harness.run_steps(2);
        harness.key_press(egui::Key::ArrowRight);
        harness.run_steps(2);
        assert_eq!(presets.borrow().pane_focus, Pane::Presets);
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        assert_eq!(loaded(instrument.as_ref()).name(), Some("Sub bass"));
        assert!(presets.borrow().is_browser_open());
    }

    /// Type a name, press Enter: the first match loads and the browser closes.
    #[test]
    fn enter_with_nothing_chosen_loads_the_first_match_and_closes() {
        let instrument = std::rc::Rc::new(TestInstrument::new());
        let presets = std::rc::Rc::new(std::cell::RefCell::new(PresetUi::at(
            Library::at(None),
            instrument.as_ref(),
        )));
        presets.borrow_mut().set_browser_open(true);
        presets.borrow_mut().search = "bra".to_owned();
        let mut harness = harness(
            std::rc::Rc::clone(&presets),
            std::rc::Rc::clone(&instrument),
        );
        harness.run_steps(2);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert_eq!(loaded(instrument.as_ref()).name(), Some("Brass"));
        assert!(!presets.borrow().is_browser_open());
    }
}
