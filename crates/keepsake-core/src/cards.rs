//! Sony cards: recognising them by folder layout and remembering them.
//!
//! A card is remembered by [`VolumeIdentity::card_key`] (file-system type,
//! serial and size), so it is the same card whichever reader, slot or drive
//! letter it turns up in. Reformatting a card gives it a new serial, and it
//! then counts as a new card (it has nothing old left to import anyway).
//!
//! Card memory lives in the library folder, not the catalog, because it is
//! user state that cannot be rebuilt from the media:
//! - `cards.json`: one [`CardRecord`] per card;
//! - `cards/<id>.json`: the originals imported from that card, as they were
//!   (path, size, modified time), so "new since last import" is a quick
//!   directory listing, with no hashing.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, ImportAssetState, ImportDiff, ImportSessionId};
use crate::devices::VolumeIdentity;
use crate::library::{write_json_atomic, Library, LibraryError};
use crate::relpath::CaseSensitivity;

/// Identifier of a remembered card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CardId(pub uuid::Uuid);

impl CardId {
    pub fn new_random() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A Sony folder layout found on a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardLayout {
    /// `PRIVATE/M4ROOT`: XAVC S / XAVC HS / XAVC S-I clips.
    Xavc,
    /// `PRIVATE/AVCHD`: AVCHD.
    Avchd,
    /// `MP_ROOT`: older MP4 movies.
    Mp4,
    /// `DCIM/nnnMSDCF`: Sony stills.
    Stills,
}

impl CardLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            CardLayout::Xavc => "XAVC",
            CardLayout::Avchd => "AVCHD",
            CardLayout::Mp4 => "MP4",
            CardLayout::Stills => "stills",
        }
    }
}

/// Entry of `dir` named `name`, ignoring case (cards are FAT/exFAT).
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
}

fn is_msdcf(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 8 && b[..3].iter().all(u8::is_ascii_digit) && name[3..].eq_ignore_ascii_case("MSDCF")
}

/// The Sony layouts on the volume at `mount` (empty: not a Sony card).
/// A `DCIM` folder without Sony's `nnnMSDCF` naming does not count.
pub fn recognize(mount: &Path) -> Vec<CardLayout> {
    let mut out = Vec::new();
    if let Some(private) = child(mount, "PRIVATE") {
        if child(&private, "M4ROOT").is_some() {
            out.push(CardLayout::Xavc);
        }
        if child(&private, "AVCHD").is_some() {
            out.push(CardLayout::Avchd);
        }
    }
    if child(mount, "MP_ROOT").is_some() {
        out.push(CardLayout::Mp4);
    }
    if let Some(dcim) = child(mount, "DCIM") {
        let sony = std::fs::read_dir(&dcim)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.path().is_dir() && is_msdcf(&e.file_name().to_string_lossy()))
            })
            .unwrap_or(false);
        if sony {
            out.push(CardLayout::Stills);
        }
    }
    out
}

/// One asset on a card, by its original file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardItem {
    /// The same key ingest uses for the asset (`source_key`).
    pub key: String,
    /// Path of the original on the card.
    pub original: String,
    pub size: u64,
    /// Modified time in milliseconds since the epoch (0 if unknown).
    pub mtime_ms: i64,
}

/// The assets on the card at `mount`, grouped the way ingest groups them.
pub fn items(mount: &Path) -> Vec<CardItem> {
    let (files, _) = crate::rebuild::list_media_files(mount);
    let grouping = crate::rebuild::group_folder(mount, &files);
    let mut out = Vec::with_capacity(grouping.groups.len());
    for g in &grouping.groups {
        let Some(rel) = g.members.first().and_then(|m| files.get(m.index)) else {
            continue;
        };
        let Ok(md) = std::fs::metadata(rel.to_fs_path(mount)) else {
            continue;
        };
        let mtime_ms = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        out.push(CardItem {
            key: rel.key(CaseSensitivity::Insensitive),
            original: rel.as_str().to_owned(),
            size: md.len(),
            mtime_ms,
        });
    }
    out.sort_by(|a, b| a.original.cmp(&b.original));
    out
}

/// A card the library has seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardRecord {
    pub id: CardId,
    /// [`VolumeIdentity::card_key`], or `path:<mount>` when the OS gave no
    /// serial (such a card is only recognised at the same mount point).
    pub key: String,
    /// Name shown to the user (the volume label when first seen).
    pub label: String,
    #[serde(default)]
    pub layouts: Vec<CardLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hw_serial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hw_model: Option<String>,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_mount: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_import_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_session: Option<ImportSessionId>,
    /// Imported automatically by `keepsake watch --auto-ingest`.
    #[serde(default)]
    pub trusted: bool,
    /// Preset used for automatic imports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Seen {
    at_ms: i64,
    items: Vec<CardItem>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn cards_file(lib: &Library) -> PathBuf {
    lib.root().join("cards.json")
}

