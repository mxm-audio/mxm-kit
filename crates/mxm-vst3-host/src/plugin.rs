//! The CLAP plugin a VST3 class is offered as.
//!
//! [`Shared`] holds the VST3 instance; [`MainThread`] what the controller says (parameters, buses)
//! and the editor; [`AudioProcessor`] everything one block needs, sized at activation so `process`
//! allocates nothing.
//!
//! **Where a value goes.** The host's values reach the processor as `IParameterChanges` points at
//! their sample, and the controller (which the editor draws) through a queue the main thread
//! drains. The editor's edits come the other way (`handler.rs`). Values that arrive while the
//! plugin is inactive, or between blocks (`params.flush`), are held and given to the processor at
//! the start of its next block. A parameter's CLAP modulation is added to its value there, so the
//! processor hears value plus modulation while the controller keeps showing the value.

use crate::component::{Component, Layout};
use crate::events::{EventList, note_off, note_on, poly_pressure};
use crate::gui::Editor;
use crate::handler::{Edit, HandlerState};
use crate::module::{ClassInfo, Module};
use crate::params::{ParamInfo, ParameterChanges, read_params, to_clap, to_normalized};
use crate::strings::{from_utf16, to_string128};
use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortRescanFlags, AudioPortType,
    HostAudioPorts, PluginAudioPorts, PluginAudioPortsImpl,
};
use clack_extensions::gui::PluginGui;
use clack_extensions::latency::{HostLatency, PluginLatency, PluginLatencyImpl};
use clack_extensions::note_ports::{
    HostNotePorts, NoteDialect, NoteDialects, NotePortInfo, NotePortInfoWriter,
    NotePortRescanFlags, PluginNotePorts, PluginNotePortsImpl,
};
use clack_extensions::params::{
    HostParams, ParamDisplayWriter, ParamInfo as ClapParamInfo, ParamInfoWriter, ParamRescanFlags,
    PluginAudioProcessorParams, PluginMainThreadParams, PluginParams,
};
use clack_extensions::render::{PluginRender, PluginRenderImpl, RenderMode};
use clack_extensions::state::{HostState, PluginState, PluginStateImpl};
use clack_extensions::tail::{PluginTail, PluginTailImpl, TailLength};
use clack_extensions::timer::PluginTimer;
use clack_plugin::events::event_types::{
    MidiEvent, NoteChokeEvent, NoteOffEvent, NoteOnEvent, ParamGestureBeginEvent,
    ParamGestureEndEvent, ParamModEvent, ParamValueEvent, TransportEvent, TransportFlags,
};
use clack_plugin::events::{Match, Pckn, UnknownEvent};
use clack_plugin::prelude::*;
use clack_plugin::stream::{InputStream, OutputStream};
use clack_plugin::utils::Cookie;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use vst3::ComWrapper;
use vst3::Steinberg::Vst::ControllerNumbers_::{kAfterTouch, kPitchBend};
use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_::{
    kBarPositionValid, kContTimeValid, kCycleActive, kCycleValid, kPlaying, kProjectTimeMusicValid,
    kRecording, kTempoValid, kTimeSigValid,
};
use vst3::Steinberg::Vst::ProcessModes_::{kOffline, kRealtime};
use vst3::Steinberg::Vst::RestartFlags_::{
    kIoChanged, kIoTitlesChanged, kLatencyChanged, kMidiCCAssignmentChanged,
    kParamIDMappingChanged, kParamTitlesChanged, kParamValuesChanged, kReloadComponent,
};
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, IAudioProcessorTrait, IComponentTrait,
    IEditControllerTrait, IMidiMapping, IMidiMappingTrait, ParamID, ProcessContext, ProcessData,
    ProcessSetup,
};
use vst3::Steinberg::kResultOk;

/// How many host and editor values may wait for the controller between main-thread turns.
const CONTROLLER_QUEUE: usize = 8192;
/// The note events one block can carry to the processor, and take back from it.
const EVENTS_PER_BLOCK: usize = 2048;
/// The points one parameter can have within a block.
const POINTS_PER_BLOCK: usize = 64;
/// MIDI channels × the controller numbers `IMidiMapping` knows (128 controllers, aftertouch,
/// pitch bend).
const MIDI_CONTROLLERS: usize = 130;
/// A parameter no MIDI controller maps to.
const UNMAPPED: ParamID = ParamID::MAX;

pub(crate) struct Vst3Plugin;

impl Plugin for Vst3Plugin {
    type AudioProcessor<'a> = AudioProcessor<'a>;
    type Shared<'a> = Shared<'a>;
    type MainThread<'a> = MainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Shared<'_>>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginNotePorts>()
            .register::<PluginParams>()
            .register::<PluginState>()
            .register::<PluginLatency>()
            .register::<PluginTail>()
            .register::<PluginRender>()
            .register::<PluginGui>()
            .register::<PluginTimer>();
        #[cfg(unix)]
        builder.register::<clack_extensions::posix_fd::PluginPosixFd>();
    }
}

/// What both threads share: the VST3 instance.
pub(crate) struct Shared<'a> {
    _host: HostSharedHandle<'a>,
    pub(crate) vst: Component,
    pub(crate) handler: Arc<HandlerState>,
    pub(crate) name: String,
    /// An instrument's audio input is not offered (see [`offered`]).
    instrument: bool,
    /// The processor's tail, read on the main thread at activation, for `tail.get` on the audio
    /// thread, which VST3 does not allow to ask.
    tail: AtomicU32,
    active: AtomicBool,
}

impl<'a> PluginShared<'a> for Shared<'a> {}

impl<'a> Shared<'a> {
    pub(crate) fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
}

