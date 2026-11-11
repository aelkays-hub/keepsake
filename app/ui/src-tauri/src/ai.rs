//! On-device AI for the UI (ADR-035): turning it on (which downloads the
//! model on a background thread, `ai://install`), its status, duplicate
//! groups and the user's choices, and AI tags on an item. The work itself
//! is in `keepsake_core::ai`; analysis runs in the render service.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keepsake_core::ai::{self, duplicates, install, Purpose, JOB_AI, JOB_AI_TAG, MODEL_ID};
use keepsake_core::catalog::{AssetId, DuplicateChoice, TagPath, TagSource};
use keepsake_core::metadata::MetadataService;
use keepsake_core::render::{source, Variant};
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{
    AiFolderStatus, AiInstallProgress, AiRangeStatus, AiStatus, AiTagDto, AiTagSourceDto,
    AiTagsDto, DuplicateDayDto, DuplicateGroupDto, DuplicateMemberDto, DuplicateSummaryDto,
};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// What the render service and the download last said.
#[derive(Default)]
pub struct AiState {
    /// Where the model runs, once the AI worker loaded it.
    pub accel: Mutex<Option<String>>,
    /// Why the AI worker did not start.
    pub problem: Mutex<Option<String>>,
    /// The running download: its stop flag and bytes so far.
    install: Mutex<Option<(Arc<AtomicBool>, u64)>>,
}

fn set<T>(m: &Mutex<T>, v: T) {
    if let Ok(mut g) = m.lock() {
        *g = v;
    }
}

fn get<T: Clone>(m: &Mutex<T>) -> Option<T> {
    m.lock().ok().map(|g| g.clone())
}

impl AiState {
    /// The render service loaded the model (`accel`) or could not (`problem`).
    pub fn note(&self, accel: Option<String>, problem: Option<String>) {
        set(&self.accel, accel);
        set(&self.problem, problem);
    }
}

#[tauri::command(async)]
pub fn ai_status(state: State<'_, AppState>) -> CmdResult<AiStatus> {
    let settings = Settings::load().map_err(err)?;
    let counts = match state.try_open() {
        Some(open) => {
            let cat = open.cat()?;
            let mut c = cat
                .read(|tx| tx.ai().counts(MODEL_ID, JOB_AI))
                .map_err(err)?;
            let t = cat
                .read(|tx| tx.ai().counts(MODEL_ID, JOB_AI_TAG))
                .map_err(err)?;
            c.pending += t.pending;
            c.failed += t.failed;
            c
        }
        None => Default::default(),
    };
    let installing = get(&state.ai.install).flatten().map(|(_, done)| done);
    let enabled = settings.ai.enabled;
    Ok(AiStatus {
        enabled,
        gpu: settings.ai.gpu,
        model_installed: install::find_model().is_some(),
        model_bytes: install::MODEL_BYTES,
        runtime_found: install::find_runtime().is_some(),
        accel: get(&state.ai.accel).flatten().filter(|_| enabled),
        problem: get(&state.ai.problem).flatten().filter(|_| enabled),
        installing,
        analysed: counts.analysed,
        assets: counts.assets,
        pending: counts.pending,
        failed: counts.failed,
    })
}

fn save_and_restart(app: &AppHandle, enabled: bool, gpu: Option<bool>) -> CmdResult<()> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.enabled = enabled;
    if let Some(g) = gpu {
        s.ai.gpu = g;
    }
    s.save().map_err(err)?;
    let state = app.state::<AppState>();
    state.ai.note(None, None);
    if state.try_open().is_some() {
        state.reload_settings()?;
        crate::render::restart_service(app)?;
    }
    Ok(())
}

/// Turn AI on or off. On: downloads the model first if needed (progress on
/// `ai://install`), then starts the analysis. `gpu`: change that setting.
#[tauri::command(async)]
pub fn ai_enable(app: AppHandle, enabled: bool, gpu: Option<bool>) -> CmdResult<()> {
    if !enabled || install::find_model().is_some() {
        return save_and_restart(&app, enabled, gpu);
    }
    let state = app.state::<AppState>();
    let cancel = {
        let mut slot = state.ai.install.lock().map_err(|_| "AI state poisoned")?;
        if slot.is_some() {
            return Ok(()); // already downloading
        }
        let c = Arc::new(AtomicBool::new(false));
        *slot = Some((Arc::clone(&c), 0));
        c
    };
    let app2 = app.clone();
    std::thread::Builder::new()
        .name("ai-install".into())
        .spawn(move || {
            let st = app2.state::<AppState>();
            let r = install::install_model(&cancel, &mut |done, total| {
                if let Ok(mut s) = st.ai.install.lock() {
                    if let Some(slot) = s.as_mut() {
                        slot.1 = done;
                    }
                }
                let _ = app2.emit(
                    "ai://install",
                    AiInstallProgress {
                        done,
                        total,
                        finished: false,
                        error: None,
                    },
                );
            });
            set(&st.ai.install, None);
            let error = match r {
                Ok(_) => save_and_restart(&app2, true, gpu).err(),
                Err(ai::AiError::Cancelled) => Some("cancelled".to_owned()),
                Err(e) => Some(e.to_string()),
            };
            let _ = app2.emit(
                "ai://install",
                AiInstallProgress {
                    done: 0,
                    total: install::MODEL_BYTES,
                    finished: true,
                    error,
                },
            );
        })
        .map_err(err)?;
    Ok(())
}

