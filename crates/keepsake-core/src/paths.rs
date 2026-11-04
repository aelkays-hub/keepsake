use std::path::PathBuf;

/// Directory where Keepsake stores its catalog and all persistent application data.
///
/// | Platform | Path |
/// |---|---|
/// | Windows | `%LOCALAPPDATA%\keepsake` (e.g. `C:\Users\<user>\AppData\Local\keepsake`) |
/// | macOS   | `~/Library/Application Support/keepsake` |
pub fn data_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("keepsake"))
}

/// Directory where Keepsake writes regenerable cache files (thumbnails, proxies).
///
/// | Platform | Path |
/// |---|---|
/// | Windows | `%LOCALAPPDATA%\keepsake\cache` |
/// | macOS   | `~/Library/Caches/keepsake` |
pub fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("keepsake"))
}

/// Directory where Keepsake reads and writes user configuration.
///
/// | Platform | Path |
/// |---|---|
/// | Windows | `%APPDATA%\keepsake` (e.g. `C:\Users\<user>\AppData\Roaming\keepsake`) |
/// | macOS   | `~/Library/Application Support/keepsake` |
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("keepsake"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_is_some_and_ends_with_keepsake() {
        let d = data_dir().expect("data_dir() must return Some on CI");
        assert_eq!(d.file_name().unwrap(), "keepsake");
    }

    #[test]
    fn cache_dir_is_some_and_ends_with_keepsake() {
        let d = cache_dir().expect("cache_dir() must return Some on CI");
        assert_eq!(d.file_name().unwrap(), "keepsake");
    }

    #[test]
    fn config_dir_is_some_and_ends_with_keepsake() {
        let d = config_dir().expect("config_dir() must return Some on CI");
        assert_eq!(d.file_name().unwrap(), "keepsake");
    }
}
