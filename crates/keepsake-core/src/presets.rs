//! Saved ingest settings ("presets"), used by trusted cards for automatic
//! imports. Stored in `<library>/presets.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ingest::template::Template;
use crate::library::{write_json_atomic, Library, LibraryError};

/// Named ingest settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    /// One or two destination folders. Two are needed for "safe to format".
    pub destinations: Vec<PathBuf>,
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
}

impl Preset {
    /// Check the preset can be used: a name, one or two distinct
    /// destinations, a valid template.
    pub fn validate(&self) -> Result<(), LibraryError> {
        if self.name.trim().is_empty() {
            return Err(LibraryError::Invalid("a preset needs a name".into()));
        }
        if self.destinations.is_empty() || self.destinations.len() > 2 {
            return Err(LibraryError::Invalid(
                "a preset needs one or two destinations".into(),
            ));
        }
        if self.destinations.len() == 2 && same_path(&self.destinations[0], &self.destinations[1]) {
            return Err(LibraryError::Invalid(
                "the two destinations are the same folder".into(),
            ));
        }
        Template::parse(&self.template)
            .map_err(|e| LibraryError::Invalid(format!("template: {e}")))?;
        Ok(())
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .trim_end_matches(['/', '\\'])
            .to_lowercase()
    };
    norm(a) == norm(b)
}

fn file(lib: &Library) -> PathBuf {
    lib.root().join("presets.json")
}

/// All presets, by name.
pub fn list(lib: &Library) -> Result<Vec<Preset>, LibraryError> {
    match std::fs::read(file(lib)) {
        Ok(b) => Ok(serde_json::from_slice(&b)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

/// The preset called `name` (case-insensitive).
pub fn get(lib: &Library, name: &str) -> Result<Option<Preset>, LibraryError> {
    Ok(list(lib)?
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name.trim())))
}

/// Add or replace a preset (matched by name, case-insensitive).
pub fn save(lib: &Library, preset: &Preset) -> Result<(), LibraryError> {
    preset.validate()?;
    let mut all = list(lib)?;
    all.retain(|p| !p.name.eq_ignore_ascii_case(preset.name.trim()));
    let mut p = preset.clone();
    p.name = p.name.trim().to_owned();
    all.push(p);
    all.sort_by_key(|p| p.name.to_lowercase());
    write_json_atomic(&file(lib), &all)
}

/// Delete a preset. Returns false if there was none.
pub fn delete(lib: &Library, name: &str) -> Result<bool, LibraryError> {
    let mut all = list(lib)?;
    let before = all.len();
    all.retain(|p| !p.name.eq_ignore_ascii_case(name.trim()));
    if all.len() == before {
        return Ok(false);
    }
    write_json_atomic(&file(lib), &all)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(dests: &[&str], template: &str) -> Preset {
        Preset {
            name: "Trip".into(),
            destinations: dests.iter().map(PathBuf::from).collect(),
            template: template.into(),
            event: None,
        }
    }

    #[test]
    fn validation() {
        let t = "{yyyy}/{orig}";
        assert!(p(&["A"], t).validate().is_ok());
        assert!(p(&["A", "B"], t).validate().is_ok());
        assert!(p(&[], t).validate().is_err());
        assert!(p(&["A", "B", "C"], t).validate().is_err());
        assert!(p(&["F:\\Photos", "f:\\photos\\"], t).validate().is_err());
        assert!(p(&["A"], "{nope}").validate().is_err());
        let mut x = p(&["A"], t);
        x.name = " ".into();
        assert!(x.validate().is_err());
    }

    #[test]
    fn save_get_replace_delete() {
        let d = tempfile::tempdir().unwrap();
        let lib = Library::create(d.path()).unwrap();
        assert!(list(&lib).unwrap().is_empty());
        save(&lib, &p(&["A"], "{orig}")).unwrap();
        save(&lib, &p(&["A", "B"], "{yyyy}/{orig}")).unwrap();
        let all = list(&lib).unwrap();
        assert_eq!(all.len(), 1, "same name replaces");
        assert_eq!(get(&lib, "trip").unwrap().unwrap().destinations.len(), 2);
        assert!(delete(&lib, "TRIP").unwrap());
        assert!(!delete(&lib, "trip").unwrap());
    }
}
