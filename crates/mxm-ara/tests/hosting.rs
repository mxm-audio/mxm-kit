//! The host's half (`mxm_ara::hosting`) driving the crate's own plug-in side, as a DAW does: a
//! document built, read on the plug-in's worker, stored and restored into another, a bound
//! instance's regions and selection, both teardown orders, and the plug-in's tables read only as
//! far as they go.

mod common;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::ThreadId;

use common::*;
use mxm_ara::hosting::*;
use mxm_ara::{ContentGrade, ContentUpdateFlags, sys};

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

/// One test at a time: the factory's initialization is per library, the controller's log per
/// thread, and the counts this file holds at zero per process.
static ONE: Mutex<()> = Mutex::new(());

fn one() -> MutexGuard<'static, ()> {
    take_log();
    set(Settings::default());
    ONE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A DAW as a host: its recordings in memory, its song's content, and what the plug-in told it.
#[derive(Default)]
struct Daw {
    sources: Mutex<HashMap<usize, Vec<Vec<f32>>>>,
    readers: Mutex<HashMap<usize, usize>>,
    next_reader: AtomicUsize,
    reading_threads: Mutex<Vec<ThreadId>>,
    contexts: Mutex<HashMap<usize, MusicalContent>>,
    events: Mutex<Vec<String>>,
}

impl Daw {
    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn told(&self, event: String) {
        self.events.lock().unwrap().push(event);
    }
}

impl Host for Daw {
    fn create_audio_reader(&self, source: usize, _use_64_bit: bool) -> Option<usize> {
        self.sources.lock().unwrap().get(&source)?;
        let reader = 1000 + self.next_reader.fetch_add(1, Ordering::SeqCst);
        self.readers.lock().unwrap().insert(reader, source);
        Some(reader)
    }

    fn read_audio_samples(&self, reader: usize, position: i64, buffers: Samples<'_>) -> bool {
        let me = std::thread::current().id();
        let mut threads = self.reading_threads.lock().unwrap();
        if !threads.contains(&me) {
            threads.push(me);
        }
        drop(threads);
        let Some(source) = self.readers.lock().unwrap().get(&reader).copied() else {
            return false;
        };
        let sources = self.sources.lock().unwrap();
        let channels = &sources[&source];
        let at = |c: usize, i: usize| {
            let frame = position + i as i64;
            usize::try_from(frame)
                .ok()
                .and_then(|frame| channels[c].get(frame).copied())
                .unwrap_or(0.0)
        };
        match buffers {
            Samples::F32(out) => {
                for (c, buffer) in out.into_iter().enumerate() {
                    for (i, sample) in buffer.iter_mut().enumerate() {
                        *sample = at(c, i);
                    }
                }
            }
            Samples::F64(out) => {
                for (c, buffer) in out.into_iter().enumerate() {
                    for (i, sample) in buffer.iter_mut().enumerate() {
                        *sample = f64::from(at(c, i));
                    }
                }
            }
        }
        true
    }

    fn destroy_audio_reader(&self, reader: usize) {
        self.readers.lock().unwrap().remove(&reader);
    }

    fn musical_context_content(&self, context: usize) -> Option<MusicalContent> {
        self.contexts.lock().unwrap().get(&context).cloned()
    }

    fn analysis_progress(&self, source: usize, state: AnalysisState, _value: f32) {
        if state != AnalysisState::Updated {
            self.told(format!("analysis {source} {state:?}"));
        }
    }

    fn audio_source_content_changed(&self, source: usize, _flags: ContentUpdateFlags) {
        self.told(format!("source {source} changed"));
    }
}

/// The song's tempo, bars and key: D major from bar 1.
fn song() -> MusicalContent {
    let content = d_major();
    MusicalContent {
        tempo: content.tempo,
        bars: content.bars,
        keys: content.keys,
        grade: Some(ContentGrade::Approved),
    }
}

fn initialized() -> Arc<Initialized> {
    // SAFETY: the crate's own factory, a static.
    let factory = unsafe { Factory::from_raw(mxm_ara::factory::<TestController>().cast()) }
        .expect("the factory");
    assert_eq!(factory.info().factory_id, CONFIG.factory_id);
    assert_eq!(
        factory.api_generation(),
        Some(sys::kARAAPIGeneration_2_3_Final)
    );
    factory.initialize().expect("initialized")
}

struct Objects {
    context: MusicalContextRef,
    sequence: RegionSequenceRef,
    source: SourceRef,
    modification: ModificationRef,
    region: RegionRef,
}

/// A song of one recording on one track, the host's refs from `base` on, in its own edit.
fn build(doc: &DocumentController, base: usize) -> Objects {
    doc.edit(|d| build_in(d, base))
}

