//! Metadata edits from the inspector and keyboard. Every edit goes through
//! `MetadataService`, which updates the catalog and writes the XMP
//! sidecars. Up to [`INLINE`] assets are edited at once; larger selections
//! run on a background thread and report with `edit://progress`.

use std::sync::Arc;

use keepsake_core::catalog::{AssetId, Rating, TagPath};
use keepsake_core::library::LibraryError;
use keepsake_core::metadata::MetadataService;
use tauri::{AppHandle, Emitter, State};

use crate::api::{EditProgress, EditResult, Selection, ShiftSample, TimeShiftPreview};
use crate::convert;
use crate::state::{err, AppState, CmdResult, Open};

const INLINE: usize = 50;

fn resolve(state: &AppState, sel: &Selection) -> CmdResult<Vec<AssetId>> {
    let mut out = convert::ids(&sel.ids)?;
    if let Some(v) = sel.view {
        state.with_view(v, |view| {
            for (from, to) in &sel.ranges {
                let a = (*from as usize).min(view.ids.len());
                let b = (*to as usize).min(view.ids.len());
                if a < b {
                    out.extend_from_slice(&view.ids[a..b]);
                }
            }
        })?;
    }
    out.sort();
    out.dedup();
    Ok(out)
}

type EditFn = dyn Fn(&mut MetadataService<'_>, AssetId) -> Result<(), LibraryError> + Send + Sync;

fn apply(
    app: &AppHandle,
    open: Arc<Open>,
    ids: Vec<AssetId>,
    f: Arc<EditFn>,
) -> CmdResult<EditResult> {
    if ids.len() <= INLINE {
        let mut cat = open.writer()?;
        let mut svc = MetadataService::new(&open.lib, &mut cat);
        for id in &ids {
            f(&mut svc, *id).map_err(err)?;
        }
        return Ok(EditResult {
            changed: ids.len() as u64,
            background: false,
        });
    }
    let total = ids.len() as u64;
    let app = app.clone();
    std::thread::Builder::new()
        .name("metadata-edit".into())
        .spawn(move || {
            let mut errors = Vec::new();
            let Ok(mut cat) = open.lib.open_catalog() else {
                return;
            };
            let mut svc = MetadataService::new(&open.lib, &mut cat);
            for (i, id) in ids.iter().enumerate() {
                if let Err(e) = f(&mut svc, *id) {
                    if errors.len() < 20 {
                        errors.push(format!("{id}: {e}"));
                    }
                }
                let done = i as u64 + 1;
                if done.is_multiple_of(200) || done == total {
                    let _ = app.emit(
                        "edit://progress",
                        EditProgress {
                            done,
                            total,
                            finished: done == total,
                            errors: errors.clone(),
                        },
                    );
                }
            }
        })
        .map_err(err)?;
    Ok(EditResult {
        changed: total,
        background: true,
    })
}

#[tauri::command(async)]
pub fn edit_rating(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
    rating: u8,
) -> CmdResult<EditResult> {
    let r = Rating::new(rating.min(5)).map_err(err)?;
    let ids = resolve(&state, &selection)?;
    apply(
        &app,
        state.open()?,
        ids,
        Arc::new(move |svc, id| svc.set_rating(id, r).map(|_| ())),
    )
}

#[tauri::command(async)]
pub fn edit_label(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
    label: Option<String>,
) -> CmdResult<EditResult> {
    let ids = resolve(&state, &selection)?;
    let label = label.filter(|l| !l.is_empty());
    apply(
        &app,
        state.open()?,
        ids,
        Arc::new(move |svc, id| svc.set_label(id, label.as_deref()).map(|_| ())),
    )
}

#[tauri::command(async)]
pub fn edit_comment(
    state: State<'_, AppState>,
    id: String,
    comment: Option<String>,
) -> CmdResult<EditResult> {
    let open = state.open()?;
    let a = convert::id(&id, "asset")?;
    let mut cat = open.writer()?;
    MetadataService::new(&open.lib, &mut cat)
        .set_comment(a, comment.as_deref())
        .map_err(err)?;
    Ok(EditResult {
        changed: 1,
        background: false,
    })
}

/// Add and remove keywords (Lightroom-style paths, `Places|Lake`).
#[tauri::command(async)]
pub fn edit_keywords(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
    add: Vec<String>,
    remove: Vec<String>,
) -> CmdResult<EditResult> {
    let parse = |v: &[String]| -> CmdResult<Vec<TagPath>> {
        v.iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| TagPath::parse_lightroom(s).map_err(err))
            .collect()
    };
    let add = parse(&add)?;
    let remove = parse(&remove)?;
    let ids = resolve(&state, &selection)?;
    apply(
        &app,
        state.open()?,
        ids,
        Arc::new(move |svc, id| {
            let mut tags: Vec<TagPath> = svc
                .user_tags(id)?
                .into_iter()
                .filter(|t| !remove.contains(t))
                .collect();
            for t in &add {
                if !tags.contains(t) {
                    tags.push(t.clone());
                }
            }
            svc.set_user_tags(id, &tags).map(|_| ())
        }),
    )
}