/// The layout as the host is offered it. An instrument's audio input is fed silence and not
/// offered: hosts take an instrument as a source without one (MXM Player refuses one that has an
/// input), and many VST3 synthesisers declare one only for effects or sidechains.
fn offered(mut layout: Layout, instrument: bool) -> Layout {
    if instrument {
        layout.main_input = None;
    }
    layout
}

fn error(message: String) -> PluginError {
    PluginError::Error(message.into())
}

/// The plugin instance, created on the host's main thread in CLAP `init`.
pub(crate) fn create<'a>(
    host: &HostMainThreadHandle<'a>,
    module: Arc<Module>,
    class: &ClassInfo,
) -> Result<(Shared<'a>, MainThreadParts), PluginError> {
    let host_name = host
        .shared()
        .name()
        .and_then(|name| name.to_str().ok())
        .unwrap_or("MXM")
        .to_owned();
    // SAFETY: `Component`'s drop unsets the controller's handler before the instance, and with it
    // the host handle, goes.
    let (handler, edits) = unsafe { HandlerState::new(host.shared()) };
    // SAFETY: on the main thread, as CLAP `init` is.
    let vst =
        unsafe { Component::create(module, class, &host_name, handler.clone()) }.map_err(error)?;
    // SAFETY: on the main thread, inactive.
    let layout = offered(unsafe { vst.layout() }, class.is_instrument());
    let params = match &vst.controller {
        // SAFETY: on the main thread.
        Some(controller) => unsafe { read_params(controller) },
        None => Vec::new(),
    };
    let shared = Shared {
        _host: host.shared(),
        vst,
        handler,
        name: class.name.clone(),
        instrument: class.is_instrument(),
        tail: AtomicU32::new(0),
        active: AtomicBool::new(false),
    };
    Ok((
        shared,
        MainThreadParts {
            layout,
            params,
            edits,
        },
    ))
}

/// What `create` hands the main thread, beyond the shared part.
pub(crate) struct MainThreadParts {
    layout: Layout,
    params: Vec<ParamInfo>,
    edits: rtrb::Consumer<Edit>,
}

/// The main thread's part: the controller's view of the plugin, and the editor.
pub(crate) struct MainThread<'a> {
    pub(crate) host: HostMainThreadHandle<'a>,
    pub(crate) shared: &'a Shared<'a>,
    layout: Layout,
    params: Vec<ParamInfo>,
    param_index: HashMap<ParamID, usize>,
    /// The editor's edits, while inactive; the audio processor holds them while active.
    edits: Option<rtrb::Consumer<Edit>>,
    /// Values for the controller, from the audio thread.
    to_controller: rtrb::Consumer<(ParamID, f64)>,
    /// The other end, while inactive; the audio processor holds it while active.
    to_controller_producer: Option<rtrb::Producer<(ParamID, f64)>>,
    /// Values that arrived while inactive, for the processor's first block.
    carry: Vec<(ParamID, f64)>,
    offline: bool,
    /// Restart flags that need the plugin inactive, kept until it is.
    deferred_restart: i32,
    pub(crate) editor: Option<Editor>,
}

impl<'a> MainThread<'a> {
    pub(crate) fn new(
        host: HostMainThreadHandle<'a>,
        shared: &'a Shared<'a>,
        parts: MainThreadParts,
    ) -> Self {
        let (to_controller_producer, to_controller) = rtrb::RingBuffer::new(CONTROLLER_QUEUE);
        let mut this = Self {
            host,
            shared,
            layout: parts.layout,
            params: Vec::new(),
            param_index: HashMap::new(),
            edits: Some(parts.edits),
            to_controller,
            to_controller_producer: Some(to_controller_producer),
            carry: Vec::new(),
            offline: false,
            deferred_restart: 0,
            editor: None,
        };
        this.set_params(parts.params);
        this
    }

    fn set_params(&mut self, params: Vec<ParamInfo>) {
        self.param_index = params
            .iter()
            .enumerate()
            .map(|(index, param)| (param.id, index))
            .collect();
        self.params = params;
    }

    fn param(&self, id: ParamID) -> Option<&ParamInfo> {
        self.param_index.get(&id).map(|&index| &self.params[index])
    }

    /// Holds `value` (normalised) for the processor's next block, replacing any held for `id`.
    fn carry(&mut self, id: ParamID, value: f64) {
        match self.carry.iter_mut().find(|(held, _)| *held == id) {
            Some(held) => held.1 = value,
            None => self.carry.push((id, value)),
        }
    }

    fn host_params(&self) -> Option<HostParams> {
        self.host.shared().get_extension::<HostParams>()
    }

    /// Acts on what the component asked for (`IComponentHandler::restartComponent`).
    fn restart(&mut self, flags: i32) {
        let active = self.shared.is_active();
        // The flags' type is a platform's enum type; as `i32` they compare on every platform.
        let has = |flag| flags & flag as i32 != 0;
        let needs_inactive = (kIoChanged
            | kParamIDMappingChanged
            | kReloadComponent
            | kMidiCCAssignmentChanged
            | kLatencyChanged) as i32;
        if active {
            if flags & needs_inactive != 0 {
                self.deferred_restart |= flags & needs_inactive;
                self.host.shared().request_restart();
            }
        } else {
            if has(kIoChanged) || has(kReloadComponent) {
                // SAFETY: on the main thread, inactive.
                self.layout = offered(unsafe { self.shared.vst.layout() }, self.shared.instrument);
                if let Some(ports) = self.host.shared().get_extension::<HostAudioPorts>() {
                    ports.rescan(&mut self.host, AudioPortRescanFlags::LIST);
                }
                if let Some(ports) = self.host.shared().get_extension::<HostNotePorts>() {
                    ports.rescan(&mut self.host, NotePortRescanFlags::ALL);
                }
            }
            if has(kParamIDMappingChanged) || has(kReloadComponent) {
                self.reread_params();
                if let Some(params) = self.host_params() {
                    params.rescan(&mut self.host, ParamRescanFlags::ALL);
                }
            }
            if has(kLatencyChanged)
                && let Some(latency) = self.host.shared().get_extension::<HostLatency>()
            {
                latency.changed(&mut self.host);
            }
            if has(kIoTitlesChanged)
                && let Some(ports) = self.host.shared().get_extension::<HostAudioPorts>()
            {
                // SAFETY: on the main thread, inactive.
                self.layout = offered(unsafe { self.shared.vst.layout() }, self.shared.instrument);
                ports.rescan(&mut self.host, AudioPortRescanFlags::NAMES);
            }
        }
        if has(kParamTitlesChanged) {
            self.reread_params();
            if let Some(params) = self.host_params() {
                params.rescan(
                    &mut self.host,
                    ParamRescanFlags::TEXT | ParamRescanFlags::INFO,
                );
            }
        }
        if has(kParamValuesChanged)
            && let Some(params) = self.host_params()
        {
            params.rescan(&mut self.host, ParamRescanFlags::VALUES);
        }
    }

