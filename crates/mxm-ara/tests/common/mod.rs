//! A test controller: it reads each source on a worker thread when access is enabled, archives
//! its "analysis" and its modifications' "edits", reads the song's key, and logs every hook to a
//! thread-local log (the model thread is the test's own thread).

#![allow(dead_code)]

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use mxm_ara::test_host::{ContextContent, HostConfig, SourceSpec, TestHost};
use mxm_ara::*;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SETTINGS: RefCell<Settings> = RefCell::new(Settings::default());
}

/// How the test controller behaves in this test.
#[derive(Clone, Debug)]
pub struct Settings {
    /// Frames per read on the worker.
    pub chunk: usize,
    /// Read each source when its access is enabled.
    pub read_on_enable: bool,
    /// The schema its stores write (a newer plug-in writes a higher one).
    pub store_schema: u32,
    /// Read 64-bit samples.
    pub sixty_four: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            chunk: 1000,
            read_on_enable: true,
            store_schema: 1,
            sixty_four: false,
        }
    }
}

pub fn set(settings: Settings) {
    SETTINGS.with(|s| *s.borrow_mut() = settings);
}

fn settings() -> Settings {
    SETTINGS.with(|s| s.borrow().clone())
}

pub fn log(line: impl Into<String>) {
    LOG.with(|l| l.borrow_mut().push(line.into()));
}

/// The log so far (taken).
pub fn take_log() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

#[derive(Default)]
pub struct SourceData {
    /// What the worker read, once it finished.
    pub samples: Arc<Mutex<Option<Vec<Vec<f32>>>>>,
    pub worker: Mutex<Option<JoinHandle<()>>>,
    /// The archived "analysis".
    pub analysis: Mutex<Vec<u8>>,
}

#[derive(Default)]
pub struct ModificationData {
    pub edits: Mutex<Vec<u8>>,
}

#[derive(Default)]
pub struct RegionData {
    pub renders: AtomicUsize,
}

pub struct TestController {
    pub notifier: Notifier,
    pub document_data: Vec<u8>,
    pub key: Option<KeySignature>,
}

impl Drop for TestController {
    fn drop(&mut self) {
        log("controller dropped");
    }
}

pub const CONFIG: FactoryConfig = FactoryConfig {
    plug_in_name: "MXM Test ARA",
    manufacturer_name: "MXM",
    information_url: "https://mxm.dk",
    version: "0.1.0",
    factory_id: "dk.mxm.test-ara.factory.1",
    document_archive_id: "dk.mxm.test-ara.archive.1",
    compatible_document_archive_ids: &["dk.mxm.test-ara.archive.0"],
    analyzeable_content_types: &[ContentType::Notes],
    supported_playback_transformation_flags: 0,
    supports_storing_audio_file_chunks: false,
};

/// Starts reading a source on a worker thread.
fn start_reading(cx: &mut Cx<'_, TestController>, source: AudioSourceId) {
    if settings().sixty_four {
        start_reading_as::<f64>(cx, source, |s| s as f32);
    } else {
        start_reading_as::<f32>(cx, source, |s| s);
    }
}

fn start_reading_as<S: Sample>(
    cx: &mut Cx<'_, TestController>,
    source: AudioSourceId,
    to_f32: fn(S) -> f32,
) {
    let Some(mut reader) = cx.create_audio_reader::<S>(source) else {
        log("no reader");
        return;
    };
    let Some(node) = cx.graph().audio_source(source) else {
        return;
    };
    let frames = node.properties().sample_count as usize;
    let out = Arc::clone(&node.data.samples);
    let notifier = cx.notifier().clone();
    let chunk = settings().chunk;
    let worker = std::thread::spawn(move || {
        let channels = reader.channel_count();
        let mut whole = vec![Vec::with_capacity(frames); channels];
        let mut buffers = vec![vec![S::default(); chunk]; channels];
        notifier.analysis_started(source);
        let mut position = 0;
        while position < frames {
            let n = chunk.min(frames - position);
            let mut slices: Vec<&mut [S]> = buffers.iter_mut().map(|b| &mut b[..n]).collect();
            match reader.read(position as i64, &mut slices) {
                Ok(()) => {}
                Err(ReadError::Cancelled) => {
                    notifier.analysis_completed(source);
                    return;
                }
                Err(e) => panic!("read failed: {e:?}"),
            }
            for (w, b) in whole.iter_mut().zip(&buffers) {
                w.extend(b[..n].iter().map(|s| to_f32(*s)));
            }
            position += n;
            notifier.analysis_progress(source, position as f32 / frames as f32);
        }
        notifier.analysis_completed(source);
        notifier.audio_source_content_changed(source, None, ContentUpdateFlags::SIGNAL_UNCHANGED);
        *out.lock().unwrap() = Some(whole);
    });
    *node.data.worker.lock().unwrap() = Some(worker);
}

