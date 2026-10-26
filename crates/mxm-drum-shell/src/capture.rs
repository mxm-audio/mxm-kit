//! Resample's plugin side: the plugin's own capture thread, the kit each engage heard, and the state
//! machine `activate` and `process` run (the drum machine's, its plan §4.7; shared since
//! 2026-10-26). A plugin's capture itself, and the frozen reader that plays it, are its DSP crate's
//! and stay there: it implements [`Capturer`].
//!
//! # Why a drum plugin does not use nice-plug's background executor
//!
//! Every other plugin in the collection that renders off the audio thread schedules onto
//! `AsyncExecutor::execute_background`, and the drum machine started there too. It crashed hosts,
//! and `clap-validator` reproduced it on `param-set-events`. The mechanism is in the nice-plug
//! fork's `src/event_loop/background_thread.rs` (mxm-audio/nice-plug) and it is not a length
//! problem:
//!
//! 1. The worker thread is **shared by every instance of the plugin in the process**, and a task
//!    carries a `Weak` reference to the instance that scheduled it.
//! 2. When the worker picks up a task whose instance has since been destroyed, the upgrade fails
//!    and the worker **returns** — killing the shared thread and dropping the channel's receiver
//!    (`background_thread.rs:148`).
//! 3. The handle outlives it, and its `Drop` then does
//!    `tasks_sender.send(Message::Shutdown).expect(…)` on a disconnected channel, which panics
//!    while unwinding a plugin's teardown (`background_thread.rs:98`).
//!
//! So *any* task still queued when its instance goes away is a live crash, and no amount of
//! chunking or rate-limiting removes it — it only narrows the window. Submitting from an editor
//! frame instead of `process` narrowed it far enough to pass the validator, but it also meant
//! **nothing rendered while the GUI was closed**: a host automating a captured axis, or a
//! controller moving one, left the frozen kit stale for ever.
//!
//! Owning the thread fixes both at once. There is no sharing, so there is no `Weak` to fail; the
//! shutdown is a flag this module owns and joins on; and the worker reads the request straight
//! out of [`Signals`], so it does not care whether an editor exists.
//!
//! **The cost, stated plainly:** one parked thread per plugin instance, waking every
//! [`POLL`] to look at one atomic. That is the price of a capture that keeps up with a closed
//! GUI, and it is charged per instance rather than per process.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use crate::bank::CaptureBank;
use crate::pack::{PackError, Written};
use crate::signals::{CaptureRequest, Export, Signals};

/// How long the worker sleeps between looks at the capture request.
///
/// `process` cannot wake a condvar — it must not touch a lock — so a pending capture is found by
/// polling. Everything on the control thread notifies instead, so this only bounds the case
/// nothing else covers: a parameter moving with the GUI closed. A capture takes around 200 ms at
/// 48 kHz, so 20 ms of latency in front of it is not a figure anybody can hear, and a parked
/// thread waking fifty times a second costs nothing measurable.
pub const POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// How much audio one chunk of a capture renders before the worker looks at `alive` again.
///
/// It bounds teardown: `Drop` joins this thread, so the longest a host can wait for the plugin to
/// go away is one chunk. At 48 kHz this is a few milliseconds of work.
pub const CHUNK_FRAMES: usize = 65_536;

/// Why an Export found no kit to write: Resample's capture has not been installed yet.
pub const EXPORT_NOT_READY: &str = "Resample is still recording the kit; export again in a moment.";

/// A drum plugin's capture: what its engine renders, and how. The one thing this crate cannot know,
/// implemented by a type of the plugin's own (so its DSP crate's engine and kit types can be named
/// here without a trait of this crate on them).
pub trait Capturer: Send + Sync + 'static {
    /// The capture thread's name.
    const THREAD_NAME: &'static str;
    /// Every slot's patch, as a capture reads it: a plain value, copied across threads.
    type Patches: Copy + Send + Sync + 'static;
    /// A rendered kit, stamped with the engage it answers and the rate it was rendered at.
    type Kit: Send + 'static;
    /// A capture under way, rendered a chunk at a time.
    type Progress;
    /// The plugin's engine, which a frozen kit is installed into.
    type Engine;

    /// The patches as they stand, read **unsmoothed** (a smoother's steps are the audio thread's),
    /// and the kit's name. Any thread.
    fn read(&self) -> (Self::Patches, String);
    /// Whether two readings capture the same kit: every field a capture bakes in agrees.
    fn same_capture(was: &Self::Patches, now: &Self::Patches) -> bool;
    /// Begins a capture at `sample_rate`, stopping if `abandon` says to; `None` when it was refused
    /// or stopped, which leaves the instrument live.
    fn begin(
        &self,
        patches: &Self::Patches,
        sample_rate: f32,
        abandon: &dyn Fn() -> bool,
    ) -> Option<Self::Progress>;
    /// Renders up to `frames` more: `Some(true)` once finished, `None` when it failed.
    fn render(progress: &mut Self::Progress, frames: usize) -> Option<bool>;
    /// The finished kit, stamped with the engage that asked.
    fn finish(progress: Self::Progress, engagement: u32) -> Self::Kit;
    /// The heard patches with the controls that stay live while frozen (Tune, Decay) as set now.
    fn live(heard: &Self::Patches, now: &Self::Patches) -> Self::Patches;
    /// Captures `captured` at `sample_rate` and writes it as a pack named `name` into `root`, each
    /// one-shot as `played` plays it, stopping if `stop` says to ([`crate::pack::write_until`]).
    ///
    /// # Errors
    ///
    /// Why, in words for the editor.
    #[allow(clippy::too_many_arguments)]
    fn write_pack(
        &self,
        root: &Path,
        name: &str,
        captured: &Self::Patches,
        played: &Self::Patches,
        sample_rate: f32,
        stop: &dyn Fn() -> bool,
    ) -> Result<Written, String>;
    /// Installs `kit` into `engine`, or takes the frozen kit away with `None`, returning the one it
    /// displaced. **Audio thread** too: the box goes through, nothing made or freed.
    fn install(engine: &mut Self::Engine, kit: Option<Box<Self::Kit>>) -> Option<Box<Self::Kit>>;
    /// Whether a frozen kit plays in `engine`.
    fn is_frozen(engine: &Self::Engine) -> bool;
    /// The engage `kit` answers.
    fn kit_engagement(kit: &Self::Kit) -> u32;
    /// The rate `kit` was rendered at.
    fn kit_rate(kit: &Self::Kit) -> f32;

    /// A whole capture, now: `activate`'s, which may block.
    fn capture_now(
        &self,
        patches: &Self::Patches,
        sample_rate: f32,
        engagement: u32,
    ) -> Option<Self::Kit> {
        let mut progress = self.begin(patches, sample_rate, &|| false)?;
        while !Self::render(&mut progress, CHUNK_FRAMES)? {}
        Some(Self::finish(progress, engagement))
    }
}

