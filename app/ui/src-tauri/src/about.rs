//! Tools → About: the version, the credits the data and services ask for,
//! and the third-party licences (ADR-047). Thin: no business logic.

use crate::state::{err, CmdResult};

/// The notices `scripts/make-notices.py` writes. Built into the app, so the
/// page works without the installer's copy (`licenses/` in the install
/// folder, the same text).
const NOTICES: &str = include_str!("../licenses/THIRD-PARTY-NOTICES.txt");

/// The only pages the About screen opens: its credits' links.
pub const LINKS: &[&str] = &[
    "https://www.flickr.com/services/api/",
    "https://opensky-network.org",
    "https://open.canada.ca/en/open-government-licence-canada",
    "https://www.faa.gov/licenses_certificates/aircraft_certification/aircraft_registry/releasable_aircraft_download",
    "https://ourairports.com/data/",
    "https://www.adsbdb.com",
    "https://www.flightradar24.com",
];

/// The third-party notices: every crate and package built into Keepsake
/// with its licence text, and the programs shipped beside it.
#[tauri::command(async)]
pub fn third_party_notices() -> &'static str {
    NOTICES
}

/// Open one of the credits' pages in the browser.
#[tauri::command(async)]
pub fn about_open(url: String) -> CmdResult<()> {
    if !LINKS.contains(&url.as_str()) {
        return Err("That page is not one of the About screen's links.".to_owned());
    }
    keepsake_core::external::open_url(&url).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notices_are_built_in_and_name_what_ships() {
        let n = third_party_notices();
        assert!(n.starts_with("THIRD-PARTY NOTICES FOR KEEPSAKE"));
        // The four sections the About screen reads.
        for part in [
            "1. PROGRAMS SHIPPED BESIDE KEEPSAKE",
            "2. RUST CRATES BUILT INTO KEEPSAKE (",
            "3. JAVASCRIPT PACKAGES IN KEEPSAKE'S WINDOW (",
            "4. LICENCE TEXTS",
        ] {
            assert!(n.contains(part), "missing {part}");
        }
        // Programs beside the app, a crate, an npm package.
        for name in ["FFmpeg", "ExifTool", "LibRaw", "rusqlite ", "react "] {
            assert!(n.contains(name), "missing {name}");
        }
    }

    #[test]
    fn only_the_credits_links_open() {
        let e = about_open("https://example.com".to_owned());
        assert!(e.is_err());
        assert!(LINKS.iter().all(|l| l.starts_with("https://")));
    }
}
