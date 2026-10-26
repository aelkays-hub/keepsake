pub mod backup;
pub mod cards;
pub mod catalog;
pub mod corpus;
pub mod devices;
pub mod diag;
pub mod error;
pub mod external;
pub mod grouping;
pub mod history;
pub mod ingest;
pub mod jobs;
pub mod library;
pub mod logs;
pub mod metadata;
pub mod paths;
pub mod presets;
pub mod probe;
pub mod rebuild;
pub mod relpath;
pub mod render;
pub mod seed;
pub mod select;
pub mod settings;
pub mod verify;
pub mod watch;
pub mod xmp;

/// Returns the application version string defined in Cargo.toml.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver() {
        let v = version();
        // Must have exactly two dots (MAJOR.MINOR.PATCH).
        assert_eq!(
            v.chars().filter(|c| *c == '.').count(),
            2,
            "version '{v}' is not semver"
        );
    }
}