/// What a kit was captured from: its patches, and the kit's name, which its pack takes.
#[derive(Debug, Clone)]
pub struct Heard<P> {
    /// Every slot's patch, as [`Capturer::read`] read it.
    pub patches: P,
    /// The kit's name then.
    pub name: String,
}

/// What the control thread has asked the worker for, beyond the capture it reads from the signals.
#[derive(Default)]
struct Pending {
    /// Cleared by [`CaptureWorker::drop`] to end the loop.
    running: bool,
    /// A sample pack the editor asked for: where to write it, and at what rate.
    ///
    /// **The destination travels with the job** rather than being resolved by the worker. The
    /// worker used to call `pack::root()` itself, which meant every test of this path wrote a
    /// real sixteen-file pack into the folder of whoever ran it — fourteen of them, in the
    /// owner's own AppData, before anybody noticed. It is also what lets a chosen folder reach
    /// the writer at all.
    export: Option<(f32, PathBuf)>,
}

/// The last two kits' [`Heard`], each with the engage it answered.
type Remembered<P> = [Option<(u32, Heard<P>)>; 2];

struct Shared<P> {
    pending: Mutex<Pending>,
    wake: Condvar,
    /// What the last two kits were captured from, with the engage each answered
    /// (`Gate::remember`): Export renders and names the one heard, not the parameters or the kit
    /// loaded since (one capture per engage), and an activation keeps a kit whose patch has not
    /// moved. Two, as a kit for a new engage can be made while the old one still plays.
    heard: Mutex<Remembered<P>>,
}

impl<P: Clone> Shared<P> {
    /// Remembers what the kit answering `engagement` was captured from.
    fn remember(&self, engagement: u32, captured: Heard<P>) {
        let mut heard = self
            .heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        heard[1] = heard[0].take();
        heard[0] = Some((engagement, captured));
    }

    /// What the kit answering `engagement` was captured from, if it is remembered.
    fn heard_for(&self, engagement: u32) -> Option<Heard<P>> {
        self.heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .flatten()
            .find(|(e, _)| *e == engagement)
            .map(|(_, heard)| heard.clone())
    }

    /// Ends the worker loop and wakes it so it notices.
    fn stop(&self) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .running = false;
        self.wake.notify_all();
    }
}

/// A way to ask the capture thread for something, which works whether or not one is running.
///
/// It exists from the plugin's construction, because an editor can be opened before `activate`
/// spawns the thread, and it outlives the thread, because an editor can be closed after one has
/// stopped. Neither end is an error: a request made before the thread starts **waits** and is
/// answered when it does, and one made after it has stopped is dropped, because there is nothing
/// left to answer it.
pub struct Gate<P>(Arc<Shared<P>>);

impl<P> Clone for Gate<P> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<P> Default for Gate<P> {
    fn default() -> Self {
        Self(Arc::new(Shared {
            pending: Mutex::new(Pending {
                running: true,
                export: None,
            }),
            wake: Condvar::new(),
            heard: Mutex::new([None, None]),
        }))
    }
}

impl<P: Clone> Gate<P> {
    /// Asks for a sample pack in `destination`. **Control thread** — the editor's export button.
    ///
    /// The caller resolves where it goes, so a chosen folder and the default one take the same
    /// path through here, and a test can name a temporary directory instead of a real one.
    pub fn request_export(&self, sample_rate: f32, destination: PathBuf) {
        self.0
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .export = Some((sample_rate, destination));
        self.0.wake.notify_all();
    }

    /// Remembers what the kit answering `engagement` was captured from: the capture thread's, and
    /// `activate`'s. **Never the audio thread** (it locks and allocates).
    pub fn remember(&self, engagement: u32, captured: Heard<P>) {
        self.0.remember(engagement, captured);
    }

    /// What the kit answering `engagement` was captured from, if it is remembered.
    #[must_use]
    pub fn heard_for(&self, engagement: u32) -> Option<Heard<P>> {
        self.0.heard_for(engagement)
    }

    /// Wakes the worker now rather than at the next poll. **Control thread.**
    ///
    /// Only ever an optimisation: the poll finds the same request a beat later.
    pub fn nudge(&self) {
        self.0.wake.notify_all();
    }
}