    fn reread_params(&mut self) {
        if let Some(controller) = &self.shared.vst.controller {
            // SAFETY: on the main thread.
            let params = unsafe { read_params(controller) };
            self.set_params(params);
        }
    }
}

impl<'a> PluginMainThread<'a, Shared<'a>> for MainThread<'a> {
    fn on_main_thread(&mut self) {
        if let Some(controller) = &self.shared.vst.controller {
            while let Ok((id, value)) = self.to_controller.pop() {
                // SAFETY: on the main thread.
                unsafe { controller.setParamNormalized(id, value) };
            }
        }
        let flags = self.shared.handler.restart.swap(0, Ordering::AcqRel);
        let deferred = if self.shared.is_active() {
            0
        } else {
            std::mem::take(&mut self.deferred_restart)
        };
        if flags | deferred != 0 {
            self.restart(flags | deferred);
        }
        if self.shared.handler.dirty.swap(false, Ordering::AcqRel)
            && let Some(mut state) = self.host.shared().get_extension::<HostState>()
        {
            state.mark_dirty(&self.host);
        }
        self.service_editor();
    }
}

impl PluginAudioPortsImpl for MainThread<'_> {
    fn count(&mut self, is_input: bool) -> u32 {
        let main = if is_input {
            self.layout.main_input
        } else {
            self.layout.main_output
        };
        u32::from(main.is_some())
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        let (buses, main) = if is_input {
            (&self.layout.inputs, self.layout.main_input)
        } else {
            (&self.layout.outputs, self.layout.main_output)
        };
        let Some(bus) = main.filter(|_| index == 0).map(|main| &buses[main]) else {
            return;
        };
        let name = if bus.name.is_empty() {
            if is_input { "Input" } else { "Output" }
        } else {
            &bus.name
        };
        writer.set(&AudioPortInfo {
            id: ClapId::new(0),
            name: name.as_bytes(),
            channel_count: bus.channels,
            flags: AudioPortFlags::IS_MAIN,
            port_type: AudioPortType::from_channel_count(bus.channels),
            in_place_pair: None,
        });
    }
}

impl PluginNotePortsImpl for MainThread<'_> {
    fn count(&mut self, is_input: bool) -> u32 {
        u32::from(is_input && self.layout.note_input)
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut NotePortInfoWriter) {
        if index != 0 || !is_input || !self.layout.note_input {
            return;
        }
        writer.set(&NotePortInfo {
            id: ClapId::new(0),
            name: b"Notes",
            supported_dialects: NoteDialects::CLAP | NoteDialects::MIDI,
            preferred_dialect: Some(NoteDialect::Clap),
        });
    }
}

