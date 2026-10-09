//! Parameters: what the edit controller says about them, how a value crosses between CLAP's range
//! and VST3's normalised one, and the per-block change lists the processor reads and writes.
//!
//! **Values.** A continuous VST3 parameter is offered to CLAP as its normalised value, 0 to 1, and
//! a stepped one (`stepCount > 0`) as its step, 0 to `stepCount`, so a host shows and automates
//! the steps the plugin has. The step of a normalised value is the SDK's
//! `ParameterInfo` rule (`min(stepCount, normalised * (stepCount + 1))`), and back is
//! `step / stepCount`.

use crate::strings::from_utf16;
use clack_extensions::params::ParamInfoFlags;
use std::cell::{Cell, UnsafeCell};
use std::collections::HashMap;
use vst3::Steinberg::Vst::ParameterInfo_::ParameterFlags_::{
    kCanAutomate, kIsBypass, kIsHidden, kIsList, kIsReadOnly, kIsWrapAround,
};
use vst3::Steinberg::Vst::{
    IEditController, IEditControllerTrait, IParamValueQueue, IParamValueQueueTrait,
    IParameterChanges, IParameterChangesTrait, IUnitInfo, IUnitInfoTrait, ParamID, ParamValue,
    ParameterInfo, UnitInfo, kNoParentUnitId, kRootUnitId,
};
use vst3::Steinberg::{int32, kInvalidArgument, kResultFalse, kResultOk, tresult};
use vst3::{Class, ComPtr, ComWrapper};

/// The id CLAP reserves as "no parameter"; a VST3 parameter that has it is not offered.
const CLAP_INVALID_ID: u32 = u32::MAX;

/// One parameter, as the host is told about it.
#[derive(Clone)]
pub(crate) struct ParamInfo {
    pub id: u32,
    pub name: String,
    pub units: String,
    /// The `/`-separated path of the unit it belongs to, for CLAP's `module`.
    pub module: String,
    /// The number of steps, or 0 for a continuous parameter.
    pub steps: u32,
    pub default_normalized: f64,
    pub flags: ParamInfoFlags,
}

impl ParamInfo {
    /// The CLAP value of a normalised one.
    pub(crate) fn to_clap(&self, normalized: f64) -> f64 {
        to_clap(self.steps, normalized)
    }

    /// The normalised value of a CLAP one.
    pub(crate) fn to_normalized(&self, value: f64) -> f64 {
        to_normalized(self.steps, value)
    }
}

pub(crate) fn to_clap(steps: u32, normalized: f64) -> f64 {
    let normalized = normalized.clamp(0.0, 1.0);
    if steps == 0 {
        normalized
    } else {
        (normalized * f64::from(steps + 1))
            .floor()
            .min(f64::from(steps))
    }
}

pub(crate) fn to_normalized(steps: u32, value: f64) -> f64 {
    if steps == 0 {
        value.clamp(0.0, 1.0)
    } else {
        value.round().clamp(0.0, f64::from(steps)) / f64::from(steps)
    }
}

/// Every parameter the controller has, in its order.
///
/// # Safety
///
/// `controller` must be initialised; this runs on the main thread.
pub(crate) unsafe fn read_params(controller: &ComPtr<IEditController>) -> Vec<ParamInfo> {
    // SAFETY: the caller's.
    let units = unsafe { unit_paths(controller) };
    let mut params = Vec::new();
    // SAFETY: the caller's.
    let count = unsafe { controller.getParameterCount() };
    for index in 0..count {
        // SAFETY: a zeroed `ParameterInfo` is a valid out-parameter; `index` is below the count.
        let mut info: ParameterInfo = unsafe { std::mem::zeroed() };
        if unsafe { controller.getParameterInfo(index, &mut info) } != kResultOk
            || info.id == CLAP_INVALID_ID
        {
            continue;
        }
        let name = from_utf16(&info.title);
        let vst_flags = info.flags;
        let has = |flag: i32| vst_flags & flag != 0;
        let steps = info.stepCount.max(0) as u32;

        let mut flags = ParamInfoFlags::empty();
        if has(kCanAutomate as i32) && !has(kIsReadOnly as i32) {
            // Modulation is added to the value before it reaches the processor (`plugin.rs`).
            flags |= ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_MODULATABLE;
        }
        if has(kIsReadOnly as i32) {
            flags |= ParamInfoFlags::IS_READONLY;
        }
        if has(kIsWrapAround as i32) {
            flags |= ParamInfoFlags::IS_PERIODIC;
        }
        if has(kIsHidden as i32) {
            flags |= ParamInfoFlags::IS_HIDDEN;
        }
        if has(kIsBypass as i32) {
            flags |= ParamInfoFlags::IS_BYPASS;
        }
        if steps > 0 {
            flags |= ParamInfoFlags::IS_STEPPED;
            if has(kIsList as i32) {
                flags |= ParamInfoFlags::IS_ENUM;
            }
        }
        // JUCE's MIDI controller stand-ins: up to 2,080 parameters named "MIDI CC <n>|<m>", none
        // automatable. They stay reachable, but out of sight.
        if !has(kCanAutomate as i32) && name.starts_with("MIDI CC ") {
            flags |= ParamInfoFlags::IS_HIDDEN;
        }

        params.push(ParamInfo {
            id: info.id,
            name,
            units: from_utf16(&info.units),
            module: units.get(&info.unitId).cloned().unwrap_or_default(),
            steps,
            default_normalized: info.defaultNormalizedValue,
            flags,
        });
    }
    params
}