/// Everything the worker needs from the plugin, by shared ownership rather than by borrow: its
/// capture, the bank it publishes into, and the telemetry that carries the [`Signals`].
pub struct Inputs<C: Capturer, T> {
    pub capturer: Arc<C>,
    pub bank: Arc<CaptureBank<C::Kit>>,
    pub telemetry: Arc<T>,
}

/// A running capture thread, stopped and joined when it is dropped.
pub struct CaptureWorker {
    stop: Box<dyn Fn() + Send + Sync>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CaptureWorker {
    /// Spawns the thread behind an existing gate.
    ///
    /// A control-thread operation — `activate`, in practice — so that a plugin merely being
    /// constructed for a host's scan does not pay for a thread it will never use.
    #[must_use]
    pub fn spawn<C, T>(gate: &Gate<C::Patches>, inputs: Inputs<C, T>) -> Self
    where
        C: Capturer,
        T: AsRef<Signals> + Send + Sync + 'static,
    {
        let shared = Arc::clone(&gate.0);
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name(String::from(C::THREAD_NAME))
            .spawn(move || run(&worker, &inputs))
            .ok();
        Self {
            stop: Box::new(move || shared.stop()),
            thread,
        }
    }

    /// Whether the thread started: spawning fails only when the system has none to give.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.thread.is_some()
    }
}

impl Drop for CaptureWorker {
    fn drop(&mut self) {
        (self.stop)();
        if let Some(thread) = self.thread.take() {
            // Deliberately not `expect`ed. A panicking join here is precisely the shape that made
            // the shared executor take hosts down with it, and a worker that somehow failed is
            // not a reason to abort a teardown that is otherwise going fine.
            let _ = thread.join();
        }
    }
}

/// The worker loop: wait for something to do, do one bounded piece of it, look again.
fn run<C: Capturer, T: AsRef<Signals>>(shared: &Shared<C::Patches>, inputs: &Inputs<C, T>) {
    loop {
        let Some(job) = wait_for_work(shared, (*inputs.telemetry).as_ref()) else {
            return;
        };
        match job {
            Job::Capture(request) => capture(shared, inputs, request),
            Job::Export(sample_rate, destination) => {
                export(shared, inputs, sample_rate, &destination);
            }
        }
    }
}

enum Job {
    Capture(CaptureRequest),
    Export(f32, PathBuf),
}

/// Blocks until there is work or the worker is stopped, returning `None` to end the loop.
fn wait_for_work<P>(shared: &Shared<P>, signals: &Signals) -> Option<Job> {
    let mut pending = shared
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    loop {
        if !pending.running {
            return None;
        }
        // Export first: it is a person waiting on a button, where a capture is housekeeping.
        if let Some((rate, destination)) = pending.export.take() {
            return Some(Job::Export(rate, destination));
        }
        if let Some(request) = signals.take_capture_request() {
            return Some(Job::Capture(request));
        }
        let (next, _) = shared
            .wake
            .wait_timeout(pending, POLL)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending = next;
    }
}

/// Whether the worker should abandon what it is doing and return.
fn stopping<P>(shared: &Shared<P>) -> bool {
    !shared
        .pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .running
}

/// Renders one kit and publishes it for the audio thread to install.
///
/// Chunked so that teardown waits for a chunk rather than a kit, and abandoned rather than
/// published if the plugin goes away mid-render: a kit nobody will ever install is not worth
/// finishing, and the buffers go back with the thread.
///
/// **Abandoned too if a newer request overtakes it.** Within one engage `process` asks once, so a
/// request arriving mid-render means Resample was turned off and on again, or the rate moved: the
/// kit in flight was made for a patch or a rate that is gone. Published, it would install first —
/// the old patch, then every reader reset for the new one — and it was a second displaced kit for
/// the bank to hold. The loop picks the newer request up next.
fn capture<C: Capturer, T: AsRef<Signals>>(
    shared: &Shared<C::Patches>,
    inputs: &Inputs<C, T>,
    request: CaptureRequest,
) {
    let signals: &Signals = (*inputs.telemetry).as_ref();
    // Free whatever the audio thread displaced before allocating a replacement, so the handoff
    // stays a ring rather than growing. It is also what lets `CaptureBank::take` accept the next
    // kit at all — it refuses while a retired one is still parked.
    inputs.bank.drain();
    let (patches, name) = inputs.capturer.read();
    let overtaken = || signals.capture_requested();
    let abandon = || stopping(shared) || overtaken();
    // The start stops for teardown too: a plugin's capture may build what it plays there.
    let Some(mut progress) = inputs
        .capturer
        .begin(&patches, request.sample_rate, &abandon)
    else {
        // A refusal leaves the instrument live, which is always a correct rendering of the patch.
        return;
    };
    loop {
        if stopping(shared) || overtaken() {
            return;
        }
        match C::render(&mut progress, CHUNK_FRAMES) {
            Some(true) => break,
            Some(false) => {}
            // Abandoned rather than published half-finished.
            None => return,
        }
    }
    // Once more after the last chunk: a request can land while it renders.
    if overtaken() {
        return;
    }
    // Stamped with the engage that asked, which is how `process` knows it is still the one to
    // install; its patches remembered first, so an Export the moment it installs finds them.
    let kit = C::finish(progress, request.engagement);
    shared.remember(request.engagement, Heard { patches, name });
    inputs.bank.publish(kit);
    inputs.bank.drain();
}