impl PluginMainThreadParams for MainThread<'_> {
    fn count(&mut self) -> u32 {
        self.params.len() as u32
    }

    fn get_info(&mut self, param_index: u32, info: &mut ParamInfoWriter) {
        let Some(param) = self.params.get(param_index as usize) else {
            return;
        };
        let Some(id) = ClapId::from_raw(param.id) else {
            return;
        };
        info.set(&ClapParamInfo {
            id,
            flags: param.flags,
            cookie: Cookie::empty(),
            name: param.name.as_bytes(),
            module: param.module.as_bytes(),
            min_value: 0.0,
            max_value: if param.steps == 0 {
                1.0
            } else {
                f64::from(param.steps)
            },
            default_value: param.to_clap(param.default_normalized),
        });
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        let param = self.param(param_id.get())?;
        let controller = self.shared.vst.controller.as_ref()?;
        // SAFETY: on the main thread.
        let normalized = unsafe { controller.getParamNormalized(param.id) };
        Some(param.to_clap(normalized))
    }

    fn value_to_text(
        &mut self,
        param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        let param = self.param(param_id.get()).ok_or(std::fmt::Error)?;
        let controller = self.shared.vst.controller.as_ref().ok_or(std::fmt::Error)?;
        let mut text = [0u16; 128];
        // SAFETY: on the main thread; `text` is a `String128`.
        let result = unsafe {
            controller.getParamStringByValue(param.id, param.to_normalized(value), &mut text)
        };
        if result != kResultOk {
            return Err(std::fmt::Error);
        }
        let text = from_utf16(&text);
        let text = text.trim();
        if param.units.is_empty() || text.ends_with(param.units.as_str()) {
            write!(writer, "{text}")
        } else {
            write!(writer, "{text} {}", param.units)
        }
    }

    fn text_to_value(&mut self, param_id: ClapId, text: &CStr) -> Option<f64> {
        let param = self.param(param_id.get())?;
        let controller = self.shared.vst.controller.as_ref()?;
        let mut text = to_string128(text.to_str().ok()?.trim());
        let mut normalized = 0.0;
        // SAFETY: on the main thread; `text` is NUL-terminated.
        let result = unsafe {
            controller.getParamValueByString(param.id, text.as_mut_ptr(), &mut normalized)
        };
        (result == kResultOk).then(|| param.to_clap(normalized))
    }

    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        output_parameter_changes: &mut OutputEvents,
    ) {
        // Inactive: the controller takes the host's values now, the processor at its first block.
        for event in input_parameter_changes {
            let Some(event) = event.as_event::<ParamValueEvent>() else {
                continue;
            };
            let Some(param) = event
                .param_id()
                .and_then(|id| self.param(id.get()))
                .filter(|_| event.pckn().matches_all())
            else {
                continue;
            };
            let (id, normalized) = (param.id, param.to_normalized(event.value()));
            if let Some(controller) = &self.shared.vst.controller {
                // SAFETY: on the main thread.
                unsafe { controller.setParamNormalized(id, normalized) };
            }
            self.carry(id, normalized);
        }
        // The editor's edits: the host records them, the processor gets them later.
        let Some(mut edits) = self.edits.take() else {
            return;
        };
        while let Ok(edit) = edits.pop() {
            match edit {
                Edit::Begin(id) => {
                    if let Some(id) = ClapId::from_raw(id) {
                        let _ =
                            output_parameter_changes.try_push(ParamGestureBeginEvent::new(0, id));
                    }
                }
                Edit::Value(id, normalized) => {
                    if let Some(param) = self.param(id) {
                        let value = param.to_clap(normalized);
                        if let Some(clap_id) = ClapId::from_raw(id) {
                            let _ = output_parameter_changes.try_push(ParamValueEvent::new(
                                0,
                                clap_id,
                                Pckn::match_all(),
                                value,
                                Cookie::empty(),
                            ));
                        }
                        self.carry(id, normalized);
                    }
                }
                Edit::End(id) => {
                    if let Some(id) = ClapId::from_raw(id) {
                        let _ = output_parameter_changes.try_push(ParamGestureEndEvent::new(0, id));
                    }
                }
            }
        }
        self.edits = Some(edits);
    }
}

impl PluginStateImpl for MainThread<'_> {
    fn save(&mut self, output: &mut OutputStream) -> Result<(), PluginError> {
        // SAFETY: on the main thread.
        let blob = unsafe { self.shared.vst.save_state() }.map_err(error)?;
        output.write_all(&blob)?;
        Ok(())
    }

    fn load(&mut self, input: &mut InputStream) -> Result<(), PluginError> {
        let mut blob = Vec::new();
        input.read_to_end(&mut blob)?;
        // SAFETY: on the main thread.
        unsafe { self.shared.vst.load_state(&blob) }.map_err(error)?;
        // What was held from before belongs to the old state.
        self.carry.clear();
        if let Some(params) = self.host_params() {
            params.rescan(&mut self.host, ParamRescanFlags::VALUES);
        }
        Ok(())
    }
}

impl PluginLatencyImpl for MainThread<'_> {
    fn get(&mut self) -> u32 {
        // SAFETY: on the main thread.
        unsafe { self.shared.vst.processor.getLatencySamples() }
    }
}

impl PluginRenderImpl for MainThread<'_> {
    fn has_hard_realtime_requirement(&self) -> bool {
        false
    }

    fn set(&mut self, mode: RenderMode) -> Result<(), PluginError> {
        let offline = mode == RenderMode::Offline;
        if offline != self.offline {
            self.offline = offline;
            // VST3 takes the process mode at setup, so it applies from the next activation.
            if self.shared.is_active() {
                self.host.shared().request_restart();
            }
        }
        Ok(())
    }
}

/// The host's and editor's values, as the processor should hear them.
struct ParamTable {
    index: HashMap<ParamID, usize>,
    ids: Vec<ParamID>,
    steps: Vec<u32>,
    /// The value the host or editor last set, normalised.
    base: Vec<f64>,
    /// The CLAP modulation on it, normalised.
    modulation: Vec<f64>,
    /// Parameters to send at the start of the next block.
    pending: Vec<usize>,
    is_pending: Vec<bool>,
}

impl ParamTable {
    fn new(params: &[ParamInfo], controller_values: impl Fn(ParamID) -> f64) -> Self {
        let count = params.len();
        Self {
            index: params
                .iter()
                .enumerate()
                .map(|(index, param)| (param.id, index))
                .collect(),
            ids: params.iter().map(|param| param.id).collect(),
            steps: params.iter().map(|param| param.steps).collect(),
            base: params
                .iter()
                .map(|param| controller_values(param.id))
                .collect(),
            modulation: vec![0.0; count],
            pending: Vec::with_capacity(count),
            is_pending: vec![false; count],
        }
    }

    fn effective(&self, index: usize) -> f64 {
        (self.base[index] + self.modulation[index]).clamp(0.0, 1.0)
    }

    fn mark_pending(&mut self, index: usize) {
        if !self.is_pending[index] {
            self.is_pending[index] = true;
            self.pending.push(index);
        }
    }
}

/// One VST3 bus's buffers: the channel pointers the processor reads or writes, and the memory
/// behind the ones the host does not supply.
struct BusBuffers {
    channels: u32,
    pointers: Vec<*mut f32>,
    scratch: Vec<Vec<f32>>,
}

impl BusBuffers {
    fn new(channels: u32, frames: u32) -> Self {
        let mut scratch: Vec<Vec<f32>> =
            (0..channels).map(|_| vec![0.0; frames as usize]).collect();
        let pointers = scratch
            .iter_mut()
            .map(|channel| channel.as_mut_ptr())
            .collect();
        Self {
            channels,
            pointers,
            scratch,
        }
    }

