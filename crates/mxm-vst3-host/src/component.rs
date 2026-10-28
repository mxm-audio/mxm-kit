//! One VST3 plugin instance: its component (the processor) and its edit controller, created,
//! initialised and connected as the SDK's host does (`public.sdk/source/vst/hosting/
//! plugprovider.cpp`), its buses, and its state.

use crate::handler::{ComponentHandler, HandlerState};
use crate::host_objects::{HostApplication, MemoryStream, guid};
use crate::module::{ClassInfo, Module};
use crate::strings::from_utf16;
use std::sync::Arc;
use vst3::Steinberg::Vst::BusDirections_::{kInput, kOutput};
use vst3::Steinberg::Vst::BusTypes_::kMain;
use vst3::Steinberg::Vst::MediaTypes_::{kAudio, kEvent};
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::{
    BusDirection, BusInfo, IAudioProcessor, IAudioProcessorTrait, IComponent, IComponentHandler,
    IComponentTrait, IConnectionPoint, IConnectionPointTrait, IEditController,
    IEditControllerTrait, IHostApplication, SpeakerArr, SpeakerArrangement,
};
use vst3::Steinberg::{FUnknown, IPluginBaseTrait, TUID, kResultOk};
use vst3::{ComPtr, ComWrapper};

/// One audio bus, as the host is offered it.
#[derive(Clone, Debug)]
pub(crate) struct Bus {
    pub name: String,
    pub channels: u32,
}

/// The buses of a component. Every audio bus is processed. The host is offered the main input,
/// the main output and every other output bus with channels (an aux output: a drum machine's
/// per-slot outputs) as CLAP ports; the other inputs (sidechains) get silence in.
#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    pub inputs: Vec<Bus>,
    pub outputs: Vec<Bus>,
    pub main_input: Option<usize>,
    pub main_output: Option<usize>,
    /// The other output buses with channels, in bus order, as indexes into `outputs`: offered
    /// after the main one unless `main_only`.
    pub aux_outputs: Vec<usize>,
    /// The "main only" configuration is selected (`audio-ports-config`): the aux outputs are
    /// switched off and not offered. A plugin starts with them on, "all outputs".
    pub main_only: bool,
    /// Whether it takes notes: its first event input bus.
    pub note_input: bool,
}

impl Layout {
    /// Whether the host is offered a choice of configurations, all outputs or the main one
    /// alone: when there are aux outputs beside a main one.
    pub fn has_configurations(&self) -> bool {
        self.main_output.is_some() && !self.aux_outputs.is_empty()
    }

    /// The output buses offered as CLAP ports, in port order: the main one, then the others
    /// unless `main_only`.
    pub fn output_ports_in(&self, main_only: bool) -> impl Iterator<Item = usize> + '_ {
        let aux: &[usize] = if main_only { &[] } else { &self.aux_outputs };
        self.main_output.into_iter().chain(aux.iter().copied())
    }

    /// The output buses offered as CLAP ports now, in port order.
    pub fn output_ports(&self) -> impl Iterator<Item = usize> + '_ {
        self.output_ports_in(self.main_only)
    }
}

/// Switches `layout`'s aux output buses on, or off when it is "main only" (`activateBus`).
///
/// # Safety
///
/// Only while the component is inactive, on the main thread.
unsafe fn switch_aux_outputs(component: &ComPtr<IComponent>, layout: &Layout) {
    let state = u8::from(!layout.main_only);
    for &aux in &layout.aux_outputs {
        // SAFETY: the caller's.
        unsafe { component.activateBus(kAudio as i32, kOutput as i32, aux as i32, state) };
    }
}

/// A created and initialised instance. Dropping it disconnects and terminates both halves.
pub(crate) struct Component {
    pub component: ComPtr<IComponent>,
    pub processor: ComPtr<IAudioProcessor>,
    pub controller: Option<ComPtr<IEditController>>,
    /// Whether the controller is an object of its own, rather than the component itself.
    separate_controller: bool,
    connection: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    handler: ComWrapper<ComponentHandler>,
    host_application: ComWrapper<HostApplication>,
    /// Last, so the module outlives every object it made.
    _module: Arc<Module>,
}