#[tauri::command(async)]
pub fn ai_install_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    state.ai.stop_install();
    Ok(())
}

impl AiState {
    /// Stop the model download, if one is running (its curl ends with it).
    pub(crate) fn stop_install(&self) {
        if let Some((c, _)) = get(&self.install).flatten() {
            c.store(true, Ordering::SeqCst);
        }
    }
}

/// Turn AI off and delete the downloaded model. AI tags and decisions stay.
#[tauri::command(async)]
pub fn ai_remove_model(app: AppHandle) -> CmdResult<bool> {
    save_and_restart(&app, false, None)?;
    install::remove_user_model().map_err(err)
}

#[tauri::command(async)]
pub fn ai_duplicates(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<DuplicateSummaryDto> {
    let open = state.open()?;
    let cat = open.cat()?;
    let s = duplicates::summary_in(&cat, from.as_deref(), to.as_deref()).map_err(err)?;
    Ok(DuplicateSummaryDto {
        days: s
            .days
            .into_iter()
            .map(|d| DuplicateDayDto {
                date: d.date,
                groups: d.groups,
                items: d.items,
                undecided: d.undecided,
            })
            .collect(),
        groups: s.groups,
        items: s.items,
        undecided: s.undecided,
    })
}

#[tauri::command(async)]
pub fn ai_duplicate_day(
    state: State<'_, AppState>,
    date: String,
) -> CmdResult<Vec<DuplicateGroupDto>> {
    let open = state.open()?;
    let cat = open.cat()?;
    let groups = duplicates::day_groups(&cat, &date).map_err(err)?;
    let mut out = Vec::with_capacity(groups.len());
    for g in groups {
        let mut members = Vec::with_capacity(g.members.len());
        for m in g.members {
            let name = cat
                .read(|tx| tx.files().originals(m.asset_id))
                .map_err(err)?
                .first()
                .map(|f| f.rel_path.file_name().to_owned())
                .unwrap_or_default();
            let hash = source::source_hash(&cat, m.asset_id).map_err(err)?;
            members.push(DuplicateMemberDto {
                id: m.asset_id.to_string(),
                name,
                video: m.media_kind == keepsake_core::catalog::MediaKind::Video,
                capture_ms: m.capture_ms,
                rating: u8::try_from(m.rating.clamp(0, 5)).unwrap_or(0),
                choice: m.choice.map(|c| c.as_str().to_owned()),
                likeness: (m.similarity.clamp(0.0, 1.0) * 100.0).round() as u8,
                exact: m.exact,
                thumb: convert::cached(&open.cache, hash.as_ref(), Variant::Thumb256),
            });
        }
        out.push(DuplicateGroupDto {
            date: g.date,
            keeper: g.keeper.to_string(),
            decided: g.decided,
            members,
        });
    }
    Ok(out)
}

fn range_dto(from: String, to: String, s: ai::analyze::RangeStatus) -> AiRangeStatus {
    AiRangeStatus {
        from,
        to,
        items: s.items,
        analysed: s.analysed,
        pending: s.pending,
    }
}

fn purpose(what: &str) -> CmdResult<Purpose> {
    Purpose::parse(what).ok_or_else(|| format!("unknown AI task: {what}"))
}

/// `what` = `duplicates` (Find duplicates: the RAW photos, analysed) or
/// `tags` (Tag photos: every item) for the items captured from `from` to
/// `to` (`YYYY-MM-DD`, inclusive): queued ahead of other work, run on the
/// render service's AI worker.
#[tauri::command(async)]
pub fn ai_analyze_range(
    app: AppHandle,
    state: State<'_, AppState>,
    from: String,
    to: String,
    what: String,
) -> CmdResult<AiRangeStatus> {
    let purpose = purpose(&what)?;
    if !Settings::load().map_err(err)?.ai.enabled {
        return Err("Turn on On-device AI in Settings first (it downloads the model once).".into());
    }
    let open = state.open()?;
    let mut cat = open.writer()?;
    let s = ai::analyze::enqueue_range(&mut cat, &from, &to, purpose).map_err(err)?;
    // The AI worker runs in the render service.
    crate::render::start_service(&app)?;
    Ok(range_dto(from, to, s))
}

#[tauri::command(async)]
pub fn ai_range_status(
    state: State<'_, AppState>,
    from: String,
    to: String,
    what: String,
) -> CmdResult<AiRangeStatus> {
    let purpose = purpose(&what)?;
    let open = state.open()?;
    let cat = open.cat()?;
    let s = ai::analyze::range_status(&cat, &from, &to, purpose).map_err(err)?;
    Ok(range_dto(from, to, s))
}

fn folder_target(volume: &str, folder: String) -> CmdResult<ai::Target> {
    Ok(ai::Target::Folder {
        volume: convert::id(volume, "volume")?,
        folder,
    })
}

fn folder_dto(s: ai::analyze::RangeStatus) -> AiFolderStatus {
    AiFolderStatus {
        items: s.items,
        analysed: s.analysed,
        pending: s.pending,
    }
}

/// Find duplicates (`what` = `"duplicates"`) or tag the photos (`"tags"`)
/// of a folder and the folders in it (`""`: the whole drive).
#[tauri::command(async)]
pub fn ai_analyze_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    volume: String,
    folder: String,
    what: String,
) -> CmdResult<AiFolderStatus> {
    let purpose = purpose(&what)?;
    if !Settings::load().map_err(err)?.ai.enabled {
        return Err("Turn on On-device AI in Settings first (it downloads the model once).".into());
    }
    let target = folder_target(&volume, folder)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    let s = ai::enqueue_target(&mut cat, &target, purpose).map_err(err)?;
    crate::render::start_service(&app)?;
    Ok(folder_dto(s))
}

