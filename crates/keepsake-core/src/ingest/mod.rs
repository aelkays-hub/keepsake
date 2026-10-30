//! The ingest engine: copy a camera card to one or two destinations, verify
//! every byte, catalogue the result, and say when the card is safe to format.
//!
//! Each asset moves through `discovered → selected → copying → copied →
//! verified → catalogued` (or `failed` / `skipped`). Every transition is
//! stored in the catalog, so an interrupted import resumes with [`run`].
//!
//! Safety rules, enforced here:
//! - the source is only ever opened for reading ([`source::SourceVolume`]);
//! - a destination may not be inside the source or on the same file system
//!   as the source or as the other destination (unless explicitly allowed
//!   for testing);
//! - bytes land in a hidden temporary file, are flushed, re-read past the OS
//!   cache and compared, and only then renamed to the final name; a final
//!   file is never replaced;
//! - the catalog records "verified" only after the final file was read back
//!   and matched.

pub mod copy;
pub mod fault;
pub mod source;
pub(crate) mod sys;
pub mod template;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::DateTime;
use serde::Serialize;
use thiserror::Error;

use crate::catalog::{
    now_ms, Asset, AssetId, Blake3Hash, Catalog, CatalogError, FileRole, ImportAsset,
    ImportAssetState, ImportDestination, ImportDiff, ImportItem, ImportItemStatus, ImportSession,
    ImportSessionId, ImportStatus, MediaKind, NewFile, SequenceId, StillSequence, VolumeId,
    VolumeKind, WriteTx,
};
use crate::grouping::{Group, GroupKind, Grouping, Member};
use crate::library::{Library, LibraryError, VolumeRecord};
use crate::probe::{ProbeReport, Prober};
use crate::relpath::{path_key, CaseSensitivity, RelPath};
use fault::{CrashPoint, FaultPlan};
use source::SourceVolume;
use template::{Template, TemplateError, TemplateInput};

/// Errors that stop an ingest run.
#[derive(Debug, Error)]
pub enum IngestError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("refused: {0}")]
    Unsafe(String),
    #[error("not enough space on {dest}: {need} bytes needed, {free} free")]
    NoSpace { dest: String, need: u64, free: u64 },
    #[error("the source card is not available at {0}")]
    SourceMissing(PathBuf),
    #[error("destination {0} is not available")]
    DestinationMissing(String),
    #[error("unknown import session {0}")]
    UnknownSession(ImportSessionId),
    #[error("cancelled")]
    Cancelled,
    #[error("simulated crash (fault injection)")]
    SimulatedCrash,
}

impl From<crate::xmp::XmpError> for IngestError {
    fn from(e: crate::xmp::XmpError) -> Self {
        IngestError::Library(LibraryError::Xmp(e))
    }
}

/// A destination volume and where it is mounted.
#[derive(Debug, Clone)]
pub struct Destination {
    pub volume: VolumeRecord,
    pub mount: PathBuf,
}

/// Which assets to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// New and modified assets (the default).
    NewOnly,
    /// Everything, including assets already in the library.
    All,
    /// Nothing yet; select later.
    Nothing,
}

/// How to plan an import.
#[derive(Debug, Clone)]
pub struct PlanOptions {
    pub template: Template,
    pub selection: Selection,
    pub event: Option<String>,
    /// Testing only: allow destinations on the same file system as the
    /// source or each other. "Safe to format" then counts volumes, not disks.
    pub allow_same_filesystem: bool,
    /// How the source card is identified in the volume registry (see
    /// [`crate::cards::key_for`]). `None`: by its mount path.
    pub source_id: Option<String>,
}

/// Progress callbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Progress {
    FileStarted {
        file: String,
        bytes: u64,
    },
    FileBytes {
        file: String,
        done: u64,
        total: u64,
    },
    FileDone {
        file: String,
    },
    AssetState {
        asset: String,
        state: ImportAssetState,
    },
    Phase {
        name: String,
    },
}

/// What happened, from the catalog's point of view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct IngestReport {
    pub session: Option<ImportSessionId>,
    pub assets: u64,
    pub by_state: BTreeMap<String, u64>,
    /// New assets the user did not select (they are not copied).
    pub unselected: u64,
    pub bytes_copied: u64,
    /// `(original file, error)` for failed assets.
    pub failed: Vec<(String, String)>,
    pub safe_to_format: bool,
    /// Why the card is not safe to format yet.
    pub not_safe: Vec<String>,
}

fn now() -> i64 {
    now_ms()
}

fn session_tag(id: ImportSessionId) -> String {
    id.0.simple().to_string()
}

/// Register a destination folder as a volume (writable, probed for case rules).
pub fn destination_volume(
    lib: &Library,
    mount: &Path,
    label: &str,
    kind: VolumeKind,
) -> Result<Destination, IngestError> {
    std::fs::create_dir_all(mount)?;
    let mount = std::fs::canonicalize(mount)?;
    let probe = crate::library::probe_volume(&mount);
    if probe.read_only {
        return Err(IngestError::Unsafe(format!(
            "destination {} is not writable",
            mount.display()
        )));
    }
    let fs_id = mount.display().to_string();
    let records = lib.volumes()?;
    let at = |v: &VolumeRecord| {
        v.last_mount
            .as_deref()
            .map(|m| std::fs::canonicalize(m).unwrap_or_else(|_| m.to_path_buf()))
    };
    let existing = records
        .iter()
        .find(|v| v.fs_id == fs_id || at(v).as_deref() == Some(mount.as_path()))
        .cloned();
    // A folder inside (or holding) one already in the library would be
    // catalogued twice: import into that folder itself instead.
    if existing.is_none() {
        for v in records.iter().filter(|v| v.kind != VolumeKind::Card) {
            let Some(m) = at(v) else { continue };
            if mount.starts_with(&m) || m.starts_with(&mount) {
                return Err(IngestError::Unsafe(format!(
                    "destination {} overlaps {} (\"{}\"), already in the library: choose that folder itself",
                    mount.display(),
                    m.display(),
                    v.label
                )));
            }
        }
    }
    let volume = match existing {
        // Already in the library (an earlier destination, or a folder added
        // in place): keep its identity and settings. A folder added in place
        // stays read-only for sidecars, which go to the library's mirror;
        // the import only adds new files to it.
        Some(v) => VolumeRecord {
            last_mount: Some(mount.clone()),
            ..v
        },
        None => VolumeRecord {
            id: VolumeId::new_random(),
            label: label.to_owned(),
            fs_id,
            kind,
            case: probe.case,
            read_only: false,
            last_mount: Some(mount.clone()),
        },
    };
    lib.register_volume(&volume)?;
    Ok(Destination { volume, mount })
}

fn source_fs_id(mount: &Path, opts: &PlanOptions) -> String {
    opts.source_id
        .clone()
        .unwrap_or_else(|| mount.display().to_string())
}

fn source_volume(
    lib: &Library,
    mount: &Path,
    label: &str,
    opts: &PlanOptions,
) -> Result<VolumeRecord, IngestError> {
    let fs_id = source_fs_id(mount, opts);
    let existing = lib.volumes()?.into_iter().find(|v| v.fs_id == fs_id);
    // Cards are always treated as read-only: no sidecar is ever written there.
    let rec = VolumeRecord {
        id: existing.map(|v| v.id).unwrap_or_else(VolumeId::new_random),
        label: label.to_owned(),
        fs_id,
        kind: VolumeKind::Card,
        case: CaseSensitivity::Insensitive,
        read_only: true,
        last_mount: Some(mount.to_path_buf()),
    };
    lib.register_volume(&rec)?;
    Ok(rec)
}