    /// Points every channel at this bus's own memory again.
    fn use_scratch(&mut self) {
        for (pointer, channel) in self.pointers.iter_mut().zip(&mut self.scratch) {
            *pointer = channel.as_mut_ptr();
        }
    }
}

/// The audio thread's part: everything one block needs, made at activation.
pub(crate) struct AudioProcessor<'a> {
    host: HostAudioProcessorHandle<'a>,
    shared: &'a Shared<'a>,
    layout: Layout,
    process_mode: i32,
    sample_rate: f64,
    params: ParamTable,
    midi_map: Vec<ParamID>,
    edits: rtrb::Consumer<Edit>,
    to_controller: rtrb::Producer<(ParamID, f64)>,
    controller_waiting: bool,
    inputs: Vec<BusBuffers>,
    outputs: Vec<BusBuffers>,
    input_headers: Vec<AudioBusBuffers>,
    output_headers: Vec<AudioBusBuffers>,
    input_changes: ComWrapper<ParameterChanges>,
    output_changes: ComWrapper<ParameterChanges>,
    input_events: ComWrapper<EventList>,
    output_events: ComWrapper<EventList>,
    context: Box<ProcessContext>,
    /// Frames processed since activation, for a project time when the host gives none.
    frames_done: i64,
}

// SAFETY: the raw pointers are into this struct's own buffers and the host's, used only inside
// `process`; the processor as a whole moves between audio threads as CLAP allows, never shared.
unsafe impl Send for AudioProcessor<'_> {}

impl<'a> PluginAudioProcessor<'a, Shared<'a>, MainThread<'a>> for AudioProcessor<'a> {
    fn activate(
        host: HostAudioProcessorHandle<'a>,
        main_thread: &mut MainThread<'a>,
        shared: &'a Shared<'a>,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let vst = &shared.vst;
        let max_frames = audio_config.max_frames_count.max(1);
        let process_mode = (if main_thread.offline {
            kOffline
        } else {
            kRealtime
        }) as i32;
        let mut setup = ProcessSetup {
            processMode: process_mode,
            symbolicSampleSize: kSample32 as i32,
            maxSamplesPerBlock: max_frames as i32,
            sampleRate: audio_config.sample_rate,
        };
        // SAFETY: on the main thread, as CLAP activates; the component is inactive.
        unsafe {
            if vst.processor.setupProcessing(&mut setup) != kResultOk {
                return Err(PluginError::Message("the VST3 processor refused its setup"));
            }
            if vst.component.setActive(1) != kResultOk {
                return Err(PluginError::Message("the VST3 component did not activate"));
            }
            shared
                .tail
                .store(vst.processor.getTailSamples(), Ordering::Release);
        }

        let edits = main_thread
            .edits
            .take()
            .ok_or(PluginError::Message("already active"))?;
        let to_controller = main_thread
            .to_controller_producer
            .take()
            .ok_or(PluginError::Message("already active"))?;

        let controller = vst.controller.clone();
        let mut params = ParamTable::new(&main_thread.params, |id| match &controller {
            // SAFETY: on the main thread.
            Some(controller) => unsafe { controller.getParamNormalized(id) },
            None => 0.0,
        });
        for (id, value) in main_thread.carry.drain(..) {
            if let Some(&index) = params.index.get(&id) {
                params.base[index] = value;
                params.mark_pending(index);
            }
        }

        let mut midi_map = vec![UNMAPPED; 16 * MIDI_CONTROLLERS];
        if main_thread.layout.note_input
            && let Some(mapping) = controller
                .as_ref()
                .and_then(|controller| controller.cast::<IMidiMapping>())
        {
            for channel in 0..16 {
                for number in 0..MIDI_CONTROLLERS {
                    let mut id: ParamID = 0;
                    // SAFETY: on the main thread; `id` is a valid out-parameter.
                    if unsafe {
                        mapping.getMidiControllerAssignment(
                            0,
                            channel as i16,
                            number as i16,
                            &mut id,
                        )
                    } == kResultOk
                    {
                        midi_map[channel * MIDI_CONTROLLERS + number] = id;
                    }
                }
            }
        }

        let layout = main_thread.layout.clone();
        let inputs: Vec<BusBuffers> = layout
            .inputs
            .iter()
            .map(|bus| BusBuffers::new(bus.channels, max_frames))
            .collect();
        let outputs: Vec<BusBuffers> = layout
            .outputs
            .iter()
            .map(|bus| BusBuffers::new(bus.channels, max_frames))
            .collect();
        let header = |buffers: &BusBuffers| AudioBusBuffers {
            numChannels: buffers.channels as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: buffers.pointers.as_ptr() as *mut *mut f32,
            },
        };
        let input_headers = inputs.iter().map(header).collect();
        let output_headers = outputs.iter().map(header).collect();
        let count = main_thread.params.len();
        let sample_rate = audio_config.sample_rate;

        shared.active.store(true, Ordering::Release);
        Ok(Self {
            host,
            shared,
            layout,
            process_mode,
            sample_rate,
            params,
            midi_map,
            edits,
            to_controller,
            controller_waiting: false,
            inputs,
            outputs,
            input_headers,
            output_headers,
            input_changes: ComWrapper::new(ParameterChanges::new(count, POINTS_PER_BLOCK)),
            output_changes: ComWrapper::new(ParameterChanges::new(count, POINTS_PER_BLOCK)),
            input_events: ComWrapper::new(EventList::new(EVENTS_PER_BLOCK)),
            output_events: ComWrapper::new(EventList::new(EVENTS_PER_BLOCK)),
            // SAFETY: `ProcessContext` is plain data; all zeros is "nothing valid".
            context: Box::new(unsafe { std::mem::zeroed() }),
            frames_done: 0,
        })
    }

    fn deactivate(self, main_thread: &mut MainThread<'a>) {
        // SAFETY: on the main thread, as CLAP deactivates.
        unsafe { self.shared.vst.component.setActive(0) };
        self.shared.active.store(false, Ordering::Release);
        // Values that were waiting go to the processor's next first block.
        for &index in &self.params.pending {
            main_thread.carry(self.params.ids[index], self.params.base[index]);
        }
        main_thread.edits = Some(self.edits);
        main_thread.to_controller_producer = Some(self.to_controller);
        if main_thread.deferred_restart != 0 {
            main_thread.host.shared().request_callback();
        }
    }

    fn start_processing(&mut self) -> Result<(), PluginError> {
        // SAFETY: on the audio thread, active.
        unsafe { self.shared.vst.processor.setProcessing(1) };
        Ok(())
    }

    fn stop_processing(&mut self) {
        // SAFETY: on the audio thread, active.
        unsafe { self.shared.vst.processor.setProcessing(0) };
    }

    fn reset(&mut self) {
        // VST3 has no reset; turning processing off and on again is the conventional one.
        // SAFETY: on the audio thread, active.
        unsafe {
            self.shared.vst.processor.setProcessing(0);
            self.shared.vst.processor.setProcessing(1);
        }
    }

    fn process(
        &mut self,
        process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        let frames = audio.frames_count();
        self.input_changes.clear();
        self.output_changes.clear();
        self.input_events.clear();
        self.output_events.clear();

        self.send_pending();
        self.take_edits(events.output, true);
        for event in events.input {
            self.host_event(event, frames, true);
        }
        self.fill_context(process.transport, process.steady_time, frames);
        self.bind_buffers(&mut audio, frames)?;

        let mut data = ProcessData {
            processMode: self.process_mode,
            symbolicSampleSize: kSample32 as i32,
            numSamples: frames as i32,
            numInputs: self.input_headers.len() as i32,
            numOutputs: self.output_headers.len() as i32,
            inputs: self.input_headers.as_mut_ptr(),
            outputs: self.output_headers.as_mut_ptr(),
            inputParameterChanges: ParameterChanges::as_ptr(&self.input_changes),
            outputParameterChanges: ParameterChanges::as_ptr(&self.output_changes),
            inputEvents: EventList::as_ptr(&self.input_events),
            outputEvents: EventList::as_ptr(&self.output_events),
            processContext: &mut *self.context,
        };
        // A processor that fails a block keeps its place: the host would otherwise bypass it for
        // good over one bad call.
        // SAFETY: every pointer in `data` is valid for this call, and the channels hold `frames`.
        unsafe { self.shared.vst.processor.process(&mut data) };

        self.report_outputs(events.output);
        self.mark_silence(&mut audio, frames);
        self.frames_done += i64::from(frames);
        if std::mem::take(&mut self.controller_waiting) {
            self.host.shared().request_callback();
        }
        Ok(ProcessStatus::Continue)
    }
}

