//! App state shared by commands: the open library, a read connection, the
//! open view snapshots, and handles to background work.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use keepsake_core::catalog::{AssetId, Catalog, VolumeId};
use keepsake_core::library::Library;
use keepsake_core::render::Cache;
use keepsake_core::settings::Settings;

pub type CmdResult<T> = Result<T, String>;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// A filtered, sorted snapshot of asset ids.
pub struct View {
    pub ids: Vec<AssetId>,
}

/// The open library and what goes with it.
pub struct Open {
    pub lib: Library,
    pub cache: Cache,
    pub settings: Settings,
    /// For reads and small writes from commands.
    pub cat: Mutex<Catalog>,
}

#[derive(Default)]
pub struct AppState {
    open: Mutex<Option<Arc<Open>>>,
    views: Mutex<HashMap<u32, View>>,
    next_view: Mutex<u32>,
    /// Stop flag of the running import, if any.
    pub import_cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// An import is running.
    pub import_running: AtomicBool,
    /// The device watcher has been started for this process.
    pub watcher_started: AtomicBool,
    pub render: crate::render::RenderState,
}

/// Running under the UI tests: allows destinations on one disk.
pub fn ui_test() -> bool {
    std::env::var_os("KEEPSAKE_UI_TEST").is_some_and(|v| v == "1")
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl AppState {
    /// Open `lib` as the current library (closing views of the previous one).
    pub fn set_library(&self, lib: Library) -> CmdResult<Arc<Open>> {
        let settings = Settings::load().map_err(err)?;
        let cache = Cache::new(settings.render.cache_root().map_err(err)?);
        std::fs::create_dir_all(cache.root()).map_err(err)?;
        let cat = lib.open_catalog().map_err(err)?;
        let open = Arc::new(Open {
            lib,
            cache,
            settings,
            cat: Mutex::new(cat),
        });
        *lock(&self.open) = Some(Arc::clone(&open));
        lock(&self.views).clear();
        Ok(open)
    }

    pub fn open(&self) -> CmdResult<Arc<Open>> {
        lock(&self.open)
            .clone()
            .ok_or_else(|| "no library is open".to_owned())
    }

    pub fn try_open(&self) -> Option<Arc<Open>> {
        lock(&self.open).clone()
    }

    pub fn add_view(&self, v: View) -> u32 {
        let mut n = lock(&self.next_view);
        *n += 1;
        let id = *n;
        let mut views = lock(&self.views);
        // Keep a few views: the grid, the details table, the calendar.
        if views.len() >= 8 {
            if let Some(oldest) = views.keys().min().copied() {
                views.remove(&oldest);
            }
        }
        views.insert(id, v);
        id
    }

    pub fn with_view<T>(&self, id: u32, f: impl FnOnce(&View) -> T) -> CmdResult<T> {
        let views = lock(&self.views);
        let v = views
            .get(&id)
            .ok_or_else(|| format!("view {id} is closed"))?;
        Ok(f(v))
    }
}

impl Open {
    /// The shared connection, for reads. Commands that write use
    /// [`Self::writer`] instead, so a slow write (waiting on the render
    /// service, say) never holds up reads such as scrolling the grid.
    pub fn cat(&self) -> MutexGuard<'_, Catalog> {
        lock(&self.cat)
    }

    /// A fresh connection for a command that writes.
    pub fn writer(&self) -> CmdResult<Catalog> {
        self.lib.open_catalog().map_err(err)
    }

    /// Volumes mounted right now.
    pub fn online_volumes(&self, cat: &Catalog) -> HashSet<VolumeId> {
        cat.read(|tx| tx.volumes().list())
            .unwrap_or_default()
            .into_iter()
            .filter(|v| keepsake_core::verify::mount_of(cat, &self.lib, v.id).is_some())
            .map(|v| v.id)
            .collect()
    }

    pub fn root(&self) -> &Path {
        self.lib.root()
    }

    pub fn cache_root(&self) -> PathBuf {
        self.cache.root().to_path_buf()
    }
}
