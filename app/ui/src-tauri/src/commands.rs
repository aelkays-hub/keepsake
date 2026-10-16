/// Returns the application version string.
#[tauri::command]
pub fn get_version() -> &'static str {
    keepsake_core::version()
}

/// Recursively list image/video files under `dir` for the spike grid test.
///
/// Returns paths with forward slashes. Only used during the spike; will be
/// replaced by a proper catalog query when the ingest engine is built.
#[tauri::command]
pub fn spike_list_files(dir: String) -> Result<Vec<String>, String> {
    let extensions = ["jpg", "jpeg", "png", "mp4", "mov", "arw", "heif", "hif"];
    let mut paths = Vec::new();
    collect_paths(std::path::Path::new(&dir), &extensions, &mut paths)
        .map_err(|e| e.to_string())?;
    paths.sort();
    Ok(paths)
}

fn collect_paths(
    dir: &std::path::Path,
    exts: &[&str],
    out: &mut Vec<String>,
) -> std::io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_paths(&path, exts, out)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if exts.contains(&ext.to_ascii_lowercase().as_str()) {
                out.push(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}