impl AudioProcessor<'_> {
    /// Values held since the last block (from `flush`, or from before activation), at its start.
    fn send_pending(&mut self) {
        for &index in &self.params.pending {
            self.params.is_pending[index] = false;
            self.input_changes
                .add(self.params.ids[index], 0, self.params.effective(index));
        }
        self.params.pending.clear();
    }

    /// The editor's edits: CLAP events for the host, and values for the processor (now, or at the
    /// next block when `in_block` is false).
    fn take_edits(&mut self, output: &mut OutputEvents, in_block: bool) {
        while let Ok(edit) = self.edits.pop() {
            match edit {
                Edit::Begin(id) => {
                    if let Some(id) = ClapId::from_raw(id) {
                        let _ = output.try_push(ParamGestureBeginEvent::new(0, id));
                    }
                }
                Edit::Value(id, normalized) => {
                    let Some(&index) = self.params.index.get(&id) else {
                        continue;
                    };
                    self.params.base[index] = normalized;
                    self.apply(index, 0, in_block);
                    if let Some(clap_id) = ClapId::from_raw(id) {
                        let _ = output.try_push(ParamValueEvent::new(
                            0,
                            clap_id,
                            Pckn::match_all(),
                            to_clap(self.params.steps[index], normalized),
                            Cookie::empty(),
                        ));
                    }
                }
                Edit::End(id) => {
                    if let Some(id) = ClapId::from_raw(id) {
                        let _ = output.try_push(ParamGestureEndEvent::new(0, id));
                    }
                }
            }
        }
    }

    /// Gives parameter `index`'s value to the processor at `time`, or holds it for the next block.
    fn apply(&mut self, index: usize, time: u32, in_block: bool) {
        if in_block {
            let value = self.params.effective(index);
            self.input_changes
                .add(self.params.ids[index], time as i32, value);
        } else {
            self.params.mark_pending(index);
        }
    }

    /// Tells the controller a value, so the editor shows it.
    fn tell_controller(&mut self, id: ParamID, normalized: f64) {
        if self.to_controller.push((id, normalized)).is_ok() {
            self.controller_waiting = true;
        }
    }

    fn host_event(&mut self, event: &UnknownEvent, frames: u32, in_block: bool) {
        let time = event.header().time().min(frames.saturating_sub(1));
        if let Some(event) = event.as_event::<ParamValueEvent>() {
            if !event.pckn().matches_all() {
                return;
            }
            let Some(&index) = event
                .param_id()
                .and_then(|id| self.params.index.get(&id.get()))
            else {
                return;
            };
            let normalized = to_normalized(self.params.steps[index], event.value());
            self.params.base[index] = normalized;
            self.apply(index, time, in_block);
            self.tell_controller(self.params.ids[index], normalized);
        } else if let Some(event) = event.as_event::<ParamModEvent>() {
            if !event.pckn().matches_all() {
                return;
            }
            let Some(&index) = event
                .param_id()
                .and_then(|id| self.params.index.get(&id.get()))
            else {
                return;
            };
            self.params.modulation[index] =
                event.amount() / f64::from(self.params.steps[index].max(1));
            self.apply(index, time, in_block);
        } else if !in_block || !self.layout.note_input {
            // Notes only mean something inside a block, to a processor that takes them.
        } else if let Some(event) = event.as_event::<NoteOnEvent>() {
            if let Some((channel, key, id)) = note_target(event.pckn()) {
                self.input_events
                    .push(note_on(time, channel, key, event.velocity() as f32, id));
            }
        } else if let Some(event) = event.as_event::<NoteOffEvent>() {
            if let Some((channel, key, id)) = note_target(event.pckn()) {
                self.input_events
                    .push(note_off(time, channel, key, event.velocity() as f32, id));
            }
        } else if let Some(event) = event.as_event::<NoteChokeEvent>() {
            if let Some((channel, key, id)) = note_target(event.pckn()) {
                self.input_events
                    .push(note_off(time, channel, key, 0.0, id));
            }
        } else if let Some(event) = event.as_event::<MidiEvent>() {
            self.midi(time, event.data());
        }
    }

    fn midi(&mut self, time: u32, data: [u8; 3]) {
        let channel = data[0] & 0x0F;
        let unit = |byte: u8| f32::from(byte & 0x7F) / 127.0;
        let key = i16::from(data[1] & 0x7F);
        match data[0] & 0xF0 {
            0x90 if data[2] != 0 => {
                self.input_events
                    .push(note_on(time, i16::from(channel), key, unit(data[2]), -1));
            }
            0x80 | 0x90 => {
                let velocity = if data[0] & 0xF0 == 0x80 {
                    unit(data[2])
                } else {
                    0.0
                };
                self.input_events
                    .push(note_off(time, i16::from(channel), key, velocity, -1));
            }
            0xA0 => {
                self.input_events.push(poly_pressure(
                    time,
                    i16::from(channel),
                    key,
                    unit(data[2]),
                    -1,
                ));
            }
            0xB0 => self.controller(
                time,
                channel,
                usize::from(data[1] & 0x7F),
                f64::from(unit(data[2])),
            ),
            0xD0 => self.controller(
                time,
                channel,
                kAfterTouch as usize,
                f64::from(unit(data[1])),
            ),
            0xE0 => {
                let bend = (u16::from(data[2] & 0x7F) << 7) | u16::from(data[1] & 0x7F);
                self.controller(
                    time,
                    channel,
                    kPitchBend as usize,
                    f64::from(bend) / 16383.0,
                );
            }
            _ => {}
        }
    }

    /// A MIDI controller, through the controller's `IMidiMapping`, as a parameter change.
    fn controller(&mut self, time: u32, channel: u8, number: usize, value: f64) {
        let id = self.midi_map[usize::from(channel) * MIDI_CONTROLLERS + number];
        if id == UNMAPPED {
            return;
        }
        match self.params.index.get(&id) {
            Some(&index) => {
                self.params.base[index] = value;
                self.apply(index, time, true);
            }
            // A hidden parameter the controller did not list: straight to the processor.
            None => {
                self.input_changes.add(id, time as i32, value);
            }
        }
        self.tell_controller(id, value);
    }

    fn fill_context(
        &mut self,
        transport: Option<&TransportEvent>,
        steady_time: Option<u64>,
        frames: u32,
    ) {
        let context = &mut *self.context;
        let mut state = 0u32;
        context.sampleRate = self.sample_rate;
        context.projectTimeSamples = self.frames_done;
        if let Some(steady) = steady_time {
            context.continousTimeSamples = steady as i64;
            state |= kContTimeValid as u32;
        }
        if let Some(transport) = transport {
            let flags = transport.flags;
            if flags.contains(TransportFlags::IS_PLAYING) {
                state |= kPlaying as u32;
            }
            if flags.contains(TransportFlags::IS_RECORDING) {
                state |= kRecording as u32;
            }
            if flags.contains(TransportFlags::IS_LOOP_ACTIVE) {
                state |= (kCycleActive | kCycleValid) as u32;
                context.cycleStartMusic = transport.loop_start_beats.to_float();
                context.cycleEndMusic = transport.loop_end_beats.to_float();
            }
            if flags.contains(TransportFlags::HAS_SECONDS_TIMELINE) {
                context.projectTimeSamples =
                    (transport.song_pos_seconds.to_float() * self.sample_rate).round() as i64;
            }
            if flags.contains(TransportFlags::HAS_BEATS_TIMELINE) {
                context.projectTimeMusic = transport.song_pos_beats.to_float();
                context.barPositionMusic = transport.bar_start.to_float();
                state |= (kProjectTimeMusicValid | kBarPositionValid) as u32;
            }
            if flags.contains(TransportFlags::HAS_TEMPO) {
                context.tempo = transport.tempo;
                state |= kTempoValid as u32;
            }
            if flags.contains(TransportFlags::HAS_TIME_SIGNATURE) {
                context.timeSigNumerator = i32::from(transport.time_signature_numerator);
                context.timeSigDenominator = i32::from(transport.time_signature_denominator);
                state |= kTimeSigValid as u32;
            }
        }
        context.state = state;
        let _ = frames;
    }

    /// Points the processor's buses at this block's memory: the host's input copied into the
    /// main input bus, the host's output as the main output bus, silence everywhere else.
    fn bind_buffers(&mut self, audio: &mut Audio, frames: u32) -> Result<(), PluginError> {
        let frames = frames as usize;
        let (host_inputs, host_outputs) = audio.raw_buffers();
        for (index, bus) in self.inputs.iter_mut().enumerate() {
            let host = (Some(index) == self.layout.main_input)
                .then(|| host_inputs.first())
                .flatten();
            for (channel, scratch) in bus.scratch.iter_mut().enumerate() {
                let source = host.and_then(|buffer| {
                    if buffer.data32.is_null() || buffer.channel_count == 0 {
                        return None;
                    }
                    // A host with fewer channels than the bus repeats its last one.
                    let channel = channel.min(buffer.channel_count as usize - 1);
                    // SAFETY: the host's port has `channel_count` channels of `frames` samples.
                    Some(unsafe { std::slice::from_raw_parts(*buffer.data32.add(channel), frames) })
                });
                match source {
                    Some(source) => scratch[..frames].copy_from_slice(source),
                    None => scratch[..frames].fill(0.0),
                }
            }
        }
        for (index, bus) in self.outputs.iter_mut().enumerate() {
            bus.use_scratch();
            if Some(index) == self.layout.main_output
                && let Some(buffer) = host_outputs.first()
            {
                if buffer.data32.is_null() {
                    return Err(PluginError::Message("the host gave no 32-bit output"));
                }
                for (channel, pointer) in bus.pointers.iter_mut().enumerate() {
                    if channel < buffer.channel_count as usize {
                        // SAFETY: the host's port has `channel_count` channels.
                        *pointer = unsafe { *buffer.data32.add(channel) };
                    }
                }
            }
            for &pointer in &bus.pointers {
                // A processor that writes nothing leaves silence, not the last block.
                // SAFETY: every pointer holds `frames` samples: the host's or the scratch's.
                unsafe { std::slice::from_raw_parts_mut(pointer, frames) }.fill(0.0);
            }
        }
        for (header, bus) in self.input_headers.iter_mut().zip(&self.inputs) {
            header.silenceFlags = 0;
            header.__field0.channelBuffers32 = bus.pointers.as_ptr() as *mut *mut f32;
        }
        for (header, bus) in self.output_headers.iter_mut().zip(&self.outputs) {
            header.silenceFlags = 0;
            header.__field0.channelBuffers32 = bus.pointers.as_ptr() as *mut *mut f32;
        }
        Ok(())
    }

    /// What the processor changed: the host is told, and the controller.
    fn report_outputs(&mut self, output: &mut OutputEvents) {
        let changes = self.output_changes.clone();
        for queue in changes.queues() {
            let id = queue.id();
            let Some(&(offset, normalized)) = queue.points().last() else {
                continue;
            };
            let steps = match self.params.index.get(&id) {
                Some(&index) => {
                    self.params.base[index] = normalized;
                    self.params.steps[index]
                }
                None => 0,
            };
            if let Some(clap_id) = ClapId::from_raw(id) {
                let _ = output.try_push(ParamValueEvent::new(
                    offset.max(0) as u32,
                    clap_id,
                    Pckn::match_all(),
                    to_clap(steps, normalized),
                    Cookie::empty(),
                ));
            }
            self.tell_controller(id, normalized);
        }
    }

    /// A channel the processor flagged silent is made exactly silent, and marked constant.
    fn mark_silence(&mut self, audio: &mut Audio, frames: u32) {
        let Some(main) = self.layout.main_output else {
            return;
        };
        let flags = self.output_headers[main].silenceFlags;
        if flags == 0 {
            return;
        }
        let mut mask = clack_plugin::process::ConstantMask::FULLY_DYNAMIC;
        let bus = &self.outputs[main];
        for (channel, &pointer) in bus.pointers.iter().enumerate().take(64) {
            if flags & (1 << channel) != 0 {
                // SAFETY: as in `bind_buffers`.
                unsafe { std::slice::from_raw_parts_mut(pointer, frames as usize) }.fill(0.0);
                mask.set_channel_constant(channel as u64, true);
            }
        }
        if let Some(mut port) = audio.output_port(0) {
            port.set_constant_mask(mask);
        }
    }
}

