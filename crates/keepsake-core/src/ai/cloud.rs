//! Tag with AI (ADR-036, ADR-038): a photo the user picks is sent to the AI
//! they choose, Claude (Anthropic), ChatGPT (OpenAI) or Gemini (Google),
//! which returns tags from Keepsake's list plus up to [`MAX_NEW_TAGS`] of
//! its own.
//!
//! - Only what the user chose is sent, one job per photo and provider
//!   (`ai.claude`, `ai.openai`, `ai.gemini`), run by one worker in the
//!   render service.
//! - What is sent is the 1024 px thumbnail from the render cache, encoded
//!   again as JPEG, never the original (which is not read or changed).
//! - The tags become the user's keywords (in the catalog and the XMP
//!   sidecars, like typed ones): list tags as they are, new ones under
//!   [`NEW_TAG_PARENT`]. They cost money, so they are kept like the user's
//!   own work, not as a cache that the on-device AI may replace.
//! - The API keys and models are looked up for each job ([`CloudTagger`]),
//!   so a key saved in Settings works at once.
//! - Each provider's request and answer are in its own module; the prompt,
//!   the tag list, tidying the answer and the job are shared here.

pub mod claude;
pub mod gemini;
pub mod openai;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{analyze, labels, AiError};
use crate::catalog::{AssetId, Catalog, NewJob, Priority, TagPath};
use crate::library::Library;
use crate::metadata::MetadataService;
use crate::render::Cache;

/// The job kind: tag one asset with Claude.
pub const JOB_AI_CLAUDE: &str = "ai.claude";
/// The job kind: tag one asset with ChatGPT.
pub const JOB_AI_OPENAI: &str = "ai.openai";
/// The job kind: tag one asset with Gemini.
pub const JOB_AI_GEMINI: &str = "ai.gemini";
/// Every cloud tagging job kind (one worker runs them all).
pub const CLOUD_JOBS: [&str; 3] = [JOB_AI_CLAUDE, JOB_AI_OPENAI, JOB_AI_GEMINI];
/// Most new tags (not on the list) one photo may get.
pub const MAX_NEW_TAGS: usize = 5;
/// The parent of the new tags: `AI|Eiffel Tower`.
pub const NEW_TAG_PARENT: &str = "AI";
/// Longest new tag kept (characters).
const MAX_TAG_CHARS: usize = 40;

