//! Edit suggestions for the UI (ADR-042): one photo's suggestions (kept,
//! or measured now from its thumbnail), asking an AI (a job on the render
//! service's cloud worker), and stopping that. The work is in
//! `keepsake_core::ai::suggest`; the original is not read or changed.

use keepsake_core::ai::cloud::Provider;
use keepsake_core::ai::suggest::{self, Advice, AiAdvice, Asking, Suggestion};
use keepsake_core::ai::AiError;
use keepsake_core::catalog::{AssetId, Priority};
use tauri::{AppHandle, State};

use crate::api::{AiAdviceDto, AskingDto, EditAdviceDto, EditSuggestionDto, MeasuredDto};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

fn suggestion(s: Suggestion) -> EditSuggestionDto {
    EditSuggestionDto {
        title: s.title,
        why: s.why,
        photoshop: s.photoshop,
        topaz: s.topaz,
    }
}

fn provider_name(id: &str) -> String {
    Provider::parse(id).map_or_else(|| id.to_owned(), |p| p.name().to_owned())
}

fn ai_dto(a: AiAdvice) -> AiAdviceDto {
    AiAdviceDto {
        provider_name: provider_name(&a.provider),
        provider: a.provider,
        model: a.model,
        summary: a.summary,
        suggestions: a.suggestions.into_iter().map(suggestion).collect(),
        made_ms: a.made_ms,
    }
}

fn asking_dto(a: Asking) -> AskingDto {
    AskingDto {
        provider: a.provider.id().to_owned(),
        provider_name: a.provider.name().to_owned(),
        pending: a.pending,
        problem: a.problem,
    }
}

fn dto(id: AssetId, a: Advice, asking: Option<Asking>) -> EditAdviceDto {
    EditAdviceDto {
        id: id.to_string(),
        waiting: false,
        measured: a
            .measured()
            .into_iter()
            .map(|m| MeasuredDto {
                label: m.label.to_owned(),
                value: m.value,
            })
            .collect(),
        summary: a.summary(),
        suggestions: a.suggestions().into_iter().map(suggestion).collect(),
        made_ms: a.made_ms,
        ai: a.ai.map(ai_dto),
        asking: asking.map(asking_dto),
    }
}

fn waiting(id: AssetId) -> EditAdviceDto {
    EditAdviceDto {
        id: id.to_string(),
        waiting: true,
        measured: Vec::new(),
        summary: String::new(),
        suggestions: Vec::new(),
        made_ms: 0,
        ai: None,
        asking: None,
    }
}

fn is_photo(state: &AppState, id: AssetId) -> CmdResult<()> {
    let open = state.open()?;
    let cat = open.cat()?;
    let a = cat.read(|tx| tx.assets().require(id)).map_err(err)?;
    if a.media_kind == keepsake_core::catalog::MediaKind::Video {
        return Err("Edit suggestions are for photos.".into());
    }
    Ok(())
}

/// The photo's edit suggestions: those kept, or (with `again`, or when
/// none are kept) measured now from its thumbnail. With no thumbnail yet,
/// one is made first and `waiting` is true: ask again shortly.
#[tauri::command(async)]
pub fn edit_suggestions(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    again: Option<bool>,
) -> CmdResult<EditAdviceDto> {
    let asset: AssetId = convert::id(&id, "asset")?;
    is_photo(&state, asset)?;
    let open = state.open()?;
    let advice = {
        let mut cat = open.writer()?;
        match suggest::advice(&mut cat, &open.cache, asset, again.unwrap_or(false)) {
            Ok(a) => a,
            Err(AiError::NoThumbnail) => {
                keepsake_core::render::enqueue(&mut cat, &[asset], Priority::Recent, true)
                    .map_err(err)?;
                drop(cat);
                crate::render::start_service(&app)?;
                return Ok(waiting(asset));
            }
            Err(e) => return Err(err(e)),
        }
    };
    let cat = open.cat()?;
    let answered = advice.ai.as_ref().map(|a| a.made_ms);
    let asking = suggest::asking(&cat, asset, answered).map_err(err)?;
    Ok(dto(asset, advice, asking))
}

/// Ask `provider` for the photo's edit suggestions (in the background):
/// its thumbnail and the measurements are sent.
#[tauri::command(async)]
pub fn edit_suggestions_ask(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    provider_id: String,
) -> CmdResult<EditAdviceDto> {
    let asset: AssetId = convert::id(&id, "asset")?;
    let p = Provider::parse(&provider_id).ok_or_else(|| format!("unknown AI {provider_id}"))?;
    if keepsake_platform::secrets::api_key(p.id())
        .map_err(err)?
        .is_none()
    {
        return Err(format!(
            "Add your {p} API key in Settings first (Settings → Tag with AI)."
        ));
    }
    is_photo(&state, asset)?;
    {
        let open = state.open()?;
        let mut cat = open.writer()?;
        suggest::enqueue_ask(&mut cat, p, asset).map_err(err)?;
    }
    crate::render::start_service(&app)?;
    edit_suggestions(app, state, id, Some(false))
}

/// Stop: a question not sent yet is not sent. A kept answer stays.
#[tauri::command(async)]
pub fn edit_suggestions_stop(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let asset: AssetId = convert::id(&id, "asset")?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    suggest::stop_asking(&mut cat, asset).map_err(err)?;
    Ok(())
}
