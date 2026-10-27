//! Library and browsing commands. Thin: validate, call keepsake-core,
//! convert to the `api` types.

use std::path::PathBuf;

use keepsake_core::catalog::{FacetColumn, FileRole, TagSource};
use keepsake_core::library::Library;
use keepsake_core::render::Variant;
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Manager, State};

use crate::api::{
    AppInfo, Bucket, CalendarLevelDto, Details, EventItem, FacetItem, Facets, FileItem, GpsItem,
    ImportItem, Query, Row, Sidebar, SortSpec, ViewInfo, VolumeItem,
};
use crate::convert;
use crate::state::{err, AppState, CmdResult, View};

#[tauri::command]
pub fn get_version() -> &'static str {
    keepsake_core::version()
}

#[tauri::command(async)]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: keepsake_core::version().to_owned(),
        library: state.try_open().map(|o| convert::path_text(o.root())),
        ui_test: crate::state::ui_test(),
    }
}

/// Make `lib` the open library and the default, and start background work.
fn adopt(app: &AppHandle, state: &AppState, lib: Library) -> CmdResult<AppInfo> {
    let root = std::fs::canonicalize(lib.root()).map_err(err)?;
    let mut settings = Settings::load().map_err(err)?;
    settings.default_library = Some(PathBuf::from(convert::path_text(&root)));
    settings.save().map_err(err)?;
    let open = state.set_library(lib)?;
    // The webview may load cached renditions, and nothing else.
    app.asset_protocol_scope()
        .allow_directory(open.cache_root(), true)
        .map_err(err)?;
    crate::background::start(app, state);
    Ok(AppInfo {
        version: keepsake_core::version().to_owned(),
        library: Some(convert::path_text(open.root())),
        ui_test: crate::state::ui_test(),
    })
}

/// Open the default library at start-up, if there is one.
pub fn open_default(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(settings) = Settings::load() else {
        return;
    };
    let Some(root) = settings.default_library else {
        return;
    };
    match Library::open(&root) {
        Ok(lib) => {
            if let Err(e) = adopt(app, &state, lib) {
                tracing::error!("opening {}: {e}", root.display());
            }
        }
        Err(e) => tracing::warn!("default library {}: {e}", root.display()),
    }
}