/// The companies whose AI can tag photos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Provider {
    #[default]
    Claude,
    OpenAi,
    Gemini,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Self::Claude, Self::OpenAi, Self::Gemini];

    /// The id in settings, commands and the key store: `openai`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// For people: "ChatGPT".
    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::OpenAi => "ChatGPT",
            Self::Gemini => "Gemini",
        }
    }

    /// Who runs it (and charges for it): "OpenAI".
    pub fn company(self) -> &'static str {
        match self {
            Self::Claude => "Anthropic",
            Self::OpenAi => "OpenAI",
            Self::Gemini => "Google",
        }
    }

    /// Its job kind.
    pub fn job_kind(self) -> &'static str {
        match self {
            Self::Claude => JOB_AI_CLAUDE,
            Self::OpenAi => JOB_AI_OPENAI,
            Self::Gemini => JOB_AI_GEMINI,
        }
    }

    /// The provider whose job kind this is.
    pub fn of_job(kind: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.job_kind() == kind)
    }

    /// The models it offers, best first.
    pub fn models(self) -> impl Iterator<Item = CloudModel> {
        CloudModel::ALL
            .into_iter()
            .filter(move |m| m.provider() == self)
    }

    /// The model used until the user picks one.
    pub fn default_model(self) -> CloudModel {
        match self {
            Self::Claude => CloudModel::ClaudeOpus55,
            Self::OpenAi => CloudModel::Gpt61Sol,
            Self::Gemini => CloudModel::Gemini36Flash,
        }
    }

    /// Does `s` look like this provider's API key? (Checked before it is
    /// saved, so a key pasted in the wrong box is caught.)
    pub fn looks_like_key(self, s: &str) -> bool {
        let s = s.trim();
        let charset = s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        charset
            && s.len() >= 20
            && match self {
                Self::Claude => s.starts_with("sk-ant-"),
                Self::OpenAi => s.starts_with("sk-") && !s.starts_with("sk-ant-"),
                // Classic keys start with AIza; newer ones (AQ.…) have dots.
                Self::Gemini => !s.starts_with("sk-") && s.len() <= 256,
            }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The models the user can choose from (Settings), for every provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudModel {
    ClaudeOpus55,
    ClaudeFable51,
    ClaudeHaiku55,
    Gpt6Astra,
    Gpt61Sol,
    Gpt6Luna,
    Gemini4Argon,
    Gemini36Flash,
    Gemini35FlashLite,
}

impl CloudModel {
    pub const ALL: [CloudModel; 9] = [
        Self::ClaudeOpus55,
        Self::ClaudeFable51,
        Self::ClaudeHaiku55,
        Self::Gpt6Astra,
        Self::Gpt61Sol,
        Self::Gpt6Luna,
        Self::Gemini4Argon,
        Self::Gemini36Flash,
        Self::Gemini35FlashLite,
    ];

    pub fn provider(self) -> Provider {
        match self {
            Self::ClaudeOpus55 | Self::ClaudeFable51 | Self::ClaudeHaiku55 => Provider::Claude,
            Self::Gpt6Astra | Self::Gpt61Sol | Self::Gpt6Luna => Provider::OpenAi,
            Self::Gemini4Argon | Self::Gemini36Flash | Self::Gemini35FlashLite => Provider::Gemini,
        }
    }

    /// The API's model id.
    pub fn id(self) -> &'static str {
        match self {
            Self::ClaudeOpus55 => "claude-opus-5-5",
            Self::ClaudeFable51 => "claude-fable-5-1",
            Self::ClaudeHaiku55 => "claude-haiku-5-5",
            Self::Gpt6Astra => "gpt-6-astra",
            Self::Gpt61Sol => "gpt-6.1-sol",
            Self::Gpt6Luna => "gpt-6-luna",
            Self::Gemini4Argon => "gemini-4-argon",
            Self::Gemini36Flash => "gemini-3.6-flash",
            Self::Gemini35FlashLite => "gemini-3.5-flash-lite",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    /// For people: "GPT-6.1 Sol".
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeOpus55 => "Claude Opus 5.5",
            Self::ClaudeFable51 => "Claude Fable 5.1",
            Self::ClaudeHaiku55 => "Claude Haiku 5.5",
            Self::Gpt6Astra => "GPT-6 Astra",
            Self::Gpt61Sol => "GPT-6.1 Sol",
            Self::Gpt6Luna => "GPT-6 Luna",
            Self::Gemini4Argon => "Gemini 4 Argon",
            Self::Gemini36Flash => "Gemini 3.6 Flash",
            Self::Gemini35FlashLite => "Gemini 3.5 Flash-Lite",
        }
    }

    /// Roughly what one photo costs, in US cents: about 1,100–1,500 image
    /// and 1,500 prompt tokens in, a few hundred out, at the list price
    /// (November 2026; for Gemini the prices from December 2026 and
    /// January 2027, when the introductory ones end).
    pub fn cents_per_photo(self) -> f64 {
        match self {
            Self::ClaudeOpus55 => 2.0,
            Self::ClaudeFable51 => 6.0,
            Self::ClaudeHaiku55 => 0.1,
            Self::Gpt6Astra => 5.0,
            Self::Gpt61Sol => 1.0,
            Self::Gpt6Luna => 0.05,
            Self::Gemini4Argon => 2.0,
            Self::Gemini36Flash => 0.8,
            Self::Gemini35FlashLite => 0.2,
        }
    }
}

/// What the AI gave one photo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CloudTags {
    /// From the list, as named there.
    pub tags: Vec<String>,
    /// Its own, without the parent: "Eiffel Tower".
    pub new_tags: Vec<String>,
    /// The model that answered (a fallback model, if the first declined).
    pub model: String,
}

/// Sends a request: `(status, body)`, or why it could not be sent.
pub trait Http: Send + Sync {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<(u16, String), String>;
}

/// [`Http`] over HTTPS with ureq.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(180)))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for UreqHttp {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<(u16, String), String> {
        let mut req = self.agent.post(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let mut resp = req.send(body).map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        Ok((status, text))
    }
}

/// Tags one photo (the JPEG bytes) from the tag names given, with one
/// provider's AI.
pub trait Tagger: Send + Sync {
    fn tag(&self, provider: Provider, jpeg: &[u8], names: &[String]) -> Result<CloudTags, AiError>;
}

type KeyFn = dyn Fn(Provider) -> Option<String> + Send + Sync;
type ModelFn = dyn Fn(Provider) -> CloudModel + Send + Sync;

