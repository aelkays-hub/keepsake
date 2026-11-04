//! The full-screen viewer: full-size pictures for 100% zoom (made on
//! demand, cached as renditions), RAW + JPEG members, video facts for the
//! timecode display, and "Open in" another application.
//!
//! Decoding can take a second or two (a RAW through LibRaw), so it runs on
//! a blocking worker, never on a command thread.

use std::path::{Path, PathBuf};

use keepsake_core::catalog::MediaKind;
use keepsake_core::render::full::{self, Member};
use keepsake_core::render::source::locate;
use keepsake_core::render::{RenderError, RenderTools};
use keepsake_core::settings::Settings;
use tauri::State;

use crate::api::{
    AssetMembers, FileMetadata, MemberDto, MetaGroupDto, MetaTagDto, OpenChoice, VideoInfo,
};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

fn member(m: MemberDto) -> Member {
    match m {
        MemberDto::Primary => Member::Primary,
        MemberDto::Raw => Member::Raw,
    }
}

fn offline_text(e: RenderError) -> String {
    match e {
        RenderError::Offline => "the drive with this item is not connected".to_owned(),
        other => other.to_string(),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

/// The full-size picture of a photo (path for `convertFileSrc`).
#[tauri::command]
pub async fn full_image(
    state: State<'_, AppState>,
    id: String,
    member: MemberDto,
) -> CmdResult<String> {
    let open = state.open()?;
    let a = convert::id(&id, "asset")?;
    let m = self::member(member);
    blocking(move || {
        let mut cat = open.writer()?;
        let p = full::full_image(
            &mut cat,
            &open.lib,
            &open.cache,
            &RenderTools::discover(),
            a,
            m,
        )
        .map_err(offline_text)?;
        Ok(convert::path_text(&p))
    })
    .await
}

#[tauri::command(async)]
pub fn asset_members(state: State<'_, AppState>, id: String) -> CmdResult<AssetMembers> {
    let open = state.open()?;
    let a = convert::id(&id, "asset")?;
    let cat = open.cat()?;
    match full::members(&cat, &open.lib, a) {
        Ok(m) => Ok(AssetMembers {
            has_pair: m.has_pair,
            raw_only: m.raw_only,
            online: true,
        }),
        Err(RenderError::Offline) => Ok(AssetMembers {
            has_pair: false,
            raw_only: false,
            online: false,
        }),
        Err(e) => Err(err(e)),
    }
}

#[tauri::command]
pub async fn video_info(state: State<'_, AppState>, id: String) -> CmdResult<VideoInfo> {
    let open = state.open()?;
    let a = convert::id(&id, "asset")?;
    blocking(move || {
        let cat = open.writer()?;
        let v =
            full::video_info(&cat, &open.lib, &RenderTools::discover(), a).map_err(offline_text)?;
        Ok(VideoInfo {
            fps_num: v.fps_num,
            fps_den: v.fps_den,
            start_timecode: v.start_timecode,
            duration_ms: v.duration_ms,
            width: v.width,
            height: v.height,
        })
    })
    .await
}

/// The file "Open in" hands over for an item: the RAW when asked and there
/// is one, else the photo (the JPEG of a pair) or the video.
fn file_to_open(state: &AppState, id: &str, member: MemberDto) -> CmdResult<PathBuf> {
    let open = state.open()?;
    let a = convert::id(id, "asset")?;
    let src = locate(&*open.cat()?, &open.lib, a).map_err(offline_text)?;
    Ok(match (member, &src.raw) {
        (MemberDto::Raw, Some(r)) => r.clone(),
        _ if src.media_kind == MediaKind::Photo => src.still.clone(),
        _ => src.original.clone(),
    })
}

/// Every metadata tag of the item's file (the RAW when asked and there is
/// one), read by ExifTool. Only reads the file.
#[tauri::command]
pub async fn metadata_all(
    state: State<'_, AppState>,
    id: String,
    member: MemberDto,
) -> CmdResult<FileMetadata> {
    let path = file_to_open(&state, &id, member)?;
    blocking(move || {
        let et = crate::import::exiftool().ok_or("ExifTool is not available")?;
        let groups = et.read_all(&path).map_err(err)?;
        Ok(FileMetadata {
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            groups: groups
                .into_iter()
                .map(|g| MetaGroupDto {
                    group: g.group,
                    tags: g
                        .tags
                        .into_iter()
                        .map(|t| MetaTagDto {
                            name: t.name,
                            value: t.value,
                        })
                        .collect(),
                })
                .collect(),
        })
    })
    .await
}

/// A name for people: `…\Topaz Photo.exe` → `Topaz Photo`, `Preview.app` → `Preview`.
pub fn app_name(app: &str) -> String {
    let base = app.rsplit(['\\', '/']).next().unwrap_or(app);
    let stem = base
        .rsplit_once('.')
        .map(|(s, _)| s)
        .filter(|s| !s.is_empty())
        .unwrap_or(base);
    stem.to_owned()
}

/// The applications offered for `path`: those set for its type (the first
/// is the default), then the system default.
pub fn choices_for(settings: &Settings, path: &Path) -> Vec<OpenChoice> {
    let apps = settings.open.apps_for(path);
    let mut out: Vec<OpenChoice> = apps
        .iter()
        .enumerate()
        .map(|(i, a)| OpenChoice {
            app: (*a).to_owned(),
            name: app_name(a),
            default: i == 0,
        })
        .collect();
    out.push(OpenChoice {
        app: String::new(),
        name: "System default".into(),
        default: apps.is_empty(),
    });
    out
}

/// Which application to use: `None` the default for the type, `Some("")`
/// the system default, else one of the applications set for the type (no
/// other program is started).
pub fn pick_app(settings: &Settings, path: &Path, app: Option<&str>) -> CmdResult<Option<String>> {
    match app.map(str::trim) {
        None => Ok(settings.open.app_for(path).map(str::to_owned)),
        Some("") => Ok(None),
        Some(a) => settings
            .open
            .apps_for(path)
            .into_iter()
            .find(|x| x.eq_ignore_ascii_case(a))
            .map(|x| Some(x.to_owned()))
            .ok_or_else(|| {
                format!(
                    "{} is not set up in Settings for this file type",
                    app_name(a)
                )
            }),
    }
}

/// The applications an item can be opened in (Open in menu).
#[tauri::command(async)]
pub fn open_choices(
    state: State<'_, AppState>,
    id: String,
    member: MemberDto,
) -> CmdResult<Vec<OpenChoice>> {
    let path = file_to_open(&state, &id, member)?;
    Ok(choices_for(&Settings::load().map_err(err)?, &path))
}

/// Open the original in another application: `app` from [`open_choices`],
/// or `None` for the default (Ctrl+O). Returns its name for the status line.
#[tauri::command(async)]
pub fn open_external(
    state: State<'_, AppState>,
    id: String,
    member: MemberDto,
    app: Option<String>,
) -> CmdResult<String> {
    let path = file_to_open(&state, &id, member)?;
    let settings = Settings::load().map_err(err)?;
    let app = pick_app(&settings, &path, app.as_deref())?;
    keepsake_core::external::open(&path, app.as_deref()).map_err(err)?;
    Ok(match &app {
        Some(a) => app_name(a),
        None => "the system default".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use keepsake_core::settings::OpenApps;

    fn settings() -> Settings {
        let mut s = Settings::default();
        s.open.apps.insert(
            "arw".into(),
            OpenApps::Many(vec![
                r"C:\Program Files\Adobe\Adobe Photoshop 2026\Photoshop.exe".into(),
                r"C:\Program Files\Topaz Labs LLC\Topaz Photo\Topaz Photo.exe".into(),
            ]),
        );
        s
    }

    #[test]
    fn choices_list_the_apps_for_the_type_then_the_system_default() {
        let s = settings();
        let c = choices_for(&s, Path::new("E:/a/DSC1.ARW"));
        let names: Vec<_> = c.iter().map(|c| (c.name.as_str(), c.default)).collect();
        assert_eq!(
            names,
            [
                ("Photoshop", true),
                ("Topaz Photo", false),
                ("System default", false)
            ]
        );
        assert_eq!(c[2].app, "");
        let c = choices_for(&s, Path::new("C1.MP4"));
        assert_eq!(c.len(), 1);
        assert!(
            c[0].default,
            "nothing set: the system default is the default"
        );
        assert_eq!(app_name("Preview.app"), "Preview");
        assert_eq!(app_name("/usr/bin/gimp"), "gimp");
    }

    #[test]
    fn only_apps_set_for_the_type_can_be_picked() {
        let s = settings();
        let p = Path::new("DSC1.arw");
        let topaz = r"C:\Program Files\Topaz Labs LLC\Topaz Photo\Topaz Photo.exe";
        assert_eq!(pick_app(&s, p, None).unwrap().as_deref(), s.open.app_for(p));
        assert_eq!(pick_app(&s, p, Some("")).unwrap(), None);
        assert_eq!(
            pick_app(&s, p, Some(&topaz.to_lowercase()))
                .unwrap()
                .as_deref(),
            Some(topaz)
        );
        assert!(pick_app(&s, p, Some(r"C:\Windows\System32\cmd.exe")).is_err());
        assert!(pick_app(&s, Path::new("a.jpg"), Some(topaz)).is_err());
    }
}
