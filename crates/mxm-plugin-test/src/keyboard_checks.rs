//! Shared headless checks for the keyboard cursor on a real plugin panel, beside
//! [`crate::paging_checks`].
//!
//! **What the rollout can get wrong is coverage, and that is what this checks.** Converting an
//! editor means wrapping every control the binding draws in a `navigation::at` scope; a control
//! that was missed paints exactly as before and simply never joins the registry, so nothing fails
//! and the parameter is quietly unreachable from the keyboard. Painting the panel and reading back
//! what registered is the only honest test of that, because the registry is built by drawing.
//!
//! It proves reachability and that a value key reaches the host (VALUE + an arrow, kept with OUT),
//! at one width, headless. It does not prove the feel, native-window behaviour, or a DAW.
//! `Coverage` has two variants and most editors construct one of them. Keep both: trimming per
//! consumer is how one shared check quietly becomes twelve different ones.

use std::sync::atomic::{AtomicUsize, Ordering};

use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::{PluginApi, PluginState};

/// A host that counts gestures instead of making a sound.
#[derive(Default)]
pub struct Recorder {
    begins: AtomicUsize,
    sets: AtomicUsize,
    ends: AtomicUsize,
}

impl Recorder {
    pub fn begins(&self) -> usize {
        self.begins.load(Ordering::Relaxed)
    }
    pub fn sets(&self) -> usize {
        self.sets.load(Ordering::Relaxed)
    }
    pub fn ends(&self) -> usize {
        self.ends.load(Ordering::Relaxed)
    }
}

impl nice_plug::context::gui::GuiContextInner for Recorder {
    // A test double has no host to ask for a restart (nice-plug 0.4).
    fn request_restart(&self) {}
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {
        self.begins.fetch_add(1, Ordering::Relaxed);
    }
    unsafe fn raw_set_parameter_normalized(&self, _param: ParamPtr, _normalized: f32) {
        self.sets.fetch_add(1, Ordering::Relaxed);
    }
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {
        self.ends.fetch_add(1, Ordering::Relaxed);
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

/// One editor, driven frame by frame with whatever keys a check hands it.
pub struct Session {
    ctx: egui::Context,
    size: egui::Vec2,
}

impl Session {
    pub fn new(size: egui::Vec2) -> Self {
        let ctx = egui::Context::default();
        mxm_ui::theme::apply(&ctx);
        mxm_ui::typography::apply(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        Self { ctx, size }
    }

    pub fn context(&self) -> &egui::Context {
        &self.ctx
    }

    pub fn frame(&self, panel: &mut impl FnMut(&mut egui::Ui), events: Vec<egui::Event>) {
        // egui only reports a modifier through `Modifiers`, and `RawInput` carries the current
        // state separately from the key events. Without this a modified key arrives as a bare one
        // and the check silently measures the wrong move.
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut events = events;
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            events,
            ..Default::default()
        };
        let mut output = self.ctx.run_ui(input, |ui| panel(ui));
        output.textures_delta.clear();
    }

    /// The registry is built by drawing, so the cursor has nothing to act on until a frame has
    /// been painted. Four frames also let the paging renderer finish measuring.
    pub fn settle(&self, panel: &mut impl FnMut(&mut egui::Ui)) {
        for _ in 0..4 {
            self.frame(panel, Vec::new());
        }
    }

    /// Every parameter the last frame registered, by permanent id.
    pub fn registered(&self) -> Vec<String> {
        let mut keys: Vec<String> = mxm_ui::navigation::spots(&self.ctx)
            .into_iter()
            .map(|spot| spot.key)
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }
}

pub use mxm_keys::Job;

/// The jobs a plugin's tests press, each on the key the default keymap gives it ([`key_of`]).
pub const VALUE: Job = Job::Verb(mxm_keys::Verb::Value);
pub const COARSE: Job = Job::Step(mxm_keys::Step::Coarse);
pub const MICRO: Job = Job::Step(mxm_keys::Step::Micro);
pub const OUT: Job = Job::Out;

/// The key the default keymap gives a job. Tests press jobs, not keys, so a remap of the default
/// keymap changes no plugin's test.
pub fn key_of(job: Job) -> egui::Key {
    mxm_ui::navigation::default_key(job)
        .unwrap_or_else(|| panic!("the default keymap binds {}", job.name()))
}

pub fn key(key: egui::Key, modifiers: egui::Modifiers, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}

pub fn press(k: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    vec![key(k, modifiers, true), key(k, modifiers, false)]
}

/// What the registry is measured against.
///
/// Two shapes, because two kinds of editor exist. Most draw their whole inventory on cards, and
/// anything absent is a forgotten scope. `mxm-mono-08` reaches a stage's routing amounts one at
/// a time through the card's own picker, so a headless sweep cannot paint them all; there the
/// inventory is a bound rather than a target.
pub enum Coverage<'a> {
    /// Every id must register, and nothing outside the list may.
    Exactly(&'a [&'a str]),
    /// Something must register, and nothing outside the list may.
    Within(&'a [&'a str]),
}