/// [`Tagger`] with the providers' APIs. The key and the model are asked
/// for each photo.
pub struct CloudTagger {
    http: Arc<dyn Http>,
    key: Arc<KeyFn>,
    model: Arc<ModelFn>,
}

impl CloudTagger {
    pub fn new(http: Arc<dyn Http>, key: Arc<KeyFn>, model: Arc<ModelFn>) -> Self {
        Self { http, key, model }
    }
}

impl Tagger for CloudTagger {
    fn tag(&self, provider: Provider, jpeg: &[u8], names: &[String]) -> Result<CloudTags, AiError> {
        let key = (self.key)(provider).ok_or(AiError::NoApiKey(provider))?;
        let model = Some((self.model)(provider))
            .filter(|m| m.provider() == provider)
            .unwrap_or_else(|| provider.default_model());
        let (url, headers, body) = match provider {
            Provider::Claude => claude::call(model, &key, jpeg, names),
            Provider::OpenAi => openai::call(model, &key, jpeg, names),
            Provider::Gemini => gemini::call(model, &key, jpeg, names),
        };
        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (status, text) = self
            .http
            .post(&url, &headers, body.to_string().as_bytes())
            .map_err(|e| AiError::Cloud {
                message: format!("could not reach {provider}: {e}"),
                retry: true,
            })?;
        match provider {
            Provider::Claude => claude::parse_response(status, &text, names),
            Provider::OpenAi => openai::parse_response(status, &text, names),
            Provider::Gemini => gemini::parse_response(status, &text, names),
        }
    }
}

/// What every provider is told. Kept the same for all, so their tags are
/// alike.
pub(crate) const SYSTEM: &str = "You tag photos in a family's personal photo library so they can find them later.

Put in `tags` every tag from the list that clearly applies to the photo. Leave out any you are unsure of.

Then put in `new_tags` up to 5 short tags of your own for clear, specific things the list does not cover: a landmark or place you recognise with confidence, an object, an animal's kind, an activity, an occasion. Write each in Title Case, singular, in at most three words (\"Eiffel Tower\", \"Birthday Cake\", \"Sailing\"). Leave `new_tags` empty when the list covers the photo.

Never name or guess who a person is, and never tag a person's age, ethnicity, religion, health or other personal traits.";

/// What the photo is sent with.
pub(crate) const ASK: &str = "Tag this photo.";

/// The answer's JSON schema: `tags` from the list, `new_tags` free.
pub(crate) fn schema(names: &[String]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["tags", "new_tags"],
        "properties": {
            "tags": {"type": "array", "items": {"type": "string", "enum": names}},
            "new_tags": {"type": "array", "items": {"type": "string"}},
        },
    })
}

/// A request that failed: whether trying again later may help.
pub(crate) fn fail(message: String, retry: bool) -> AiError {
    AiError::Cloud { message, retry }
}