fn check_destinations(
    src: &SourceVolume,
    dests: &[Destination],
    allow_same_fs: bool,
) -> Result<(), IngestError> {
    if dests.is_empty() || dests.len() > 2 {
        return Err(IngestError::Unsafe("choose one or two destinations".into()));
    }
    let fs_of = |p: &Path| sys::existing_ancestor(p).and_then(|a| sys::filesystem_id(&a));
    let src_fs = fs_of(src.root());
    for (i, d) in dests.iter().enumerate() {
        if src.contains(&d.mount) {
            return Err(IngestError::Unsafe(format!(
                "destination {} is inside the source card",
                d.mount.display()
            )));
        }
        if d.mount.starts_with(src.root()) || src.root().starts_with(&d.mount) {
            return Err(IngestError::Unsafe(format!(
                "destination {} and the source overlap",
                d.mount.display()
            )));
        }
        for other in &dests[i + 1..] {
            if d.volume.id == other.volume.id
                || d.mount.starts_with(&other.mount)
                || other.mount.starts_with(&d.mount)
            {
                return Err(IngestError::Unsafe("the two destinations overlap".into()));
            }
            if !allow_same_fs && fs_of(&d.mount) == fs_of(&other.mount) {
                return Err(IngestError::Unsafe(
                    "both destinations are on the same disk; two copies must be on two disks"
                        .into(),
                ));
            }
        }
        if !allow_same_fs && src_fs.is_some() && fs_of(&d.mount) == src_fs {
            return Err(IngestError::Unsafe(format!(
                "destination {} is on the same disk as the source",
                d.mount.display()
            )));
        }
    }
    Ok(())
}

fn ext_of(name: &str) -> &str {
    name.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

fn stem_of(name: &str) -> &str {
    name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name)
}

/// Destination base name (no extension) for each member of a group, given
/// the original's new base. Companions keep their Sony suffix; files that
/// share a stem share the new name; AVCHD segments get `_02`, `_03`, ….
fn member_names(files: &[RelPath], g: &Group, dir_and_base: &RelPath) -> Vec<RelPath> {
    let orig_stem = stem_of(files[g.original()].file_name()).to_uppercase();
    let base = dir_and_base.file_name().to_owned();
    let parent = dir_and_base.parent();
    // New base per source stem (segments first, so their CPI files follow them).
    let mut by_stem: HashMap<String, String> = HashMap::new();
    by_stem.insert(orig_stem.clone(), base.clone());
    for m in &g.members {
        if let Some(seg) = m.segment.filter(|s| *s > 0) {
            let st = stem_of(files[m.index].file_name()).to_uppercase();
            by_stem.insert(st, format!("{base}_{:02}", seg + 1));
        }
    }
    g.members
        .iter()
        .map(|m| {
            let name = files[m.index].file_name();
            let st = stem_of(name);
            let st_u = st.to_uppercase();
            let new_base = if let Some(b) = by_stem.get(&st_u) {
                b.clone()
            } else if let Some(rest) = st_u.strip_prefix(&orig_stem) {
                // C0001M01 → <base>M01, keeping the original's spelling of the suffix.
                format!("{base}{}", &st[st.len() - rest.len()..])
            } else {
                format!("{base}_{st}")
            };
            let ext = ext_of(name);
            let file = if ext.is_empty() {
                new_base
            } else {
                format!("{new_base}.{ext}")
            };
            let full = match &parent {
                Some(p) => format!("{p}/{file}"),
                None => file,
            };
            RelPath::parse(&full).unwrap_or_else(|_| files[m.index].clone())
        })
        .collect()
}

fn append_suffix(base: &RelPath, n: u32) -> RelPath {
    if n <= 1 {
        return base.clone();
    }
    let s = format!("{} ({n})", base.as_str());
    RelPath::parse(&s).unwrap_or_else(|_| base.clone())
}

/// The planning computation shared by [`plan`] and [`preview`]: reads the
/// card and the catalog, writes nothing.
fn compute_plan(
    cat: &Catalog,
    prober: Option<&Prober>,
    src: &SourceVolume,
    src_rec: &VolumeRecord,
    dests: &[Destination],
    opts: &PlanOptions,
    session: ImportSessionId,
) -> Result<(Vec<ImportAsset>, Vec<ImportItem>), IngestError> {
    let (files, _) = crate::rebuild::list_media_files(src.root());
    let grouping = crate::rebuild::group_folder(src.root(), &files);
    let probed = prober.map(|p| p.probe(src.root(), &files, &grouping));

    struct Planned {
        source_key: String,
        asset_id: AssetId,
        diff: ImportDiff,
        local: Option<chrono::NaiveDateTime>,
        input: TemplateInput,
        group: usize,
        quick: Vec<(u64, u64)>, // (size, quick hash) per member
    }
    let mut planned: Vec<Planned> = Vec::new();
    for (gi, g) in grouping.groups.iter().enumerate() {
        let orig = &files[g.original()];
        let source_key = orig.key(src_rec.case);
        let mut quick = Vec::with_capacity(g.members.len());
        for m in &g.members {
            let rel = &files[m.index];
            let size = src.metadata(rel)?.len();
            quick.push((size, src.quick_hash(rel)?));
        }
        // Quick-diff: is the original already in the library (off this card)?
        let (osize, oquick) = quick[0];
        let known = cat.read(|tx| tx.files().by_quick(osize, oquick))?;
        let known = known.into_iter().find(|f| f.volume_id != src_rec.id);
        let (diff, asset_id) = match known {
            Some(f) => (ImportDiff::AlreadyImported, f.asset_id),
            None => {
                let earlier = cat.read(|tx| {
                    tx.imports()
                        .earlier_items(src_rec.id, orig.as_str(), session)
                })?;
                if earlier
                    .iter()
                    .any(|i| i.quick_hash != oquick || i.size_bytes != osize)
                {
                    (ImportDiff::Modified, AssetId::new_random())
                } else {
                    (ImportDiff::New, AssetId::new_random())
                }
            }
        };
        let pa = probed.as_ref().and_then(|r| r.assets.get(gi));
        let local = pa.and_then(|a| a.capture.as_ref()).map(|c| {
            let ms = c.time.utc_ms + i64::from(c.time.utc_offset_min.unwrap_or(0)) * 60_000;
            DateTime::from_timestamp_millis(ms)
                .map(|d| d.naive_utc())
                .unwrap_or_default()
        });
        planned.push(Planned {
            source_key,
            asset_id,
            diff,
            local,
            input: TemplateInput {
                local_time: local,
                event: opts.event.clone(),
                camera: pa.and_then(|a| a.model.clone()),
                lens: pa.and_then(|a| a.lens.clone()),
                orig: stem_of(orig.file_name()).to_owned(),
            },
            group: gi,
            quick,
        });
    }
    // Deterministic order: capture time, then source path.
    planned.sort_by(|a, b| {
        (a.local, files[grouping.groups[a.group].original()].as_str())
            .cmp(&(b.local, files[grouping.groups[b.group].original()].as_str()))
    });

    let selected = |p: &Planned| match opts.selection {
        Selection::All => true,
        Selection::NewOnly => p.diff != ImportDiff::AlreadyImported,
        Selection::Nothing => false,
    };

    // Numbering for {seq}: per rendered folder, in the order above.
    let mut seq_counter: HashMap<String, u32> = HashMap::new();
    let mut taken: HashSet<(VolumeId, String)> = HashSet::new();
    let mut assets_rows = Vec::new();
    let mut item_rows = Vec::new();
    for p in &planned {
        let g = &grouping.groups[p.group];
        let state = if selected(p) {
            ImportAssetState::Selected
        } else if opts.selection == Selection::Nothing {
            ImportAssetState::Discovered
        } else {
            ImportAssetState::Skipped
        };
        assets_rows.push(ImportAsset {
            session_id: session,
            source_key: p.source_key.clone(),
            asset_id: p.asset_id,
            diff: p.diff,
            state,
            error: None,
            attempts: 0,
            updated_ms: now(),
        });
        let seq = if opts.template.uses_seq() {
            let c = seq_counter
                .entry(opts.template.seq_scope(&p.input))
                .or_insert(0);
            *c += 1;
            Some(*c)
        } else {
            None
        };
        let base = opts.template.render(&p.input, seq)?;
        // Collisions: same names on every destination, free or identical.
        let mut n = 1u32;
        let names = loop {
            let names = member_names(&files, g, &append_suffix(&base, n));
            let ok = dests.iter().all(|d| {
                names.iter().zip(&p.quick).all(|(name, (size, quick))| {
                    let key = (d.volume.id, name.key(d.volume.case));
                    if taken.contains(&key) {
                        return false;
                    }
                    let path = name.to_fs_path(&d.mount);
                    match std::fs::metadata(&path) {
                        Err(_) => true,
                        // An identical file already there is reused (confirmed by BLAKE3 later).
                        Ok(m) => m.len() == *size && source::quick_hash(&path).ok() == Some(*quick),
                    }
                })
            });
            if ok || n > 9999 {
                break names;
            }
            n += 1;
        };
        for d in dests {
            for ((m, name), (size, quick)) in g.members.iter().zip(&names).zip(&p.quick) {
                taken.insert((d.volume.id, name.key(d.volume.case)));
                item_rows.push(ImportItem {
                    session_id: session,
                    source_rel_path: files[m.index].clone(),
                    dest_volume_id: d.volume.id,
                    source_key: p.source_key.clone(),
                    role: m.role,
                    segment_index: m.segment.filter(|s| *s > 0),
                    size_bytes: *size,
                    quick_hash: *quick,
                    dest_rel_path: name.clone(),
                    file_id: None,
                    blake3: None,
                    verified_blake3: None,
                    status: ImportItemStatus::Pending,
                    error: None,
                });
            }
        }
    }
    Ok((assets_rows, item_rows))
}

