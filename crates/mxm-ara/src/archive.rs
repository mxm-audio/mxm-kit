//! The archive container: the document controller's envelope around its controller's payloads,
//! each filed under its object's persistent ID, so stores and restores can be filtered and IDs
//! mapped without the controller knowing the format.
//!
//! ```text
//! magic  b"MXM-ARA\0"         8 bytes
//! version u32 LE              CONTAINER_VERSION; frozen: new needs are new entry kinds
//! count   u32 LE
//! entries count times:
//!   kind    u8                0 document data, 1 audio source, 2 audio modification; any other
//!                             kind is kept verbatim with the document data and written back
//!   schema  u32 LE            the controller's schema version for this payload
//!   id_len  u32 LE, id        the object's persistent ID (empty for the document), UTF-8
//!   len     u64 LE, payload
//! ```

use std::collections::HashMap;

use crate::controller::{Controller, Payload};
use crate::graph::{AudioModificationId, AudioSourceId, Graph};
use crate::sys;
use crate::util::{implemented_size, opt_string, read_field, struct_size};
use crate::validate::check;

pub(crate) const MAGIC: &[u8; 8] = b"MXM-ARA\0";
pub(crate) const CONTAINER_VERSION: u32 = 1;

pub(crate) const KIND_DOCUMENT: u8 = 0;
pub(crate) const KIND_AUDIO_SOURCE: u8 = 1;
pub(crate) const KIND_AUDIO_MODIFICATION: u8 = 2;

/// One archived object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) kind: u8,
    pub(crate) id: String,
    pub(crate) payload: Payload,
}

pub(crate) fn header(count: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CONTAINER_VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
    out
}

/// An entry's bytes before its payload, which is written after them as is.
pub(crate) fn entry_head(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::with_capacity(17 + entry.id.len());
    out.push(entry.kind);
    out.extend_from_slice(&entry.payload.schema.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(entry.id.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(entry.id.as_bytes());
    out.extend_from_slice(&(entry.payload.bytes.len() as u64).to_le_bytes());
    out
}

/// The whole container in one buffer.
#[cfg(test)]
pub(crate) fn encode(entries: &[Entry]) -> Vec<u8> {
    let mut out = header(entries.len());
    for e in entries {
        out.extend_from_slice(&entry_head(e));
        out.extend_from_slice(&e.payload.bytes);
    }
    out
}

/// Why an archive can't be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    NotOurs,
    NewerContainer,
    Truncated,
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<Entry>, DecodeError> {
    struct Cursor<'a>(&'a [u8]);
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
            if self.0.len() < n {
                return Err(DecodeError::Truncated);
            }
            let (head, rest) = self.0.split_at(n);
            self.0 = rest;
            Ok(head)
        }
        fn u32(&mut self) -> Result<u32, DecodeError> {
            Ok(u32::from_le_bytes(
                self.take(4)?
                    .try_into()
                    .map_err(|_| DecodeError::Truncated)?,
            ))
        }
        fn u64(&mut self) -> Result<u64, DecodeError> {
            Ok(u64::from_le_bytes(
                self.take(8)?
                    .try_into()
                    .map_err(|_| DecodeError::Truncated)?,
            ))
        }
    }
    let mut c = Cursor(bytes);
    if c.take(MAGIC.len()).map_err(|_| DecodeError::NotOurs)? != MAGIC {
        return Err(DecodeError::NotOurs);
    }
    if c.u32()? != CONTAINER_VERSION {
        return Err(DecodeError::NewerContainer);
    }
    let count = c.u32()?;
    let mut entries = Vec::with_capacity(count.min(1 << 16) as usize);
    for _ in 0..count {
        let kind = c.take(1)?[0];
        let schema = c.u32()?;
        let id_len = c.u32()? as usize;
        let id = String::from_utf8_lossy(c.take(id_len)?).into_owned();
        let len = usize::try_from(c.u64()?).map_err(|_| DecodeError::Truncated)?;
        let bytes = c.take(len)?.to_vec();
        entries.push(Entry {
            kind,
            id,
            payload: Payload { schema, bytes },
        });
    }
    Ok(entries)
}

// ---- filters -----------------------------------------------------------------------------------

