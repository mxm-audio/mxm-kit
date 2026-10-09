//! `IComponentHandler`: how an edit controller tells the host a parameter was edited, and that
//! something about the component changed.
//!
//! VST3 leaves both to the host: an edit made in the plugin's editor reaches the processor only
//! when the host passes it on, and a restart request is the host's to carry out. Here an edit goes
//! onto a queue the audio thread drains at the start of the next block (or `params.flush` while the
//! plugin is inactive), where it becomes the CLAP gesture and value events a host records, and is
//! handed to the processor. Restart requests and the dirty flag are kept for the main thread.

use clack_extensions::params::HostParams;
use clack_plugin::host::HostSharedHandle;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use vst3::Class;
use vst3::Steinberg::Vst::{
    IComponentHandler, IComponentHandler2, IComponentHandler2Trait, IComponentHandlerTrait,
    ParamID, ParamValue,
};
use vst3::Steinberg::{FIDString, TBool, int32, kResultFalse, kResultOk, tresult};

/// How many edits may wait for the audio thread. An editor sends a few per mouse move.
pub(crate) const EDIT_QUEUE: usize = 4096;

/// One edit from the plugin's editor, as `IComponentHandler` received it. Values are normalised.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Edit {
    Begin(ParamID),
    Value(ParamID, ParamValue),
    End(ParamID),
}

/// What the handler shares with the plugin.
pub(crate) struct HandlerState {
    /// Valid while the plugin instance is: the controller is told to forget the handler before
    /// the instance goes.
    host: HostSharedHandle<'static>,
    host_params: Option<HostParams>,
    edits: Mutex<rtrb::Producer<Edit>>,
    /// The `RestartFlags` asked for and not yet acted on.
    pub restart: AtomicI32,
    /// Whether the plugin said its state changed.
    pub dirty: AtomicBool,
    /// Edits the full queue turned away.
    pub lost_edits: AtomicU32,
}

impl HandlerState {
    /// The state, and the consuming end of its edit queue.
    ///
    /// # Safety
    ///
    /// `host` must stay valid for as long as the state is reachable, which the plugin ensures by
    /// unsetting the controller's handler before it is destroyed.
    pub(crate) unsafe fn new(host: HostSharedHandle<'_>) -> (Arc<Self>, rtrb::Consumer<Edit>) {
        let (producer, consumer) = rtrb::RingBuffer::new(EDIT_QUEUE);
        // SAFETY: the caller's.
        let host: HostSharedHandle<'static> = unsafe { host.with_arbitrary_lifetime() };
        let host_params = host.get_extension::<HostParams>();
        let state = Self {
            host,
            host_params,
            edits: Mutex::new(producer),
            restart: AtomicI32::new(0),
            dirty: AtomicBool::new(false),
            lost_edits: AtomicU32::new(0),
        };
        (Arc::new(state), consumer)
    }

    fn send(&self, edit: Edit) -> tresult {
        let pushed = self
            .edits
            .lock()
            .map(|mut edits| edits.push(edit).is_ok())
            .unwrap_or(false);
        if !pushed {
            self.lost_edits.fetch_add(1, Ordering::Relaxed);
        }
        // CLAP: the host then calls `process` or `params.flush`, whichever applies.
        match &self.host_params {
            Some(params) => params.request_flush(&self.host),
            None => self.host.request_process(),
        }
        if pushed { kResultOk } else { kResultFalse }
    }

    /// Asks the host for a main-thread callback, where restarts and the dirty flag are acted on.
    fn call_back(&self) {
        self.host.request_callback();
    }
}

/// The handler a controller is given.
pub(crate) struct ComponentHandler {
    state: Arc<HandlerState>,
}

impl ComponentHandler {
    pub(crate) fn new(state: Arc<HandlerState>) -> Self {
        Self { state }
    }
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler, IComponentHandler2);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, id: ParamID) -> tresult {
        self.state.send(Edit::Begin(id))
    }

    unsafe fn performEdit(&self, id: ParamID, value_normalized: ParamValue) -> tresult {
        self.state.send(Edit::Value(id, value_normalized))
    }

    unsafe fn endEdit(&self, id: ParamID) -> tresult {
        self.state.send(Edit::End(id))
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        // Kept for the main thread: plugins call this from whichever thread they are on.
        self.state.restart.fetch_or(flags, Ordering::AcqRel);
        self.state.call_back();
        kResultOk
    }
}

impl IComponentHandler2Trait for ComponentHandler {
    unsafe fn setDirty(&self, state: TBool) -> tresult {
        if state != 0 {
            self.state.dirty.store(true, Ordering::Release);
            self.state.call_back();
        }
        kResultOk
    }

    unsafe fn requestOpenEditor(&self, _name: FIDString) -> tresult {
        // The host decides when an editor opens.
        kResultFalse
    }

    unsafe fn startGroupEdit(&self) -> tresult {
        kResultOk
    }

    unsafe fn finishGroupEdit(&self) -> tresult {
        kResultOk
    }
}
