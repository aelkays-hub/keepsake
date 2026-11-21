//! Upload to Flickr (ADR-046): the photos the user picks go to their own
//! Flickr account, public or private (or friends and family), with the
//! tags they checked.
//!
//! - The user registers their own Flickr app (a non-commercial key from
//!   Flickr's App Garden) and signs in once with OAuth 1.0a
//!   ([`client::Flickr::request_token`], then the code Flickr shows,
//!   [`client::Flickr::access_token`]). The key, its secret and the
//!   sign-in's token are kept in the OS's credential store by the app, and
//!   looked up for each job ([`Uploader`]), never written to a file or log.
//! - Each photo is one `flickr.upload` job on the job queue, run by one
//!   worker of the render service; what the user chose (privacy, title,
//!   tags) waits in `flickr_queue` until then ([`enqueue`]).
//! - What is sent: a JPEG, PNG, GIF or TIFF original as it is (read, never
//!   changed); a RAW or HEIF photo as the full-size JPEG Keepsake makes for
//!   the viewer (`render::full`). Videos are not uploaded.
//! - Each upload is kept in `flickr_uploads` (Flickr's photo id and page),
//!   so the window says a photo is on Flickr already, and does not tick it.

pub mod account;
pub mod client;
pub mod oauth;
pub mod tags;

use std::path::Path;
use std::sync::Arc;

use crate::ai::cloud::Http;
use crate::catalog::{
    AssetId, Catalog, CatalogError, FlickrQueued, FlickrUpload, JobState, MediaKind, NewJob,
    Priority,
};
use crate::library::Library;
use crate::render::source::{is_raw, locate, lower_ext};
use crate::render::{Cache, RenderError, RenderTools};

pub use client::{Account, App, Flickr, Pending, Privacy, UploadParams};
pub use tags::TagOptions;

/// The job: upload one photo to Flickr.
pub const JOB_FLICKR_UPLOAD: &str = "flickr.upload";

/// Flickr takes photos up to 200 MB.
pub const MAX_PHOTO_BYTES: u64 = 200 * 1024 * 1024;

/// Why Flickr upload did not work.
#[derive(Debug, thiserror::Error)]
pub enum FlickrError {
    #[error("Add your Flickr API key and secret in Settings → Flickr first.")]
    NoApp,
    #[error("Sign in to Flickr in Settings → Flickr first.")]
    NotSignedIn,
    /// Flickr no longer accepts the sign-in (or the key).
    #[error("{0}")]
    SignedOut(String),
    /// Could not reach Flickr, or it is not available for now: tried again
    /// later.
    #[error("{0}")]
    Network(String),
    /// Flickr refused, for a reason trying again will not fix.
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Unexpected(String),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl FlickrError {
    /// Trying again later may help.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_))
    }
}

/// Looks the user's Flickr app up.
pub type AppFn = dyn Fn() -> Option<App> + Send + Sync;
/// Looks the signed-in account up.
pub type AccountFn = dyn Fn() -> Option<Account> + Send + Sync;

/// What the upload worker needs: Flickr's API, and where the key and the
/// sign-in are (looked up for each photo, so a sign-in in Settings works
/// at once).
pub struct Uploader {
    pub client: Flickr,
    pub app: Arc<AppFn>,
    pub account: Arc<AccountFn>,
}

impl Uploader {
    pub fn new(http: Arc<dyn Http>, app: Arc<AppFn>, account: Arc<AccountFn>) -> Self {
        Self {
            client: Flickr::new(http),
            app,
            account,
        }
    }

    pub fn with_client(mut self, client: Flickr) -> Self {
        self.client = client;
        self
    }
}

/// The original's file name (`DSC01234.ARW`).
pub fn file_name(cat: &Catalog, asset: AssetId) -> Result<Option<String>, CatalogError> {
    Ok(cat
        .read(|tx| tx.files().originals(asset))?
        .first()
        .map(|f| f.rel_path.file_name().to_owned()))
}

