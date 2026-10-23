pub mod catalog;
pub mod corpus;
pub mod error;
pub mod grouping;
pub mod library;
pub mod metadata;
pub mod paths;
pub mod probe;
pub mod rebuild;
pub mod relpath;
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
