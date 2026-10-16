/// Returns the application version string.
#[tauri::command]
pub fn get_version() -> &'static str {
    keepsake_core::version()
}
