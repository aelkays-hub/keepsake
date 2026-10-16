mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
            commands::spike_list_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keepsake");
}