/// Plan an import: group and probe the card, quick-diff against the
/// library, name every file on every destination, and store the session.
pub fn plan(
    cat: &mut Catalog,
    lib: &Library,
    prober: Option<&Prober>,
    source_mount: &Path,
    source_label: &str,
    dests: &[Destination],
    opts: &PlanOptions,
) -> Result<ImportSessionId, IngestError> {
    let src = SourceVolume::new(source_mount)
        .map_err(|_| IngestError::SourceMissing(source_mount.to_path_buf()))?;
    check_destinations(&src, dests, opts.allow_same_filesystem)?;
    let src_rec = source_volume(lib, src.root(), source_label, opts)?;
    cat.write(|tx| -> Result<(), IngestError> {
        tx.volumes().upsert(&src_rec.to_volume(Some(src.root())))?;
        for d in dests {
            tx.volumes().upsert(&d.volume.to_volume(Some(&d.mount)))?;
        }
        Ok(())
    })?;

    let session = ImportSessionId::new_random();
    let (assets_rows, item_rows) = compute_plan(cat, prober, &src, &src_rec, dests, opts, session)?;

    cat.write(|tx| -> Result<(), IngestError> {
        tx.imports().upsert_session(&ImportSession {
            id: session,
            source_volume_id: src_rec.id,
            started_ms: now(),
            finished_ms: None,
            status: ImportStatus::Running,
            verified: false,
            template: Some(opts.template.to_string()),
            safe_to_format: false,
        })?;
        for d in dests {
            tx.imports().upsert_destination(&ImportDestination {
                session_id: session,
                volume_id: d.volume.id,
                verified: false,
            })?;
        }
        for a in &assets_rows {
            tx.imports().upsert_asset(a)?;
        }
        for i in &item_rows {
            tx.imports().upsert_item(i)?;
        }
        Ok(())
    })?;
    Ok(session)
}

/// A destination described for [`preview`] without registering it or
/// touching the disk. Uses the library's record if the folder is known.
pub fn preview_destination(
    lib: &Library,
    mount: &Path,
    label: &str,
) -> Result<Destination, IngestError> {
    let mount = match std::fs::canonicalize(mount) {
        Ok(p) => p,
        Err(_) if mount.is_absolute() => mount.to_path_buf(),
        Err(_) => std::env::current_dir()?.join(mount),
    };
    let fs_id = mount.display().to_string();
    let volume = match lib.volumes()?.into_iter().find(|v| v.fs_id == fs_id) {
        Some(v) => v,
        None => VolumeRecord {
            id: VolumeId::new_random(),
            label: label.to_owned(),
            fs_id,
            kind: VolumeKind::External,
            // Windows and macOS file systems are case-insensitive by default.
            case: CaseSensitivity::Insensitive,
            read_only: false,
            last_mount: Some(mount.clone()),
        },
    };
    Ok(Destination { volume, mount })
}

/// One file of a [`Preview`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewFile {
    pub source: String,
    pub dest: String,
    pub role: FileRole,
    pub size: u64,
}

/// One asset of a [`Preview`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewAsset {
    pub original: String,
    pub diff: ImportDiff,
    pub state: ImportAssetState,
    pub files: Vec<PreviewFile>,
}

/// Space needed on one destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewSpace {
    pub destination: String,
    pub mount: PathBuf,
    pub bytes_needed: u64,
    pub bytes_free: Option<u64>,
}

/// What an import would do. Producing it writes nothing anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub assets: Vec<PreviewAsset>,
    pub space: Vec<PreviewSpace>,
}

/// Plan an import without storing anything: no session, no volume
/// registration, no folders created. Destinations must come from
/// [`preview_destination`] (or be already registered).
pub fn preview(
    cat: &Catalog,
    lib: &Library,
    prober: Option<&Prober>,
    source_mount: &Path,
    dests: &[Destination],
    opts: &PlanOptions,
) -> Result<Preview, IngestError> {
    let src = SourceVolume::new(source_mount)
        .map_err(|_| IngestError::SourceMissing(source_mount.to_path_buf()))?;
    check_destinations(&src, dests, opts.allow_same_filesystem)?;
    let fs_id = source_fs_id(src.root(), opts);
    let src_rec = match lib.volumes()?.into_iter().find(|v| v.fs_id == fs_id) {
        Some(v) => v,
        None => VolumeRecord {
            id: VolumeId::new_random(),
            label: "card".into(),
            fs_id,
            kind: VolumeKind::Card,
            case: CaseSensitivity::Insensitive,
            read_only: true,
            last_mount: Some(src.root().to_path_buf()),
        },
    };
    let (assets, items) = compute_plan(
        cat,
        prober,
        &src,
        &src_rec,
        dests,
        opts,
        ImportSessionId::new_random(),
    )?;
    let first = dests.first().map(|d| d.volume.id);
    let mut out = Vec::new();
    for a in &assets {
        let mine: Vec<&ImportItem> = items
            .iter()
            .filter(|i| i.source_key == a.source_key && Some(i.dest_volume_id) == first)
            .collect();
        let original = mine
            .iter()
            .find(|i| i.role == FileRole::Original)
            .map(|i| i.source_rel_path.to_string())
            .unwrap_or_else(|| a.source_key.clone());
        out.push(PreviewAsset {
            original,
            diff: a.diff,
            state: a.state,
            files: mine
                .iter()
                .map(|i| PreviewFile {
                    source: i.source_rel_path.to_string(),
                    dest: i.dest_rel_path.to_string(),
                    role: i.role,
                    size: i.size_bytes,
                })
                .collect(),
        });
    }
    out.sort_by(|x, y| x.original.cmp(&y.original));
    let selected: HashSet<&str> = assets
        .iter()
        .filter(|a| a.state == ImportAssetState::Selected)
        .map(|a| a.source_key.as_str())
        .collect();
    let space = dests
        .iter()
        .map(|d| PreviewSpace {
            destination: d.volume.label.clone(),
            mount: d.mount.clone(),
            bytes_needed: items
                .iter()
                .filter(|i| {
                    i.dest_volume_id == d.volume.id && selected.contains(i.source_key.as_str())
                })
                .map(|i| i.size_bytes)
                .sum(),
            bytes_free: sys::existing_ancestor(&d.mount).and_then(|p| sys::free_space(&p)),
        })
        .collect();
    Ok(Preview { assets: out, space })
}