/// Renders a kit and writes it out as a sample pack, reporting the outcome either way.
///
/// Abandoned if the plugin goes away mid-render or mid-write, so closing a project does not wait
/// on a pack; files already staged are removed, so it never leaves half a pack behind.
fn export<C: Capturer, T: AsRef<Signals>>(
    shared: &Shared<C::Patches>,
    inputs: &Inputs<C, T>,
    sample_rate: f32,
    root: &Path,
) {
    let signals: &Signals = (*inputs.telemetry).as_ref();
    let (current, _) = inputs.capturer.read();
    // **The kit heard**: frozen, the patches its kit was captured from (an edit since renders
    // nothing, so a pack of the parameters would not be what plays), at the pack's rate; the
    // controls that stay live as set now, as they are live.
    let heard = signals
        .frozen()
        .and_then(|engagement| shared.heard_for(engagement));
    // **Only the kit heard makes a pack**: until a capture is installed, nothing frozen plays and
    // the parameters may not be what will (the editor's Export waits for it too).
    let Some(heard) = heard else {
        signals.publish_export(Export::Failed(EXPORT_NOT_READY.to_owned()));
        return;
    };
    let played = C::live(&heard.patches, &current);
    let stop = || stopping(shared);
    let written = inputs.capturer.write_pack(
        root,
        &heard.name,
        &heard.patches,
        &played,
        sample_rate,
        &stop,
    );
    signals.publish_export(match written {
        Ok(written) => Export::Written {
            files: written.files,
            clipped: written.clipped,
        },
        Err(why) => Export::Failed(why),
    });
}

/// A [`PackError::Cancelled`] in words, for a [`Capturer::write_pack`] that stopped before writing.
#[must_use]
pub fn cancelled() -> String {
    PackError::Cancelled.to_string()
}

/// Resample's state, as the drum machine's: the engage count, whether this engage has asked, the
/// rate of the kit installed, the bank, the gate and the capture thread.
///
/// - **One capture per engage**: nothing re-renders while engaged, as installing a kit cuts sound;
///   to change a frozen kit, off, edit, on.
/// - **Only a kit carrying the current engage number and rate installs**; one from an engage since
///   left is the patch as it was, and is retired unplayed.
/// - **An activation keeps a kit at its rate whose patch has not moved** and captures one that has.
pub struct Resample<C: Capturer> {
    capturer: Arc<C>,
    /// **Which engage this is**: advanced when a disengage ends one and at every activation.
    ///
    /// A request carries it and the worker stamps it on the kit, and only a kit stamped with the
    /// current one installs. Off, edit, on is the owner's way to change a frozen kit, and a kit the
    /// old engage asked for can land after it — still rendering, or waiting in the bank because the
    /// disengage came first. Installed, it was the old patch, and having a kit the new engage never
    /// asked for the edited one.
    pub engagement: u32,
    /// Whether this engage has already asked for its one capture.
    ///
    /// Cleared by disengaging, which is the only thing that arms another — the owner's rule is
    /// that a frozen kit is captured once and edits happen with Resample off.
    asked: bool,
    /// The rate the installed kit was rendered at, or `None` for no kit. While engaged a kit is
    /// captured once, and a parameter edit never re-renders it (the owner, 2026-09-21), so
    /// [`Self::service`] compares only the rate: a kit rendered at a rate the engine has since left
    /// is unplayable rather than merely out of date. An activation also compares the patch it was
    /// captured from ([`Self::activate`]).
    captured: Option<f32>,
    /// The lock-free handoff the capture thread publishes into.
    pub bank: Arc<CaptureBank<C::Kit>>,
    /// The way to the capture thread, from construction to after its teardown.
    pub gate: Gate<C::Patches>,
    /// The capture thread, spawned at the first activation; a test standing in for it sets this to
    /// `None`, which stops and joins it.
    pub worker: Option<CaptureWorker>,
}

impl<C: Capturer> Resample<C> {
    /// Nothing engaged, no thread yet.
    #[must_use]
    pub fn new(capturer: C) -> Self {
        Self {
            capturer: Arc::new(capturer),
            engagement: 0,
            asked: false,
            captured: None,
            bank: Arc::new(CaptureBank::new()),
            gate: Gate::default(),
            worker: None,
        }
    }

    /// The plugin's capture.
    #[must_use]
    pub fn capturer(&self) -> &C {
        &self.capturer
    }

    /// Whether this engage has asked the capture thread for its kit.
    #[must_use]
    pub const fn asked(&self) -> bool {
        self.asked
    }

    /// The rate the installed kit was rendered at, or `None` for no kit.
    #[must_use]
    pub const fn captured(&self) -> Option<f32> {
        self.captured
    }