// SAFETY: VST3 calls each method from the thread its specification names (the UI thread, or the
// processing thread for `process` and `setProcessing`), which the plugin keeps to: CLAP's main
// thread and audio thread. The pointers themselves are reference-counted and thread-agnostic.
unsafe impl Send for Component {}
// SAFETY: as above.
unsafe impl Sync for Component {}

impl Component {
    /// Creates class `class` of `module`.
    ///
    /// # Safety
    ///
    /// Runs the plugin's code; call it on the main thread.
    pub(crate) unsafe fn create(
        module: Arc<Module>,
        class: &ClassInfo,
        host_name: &str,
        handler_state: Arc<HandlerState>,
    ) -> Result<Self, String> {
        let host_application = ComWrapper::new(HostApplication::new(host_name));
        let context = host_application
            .as_com_ref::<IHostApplication>()
            .ok_or("no host application")?
            .as_ptr() as *mut FUnknown;
        // SAFETY (this block): the module's factory and the plugin's objects, used as the SDK
        // documents, on the main thread (the caller's).
        unsafe {
            module.set_host_context(context);
            let component = module
                .create::<IComponent>(&class.cid)
                .ok_or("the class made no component")?;
            if component.initialize(context) != kResultOk {
                return Err("the component did not initialise".into());
            }
            let Some(processor) = component.cast::<IAudioProcessor>() else {
                component.terminate();
                return Err("the component processes no audio".into());
            };
            let handler = ComWrapper::new(ComponentHandler::new(handler_state));
            // From here `Drop` terminates whatever was set up.
            let mut this = Self {
                component,
                processor,
                controller: None,
                separate_controller: false,
                connection: None,
                handler,
                host_application,
                _module: module.clone(),
            };
            if this.processor.canProcessSampleSize(kSample32 as i32) != kResultOk {
                return Err("the processor cannot take 32-bit samples".into());
            }

            if let Some(controller) = this.component.cast::<IEditController>() {
                this.controller = Some(controller);
            } else {
                let mut controller_id: TUID = [0; 16];
                if this.component.getControllerClassId(&mut controller_id) == kResultOk
                    && guid(&controller_id) != [0; 16]
                    && let Some(controller) = module.create::<IEditController>(&controller_id)
                    && controller.initialize(context) == kResultOk
                {
                    this.controller = Some(controller);
                    this.separate_controller = true;
                }
            }

            if this.separate_controller
                && let Some(controller) = &this.controller
                && let (Some(from_component), Some(from_controller)) = (
                    this.component.cast::<IConnectionPoint>(),
                    controller.cast::<IConnectionPoint>(),
                )
            {
                from_component.connect(from_controller.as_ptr());
                from_controller.connect(from_component.as_ptr());
                this.connection = Some((from_component, from_controller));
            }

            if let Some(controller) = &this.controller {
                let handler = this
                    .handler
                    .as_com_ref::<IComponentHandler>()
                    .map_or(std::ptr::null_mut(), |handler| handler.as_ptr());
                controller.setComponentHandler(handler);
            }
            if this.separate_controller {
                this.sync_controller();
            }
            Ok(this)
        }
    }

    /// Tells a separate controller the component's current state, as the SDK's host does after
    /// creating them and after loading state.
    unsafe fn sync_controller(&self) {
        let Some(controller) = self
            .controller
            .as_ref()
            .filter(|_| self.separate_controller)
        else {
            return;
        };
        let stream = ComWrapper::new(MemoryStream::new());
        // SAFETY: both are live; the stream outlives both calls.
        unsafe {
            if self.component.getState(MemoryStream::as_ptr(&stream)) == kResultOk {
                stream.rewind();
                controller.setComponentState(MemoryStream::as_ptr(&stream));
            }
        }
    }

