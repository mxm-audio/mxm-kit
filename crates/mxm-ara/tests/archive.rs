//! Archives: a round trip; filtered stores and restores with persistent IDs mapped; a newer
//! schema kept verbatim and written back; a malformed object; a cloned modification.

mod common;

use common::*;
use mxm_ara::sys;
use mxm_ara::test_host::{
    HostConfig, HostEvent, Modification, RestoreFilter, Source, StoreFilter, TestDocument, TestHost,
};
use mxm_ara::*;

const ALL: u32 =
    (sys::kARAPlaybackRendererRole | sys::kARAEditorRendererRole | sys::kARAEditorViewRole) as u32;

/// A document with one source and its modifications, by persistent ID.
struct Song<'h> {
    doc: TestDocument<'h>,
    ext: PlugInExtension<TestController>,
    source: Source,
    modifications: Vec<Modification>,
}

impl<'h> Song<'h> {
    fn new(host: &'h TestHost, source: &str, modifications: &[&str]) -> Self {
        let doc = host.create_document("song").unwrap();
        let ext = PlugInExtension::<TestController>::bind(doc.controller_ref(), ALL, ALL).unwrap();
        let (source, modifications) = doc.edit(|d| {
            let s = d.create_audio_source(&source_spec(source, 100));
            let m = modifications
                .iter()
                .map(|id| d.create_audio_modification(&s, id, id))
                .collect();
            (s, m)
        });
        Song {
            doc,
            ext,
            source,
            modifications,
        }
    }

    fn set(&self, document: &str, analysis: &str, edits: &[&str]) {
        let s = AudioSourceId::from_ref(self.source.plugin_ref);
        let ms: Vec<_> = self
            .modifications
            .iter()
            .map(|m| AudioModificationId::from_ref(m.plugin_ref))
            .collect();
        self.ext.with_controller(|c, g| {
            c.document_data = document.as_bytes().to_vec();
            *g.audio_source(s).unwrap().data.analysis.lock().unwrap() =
                analysis.as_bytes().to_vec();
            for (m, e) in ms.iter().zip(edits) {
                *g.audio_modification(*m).unwrap().data.edits.lock().unwrap() =
                    e.as_bytes().to_vec();
            }
        });
    }

    /// (document data, analysis, each modification's edits).
    fn get(&self) -> (String, String, Vec<String>) {
        let s = AudioSourceId::from_ref(self.source.plugin_ref);
        let ms: Vec<_> = self
            .modifications
            .iter()
            .map(|m| AudioModificationId::from_ref(m.plugin_ref))
            .collect();
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        self.ext
            .with_controller(|c, g| {
                (
                    text(&c.document_data),
                    text(&g.audio_source(s).unwrap().data.analysis.lock().unwrap()),
                    ms.iter()
                        .map(|m| {
                            text(&g.audio_modification(*m).unwrap().data.edits.lock().unwrap())
                        })
                        .collect(),
                )
            })
            .unwrap()
    }

    fn kept(&self, index: usize) -> Option<Payload> {
        let m = AudioModificationId::from_ref(self.modifications[index].plugin_ref);
        self.ext
            .with_controller(|_, g| g.audio_modification(m).unwrap().kept_archive().cloned())
            .unwrap()
    }

    fn restore(&self, archive: &[u8], filter: Option<&RestoreFilter>) -> bool {
        self.doc.edit(|d| d.restore(archive, filter))
    }

    fn close(self) {
        let Song {
            doc,
            ext,
            source,
            modifications,
        } = self;
        drop(ext);
        doc.edit(|d| {
            for m in &modifications {
                d.destroy_audio_modification(m);
            }
            d.destroy_audio_source(&source);
        });
        doc.destroy();
    }
}