fn join_worker(cx: &Cx<'_, TestController>, source: AudioSourceId) {
    let worker = cx
        .graph()
        .audio_source(source)
        .and_then(|s| s.data.worker.lock().unwrap().take());
    if let Some(w) = worker {
        w.join().expect("worker");
    }
}

impl Controller for TestController {
    type AudioSource = SourceData;
    type AudioModification = ModificationData;
    type PlaybackRegion = RegionData;

    const FACTORY: FactoryConfig = CONFIG;

    fn new(init: Init<'_>) -> Self {
        log(format!(
            "new {:?} generation {}",
            init.document.name, init.api_generation
        ));
        TestController {
            notifier: init.notifier,
            document_data: Vec::new(),
            key: None,
        }
    }

    fn will_begin_editing(&mut self, _cx: &mut Cx<'_, Self>) {
        log("begin editing");
    }
    fn will_end_editing(&mut self, cx: &mut Cx<'_, Self>) {
        log(format!(
            "will end editing, graph exclusive {}",
            cx.graph_mut().is_some()
        ));
    }
    fn did_end_editing(&mut self, cx: &mut Cx<'_, Self>) {
        log(format!(
            "did end editing, graph exclusive {}",
            cx.graph_mut().is_some()
        ));
    }
    fn did_update_document_properties(&mut self, cx: &mut Cx<'_, Self>) {
        log(format!("document {:?}", cx.graph().document().name));
    }

    fn did_create_musical_context(&mut self, cx: &mut Cx<'_, Self>, context: MusicalContextId) {
        let keys = cx.read_musical_context_content::<KeySignature>(context, None);
        let tempo = cx.read_musical_context_content::<TempoEntry>(context, None);
        log(format!(
            "context keys {:?} tempo {:?}",
            keys.as_ref().map(|k| k.events.len()),
            tempo.as_ref().map(|t| t.events.len())
        ));
        self.key = keys.and_then(|k| k.events.into_iter().next());
    }
    fn did_update_musical_context_content(
        &mut self,
        cx: &mut Cx<'_, Self>,
        context: MusicalContextId,
        _range: Option<TimeRange>,
        _flags: ContentUpdateFlags,
    ) {
        self.key = cx
            .read_musical_context_content::<KeySignature>(context, None)
            .and_then(|k| k.events.into_iter().next());
        log(format!(
            "context content updated, key root {:?}",
            self.key.as_ref().map(|k| k.root)
        ));
    }
    fn will_destroy_musical_context(&mut self, _cx: &mut Cx<'_, Self>, _context: MusicalContextId) {
        log("destroy context");
    }
    fn did_create_region_sequence(&mut self, cx: &mut Cx<'_, Self>, sequence: RegionSequenceId) {
        log(format!(
            "sequence {:?}",
            cx.graph()
                .region_sequence(sequence)
                .map(|s| s.properties().name.clone())
        ));
    }
    fn will_destroy_region_sequence(
        &mut self,
        _cx: &mut Cx<'_, Self>,
        _sequence: RegionSequenceId,
    ) {
        log("destroy sequence");
    }

    fn did_create_audio_source(&mut self, cx: &mut Cx<'_, Self>, source: AudioSourceId) {
        // Not allowed here: a musical context's content (the scope is this source).
        let context = cx.graph().musical_contexts().next().map(|c| c.id());
        let key_here = context
            .and_then(|c| cx.read_musical_context_content::<KeySignature>(c, None))
            .is_some();
        log(format!(
            "source {}, key readable here {key_here}",
            cx.graph().audio_source(source).unwrap().persistent_id()
        ));
    }
    fn did_update_audio_source_properties(&mut self, cx: &mut Cx<'_, Self>, source: AudioSourceId) {
        log(format!(
            "source properties {}",
            cx.graph()
                .audio_source(source)
                .unwrap()
                .properties()
                .sample_rate
        ));
    }
    fn will_enable_audio_source_samples_access(
        &mut self,
        _cx: &mut Cx<'_, Self>,
        _source: AudioSourceId,
        enable: bool,
    ) {
        log(format!("will enable {enable}"));
    }
    fn did_enable_audio_source_samples_access(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        enable: bool,
    ) {
        log(format!("did enable {enable}"));
        if enable && settings().read_on_enable {
            start_reading(cx, source);
        } else if !enable {
            // mxm-ara cancelled the readers before this; the worker ends on its next read.
            join_worker(cx, source);
        }
    }
    fn will_destroy_audio_source(&mut self, cx: &mut Cx<'_, Self>, source: AudioSourceId) {
        log("destroy source");
        let _ = cx.cancel_audio_readers(source);
        join_worker(cx, source);
    }

