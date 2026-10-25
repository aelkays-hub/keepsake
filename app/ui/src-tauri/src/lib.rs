mod commands;
mod render;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(render::RenderState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
            commands::spike_list_files,
            render::render_status,
            render::render_set_visible,
            render::render_lookup,
            render::render_start,
            render::render_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keepsake");
}
