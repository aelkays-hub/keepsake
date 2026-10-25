//! The Settings screen: cache location and size, render workers, "Open in"
//! applications, and import presets. Saved to `config.toml`; the render
//! service restarts with the new values.

use std::path::{Path, PathBuf};

use keepsake_core::presets;
use keepsake_core::render::Cache;
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Manager, State};

use crate::api::{OpenApp, SettingsInfo, SettingsSaved, SettingsUpdate};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// Smallest cache limit accepted (256 MiB).
const MIN_CACHE: u64 = 256 << 20;

fn info(state: &AppState, s: &Settings) -> CmdResult<SettingsInfo> {
    let cache = s.render.cache_root().map_err(err)?;
    let open = state.try_open();
    let cache_bytes = match &open {
        Some(o) => o.cat().read(|tx| tx.renditions().totals()).map_err(err)?.1,
        None => 0,
    };
    Ok(SettingsInfo {
        library: open.as_ref().map(|o| convert::path_text(o.root())),
        cache_dir: convert::path_text(&cache),
        cache_dir_custom: s.render.cache_dir.is_some(),
        cache_max_bytes: s.render.max_bytes,
        cache_bytes,
        workers: s.render.workers.map(|w| w as u32),
        workers_default: keepsake_core::settings::default_workers() as u32,
        proxy_workers: s.render.proxy_workers as u32,
        pause_on_battery: s.render.pause_on_battery,
        open_apps: s
            .open
            .apps
            .iter()
            .map(|(ext, app)| OpenApp {
                ext: ext.clone(),
                app: app.clone(),
            })
            .collect(),
        config_file: convert::path_text(&Settings::path().map_err(err)?),
    })
}

#[tauri::command(async)]
pub fn settings_get(state: State<'_, AppState>) -> CmdResult<SettingsInfo> {
    info(&state, &Settings::load().map_err(err)?)
}

/// `.ARW`, ` arw ` → `arw`.
fn ext_key(e: &str) -> String {
    e.trim().trim_start_matches('.').to_ascii_lowercase()
}

/// Apply `u` to `s`, rejecting values that make no sense.
pub fn apply_update(s: &mut Settings, u: &SettingsUpdate) -> CmdResult<()> {
    if u.cache_max_bytes < MIN_CACHE {
        return Err("the cache limit must be at least 256 MB".into());
    }
    if u.workers.is_some_and(|w| !(1..=16).contains(&w)) {
        return Err("image workers must be between 1 and 16".into());
    }
    if !(1..=4).contains(&u.proxy_workers) {
        return Err("video workers must be between 1 and 4".into());
    }
    s.render.cache_dir = u
        .cache_dir
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    if let Some(d) = &s.render.cache_dir {
        if !d.is_absolute() {
            return Err("choose a full path for the cache folder".into());
        }
    }
    s.render.max_bytes = u.cache_max_bytes;
    s.render.workers = u.workers.map(|w| w as usize);
    s.render.proxy_workers = u.proxy_workers as usize;
    s.render.pause_on_battery = u.pause_on_battery;
    s.open.apps.clear();
    for a in &u.open_apps {
        let ext = ext_key(&a.ext);
        let app = a.app.trim();
        if ext.is_empty() || app.is_empty() {
            continue;
        }
        if ext.contains(['/', '\\', ' ']) {
            return Err(format!("not a file type: {}", a.ext));
        }
        s.open.apps.insert(ext, app.to_owned());
    }
    Ok(())
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let c = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    c(a) == c(b)
}

#[tauri::command(async)]
pub fn settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    update: SettingsUpdate,
) -> CmdResult<SettingsSaved> {
    let mut s = Settings::load().map_err(err)?;
    let old_cache = s.render.cache_root().map_err(err)?;
    apply_update(&mut s, &update)?;
    let new_cache = s.render.cache_root().map_err(err)?;
    std::fs::create_dir_all(&new_cache)
        .map_err(|e| format!("cannot use {}: {e}", new_cache.display()))?;
    s.save().map_err(err)?;
    if state.try_open().is_some() {
        let open = state.reload_settings()?;
        app.asset_protocol_scope()
            .allow_directory(open.cache_root(), true)
            .map_err(err)?;
        crate::render::restart_service(&app)?;
    }
    let moved = !same_dir(&old_cache, &new_cache) && old_cache.is_dir();
    Ok(SettingsSaved {
        settings: info(&state, &s)?,
        old_cache: moved.then(|| convert::path_text(&old_cache)),
    })
}

/// Delete Keepsake's files from a cache folder no longer in use. Returns
/// the bytes freed.
#[tauri::command(async)]
pub fn cache_remove_old(path: String) -> CmdResult<u64> {
    let old = PathBuf::from(&path);
    let current = Settings::load()
        .map_err(err)?
        .render
        .cache_root()
        .map_err(err)?;
    if same_dir(&old, &current) {
        return Err("that is the cache in use".into());
    }
    Cache::new(old).remove_files().map_err(err)
}

#[tauri::command(async)]
pub fn preset_delete(state: State<'_, AppState>, name: String) -> CmdResult<bool> {
    let open = state.open()?;
    presets::delete(&open.lib, &name).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update() -> SettingsUpdate {
        SettingsUpdate {
            cache_dir: None,
            cache_max_bytes: 1 << 30,
            workers: Some(2),
            proxy_workers: 1,
            pause_on_battery: false,
            open_apps: vec![
                OpenApp {
                    ext: ".ARW".into(),
                    app: r"C:\Apps\Raw.exe".into(),
                },
                OpenApp {
                    ext: "mp4".into(),
                    app: "  ".into(),
                },
            ],
        }
    }

    #[test]
    fn updates_are_checked_and_normalised() {
        let mut s = Settings::default();
        apply_update(&mut s, &update()).unwrap();
        assert_eq!(s.render.workers, Some(2));
        assert!(!s.render.pause_on_battery);
        assert_eq!(
            s.open.apps.get("arw").map(String::as_str),
            Some(r"C:\Apps\Raw.exe")
        );
        assert!(!s.open.apps.contains_key("mp4"), "blank app: default");
        let mut bad = update();
        bad.cache_max_bytes = 1;
        assert!(apply_update(&mut s, &bad).is_err());
        let mut bad = update();
        bad.workers = Some(0);
        assert!(apply_update(&mut s, &bad).is_err());
        let mut bad = update();
        bad.cache_dir = Some("relative/cache".into());
        assert!(apply_update(&mut s, &bad).is_err());
    }
}