#[test]
fn an_archive_round_trips() {
    let host = host(HostConfig::default());
    let song = Song::new(&host, "take-1", &["mod-1"]);
    song.set("doc", "analysis-1", &["edits-1"]);
    let archive = song.doc.store(None).expect("stored");
    assert!(
        host.take_events()
            .contains(&HostEvent::ArchivingProgress(1.0))
    );
    song.close();

    let reopened = Song::new(&host, "take-1", &["mod-1"]);
    assert!(reopened.restore(&archive, None));
    assert_eq!(
        reopened.get(),
        ("doc".into(), "analysis-1".into(), vec!["edits-1".into()])
    );
    assert!(
        host.take_events()
            .contains(&HostEvent::UnarchivingProgress(1.0))
    );
    // Stored again, the same bytes.
    assert_eq!(reopened.doc.store(None).as_deref(), Some(&archive[..]));
    reopened.close();

    // The control: a document whose objects carry other IDs takes none of their state.
    let other = Song::new(&host, "take-9", &["mod-9"]);
    assert!(other.restore(&archive, None));
    assert_eq!(
        other.get(),
        ("doc".into(), String::new(), vec![String::new()])
    );
    other.close();
    assert!(host.asserts().is_empty(), "{:?}", host.asserts());
}

#[test]
fn a_filtered_store_and_restore_map_persistent_ids() {
    let host = host(HostConfig::default());
    let song = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    song.set("doc", "analysis", &["one", "two"]);
    let partial = song
        .doc
        .store(Some(&StoreFilter {
            document_data: false,
            sources: vec![],
            modifications: vec![song.modifications[0]],
        }))
        .unwrap();
    song.close();

    // Only mod-1 is in it.
    let both = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    assert!(both.restore(&partial, None));
    assert_eq!(
        both.get(),
        (
            String::new(),
            String::new(),
            vec!["one".into(), String::new()]
        )
    );
    both.close();

    // Imported into another song under another ID, by the host's mapping.
    let mapping = RestoreFilter {
        document_data: false,
        sources: vec![],
        modifications: vec![("mod-1".into(), Some("mod-copy".into()))],
    };
    let import = Song::new(&host, "take-9", &["mod-copy"]);
    assert!(import.restore(&partial, Some(&mapping)));
    assert_eq!(import.get().2, ["one"]);
    import.close();

    // The control: the same import without the mapping lands nowhere.
    let unmapped = RestoreFilter {
        document_data: false,
        sources: vec![],
        modifications: vec![("mod-1".into(), None)],
    };
    let import = Song::new(&host, "take-9", &["mod-copy"]);
    assert!(import.restore(&partial, Some(&unmapped)));
    assert_eq!(import.get().2, [""]);
    import.close();

    // A restore filter that leaves an object out restores nothing of it.
    let whole = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    whole.set("doc", "analysis", &["one", "two"]);
    let archive = whole.doc.store(None).unwrap();
    whole.close();
    let only_two = RestoreFilter {
        document_data: false,
        sources: vec![],
        modifications: vec![("mod-2".into(), None)],
    };
    let target = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    assert!(target.restore(&archive, Some(&only_two)));
    assert_eq!(
        target.get(),
        (
            String::new(),
            String::new(),
            vec![String::new(), "two".into()]
        )
    );
    target.close();
    assert!(host.asserts().is_empty(), "{:?}", host.asserts());
}

