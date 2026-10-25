//! Rendition commands for the UI. Thin: validate, call keepsake-core.
//!
//! - `render_start` runs a `RenderService` for the default library on a
//!   background thread (never a command thread) and forwards its events as
//!   `render://progress`;
//! - `render_stop` stops it after the current jobs;
//! - `render_status`, `render_set_visible` (the grid reports what is on
//!   screen), `render_lookup` (the path of a rendition, for `convertFileSrc`).

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use keepsake_core::catalog::AssetId;
use keepsake_core::library::Library;
use keepsake_core::render::service::{self, RenderStatus};
use keepsake_core::render::{self, Cache, RenderOptions, RenderService, RenderTools, Variant};
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Emitter, State};

/// The background render service, if running.
#[derive(Default)]
pub struct RenderState {
    cancel: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

fn setup() -> Result<(Library, Settings, Cache), String> {
    let settings = Settings::load().map_err(|e| e.to_string())?;
    let root = settings
        .default_library
        .clone()
        .ok_or("no default library: run `keepsake init <path>`")?;
    let lib = Library::open(&root).map_err(|e| e.to_string())?;
    let cache = Cache::new(settings.render.cache_root().map_err(|e| e.to_string())?);
    Ok((lib, settings, cache))
}

fn parse_assets(ids: &[String]) -> Result<Vec<AssetId>, String> {
    ids.iter()
        .map(|s| AssetId::from_str(s).map_err(|e| format!("bad asset id {s}: {e}")))
        .collect()
}

#[tauri::command]
pub fn render_status() -> Result<RenderStatus, String> {
    let (lib, settings, cache) = setup()?;
    let cat = lib.open_catalog().map_err(|e| e.to_string())?;
    let power: service::PowerCheck = Arc::new(keepsake_platform::on_battery);
    service::status(&cat, &lib, &cache, &settings.render, &power).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn render_set_visible(assets: Vec<String>) -> Result<u64, String> {
    let ids = parse_assets(&assets)?;
    let (lib, _, cache) = setup()?;
    let mut cat = lib.open_catalog().map_err(|e| e.to_string())?;
    render::set_visible(&mut cat, &cache, &ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn render_lookup(asset: String, variant: String) -> Result<Option<String>, String> {
    let id = parse_assets(&[asset])?[0];
    let v = Variant::parse(&variant).ok_or_else(|| format!("unknown variant {variant}"))?;
    let (lib, _, cache) = setup()?;
    let mut cat = lib.open_catalog().map_err(|e| e.to_string())?;
    let p = if v == Variant::Proxy720 || v == Variant::CameraProxy {
        cache.lookup_proxy(&mut cat, id)
    } else {
        cache.lookup(&mut cat, id, v)
    }
    .map_err(|e| e.to_string())?;
    Ok(p.map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn render_start(app: AppHandle, state: State<'_, RenderState>) -> Result<bool, String> {
    let mut slot = state.worker.lock().map_err(|_| "render state poisoned")?;
    if slot.as_ref().is_some_and(|h| !h.is_finished()) {
        return Ok(false);
    }
    let (lib, settings, cache) = setup()?;
    let mut opts = RenderOptions::from_settings(&settings.render);
    opts.until_empty = false;
    let svc = RenderService::new(&lib, RenderTools::discover(), cache, opts)
        .with_power(Arc::new(keepsake_platform::on_battery));
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = Arc::clone(&state.cancel);
    let handle = std::thread::Builder::new()
        .name("render-service".into())
        .spawn(move || {
            if let Ok(mut cat) = lib.open_catalog() {
                let _ = render::enqueue_backlog(&mut cat);
            }
            let result = svc.run(&cancel, &mut |ev| {
                let _ = app.emit("render://progress", ev);
            });
            if let Err(e) = result {
                tracing::error!("render service: {e}");
                let _ = app.emit("render://error", e.to_string());
            }
        })
        .map_err(|e| e.to_string())?;
    *slot = Some(handle);
    Ok(true)
}

#[tauri::command]
pub fn render_stop(state: State<'_, RenderState>) -> Result<(), String> {
    state.cancel.store(true, Ordering::SeqCst);
    Ok(())
}