/// What a store writes (`ARAStoreObjectsFilter`; all of it when the host gives none).
pub(crate) struct StoreSelection {
    pub(crate) document_data: bool,
    pub(crate) sources: Vec<AudioSourceId>,
    pub(crate) modifications: Vec<AudioModificationId>,
}

impl StoreSelection {
    /// # Safety
    /// `filter` must be null or point to a filter valid for the call.
    pub(crate) unsafe fn new<C: Controller>(
        graph: &Graph<C>,
        filter: *const sys::ARAStoreObjectsFilter,
    ) -> Self {
        if filter.is_null() {
            let sources: Vec<_> = graph.source_order.clone();
            let modifications = sources
                .iter()
                .filter_map(|s| graph.audio_source(*s))
                .flat_map(|s| s.audio_modifications.iter().copied())
                .collect();
            return StoreSelection {
                document_data: true,
                sources,
                modifications,
            };
        }
        type F = sys::ARAStoreObjectsFilter;
        // SAFETY: non-null; the size is checked before any field is read.
        unsafe {
            if !check!(
                struct_size(filter) >= implemented_size!(F, audioModificationRefs),
                Argument,
                filter,
                c"ARAStoreObjectsFilter too small"
            ) {
                return StoreSelection {
                    document_data: false,
                    sources: Vec::new(),
                    modifications: Vec::new(),
                };
            }
            let document_data =
                read_field!(filter, F, documentData).is_some_and(|b| b != sys::kARAFalse);
            let sources = refs(
                read_field!(filter, F, audioSourceRefs),
                read_field!(filter, F, audioSourceRefsCount),
            )
            .into_iter()
            .map(|r| AudioSourceId::from_ref(r as sys::ARAAudioSourceRef))
            .filter(|id| {
                check!(
                    graph.audio_source(*id).is_some(),
                    Argument,
                    filter,
                    c"store filter names an unknown audio source"
                )
            })
            .collect();
            let modifications = refs(
                read_field!(filter, F, audioModificationRefs),
                read_field!(filter, F, audioModificationRefsCount),
            )
            .into_iter()
            .map(|r| AudioModificationId::from_ref(r as sys::ARAAudioModificationRef))
            .filter(|id| {
                check!(
                    graph.audio_modification(*id).is_some(),
                    Argument,
                    filter,
                    c"store filter names an unknown audio modification"
                )
            })
            .collect();
            StoreSelection {
                document_data,
                sources,
                modifications,
            }
        }
    }
}

/// Copies a C array of refs (as addresses).
unsafe fn refs<T>(array: Option<*const *mut T>, count: Option<usize>) -> Vec<usize> {
    match (array, count) {
        (Some(a), Some(n)) if !a.is_null() && n > 0 => {
            // SAFETY: the host's array of `n` refs, valid for the call.
            unsafe { std::slice::from_raw_parts(a, n) }
                .iter()
                .map(|r| *r as usize)
                .collect()
        }
        _ => Vec::new(),
    }
}

/// What a restore reads, and into which objects (`ARARestoreObjectsFilter`): archived persistent
/// IDs, mapped to current ones where the host says, then to the objects that carry them.
pub(crate) struct RestoreSelection {
    pub(crate) document_data: bool,
    pub(crate) sources: HashMap<String, AudioSourceId>,
    pub(crate) modifications: HashMap<String, AudioModificationId>,
}