/// What an HTTP status other than 200 means, the same way for every
/// provider: a refused key, busy, broken, or the request refused.
pub(crate) fn http_error(provider: Provider, status: u16, api_message: &str) -> AiError {
    match status {
        401 | 403 => fail(
            format!("{provider} did not accept the API key: check it in Settings"),
            false,
        ),
        429 | 529 => fail(format!("{provider} is busy: trying again soon"), true),
        s if s >= 500 => fail(format!("{provider} had a problem ({s})"), true),
        s if api_message.is_empty() => fail(format!("{provider} refused the request ({s})"), false),
        _ => fail(
            format!("{provider} refused the request: {api_message}"),
            false,
        ),
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    new_tags: Vec<String>,
}

/// Read the answer's JSON text: tags kept only if they are on the list, new
/// tags cleaned up and at most [`MAX_NEW_TAGS`].
pub(crate) fn tidy(
    provider: Provider,
    text: &str,
    names: &[String],
    model: &str,
) -> Result<CloudTags, AiError> {
    let answer: Answer = serde_json::from_str(text.trim())
        .map_err(|e| fail(format!("{provider}'s answer could not be read: {e}"), true))?;
    let mut out = CloudTags {
        model: model.to_owned(),
        ..CloudTags::default()
    };
    let known = |t: &str| names.iter().find(|n| n.eq_ignore_ascii_case(t.trim()));
    for t in &answer.tags {
        if let Some(n) = known(t) {
            if !out.tags.contains(n) {
                out.tags.push(n.clone());
            }
        }
    }
    for t in &answer.new_tags {
        // A "new" tag that is on the list is a list tag.
        if let Some(n) = known(t) {
            if !out.tags.contains(n) {
                out.tags.push(n.clone());
            }
            continue;
        }
        let clean: String = t
            .chars()
            .filter(|c| !c.is_control() && *c != '|')
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if clean.is_empty() || clean.chars().count() > MAX_TAG_CHARS {
            continue;
        }
        if out.new_tags.len() < MAX_NEW_TAGS
            && !out.new_tags.iter().any(|x| x.eq_ignore_ascii_case(&clean))
        {
            out.new_tags.push(clean);
        }
    }
    Ok(out)
}

/// Standard base64 (RFC 4648, with padding).
pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The keyword paths for what the AI gave: list tags (and the tags they
/// imply, as on-device: Portrait gives People), then new ones under
/// [`NEW_TAG_PARENT`].
pub fn keyword_paths(t: &CloudTags) -> Result<Vec<TagPath>, AiError> {
    let entries = labels::builtin_entries()?;
    let mut out: Vec<TagPath> = Vec::new();
    let mut add = |p: TagPath| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for name in &t.tags {
        add(TagPath::new([name.as_str()])?);
        if let Some(i) = entries
            .iter()
            .find(|e| &e.name == name)
            .and_then(|e| e.implies.as_deref())
        {
            add(TagPath::new([i])?);
        }
    }
    for name in &t.new_tags {
        add(TagPath::new([NEW_TAG_PARENT, name.as_str()])?);
    }
    Ok(out)
}

/// One cloud job: send the asset's thumbnail to `provider` through `tagger`
/// and add what it gives to the asset's keywords (those it has stay),
/// recording that `provider` gave them (ADR-039). Returns the keywords
/// added, as `A|B`.
pub fn tag_asset(
    cat: &mut Catalog,
    lib: &Library,
    cache: &Cache,
    tagger: &dyn Tagger,
    provider: Provider,
    asset: AssetId,
) -> Result<Vec<String>, AiError> {
    let img = analyze::thumbnail(cat, cache, asset)?;
    let jpeg = crate::render::still::encode_jpeg(&img)?;
    drop(img);
    let names = labels::parse_names(labels::TSV)?;
    let got = tagger.tag(provider, &jpeg, &names)?;
    let paths = keyword_paths(&got)?;
    let added: Vec<String> = MetadataService::new(lib, cat)
        .add_ai_keywords(asset, provider.id(), &paths)
        .map_err(lib_err)?
        .iter()
        .map(TagPath::to_lightroom)
        .collect();
    tracing::info!(asset = %asset, provider = provider.id(), model = %got.model, "tagged: {added:?}");
    Ok(added)
}

fn lib_err(e: crate::library::LibraryError) -> AiError {
    match e {
        crate::library::LibraryError::Catalog(e) => AiError::Catalog(e),
        e => AiError::Image(e.to_string()),
    }
}

/// How far tagging some photos with one provider has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CloudStatus {
    /// Photos asked for.
    pub items: u64,
    /// Of those, tagged.
    pub done: u64,
    /// Still to send (queued, waiting for a thumbnail or being sent).
    pub pending: u64,
    /// Not tagged, for good.
    pub failed: u64,
}

/// Queue `ids` to be tagged by `provider` (again, if they were before).
pub fn enqueue(cat: &mut Catalog, provider: Provider, ids: &[AssetId]) -> Result<u64, AiError> {
    let now = crate::jobs::now_ms();
    let jobs: Vec<NewJob<'_>> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| NewJob {
            kind: provider.job_kind(),
            asset_id: *id,
            rank: 0,
            sort_key: i as i64,
        })
        .collect();
    Ok(cat.write(|tx| tx.jobs().enqueue(&jobs, Priority::Recent, now, true))?)
}

/// How `ids` are getting on with `provider`, and the latest problem (a
/// failed photo's, or one being tried again).
pub fn status(
    cat: &Catalog,
    provider: Provider,
    ids: &[AssetId],
) -> Result<(CloudStatus, Option<String>), AiError> {
    job_status(cat, provider.job_kind(), ids)
}