/// Each unit's path of names below the root, from `IUnitInfo` when the controller has it.
unsafe fn unit_paths(controller: &ComPtr<IEditController>) -> HashMap<i32, String> {
    let Some(units) = controller.cast::<IUnitInfo>() else {
        return HashMap::new();
    };
    let mut tree: HashMap<i32, (i32, String)> = HashMap::new();
    // SAFETY: `units` is the controller's; a zeroed `UnitInfo` is a valid out-parameter.
    for index in 0..unsafe { units.getUnitCount() } {
        let mut info: UnitInfo = unsafe { std::mem::zeroed() };
        if unsafe { units.getUnitInfo(index, &mut info) } == kResultOk {
            tree.insert(info.id, (info.parentUnitId, from_utf16(&info.name)));
        }
    }
    let mut paths = HashMap::new();
    for &unit in tree.keys() {
        let mut names = Vec::new();
        let mut at = unit;
        // A tree is at most as deep as it has units; a malformed one with a loop stops there.
        for _ in 0..tree.len() {
            if at == kRootUnitId || at == kNoParentUnitId {
                break;
            }
            let Some((parent, name)) = tree.get(&at) else {
                break;
            };
            names.push(name.clone());
            at = *parent;
        }
        names.reverse();
        paths.insert(unit, names.join("/"));
    }
    paths
}

/// `IParamValueQueue`: one parameter's changes within a block, in time order.
///
/// Fixed in size when made, so the audio thread never allocates: a point at an offset already
/// held replaces it, and a full queue makes room by dropping its last point (a later point still
/// takes the last place, so the block always ends on the newest value).
pub(crate) struct ParamValueQueue {
    id: Cell<ParamID>,
    points: UnsafeCell<Box<[(int32, ParamValue)]>>,
    len: Cell<usize>,
}

// SAFETY: a queue is only ever touched by the audio thread that owns the processor using it, and
// by the plugin within the `process` call that thread makes. It moves between threads only with
// the processor, whole.
unsafe impl Send for ParamValueQueue {}
// SAFETY: as above: never used from two threads at once.
unsafe impl Sync for ParamValueQueue {}

impl Class for ParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl ParamValueQueue {
    fn new(capacity: usize) -> Self {
        Self {
            id: Cell::new(0),
            points: UnsafeCell::new(vec![(0, 0.0); capacity.max(1)].into_boxed_slice()),
            len: Cell::new(0),
        }
    }

    fn reset(&self, id: ParamID) {
        self.id.set(id);
        self.len.set(0);
    }

    pub(crate) fn id(&self) -> ParamID {
        self.id.get()
    }

    /// The points, in time order.
    pub(crate) fn points(&self) -> &[(int32, ParamValue)] {
        // SAFETY: only this thread touches the points (see the `Send` impl), and no `&mut` to them
        // outlives `add`.
        unsafe { &(&*self.points.get())[..self.len.get()] }
    }

    /// Adds a point, keeping time order, and returns where it went.
    fn add(&self, offset: int32, value: ParamValue) -> usize {
        // SAFETY: as in `points`.
        let points = unsafe { &mut *self.points.get() };
        let len = self.len.get();
        let at = points[..len]
            .iter()
            .position(|&(held, _)| held >= offset)
            .unwrap_or(len);
        if at < len && points[at].0 == offset {
            points[at].1 = value;
            return at;
        }
        if len == points.len() {
            if at == len {
                points[len - 1] = (offset, value);
                return len - 1;
            }
            points.copy_within(at..len - 1, at + 1);
        } else {
            points.copy_within(at..len, at + 1);
            self.len.set(len + 1);
        }
        points[at] = (offset, value);
        at
    }
}

impl IParamValueQueueTrait for ParamValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.len.get() as int32
    }

    unsafe fn getPoint(
        &self,
        index: int32,
        sample_offset: *mut int32,
        value: *mut ParamValue,
    ) -> tresult {
        if sample_offset.is_null() || value.is_null() {
            return kInvalidArgument;
        }
        let Some(&(offset, held)) = usize::try_from(index)
            .ok()
            .and_then(|index| self.points().get(index))
        else {
            return kResultFalse;
        };
        // SAFETY: both checked non-null.
        unsafe {
            *sample_offset = offset;
            *value = held;
        }
        kResultOk
    }

    unsafe fn addPoint(
        &self,
        sample_offset: int32,
        value: ParamValue,
        index: *mut int32,
    ) -> tresult {
        let at = self.add(sample_offset, value);
        if !index.is_null() {
            // SAFETY: checked non-null.
            unsafe { *index = at as int32 };
        }
        kResultOk
    }
}