    fn did_create_audio_modification(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
        cloned_from: Option<AudioModificationId>,
    ) {
        let g = cx.graph_mut().expect("created inside the edit");
        if let Some(original) = cloned_from {
            let edits = g
                .audio_modification(original)
                .unwrap()
                .data
                .edits
                .lock()
                .unwrap()
                .clone();
            *g.audio_modification(modification)
                .unwrap()
                .data
                .edits
                .lock()
                .unwrap() = edits;
        }
        log(format!(
            "modification {} cloned {}",
            g.audio_modification(modification).unwrap().persistent_id(),
            cloned_from.is_some()
        ));
    }
    fn will_destroy_audio_modification(
        &mut self,
        _cx: &mut Cx<'_, Self>,
        _modification: AudioModificationId,
    ) {
        log("destroy modification");
    }

    fn did_create_playback_region(&mut self, cx: &mut Cx<'_, Self>, region: PlaybackRegionId) {
        let r = cx.graph().playback_region(region).unwrap();
        log(format!(
            "region {:?} at {}",
            r.properties().name,
            r.properties().start_in_playback_time
        ));
    }
    fn did_update_playback_region_properties(
        &mut self,
        cx: &mut Cx<'_, Self>,
        region: PlaybackRegionId,
    ) {
        let r = cx.graph().playback_region(region).unwrap();
        log(format!(
            "region moved to {}",
            r.properties().start_in_playback_time
        ));
    }
    fn will_destroy_playback_region(&mut self, _cx: &mut Cx<'_, Self>, _region: PlaybackRegionId) {
        log("destroy region");
    }
    fn playback_region_head_and_tail_time(
        &self,
        graph: &Graph<Self>,
        region: PlaybackRegionId,
    ) -> (f64, f64) {
        let named_tail = graph
            .playback_region(region)
            .is_some_and(|r| r.properties().name.as_deref() == Some("with tail"));
        (0.0, if named_tail { 0.5 } else { 0.0 })
    }

    fn did_bind(&mut self, _cx: &mut Cx<'_, Self>, _extension: ExtensionId, roles: Roles) {
        log(format!("bind {}", roles.0));
    }
    fn will_unbind(&mut self, _cx: &mut Cx<'_, Self>, _extension: ExtensionId) {
        log("unbind");
    }
    fn did_add_playback_region_to_renderer(
        &mut self,
        _cx: &mut Cx<'_, Self>,
        _e: ExtensionId,
        _region: PlaybackRegionId,
    ) {
        log("renderer +region");
    }
    fn did_remove_playback_region_from_renderer(
        &mut self,
        _cx: &mut Cx<'_, Self>,
        _e: ExtensionId,
        _region: PlaybackRegionId,
    ) {
        log("renderer -region");
    }
    fn did_change_editor_renderer(&mut self, cx: &mut Cx<'_, Self>, e: ExtensionId) {
        let seen = cx
            .editor_renderer(e)
            .map(|a| (a.playback_regions.len(), a.region_sequences.len()));
        log(format!("editor renderer {seen:?}"));
    }
    fn did_notify_selection(
        &mut self,
        _cx: &mut Cx<'_, Self>,
        _e: ExtensionId,
        selection: &ViewSelection,
    ) {
        log(format!(
            "selection {} regions",
            selection.playback_regions.len()
        ));
    }