#[tauri::command(async)]
pub fn ai_folder_status(
    state: State<'_, AppState>,
    volume: String,
    folder: String,
    what: String,
) -> CmdResult<AiFolderStatus> {
    let purpose = purpose(&what)?;
    let target = folder_target(&volume, folder)?;
    let open = state.open()?;
    let cat = open.cat()?;
    Ok(folder_dto(
        ai::target_status(&cat, &target, purpose).map_err(err)?,
    ))
}

/// Stop: drop the date range's queued jobs of `what` (what was done stays).
#[tauri::command(async)]
pub fn ai_range_stop(
    state: State<'_, AppState>,
    from: String,
    to: String,
    what: String,
) -> CmdResult<()> {
    let purpose = purpose(&what)?;
    let target = ai::Target::Dates { from, to };
    let open = state.open()?;
    let mut cat = open.writer()?;
    ai::stop_target(&mut cat, &target, purpose).map_err(err)?;
    Ok(())
}

/// Stop: drop the folder's queued jobs of `what` (what was done stays).
#[tauri::command(async)]
pub fn ai_folder_stop(
    state: State<'_, AppState>,
    volume: String,
    folder: String,
    what: String,
) -> CmdResult<()> {
    let purpose = purpose(&what)?;
    let target = folder_target(&volume, folder)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    ai::stop_target(&mut cat, &target, purpose).map_err(err)?;
    Ok(())
}

/// Find duplicates or tag the photos with no date (the calendar's "No
/// date"), as for a folder.
#[tauri::command(async)]
pub fn ai_analyze_undated(
    app: AppHandle,
    state: State<'_, AppState>,
    what: String,
) -> CmdResult<AiFolderStatus> {
    let purpose = purpose(&what)?;
    if !Settings::load().map_err(err)?.ai.enabled {
        return Err("Turn on On-device AI in Settings first (it downloads the model once).".into());
    }
    let open = state.open()?;
    let mut cat = open.writer()?;
    let s = ai::enqueue_target(&mut cat, &ai::Target::Undated, purpose).map_err(err)?;
    crate::render::start_service(&app)?;
    Ok(folder_dto(s))
}

#[tauri::command(async)]
pub fn ai_undated_status(state: State<'_, AppState>, what: String) -> CmdResult<AiFolderStatus> {
    let purpose = purpose(&what)?;
    let open = state.open()?;
    let cat = open.cat()?;
    Ok(folder_dto(
        ai::target_status(&cat, &ai::Target::Undated, purpose).map_err(err)?,
    ))
}