impl RestoreSelection {
    /// # Safety
    /// `filter` must be null or point to a filter valid for the call.
    pub(crate) unsafe fn new<C: Controller>(
        graph: &Graph<C>,
        filter: *const sys::ARARestoreObjectsFilter,
    ) -> Self {
        let mut sources_by_id = HashMap::new();
        let mut modifications_by_id = HashMap::new();
        for source in graph.audio_sources() {
            let unique = sources_by_id
                .insert(source.persistent_id().to_owned(), source.id())
                .is_none();
            check!(
                unique,
                State,
                std::ptr::null::<u8>(),
                c"audio source persistent IDs are not unique"
            );
            for m in source
                .audio_modifications()
                .iter()
                .filter_map(|m| graph.audio_modification(*m))
            {
                let unique = modifications_by_id
                    .insert(m.persistent_id().to_owned(), m.id())
                    .is_none();
                check!(
                    unique,
                    State,
                    std::ptr::null::<u8>(),
                    c"audio modification persistent IDs are not unique"
                );
            }
        }
        if filter.is_null() {
            return RestoreSelection {
                document_data: true,
                sources: sources_by_id,
                modifications: modifications_by_id,
            };
        }
        type F = sys::ARARestoreObjectsFilter;
        // SAFETY: non-null; the size is checked before any field is read; the ID arrays hold the
        // counts the host gives.
        unsafe {
            if !check!(
                struct_size(filter) >= implemented_size!(F, audioModificationCurrentIDs),
                Argument,
                filter,
                c"ARARestoreObjectsFilter too small"
            ) {
                return RestoreSelection {
                    document_data: false,
                    sources: HashMap::new(),
                    modifications: HashMap::new(),
                };
            }
            let document_data =
                read_field!(filter, F, documentData).is_some_and(|b| b != sys::kARAFalse);
            let sources = mapped(
                read_field!(filter, F, audioSourceIDsCount).unwrap_or(0),
                read_field!(filter, F, audioSourceArchiveIDs).unwrap_or(std::ptr::null()),
                read_field!(filter, F, audioSourceCurrentIDs).unwrap_or(std::ptr::null()),
                &sources_by_id,
            );
            let modifications = mapped(
                read_field!(filter, F, audioModificationIDsCount).unwrap_or(0),
                read_field!(filter, F, audioModificationArchiveIDs).unwrap_or(std::ptr::null()),
                read_field!(filter, F, audioModificationCurrentIDs).unwrap_or(std::ptr::null()),
                &modifications_by_id,
            );
            RestoreSelection {
                document_data,
                sources,
                modifications,
            }
        }
    }
}

/// Archive IDs to objects, through the current IDs the host maps them to (or the same IDs).
unsafe fn mapped<Id: Copy>(
    count: usize,
    archive_ids: *const sys::ARAPersistentID,
    current_ids: *const sys::ARAPersistentID,
    by_id: &HashMap<String, Id>,
) -> HashMap<String, Id> {
    let mut out = HashMap::new();
    if count == 0 || archive_ids.is_null() {
        return out;
    }
    for i in 0..count {
        // SAFETY: the host's arrays hold `count` strings (the caller's contract).
        let (archived, current) = unsafe {
            let archived = opt_string(*archive_ids.add(i));
            let current = if current_ids.is_null() {
                archived.clone()
            } else {
                opt_string(*current_ids.add(i))
            };
            (archived, current)
        };
        let (Some(archived), Some(current)) = (archived, current) else {
            continue;
        };
        check!(
            !out.contains_key(&archived),
            State,
            std::ptr::null::<u8>(),
            c"restore filter repeats a persistent ID"
        );
        if let Some(id) = by_id.get(&current) {
            out.insert(archived, *id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: u8, id: &str, schema: u32, bytes: &[u8]) -> Entry {
        Entry {
            kind,
            id: id.into(),
            payload: Payload {
                schema,
                bytes: bytes.to_vec(),
            },
        }
    }

    #[test]
    fn a_container_round_trips_every_kind_including_unknown_ones() {
        let entries = vec![
            entry(KIND_DOCUMENT, "", 1, b"doc"),
            entry(KIND_AUDIO_SOURCE, "src-1", 2, b"\x00\x01\x02"),
            entry(KIND_AUDIO_MODIFICATION, "mod-1", 1, b""),
            entry(9, "future", 7, b"kept"),
        ];
        assert_eq!(decode(&encode(&entries)), Ok(entries));
    }

    #[test]
    fn foreign_newer_and_truncated_archives_are_refused() {
        assert_eq!(decode(b"something else"), Err(DecodeError::NotOurs));
        let mut newer = encode(&[]);
        newer[8] = 2;
        assert_eq!(decode(&newer), Err(DecodeError::NewerContainer));
        let whole = encode(&[entry(KIND_AUDIO_SOURCE, "src", 1, b"payload")]);
        assert_eq!(
            decode(&whole[..whole.len() - 1]),
            Err(DecodeError::Truncated)
        );
        // The control: the same bytes, whole, decode.
        assert!(decode(&whole).is_ok());
    }
}