/// The same inside an edit already open.
fn build_in(d: &DocumentController, base: usize) -> Objects {
    {
        let context = d
            .create_musical_context(
                base,
                &MusicalContextProperties {
                    name: Some("song".into()),
                    ..MusicalContextProperties::default()
                },
            )
            .expect("a context");
        let sequence = d
            .create_region_sequence(
                base + 1,
                &RegionSequenceProperties {
                    name: Some("Lead".into()),
                    order_index: 0,
                    context,
                    color: Some(Color {
                        r: 0.2,
                        g: 0.6,
                        b: 0.9,
                    }),
                },
            )
            .expect("a sequence");
        let source = d
            .create_audio_source(
                base + 2,
                &SourceProperties {
                    name: Some("take".into()),
                    persistent_id: "take-1".into(),
                    sample_count: 3000,
                    sample_rate: 44_100.0,
                    channel_count: 2,
                    merits_64_bit_samples: false,
                },
            )
            .expect("a source");
        let modification = d
            .create_audio_modification(
                source,
                base + 3,
                &ModificationProperties {
                    name: Some("edits".into()),
                    persistent_id: "mod-1".into(),
                },
            )
            .expect("a modification");
        let region = d
            .create_playback_region(
                modification,
                base + 4,
                &RegionProperties {
                    transformation_flags: sys::kARAPlaybackTransformationNoChanges,
                    start_in_modification: 0.0,
                    duration_in_modification: 3000.0 / 44_100.0,
                    start_in_playback: 1.0,
                    duration_in_playback: 3000.0 / 44_100.0,
                    sequence,
                    name: Some("with tail".into()),
                    color: None,
                },
            )
            .expect("a region");
        Objects {
            context,
            sequence,
            source,
            modification,
            region,
        }
    }
}

fn tear(doc: &DocumentController, o: &Objects, base: usize) {
    doc.enable_audio_source_samples_access(o.source, false);
    doc.edit(|d| {
        d.destroy_playback_region(o.region);
        d.destroy_audio_modification(o.modification);
        d.destroy_audio_source(o.source, base + 2);
        d.destroy_region_sequence(o.sequence);
        d.destroy_musical_context(o.context);
    });
}

fn daw() -> Arc<Daw> {
    let daw = Arc::new(Daw::default());
    daw.sources.lock().unwrap().insert(12, ramp(2, 3000));
    daw.sources.lock().unwrap().insert(22, ramp(2, 3000));
    daw.contexts.lock().unwrap().insert(10, song());
    daw.contexts.lock().unwrap().insert(20, song());
    daw
}

#[test]
fn a_document_is_built_read_stored_restored_into_another_and_torn_down() {
    let _one = one();
    let (asserts, violations) = (assert_count(), mxm_ara::api_violations());
    let ara = initialized();
    let daw = daw();
    let doc = ara
        .create_document(daw.clone(), "song")
        .expect("a document");
    let o = build(&doc, 10);
    let log = take_log();
    assert!(
        log.contains(&"context keys Some(1) tempo Some(2)".to_owned()),
        "the plug-in read the host's key and tempo: {log:?}"
    );

    // The plug-in reads the recording on its worker, never on the host's thread.
    doc.enable_audio_source_samples_access(o.source, true);
    wait_for("the plug-in's reads to end", || {
        doc.notify_model_updates();
        daw.events().contains(&"source 12 changed".to_owned())
    });
    assert!(daw.events().contains(&"analysis 12 Started".to_owned()));
    let threads = daw.reading_threads.lock().unwrap().clone();
    assert!(!threads.is_empty());
    assert!(!threads.contains(&std::thread::current().id()));

    // Head and tail from another thread.
    let query = doc.head_tail().expect("the query");
    let region = o.region;
    let (head, tail) = std::thread::spawn(move || query.of(region)).join().unwrap();
    assert_eq!((head, tail), (0.0, 0.5));

    // Stored, then restored into a new document of the same persistent ids.
    let archive = doc.store(None).expect("an archive");
    assert!(!archive.is_empty());
    let again = ara
        .create_document(daw.clone(), "song again")
        .expect("another");
    let archive_id = ara.factory().info().document_archive_id;
    let (restored, o2) = again.edit(|d| {
        let o2 = build_in(d, 20);
        (d.restore(&archive, &archive_id, None), o2)
    });
    assert_eq!(restored, Some(true));
    let log = take_log();
    assert!(log.contains(&"restore source take-1".to_owned()), "{log:?}");
    assert!(
        log.contains(&"restore modification mod-1".to_owned()),
        "{log:?}"
    );

    // A filtered store writes what it names.
    let filtered = doc
        .store(Some(&StoreFilter {
            document_data: false,
            sources: vec![o.source],
            modifications: Vec::new(),
        }))
        .expect("a filtered archive");
    assert!(!filtered.is_empty() && filtered.len() < archive.len());

    tear(&again, &o2, 20);
    tear(&doc, &o, 10);
    drop(again);
    drop(doc);
    assert!(
        daw.readers.lock().unwrap().is_empty(),
        "every reader destroyed"
    );
    assert_eq!(assert_count(), asserts, "{:?}", take_asserts());
    assert_eq!(mxm_ara::api_violations(), violations);
}