    /// Reads the buses, asks for stereo where a main bus is neither mono nor stereo, and turns on
    /// the main audio buses and the first event input, and the aux outputs unless `main_only`.
    /// Aux outputs start inactive in VST3 and produce nothing until turned on; with "all
    /// outputs", as a plugin starts, all of them are, as Bitwig does.
    ///
    /// # Safety
    ///
    /// Only while the component is inactive, on the main thread.
    pub(crate) unsafe fn layout(&self, main_only: bool) -> Layout {
        // SAFETY (this block): the caller's.
        unsafe {
            let mut layout = self.read_layout();
            let odd = |buses: &[Bus], main: Option<usize>| {
                main.is_some_and(|main| !matches!(buses[main].channels, 1 | 2))
            };
            if odd(&layout.inputs, layout.main_input) || odd(&layout.outputs, layout.main_output) {
                let mut inputs = self.arrangements(kInput as BusDirection, layout.inputs.len());
                let mut outputs = self.arrangements(kOutput as BusDirection, layout.outputs.len());
                if let Some(main) = layout.main_input {
                    inputs[main] = SpeakerArr::kStereo;
                }
                if let Some(main) = layout.main_output {
                    outputs[main] = SpeakerArr::kStereo;
                }
                self.processor.setBusArrangements(
                    inputs.as_mut_ptr(),
                    inputs.len() as i32,
                    outputs.as_mut_ptr(),
                    outputs.len() as i32,
                );
                // Whatever it settled on is what it has.
                layout = self.read_layout();
            }
            if let Some(main) = layout.main_input {
                self.component
                    .activateBus(kAudio as i32, kInput as i32, main as i32, 1);
            }
            if let Some(main) = layout.main_output {
                self.component
                    .activateBus(kAudio as i32, kOutput as i32, main as i32, 1);
            }
            layout.main_only = main_only && layout.has_configurations();
            switch_aux_outputs(&self.component, &layout);
            if layout.note_input {
                self.component
                    .activateBus(kEvent as i32, kInput as i32, 0, 1);
            }
            layout
        }
    }

    unsafe fn read_layout(&self) -> Layout {
        // SAFETY: the caller's.
        unsafe {
            let (inputs, main_input) = self.read_buses(kInput as BusDirection);
            let (outputs, main_output) = self.read_buses(kOutput as BusDirection);
            // A bus without channels has nothing to offer; a second bus marked main is offered as
            // an extra output like an aux one.
            let aux_outputs = (0..outputs.len())
                .filter(|&index| Some(index) != main_output && outputs[index].channels > 0)
                .collect();
            Layout {
                inputs,
                outputs,
                main_input,
                main_output,
                aux_outputs,
                main_only: false,
                note_input: self.component.getBusCount(kEvent as i32, kInput as i32) > 0,
            }
        }
    }

    /// Switches the aux outputs on, or off for "main only", as `layout.main_only` says.
    ///
    /// # Safety
    ///
    /// Only while the component is inactive, on the main thread.
    pub(crate) unsafe fn switch_aux_outputs(&self, layout: &Layout) {
        // SAFETY: the caller's.
        unsafe { switch_aux_outputs(&self.component, layout) }
    }

    unsafe fn read_buses(&self, direction: BusDirection) -> (Vec<Bus>, Option<usize>) {
        let mut buses = Vec::new();
        let mut main = None;
        // SAFETY: the caller's; zeroed out-parameters are valid.
        unsafe {
            for index in 0..self.component.getBusCount(kAudio as i32, direction) {
                let mut info: BusInfo = std::mem::zeroed();
                if self
                    .component
                    .getBusInfo(kAudio as i32, direction, index, &mut info)
                    != kResultOk
                {
                    info.channelCount = 0;
                }
                let mut arrangement: SpeakerArrangement = 0;
                let channels =
                    if self
                        .processor
                        .getBusArrangement(direction, index, &mut arrangement)
                        == kResultOk
                    {
                        arrangement.count_ones()
                    } else {
                        info.channelCount.max(0) as u32
                    };
                if main.is_none() && info.busType == kMain as i32 {
                    main = Some(buses.len());
                }
                buses.push(Bus {
                    name: from_utf16(&info.name),
                    channels,
                });
            }
        }
        (buses, main)
    }