    /// `activate`'s part: a new engage, the bank drained, the kit captured now when Resample is on
    /// (kept when its patch has not moved), and the capture thread spawned the first time.
    ///
    /// **A control-thread operation.** It renders seconds of audio and allocates, so it belongs to
    /// `activate`, never to `process`. Activation is the important one: a session or an offline
    /// bounce that opens with Resample engaged is frozen from its first sample, with no worker
    /// timing anywhere in the restore path. **An activation supersedes everything asked before
    /// it**: a kit still rendering was made for the rate and the patch as they were. A capture that
    /// fails leaves the instrument live rather than silent, which is the safe way round: the engine
    /// is always a correct rendering of the patch.
    pub fn activate<T>(
        &mut self,
        engine: &mut C::Engine,
        engaged: bool,
        telemetry: &Arc<T>,
        sample_rate: f32,
    ) where
        T: AsRef<Signals> + Send + Sync + 'static,
    {
        self.asked = false;
        self.engagement = self.engagement.wrapping_add(1);
        self.bank.drain();
        self.refresh(engine, engaged, (**telemetry).as_ref(), sample_rate);
        // Spawned here rather than at construction, so a host's scan — which builds the plugin to
        // read its parameters and throws it away — never starts a thread. Once only: a second
        // `activate` (a rate change, say) keeps the thread it already has, because the rate
        // travels with each request rather than being fixed when the thread starts.
        if self.worker.is_none() {
            let worker = CaptureWorker::spawn(
                &self.gate,
                Inputs {
                    capturer: Arc::clone(&self.capturer),
                    bank: Arc::clone(&self.bank),
                    telemetry: Arc::clone(telemetry),
                },
            );
            // Resample stays live and correct without it; the editor says why it never freezes.
            (**telemetry)
                .as_ref()
                .set_capture_unavailable(!worker.is_running());
            self.worker = Some(worker);
        }
    }

    fn refresh(
        &mut self,
        engine: &mut C::Engine,
        engaged: bool,
        signals: &Signals,
        sample_rate: f32,
    ) {
        if !engaged {
            drop(C::install(engine, None));
            self.captured = None;
            self.asked = false;
            signals.set_frozen(None);
            return;
        }
        let (patches, name) = self.capturer.read();
        // **A kit at this rate whose patch has not moved is kept** (re-rendering it would only cut
        // every hit), as this engage's; a state restored since, at the same rate, is captured
        // again (model drums' review, 2026-10-26).
        let heard = signals
            .frozen()
            .and_then(|engagement| self.gate.heard_for(engagement))
            .map(|heard| heard.patches);
        #[allow(clippy::float_cmp)]
        let at_rate = self.captured == Some(sample_rate);
        if at_rate
            && let Some(heard) = heard
            && C::same_capture(&heard, &patches)
        {
            // The patch has not moved, so the kit loaded now names what plays.
            self.gate.remember(
                self.engagement,
                Heard {
                    patches: heard,
                    name,
                },
            );
            signals.set_frozen(Some(self.engagement));
            self.asked = false;
            return;
        }
        match self
            .capturer
            .capture_now(&patches, sample_rate, self.engagement)
        {
            Some(kit) => {
                drop(C::install(engine, Some(Box::new(kit))));
                self.captured = Some(sample_rate);
                self.gate.remember(self.engagement, Heard { patches, name });
                signals.set_frozen(Some(self.engagement));
            }
            None => {
                drop(C::install(engine, None));
                self.captured = None;
                signals.set_frozen(None);
            }
        }
        self.asked = false;
    }

    /// Installs a rendered kit and asks for one when Resample is engaged without one. **Audio
    /// thread, once a `process` call**: a pointer swap, a handful of comparisons and at most one
    /// relaxed store; nothing allocated, locked or dropped, a displaced kit going back to the bank
    /// for the capture thread to free.
    ///
    /// Disengaging is immediate, as it needs no render. Engaging asks the capture thread once (one
    /// capture per engage: a parameter edit while frozen renders nothing, so a knob sweep never
    /// queues renders).
    pub fn service(
        &mut self,
        engine: &mut C::Engine,
        engaged: bool,
        signals: &Signals,
        sample_rate: f32,
    ) {
        if !engaged {
            // **A disengage ends the engage**, so whatever it asked for — still rendering, or
            // waiting in the bank — is refused when it lands. Only once per engage: after this
            // block nothing is frozen and nothing asked.
            if C::is_frozen(engine) || self.asked {
                self.engagement = self.engagement.wrapping_add(1);
            }
            if C::is_frozen(engine) {
                if let Some(displaced) = C::install(engine, None) {
                    self.bank.retire(displaced);
                }
                self.captured = None;
                signals.set_frozen(None);
            }
            // Cleared whether or not a kit was installed: disengaging is what arms the next
            // capture, and a request that never landed must not count as this engage's one.
            self.asked = false;
            return;
        }

        if let Some(kit) = self.bank.take() {
            // **Only a kit this engage asked for, at this rate, installs**; any other is retired
            // unplayed. One from an earlier engage is the patch as it was, and this engage has
            // asked, or already has its kit, so nothing more is needed. One at a rate the engine
            // has left would play every slot at the wrong speed, so the comparison below asks
            // again.
            let kit_rate = C::kit_rate(&kit);
            let this_engage = C::kit_engagement(&kit) == self.engagement;
            #[allow(clippy::float_cmp)]
            let usable = this_engage && kit_rate == sample_rate;
            if usable {
                // The `Box` goes straight through: unwrapping it would free an allocation here
                // and reboxing the displaced kit would make one, both of which `process` forbids.
                if let Some(displaced) = C::install(engine, Some(kit)) {
                    self.bank.retire(displaced);
                }
                self.captured = Some(kit_rate);
                signals.set_frozen(Some(self.engagement));
            } else {
                self.bank.retire(kit);
                if this_engage {
                    // Ask again, at the rate the engine is actually running.
                    self.asked = false;
                }
            }
        }

        #[allow(clippy::float_cmp)]
        if self.captured == Some(sample_rate) {
            self.asked = false;
            return;
        }
        if !self.asked {
            // `process` asks; it does not render (the capture thread does).
            self.asked = true;
            signals.request_capture(sample_rate, self.engagement);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SLOT_COUNT;
    use crate::pack::OneShots;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A slot's patch as the stand-in reads it: its sound, and its live control (index 1).
    type Patches = [[u8; 2]; SLOT_COUNT];

    /// A kit as the stand-in captures one.
    #[derive(Debug)]
    struct Kit {
        engagement: u32,
        rate: f32,
        patches: Patches,
    }

    /// A capture under way: chunks left, each taking `delay`.
    struct Progress {
        patches: Patches,
        rate: f32,
        left: usize,
        delay: std::time::Duration,
    }

    /// The stand-in engine: a frozen kit or none.
    #[derive(Default)]
    struct Engine {
        kit: Option<Box<Kit>>,
    }

    /// A capture whose reading is set by hand, rendering `chunks` chunks of `delay` each.
    struct Fake {
        reading: Mutex<(Patches, String)>,
        chunks: usize,
        delay: std::time::Duration,
        begun: AtomicUsize,
    }

    impl Fake {
        fn new(chunks: usize, delay_ms: u64) -> Self {
            let mut patches = [[0, 0]; SLOT_COUNT];
            patches[0] = [3, 7];
            Self {
                reading: Mutex::new((patches, "Kit A".to_owned())),
                chunks,
                delay: std::time::Duration::from_millis(delay_ms),
                begun: AtomicUsize::new(0),
            }
        }

        fn set(&self, slot: usize, patch: [u8; 2], name: &str) {
            let mut reading = self.reading.lock().unwrap();
            reading.0[slot] = patch;
            reading.1 = name.to_owned();
        }
    }

    /// A pack's one-shots: each slot's sound byte as its length, its live byte as its level.
    struct Shots<'a> {
        captured: &'a Patches,
        played: &'a Patches,
        rate: f32,
    }

    impl OneShots for Shots<'_> {
        fn sample_rate(&self) -> f32 {
            self.rate
        }
        fn is_empty(&self, slot: usize) -> bool {
            self.captured[slot][0] == 0
        }
        fn one_shot_until(
            &self,
            slot: usize,
            cancelled: &dyn Fn() -> bool,
        ) -> Result<Vec<f32>, PackError> {
            let length = usize::from(self.captured[slot][0]) * 100;
            let level = f32::from(self.played[slot][1]) / 255.0;
            let mut at = 0;
            crate::pack::render_one_shot(slot, length, cancelled, || {
                at += 1;
                (at <= length).then_some(level)
            })
        }
        fn label(&self, _slot: usize) -> &str {
            "Drum"
        }
        fn note(&self, slot: usize) -> Option<u8> {
            Some(36 + slot as u8)
        }
        fn product(&self) -> &str {
            "mxm-test-drums"
        }
        fn losses(&self) -> &str {
            "A test.\n"
        }
    }