/// Select or deselect assets of a planned session (by source original key).
pub fn select(
    cat: &mut Catalog,
    session: ImportSessionId,
    source_keys: &[String],
    selected: bool,
) -> Result<(), IngestError> {
    cat.write(|tx| -> Result<(), IngestError> {
        for k in source_keys {
            let Some(a) = tx.imports().asset(session, k)? else {
                continue;
            };
            let movable = matches!(
                a.state,
                ImportAssetState::Discovered
                    | ImportAssetState::Selected
                    | ImportAssetState::Skipped
            );
            if movable {
                let to = if selected {
                    ImportAssetState::Selected
                } else {
                    ImportAssetState::Skipped
                };
                tx.imports().set_asset_state(session, k, to, None, now())?;
            }
        }
        Ok(())
    })
}

/// Everything [`run`] needs besides the catalog and library.
pub struct RunContext<'a> {
    pub prober: Option<&'a Prober>,
    pub fault: &'a FaultPlan,
    pub cancel: &'a AtomicBool,
    pub progress: &'a mut dyn FnMut(&Progress),
    /// Retry assets that failed in an earlier run.
    pub retry_failed: bool,
}

struct Mounts {
    src: SourceVolume,
    dests: Vec<(VolumeId, PathBuf, CaseSensitivity)>,
}

impl Mounts {
    fn dest(&self, id: VolumeId) -> Option<(VolumeId, &Path)> {
        self.dests
            .iter()
            .find(|d| d.0 == id)
            .map(|d| (d.0, d.1.as_path()))
    }
}

fn set_state(
    cat: &mut Catalog,
    ctx: &mut RunContext<'_>,
    session: ImportSessionId,
    key: &str,
    state: ImportAssetState,
    error: Option<&str>,
) -> Result<(), IngestError> {
    cat.write(|tx| {
        tx.imports()
            .set_asset_state(session, key, state, error, now())
    })?;
    (ctx.progress)(&Progress::AssetState {
        asset: key.to_owned(),
        state,
    });
    Ok(())
}

fn set_item(
    cat: &mut Catalog,
    session: ImportSessionId,
    it: &ImportItem,
    status: ImportItemStatus,
    blake3: Option<&Blake3Hash>,
    verified: Option<&Blake3Hash>,
    error: Option<&str>,
) -> Result<(), IngestError> {
    cat.write(|tx| {
        tx.imports().set_item(
            session,
            it.source_rel_path.as_str(),
            it.dest_volume_id,
            status,
            blake3,
            verified,
            error,
        )
    })?;
    Ok(())
}

fn hash_source(src: &SourceVolume, rel: &RelPath) -> io::Result<Blake3Hash> {
    let mut f = src.open(rel)?;
    let mut h = blake3::Hasher::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(Blake3Hash(*h.finalize().as_bytes()))
}

