//! Background work for the open library: the render service, and the
//! device watcher, which announces Sony cards (`device://card`) and opens
//! the import window.
//!
//! Tests replace the OS device backend with a script of fake events
//! (`KEEPSAKE_FAKE_EVENTS=<file.jsonl>`, as in `keepsake watch`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;

use keepsake_core::watch::{WatchEvent, WatchOptions, Watcher};
use keepsake_platform::{DeviceEvents, FakeDevices};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::api::CardNotice;
use crate::convert;
use crate::state::AppState;

pub fn start(app: &AppHandle, state: &AppState) {
    if let Err(e) = crate::render::start_service(app) {
        tracing::warn!("render service: {e}");
    }
    if !state.watcher_started.swap(true, Ordering::SeqCst) {
        start_watcher(app.clone());
    }
}

/// Open (or focus) the import window for the card at `mount`.
pub fn open_import_window(app: &AppHandle, mount: &str) {
    let url = format!("index.html#/import?mount={}", urlencode(mount));
    if let Some(w) = app.get_webview_window("import") {
        let _ = w.emit("import://open", mount.to_owned());
        let _ = w.set_focus();
        return;
    }
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let mut b = WebviewWindowBuilder::new(&app2, "import", WebviewUrl::App(url.into()))
            .title("Import")
            .inner_size(1040.0, 780.0)
            .min_inner_size(760.0, 560.0);
        // Same WebView2 arguments as the main window (they share a profile).
        if let Some(args) = crate::state::browser_args() {
            b = b.additional_browser_args(&args);
        }
        let built = b.build();
        if let Err(e) = built {
            tracing::error!("import window: {e}");
        }
    });
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn start_watcher(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("device-watcher".into())
        .spawn(move || {
            let Some(open) = app.state::<AppState>().try_open() else {
                return;
            };
            let mut backend: Box<dyn DeviceEvents> = match std::env::var_os("KEEPSAKE_FAKE_EVENTS")
            {
                Some(p) => match FakeDevices::from_jsonl(std::path::Path::new(&p)) {
                    Ok(f) => Box::new(f),
                    Err(e) => {
                        tracing::error!("fake events: {e}");
                        return;
                    }
                },
                None => match keepsake_platform::system_devices() {
                    Ok(d) => Box::new(d),
                    Err(e) => {
                        tracing::warn!("device watcher unavailable: {e}");
                        return;
                    }
                },
            };
            let (tx, rx) = channel();
            if let Err(e) = backend.start(tx) {
                tracing::warn!("device watcher: {e}");
                return;
            }
            let lib = open.lib.clone();
            let mut watcher = Watcher::new(&lib, None, WatchOptions::default());
            let never = AtomicBool::new(false);
            for ev in rx {
                watcher.handle(&ev, &never, &mut |w| match w {
                    WatchEvent::CardInserted {
                        card,
                        label,
                        mount,
                        known,
                        items,
                        new_items,
                        preset,
                        ..
                    } => {
                        let mount = convert::path_text(mount);
                        let _ = app.emit(
                            "device://card",
                            CardNotice {
                                card: card.to_string(),
                                label: label.clone(),
                                mount: mount.clone(),
                                known: *known,
                                items: *items as u64,
                                new_items: *new_items as u64,
                                preset: preset.clone(),
                                removed: false,
                            },
                        );
                        if *new_items > 0 {
                            open_import_window(&app, &mount);
                        }
                    }
                    WatchEvent::CardRemoved { card, label, mount } => {
                        let _ = app.emit(
                            "device://card",
                            CardNotice {
                                card: card.to_string(),
                                label: label.clone(),
                                mount: convert::path_text(mount),
                                known: true,
                                items: 0,
                                new_items: 0,
                                preset: None,
                                removed: true,
                            },
                        );
                    }
                    _ => {}
                });
            }
        });
}