    impl Capturer for Fake {
        const THREAD_NAME: &'static str = "mxm-test-drums capture";
        type Patches = Patches;
        type Kit = Kit;
        type Progress = Progress;
        type Engine = Engine;

        fn read(&self) -> (Patches, String) {
            self.reading.lock().unwrap().clone()
        }
        fn same_capture(was: &Patches, now: &Patches) -> bool {
            was.iter().zip(now).all(|(a, b)| a[0] == b[0])
        }
        fn begin(
            &self,
            patches: &Patches,
            sample_rate: f32,
            abandon: &dyn Fn() -> bool,
        ) -> Option<Progress> {
            self.begun.fetch_add(1, Ordering::Relaxed);
            (!abandon()).then_some(Progress {
                patches: *patches,
                rate: sample_rate,
                left: self.chunks,
                delay: self.delay,
            })
        }
        fn render(progress: &mut Progress, _frames: usize) -> Option<bool> {
            std::thread::sleep(progress.delay);
            progress.left = progress.left.saturating_sub(1);
            Some(progress.left == 0)
        }
        fn finish(progress: Progress, engagement: u32) -> Kit {
            Kit {
                engagement,
                rate: progress.rate,
                patches: progress.patches,
            }
        }
        fn live(heard: &Patches, now: &Patches) -> Patches {
            std::array::from_fn(|slot| [heard[slot][0], now[slot][1]])
        }
        fn write_pack(
            &self,
            root: &Path,
            name: &str,
            captured: &Patches,
            played: &Patches,
            sample_rate: f32,
            stop: &dyn Fn() -> bool,
        ) -> Result<Written, String> {
            let shots = Shots {
                captured,
                played,
                rate: sample_rate,
            };
            crate::pack::write_until(root, name, &shots, stop).map_err(|error| error.to_string())
        }
        fn install(engine: &mut Engine, kit: Option<Box<Kit>>) -> Option<Box<Kit>> {
            std::mem::replace(&mut engine.kit, kit)
        }
        fn is_frozen(engine: &Engine) -> bool {
            engine.kit.is_some()
        }
        fn kit_engagement(kit: &Kit) -> u32 {
            kit.engagement
        }
        fn kit_rate(kit: &Kit) -> f32 {
            kit.rate
        }
    }

    /// The telemetry a plugin hands the worker: here only the signals.
    #[derive(Default)]
    struct Telemetry {
        signals: Signals,
    }

    impl AsRef<Signals> for Telemetry {
        fn as_ref(&self) -> &Signals {
            &self.signals
        }
    }

    fn inputs(fake: Fake) -> Inputs<Fake, Telemetry> {
        Inputs {
            capturer: Arc::new(fake),
            bank: Arc::new(CaptureBank::new()),
            telemetry: Arc::new(Telemetry::default()),
        }
    }