/// Stop: drop the queued jobs of `what` for the photos with no date.
#[tauri::command(async)]
pub fn ai_undated_stop(state: State<'_, AppState>, what: String) -> CmdResult<()> {
    let purpose = purpose(&what)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    ai::stop_target(&mut cat, &ai::Target::Undated, purpose).map_err(err)?;
    Ok(())
}

/// Keep `keeper`; the rest of `group` become "not kept" (nothing deleted).
#[tauri::command(async)]
pub fn ai_pick_keeper(
    state: State<'_, AppState>,
    keeper: String,
    group: Vec<String>,
) -> CmdResult<()> {
    let open = state.open()?;
    let keeper: AssetId = convert::id(&keeper, "asset")?;
    let group = convert::ids(&group)?;
    let mut cat = open.writer()?;
    MetadataService::new(&open.lib, &mut cat)
        .pick_keeper(keeper, &group)
        .map_err(err)
}

/// Set (or clear, with `null`) the duplicate decision of these items.
#[tauri::command(async)]
pub fn ai_set_choice(
    state: State<'_, AppState>,
    ids: Vec<String>,
    choice: Option<String>,
) -> CmdResult<()> {
    let open = state.open()?;
    let choice = match choice.as_deref() {
        None => None,
        Some(c) => Some(DuplicateChoice::parse(c).ok_or_else(|| format!("bad choice: {c}"))?),
    };
    let ids = convert::ids(&ids)?;
    let mut cat = open.writer()?;
    let mut svc = MetadataService::new(&open.lib, &mut cat);
    for id in ids {
        svc.set_duplicate_choice(id, choice).map_err(err)?;
    }
    Ok(())
}

/// AI tags on an item, most likely first, and whether it was analysed.
#[tauri::command(async)]
pub fn ai_tags(state: State<'_, AppState>, id: String) -> CmdResult<AiTagsDto> {
    let open = state.open()?;
    let id: AssetId = convert::id(&id, "asset")?;
    let cat = open.cat()?;
    // "Analysed" here means tagged: an analysis made to find duplicates
    // has no tags yet.
    let analysed = cat
        .read(|tx| tx.ai().analysis(id))
        .map_err(err)?
        .is_some_and(|a| a.model == MODEL_ID && a.tag_list > 0);
    let all = cat.read(|tx| tx.tags().for_asset(id)).map_err(err)?;
    let sources = cat.read(|tx| tx.ai().tag_sources(id)).map_err(err)?;
    let by_ai = by_ai(&all, &sources);
    let mut tags: Vec<AiTagDto> = all
        .into_iter()
        .filter(|t| t.source == TagSource::Ai)
        .map(|t| AiTagDto {
            tag: t.path.to_lightroom(),
            confidence: (t.confidence.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0).round() as u8,
        })
        .collect();
    tags.sort_by(|a, b| b.confidence.cmp(&a.confidence).then(a.tag.cmp(&b.tag)));
    Ok(AiTagsDto {
        enabled: Settings::load().map_err(err)?.ai.enabled,
        analysed,
        tags,
        by_ai,
    })
}

/// Per AI (and the aircraft reader, ADR-041), the keywords it gave that
/// the item still has.
fn by_ai(
    tags: &[keepsake_core::catalog::AssetTag],
    sources: &[(String, TagPath)],
) -> Vec<AiTagSourceDto> {
    keepsake_core::ai::tag_sources()
        .into_iter()
        .filter_map(|(id, name)| {
            let tags: Vec<String> = sources
                .iter()
                .filter(|(by, path)| {
                    by == id
                        && tags
                            .iter()
                            .any(|t| t.source == TagSource::User && &t.path == path)
                })
                .map(|(_, path)| path.to_lightroom())
                .collect();
            (!tags.is_empty()).then(|| AiTagSourceDto {
                provider: id.to_owned(),
                name: name.to_owned(),
                tags,
            })
        })
        .collect()
}

/// Remove an AI tag for good (`keep: false`) or make it the user's own
/// keyword (`keep: true`).
#[tauri::command(async)]
pub fn ai_tag_decide(
    state: State<'_, AppState>,
    id: String,
    tag: String,
    keep: bool,
) -> CmdResult<()> {
    let open = state.open()?;
    let id: AssetId = convert::id(&id, "asset")?;
    let path = TagPath::parse_lightroom(&tag).map_err(err)?;
    let mut cat = open.writer()?;
    let mut svc = MetadataService::new(&open.lib, &mut cat);
    if keep {
        svc.keep_ai_tag(id, &path).map_err(err)?;
    } else {
        svc.reject_ai_tag(id, &path).map_err(err)?;
    }
    Ok(())
}