fn seen_file(lib: &Library, id: CardId) -> PathBuf {
    lib.root().join("cards").join(format!("{id}.json"))
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T, LibraryError> {
    match std::fs::read(path) {
        Ok(b) => Ok(serde_json::from_slice(&b)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}

/// Every remembered card.
pub fn list(lib: &Library) -> Result<Vec<CardRecord>, LibraryError> {
    read_json(&cards_file(lib))
}

/// Add or replace a card record.
pub fn save(lib: &Library, card: &CardRecord) -> Result<(), LibraryError> {
    let mut all = list(lib)?;
    all.retain(|c| c.id != card.id);
    all.push(card.clone());
    all.sort_by(|a, b| a.first_seen_ms.cmp(&b.first_seen_ms).then(a.id.cmp(&b.id)));
    write_json_atomic(&cards_file(lib), &all)
}

/// Find a card by id (or a unique id prefix of at least 4 characters) or
/// by label (case-insensitive, must be unique).
pub fn find(lib: &Library, name: &str) -> Result<Option<CardRecord>, LibraryError> {
    let all = list(lib)?;
    let name = name.trim();
    if let Some(c) = all.iter().find(|c| c.id.to_string() == name) {
        return Ok(Some(c.clone()));
    }
    let pick = |m: Vec<&CardRecord>| -> Result<Option<CardRecord>, LibraryError> {
        match m.len() {
            0 => Ok(None),
            1 => Ok(Some(m[0].clone())),
            _ => Err(LibraryError::Invalid(format!(
                "{name} matches {} cards; use the card id",
                m.len()
            ))),
        }
    };
    let by_label: Vec<&CardRecord> = all
        .iter()
        .filter(|c| c.label.eq_ignore_ascii_case(name))
        .collect();
    if !by_label.is_empty() {
        return pick(by_label);
    }
    if name.len() >= 4 {
        let lower = name.to_ascii_lowercase();
        return pick(
            all.iter()
                .filter(|c| c.id.to_string().starts_with(&lower))
                .collect(),
        );
    }
    Ok(None)
}

/// Forget a card: its record and its "imported items" list. Import
/// sessions and media in the library are untouched.
pub fn forget(lib: &Library, id: CardId) -> Result<bool, LibraryError> {
    let mut all = list(lib)?;
    let before = all.len();
    all.retain(|c| c.id != id);
    if all.len() == before {
        return Ok(false);
    }
    write_json_atomic(&cards_file(lib), &all)?;
    match std::fs::remove_file(seen_file(lib, id)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(true)
}

/// The key a card at `mount` with this identity is remembered by.
pub fn key_for(mount: &Path, identity: &VolumeIdentity) -> String {
    identity
        .card_key()
        .unwrap_or_else(|| format!("path:{}", mount.display()))
}

/// What we know about a card that has just been seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CardStatus {
    pub card: CardRecord,
    /// Seen before (by this library).
    pub known: bool,
    /// Assets on the card now.
    pub items: usize,
    /// Assets not imported from this card before (all of them for a card
    /// never imported from).
    pub new_items: usize,
    /// Originals of the new assets.
    pub new: Vec<String>,
}

/// Look at the volume at `mount`: `None` if it is not a Sony card,
/// otherwise its status. Remembers the card (new cards are added, known
/// ones get their last-seen time and mount updated). Never writes to the
/// card.
pub fn identify(
    lib: &Library,
    mount: &Path,
    identity: &VolumeIdentity,
) -> Result<Option<CardStatus>, LibraryError> {
    let layouts = recognize(mount);
    if layouts.is_empty() {
        return Ok(None);
    }
    let key = key_for(mount, identity);
    let now = now_ms();
    let existing = list(lib)?.into_iter().find(|c| c.key == key);
    let known = existing.is_some();
    let mut card = existing.unwrap_or_else(|| CardRecord {
        id: CardId::new_random(),
        key,
        label: identity
            .label
            .clone()
            .filter(|l| !l.trim().is_empty())
            .unwrap_or_else(|| "Card".to_owned()),
        layouts: Vec::new(),
        capacity: identity.capacity,
        hw_serial: None,
        hw_model: None,
        first_seen_ms: now,
        last_seen_ms: now,
        last_mount: None,
        last_import_ms: None,
        last_session: None,
        trusted: false,
        preset: None,
    });
    card.layouts = layouts;
    card.last_seen_ms = now;
    card.last_mount = Some(mount.to_path_buf());
    if identity.hw_serial.is_some() {
        card.hw_serial = identity.hw_serial.clone();
    }
    if identity.hw_model.is_some() {
        card.hw_model = identity.hw_model.clone();
    }
    if identity.capacity.is_some() {
        card.capacity = identity.capacity;
    }
    save(lib, &card)?;
    let status = status_of(lib, card, known, mount)?;
    Ok(Some(status))
}

fn status_of(
    lib: &Library,
    card: CardRecord,
    known: bool,
    mount: &Path,
) -> Result<CardStatus, LibraryError> {
    let seen: Seen = read_json(&seen_file(lib, card.id))?;
    let seen: HashMap<&str, &CardItem> = seen.items.iter().map(|i| (i.key.as_str(), i)).collect();
    let now = items(mount);
    let new: Vec<String> = now
        .iter()
        .filter(|i| match seen.get(i.key.as_str()) {
            Some(s) => s.size != i.size || s.mtime_ms != i.mtime_ms,
            None => true,
        })
        .map(|i| i.original.clone())
        .collect();
    Ok(CardStatus {
        card,
        known,
        items: now.len(),
        new_items: new.len(),
        new,
    })
}

/// The status of a remembered card mounted at `mount`, without updating
/// anything.
pub fn status(lib: &Library, card: &CardRecord, mount: &Path) -> Result<CardStatus, LibraryError> {
    status_of(lib, card.clone(), true, mount)
}

/// After an import of the card at `mount`: remember which of its assets
/// are now in the library (catalogued in `session`, or found to be
/// already imported), so they no longer count as new.
pub fn record_import(
    lib: &Library,
    cat: &Catalog,
    card: CardId,
    mount: &Path,
    session: ImportSessionId,
) -> Result<(), LibraryError> {
    let Some(mut rec) = list(lib)?.into_iter().find(|c| c.id == card) else {
        return Err(LibraryError::Invalid(format!("unknown card {card}")));
    };
    let done: HashSet<String> = cat
        .read(|tx| tx.imports().assets(session))?
        .into_iter()
        .filter(|a| {
            a.state == ImportAssetState::Catalogued || a.diff == ImportDiff::AlreadyImported
        })
        .map(|a| a.source_key)
        .collect();
    let old: Seen = read_json(&seen_file(lib, card))?;
    let old: HashMap<String, CardItem> =
        old.items.into_iter().map(|i| (i.key.clone(), i)).collect();
    // Keep what is on the card now and was imported, this time or before.
    let kept: Vec<CardItem> = items(mount)
        .into_iter()
        .filter(|i| {
            done.contains(&i.key)
                || old
                    .get(&i.key)
                    .is_some_and(|o| o.size == i.size && o.mtime_ms == i.mtime_ms)
        })
        .collect();
    let now = now_ms();
    write_json_atomic(
        &seen_file(lib, card),
        &Seen {
            at_ms: now,
            items: kept,
        },
    )?;
    rec.last_import_ms = Some(now);
    rec.last_session = Some(session);
    save(lib, &rec)
}

/// After an import or resume: if the session's source is a remembered
/// card that is still mounted, update its memory ([`record_import`]).
/// Returns the card, if any.
pub fn record_session(
    lib: &Library,
    cat: &Catalog,
    session: ImportSessionId,
) -> Result<Option<CardId>, LibraryError> {
    let Some(s) = cat.read(|tx| tx.imports().get(session))? else {
        return Ok(None);
    };
    let Some(vol) = lib
        .volumes()?
        .into_iter()
        .find(|v| v.id == s.source_volume_id)
    else {
        return Ok(None);
    };
    let Some(card) = list(lib)?.into_iter().find(|c| c.key == vol.fs_id) else {
        return Ok(None);
    };
    let Some(mount) = vol.last_mount.filter(|m| m.is_dir()) else {
        return Ok(None);
    };
    record_import(lib, cat, card.id, &mount, session)?;
    Ok(Some(card.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msdcf_names() {
        assert!(is_msdcf("100MSDCF"));
        assert!(is_msdcf("999msdcf"));
        assert!(!is_msdcf("100CANON"));
        assert!(!is_msdcf("10MSDCF"));
        assert!(!is_msdcf("A00MSDCF"));
    }

    #[test]
    fn layouts_are_recognised_ignoring_case() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        assert!(recognize(r).is_empty());
        std::fs::create_dir_all(r.join("DCIM/100CANON")).unwrap();
        assert!(recognize(r).is_empty(), "non-Sony DCIM is not a Sony card");
        std::fs::create_dir_all(r.join("DCIM/101msdcf")).unwrap();
        std::fs::create_dir_all(r.join("private/m4root")).unwrap();
        std::fs::create_dir_all(r.join("private/AVCHD")).unwrap();
        std::fs::create_dir_all(r.join("mp_root")).unwrap();
        let mut got = recognize(r);
        got.sort();
        assert_eq!(
            got,
            vec![
                CardLayout::Xavc,
                CardLayout::Avchd,
                CardLayout::Mp4,
                CardLayout::Stills
            ]
        );
    }
}