/// **The one check an editor owes the keyboard cursor.**
///
/// Two properties, one call, because they fail for the same reason and an editor that declares
/// them separately mostly declares the same eight lines of setup twice:
///
/// 1. **It operates.** The cursor lands without being aimed, VALUE + an arrow, kept with OUT,
///    edits the parameter it landed on as one balanced host gesture, and COARSE + an arrow
///    selects a card instead of editing. The edit is also what proves the binding filled
///    `ParamView::stepping`: a control left with no step law moves by nothing and the host sees
///    no set.
/// 2. **It reaches everything.** Each card is requested in turn — so a parameter on a page this
///    width does not show is still visited — with `reveal` opening whatever the editor keeps
///    behind a disclosure, and the registry is compared against `coverage`. A parameter missing
///    from it is a control whose `navigation::at` scope was forgotten: it paints exactly as
///    before, and nothing else in the suite would say so.
///
/// **In that order**, because the first needs a cursor that has not been anywhere: it lands on
/// the first spot the default page draws, which is deterministic, where a cursor left at the end
/// of a sweep is on whatever card happened to be last.
pub fn the_cursor_reaches_and_operates(
    size: egui::Vec2,
    items: &[mxm_ui::paging::Item<'_>],
    coverage: Coverage<'_>,
    reveal: &dyn Fn(&egui::Context),
    host: &Recorder,
    panel: &mut impl FnMut(&mut egui::Ui),
) {
    operates(size, host, None, panel);
    reaches(size, items, coverage, reveal, panel);
}

/// [`the_cursor_reaches_and_operates`], with the arrow pressed **on `first`**, which a pointer
/// press has landed the cursor on, as it does for a person. For an editor whose opening control
/// edits something other than a host parameter — mxm-fx-convolution's response model, whose
/// Interpretation opens the page — so the arrow has a parameter to prove the binding on.
#[allow(dead_code)]
pub fn the_cursor_reaches_and_operates_from(
    size: egui::Vec2,
    items: &[mxm_ui::paging::Item<'_>],
    coverage: Coverage<'_>,
    first: &str,
    reveal: &dyn Fn(&egui::Context),
    host: &Recorder,
    panel: &mut impl FnMut(&mut egui::Ui),
) {
    operates(size, host, Some(first), panel);
    reaches(size, items, coverage, reveal, panel);
}

fn operates(
    size: egui::Vec2,
    host: &Recorder,
    first: Option<&str>,
    panel: &mut impl FnMut(&mut egui::Ui),
) {
    let session = Session::new(size);
    session.settle(panel);
    assert!(
        !session.registered().is_empty(),
        "nothing registered: this editor is not running the cursor at all"
    );
    if let Some(first) = first {
        let spot = mxm_ui::navigation::spots(session.context())
            .into_iter()
            .find(|spot| spot.key == first)
            .unwrap_or_else(|| panic!("{first} is not on the opening page"));
        let at = spot.rect.center();
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        session.frame(panel, vec![egui::Event::PointerMoved(at), button(true)]);
        session.frame(panel, vec![button(false)]);
        session.frame(panel, Vec::new());
    }

    // The keyboard language (every editor's since 2026-10-08): the edit is VALUE + an arrow kept
    // with OUT, and the card step is COARSE + an arrow.
    let none = egui::Modifiers::NONE;
    let taps = |keys: &[egui::Key]| -> Vec<egui::Event> {
        keys.iter().flat_map(|&k| press(k, none)).collect()
    };
    let (edit, card, edit_name, card_name) = (
        taps(&[key_of(VALUE), egui::Key::ArrowUp, key_of(OUT)]),
        taps(&[key_of(COARSE), egui::Key::ArrowRight]),
        "VALUE + an arrow",
        "COARSE + an arrow",
    );

    let (begins, sets, ends) = (host.begins(), host.sets(), host.ends());
    session.frame(panel, edit);
    session.frame(panel, Vec::new());
    assert!(
        host.sets() > sets,
        "{edit_name} on the landed cursor set nothing"
    );
    assert_eq!(
        host.begins() - begins,
        host.ends() - ends,
        "{edit_name} left an unbalanced host gesture"
    );

    let sets = host.sets();
    session.frame(panel, card);
    session.frame(panel, Vec::new());
    assert_eq!(
        host.sets(),
        sets,
        "{card_name} edited a value; it selects a card"
    );
}

fn reaches(
    size: egui::Vec2,
    items: &[mxm_ui::paging::Item<'_>],
    coverage: Coverage<'_>,
    reveal: &dyn Fn(&egui::Context),
    panel: &mut impl FnMut(&mut egui::Ui),
) {
    let session = Session::new(size);
    let mut found: Vec<String> = Vec::new();
    // An editor with no paging renderer passes no items: every card it has is on screen at
    // once (mxm-fx-curve), so one pass with nothing requested sees them all.
    let requests: Vec<Option<mxm_ui::paging::Key>> = if items.is_empty() {
        vec![None]
    } else {
        items.iter().map(|item| Some(item.key)).collect()
    };
    for request in requests {
        if let Some(key) = request {
            mxm_ui::paging::editor::request_card(session.context(), key);
        }
        // A control behind a closed disclosure is not painted, so it cannot register. Opening
        // them is what makes this a check of the *editor* rather than of one frame of it.
        reveal(session.context());
        session.settle(panel);
        found.extend(session.registered());
    }
    let declared = match coverage {
        Coverage::Exactly(ids) => {
            let missing: Vec<&&str> = ids
                .iter()
                .filter(|id| !found.iter().any(|seen| seen == *id))
                .collect();
            assert!(
                missing.is_empty(),
                "these parameters are drawn but never join the keyboard cursor's registry, so                      the cursor cannot reach them: {missing:?}. Each one is a control whose                      `mxm_ui::navigation::at` scope is missing."
            );
            ids
        }
        Coverage::Within(ids) => ids,
    };
    let unknown: Vec<&String> = found
        .iter()
        .filter(|id| !declared.contains(&id.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "the registry names {unknown:?}, which are not parameters of this plugin"
    );
}
