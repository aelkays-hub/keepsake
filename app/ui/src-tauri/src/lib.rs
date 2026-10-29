pub mod api;
mod background;
mod commands;
mod convert;
mod diagnostics;
mod edit;
mod folders;
mod import;
mod maintenance;
mod prefs;
mod render;
mod state;
mod viewer;

use tauri::Manager;

/// `keepsake-app --self-check [out.json]`: find and run every bundled tool,
/// write what was found (JSON, to the file or stdout) and exit 0 if all
/// work. Used by the release smoke test on the installed app.
fn self_check(out: Option<&String>) -> i32 {
    let tools = keepsake_core::diag::tools(&keepsake_core::render::RenderTools::discover());
    let ok = tools.iter().all(|t| t.ok());
    let report = serde_json::json!({
        "version": keepsake_core::version(),
        "ok": ok,
        "tools": tools,
    });
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    match out {
        Some(p) => {
            if std::fs::write(p, text).is_err() {
                return 2;
            }
        }
        None => println!("{text}"),
    }
    i32::from(!ok)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diagnostics::mark_start();
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--self-check") {
        std::process::exit(self_check(args.get(2)));
    }
    diagnostics::init();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .setup(|app| {
            // The installed app's tools are in its resource folder, laid out
            // like vendor/ (tauri.windows.conf.json).
            if let Ok(dir) = app.path().resource_dir() {
                if keepsake_core::probe::ToolPaths::in_vendor(&dir)
                    .ffprobe
                    .is_file()
                {
                    keepsake_core::probe::tools::set_bundled(dir);
                }
            }
            // The main window is made here (not from the config) so the UI
            // tests can give WebView2 a debugging port.
            let conf = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or("no window in tauri.conf.json")?;
            let mut main = tauri::WebviewWindowBuilder::from_config(app.handle(), &conf)?;
            if let Some(args) = state::browser_args() {
                main = main.additional_browser_args(&args);
            }
            main.build()?;
            commands::open_default(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
            commands::app_info,
            commands::library_create,
            commands::library_open,
            commands::view_open,
            commands::view_rows,
            commands::view_index_of,
            commands::calendar,
            commands::facets,
            commands::sidebar,
            commands::details,
            edit::edit_rating,
            edit::edit_label,
            edit::edit_comment,
            edit::edit_keywords,
            edit::keywords,
            render::render_status,
            render::render_set_visible,
            render::render_start,
            render::render_stop,
            import::import_scan,
            import::card_thumb,
            import::presets_list,
            import::preset_save,
            import::import_check,
            import::import_start,
            import::import_cancel,
            viewer::full_image,
            viewer::asset_members,
            viewer::video_info,
            viewer::open_external,
            viewer::metadata_all,
            viewer::open_choices,
            edit::time_shift_preview,
            edit::time_shift_apply,
            maintenance::backup_overview,
            maintenance::backup_run,
            maintenance::verify_run,
            maintenance::maint_cancel,
            folders::folder_add,
            prefs::settings_get,
            prefs::settings_set,
            prefs::cache_remove_old,
            prefs::preset_delete,
            diagnostics::diagnostic_report,
            diagnostics::open_log_folder,
            diagnostics::log_ui_error,
            diagnostics::crash_notice,
            diagnostics::crash_dismiss,
            diagnostics::ui_ready,
            diagnostics::startup_ms,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("Keepsake could not start: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn self_check_writes_its_report_where_asked() {
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("self-check.json");
        let code = super::self_check(Some(&out.to_string_lossy().into_owned()));
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["version"], keepsake_core::version());
        assert_eq!(v["tools"].as_array().unwrap().len(), 4);
        assert_eq!(code == 0, v["ok"].as_bool().unwrap());
    }
}
