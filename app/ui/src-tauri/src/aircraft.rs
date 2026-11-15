//! Read aircraft for the UI (ADR-041): turning it on (which downloads the
//! two text models on a background thread, `aircraft://install`) or off,
//! its status, and queueing, following and stopping the photos the user
//! picked. The work is in `keepsake_core::ai::aircraft`; reading runs in
//! the render service.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keepsake_core::ai::aircraft::marks::Confidence;
use keepsake_core::ai::aircraft::{install, scan};
use keepsake_core::ai::install as ai_install;
use keepsake_core::catalog::{AssetId, TagPath};
use keepsake_core::render::Variant;
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{
    AiInstallProgress, AircraftConfidenceDto, AircraftLookupDto, AircraftNeighbourDto,
    AircraftReviewDto, AircraftReviewItemDto, AircraftStatus, CloudStarted, CloudStatusDto,
    SamePlaneDto, ScoredMarkDto, Selection,
};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// What the aircraft worker and the download last said.
#[derive(Default)]
pub struct AircraftState {
    /// Where the text detector runs, once the aircraft worker loaded it.
    accel: Mutex<Option<String>>,
    /// Why the aircraft worker did not start.
    problem: Mutex<Option<String>>,
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

impl AircraftState {
    /// The render service loaded the text models (`accel`) or could not
    /// (`problem`).
    pub fn note(&self, accel: Option<String>, problem: Option<String>) {
        set(&self.accel, accel);
        set(&self.problem, problem);
    }
}

#[tauri::command(async)]
pub fn aircraft_status(state: State<'_, AppState>) -> CmdResult<AircraftStatus> {
    let settings = Settings::load().map_err(err)?;
    let on = settings.ai.aircraft;
    let st = &state.aircraft;
    Ok(AircraftStatus {
        enabled: on,
        models_installed: install::installed(),
        models_bytes: install::TOTAL_BYTES,
        runtime_found: ai_install::find_runtime().is_some(),
        looks: ai_install::find_model().is_some(),
        accel: get(&st.accel).flatten().filter(|_| on),
        problem: get(&st.problem).flatten().filter(|_| on),
        installing: get(&st.install).flatten().map(|(_, done)| done),
        confidence: confidence_dto(settings.ai.aircraft_confidence(), settings.ai.same_shot_s),
        lookup: settings.ai.aircraft_lookup,
    })
}

fn save_and_restart(app: &AppHandle, enabled: bool) -> CmdResult<()> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.aircraft = enabled;
    s.save().map_err(err)?;
    let state = app.state::<AppState>();
    state.aircraft.note(None, None);
    if state.try_open().is_some() {
        state.reload_settings()?;
        crate::render::restart_service(app)?;
    }
    let _ = app.emit("aircraft://status", ());
    Ok(())
}

/// Turn Read aircraft on or off. On: downloads the two text models first
/// if needed (progress on `aircraft://install`).
#[tauri::command(async)]
pub fn aircraft_enable(app: AppHandle, enabled: bool) -> CmdResult<()> {
    if !enabled || install::installed() {
        return save_and_restart(&app, enabled);
    }
    let state = app.state::<AppState>();
    let cancel = {
        let mut slot = state
            .aircraft
            .install
            .lock()
            .map_err(|_| "aircraft state poisoned")?;
        if slot.is_some() {
            return Ok(()); // already downloading
        }
        let c = Arc::new(AtomicBool::new(false));
        *slot = Some((Arc::clone(&c), 0));
        c
    };
    let app2 = app.clone();
    std::thread::Builder::new()
        .name("aircraft-install".into())
        .spawn(move || {
            let st = app2.state::<AppState>();
            let r = install::install(&cancel, &mut |done, total| {
                if let Ok(mut s) = st.aircraft.install.lock() {
                    if let Some(slot) = s.as_mut() {
                        slot.1 = done;
                    }
                }
                let _ = app2.emit(
                    "aircraft://install",
                    AiInstallProgress {
                        done,
                        total,
                        finished: false,
                        error: None,
                    },
                );
            });
            set(&st.aircraft.install, None);
            let error = match r {
                Ok(()) => save_and_restart(&app2, true).err(),
                Err(keepsake_core::ai::AiError::Cancelled) => Some("cancelled".to_owned()),
                Err(e) => Some(e.to_string()),
            };
            let _ = app2.emit(
                "aircraft://install",
                AiInstallProgress {
                    done: 0,
                    total: install::TOTAL_BYTES,
                    finished: true,
                    error,
                },
            );
        })
        .map_err(err)?;
    Ok(())
}

