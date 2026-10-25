pub mod api;
mod background;
mod commands;
mod convert;
mod edit;
mod import;
mod render;
mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init()
        .ok();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .setup(|app| {
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
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("Keepsake could not start: {e}");
        std::process::exit(1);
    }
}
