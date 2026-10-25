//! The full-screen viewer: full-size pictures for 100% zoom (made on
//! demand, cached as renditions), RAW + JPEG members, video facts for the
//! timecode display, and "Open in" another application.
//!
//! Decoding can take a second or two (a RAW through LibRaw), so it runs on
//! a blocking worker, never on a command thread.

use keepsake_core::catalog::MediaKind;
use keepsake_core::render::full::{self, Member};
use keepsake_core::render::source::locate;
use keepsake_core::render::{RenderError, RenderTools};
use keepsake_core::settings::Settings;
use tauri::State;

use crate::api::{AssetMembers, MemberDto, VideoInfo};
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
    let cat = open.cat();
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

/// Open the original in another application: the one set for its type in
/// Settings, else the system default. Returns the program used.
#[tauri::command(async)]
pub fn open_external(
    state: State<'_, AppState>,
    id: String,
    member: MemberDto,
) -> CmdResult<String> {
    let open = state.open()?;
    let a = convert::id(&id, "asset")?;
    let src = locate(&open.cat(), &open.lib, a).map_err(offline_text)?;
    let path = match (member, &src.raw) {
        (MemberDto::Raw, Some(r)) => r.clone(),
        _ if src.media_kind == MediaKind::Photo => src.still.clone(),
        _ => src.original.clone(),
    };
    let settings = Settings::load().map_err(err)?;
    let app = settings.open.app_for(&path);
    keepsake_core::external::open(&path, app).map_err(err)?;
    Ok(keepsake_core::external::command_for(&path, app).0)
}