    unsafe fn arrangements(
        &self,
        direction: BusDirection,
        count: usize,
    ) -> Vec<SpeakerArrangement> {
        (0..count)
            .map(|index| {
                let mut arrangement: SpeakerArrangement = 0;
                // SAFETY: the caller's.
                unsafe {
                    self.processor
                        .getBusArrangement(direction, index as i32, &mut arrangement);
                }
                arrangement
            })
            .collect()
    }

    /// The component's state and the controller's, in one blob: `MXV3`, a version, then each as a
    /// length and its bytes.
    ///
    /// # Safety
    ///
    /// On the main thread.
    pub(crate) unsafe fn save_state(&self) -> Result<Vec<u8>, String> {
        let component_state = ComWrapper::new(MemoryStream::new());
        let controller_state = ComWrapper::new(MemoryStream::new());
        // SAFETY: the caller's; the streams outlive the calls.
        unsafe {
            if self
                .component
                .getState(MemoryStream::as_ptr(&component_state))
                != kResultOk
            {
                return Err("the component gave no state".into());
            }
            if let Some(controller) = &self.controller {
                // A controller without state of its own may refuse; that is not an error.
                controller.getState(MemoryStream::as_ptr(&controller_state));
            }
        }
        let component_state = component_state.bytes();
        let controller_state = controller_state.bytes();
        let mut blob = Vec::with_capacity(24 + component_state.len() + controller_state.len());
        blob.extend_from_slice(STATE_MAGIC);
        blob.extend_from_slice(&STATE_VERSION.to_le_bytes());
        for part in [&component_state, &controller_state] {
            blob.extend_from_slice(&(part.len() as u64).to_le_bytes());
            blob.extend_from_slice(part);
        }
        Ok(blob)
    }

    /// Loads a blob [`save_state`](Self::save_state) made.
    ///
    /// # Safety
    ///
    /// On the main thread.
    pub(crate) unsafe fn load_state(&self, blob: &[u8]) -> Result<(), String> {
        let (component_state, controller_state) = split_state(blob)?;
        let component_stream = ComWrapper::new(MemoryStream::from_bytes(component_state.to_vec()));
        // SAFETY: the caller's; the streams outlive the calls.
        unsafe {
            if self
                .component
                .setState(MemoryStream::as_ptr(&component_stream))
                != kResultOk
            {
                return Err("the component refused its state".into());
            }
            if let Some(controller) = &self.controller {
                if self.separate_controller {
                    component_stream.rewind();
                    controller.setComponentState(MemoryStream::as_ptr(&component_stream));
                }
                if !controller_state.is_empty() {
                    let controller_stream =
                        ComWrapper::new(MemoryStream::from_bytes(controller_state.to_vec()));
                    controller.setState(MemoryStream::as_ptr(&controller_stream));
                }
            }
        }
        Ok(())
    }
}

const STATE_MAGIC: &[u8; 4] = b"MXV3";
const STATE_VERSION: u32 = 1;

/// The component's and the controller's parts of a state blob.
fn split_state(blob: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let bad = || "not a VST3 state saved by this host".to_string();
    let rest = blob.strip_prefix(STATE_MAGIC.as_slice()).ok_or_else(bad)?;
    let (version, mut rest) = rest.split_at_checked(4).ok_or_else(bad)?;
    if u32::from_le_bytes(version.try_into().map_err(|_| bad())?) != STATE_VERSION {
        return Err("a VST3 state of a newer version".into());
    }
    let mut parts = [&[][..]; 2];
    for part in &mut parts {
        let (length, after) = rest.split_at_checked(8).ok_or_else(bad)?;
        let length = u64::from_le_bytes(length.try_into().map_err(|_| bad())?);
        let length = usize::try_from(length).map_err(|_| bad())?;
        let (bytes, after) = after.split_at_checked(length).ok_or_else(bad)?;
        *part = bytes;
        rest = after;
    }
    Ok((parts[0], parts[1]))
}