/// Bring items left behind by an interrupted run into a known state.
fn recover_items(
    cat: &mut Catalog,
    session: ImportSessionId,
    m: &Mounts,
    items: &[ImportItem],
    retry_failed: bool,
) -> Result<(), IngestError> {
    let tag = session_tag(session);
    for it in items {
        let Some((_, mount)) = m.dest(it.dest_volume_id) else {
            continue;
        };
        let fin = it.dest_rel_path.to_fs_path(mount);
        let tmp = copy::temp_path(&fin, &tag);
        match it.status {
            ImportItemStatus::Verified | ImportItemStatus::Reused | ImportItemStatus::Skipped => {}
            ImportItemStatus::Failed if !retry_failed => {
                copy::remove_if_exists(&tmp)?;
            }
            ImportItemStatus::Copied => {
                copy::remove_if_exists(&tmp)?;
                if !fin.exists() {
                    set_item(
                        cat,
                        session,
                        it,
                        ImportItemStatus::Pending,
                        None,
                        None,
                        None,
                    )?;
                }
            }
            _ => {
                // pending / copying / failed (retrying): no temporary survives a restart.
                copy::remove_if_exists(&tmp)?;
                if !fin.exists() {
                    if it.status != ImportItemStatus::Pending {
                        set_item(
                            cat,
                            session,
                            it,
                            ImportItemStatus::Pending,
                            None,
                            None,
                            None,
                        )?;
                    }
                    continue;
                }
                // A final file is already there.
                let want = match it.blake3 {
                    // We streamed it, renamed it, and died before recording "copied".
                    Some(h) => h,
                    // Not ours (or a planned reuse): it must equal the source to count.
                    None => match hash_source(&m.src, &it.source_rel_path) {
                        Ok(h) => h,
                        Err(_) => continue, // card missing: decide on the next run
                    },
                };
                let got = copy::rehash(&fin, false)?;
                if got == want {
                    let status = if it.blake3.is_some() {
                        ImportItemStatus::Copied
                    } else {
                        ImportItemStatus::Reused
                    };
                    set_item(cat, session, it, status, Some(&want), Some(&got), None)?;
                } else if it.blake3.is_some() {
                    // Our own rename of a bad file: move it out of the way and copy again.
                    std::fs::rename(&fin, copy::bad_path(&fin, &tag))?;
                    set_item(
                        cat,
                        session,
                        it,
                        ImportItemStatus::Pending,
                        None,
                        None,
                        None,
                    )?;
                } else {
                    set_item(
                        cat,
                        session,
                        it,
                        ImportItemStatus::Failed,
                        None,
                        None,
                        Some(&format!(
                            "a different file already exists at {}",
                            fin.display()
                        )),
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Copy (and verify) the files of one asset. Returns the first error, if any.
fn copy_asset(
    cat: &mut Catalog,
    ctx: &mut RunContext<'_>,
    session: ImportSessionId,
    m: &Mounts,
    key: &str,
    bytes: &mut u64,
) -> Result<Option<String>, IngestError> {
    let tag = session_tag(session);
    let items = cat.read(|tx| tx.imports().asset_items(session, key))?;
    let mut by_file: BTreeMap<String, Vec<ImportItem>> = BTreeMap::new();
    for it in items {
        by_file
            .entry(it.source_rel_path.as_str().to_owned())
            .or_default()
            .push(it);
    }
    let mut first_error: Option<String> = None;
    for (file, its) in &by_file {
        // Ctrl-C finishes the file in flight, then stops before the next one.
        if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(IngestError::Cancelled);
        }
        let mut todo: Vec<&ImportItem> = its
            .iter()
            .filter(|i| {
                matches!(
                    i.status,
                    ImportItemStatus::Pending
                        | ImportItemStatus::Failed
                        | ImportItemStatus::Copying
                )
            })
            .collect();
        if todo.is_empty() {
            continue;
        }
        let rel = &todo[0].source_rel_path;
        let size = todo[0].size_bytes;
        (ctx.progress)(&Progress::FileStarted {
            file: file.clone(),
            bytes: size,
        });
        for attempt in 0..2 {
            let targets: Vec<(VolumeId, PathBuf, PathBuf)> = todo
                .iter()
                .filter_map(|it| {
                    let (slot, mount) = m.dest(it.dest_volume_id)?;
                    let fin = it.dest_rel_path.to_fs_path(mount);
                    Some((slot, copy::temp_path(&fin, &tag), fin))
                })
                .collect();
            for (_, tmp, _) in &targets {
                copy::remove_if_exists(tmp)?;
            }
            for it in &todo {
                set_item(
                    cat,
                    session,
                    it,
                    ImportItemStatus::Copying,
                    None,
                    None,
                    None,
                )?;
            }
            let temps: Vec<PathBuf> = targets.iter().map(|t| t.1.clone()).collect();
            let fault = ctx.fault;
            let progress = &mut ctx.progress;
            let label = file.clone();
            let mut on_bytes = |done: u64| {
                progress(&Progress::FileBytes {
                    file: label.clone(),
                    done,
                    total: size,
                })
            };
            let slots: Vec<VolumeId> = targets.iter().map(|t| t.0).collect();
            let teed = copy::tee(&m.src, rel, size, &temps, &slots, fault, &mut on_bytes);
            let teed = match teed {
                Ok(t) => t,
                Err(copy::SourceStop::Crash) => return Err(IngestError::SimulatedCrash),
                Err(stop) => {
                    for t in &temps {
                        copy::remove_if_exists(t)?;
                    }
                    let msg = match stop {
                        copy::SourceStop::Read(e) => format!("reading {file}: {e}"),
                        copy::SourceStop::Changed(why) => format!("{file}: {why}"),
                        copy::SourceStop::Crash => unreachable_crash(),
                    };
                    for it in &todo {
                        set_item(
                            cat,
                            session,
                            it,
                            ImportItemStatus::Failed,
                            None,
                            None,
                            Some(&msg),
                        )?;
                    }
                    if !m.src.is_present() {
                        return Err(IngestError::SourceMissing(m.src.root().to_path_buf()));
                    }
                    return Ok(Some(msg));
                }
            };
            *bytes += teed.len;
            let mut retry: Vec<&ImportItem> = Vec::new();
            for ((it, (slot, tmp, fin)), write) in todo.iter().zip(&targets).zip(&teed.writes) {
                if let Err(e) = write {
                    copy::remove_if_exists(tmp)?;
                    let msg = format!("writing {}: {e}", fin.display());
                    set_item(
                        cat,
                        session,
                        it,
                        ImportItemStatus::Failed,
                        None,
                        None,
                        Some(&msg),
                    )?;
                    first_error.get_or_insert(msg);
                    continue;
                }
                // Record the stream hash before the rename, so a crash after
                // the rename can be checked on resume.
                set_item(
                    cat,
                    session,
                    it,
                    ImportItemStatus::Copying,
                    Some(&teed.hash),
                    None,
                    None,
                )?;
                let got = copy::rehash(tmp, ctx.fault.corrupt_verify(*slot, file))?;
                if got != teed.hash {
                    copy::remove_if_exists(tmp)?;
                    if attempt == 0 {
                        retry.push(it);
                    } else {
                        let msg = format!("{}: read-back did not match (twice)", fin.display());
                        set_item(
                            cat,
                            session,
                            it,
                            ImportItemStatus::Failed,
                            None,
                            None,
                            Some(&msg),
                        )?;
                        first_error.get_or_insert(msg);
                    }
                    continue;
                }
                if let Err(e) = copy::finalize(tmp, fin, *slot, file, ctx.fault) {
                    copy::remove_if_exists(tmp)?;
                    let msg = format!("renaming to {}: {e}", fin.display());
                    set_item(
                        cat,
                        session,
                        it,
                        ImportItemStatus::Failed,
                        None,
                        None,
                        Some(&msg),
                    )?;
                    first_error.get_or_insert(msg);
                    continue;
                }
                if ctx
                    .fault
                    .crash(&CrashPoint::AfterRename { file: file.clone() })
                {
                    return Err(IngestError::SimulatedCrash);
                }
                set_item(
                    cat,
                    session,
                    it,
                    ImportItemStatus::Copied,
                    Some(&teed.hash),
                    Some(&got),
                    None,
                )?;
            }
            if retry.is_empty() {
                break;
            }
            todo = retry;
        }
        (ctx.progress)(&Progress::FileDone { file: file.clone() });
    }
    Ok(first_error)
}

#[cold]
fn unreachable_crash() -> String {
    "crash".into()
}

/// Re-read every copied final file of an asset and compare with the stream hash.
fn verify_asset(
    cat: &mut Catalog,
    ctx: &mut RunContext<'_>,
    session: ImportSessionId,
    m: &Mounts,
    key: &str,
) -> Result<Option<String>, IngestError> {
    let tag = session_tag(session);
    let items = cat.read(|tx| tx.imports().asset_items(session, key))?;
    let mut first_error = None;
    for it in items
        .iter()
        .filter(|i| i.status == ImportItemStatus::Copied)
    {
        let Some((slot, mount)) = m.dest(it.dest_volume_id) else {
            continue;
        };
        let fin = it.dest_rel_path.to_fs_path(mount);
        let Some(want) = it.blake3 else { continue };
        let got = copy::rehash(
            &fin,
            ctx.fault.corrupt_final(slot, it.source_rel_path.as_str()),
        );
        match got {
            Ok(h) if h == want => {
                set_item(
                    cat,
                    session,
                    it,
                    ImportItemStatus::Verified,
                    Some(&want),
                    Some(&h),
                    None,
                )?;
            }
            other => {
                let why = match other {
                    Ok(_) => "content differs from the card".to_owned(),
                    Err(e) => e.to_string(),
                };
                // Never leave a file that failed verification under its final name.
                if fin.exists() {
                    std::fs::rename(&fin, copy::bad_path(&fin, &tag))?;
                }
                let msg = format!("verifying {}: {why}", fin.display());
                set_item(
                    cat,
                    session,
                    it,
                    ImportItemStatus::Failed,
                    None,
                    None,
                    Some(&msg),
                )?;
                first_error.get_or_insert(msg);
            }
        }
    }
    let _ = ctx;
    Ok(first_error)
}

fn group_kind(items: &[&ImportItem]) -> GroupKind {
    let orig = items.iter().find(|i| i.role == FileRole::Original);
    let video = orig.is_some_and(|o| {
        matches!(
            ext_of(o.dest_rel_path.file_name())
                .to_ascii_lowercase()
                .as_str(),
            "mp4" | "mts" | "m2ts" | "mov" | "avi" | "mkv"
        )
    });
    if items.iter().any(|i| i.role == FileRole::Segment)
        || orig.is_some_and(|o| ext_of(o.dest_rel_path.file_name()).eq_ignore_ascii_case("mts"))
    {
        GroupKind::Avchd
    } else if video
        && items.iter().any(|i| {
            i.role == FileRole::Sidecar
                && ext_of(i.dest_rel_path.file_name()).eq_ignore_ascii_case("xml")
        })
    {
        GroupKind::Xavc
    } else if video {
        GroupKind::Video
    } else {
        GroupKind::Still
    }
}

/// Catalogue every verified asset: probe the first destination's copies,
/// insert assets and files on all destination volumes, write sidecars, and
/// record sequences.
fn catalogue(
    cat: &mut Catalog,
    lib: &Library,
    ctx: &mut RunContext<'_>,
    session: ImportSessionId,
    m: &Mounts,
) -> Result<(), IngestError> {
    let assets: Vec<ImportAsset> = cat
        .read(|tx| tx.imports().assets(session))?
        .into_iter()
        .filter(|a| a.state == ImportAssetState::Verified)
        .collect();
    if assets.is_empty() {
        return Ok(());
    }
    (ctx.progress)(&Progress::Phase {
        name: "catalogue".into(),
    });
    let Some((first_vol, first_mount, _)) = m.dests.first().cloned() else {
        return Ok(());
    };
    // Build a grouping over the first destination's copies for probing.
    let mut rels: Vec<RelPath> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut per_asset: Vec<Vec<ImportItem>> = Vec::new();
    for a in &assets {
        let items = cat.read(|tx| tx.imports().asset_items(session, &a.source_key))?;
        let mine: Vec<&ImportItem> = items
            .iter()
            .filter(|i| i.dest_volume_id == first_vol)
            .collect();
        let mut members = Vec::new();
        let mut ordered = mine.clone();
        ordered.sort_by_key(|i| {
            (
                i.role != FileRole::Original,
                i.segment_index,
                i.dest_rel_path.clone(),
            )
        });
        for i in &ordered {
            members.push(Member {
                index: rels.len(),
                role: i.role,
                segment: match i.role {
                    FileRole::Original if group_kind(&mine) == GroupKind::Avchd => Some(0),
                    _ => i.segment_index,
                },
            });
            rels.push(i.dest_rel_path.clone());
        }
        groups.push(Group {
            kind: group_kind(&mine),
            members,
        });
        per_asset.push(items);
    }
    let grouping = Grouping {
        groups,
        ..Default::default()
    };
    let report: ProbeReport = match ctx.prober {
        Some(p) => p.probe(&first_mount, &rels, &grouping),
        None => ProbeReport::default(),
    };

    let mut catalogued: Vec<usize> = Vec::new();
    for (ai, a) in assets.iter().enumerate() {
        let items = &per_asset[ai];
        let label = items
            .iter()
            .find(|i| i.role == FileRole::Original)
            .map(|i| i.source_rel_path.as_str().to_owned())
            .unwrap_or_default();
        if ctx.fault.crash(&CrashPoint::AfterVerify {
            file: label.clone(),
        }) {
            return Err(IngestError::SimulatedCrash);
        }
        let probed = report.assets.get(ai);
        let kind = grouping.groups[ai].media_kind();
        let result = cat.write(|tx| -> Result<(), IngestError> {
            if ctx.fault.catalog(&label) {
                return Err(IngestError::Io(io::Error::other(
                    "injected catalog failure",
                )));
            }
            catalogue_one(tx, lib, a, items, kind, probed)?;
            tx.imports().set_asset_state(
                session,
                &a.source_key,
                ImportAssetState::Catalogued,
                None,
                now(),
            )?;
            Ok(())
        });
        match result {
            Ok(()) => {
                catalogued.push(ai);
                (ctx.progress)(&Progress::AssetState {
                    asset: a.source_key.clone(),
                    state: ImportAssetState::Catalogued,
                });
            }
            Err(IngestError::SimulatedCrash) => return Err(IngestError::SimulatedCrash),
            Err(e) => {
                // Files stay verified on disk; the next run retries cataloguing.
                let msg = format!("cataloguing: {e}");
                cat.write(|tx| {
                    tx.imports().set_asset_state(
                        session,
                        &a.source_key,
                        ImportAssetState::Verified,
                        Some(&msg),
                        now(),
                    )
                })?;
            }
        }
    }

    // Sequences among the newly catalogued stills.
    let done: HashSet<usize> = catalogued.iter().copied().collect();
    cat.write(|tx| -> Result<(), IngestError> {
        for s in &report.sequences {
            if !s.members.iter().all(|gi| done.contains(gi)) {
                continue;
            }
            let Some(first) = s.members.first() else {
                continue;
            };
            let first_rel = &rels[grouping.groups[*first].original()];
            let first_case = m.dests[0].2;
            let id = SequenceId::for_first(first_vol, &first_rel.key(first_case));
            let members: Vec<AssetId> = s.members.iter().map(|gi| assets[*gi].asset_id).collect();
            tx.sequences().upsert(&StillSequence {
                id,
                volume_id: first_vol,
                kind: s.kind,
                members: members.clone(),
            })?;
            for a in members {
                crate::metadata::sync_sidecars(tx, lib, a)?;
            }
        }
        Ok(())
    })?;
    Ok(())
}

fn catalogue_one(
    tx: &WriteTx<'_>,
    lib: &Library,
    a: &ImportAsset,
    items: &[ImportItem],
    kind: MediaKind,
    probed: Option<&crate::probe::ProbedAsset>,
) -> Result<(), IngestError> {
    let existing = tx.assets().get(a.asset_id)?;
    if existing.is_none() {
        let mut asset = Asset::new(a.asset_id, kind, now());
        if let Some(p) = probed {
            if let Some(c) = &p.capture {
                asset.capture = Some(c.time);
                asset.capture_source = Some(c.source);
                asset.capture_confidence = Some(c.confidence);
            }
            asset.camera_make = p.make.clone();
            asset.camera_model = p.model.clone();
            asset.lens_model = p.lens.clone();
            asset.duration_ms = p.duration_ms;
            asset.gps = p.gps;
        }
        tx.assets().insert(&asset)?;
    }
    for it in items.iter().filter(|i| {
        matches!(
            i.status,
            ImportItemStatus::Verified | ImportItemStatus::Reused
        )
    }) {
        let hash = it.verified_blake3.or(it.blake3).ok_or_else(|| {
            IngestError::Unsafe(format!("{} has no verified hash", it.dest_rel_path))
        })?;
        let file = match tx
            .files()
            .by_location(it.dest_volume_id, &it.dest_rel_path)?
        {
            Some(f) if f.asset_id == a.asset_id => f,
            Some(f) => {
                return Err(IngestError::Unsafe(format!(
                    "{} already belongs to another asset ({})",
                    it.dest_rel_path, f.asset_id
                )))
            }
            None => tx.files().insert(
                a.asset_id,
                &NewFile {
                    volume_id: it.dest_volume_id,
                    rel_path: it.dest_rel_path.clone(),
                    role: it.role,
                    size_bytes: it.size_bytes,
                    blake3: hash,
                    xxh3: Some(it.quick_hash),
                    mtime_ms: None,
                    segment_index: it.segment_index,
                },
            )?,
        };
        tx.imports().set_item_file(
            it.session_id,
            it.source_rel_path.as_str(),
            it.dest_volume_id,
            file.id,
        )?;
    }
    crate::metadata::sync_sidecars(tx, lib, a.asset_id)?;
    Ok(())
}

/// For assets already in the library: hash the card's files fully and check
/// each exists (same BLAKE3) on two different volumes other than the card.
fn confirm_already_imported(
    cat: &mut Catalog,
    ctx: &mut RunContext<'_>,
    session: ImportSessionId,
    m: &Mounts,
    source_volume: VolumeId,
) -> Result<(), IngestError> {
    let assets = cat.read(|tx| tx.imports().assets(session))?;
    for a in assets
        .iter()
        .filter(|a| a.diff == ImportDiff::AlreadyImported && a.state == ImportAssetState::Skipped)
    {
        // Member files are known from the original's existing catalog asset.
        let existing = cat.read(|tx| tx.files().for_asset(a.asset_id))?;
        let items = cat.read(|tx| tx.imports().asset_items(session, &a.source_key))?;
        let mut sources: Vec<RelPath> = items.iter().map(|i| i.source_rel_path.clone()).collect();
        sources.sort();
        sources.dedup();
        let mut worst = usize::MAX;
        for rel in &sources {
            if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(IngestError::Cancelled);
            }
            let Ok(h) = hash_source(&m.src, rel) else {
                worst = 0;
                break;
            };
            let vols: HashSet<VolumeId> = existing
                .iter()
                .filter(|f| f.blake3 == h && f.volume_id != source_volume)
                .map(|f| f.volume_id)
                .collect();
            worst = worst.min(vols.len());
        }
        let (state, err) = if worst >= 2 && worst != usize::MAX {
            (ImportAssetState::Catalogued, None)
        } else {
            (
                ImportAssetState::Skipped,
                Some(format!(
                    "already in the library, but only on {} volume(s)",
                    if worst == usize::MAX { 0 } else { worst }
                )),
            )
        };
        cat.write(|tx| {
            tx.imports()
                .set_asset_state(session, &a.source_key, state, err.as_deref(), now())
        })?;
    }
    Ok(())
}

/// Run (or resume) a planned import session.
/// Copy, verify and catalogue a planned session (also resumes one).
///
/// While it runs, an [`crate::jobs::IngestHeartbeat`] pauses render
/// workers. Whatever the outcome, assets it catalogued are queued for
/// renditions as recent imports.
pub fn run(
    cat: &mut Catalog,
    lib: &Library,
    session: ImportSessionId,
    ctx: &mut RunContext<'_>,
) -> Result<IngestReport, IngestError> {
    let heartbeat = crate::jobs::IngestHeartbeat::start(lib);
    let result = run_session(cat, lib, session, ctx);
    drop(heartbeat);
    if let Err(e) = queue_renders(cat, session) {
        tracing::warn!("could not queue renditions for session {session}: {e}");
    }
    result
}

/// Queue renditions of the session's catalogued assets (recent imports).
fn queue_renders(cat: &mut Catalog, session: ImportSessionId) -> Result<u64, IngestError> {
    let ids: Vec<AssetId> = cat
        .read(|tx| tx.imports().assets(session))?
        .into_iter()
        .filter(|a| a.state == ImportAssetState::Catalogued)
        .map(|a| a.asset_id)
        .collect();
    Ok(crate::render::enqueue(
        cat,
        &ids,
        crate::catalog::Priority::Recent,
        false,
    )?)
}

fn run_session(
    cat: &mut Catalog,
    lib: &Library,
    session: ImportSessionId,
    ctx: &mut RunContext<'_>,
) -> Result<IngestReport, IngestError> {
    let s = cat
        .read(|tx| tx.imports().get(session))?
        .ok_or(IngestError::UnknownSession(session))?;
    let src_vol = lib.volume(s.source_volume_id)?;
    let src_mount = src_vol
        .last_mount
        .clone()
        .ok_or_else(|| IngestError::SourceMissing(PathBuf::new()))?;
    let src =
        SourceVolume::new(&src_mount).map_err(|_| IngestError::SourceMissing(src_mount.clone()))?;
    let mut dests = Vec::new();
    for d in cat.read(|tx| tx.imports().destinations(session))? {
        let rec = lib.volume(d.volume_id)?;
        let mount = rec
            .last_mount
            .clone()
            .filter(|p| p.is_dir())
            .ok_or_else(|| IngestError::DestinationMissing(rec.label.clone()))?;
        dests.push((rec.id, mount, rec.case));
    }
    let m = Mounts { src, dests };
    cat.write(|tx| {
        tx.imports()
            .set_session_status(session, ImportStatus::Running, false, None)
    })?;

    // 1. Recover from any interruption.
    let items = cat.read(|tx| tx.imports().items(session))?;
    recover_items(cat, session, &m, &items, ctx.retry_failed)?;

    // 2. Space check for what is left to copy.
    let items = cat.read(|tx| tx.imports().items(session))?;
    let active: HashSet<String> = cat
        .read(|tx| tx.imports().assets(session))?
        .into_iter()
        .filter(|a| {
            matches!(
                a.state,
                ImportAssetState::Selected | ImportAssetState::Copying | ImportAssetState::Copied
            ) || (ctx.retry_failed && a.state == ImportAssetState::Failed)
        })
        .map(|a| a.source_key)
        .collect();
    for (vol, mount, _) in &m.dests {
        let need: u64 = items
            .iter()
            .filter(|i| i.dest_volume_id == *vol && active.contains(&i.source_key))
            .filter(|i| {
                matches!(
                    i.status,
                    ImportItemStatus::Pending
                        | ImportItemStatus::Copying
                        | ImportItemStatus::Failed
                )
            })
            .map(|i| i.size_bytes)
            .sum();
        if need == 0 {
            continue;
        }
        let margin = need / 20 + (16 << 20);
        if let Some(free) = sys::free_space(mount) {
            if free < need + margin {
                return Err(IngestError::NoSpace {
                    dest: mount.display().to_string(),
                    need: need + margin,
                    free,
                });
            }
        }
    }

    // 3. Copy and verify, asset by asset.
    let mut bytes = 0u64;
    let assets = cat.read(|tx| tx.imports().assets(session))?;
    for a in assets.iter().filter(|a| active.contains(&a.source_key)) {
        if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let key = a.source_key.as_str();
        if a.state != ImportAssetState::Copied {
            set_state(cat, ctx, session, key, ImportAssetState::Copying, None)?;
            match copy_asset(cat, ctx, session, &m, key, &mut bytes) {
                Ok(None) => set_state(cat, ctx, session, key, ImportAssetState::Copied, None)?,
                Ok(Some(err)) => {
                    set_state(cat, ctx, session, key, ImportAssetState::Failed, Some(&err))?;
                    continue;
                }
                Err(IngestError::Cancelled) => {
                    set_state(cat, ctx, session, key, ImportAssetState::Selected, None)?;
                    break;
                }
                Err(IngestError::SourceMissing(p)) => {
                    set_state(
                        cat,
                        ctx,
                        session,
                        key,
                        ImportAssetState::Failed,
                        Some("the card was removed"),
                    )?;
                    cat.write(|tx| {
                        tx.imports()
                            .set_session_status(session, ImportStatus::Failed, false, None)
                    })?;
                    return Err(IngestError::SourceMissing(p));
                }
                Err(e) => return Err(e),
            }
        }
        match verify_asset(cat, ctx, session, &m, key)? {
            None => set_state(cat, ctx, session, key, ImportAssetState::Verified, None)?,
            Some(err) => set_state(cat, ctx, session, key, ImportAssetState::Failed, Some(&err))?,
        }
    }
    let cancelled = ctx.cancel.load(std::sync::atomic::Ordering::Relaxed);

    // 4. Catalogue, 5. confirm already-imported assets.
    if !cancelled {
        catalogue(cat, lib, ctx, session, &m)?;
        confirm_already_imported(cat, ctx, session, &m, s.source_volume_id)?;
    }

    // 6. Safe to format?
    let mut rep = report(cat, session)?;
    rep.bytes_copied = bytes;
    let status = if cancelled {
        ImportStatus::Cancelled
    } else if rep.failed.is_empty() && !rep.by_state.contains_key("verified") {
        ImportStatus::Completed
    } else {
        ImportStatus::Failed
    };
    let finished = (status == ImportStatus::Completed).then(now);
    cat.write(|tx| {
        tx.imports()
            .set_session_status(session, status, rep.safe_to_format, finished)
    })?;
    if cancelled {
        return Err(IngestError::Cancelled);
    }
    Ok(rep)
}

/// Compute the report (and "safe to format") from the catalog alone.
pub fn report(cat: &Catalog, session: ImportSessionId) -> Result<IngestReport, IngestError> {
    let s = cat
        .read(|tx| tx.imports().get(session))?
        .ok_or(IngestError::UnknownSession(session))?;
    let assets = cat.read(|tx| tx.imports().assets(session))?;
    let items = cat.read(|tx| tx.imports().items(session))?;
    let dest_count = cat.read(|tx| tx.imports().destinations(session))?.len();
    let mut rep = IngestReport {
        session: Some(session),
        assets: assets.len() as u64,
        ..Default::default()
    };
    let orig_of = |key: &str| {
        items
            .iter()
            .find(|i| i.source_key == key && i.role == FileRole::Original)
            .map(|i| i.source_rel_path.to_string())
            .unwrap_or_else(|| key.to_owned())
    };
    for a in &assets {
        *rep.by_state.entry(a.state.as_str().to_owned()).or_default() += 1;
        if a.state == ImportAssetState::Failed {
            rep.failed
                .push((orig_of(&a.source_key), a.error.clone().unwrap_or_default()));
        }
    }
    let mut not_safe = Vec::new();
    for a in &assets {
        let name = orig_of(&a.source_key);
        let unselected_new = a.diff != ImportDiff::AlreadyImported
            && matches!(
                a.state,
                ImportAssetState::Skipped | ImportAssetState::Discovered
            );
        if unselected_new {
            rep.unselected += 1;
            continue;
        }
        if a.state != ImportAssetState::Catalogued {
            not_safe.push(format!(
                "{name}: {}{}",
                a.state,
                a.error
                    .as_deref()
                    .map(|e| format!(" ({e})"))
                    .unwrap_or_default()
            ));
            continue;
        }
        if a.diff == ImportDiff::AlreadyImported
            && !items.iter().any(|i| {
                i.source_key == a.source_key
                    && matches!(
                        i.status,
                        ImportItemStatus::Verified | ImportItemStatus::Reused
                    )
            })
        {
            // Confirmed by full BLAKE3 against two library volumes.
            continue;
        }
        let mut files: BTreeMap<&str, HashSet<VolumeId>> = BTreeMap::new();
        for i in items.iter().filter(|i| i.source_key == a.source_key) {
            let e = files.entry(i.source_rel_path.as_str()).or_default();
            if matches!(
                i.status,
                ImportItemStatus::Verified | ImportItemStatus::Reused
            ) {
                e.insert(i.dest_volume_id);
            }
        }
        for (f, vols) in files {
            if vols.len() < 2 {
                not_safe.push(format!("{f}: verified on {} destination(s)", vols.len()));
            }
        }
    }
    if dest_count < 2 && rep.assets > rep.unselected {
        not_safe.insert(0, "only one destination was chosen".into());
    }
    if rep.assets == 0 {
        not_safe.push("the card has no media".into());
    }
    rep.safe_to_format = not_safe.is_empty();
    rep.not_safe = not_safe;
    let _ = s;
    Ok(rep)
}

/// Hidden temporary or quarantined files Keepsake left in a folder tree.
pub fn leftovers(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(".keepsake-partial-"))
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Keys of a session's assets, for [`select`].
pub fn asset_keys(cat: &Catalog, session: ImportSessionId) -> Result<Vec<String>, IngestError> {
    Ok(cat
        .read(|tx| tx.imports().assets(session))?
        .into_iter()
        .map(|a| a.source_key)
        .collect())
}

/// Normalise a key the way plans do (for callers selecting by path).
pub fn key_for(rel: &RelPath) -> String {
    path_key(rel.as_str(), CaseSensitivity::Insensitive)
}

/// One asset found on a card, for an import preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CardAsset {
    /// The key `select` and plans use (the original's normalised path).
    pub key: String,
    pub kind: crate::catalog::MediaKind,
    /// Paths relative to the card, the original first.
    pub files: Vec<String>,
    /// Bytes of all its files.
    pub size: u64,
    /// The original's modified time (ms), a stand-in for the capture day.
    pub mtime_ms: Option<i64>,
    pub diff: ImportDiff,
}

/// What is on the card at `mount`, grouped into assets, and whether each is
/// already in the library. Cheap: items this card's memory says were
/// imported are not read; others are quick-hashed (first and last MiB of
/// the original) against the catalog. Writes nothing.
pub fn scan_card(
    cat: &Catalog,
    lib: &Library,
    mount: &Path,
    card: Option<&crate::cards::CardRecord>,
) -> Result<Vec<CardAsset>, IngestError> {
    let src =
        SourceVolume::new(mount).map_err(|_| IngestError::SourceMissing(mount.to_path_buf()))?;
    let (files, _) = crate::rebuild::list_media_files(src.root());
    let grouping = crate::rebuild::group_folder(src.root(), &files);
    let imported: HashSet<String> = match card {
        Some(c) => {
            let st = crate::cards::status(lib, c, mount)?;
            let new: HashSet<&String> = st.new.iter().collect();
            crate::cards::items(mount)
                .into_iter()
                .filter(|i| !new.contains(&i.original))
                .map(|i| i.key)
                .collect()
        }
        None => HashSet::new(),
    };
    let mut out = Vec::with_capacity(grouping.groups.len());
    for g in &grouping.groups {
        let Some(orig) = g.members.first().and_then(|m| files.get(m.index)) else {
            continue;
        };
        let key = orig.key(CaseSensitivity::Insensitive);
        let mut size = 0u64;
        let mut paths = Vec::with_capacity(g.members.len());
        for m in &g.members {
            if let Some(f) = files.get(m.index) {
                size += src.metadata(f).map(|md| md.len()).unwrap_or(0);
                paths.push(f.as_str().to_owned());
            }
        }
        let md = src.metadata(orig)?;
        let mtime_ms = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| i64::try_from(d.as_millis()).ok());
        let diff = if imported.contains(&key) {
            ImportDiff::AlreadyImported
        } else {
            let q = src.quick_hash(orig)?;
            let known = cat.read(|tx| tx.files().by_quick(md.len(), q))?;
            if known.is_empty() {
                ImportDiff::New
            } else {
                ImportDiff::AlreadyImported
            }
        };
        out.push(CardAsset {
            key,
            kind: g.media_kind(),
            files: paths,
            size,
            mtime_ms,
            diff,
        });
    }
    out.sort_by(|a, b| a.mtime_ms.cmp(&b.mtime_ms).then(a.key.cmp(&b.key)));
    Ok(out)
}

/// A cheap pre-check for an import window: are these destinations
/// acceptable for this source (same rules as [`plan`]), and how much space
/// is free on each? Registers and creates nothing.
pub fn check_destinations_for(
    lib: &Library,
    source: &Path,
    destinations: &[PathBuf],
    allow_same_filesystem: bool,
) -> Result<Vec<(PathBuf, Option<u64>)>, IngestError> {
    let src =
        SourceVolume::new(source).map_err(|_| IngestError::SourceMissing(source.to_path_buf()))?;
    let mut dests = Vec::new();
    for (i, d) in destinations.iter().enumerate() {
        if sys::existing_ancestor(d).is_none() {
            return Err(IngestError::DestinationMissing(d.display().to_string()));
        }
        dests.push(preview_destination(
            lib,
            d,
            &format!("Destination {}", i + 1),
        )?);
    }
    check_destinations(&src, &dests, allow_same_filesystem)?;
    Ok(dests
        .iter()
        .map(|d| {
            let free = sys::existing_ancestor(&d.mount).and_then(|p| sys::free_space(&p));
            (d.mount.clone(), free)
        })
        .collect())
}

/// Where the template would put an original named `file_name` captured at
/// `local_time` (an example for the import window).
pub fn example_name(
    template: &Template,
    file_name: &str,
    local_time: Option<chrono::NaiveDateTime>,
    event: Option<&str>,
) -> Result<String, IngestError> {
    let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
    let ext = file_name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let input = TemplateInput {
        local_time,
        event: event.map(str::to_owned),
        camera: None,
        lens: None,
        orig: stem.to_owned(),
    };
    let base = template.render(&input, Some(1))?;
    Ok(if ext.is_empty() {
        base.to_string()
    } else {
        format!("{base}.{ext}")
    })
}
