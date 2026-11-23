//! The plug-in side of ARA 2 (Celemony's Audio Random Access, SDK 2.3) in Rust.
//!
//! A plug-in implements [`Controller`] (what its document controller does at each moment of the
//! model's life) and hands its companion API two things, which nice-plug's `AraHooks` asks for:
//!
//! - the factory: [`factory::<MyController>()`](factory), a static `ARAFactory`;
//! - the binding: [`bind_into`], which binds an instance to the host's document controller and
//!   keeps the [`PlugInExtension`] in the instance.
//!
//! The document controller keeps the model graph ([`Graph`]: musical contexts, region sequences,
//! audio sources, audio modifications and playback regions, with their properties and links) and
//! the host's interfaces ([`Cx`] grants their calls where ARA allows them). The audio thread
//! reads the graph through [`PlugInExtension::render_access`], ARA's renderer guard: refused, never
//! waiting, while the host edits.
//!
//! The `test-host` feature adds [`test_host`], a Rust ARA host that drives a factory through
//! ARA's C function tables, as a real host does.
//!
//! [`hosting`] is the host's half, for a DAW that hosts ARA plug-ins: a factory initialized and
//! counted, the host's interfaces behind one trait, a document controller's whole interface, and a
//! bound instance's renderers and view.
//!
//! The C API itself is [`sys`], generated from the vendored `ara_api/ARAInterface.h`.

pub mod sys;

mod archive;
mod content;
mod controller;
mod document;
mod extension;
mod factory;
mod graph;
mod guard;
mod host;
pub mod hosting;
mod notify;
mod util;
mod validate;

#[cfg(feature = "test-host")]
pub mod test_host;

pub use content::{
    BarSignature, Chord, ContentEvents, ContentGrade, ContentType, ContentUpdateFlags, HostContent,
    HostContentEvents, KeySignature, Note, TempoEntry, TimeRange, Tuning,
};
pub use controller::{
    ArchivedObject, Controller, Cx, Init, Payload, ProcessingAlgorithm, Roles, ViewSelection,
};
pub use extension::{EditorAssignment, PlugInExtension, RenderAccess, bind_into};
pub use factory::{
    FactoryConfig, HIGHEST_API_GENERATION, LOWEST_API_GENERATION, api_generation, factory,
};
pub use graph::{
    AudioModification, AudioModificationId, AudioModificationProperties, AudioSource,
    AudioSourceId, AudioSourceProperties, ChannelArrangement, Color, DocumentProperties,
    ExtensionId, Graph, MusicalContext, MusicalContextId, MusicalContextProperties, PlaybackRegion,
    PlaybackRegionId, PlaybackRegionProperties, RegionSequence, RegionSequenceId,
    RegionSequenceProperties,
};
pub use host::{ArchiveReader, ArchiveWriter, AudioReader, PlaybackController, ReadError, Sample};
pub use notify::{AnalysisProgress, Notifier};
pub use validate::api_violations;