/// What the upload window offers for one photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub asset: AssetId,
    pub name: String,
    /// The original's name without its extension.
    pub title: String,
    /// The photo's comment.
    pub description: String,
    pub tags: Vec<String>,
    pub video: bool,
    /// Its uploads so far, newest first.
    pub uploads: Vec<FlickrUpload>,
}

/// The title, description, tags and earlier uploads of `asset`.
pub fn prepare(cat: &Catalog, asset: AssetId, opts: TagOptions) -> Result<Prepared, FlickrError> {
    let a = cat.read(|tx| tx.assets().require(asset))?;
    let keywords = cat.read(|tx| tx.tags().for_asset(asset))?;
    let name = file_name(cat, asset)?.unwrap_or_default();
    let title = Path::new(&name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Prepared {
        asset,
        title,
        description: a.comment.clone().unwrap_or_default(),
        tags: tags::suggested(&a, &keywords, opts),
        video: a.media_kind == MediaKind::Video,
        uploads: cat.read(|tx| tx.flickr().uploads(asset))?,
        name,
    })
}

/// One photo to upload, as the user left it in the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub asset: AssetId,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
}

/// Queue `requests` (again for photos uploaded or failed before). Videos
/// are left out. Returns how many were queued.
pub fn enqueue(
    cat: &mut Catalog,
    requests: &[Request],
    privacy: Privacy,
) -> Result<u64, FlickrError> {
    let now = crate::jobs::now_ms();
    let mut n = 0;
    for chunk in requests.chunks(crate::render::ENQUEUE_CHUNK) {
        n += cat.write(|tx| {
            let mut jobs = Vec::new();
            for (i, r) in chunk.iter().enumerate() {
                let a = tx.assets().require(r.asset)?;
                if a.media_kind == MediaKind::Video {
                    continue;
                }
                tx.flickr().put_queued(&FlickrQueued {
                    asset_id: r.asset,
                    privacy: privacy.id().to_owned(),
                    title: r.title.trim().to_owned(),
                    description: r.description.trim().to_owned(),
                    tags: tags::dedupe(r.tags.iter().cloned()),
                    queued_ms: now,
                })?;
                jobs.push(NewJob {
                    kind: JOB_FLICKR_UPLOAD,
                    asset_id: r.asset,
                    rank: 0,
                    sort_key: i64::try_from(i).unwrap_or(i64::MAX),
                });
            }
            tx.jobs().enqueue(&jobs, Priority::Recent, now, true)
        })?;
    }
    Ok(n)
}

/// Stop the uploads of `ids` not yet started (one under way finishes).
pub fn stop(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, FlickrError> {
    Ok(cat.write(|tx| {
        let n = tx.jobs().remove_queued_for(JOB_FLICKR_UPLOAD, ids)?;
        for id in ids {
            let running = tx
                .jobs()
                .find(JOB_FLICKR_UPLOAD, *id)?
                .is_some_and(|j| j.state == JobState::Running);
            if !running {
                tx.flickr().remove_queued(*id)?;
            }
        }
        Ok::<_, CatalogError>(n)
    })?)
}

/// The file sent for `asset`: its bytes and the name Flickr is given.
pub fn file_to_send(
    cat: &mut Catalog,
    lib: &Library,
    cache: &Cache,
    tools: &RenderTools,
    asset: AssetId,
) -> Result<(Vec<u8>, String), FlickrError> {
    let src = locate(cat, lib, asset)?;
    if src.media_kind == MediaKind::Video {
        return Err(FlickrError::Refused(
            "Videos are not uploaded to Flickr (photos only).".into(),
        ));
    }
    let name = src
        .still
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "photo.jpg".into());
    let as_is = matches!(
        lower_ext(&src.still).as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "tif" | "tiff"
    ) && !is_raw(&src.still);
    let (path, name) = if as_is {
        (src.still.clone(), name)
    } else {
        // RAW or HEIF: the full-size JPEG the viewer uses.
        let full = crate::render::full::full_image(
            cat,
            lib,
            cache,
            tools,
            asset,
            crate::render::full::Member::Primary,
        )?;
        let stem = Path::new(&name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "photo".into());
        (full, format!("{stem}.jpg"))
    };
    let size = std::fs::metadata(&path)?.len();
    if size > MAX_PHOTO_BYTES {
        return Err(FlickrError::Refused(format!(
            "{name} is {} MB; Flickr takes photos up to 200 MB.",
            size / (1024 * 1024)
        )));
    }
    Ok((std::fs::read(&path)?, name))
}

