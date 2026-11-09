//! "Show in Explorer / Finder": where an item's original is, and opening
//! its folder with it selected. The path comes from keepsake-core, the OS
//! call from keepsake-platform; the original is only pointed at.

use std::path::Path;

use keepsake_core::reveal::{original_location, OriginalLocation};
use keepsake_platform::{file_manager_name, FileReveal, SystemReveal};
use tauri::State;

use crate::api::OriginalPlace;
use crate::convert;
use crate::state::{err, AppState, CmdResult};

fn text(p: &Path) -> String {
    let s = p.to_string_lossy().into_owned();
    s.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(s)
}

/// What the menu shows for a location.
pub fn place(loc: &OriginalLocation) -> OriginalPlace {
    let manager = file_manager_name().to_owned();
    match loc {
        OriginalLocation::Online(p) => OriginalPlace {
            online: true,
            path: Some(text(p)),
            problem: None,
            manager,
        },
        OriginalLocation::Offline { volume } => OriginalPlace {
            online: false,
            path: None,
            problem: Some(format!("Drive not connected ({volume})")),
            manager,
        },
        OriginalLocation::Missing(p) => OriginalPlace {
            online: false,
            path: Some(text(p)),
            problem: Some(format!("The file is not there any more: {}", text(p))),
            manager,
        },
    }
}

/// Show it if it can be shown.
pub fn reveal(by: &dyn FileReveal, loc: &OriginalLocation) -> CmdResult<()> {
    match loc {
        OriginalLocation::Online(p) => by.reveal(p).map_err(err),
        _ => Err(place(loc).problem.unwrap_or_default()),
    }
}

fn locate(state: &AppState, id: &str) -> CmdResult<OriginalLocation> {
    let open = state.open()?;
    let a = convert::id(id, "asset")?;
    let cat = open.cat()?;
    original_location(&cat, &open.lib, a).map_err(err)
}

/// Where the item's original is (for its right-click menu).
#[tauri::command(async)]
pub fn original_place(state: State<'_, AppState>, id: String) -> CmdResult<OriginalPlace> {
    Ok(place(&locate(&state, &id)?))
}

/// Open the folder holding the item's original, with the file selected.
#[tauri::command(async)]
pub fn show_in_folder(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    reveal(&SystemReveal, &locate(&state, &id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keepsake_platform::FakeReveal;
    use std::path::PathBuf;

    #[test]
    fn online_is_shown_and_offline_says_why() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("DSC00001.JPG");
        std::fs::write(&f, b"x").unwrap();
        let fake = FakeReveal::default();

        let on = OriginalLocation::Online(f.clone());
        assert!(place(&on).online);
        assert_eq!(place(&on).path.as_deref(), Some(text(&f).as_str()));
        reveal(&fake, &on).unwrap();
        assert_eq!(*fake.shown.lock().unwrap(), vec![f]);

        let off = OriginalLocation::Offline {
            volume: "Card A".into(),
        };
        let p = place(&off);
        assert!(!p.online);
        assert_eq!(p.problem.as_deref(), Some("Drive not connected (Card A)"));
        assert_eq!(
            reveal(&fake, &off).unwrap_err(),
            "Drive not connected (Card A)"
        );

        let gone = OriginalLocation::Missing(PathBuf::from("E:/DCIM/gone.JPG"));
        assert!(!place(&gone).online);
        assert!(reveal(&fake, &gone).is_err());
        assert_eq!(fake.shown.lock().unwrap().len(), 1, "only the online one");
        assert!(!place(&gone).manager.is_empty());
    }
}
