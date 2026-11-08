//! Tag with Claude (ADR-036) for the UI: the API key (kept in the OS's
//! credential store by `keepsake_platform::secrets`), the model, and
//! queueing, following and stopping the photos the user picked. The work is
//! in `keepsake_core::ai::cloud`; it runs in the render service.

use std::sync::Arc;

use keepsake_core::ai::cloud::{self, Claude, ClaudeModel, UreqHttp};
use keepsake_core::catalog::AssetId;
use keepsake_core::settings::Settings;
use tauri::{AppHandle, State};

use crate::api::{ClaudeInfo, ClaudeModelDto, ClaudeStarted, ClaudeStatusDto, Selection};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// The chosen model (the default if the settings name none we know).
fn chosen() -> ClaudeModel {
    Settings::load()
        .ok()
        .and_then(|s| ClaudeModel::parse(&s.ai.claude_model))
        .unwrap_or_default()
}

/// The tagger the render service uses: it asks for the key and the model
/// for each photo, so changes in Settings apply at once.
pub fn tagger() -> Arc<dyn cloud::Tagger> {
    Arc::new(Claude::new(
        Arc::new(UreqHttp::new()),
        Arc::new(|| keepsake_platform::secrets::claude_key().ok().flatten()),
        Arc::new(chosen),
    ))
}

#[tauri::command(async)]
pub fn ai_claude_info() -> CmdResult<ClaudeInfo> {
    Ok(ClaudeInfo {
        has_key: keepsake_platform::secrets::claude_key()
            .map_err(err)?
            .is_some(),
        model: chosen().id().to_owned(),
        models: ClaudeModel::ALL
            .iter()
            .map(|m| ClaudeModelDto {
                id: m.id().to_owned(),
                name: m.name().to_owned(),
                cents_per_photo: m.cents_per_photo(),
            })
            .collect(),
    })
}

/// Save the API key (`None` or empty: remove it).
#[tauri::command(async)]
pub fn ai_claude_set_key(key: Option<String>) -> CmdResult<ClaudeInfo> {
    let key = key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
    if let Some(k) = &key {
        if !cloud::looks_like_key(k) {
            return Err(
                "That does not look like a Claude API key (they start with sk-ant-).".into(),
            );
        }
    }
    keepsake_platform::secrets::set_claude_key(key.as_deref()).map_err(err)?;
    ai_claude_info()
}

#[tauri::command(async)]
pub fn ai_claude_set_model(model: String) -> CmdResult<ClaudeInfo> {
    let m = ClaudeModel::parse(&model).ok_or_else(|| format!("unknown model {model}"))?;
    let mut s = Settings::load().map_err(err)?;
    s.ai.claude_model = m.id().to_owned();
    s.save().map_err(err)?;
    ai_claude_info()
}

fn dto((s, problem): (cloud::ClaudeStatus, Option<String>)) -> ClaudeStatusDto {
    ClaudeStatusDto {
        items: s.items,
        analysed: s.done,
        pending: s.pending,
        failed: s.failed,
        problem,
    }
}

fn unique(ids: &[String]) -> CmdResult<Vec<AssetId>> {
    let mut out: Vec<AssetId> = Vec::new();
    for id in convert::ids(ids)? {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// Send the selected photos to Claude to be tagged (in the background).
#[tauri::command(async)]
pub fn ai_claude_tag(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
) -> CmdResult<ClaudeStarted> {
    if keepsake_platform::secrets::claude_key()
        .map_err(err)?
        .is_none()
    {
        return Err("Add your Claude API key in Settings first.".into());
    }
    let mut ids: Vec<AssetId> = Vec::new();
    for id in crate::edit::resolve(&state, &selection)? {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err("Select the photos to tag first.".into());
    }
    let open = state.open()?;
    {
        let mut cat = open.writer()?;
        cloud::enqueue(&mut cat, &ids).map_err(err)?;
    }
    crate::render::start_service(&app)?;
    let cat = open.cat()?;
    Ok(ClaudeStarted {
        ids: ids.iter().map(ToString::to_string).collect(),
        status: dto(cloud::status(&cat, &ids).map_err(err)?),
    })
}

#[tauri::command(async)]
pub fn ai_claude_status(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<ClaudeStatusDto> {
    let ids = unique(&ids)?;
    let open = state.open()?;
    let cat = open.cat()?;
    Ok(dto(cloud::status(&cat, &ids).map_err(err)?))
}

/// Stop: photos not sent yet are not sent; those tagged stay tagged.
#[tauri::command(async)]
pub fn ai_claude_stop(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    let ids = unique(&ids)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    cloud::stop(&mut cat, &ids).map_err(err)?;
    Ok(())
}