    fn store_document_data(&mut self, _cx: &mut Cx<'_, Self>) -> Option<Payload> {
        (!self.document_data.is_empty()).then(|| Payload {
            schema: settings().store_schema,
            bytes: self.document_data.clone(),
        })
    }
    fn store_audio_source(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
    ) -> Option<Payload> {
        let bytes = cx
            .graph()
            .audio_source(source)?
            .data
            .analysis
            .lock()
            .unwrap()
            .clone();
        Some(Payload {
            schema: settings().store_schema,
            bytes,
        })
    }
    fn store_audio_modification(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
    ) -> Option<Payload> {
        let bytes = cx
            .graph()
            .audio_modification(modification)?
            .data
            .edits
            .lock()
            .unwrap()
            .clone();
        Some(Payload {
            schema: settings().store_schema,
            bytes,
        })
    }
    fn restore_document_data(&mut self, _cx: &mut Cx<'_, Self>, payload: &Payload) -> bool {
        log(format!(
            "restore document {:?}",
            String::from_utf8_lossy(&payload.bytes)
        ));
        self.document_data = payload.bytes.clone();
        true
    }
    fn restore_audio_source(
        &mut self,
        cx: &mut Cx<'_, Self>,
        source: AudioSourceId,
        payload: &Payload,
    ) -> bool {
        let g = cx.graph_mut().expect("restored inside the edit");
        *g.audio_source(source)
            .unwrap()
            .data
            .analysis
            .lock()
            .unwrap() = payload.bytes.clone();
        log(format!(
            "restore source {}",
            g.audio_source(source).unwrap().persistent_id()
        ));
        true
    }
    fn restore_audio_modification(
        &mut self,
        cx: &mut Cx<'_, Self>,
        modification: AudioModificationId,
        payload: &Payload,
    ) -> bool {
        let g = cx.graph();
        let m = g.audio_modification(modification).unwrap();
        log(format!("restore modification {}", m.persistent_id()));
        if payload.bytes == b"malformed" {
            return false;
        }
        *m.data.edits.lock().unwrap() = payload.bytes.clone();
        true
    }
    fn did_keep_newer_archive(&mut self, _cx: &mut Cx<'_, Self>, object: ArchivedObject) {
        log(format!(
            "kept newer {}",
            match object {
                ArchivedObject::Document => "document",
                ArchivedObject::AudioSource(_) => "source",
                ArchivedObject::AudioModification(_) => "modification",
            }
        ));
    }

    fn audio_source_content_grade(
        &self,
        graph: &Graph<Self>,
        source: AudioSourceId,
        ty: ContentType,
    ) -> Option<ContentGrade> {
        let analysed = graph
            .audio_source(source)?
            .data
            .samples
            .lock()
            .unwrap()
            .is_some();
        (ty == ContentType::Notes && analysed).then_some(ContentGrade::Detected)
    }
    fn audio_source_content(
        &self,
        graph: &Graph<Self>,
        source: AudioSourceId,
        ty: ContentType,
        _range: Option<TimeRange>,
    ) -> Option<ContentEvents> {
        self.audio_source_content_grade(graph, source, ty)?;
        let s = graph.audio_source(source)?;
        let seconds = s.properties().sample_count as f64 / s.properties().sample_rate;
        Some(ContentEvents::Notes(vec![Note {
            frequency: 440.0,
            pitch_number: 69,
            volume: 0.5,
            start_position: 0.0,
            attack_duration: 0.0,
            note_duration: seconds,
            signal_duration: seconds,
        }]))
    }
}

/// A ramp per channel: sample i of channel c is `c + i / 1e6`, so every sample is distinct.
pub fn ramp(channels: usize, frames: usize) -> Vec<Vec<f32>> {
    (0..channels)
        .map(|c| (0..frames).map(|i| c as f32 + i as f32 * 1e-6).collect())
        .collect()
}

pub fn source_spec(persistent_id: &str, frames: usize) -> SourceSpec {
    SourceSpec {
        name: format!("take {persistent_id}"),
        persistent_id: persistent_id.into(),
        sample_rate: 44_100.0,
        channels: ramp(2, frames),
    }
}

/// The song's key: D major from bar 1.
pub fn d_major() -> ContextContent {
    let major = [
        true, false, true, false, true, true, false, true, false, true, false, true,
    ];
    ContextContent {
        tempo: vec![
            TempoEntry {
                time_position: 0.0,
                quarter_position: 0.0,
            },
            TempoEntry {
                time_position: 0.5,
                quarter_position: 1.0,
            },
        ],
        bars: vec![BarSignature {
            numerator: 4,
            denominator: 4,
            position: 0.0,
        }],
        keys: vec![KeySignature {
            root: 2,
            intervals: major,
            name: Some("D major".into()),
            position: 0.0,
        }],
    }
}

/// A host of the test controller's factory, initialized at ARA 2.3.
pub fn host(config: HostConfig) -> TestHost {
    take_log();
    set(Settings::default());
    let host = TestHost::of::<TestController>(config);
    host.initialize(HIGHEST_API_GENERATION);
    host
}

/// Waits (up to 10 s) for a condition another thread makes true.
pub fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(
            start.elapsed().as_secs() < 10,
            "timed out waiting for {what}"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// A flag a test can share with a thread.
pub fn flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

pub fn is_set(f: &AtomicBool) -> bool {
    f.load(Ordering::SeqCst)
}