/// `IParameterChanges`: the queues of one block, at most one per parameter.
pub(crate) struct ParameterChanges {
    queues: Box<[ComWrapper<ParamValueQueue>]>,
    pointers: Box<[*mut IParamValueQueue]>,
    used: Cell<usize>,
}

// SAFETY: as for `ParamValueQueue`; the pointers point into `queues`, which this owns.
unsafe impl Send for ParameterChanges {}
// SAFETY: as above.
unsafe impl Sync for ParameterChanges {}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl ParameterChanges {
    /// Room for `parameters` parameters with `points` points each in one block.
    pub(crate) fn new(parameters: usize, points: usize) -> Self {
        let queues: Box<[_]> = (0..parameters.max(1))
            .map(|_| ComWrapper::new(ParamValueQueue::new(points)))
            .collect();
        let pointers = queues
            .iter()
            .map(|queue| {
                queue
                    .as_com_ref::<IParamValueQueue>()
                    .map(|queue| queue.as_ptr())
                    .unwrap_or(std::ptr::null_mut())
            })
            .collect();
        Self {
            queues,
            pointers,
            used: Cell::new(0),
        }
    }

    pub(crate) fn clear(&self) {
        self.used.set(0);
    }

    /// The queues holding changes, in the order they were added.
    pub(crate) fn queues(&self) -> impl Iterator<Item = &ParamValueQueue> {
        self.queues[..self.used.get()].iter().map(|queue| &**queue)
    }

    /// The queue for `id`, added if it has none yet; `None` once every queue is taken.
    fn queue_for(&self, id: ParamID) -> Option<usize> {
        let used = self.used.get();
        if let Some(index) = self.queues[..used]
            .iter()
            .position(|queue| queue.id() == id)
        {
            return Some(index);
        }
        if used == self.queues.len() {
            return None;
        }
        self.queues[used].reset(id);
        self.used.set(used + 1);
        Some(used)
    }

    /// Adds a change of `id` to `value` (normalised) at `offset`; false when the list is full.
    pub(crate) fn add(&self, id: ParamID, offset: int32, value: ParamValue) -> bool {
        match self.queue_for(id) {
            Some(index) => {
                self.queues[index].add(offset, value);
                true
            }
            None => false,
        }
    }

    pub(crate) fn as_ptr(changes: &ComWrapper<ParameterChanges>) -> *mut IParameterChanges {
        changes
            .as_com_ref::<IParameterChanges>()
            .map(|changes| changes.as_ptr())
            .unwrap_or(std::ptr::null_mut())
    }
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.used.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        // No reference is added, as in the SDK: the queue lives as long as the list.
        usize::try_from(index)
            .ok()
            .filter(|&index| index < self.used.get())
            .map_or(std::ptr::null_mut(), |index| self.pointers[index])
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut int32,
    ) -> *mut IParamValueQueue {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: checked non-null.
        let Some(at) = self.queue_for(unsafe { *id }) else {
            return std::ptr::null_mut();
        };
        if !index.is_null() {
            // SAFETY: checked non-null.
            unsafe { *index = at as int32 };
        }
        self.pointers[at]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_round_trip_the_sdk_way() {
        // A three-step parameter has four values: 0, 1/3, 2/3 and 1.
        for step in 0..=3 {
            let normalized = to_normalized(3, f64::from(step));
            assert_eq!(to_clap(3, normalized), f64::from(step));
        }
        // The SDK's rule: normalised 0.5 of three steps is step 2 (0.5 * 4 = 2).
        assert_eq!(to_clap(3, 0.5), 2.0);
        assert_eq!(to_clap(3, 1.0), 3.0);
        assert_eq!(to_clap(0, 0.25), 0.25);
    }

    #[test]
    fn a_queue_keeps_time_order_and_its_newest_point_when_full() {
        let queue = ParamValueQueue::new(3);
        queue.reset(7);
        queue.add(10, 0.1);
        queue.add(0, 0.0);
        queue.add(5, 0.05);
        assert_eq!(queue.points(), &[(0, 0.0), (5, 0.05), (10, 0.1)]);
        queue.add(5, 0.5);
        assert_eq!(queue.points()[1], (5, 0.5));
        // Full: a later point takes the last place.
        queue.add(20, 0.2);
        assert_eq!(queue.points(), &[(0, 0.0), (5, 0.5), (20, 0.2)]);
        // Full: an earlier point drops the last.
        queue.add(1, 0.01);
        assert_eq!(queue.points(), &[(0, 0.0), (1, 0.01), (5, 0.5)]);
    }

    #[test]
    fn a_change_list_has_one_queue_per_parameter_and_refuses_when_full() {
        let changes = ParameterChanges::new(2, 4);
        assert!(changes.add(1, 0, 0.5));
        assert!(changes.add(1, 8, 0.6));
        assert!(changes.add(2, 0, 0.1));
        assert!(!changes.add(3, 0, 0.9));
        assert_eq!(changes.queues().count(), 2);
        changes.clear();
        assert_eq!(changes.queues().count(), 0);
    }
}