/// How the jobs of kind `kind` for `ids` are getting on, and the latest
/// problem (shared with the aircraft reader, ADR-041).
pub fn job_status(
    cat: &Catalog,
    kind: &str,
    ids: &[AssetId],
) -> Result<(CloudStatus, Option<String>), AiError> {
    use crate::catalog::JobState;
    let mut st = CloudStatus {
        items: ids.len() as u64,
        ..CloudStatus::default()
    };
    let mut problem = None;
    cat.read(|tx| {
        for id in ids {
            let Some(job) = tx.jobs().find(kind, *id)? else {
                continue;
            };
            match job.state {
                JobState::Done => st.done += 1,
                JobState::Failed => {
                    st.failed += 1;
                    if job.error.is_some() {
                        problem = job.error;
                    }
                }
                _ => {
                    st.pending += 1;
                    if let Some(e) = job.error.filter(|e| !e.starts_with("waiting")) {
                        problem = Some(e);
                    }
                }
            }
        }
        Ok::<_, crate::catalog::CatalogError>(())
    })?;
    Ok((st, problem))
}

/// Stop: drop the queued jobs of `ids` for `provider` (photos tagged stay
/// tagged).
pub fn stop(cat: &mut Catalog, provider: Provider, ids: &[AssetId]) -> Result<u64, AiError> {
    Ok(cat.write(|tx| tx.jobs().remove_queued_for(provider.job_kind(), ids))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn names() -> Vec<String> {
        ["People", "Beach", "Dog", "Portrait"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn base64_matches_rfc_4648() {
        for (i, o) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(i.as_bytes()), o);
        }
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn keys_are_checked_against_the_right_provider() {
        let claude = "sk-ant-api03-abcdefghijklmnop_QR-12";
        let openai = "sk-proj-abcdefghijklmnopqrstuvwxyz0123";
        let gemini = "AIzaSyAbcdefghijklmnopqrstuvwxyz012345";
        let gemini_new = "AQ.Ab8RN6Kq-abcdefghijklmnopqrstuvwxyz_0123456789";
        assert!(Provider::Claude.looks_like_key(claude));
        assert!(Provider::Claude.looks_like_key(&format!("  {claude}  ")));
        assert!(!Provider::Claude.looks_like_key(openai));
        assert!(Provider::OpenAi.looks_like_key(openai));
        assert!(!Provider::OpenAi.looks_like_key(claude));
        assert!(!Provider::OpenAi.looks_like_key(gemini));
        assert!(Provider::Gemini.looks_like_key(gemini));
        assert!(
            Provider::Gemini.looks_like_key(gemini_new),
            "keys with dots"
        );
        assert!(!Provider::Claude.looks_like_key(gemini_new));
        assert!(!Provider::OpenAi.looks_like_key(gemini_new));
        assert!(!Provider::Gemini.looks_like_key(openai));
        for p in Provider::ALL {
            assert!(!p.looks_like_key("sk-ant-short"));
            assert!(!p.looks_like_key("hello there, this is not a key"));
            assert!(!p.looks_like_key("sk-ant-api03-abc def ghi jkl mno"));
        }
    }

    #[test]
    fn every_provider_has_its_models_and_job() {
        for p in Provider::ALL {
            assert_eq!(Provider::parse(p.id()), Some(p));
            assert_eq!(Provider::of_job(p.job_kind()), Some(p));
            assert_eq!(p.default_model().provider(), p);
            assert_eq!(p.models().count(), 3, "{p}");
            assert!(CLOUD_JOBS.contains(&p.job_kind()));
        }
        for m in CloudModel::ALL {
            assert_eq!(CloudModel::parse(m.id()), Some(m));
            assert!(m.cents_per_photo() > 0.0);
        }
        assert_eq!(Provider::of_job("ai.analyze"), None);
        assert_eq!(CloudModel::parse("gpt"), None);
        assert_eq!(Provider::OpenAi.to_string(), "ChatGPT");
    }

    #[test]
    fn the_answer_keeps_list_tags_and_tidies_new_ones() {
        let text = r#"{"tags": ["Beach", "Unicorn", "beach", "People"],
                       "new_tags": ["Eiffel  Tower", "dog", "A|B", "", "Kite", "Sailing",
                                    "Sand Castle", "Sunset Glow", "Too Many",
                                    "a very very long tag that goes on and on and on"]}"#;
        let t = tidy(Provider::Gemini, text, &names(), "m").unwrap();
        assert_eq!(t.tags, ["Beach", "People", "Dog"]);
        assert_eq!(
            t.new_tags,
            ["Eiffel Tower", "AB", "Kite", "Sailing", "Sand Castle"]
        );
        assert_eq!(t.model, "m");
        match tidy(Provider::OpenAi, "not json", &names(), "m") {
            Err(AiError::Cloud { retry, message }) => {
                assert!(retry);
                assert!(message.starts_with("ChatGPT's answer"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn http_errors_name_the_provider_and_say_whether_to_retry() {
        let e = |p, s, m| match http_error(p, s, m) {
            AiError::Cloud { retry, message } => (retry, message),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            e(Provider::Gemini, 401, ""),
            (
                false,
                "Gemini did not accept the API key: check it in Settings".into()
            )
        );
        assert!(e(Provider::OpenAi, 429, "").0);
        assert!(e(Provider::Claude, 529, "").0);
        assert!(e(Provider::Claude, 503, "").0);
        let (r, m) = e(Provider::OpenAi, 400, "image too large");
        assert!(
            !r && m == "ChatGPT refused the request: image too large",
            "{m}"
        );
    }

    #[test]
    fn keywords_get_implied_tags_and_new_ones_go_under_ai() {
        let t = CloudTags {
            tags: vec!["Portrait".into()],
            new_tags: vec!["Eiffel Tower".into()],
            model: String::new(),
        };
        let p: Vec<String> = keyword_paths(&t)
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect();
        assert_eq!(p, ["Portrait", "People", "AI|Eiffel Tower"]);
    }

    /// One request: URL, headers, body.
    pub(super) type Sent = (String, Vec<(String, String)>, String);

    /// Stands in for the network: answers every request with `answer`.
    pub(super) struct Fake {
        pub sent: std::sync::Mutex<Vec<Sent>>,
        pub answer: (u16, String),
    }

    impl Fake {
        pub fn new(status: u16, body: String) -> Arc<Self> {
            Arc::new(Self {
                sent: Default::default(),
                answer: (status, body),
            })
        }

        /// The last request's header `k`.
        pub fn header(&self, k: &str) -> Option<String> {
            let sent = self.sent.lock().unwrap();
            sent.last()?
                .1
                .iter()
                .find(|(a, _)| a == k)
                .map(|(_, v)| v.clone())
        }

        pub fn last(&self) -> Sent {
            self.sent.lock().unwrap().last().cloned().unwrap()
        }
    }

    impl Http for Fake {
        fn post(
            &self,
            url: &str,
            headers: &[(&str, &str)],
            body: &[u8],
        ) -> Result<(u16, String), String> {
            self.sent.lock().map_err(|e| e.to_string())?.push((
                url.into(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                String::from_utf8_lossy(body).into_owned(),
            ));
            Ok(self.answer.clone())
        }
    }

    /// A tagger over `http` with a key for every provider and `model`.
    pub(super) fn tagger(http: Arc<Fake>, model: CloudModel) -> CloudTagger {
        CloudTagger::new(
            http,
            Arc::new(|p: Provider| Some(format!("{}-test-key-000000", p.id()))),
            Arc::new(move |_| model),
        )
    }

    #[test]
    fn a_missing_key_names_the_provider() {
        let http = Fake::new(200, String::new());
        let t = CloudTagger::new(
            http.clone(),
            Arc::new(|_| None),
            Arc::new(|_| CloudModel::Gpt6Luna),
        );
        match t.tag(Provider::OpenAi, b"x", &names()) {
            Err(AiError::NoApiKey(Provider::OpenAi)) => {}
            other => panic!("{other:?}"),
        }
        assert!(http.sent.lock().unwrap().is_empty(), "nothing is sent");
        assert_eq!(
            AiError::NoApiKey(Provider::Gemini).to_string(),
            "no Gemini API key: add one in Settings"
        );
    }

    #[test]
    fn a_model_of_another_provider_falls_back_to_the_default() {
        let http = Fake::new(500, String::new());
        let t = tagger(http.clone(), CloudModel::ClaudeHaiku55);
        let _ = t.tag(Provider::Gemini, b"x", &names());
        assert!(
            http.last().0.contains("gemini-3.6-flash"),
            "{}",
            http.last().0
        );
    }

    #[test]
    fn an_unreachable_api_is_tried_again() {
        struct Down;
        impl Http for Down {
            fn post(&self, _: &str, _: &[(&str, &str)], _: &[u8]) -> Result<(u16, String), String> {
                Err("no network".into())
            }
        }
        let t = CloudTagger::new(
            Arc::new(Down),
            Arc::new(|_| Some("k".into())),
            Arc::new(Provider::default_model),
        );
        match t.tag(Provider::Gemini, b"x", &names()) {
            Err(AiError::Cloud {
                retry: true,
                message,
            }) => {
                assert_eq!(message, "could not reach Gemini: no network");
            }
            other => panic!("{other:?}"),
        }
    }
}