#[tauri::command(async)]
pub fn aircraft_install_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some((c, _)) = get(&state.aircraft.install).flatten() {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Turn Read aircraft off and delete the downloaded text models. Keywords
/// it gave stay.
#[tauri::command(async)]
pub fn aircraft_remove_models(app: AppHandle) -> CmdResult<bool> {
    save_and_restart(&app, false)?;
    install::remove().map_err(err)
}

fn dto((s, problem): (keepsake_core::ai::cloud::CloudStatus, Option<String>)) -> CloudStatusDto {
    CloudStatusDto {
        items: s.items,
        analysed: s.done,
        pending: s.pending,
        failed: s.failed,
        problem,
    }
}

fn unique(ids: impl IntoIterator<Item = AssetId>) -> Vec<AssetId> {
    let mut out: Vec<AssetId> = Vec::new();
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Read the aircraft in the selected items (in the background).
#[tauri::command(async)]
pub fn aircraft_read(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
) -> CmdResult<CloudStarted> {
    if !Settings::load().map_err(err)?.ai.aircraft {
        return Err("Turn on Read aircraft in Settings first.".into());
    }
    let ids = unique(crate::edit::resolve(&state, &selection)?);
    if ids.is_empty() {
        return Err("Select the photos to read first.".into());
    }
    let open = state.open()?;
    {
        let mut cat = open.writer()?;
        scan::enqueue(&mut cat, &ids).map_err(err)?;
    }
    crate::render::start_service(&app)?;
    let cat = open.cat()?;
    Ok(CloudStarted {
        provider: scan::SOURCE.to_owned(),
        ids: ids.iter().map(ToString::to_string).collect(),
        status: dto(scan::status(&cat, &ids).map_err(err)?),
    })
}

#[tauri::command(async)]
pub fn aircraft_read_status(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<CloudStatusDto> {
    let ids = unique(convert::ids(&ids)?);
    let open = state.open()?;
    let cat = open.cat()?;
    let mut st = dto(scan::status(&cat, &ids).map_err(err)?);
    // Nothing moves while the worker is not running: say why.
    if st.pending > 0 && st.problem.is_none() {
        st.problem = get(&state.aircraft.problem)
            .flatten()
            .map(|p| format!("Not running: {p}"));
    }
    Ok(st)
}

/// Read the aircraft photos of a folder or dates (those tagged Airplane,
/// Helicopter or Airport and not read yet), in the background.
pub fn target_start(
    app: &AppHandle,
    state: &AppState,
    target: &keepsake_core::ai::Target,
) -> CmdResult<keepsake_core::ai::analyze::RangeStatus> {
    if !Settings::load().map_err(err)?.ai.aircraft {
        return Err("Turn on Read aircraft in Settings first.".into());
    }
    let open = state.open()?;
    let st = {
        let mut cat = open.writer()?;
        scan::enqueue_target(&mut cat, target).map_err(err)?
    };
    crate::render::start_service(app)?;
    Ok(st)
}

pub fn target_status(
    state: &AppState,
    target: &keepsake_core::ai::Target,
) -> CmdResult<keepsake_core::ai::analyze::RangeStatus> {
    let open = state.open()?;
    let cat = open.cat()?;
    scan::target_status(&cat, target).map_err(err)
}

pub fn target_stop(state: &AppState, target: &keepsake_core::ai::Target) -> CmdResult<()> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::stop_target(&mut cat, target).map_err(err)?;
    Ok(())
}

/// Stop: items not read yet are not read; keywords already added stay.
#[tauri::command(async)]
pub fn aircraft_read_stop(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    let ids = unique(convert::ids(&ids)?);
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::stop(&mut cat, &ids).map_err(err)?;
    Ok(())
}

fn confidence_dto(c: Confidence, same_shot_s: u8) -> AircraftConfidenceDto {
    let p = |v: f32| (v * 100.0).round().clamp(0.0, 100.0) as u8;
    AircraftConfidenceDto {
        registration_pct: p(c.registration),
        airline_pct: p(c.airline),
        type_pct: p(c.aircraft_type),
        same_shot_s: same_shot_s.clamp(1, 60),
    }
}

fn confidence_of(d: &AircraftConfidenceDto) -> Confidence {
    let f = |p: u8| f32::from(p) / 100.0;
    Confidence {
        registration: f(d.registration_pct),
        airline: f(d.airline_pct),
        aircraft_type: f(d.type_pct),
    }
    .clamped()
}

fn scored(list: &[(String, f32)]) -> Vec<ScoredMarkDto> {
    let mut out: Vec<ScoredMarkDto> = list
        .iter()
        .map(|(name, s)| ScoredMarkDto {
            name: name.clone(),
            pct: (s * 100.0).round().clamp(0.0, 100.0) as u8,
        })
        .collect();
    out.sort_by_key(|m| std::cmp::Reverse(m.pct));
    out
}

/// The photos of a folder (`volume` and `folder`) or of dates (`from`,
/// `to`) that Read aircraft reads: those with an aircraft tag.
#[tauri::command(async)]
pub fn aircraft_target_ids(
    state: State<'_, AppState>,
    volume: Option<String>,
    folder: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> CmdResult<Vec<String>> {
    let target = match (volume, from, to) {
        (Some(v), _, _) => keepsake_core::ai::Target::Folder {
            volume: convert::id(&v, "volume")?,
            folder: folder.unwrap_or_default(),
        },
        (None, Some(from), Some(to)) => keepsake_core::ai::Target::Dates { from, to },
        _ => return Err("Read aircraft needs a folder or dates.".into()),
    };
    let open = state.open()?;
    let cat = open.cat()?;
    Ok(scan::target_items(&cat, &target)
        .map_err(err)?
        .iter()
        .map(ToString::to_string)
        .collect())
}

/// The review window: what was read in `ids` so far, offered at
/// `confidence` (the saved one when `None`), and how reading is getting on.
/// Thumbnails and neighbours only for the photos in `shown` (those on
/// screen; `None`: all): a batch can hold thousands, and looking for each
/// one's neighbours takes long.
#[tauri::command(async)]
pub fn aircraft_review(
    state: State<'_, AppState>,
    ids: Vec<String>,
    confidence: Option<AircraftConfidenceDto>,
    shown: Option<Vec<String>>,
) -> CmdResult<AircraftReviewDto> {
    let shown: Option<std::collections::HashSet<AssetId>> = match shown {
        Some(s) => Some(convert::ids(&s)?.into_iter().collect()),
        None => None,
    };
    let saved = Settings::load().map_err(err)?;
    let conf = match &confidence {
        Some(c) => confidence_of(c),
        None => saved.ai.aircraft_confidence(),
    };
    let same_shot_s = confidence
        .as_ref()
        .map_or(saved.ai.same_shot_s, |c| c.same_shot_s)
        .clamp(1, 60);
    let ids = unique(convert::ids(&ids)?);
    let open = state.open()?;
    let cat = open.cat()?;
    let (st, problem) = scan::status(&cat, &ids).map_err(err)?;
    let mut items = Vec::new();
    for it in scan::review_items(&cat, &ids).map_err(err)? {
        let on_screen = shown.as_ref().is_none_or(|s| s.contains(&it.asset));
        let (name, thumb) = if on_screen {
            photo_of(&cat, &open.cache, it.asset)?
        } else {
            (name_of(&cat, it.asset)?, None)
        };
        let found = scan::found_for(&cat, &it, &conf).map_err(err)?;
        let offer = scan::offer(&it, &conf, &found).map_err(err)?;
        let names = |v: &[TagPath]| v.iter().map(TagPath::to_lightroom).collect::<Vec<_>>();
        let mut neighbours = Vec::new();
        if on_screen && it.review == scan::Review::Waiting {
            for n in scan::neighbours(&cat, it.asset, i64::from(same_shot_s) * 1000).map_err(err)? {
                let (name, thumb) = photo_of(&cat, &open.cache, n.asset)?;
                neighbours.push(AircraftNeighbourDto {
                    id: n.asset.to_string(),
                    name,
                    thumb,
                    seconds: n.ms as f32 / 1000.0,
                    similarity_pct: n
                        .similarity
                        .map(|s| (s * 100.0).round().clamp(0.0, 100.0) as u8),
                    alike: n.alike,
                });
            }
        }
        items.push(AircraftReviewItemDto {
            id: it.asset.to_string(),
            name,
            thumb,
            review: it.review.as_str().to_owned(),
            full_size: it.full_size,
            registrations: scored(&it.candidates.registrations),
            airlines: scored(&it.candidates.airlines),
            types: scored(&it.candidates.types),
            looks_like: it.looks_like.clone(),
            offered: names(&offer.keywords),
            unticked: names(&offer.unticked),
            from_lookup: names(&offer.from_lookup),
            lookups: found
                .into_iter()
                .map(|f| AircraftLookupDto {
                    registration: f.registration,
                    known: f.known,
                    aircraft_type: f.aircraft_type,
                    airline: f.airline,
                    owner: f.owner,
                    country: f.country,
                })
                .collect(),
            neighbours: on_screen.then_some(neighbours),
        });
    }
    let read = items.len() as u64;
    let mut problem = problem;
    if st.pending > 0 && problem.is_none() {
        problem = get(&state.aircraft.problem)
            .flatten()
            .map(|p| format!("Not running: {p}"));
    }
    Ok(AircraftReviewDto {
        items,
        confidence: confidence_dto(conf, same_shot_s),
        total: st.items,
        read,
        pending: st.pending,
        failed: st.failed,
        problem,
    })
}

/// Keep the review window's confidence for next time.
#[tauri::command(async)]
pub fn aircraft_set_confidence(
    confidence: AircraftConfidenceDto,
) -> CmdResult<AircraftConfidenceDto> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.set_aircraft_confidence(confidence_of(&confidence));
    s.ai.same_shot_s = confidence.same_shot_s.clamp(1, 60);
    s.save().map_err(err)?;
    Ok(confidence_dto(s.ai.aircraft_confidence(), s.ai.same_shot_s))
}

fn paths_of(keywords: &[String]) -> CmdResult<Vec<TagPath>> {
    keywords
        .iter()
        .map(|k| TagPath::parse_lightroom(k).map_err(err))
        .collect()
}

/// A photo's file name and thumbnail, for the window.
fn photo_of(
    cat: &keepsake_core::catalog::Catalog,
    cache: &keepsake_core::render::Cache,
    asset: AssetId,
) -> CmdResult<(Option<String>, Option<String>)> {
    let name = name_of(cat, asset)?;
    let thumb = keepsake_core::render::source::source_hash(cat, asset)
        .ok()
        .flatten()
        .and_then(|h| {
            convert::cached(cache, Some(&h), Variant::Thumb256)
                .or_else(|| convert::cached(cache, Some(&h), Variant::Thumb1024))
        });
    Ok((name, thumb))
}

fn name_of(cat: &keepsake_core::catalog::Catalog, asset: AssetId) -> CmdResult<Option<String>> {
    Ok(cat
        .read(|tx| tx.files().originals(asset))
        .map_err(err)?
        .first()
        .map(|f| f.rel_path.file_name().to_owned()))
}

/// The lookup the render service uses (adsbdb.com).
pub fn lookup() -> Arc<dyn keepsake_core::ai::aircraft::lookup::Lookup> {
    Arc::new(keepsake_core::ai::aircraft::lookup::AdsbDb::new(Arc::new(
        keepsake_core::ai::cloud::UreqHttp::new(),
    )))
}

/// Turn the online registration lookup on or off.
#[tauri::command(async)]
pub fn aircraft_set_lookup(app: AppHandle, enabled: bool) -> CmdResult<()> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.aircraft_lookup = enabled;
    s.save().map_err(err)?;
    let state = app.state::<AppState>();
    if state.try_open().is_some() {
        state.reload_settings()?;
        crate::render::restart_service(&app)?;
    }
    let _ = app.emit("aircraft://status", ());
    Ok(())
}

/// Accept a photo's read: `keywords` (`Aircraft|…|…`, those left ticked)
/// become its keywords, and the `neighbours` ticked (photos of the same
/// burst) get them too. None: the read is dismissed. Returns those added.
#[tauri::command(async)]
pub fn aircraft_accept(
    state: State<'_, AppState>,
    id: String,
    keywords: Vec<String>,
    neighbours: Option<Vec<String>>,
) -> CmdResult<Vec<String>> {
    let asset: AssetId = convert::id(&id, "asset")?;
    let paths = paths_of(&keywords)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    let added = scan::accept(&mut cat, &open.lib, asset, &paths).map_err(err)?;
    // `None`: its neighbours were not shown (Accept all, off screen): the
    // alike ones, as they would have been ticked.
    let others = match neighbours {
        Some(n) => convert::ids(&n)?,
        None if paths.is_empty() => Vec::new(),
        None => {
            let window =
                i64::from(Settings::load().map_err(err)?.ai.same_shot_s.clamp(1, 60)) * 1000;
            scan::neighbours(&cat, asset, window)
                .map_err(err)?
                .into_iter()
                .filter(|n| n.alike)
                .map(|n| n.asset)
                .collect()
        }
    };
    if !paths.is_empty() && !others.is_empty() {
        scan::add_to(&mut cat, &open.lib, &others, &paths).map_err(err)?;
    }
    Ok(added)
}

/// After accepting `keywords` for a photo: the other photos of its
/// registration that lack the airline or type among them.
#[tauri::command(async)]
pub fn aircraft_same_plane(
    state: State<'_, AppState>,
    id: String,
    keywords: Vec<String>,
) -> CmdResult<Vec<SamePlaneDto>> {
    let asset: AssetId = convert::id(&id, "asset")?;
    let paths = paths_of(&keywords)?;
    let kind = |p: &TagPath| p.segments().get(1).cloned().unwrap_or_default();
    let wanted: Vec<TagPath> = paths
        .iter()
        .filter(|p| matches!(kind(p).as_str(), "Airline" | "Type"))
        .cloned()
        .collect();
    let open = state.open()?;
    let cat = open.cat()?;
    let mut out = Vec::new();
    for reg in paths.iter().filter(|p| kind(p) == "Registration") {
        if wanted.is_empty() {
            break;
        }
        for (other, missing) in
            scan::same_registration(&cat, reg.leaf(), &wanted, &[asset]).map_err(err)?
        {
            if out.iter().any(|o: &SamePlaneDto| o.id == other.to_string()) {
                continue;
            }
            let (name, thumb) = photo_of(&cat, &open.cache, other)?;
            out.push(SamePlaneDto {
                id: other.to_string(),
                name,
                thumb,
                missing: missing.iter().map(TagPath::to_lightroom).collect(),
            });
        }
    }
    Ok(out)
}

/// Add aircraft keywords to photos (the same aircraft's other photos).
/// Returns how many got something new.
#[tauri::command(async)]
pub fn aircraft_add_keywords(
    state: State<'_, AppState>,
    ids: Vec<String>,
    keywords: Vec<String>,
) -> CmdResult<u64> {
    let ids = convert::ids(&ids)?;
    let paths = paths_of(&keywords)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::add_to(&mut cat, &open.lib, &ids, &paths).map_err(err)
}

/// Dismiss a photo's read: nothing becomes a keyword.
#[tauri::command(async)]
pub fn aircraft_dismiss(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let asset: AssetId = convert::id(&id, "asset")?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::dismiss(&mut cat, asset).map_err(err)
}