#[tauri::command(async)]
pub fn library_create(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<AppInfo> {
    let lib = match Library::open(std::path::Path::new(&path)) {
        Ok(l) => l,
        Err(_) => Library::create(std::path::Path::new(&path)).map_err(err)?,
    };
    adopt(&app, &state, lib)
}

#[tauri::command(async)]
pub fn library_open(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<AppInfo> {
    let lib = Library::open(std::path::Path::new(&path)).map_err(err)?;
    adopt(&app, &state, lib)
}

#[tauri::command(async)]
pub fn view_open(state: State<'_, AppState>, query: Query, sort: SortSpec) -> CmdResult<ViewInfo> {
    let open = state.open()?;
    let q = convert::query(&query)?;
    let s = convert::sort(&sort);
    let ids = open
        .cat()?
        .read(|tx| tx.browse().ordered_ids(&q, s))
        .map_err(err)?;
    let count = ids.len() as u64;
    let view = state.add_view(View { ids });
    Ok(ViewInfo { view, count })
}

#[tauri::command(async)]
pub fn view_rows(
    state: State<'_, AppState>,
    view: u32,
    offset: u64,
    limit: u32,
) -> CmdResult<Vec<Row>> {
    let open = state.open()?;
    let ids = state.with_view(view, |v| {
        let start = (offset as usize).min(v.ids.len());
        let end = (start + limit.min(1000) as usize).min(v.ids.len());
        v.ids[start..end].to_vec()
    })?;
    let cat = open.cat()?;
    let rows = cat.read(|tx| tx.browse().rows(&ids)).map_err(err)?;
    let online = open.online_volumes(&cat);
    Ok(rows
        .iter()
        .map(|r| convert::row(r, &open.cache, &online))
        .collect())
}

/// Where an asset is in a view (to keep it selected after re-sorting).
#[tauri::command(async)]
pub fn view_index_of(state: State<'_, AppState>, view: u32, id: String) -> CmdResult<Option<u64>> {
    let a = convert::id(&id, "asset")?;
    state.with_view(view, |v| {
        v.ids.iter().position(|x| *x == a).map(|p| p as u64)
    })
}

#[tauri::command(async)]
pub fn calendar(
    state: State<'_, AppState>,
    query: Query,
    level: CalendarLevelDto,
) -> CmdResult<Vec<Bucket>> {
    let open = state.open()?;
    let q = convert::query(&query)?;
    let buckets = open
        .cat()?
        .read(|tx| tx.browse().calendar(&q, convert::level(level)))
        .map_err(err)?;
    Ok(buckets
        .into_iter()
        .map(|b| Bucket {
            cover: convert::cached(&open.cache, b.cover_hash.as_ref(), Variant::Thumb256),
            cover_id: b.cover.map(|c| c.to_string()),
            key: b.key,
            count: b.count,
        })
        .collect())
}

#[tauri::command(async)]
pub fn facets(state: State<'_, AppState>) -> CmdResult<Facets> {
    let open = state.open()?;
    let cat = open.cat()?;
    let f = |c| -> CmdResult<Vec<FacetItem>> {
        Ok(cat
            .read(|tx| tx.browse().facets(c))
            .map_err(err)?
            .into_iter()
            .map(|f| FacetItem {
                value: f.value,
                count: f.count,
            })
            .collect())
    };
    Ok(Facets {
        cameras: f(FacetColumn::Camera)?,
        lenses: f(FacetColumn::Lens)?,
    })
}

#[tauri::command(async)]
pub fn sidebar(state: State<'_, AppState>) -> CmdResult<Sidebar> {
    let open = state.open()?;
    let cat = open.cat()?;
    let online = open.online_volumes(&cat);
    let vcount: std::collections::HashMap<_, _> = cat
        .read(|tx| tx.browse().volume_counts())
        .map_err(err)?
        .into_iter()
        .collect();
    let ecount: std::collections::HashMap<_, _> = cat
        .read(|tx| tx.browse().event_counts())
        .map_err(err)?
        .into_iter()
        .collect();
    let scount: std::collections::HashMap<_, _> = cat
        .read(|tx| tx.browse().session_counts())
        .map_err(err)?
        .into_iter()
        .collect();
    let records = open.lib.volumes().map_err(err)?;
    let label_of = |id| {
        records
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.label.clone())
            .unwrap_or_else(|| "?".into())
    };
    let volumes = cat
        .read(|tx| tx.volumes().list())
        .map_err(err)?
        .into_iter()
        .filter(|v| vcount.get(&v.id).copied().unwrap_or(0) > 0)
        .map(|v| VolumeItem {
            id: v.id.to_string(),
            online: online.contains(&v.id),
            mount: keepsake_core::verify::mount_of(&cat, &open.lib, v.id)
                .map(|m| convert::path_text(&m)),
            count: vcount.get(&v.id).copied().unwrap_or(0),
            label: v.label,
        })
        .collect();
    let events = cat
        .read(|tx| tx.events().list())
        .map_err(err)?
        .into_iter()
        .map(|e| EventItem {
            id: e.id.to_string(),
            count: ecount.get(&e.id).copied().unwrap_or(0),
            title: e.title,
        })
        .filter(|e| e.count > 0)
        .collect();
    let mut imports: Vec<ImportItem> = cat
        .read(|tx| tx.imports().list())
        .map_err(err)?
        .into_iter()
        .map(|s| ImportItem {
            id: s.id.to_string(),
            started_ms: s.started_ms,
            status: s.status.as_str().to_owned(),
            source: label_of(s.source_volume_id),
            safe_to_format: s.safe_to_format,
            count: scount.get(&s.id).copied().unwrap_or(0),
        })
        .collect();
    imports.sort_by_key(|i| std::cmp::Reverse(i.started_ms));
    imports.truncate(30);
    Ok(Sidebar {
        events,
        volumes,
        imports,
    })
}

#[tauri::command(async)]
pub fn details(state: State<'_, AppState>, id: String) -> CmdResult<Details> {
    let open = state.open()?;
    let a_id = convert::id(&id, "asset")?;
    let cat = open.cat()?;
    let a = cat.read(|tx| tx.assets().require(a_id)).map_err(err)?;
    let files = cat.read(|tx| tx.files().for_asset(a_id)).map_err(err)?;
    let tags = cat.read(|tx| tx.tags().for_asset(a_id)).map_err(err)?;
    let event = match a.event_id {
        Some(e) => cat
            .read(|tx| tx.events().get(e))
            .map_err(err)?
            .map(|e| e.title),
        None => None,
    };
    let online = open.online_volumes(&cat);
    let records = open.lib.volumes().map_err(err)?;
    let mut items = Vec::new();
    let mut name = None;
    let mut raw = false;
    for f in &files {
        let mount = online
            .contains(&f.volume_id)
            .then(|| keepsake_core::verify::mount_of(&cat, &open.lib, f.volume_id))
            .flatten();
        if f.role == FileRole::Original && name.is_none() {
            name = Some(f.rel_path.file_name().to_owned());
        }
        if matches!(f.role, FileRole::Original | FileRole::RawPair)
            && keepsake_core::render::source::lower_ext(std::path::Path::new(f.rel_path.as_str()))
                == "arw"
        {
            raw = true;
        }
        items.push(FileItem {
            path: match &mount {
                Some(m) => convert::path_text(&f.rel_path.to_fs_path(m)),
                None => f.rel_path.as_str().to_owned(),
            },
            volume: records
                .iter()
                .find(|r| r.id == f.volume_id)
                .map(|r| r.label.clone())
                .unwrap_or_default(),
            role: f.role.as_str().to_owned(),
            size: f.size_bytes,
            online: mount.is_some(),
        });
    }
    drop(cat);
    // Lookups may record a last use (a write): their own connection.
    let mut cat = open.writer()?;
    let preview = open
        .cache
        .lookup(&mut cat, a_id, Variant::Preview2560)
        .map_err(err)?
        .map(|p| convert::path_text(&p));
    let proxy = open
        .cache
        .lookup_proxy(&mut cat, a_id)
        .map_err(err)?
        .map(|p| convert::path_text(&p));
    Ok(Details {
        id,
        name,
        kind: convert::kind(a.media_kind, raw),
        captured: convert::local_time(
            a.capture.map(|c| c.utc_ms),
            a.capture.and_then(|c| c.utc_offset_min),
        ),
        capture_source: a.capture_source.map(|s| s.as_str().to_owned()),
        confidence: a.capture_confidence.map(|c| c.as_str().to_owned()),
        camera_make: a.camera_make,
        camera_model: a.camera_model,
        lens: a.lens_model,
        duration_ms: a.duration_ms,
        rating: a.rating.get(),
        label: a.label,
        comment: a.comment,
        keywords: tags
            .iter()
            .filter(|t| t.source == TagSource::User)
            .map(|t| t.path.to_lightroom())
            .collect(),
        gps: a.gps.map(|g| GpsItem {
            lat: g.lat,
            lon: g.lon,
        }),
        event,
        online: files
            .iter()
            .any(|f| f.role == FileRole::Original && online.contains(&f.volume_id)),
        files: items,
        preview,
        proxy,
    })
}
