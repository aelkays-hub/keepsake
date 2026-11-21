//! Upload to Flickr for the UI (ADR-046): the user's API key and secret
//! and the sign-in, kept in the OS's credential store (never in a file,
//! the log or a reply to the UI); the upload window's photos; queueing
//! uploads. The work is in `keepsake_core::flickr`; uploads run in the
//! render service's Flickr worker.

use std::sync::{Arc, Mutex};

use keepsake_core::catalog::AssetId;
use keepsake_core::flickr::{self, Account, App, Flickr, Pending, Privacy, Request, TagOptions};
use keepsake_platform::secrets::{self, FLICKR_KEY, FLICKR_SECRET, FLICKR_SIGN_IN};
use tauri::{AppHandle, State};

use crate::api::{
    FlickrItemDto, FlickrPrepareDto, FlickrProgressDto, FlickrProgressItemDto, FlickrRequestDto,
    FlickrStatusDto, FlickrUploadDto, Selection,
};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// The sign-in under way: its request token, in memory only.
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

fn app_keys() -> Option<App> {
    let key = secrets::flickr(FLICKR_KEY).ok().flatten()?;
    let secret = secrets::flickr(FLICKR_SECRET).ok().flatten()?;
    Some(App { key, secret })
}

fn account() -> Option<Account> {
    let s = secrets::flickr(FLICKR_SIGN_IN).ok().flatten()?;
    serde_json::from_str(&s).ok()
}

fn client() -> Flickr {
    Flickr::new(Arc::new(keepsake_core::ai::cloud::UreqHttp::new()))
}

/// The uploader the render service uses: the key and sign-in looked up
/// for each photo.
pub fn uploader() -> Arc<flickr::Uploader> {
    Arc::new(flickr::Uploader::new(
        Arc::new(keepsake_core::ai::cloud::UreqHttp::new()),
        Arc::new(app_keys),
        Arc::new(account),
    ))
}

fn status(state: &AppState) -> CmdResult<FlickrStatusDto> {
    let acc = account();
    let uploaded = match state.try_open() {
        Some(open) => {
            let cat = open.cat()?;
            cat.read(|tx| tx.flickr().uploaded_count()).map_err(err)?
        }
        None => 0,
    };
    Ok(FlickrStatusDto {
        has_app: app_keys().is_some(),
        signed_in: acc.is_some(),
        username: acc.map(|a| a.username),
        awaiting_code: PENDING.lock().map(|p| p.is_some()).unwrap_or(false),
        uploaded,
    })
}

#[tauri::command(async)]
pub fn flickr_status(state: State<'_, AppState>) -> CmdResult<FlickrStatusDto> {
    status(&state)
}