/// Every keyword in the library (for suggestions).
#[tauri::command(async)]
pub fn keywords(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    let open = state.open()?;
    let mut v: Vec<String> = open
        .cat()?
        .read(|tx| tx.tags().all_paths())
        .map_err(err)?
        .iter()
        .map(|t| t.to_lightroom())
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    Ok(v)
}

// ── Time shift ──────────────────────────────────────────────────────────────

/// How many before → after examples the dialog shows.
const SAMPLES: usize = 10;

fn parse_local(s: &str) -> CmdResult<chrono::NaiveDateTime> {
    let s = s.trim().replace(' ', "T");
    chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M"))
        .map_err(|_| format!("not a date and time: {s} (use YYYY-MM-DD HH:MM:SS)"))
}

/// The offset that makes `reference` read `correct` (camera-local time),
/// and what it does to the selection. Writes nothing.
#[tauri::command(async)]
pub fn time_shift_preview(
    state: State<'_, AppState>,
    selection: Selection,
    reference: String,
    correct: String,
) -> CmdResult<TimeShiftPreview> {
    let open = state.open()?;
    let reference = convert::id(&reference, "asset")?;
    let correct = parse_local(&correct)?;
    let ids = resolve(&state, &selection)?;
    let cat = open.cat()?;
    let r = cat
        .read(|tx| tx.assets().require(reference))
        .map_err(err)?
        .capture
        .ok_or("the reference photo has no capture time")?;
    let offset = keepsake_core::select::shift_for(r, correct);
    let mut rows = cat.read(|tx| tx.browse().rows(&ids)).map_err(err)?;
    drop(cat);
    let undated = rows.iter().filter(|r| r.capture_ms.is_none()).count() as u64;
    rows.retain(|r| r.capture_ms.is_some());
    rows.sort_by_key(|r| r.capture_ms);
    let fmt = |ms: Option<i64>, off: Option<i16>| {
        convert::local_time(ms, off)
            .unwrap_or_default()
            .replace('T', " ")
    };
    let samples = rows
        .iter()
        .take(SAMPLES)
        .map(|r| ShiftSample {
            id: r.id.to_string(),
            name: r.name.clone(),
            before: fmt(r.capture_ms, r.utc_offset_min),
            after: fmt(r.capture_ms.map(|t| t + offset * 1000), r.utc_offset_min),
        })
        .collect();
    Ok(TimeShiftPreview {
        offset_s: offset,
        total: rows.len() as u64,
        undated,
        samples,
        reference_before: fmt(Some(r.utc_ms), r.utc_offset_min),
    })
}

/// Shift the capture time of the selection by `offset_s` seconds. Only the
/// catalog and the XMP sidecars change; originals are never written.
#[tauri::command(async)]
pub fn time_shift_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
    offset_s: i64,
) -> CmdResult<EditResult> {
    if offset_s == 0 {
        return Ok(EditResult {
            changed: 0,
            background: false,
        });
    }
    let open = state.open()?;
    let ids = resolve(&state, &selection)?;
    let dated: Vec<AssetId> = open
        .cat()?
        .read(|tx| tx.browse().rows(&ids))
        .map_err(err)?
        .into_iter()
        .filter(|r| r.capture_ms.is_some())
        .map(|r| r.id)
        .collect();
    apply(
        &app,
        open,
        dated,
        Arc::new(move |svc, id| svc.shift_capture_time(id, offset_s).map(|_| ())),
    )
}