impl Drop for Component {
    fn drop(&mut self) {
        // SAFETY: the reverse of `create`, as the SDK's host tears down, on the main thread where
        // CLAP destroys a plugin.
        unsafe {
            if let Some((from_component, from_controller)) = self.connection.take() {
                from_component.disconnect(from_controller.as_ptr());
                from_controller.disconnect(from_component.as_ptr());
            }
            if let Some(controller) = &self.controller {
                controller.setComponentHandler(std::ptr::null_mut());
                if self.separate_controller {
                    controller.terminate();
                }
            }
            self.component.terminate();
        }
        let _ = &self.host_application;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use vst3::Class;
    use vst3::Steinberg::Vst::{IoMode, MediaType, RoutingInfo};
    use vst3::Steinberg::{IBStream, TBool, int32, tresult};

    /// A component that only records the buses it is told to switch.
    #[derive(Default)]
    struct Switches {
        calls: Mutex<Vec<(MediaType, BusDirection, int32, TBool)>>,
    }

    impl Class for Switches {
        type Interfaces = (IComponent,);
    }

    impl IPluginBaseTrait for Switches {
        unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
            kResultOk
        }
        unsafe fn terminate(&self) -> tresult {
            kResultOk
        }
    }

    impl IComponentTrait for Switches {
        unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
            kResultOk
        }
        unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
            kResultOk
        }
        unsafe fn getBusCount(&self, _type: MediaType, _dir: BusDirection) -> int32 {
            0
        }
        unsafe fn getBusInfo(
            &self,
            _type: MediaType,
            _dir: BusDirection,
            _index: int32,
            _bus: *mut BusInfo,
        ) -> tresult {
            kResultOk
        }
        unsafe fn getRoutingInfo(&self, _in: *mut RoutingInfo, _out: *mut RoutingInfo) -> tresult {
            kResultOk
        }
        unsafe fn activateBus(
            &self,
            media: MediaType,
            dir: BusDirection,
            index: int32,
            state: TBool,
        ) -> tresult {
            self.calls.lock().unwrap().push((media, dir, index, state));
            kResultOk
        }
        unsafe fn setActive(&self, _state: TBool) -> tresult {
            kResultOk
        }
        unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
            kResultOk
        }
        unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
            kResultOk
        }
    }

    #[test]
    fn main_only_switches_the_aux_outputs_off_and_all_outputs_on_again() {
        let switches = ComWrapper::new(Switches::default());
        let component = switches.to_com_ptr::<IComponent>().unwrap();
        // A drum machine's buses: the main one, then its slots.
        let mut layout = Layout {
            main_output: Some(0),
            aux_outputs: vec![1, 2, 3],
            ..Layout::default()
        };
        let audio_out = |index, state| (kAudio as MediaType, kOutput as BusDirection, index, state);
        for (main_only, state) in [(true, 0), (false, 1)] {
            layout.main_only = main_only;
            // SAFETY: the component is a fake that records the calls.
            unsafe { switch_aux_outputs(&component, &layout) };
            let calls = std::mem::take(&mut *switches.calls.lock().unwrap());
            assert_eq!(
                calls,
                [1, 2, 3].map(|index| audio_out(index, state)),
                "main only {main_only}: every aux output, and never the main one"
            );
        }
    }

    #[test]
    fn a_state_blob_splits_into_what_was_saved() {
        let mut blob = Vec::new();
        blob.extend_from_slice(STATE_MAGIC);
        blob.extend_from_slice(&STATE_VERSION.to_le_bytes());
        blob.extend_from_slice(&3u64.to_le_bytes());
        blob.extend_from_slice(b"abc");
        blob.extend_from_slice(&0u64.to_le_bytes());
        assert_eq!(split_state(&blob).unwrap(), (&b"abc"[..], &b""[..]));
        assert!(split_state(b"MXV3").is_err());
        assert!(split_state(&blob[..blob.len() - 1]).is_err());
    }
}
