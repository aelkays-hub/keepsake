/// Minimal stub — full implementation lands in prompt 1 (device watcher).
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub label: String,
    pub mount_path: std::path::PathBuf,
}

/// Platform-agnostic trait for enumerating currently-mounted volumes.
pub trait VolumeEnumerator {
    fn list_volumes(&self) -> Result<Vec<VolumeInfo>, keepsake_core::error::CoreError>;
}
