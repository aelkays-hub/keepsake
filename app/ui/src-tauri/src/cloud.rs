//! Tag with AI (ADR-036, ADR-038) for the UI: each provider's API key
//! (kept in the OS's credential store by `keepsake_platform::secrets`) and
//! model, the provider offered first, and queueing, following and stopping
//! the photos the user picked. The work is in `keepsake_core::ai::cloud`;
//! it runs in the render service.

use std::sync::Arc;

use keepsake_core::ai::cloud::{self, CloudModel, CloudTagger, Provider, UreqHttp};
use keepsake_core::catalog::AssetId;
use keepsake_core::settings::{Settings, ALL_PROVIDERS};
use tauri::{AppHandle, State};

use crate::api::{
    CloudInfo, CloudModelDto, CloudProviderDto, CloudStarted, CloudStatusDto, Selection,
};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

fn provider(id: &str) -> CmdResult<Provider> {
    Provider::parse(id).ok_or_else(|| format!("unknown AI provider {id}"))
}

fn key(p: Provider) -> CmdResult<Option<String>> {
    keepsake_platform::secrets::api_key(p.id()).map_err(err)
}

/// `p`'s chosen model (its default if the settings name none we know).
fn chosen(p: Provider) -> CloudModel {
    Settings::load()
        .map(|s| s.ai.cloud_model(p))
        .unwrap_or_else(|_| p.default_model())
}

/// The tagger the render service uses: it asks for the key and the model
/// for each photo, so changes in Settings apply at once.
pub fn tagger() -> Arc<dyn cloud::Tagger> {
    Arc::new(CloudTagger::new(
        Arc::new(UreqHttp::new()),
        Arc::new(|p: Provider| keepsake_platform::secrets::api_key(p.id()).ok().flatten()),
        Arc::new(chosen),
    ))
}

#[tauri::command(async)]
pub fn ai_cloud_info() -> CmdResult<CloudInfo> {
    let settings = Settings::load().map_err(err)?;
    let mut providers = Vec::new();
    for p in Provider::ALL {
        providers.push(CloudProviderDto {
            id: p.id().to_owned(),
            name: p.name().to_owned(),
            company: p.company().to_owned(),
            has_key: key(p)?.is_some(),
            model: settings.ai.cloud_model(p).id().to_owned(),
            models: p
                .models()
                .map(|m| CloudModelDto {
                    id: m.id().to_owned(),
                    name: m.name().to_owned(),
                    cents_per_photo: m.cents_per_photo(),
                })
                .collect(),
        });
    }
    let default_provider = if settings.ai.cloud_all() {
        ALL_PROVIDERS
    } else {
        settings.ai.cloud_provider().id()
    };
    Ok(CloudInfo {
        default_provider: default_provider.to_owned(),
        providers,
    })
}

/// Save `provider`'s API key (`None` or empty: remove it).
#[tauri::command(async)]
pub fn ai_cloud_set_key(provider_id: String, key: Option<String>) -> CmdResult<CloudInfo> {
    let p = provider(&provider_id)?;
    let key = key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty());
    if let Some(k) = &key {
        if !p.looks_like_key(k) {
            let hint = match p {
                Provider::Claude => " (they start with sk-ant-)",
                Provider::OpenAi => " (they start with sk-)",
                Provider::Gemini => "",
            };
            return Err(format!("That does not look like a {p} API key{hint}."));
        }
    }
    keepsake_platform::secrets::set_api_key(p.id(), key.as_deref()).map_err(err)?;
    ai_cloud_info()
}

#[tauri::command(async)]
pub fn ai_cloud_set_model(model: String) -> CmdResult<CloudInfo> {
    let m = CloudModel::parse(&model).ok_or_else(|| format!("unknown model {model}"))?;
    let mut s = Settings::load().map_err(err)?;
    s.ai.set_cloud_model(m);
    s.save().map_err(err)?;
    ai_cloud_info()
}

/// The provider offered first next time (`all`: every AI with a key).
#[tauri::command(async)]
pub fn ai_cloud_set_default(provider_id: String) -> CmdResult<CloudInfo> {
    let id = if provider_id == ALL_PROVIDERS {
        ALL_PROVIDERS
    } else {
        provider(&provider_id)?.id()
    };
    let mut s = Settings::load().map_err(err)?;
    s.ai.cloud_provider = id.to_owned();
    s.save().map_err(err)?;
    ai_cloud_info()
}

fn dto((s, problem): (cloud::CloudStatus, Option<String>)) -> CloudStatusDto {
    CloudStatusDto {
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

/// Send the selected photos to `provider` to be tagged (in the background).
#[tauri::command(async)]
pub fn ai_cloud_tag(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
    provider_id: String,
) -> CmdResult<CloudStarted> {
    let p = provider(&provider_id)?;
    if key(p)?.is_none() {
        return Err(format!("Add your {p} API key in Settings first."));
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
        cloud::enqueue(&mut cat, p, &ids).map_err(err)?;
    }
    crate::render::start_service(&app)?;
    let cat = open.cat()?;
    Ok(CloudStarted {
        provider: p.id().to_owned(),
        ids: ids.iter().map(ToString::to_string).collect(),
        status: dto(cloud::status(&cat, p, &ids).map_err(err)?),
    })
}

#[tauri::command(async)]
pub fn ai_cloud_status(
    state: State<'_, AppState>,
    ids: Vec<String>,
    provider_id: String,
) -> CmdResult<CloudStatusDto> {
    let p = provider(&provider_id)?;
    let ids = unique(&ids)?;
    let open = state.open()?;
    let cat = open.cat()?;
    Ok(dto(cloud::status(&cat, p, &ids).map_err(err)?))
}

/// Stop: photos not sent yet are not sent; those tagged stay tagged.
#[tauri::command(async)]
pub fn ai_cloud_stop(
    state: State<'_, AppState>,
    ids: Vec<String>,
    provider_id: String,
) -> CmdResult<()> {
    let p = provider(&provider_id)?;
    let ids = unique(&ids)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    cloud::stop(&mut cat, p, &ids).map_err(err)?;
    Ok(())
}