/// A Flickr key or secret as the App Garden gives it: letters and digits.
fn looks_like(s: &str, min: usize) -> bool {
    (min..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Save the API key and secret (both blank: remove them, and sign out).
#[tauri::command(async)]
pub fn flickr_set_app(
    state: State<'_, AppState>,
    key: String,
    secret: String,
) -> CmdResult<FlickrStatusDto> {
    let (key, secret) = (key.trim(), secret.trim());
    match (key.is_empty(), secret.is_empty()) {
        (true, true) => {
            secrets::set_flickr(FLICKR_KEY, None).map_err(err)?;
            secrets::set_flickr(FLICKR_SECRET, None).map_err(err)?;
            secrets::set_flickr(FLICKR_SIGN_IN, None).map_err(err)?;
        }
        (false, false) => {
            if !looks_like(key, 16) || !looks_like(secret, 8) {
                return Err(
                    "That does not look like a Flickr API key and secret (letters and digits, from Flickr's App Garden)."
                        .into(),
                );
            }
            secrets::set_flickr(FLICKR_KEY, Some(key)).map_err(err)?;
            secrets::set_flickr(FLICKR_SECRET, Some(secret)).map_err(err)?;
            // A sign-in belongs to the key it was made with.
            secrets::set_flickr(FLICKR_SIGN_IN, None).map_err(err)?;
        }
        _ => return Err("Fill in both the key and the secret.".into()),
    }
    if let Ok(mut p) = PENDING.lock() {
        *p = None;
    }
    status(&state)
}

/// Start signing in: Flickr's page where the user allows Keepsake opens in
/// the browser (its address is returned too, to copy).
#[tauri::command(async)]
pub fn flickr_sign_in_start() -> CmdResult<String> {
    let app = app_keys().ok_or("Add your Flickr API key and secret first.")?;
    let pending = client().request_token(&app).map_err(err)?;
    let url = pending.authorize_url.clone();
    *PENDING.lock().map_err(|_| "sign-in state poisoned")? = Some(pending);
    if let Err(e) = keepsake_core::external::open_url(&url) {
        tracing::warn!("could not open the browser for the Flickr sign-in: {e}");
    }
    Ok(url)
}

/// Finish signing in with the code Flickr showed.
#[tauri::command(async)]
pub fn flickr_sign_in_finish(
    state: State<'_, AppState>,
    code: String,
) -> CmdResult<FlickrStatusDto> {
    let app = app_keys().ok_or("Add your Flickr API key and secret first.")?;
    let pending = PENDING
        .lock()
        .map_err(|_| "sign-in state poisoned")?
        .clone()
        .ok_or("Choose Sign in to Flickr first.")?;
    let acc = client().access_token(&app, &pending, &code).map_err(err)?;
    let json = serde_json::to_string(&acc).map_err(err)?;
    secrets::set_flickr(FLICKR_SIGN_IN, Some(&json)).map_err(err)?;
    if let Ok(mut p) = PENDING.lock() {
        *p = None;
    }
    status(&state)
}

/// Sign out: the sign-in is removed from this computer (Keepsake can be
/// removed from the account on flickr.com too).
#[tauri::command(async)]
pub fn flickr_sign_out(state: State<'_, AppState>) -> CmdResult<FlickrStatusDto> {
    secrets::set_flickr(FLICKR_SIGN_IN, None).map_err(err)?;
    if let Ok(mut p) = PENDING.lock() {
        *p = None;
    }
    status(&state)
}

/// Ask Flickr whether the sign-in works.
#[tauri::command(async)]
pub fn flickr_test() -> CmdResult<String> {
    let app = app_keys().ok_or("Add your Flickr API key and secret first.")?;
    let acc = account().ok_or("Sign in to Flickr first.")?;
    let name = client().test_login(&app, &acc).map_err(err)?;
    Ok(format!("Flickr says you are signed in as {name}."))
}

/// Open a photo's Flickr page in the browser.
#[tauri::command(async)]
pub fn flickr_open(url: String) -> CmdResult<()> {
    keepsake_core::external::open_url(&url).map_err(err)
}

fn date(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// The Upload to Flickr window: the selected photos with their title and
/// suggested tags, and their earlier uploads.
#[tauri::command(async)]
pub fn flickr_prepare(
    state: State<'_, AppState>,
    selection: Selection,
    camera: bool,
    machine: bool,
    parents: bool,
) -> CmdResult<FlickrPrepareDto> {
    let mut ids: Vec<AssetId> = Vec::new();
    for id in crate::edit::resolve(&state, &selection)? {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return Err("Select the photos first.".into());
    }
    let opts = TagOptions {
        camera,
        machine,
        parents,
    };
    let open = state.open()?;
    let mut items = Vec::new();
    {
        let cat = open.cat()?;
        for id in &ids {
            let p = flickr::prepare(&cat, *id, opts).map_err(err)?;
            let (_, thumb) = crate::aircraft::photo_of(&cat, &open.cache, *id)?;
            items.push(FlickrItemDto {
                id: id.to_string(),
                name: p.name,
                thumb,
                title: p.title,
                description: p.description,
                tags: p.tags,
                video: p.video,
                uploads: p
                    .uploads
                    .into_iter()
                    .map(|u| FlickrUploadDto {
                        url: u.url,
                        privacy: u.privacy,
                        date: date(u.uploaded_ms),
                    })
                    .collect(),
            });
        }
    }
    Ok(FlickrPrepareDto {
        items,
        status: status(&state)?,
    })
}

/// Upload these photos (in the background), visible to `privacy`.
#[tauri::command(async)]
pub fn flickr_upload(
    app: AppHandle,
    state: State<'_, AppState>,
    items: Vec<FlickrRequestDto>,
    privacy: String,
) -> CmdResult<u64> {
    let privacy = Privacy::parse(&privacy).ok_or("Choose who may see the photos.")?;
    if app_keys().is_none() {
        return Err("Add your Flickr API key and secret in Settings → Flickr first.".into());
    }
    if account().is_none() {
        return Err("Sign in to Flickr in Settings → Flickr first.".into());
    }
    if items.is_empty() {
        return Err("Tick the photos to upload first.".into());
    }
    let mut requests = Vec::new();
    for it in &items {
        let asset: AssetId = convert::id(&it.id, "asset")?;
        requests.push(Request {
            asset,
            title: it.title.clone(),
            description: it.description.clone(),
            tags: it.tags.clone(),
        });
    }
    let open = state.open()?;
    let n = {
        let mut cat = open.writer()?;
        flickr::enqueue(&mut cat, &requests, privacy).map_err(err)?
    };
    crate::render::start_service(&app)?;
    Ok(n)
}

/// Where the uploads of `ids` stand.
#[tauri::command(async)]
pub fn flickr_upload_status(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<FlickrProgressDto> {
    use keepsake_core::flickr::State as Up;
    let ids = convert::ids(&ids)?;
    let open = state.open()?;
    let cat = open.cat()?;
    let mut pending = 0;
    let mut items = Vec::new();
    for id in &ids {
        let (st, url, error) = match flickr::state(&cat, *id).map_err(err)? {
            Up::Waiting { retry: None } => ("waiting", None, None),
            Up::Waiting { retry: Some(e) } => ("retrying", None, Some(e)),
            Up::Uploading => ("uploading", None, None),
            Up::Done(u) => ("done", Some(u.url), None),
            Up::Failed(e) => ("failed", None, Some(e)),
            Up::None => ("none", None, None),
        };
        if matches!(st, "waiting" | "retrying" | "uploading") {
            pending += 1;
        }
        items.push(FlickrProgressItemDto {
            id: id.to_string(),
            state: st.to_owned(),
            url,
            error,
        });
    }
    Ok(FlickrProgressDto { pending, items })
}

/// Stop the uploads of `ids` not yet started.
#[tauri::command(async)]
pub fn flickr_upload_stop(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    let ids = convert::ids(&ids)?;
    let open = state.open()?;
    let mut cat = open.writer()?;
    flickr::stop(&mut cat, &ids).map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_checked_before_they_are_saved() {
        assert!(looks_like("0123456789abcdef0123456789abcdef", 16));
        assert!(looks_like("0123456789abcdef", 8));
        assert!(!looks_like("short", 8));
        assert!(!looks_like("has space in it 123456", 8));
        assert!(!looks_like("sk-ant-0123456789abcdef", 16));
    }
}
