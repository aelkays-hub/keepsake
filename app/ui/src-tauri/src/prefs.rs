//! The Settings screen: cache location and size, render workers, "Open in"
//! applications, and import presets. Saved to `config.toml`; the render
//! service restarts with the new values.

use std::path::{Path, PathBuf};

use keepsake_core::ingest::template::Template;
use keepsake_core::presets;
use keepsake_core::render::Cache;
use keepsake_core::settings::{OpenApps, Settings};
use tauri::{AppHandle, Manager, State};

use crate::api::{ImportPrefs, OpenApp, SettingsInfo, SettingsSaved, SettingsUpdate};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// Smallest cache limit accepted (256 MiB).
const MIN_CACHE: u64 = 256 << 20;

fn info(state: &AppState, s: &Settings) -> CmdResult<SettingsInfo> {
    let cache = s.render.cache_root().map_err(err)?;
    let open = state.try_open();
    let cache_bytes = match &open {
        Some(o) => o.cat()?.read(|tx| tx.renditions().totals()).map_err(err)?.1,
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
        // One row per application, default first within each type.
        open_apps: s
            .open
            .apps
            .iter()
            .flat_map(|(ext, apps)| {
                apps.list().into_iter().map(move |app| OpenApp {
                    ext: ext.clone(),
                    app: app.to_owned(),
                })
            })
            .collect(),
        import: ImportPrefs {
            auto: s.import.auto,
            destination: s.import.destination.as_deref().map(convert::path_text),
            template: s.import.template.clone(),
        },
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
    // Rows in order: several rows for a type make a list, the first being
    // the default. A row may name several types: `arw, jpg, dng`.
    let mut lists: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for a in &u.open_apps {
        let app = a.app.trim();
        if app.is_empty() {
            continue;
        }
        for e in a.ext.split([',', ';']) {
            let ext = ext_key(e);
            if ext.is_empty() {
                continue;
            }
            if ext.contains(['/', '\\', ' ']) {
                return Err(format!("not a file type: {}", e.trim()));
            }
            let list = lists.entry(ext).or_default();
            if !list.iter().any(|x| x.eq_ignore_ascii_case(app)) {
                list.push(app.to_owned());
            }
        }
    }
    s.open.apps = lists
        .into_iter()
        .map(|(ext, apps)| (ext, OpenApps::from_list(apps)))
        .collect();
    // When a card is inserted.
    let template = u.import.template.trim();
    Template::parse(template).map_err(|e| format!("folder layout: {e}"))?;
    let dest = u
        .import
        .destination
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    if let Some(d) = &dest {
        if !d.is_absolute() {
            return Err("choose a full path for where cards are copied".into());
        }
    }
    if u.import.auto && dest.is_none() {
        return Err("choose where cards are copied to import them automatically".into());
    }
    s.import.auto = u.import.auto;
    s.import.destination = dest;
    s.import.template = template.to_owned();
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
            import: ImportPrefs {
                auto: false,
                destination: None,
                template: "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}".into(),
            },
        }
    }

    #[test]
    fn automatic_import_needs_a_full_destination_and_a_valid_layout() {
        let mut s = Settings::default();
        let dest = if cfg!(windows) {
            r"E:\Pictures"
        } else {
            "/Volumes/E/Pictures"
        };
        let mut u = update();
        u.import.auto = true;
        assert!(apply_update(&mut s, &u).is_err(), "no destination");
        u.import.destination = Some("Pictures".into());
        assert!(apply_update(&mut s, &u).is_err(), "not a full path");
        u.import.destination = Some(format!(" {dest} "));
        apply_update(&mut s, &u).unwrap();
        assert!(s.import.auto);
        assert_eq!(s.import.auto_destination(), Some(Path::new(dest)));
        u.import.template = "{yyyy}/{nonsense}".into();
        assert!(apply_update(&mut s, &u).is_err(), "bad layout");
        // Off: the destination is kept for next time.
        let mut u = update();
        u.import.destination = Some(dest.into());
        apply_update(&mut s, &u).unwrap();
        assert!(!s.import.auto);
        assert_eq!(s.import.auto_destination(), None);
        assert_eq!(s.import.destination.as_deref(), Some(Path::new(dest)));
    }

    #[test]
    fn updates_are_checked_and_normalised() {
        let mut s = Settings::default();
        apply_update(&mut s, &update()).unwrap();
        assert_eq!(s.render.workers, Some(2));
        assert!(!s.render.pause_on_battery);
        assert_eq!(
            s.open.apps.get("arw"),
            Some(&OpenApps::One(r"C:\Apps\Raw.exe".into()))
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

    #[test]
    fn several_rows_for_a_type_make_a_list_in_order() {
        let (ps, topaz) = (r"C:\Apps\Photoshop.exe", r"C:\Apps\Topaz Photo.exe");
        let row = |ext: &str, app: &str| OpenApp {
            ext: ext.into(),
            app: app.into(),
        };
        let mut u = update();
        u.open_apps = vec![
            row("arw, .JPG; dng", ps),
            row("arw", topaz),
            row("jpg", topaz),
            row("ARW", ps), // a repeat: ignored
        ];
        let mut s = Settings::default();
        apply_update(&mut s, &u).unwrap();
        let path = |p: &str| std::path::PathBuf::from(p);
        assert_eq!(s.open.apps_for(&path("a.ARW")), [ps, topaz]);
        assert_eq!(s.open.apps_for(&path("a.jpg")), [ps, topaz]);
        assert_eq!(s.open.apps_for(&path("a.dng")), [ps]);
        assert_eq!(s.open.app_for(&path("a.arw")), Some(ps), "first row");
        // Read back as one row per app, default first.
        let back: Vec<(String, String)> = s
            .open
            .apps
            .iter()
            .flat_map(|(e, a)| a.list().into_iter().map(move |x| (e.clone(), x.to_owned())))
            .collect();
        assert_eq!(back[0], ("arw".into(), ps.into()));
        assert_eq!(back[1], ("arw".into(), topaz.into()));
        let mut bad = update();
        bad.open_apps = vec![row("arw, a b", ps)];
        assert!(apply_update(&mut s, &bad).is_err());
    }
}