#[test]
fn a_newer_schema_is_kept_verbatim_and_written_back() {
    let host = host(HostConfig::default());
    // Written by a newer version of the plug-in.
    set(Settings {
        store_schema: 99,
        ..Settings::default()
    });
    let song = Song::new(&host, "take-1", &["mod-1"]);
    song.set("future doc", "future analysis", &["future edits"]);
    let newer = song.doc.store(None).unwrap();
    song.close();

    set(Settings::default());
    take_log();
    let reopened = Song::new(&host, "take-1", &["mod-1"]);
    assert!(reopened.restore(&newer, None));
    let log = take_log();
    for kept in [
        "kept newer source",
        "kept newer modification",
        "kept newer document",
    ] {
        assert!(log.iter().any(|l| l == kept), "missing {kept:?}: {log:#?}");
    }
    assert!(
        !log.iter().any(|l| l.starts_with("restore ")),
        "nothing reached the controller: {log:#?}"
    );
    assert_eq!(
        reopened.get(),
        (String::new(), String::new(), vec![String::new()])
    );
    assert_eq!(
        reopened.kept(0),
        Some(Payload {
            schema: 99,
            bytes: b"future edits".to_vec()
        })
    );
    assert_eq!(
        reopened.doc.store(None).as_deref(),
        Some(&newer[..]),
        "written back byte for byte"
    );

    // A cloned modification keeps the newer state too.
    let clone = reopened
        .doc
        .edit(|d| d.clone_audio_modification(&reopened.modifications[0], "mod-clone"));
    let clone_id = AudioModificationId::from_ref(clone.plugin_ref);
    assert_eq!(
        reopened
            .ext
            .with_controller(|_, g| g
                .audio_modification(clone_id)
                .unwrap()
                .kept_archive()
                .cloned())
            .unwrap(),
        Some(Payload {
            schema: 99,
            bytes: b"future edits".to_vec()
        })
    );
    reopened.doc.edit(|d| d.destroy_audio_modification(&clone));
    reopened.close();

    // The control: a schema this version reads goes to the controller and isn't kept.
    let current = Song::new(&host, "take-1", &["mod-1"]);
    current.set("doc", "analysis", &["edits"]);
    let archive = current.doc.store(None).unwrap();
    current.close();
    take_log();
    let reopened = Song::new(&host, "take-1", &["mod-1"]);
    assert!(reopened.restore(&archive, None));
    assert!(take_log().contains(&"restore modification mod-1".to_string()));
    assert_eq!(reopened.kept(0), None);
    reopened.close();
}

#[test]
fn a_malformed_object_fails_the_restore_but_not_its_neighbours() {
    let host = host(HostConfig::default());
    let song = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    song.set("", "analysis", &["malformed", "fine"]);
    let archive = song.doc.store(None).unwrap();
    song.close();
    let reopened = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    assert!(
        !reopened.restore(&archive, None),
        "the host is told the restore failed"
    );
    assert_eq!(reopened.get().2, ["", "fine"]);
    reopened.close();

    // The control: the same archive well formed restores clean.
    let song = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    song.set("", "analysis", &["good", "fine"]);
    let archive = song.doc.store(None).unwrap();
    song.close();
    let reopened = Song::new(&host, "take-1", &["mod-1", "mod-2"]);
    assert!(reopened.restore(&archive, None));
    reopened.close();

    // Not an archive of this plug-in at all.
    let reopened = Song::new(&host, "take-1", &["mod-1"]);
    assert!(!reopened.restore(b"not an archive", None));
    reopened.close();
}

#[test]
fn a_cloned_modification_copies_its_edits() {
    let host = host(HostConfig::default());
    let song = Song::new(&host, "take-1", &["mod-1"]);
    song.set("", "", &["edits"]);
    let (clone, fresh) = song.doc.edit(|d| {
        (
            d.clone_audio_modification(&song.modifications[0], "mod-clone"),
            d.create_audio_modification(&song.source, "mod-new", "new"),
        )
    });
    let edits = |m: &Modification| {
        let id = AudioModificationId::from_ref(m.plugin_ref);
        song.ext
            .with_controller(|_, g| {
                g.audio_modification(id)
                    .unwrap()
                    .data
                    .edits
                    .lock()
                    .unwrap()
                    .clone()
            })
            .unwrap()
    };
    assert_eq!(edits(&clone), b"edits");
    // The control: a new modification starts empty.
    assert_eq!(edits(&fresh), b"");
    assert!(take_log().contains(&"modification mod-clone cloned true".to_string()));
    song.doc.edit(|d| {
        d.destroy_audio_modification(&clone);
        d.destroy_audio_modification(&fresh);
    });
    song.close();
}
