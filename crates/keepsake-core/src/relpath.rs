//! Volume-relative paths.
//!
//! Every path stored in the catalog is relative to a volume root. A [`RelPath`]
//! keeps the file's exact on-disk spelling (so it can always be reopened, even on
//! NTFS where NFC and NFD spellings are different files) and exposes a
//! normalised [`RelPath::key`] used for identity and duplicate detection.
//!
//! Canonical form: components joined with `/`, no leading or trailing
//! separator, no empty, `.` or `..` components, no drive letters or UNC
//! prefixes. Input may use `\` or `/` as separators; a backslash is never
//! allowed inside a component because such a name cannot exist on Windows and
//! would break library portability.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

/// Why a path could not be turned into a [`RelPath`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathError {
    #[error("path is empty")]
    Empty,
    #[error("path is absolute or has a drive/UNC prefix: {0}")]
    Absolute(String),
    #[error("path contains a '.' or '..' component: {0}")]
    DotComponent(String),
    #[error("path contains a NUL character")]
    Nul,
    #[error("file name is not valid UTF-8: {0}")]
    NonUtf8(String),
    #[error("path {path} is not inside {root}")]
    NotUnderRoot { path: String, root: String },
}

/// Whether a volume's file system treats names differing only in case as the same file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSensitivity {
    Sensitive,
    Insensitive,
}

impl CaseSensitivity {
    pub fn from_bool(case_sensitive: bool) -> Self {
        if case_sensitive {
            Self::Sensitive
        } else {
            Self::Insensitive
        }
    }

    pub fn is_sensitive(self) -> bool {
        matches!(self, Self::Sensitive)
    }
}

/// A validated, volume-relative path in canonical `/`-separated form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelPath(String);

impl RelPath {
    /// Parse a relative path written with either `/` or `\` separators.
    ///
    /// Repeated separators are collapsed. Leading separators, drive letters
    /// (`C:`), UNC prefixes and `.`/`..` components are rejected.
    pub fn parse(input: &str) -> Result<Self, PathError> {
        if input.contains('\0') {
            return Err(PathError::Nul);
        }
        if input.starts_with('/') || input.starts_with('\\') || has_drive_prefix(input) {
            return Err(PathError::Absolute(input.to_owned()));
        }
        let mut parts = Vec::new();
        for comp in input.split(['/', '\\']) {
            if comp.is_empty() {
                continue;
            }
            if comp == "." || comp == ".." {
                return Err(PathError::DotComponent(input.to_owned()));
            }
            parts.push(comp);
        }
        if parts.is_empty() {
            return Err(PathError::Empty);
        }
        Ok(Self(parts.join("/")))
    }

    /// Build from an absolute file-system path located under `root`.
    pub fn from_fs(root: &Path, full: &Path) -> Result<Self, PathError> {
        let rel = full
            .strip_prefix(root)
            .map_err(|_| PathError::NotUnderRoot {
                path: full.display().to_string(),
                root: root.display().to_string(),
            })?;
        let mut parts = Vec::new();
        for comp in rel.components() {
            match comp {
                Component::Normal(os) => {
                    let s = os
                        .to_str()
                        .ok_or_else(|| PathError::NonUtf8(os.to_string_lossy().into_owned()))?;
                    if s.contains('\\') || s.contains('/') {
                        // A macOS file name containing a backslash: not portable.
                        return Err(PathError::Absolute(s.to_owned()));
                    }
                    parts.push(s.to_owned());
                }
                Component::CurDir | Component::ParentDir => {
                    return Err(PathError::DotComponent(rel.display().to_string()))
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(PathError::Absolute(rel.display().to_string()))
                }
            }
        }
        if parts.is_empty() {
            return Err(PathError::Empty);
        }
        Self::parse(&parts.join("/"))
    }