/// The VST3 channel, key and note ID a CLAP note event addresses, when it addresses one key.
fn note_target(pckn: Pckn) -> Option<(i16, i16, i32)> {
    let Match::Specific(key) = pckn.key else {
        return None;
    };
    let channel = match pckn.channel {
        Match::Specific(channel) => channel as i16,
        Match::All => 0,
    };
    let id = match pckn.note_id {
        Match::Specific(id) => id as i32,
        Match::All => -1,
    };
    Some((channel, key as i16, id))
}

impl PluginAudioProcessorParams for AudioProcessor<'_> {
    fn flush(
        &mut self,
        input_parameter_changes: &InputEvents,
        output_parameter_changes: &mut OutputEvents,
    ) {
        // Active, between blocks: everything is held for the next block.
        self.take_edits(output_parameter_changes, false);
        for event in input_parameter_changes {
            self.host_event(event, 1, false);
        }
        if std::mem::take(&mut self.controller_waiting) {
            self.host.shared().request_callback();
        }
    }
}

impl PluginTailImpl for AudioProcessor<'_> {
    fn get(&self) -> TailLength {
        // VST3 hosts keep processing whatever a plugin says, and many plugins report no tail while
        // having one (ValhallaSupermassive rings for seconds and reports 0): so "none" is taken as
        // "unknown", which CLAP spells infinite. A finite tail is believed, and `kInfiniteTail` is
        // CLAP's infinite already (both `u32::MAX`).
        match self.shared.tail.load(Ordering::Acquire) {
            0 => TailLength::Infinite,
            frames => TailLength::from_raw(frames),
        }
    }
}