/// Upload `asset` as queued; returns the upload to record.
pub fn upload_one(
    cat: &mut Catalog,
    lib: &Library,
    cache: &Cache,
    tools: &RenderTools,
    up: &Uploader,
    asset: AssetId,
) -> Result<FlickrUpload, FlickrError> {
    let Some(q) = cat.read(|tx| tx.flickr().queued(asset))? else {
        return Err(FlickrError::Refused(
            "Nothing to upload: choose Upload to Flickr again.".into(),
        ));
    };
    let app = (up.app)().ok_or(FlickrError::NoApp)?;
    let account = (up.account)().ok_or(FlickrError::NotSignedIn)?;
    let privacy = Privacy::parse(&q.privacy).unwrap_or_default();
    let (bytes, name) = file_to_send(cat, lib, cache, tools, asset)?;
    let params = UploadParams {
        title: q.title.clone(),
        description: q.description.clone(),
        tags: q.tags.clone(),
        privacy,
    };
    let photo_id = up.client.upload(&app, &account, &bytes, &name, &params)?;
    Ok(FlickrUpload {
        asset_id: asset,
        url: client::photo_url(&account.nsid, &photo_id),
        user_nsid: account.nsid,
        photo_id,
        privacy: privacy.id().to_owned(),
        tags: q.tags,
        uploaded_ms: crate::jobs::now_ms(),
    })
}

/// Record a finished upload (and forget what waited for it).
pub fn record(cat: &mut Catalog, u: &FlickrUpload) -> Result<(), CatalogError> {
    cat.write(|tx| tx.flickr().record(u))
}

/// Where one photo's upload stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Queued (`retry`: why the last try failed; it is tried again).
    Waiting {
        retry: Option<String>,
    },
    Uploading,
    /// Uploaded: the newest upload.
    Done(FlickrUpload),
    /// Not uploaded, and why.
    Failed(String),
    /// Never asked (or stopped).
    None,
}

/// Where the upload of `asset` stands.
pub fn state(cat: &Catalog, asset: AssetId) -> Result<State, FlickrError> {
    let job = cat.read(|tx| tx.jobs().find(JOB_FLICKR_UPLOAD, asset))?;
    let newest = || -> Result<Option<FlickrUpload>, FlickrError> {
        Ok(cat
            .read(|tx| tx.flickr().uploads(asset))?
            .into_iter()
            .next())
    };
    Ok(match job {
        None => newest()?.map_or(State::None, State::Done),
        Some(j) => match j.state {
            JobState::Queued => State::Waiting { retry: j.error },
            JobState::Running => State::Uploading,
            JobState::Failed => State::Failed(j.error.unwrap_or_default()),
            JobState::Done => newest()?.map_or(State::None, State::Done),
        },
    })
}

/// The error the worker records for a failed upload: retried (network,
/// Flickr unavailable), deferred (the photo's drive is offline) or final.
pub fn to_render_error(e: FlickrError) -> RenderError {
    match e {
        FlickrError::Render(r) => r,
        FlickrError::Catalog(c) => RenderError::Catalog(c),
        e if e.is_retryable() => RenderError::Failed(e.to_string()),
        e => RenderError::Refused(e.to_string()),
    }
}