    fn wait_for_kit(bank: &CaptureBank<Kit>) -> Box<Kit> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(kit) = bank.take() {
                return kit;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the worker never published a kit"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    #[test]
    fn a_request_made_with_no_editor_open_is_still_rendered() {
        // The hole that owning the thread exists to close. Nothing here is an editor frame: the
        // request goes in the way `process` makes it, and the kit has to arrive anyway.
        let inputs = inputs(Fake::new(2, 0));
        let bank = Arc::clone(&inputs.bank);
        let telemetry = Arc::clone(&inputs.telemetry);
        let gate = Gate::default();
        let _worker = CaptureWorker::spawn(&gate, inputs);

        telemetry.signals.request_capture(96_000.0, 1);
        gate.nudge();
        let kit = wait_for_kit(&bank);
        assert_eq!(kit.rate, 96_000.0, "at the rate it was asked for");
        assert_eq!(kit.engagement, 1, "stamped with the engage that asked");
    }

    /// **A render a newer request overtakes is abandoned, not published.** Resample turned off and
    /// on again while the first engage's kit is still rendering asks again; the kit in flight was
    /// made for the patch as it was. The render runs on this thread with the newer request already
    /// waiting, so nothing here depends on timing: the look between chunks sees it before the first
    /// chunk and nothing reaches the bank. The same render with nothing waiting publishes, stamped
    /// with its engage — so the refusal is about the newer request and nothing else.
    #[test]
    fn a_render_a_newer_request_overtakes_is_abandoned() {
        let inputs = inputs(Fake::new(2, 0));
        let gate = Gate::default();
        let first = CaptureRequest {
            sample_rate: 48_000.0,
            engagement: 1,
        };

        inputs.telemetry.signals.request_capture(48_000.0, 2);
        capture(&gate.0, &inputs, first);
        assert!(
            inputs.bank.take().is_none(),
            "the overtaken render was published"
        );

        let _ = inputs.telemetry.signals.take_capture_request();
        capture(&gate.0, &inputs, first);
        let kit = inputs
            .bank
            .take()
            .expect("with nothing newer waiting, the render is published");
        assert_eq!(kit.engagement, 1, "stamped with the engage that asked");
        assert_eq!(
            gate.heard_for(1).expect("remembered").name,
            "Kit A",
            "and what it was captured from"
        );
    }

    /// The one pack under `root`: its name, and every file in it by name.
    fn pack_files(root: &Path) -> (std::ffi::OsString, Vec<(std::ffi::OsString, Vec<u8>)>) {
        let pack = std::fs::read_dir(root)
            .expect("the root")
            .map(|entry| entry.expect("an entry").path())
            .find(|path| path.is_dir())
            .expect("a pack");
        let mut files: Vec<_> = std::fs::read_dir(&pack)
            .expect("the pack")
            .map(|entry| {
                let path = entry.expect("an entry").path();
                (
                    path.file_name().expect("a name").to_owned(),
                    std::fs::read(&path).expect("a file"),
                )
            })
            .collect();
        files.sort();
        (pack.file_name().expect("a name").to_owned(), files)
    }

    /// **Export writes the kit heard, under its name** (model drums' review, 2026-10-26): frozen,
    /// an edit renders nothing and a kit loaded since is not what plays, so the pack is the patches
    /// the playing kit was captured from, named for it, its live control as set now; with no kit
    /// installed, no pack and why.
    #[test]
    fn an_export_while_frozen_writes_the_kit_heard_not_an_edit_since() {
        let inputs = inputs(Fake::new(1, 0));
        let gate = Gate::default();
        let (heard, name) = inputs.capturer.read();
        gate.remember(
            5,
            Heard {
                patches: heard,
                name,
            },
        );
        inputs.telemetry.signals.set_frozen(Some(5));
        // Kit B loaded while frozen: slot 1's sound edited, which the capture bakes in, and its
        // live control moved.
        inputs.capturer.set(0, [9, 200], "Kit B");

        let frozen = crate::pack::tests::scratch("capture-heard");
        export(&gate.0, &inputs, 48_000.0, &frozen);
        let reference = crate::pack::tests::scratch("capture-heard-reference");
        let played = Fake::live(&heard, &inputs.capturer.read().0);
        assert_eq!(played[0], [3, 200]);
        let shots = Shots {
            captured: &heard,
            played: &played,
            rate: 48_000.0,
        };
        crate::pack::write(&reference, "Kit A", &shots).expect("a pack");
        assert_eq!(
            pack_files(&frozen),
            pack_files(&reference),
            "the pack is the kit heard, under its name"
        );
        assert!(matches!(
            inputs.telemetry.signals.take_export(),
            Some(Export::Written { files: 1, .. })
        ));

        inputs.telemetry.signals.set_frozen(None);
        let live = crate::pack::tests::scratch("capture-heard-live");
        export(&gate.0, &inputs, 48_000.0, &live);
        assert!(
            matches!(inputs.telemetry.signals.take_export(), Some(Export::Failed(why)) if why == EXPORT_NOT_READY),
            "with no kit installed, Export says why"
        );
        assert_eq!(
            std::fs::read_dir(&live).expect("the root").count(),
            0,
            "and writes nothing"
        );
        for root in [frozen, reference, live] {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn dropping_the_worker_stops_it_promptly_even_mid_capture() {
        // Teardown is the whole reason this type exists, so it is timed rather than assumed: a
        // capture of ten thousand 1 ms chunks, dropped a moment in.
        let inputs = inputs(Fake::new(10_000, 1));
        let telemetry = Arc::clone(&inputs.telemetry);
        let gate = Gate::default();
        let worker = CaptureWorker::spawn(&gate, inputs);
        telemetry.signals.request_capture(48_000.0, 1);
        gate.nudge();
        std::thread::sleep(std::time::Duration::from_millis(60));

        let started = std::time::Instant::now();
        drop(worker);
        let took = started.elapsed();
        assert!(
            took < std::time::Duration::from_secs(1),
            "joining took {took:?}, so a chunk is not bounding teardown"
        );
    }

    #[test]
    fn the_gate_works_on_both_sides_of_the_threads_life() {
        // An editor can be opened before `activate` spawns the thread and closed after teardown
        // has joined it, so neither end may be an error. Asked early, the request waits; asked
        // late, it is dropped. Both are checked here because both are documented.
        let gate = Gate::default();
        let inputs = inputs(Fake::new(1, 0));
        let telemetry = Arc::clone(&inputs.telemetry);

        // Into the build's own folder, never the real sample-pack folder, and never on C:.
        let into = crate::pack::tests::scratch("capture-gate");

        // Before the thread exists.
        gate.request_export(48_000.0, into.clone());
        let worker = CaptureWorker::spawn(&gate, inputs);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            // Wrote somewhere or said why; either is the worker having picked the request up.
            if telemetry.signals.take_export().is_some() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "a request made before the thread started was lost"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // After it has stopped: inert, and above all not a panic.
        drop(worker);
        gate.request_export(48_000.0, into.clone());
        gate.nudge();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            telemetry.signals.take_export().is_none(),
            "a stopped worker answered a request"
        );
        let _ = std::fs::remove_dir_all(into);
    }

    #[test]
    fn a_worker_that_is_never_asked_for_anything_still_joins() {
        let worker = CaptureWorker::spawn(&Gate::default(), inputs(Fake::new(1, 0)));
        let started = std::time::Instant::now();
        drop(worker);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "an idle worker should stop on the first wake"
        );
    }

    /// The state machine without a thread: a test stands in for the worker, as a plugin's do.
    fn resample() -> (Resample<Fake>, Engine, Arc<Telemetry>) {
        let mut resample = Resample::new(Fake::new(1, 0));
        let mut engine = Engine::default();
        let telemetry = Arc::new(Telemetry::default());
        resample.activate(&mut engine, false, &telemetry, 48_000.0);
        resample.worker = None;
        (resample, engine, telemetry)
    }

    /// Engaged at activation, the kit is captured then and plays from the first callback; an
    /// activation at the same rate keeps a kit whose patch has not moved and captures one that has.
    #[test]
    fn an_activation_captures_now_and_keeps_an_unmoved_kit() {
        let (mut resample, mut engine, telemetry) = resample();
        resample.activate(&mut engine, true, &telemetry, 48_000.0);
        assert!(Fake::is_frozen(&engine), "frozen at once");
        let first = resample.engagement;
        assert_eq!(telemetry.signals.frozen(), Some(first));
        assert_eq!(resample.capturer().begun.load(Ordering::Relaxed), 1);

        resample.activate(&mut engine, true, &telemetry, 48_000.0);
        assert_eq!(
            resample.capturer().begun.load(Ordering::Relaxed),
            1,
            "kept, not captured again"
        );
        assert_ne!(telemetry.signals.frozen(), Some(first), "this engage's now");

        resample.capturer().set(0, [4, 7], "Kit B");
        resample.activate(&mut engine, true, &telemetry, 48_000.0);
        assert_eq!(
            resample.capturer().begun.load(Ordering::Relaxed),
            2,
            "moved: captured"
        );
        assert_eq!(engine.kit.as_ref().expect("a kit").patches[0], [4, 7]);
        let heard = resample
            .gate
            .heard_for(resample.engagement)
            .expect("remembered");
        assert_eq!(heard.name, "Kit B");
    }

    /// Engaged while running, `process` asks once; only the kit of this engage at this rate
    /// installs; a kit of an engage since left is retired unplayed; a disengage takes the kit
    /// away at once.
    #[test]
    fn process_asks_once_and_installs_only_this_engages_kit() {
        let (mut resample, mut engine, telemetry) = resample();
        let signals = &telemetry.signals;
        resample.service(&mut engine, true, signals, 48_000.0);
        let asked = signals.take_capture_request().expect("asked");
        resample.service(&mut engine, true, signals, 48_000.0);
        assert_eq!(signals.take_capture_request(), None, "once an engage");

        // Off, then on: the first engage's kit lands after the second asked.
        resample.service(&mut engine, false, signals, 48_000.0);
        resample.service(&mut engine, true, signals, 48_000.0);
        let second = signals.take_capture_request().expect("asked again");
        assert_ne!(second.engagement, asked.engagement);
        resample.bank.publish(Kit {
            engagement: asked.engagement,
            rate: 48_000.0,
            patches: [[1, 1]; SLOT_COUNT],
        });
        resample.service(&mut engine, true, signals, 48_000.0);
        assert!(!Fake::is_frozen(&engine), "a stale kit is never installed");
        resample.bank.drain();

        resample.bank.publish(Kit {
            engagement: second.engagement,
            rate: 48_000.0,
            patches: [[2, 2]; SLOT_COUNT],
        });
        resample.service(&mut engine, true, signals, 48_000.0);
        assert!(Fake::is_frozen(&engine), "this engage's kit installs");
        assert_eq!(signals.frozen(), Some(second.engagement));

        resample.service(&mut engine, false, signals, 48_000.0);
        assert!(!Fake::is_frozen(&engine), "a disengage is immediate");
        assert_eq!(signals.frozen(), None);
        resample.bank.drain();
    }
}