    /// The canonical on-disk spelling, `/`-separated.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Resolve against a mount point using the host OS separator.
    pub fn to_fs_path(&self, root: &Path) -> PathBuf {
        let mut p = root.to_path_buf();
        for c in self.components() {
            p.push(c);
        }
        p
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// Last component (the file name) in its on-disk spelling.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// The parent directory, or `None` for a top-level file.
    pub fn parent(&self) -> Option<RelPath> {
        self.0.rfind('/').map(|i| RelPath(self.0[..i].to_owned()))
    }

    /// Append a single file name (no separators allowed in `name`).
    pub fn join(&self, name: &str) -> Result<RelPath, PathError> {
        RelPath::parse(&format!("{}/{}", self.0, name))
    }

    /// Return a sibling path with `suffix` appended to the file name
    /// (e.g. `.xmp` for a sidecar).
    pub fn with_appended_suffix(&self, suffix: &str) -> Result<RelPath, PathError> {
        RelPath::parse(&format!("{}{}", self.0, suffix))
    }

    /// Identity key: NFC, and lowercased for case-insensitive volumes.
    pub fn key(&self, case: CaseSensitivity) -> String {
        path_key(&self.0, case)
    }

    /// The file name in NFC (for display and search).
    pub fn file_name_nfc(&self) -> String {
        nfc(self.file_name())
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for RelPath {
    type Error = PathError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        RelPath::parse(&value)
    }
}

impl From<RelPath> for String {
    fn from(value: RelPath) -> Self {
        value.0
    }
}

/// NFC-normalise a string.
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Normalised identity key for a name or path.
///
/// NFC first, then (for case-insensitive volumes) Unicode lowercase, then NFC
/// again because lowercasing can produce combining sequences.
pub fn path_key(s: &str, case: CaseSensitivity) -> String {
    let n = nfc(s);
    match case {
        CaseSensitivity::Sensitive => n,
        CaseSensitivity::Insensitive => nfc(&n.to_lowercase()),
    }
}

fn has_drive_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_separators() {
        let a = RelPath::parse("DCIM\\100MSDCF\\DSC00001.JPG").unwrap();
        let b = RelPath::parse("DCIM/100MSDCF/DSC00001.JPG").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "DCIM/100MSDCF/DSC00001.JPG");
        assert_eq!(a.file_name(), "DSC00001.JPG");
        assert_eq!(a.parent().unwrap().as_str(), "DCIM/100MSDCF");
    }

    #[test]
    fn rejects_bad_paths() {
        for bad in [
            "",
            "/",
            "/abs",
            "\\abs",
            "\\\\server\\share\\x",
            "C:\\x",
            "c:x",
            "a/../b",
            "./a",
            "a/.",
            "a\0b",
        ] {
            assert!(RelPath::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn nfd_and_nfc_share_a_key_but_keep_spelling() {
        let nfc_p = RelPath::parse("Caf\u{e9}/x.jpg").unwrap();
        let nfd_p = RelPath::parse("Cafe\u{301}/x.jpg").unwrap();
        assert_ne!(nfc_p.as_str(), nfd_p.as_str());
        assert_eq!(
            nfc_p.key(CaseSensitivity::Sensitive),
            nfd_p.key(CaseSensitivity::Sensitive)
        );
    }

    #[test]
    fn case_key() {
        let a = RelPath::parse("DCIM/DSC00001.JPG").unwrap();
        let b = RelPath::parse("dcim/dsc00001.jpg").unwrap();
        assert_eq!(
            a.key(CaseSensitivity::Insensitive),
            b.key(CaseSensitivity::Insensitive)
        );
        assert_ne!(
            a.key(CaseSensitivity::Sensitive),
            b.key(CaseSensitivity::Sensitive)
        );
    }

    #[test]
    fn fs_round_trip() {
        let root = Path::new("vol");
        let p = RelPath::parse("PRIVATE/M4ROOT/CLIP/C0001.MP4").unwrap();
        let full = p.to_fs_path(root);
        assert_eq!(RelPath::from_fs(root, &full).unwrap(), p);
    }
}