#[test]
fn a_bound_instance_takes_regions_and_the_selection_in_either_teardown_order() {
    for instance_first in [true, false] {
        let _one = one();
        let violations = mxm_ara::api_violations();
        let ara = initialized();
        let daw = daw();
        let doc = ara
            .create_document(daw.clone(), "song")
            .expect("a document");
        let o = build(&doc, 10);
        let bound =
            mxm_ara::PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL)
                .expect("bound");
        // SAFETY: used while `bound` lives.
        let instance =
            unsafe { PlugInExtension::from_raw(bound.instance()) }.expect("the instance");
        assert!(
            instance.has_playback_renderer()
                && instance.has_editor_renderer()
                && instance.has_editor_view()
        );

        instance.add_playback_region(o.region).expect("added");
        assert_eq!(
            bound.render_access().map(|a| a.playback_regions().count()),
            Some(1)
        );
        instance.add_editor_region(o.region).expect("previewed");
        instance
            .notify_selection(&[o.region], &[o.sequence], Some((1.0, 0.5)))
            .expect("told");
        let log = take_log();
        assert!(log.contains(&"renderer +region".to_owned()), "{log:?}");
        assert!(log.contains(&"selection 1 regions".to_owned()), "{log:?}");
        instance.remove_playback_region(o.region).expect("removed");
        instance
            .remove_editor_region(o.region)
            .expect("no longer previewed");
        assert_eq!(
            bound.render_access().map(|a| a.playback_regions().count()),
            Some(0)
        );

        tear(&doc, &o, 10);
        if instance_first {
            drop(bound);
            drop(doc);
        } else {
            drop(doc);
            assert!(bound.render_access().is_none());
            drop(bound);
        }
        assert_eq!(mxm_ara::api_violations(), violations);
    }
}

#[test]
fn a_call_off_the_controllers_thread_is_refused() {
    let _one = one();
    let ara = initialized();
    let doc = ara.create_document(daw(), "song").expect("a document");
    let elsewhere = std::thread::scope(|s| s.spawn(|| doc.begin_editing()).join().unwrap());
    assert_eq!(elsewhere, None);
    assert_eq!(doc.begin_editing(), Some(()));
    assert_eq!(doc.end_editing(), Some(()));
}

#[test]
fn the_factory_is_initialized_once_however_many_hold_it() {
    let _one = one();
    let first = initialized();
    let second = initialized();
    assert!(Arc::ptr_eq(&first, &second));
    drop(first);
    // Still initialized: a document can still be made.
    assert!(second.create_document(daw(), "song").is_some());
}

#[test]
fn the_plug_ins_tables_are_read_only_as_far_as_they_go() {
    // SAFETY (each from_raw below): a struct of ours, as large as its size says.
    assert!(unsafe { Factory::from_raw(std::ptr::null()) }.is_none());
    // SAFETY: zero is a valid ARAFactory (sizes, nulls, `None`s).
    let mut f: sys::ARAFactory = unsafe { std::mem::zeroed() };
    f.structSize = 8;
    assert!(unsafe { Factory::from_raw(std::ptr::from_ref(&f).cast()) }.is_none());

    f.structSize = std::mem::size_of::<sys::ARAFactory>();
    let generation = |f: &sys::ARAFactory| {
        unsafe { Factory::from_raw(std::ptr::from_ref(f).cast()) }
            .unwrap()
            .api_generation()
    };
    f.lowestSupportedApiGeneration = sys::kARAAPIGeneration_2_0_Final;
    f.highestSupportedApiGeneration = sys::kARAAPIGeneration_2_0_Final;
    assert_eq!(generation(&f), Some(sys::kARAAPIGeneration_2_0_Final));
    f.highestSupportedApiGeneration = sys::kARAAPIGeneration_2_3_Final + 3;
    assert_eq!(generation(&f), Some(sys::kARAAPIGeneration_2_3_Final));
    f.lowestSupportedApiGeneration = sys::kARAAPIGeneration_1_0_Final;
    f.highestSupportedApiGeneration = sys::kARAAPIGeneration_2_0_Draft;
    assert_eq!(generation(&f), None, "ARA 1 and 2.0's draft aren't hosted");
    // No strings: empty, not a crash.
    assert_eq!(
        unsafe { Factory::from_raw(std::ptr::from_ref(&f).cast()) }
            .unwrap()
            .info()
            .factory_id,
        ""
    );

    // An instance as large as its playback renderer: what lies past it reads as absent, whatever
    // is there.
    // SAFETY: zero is valid for both structs.
    let renderer: sys::ARAPlaybackRendererInterface = unsafe { std::mem::zeroed() };
    let mut e: sys::ARAPlugInExtensionInstance = unsafe { std::mem::zeroed() };
    e.structSize = std::mem::offset_of!(sys::ARAPlugInExtensionInstance, editorRendererRef);
    e.playbackRendererInterface = &renderer;
    e.editorViewInterface = std::ptr::NonNull::dangling().as_ptr();
    let instance = unsafe { PlugInExtension::from_raw(std::ptr::from_ref(&e).cast()) }.unwrap();
    assert!(instance.has_playback_renderer());
    assert!(!instance.has_editor_view());
    // Its view isn't there: refused, not called.
    assert_eq!(instance.notify_selection(&[], &[], None), None);
    e.structSize = 8;
    assert!(unsafe { PlugInExtension::from_raw(std::ptr::from_ref(&e).cast()) }.is_none());
}
