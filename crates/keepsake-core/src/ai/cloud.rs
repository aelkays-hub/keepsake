//! Tag with Claude (ADR-036): a photo the user picks is sent to the Claude
//! API, which returns tags from Keepsake's list plus up to
//! [`MAX_NEW_TAGS`] of its own.
//!
//! - Only what the user chose is sent, one `ai.claude` job per photo, run
//!   by its own worker in the render service.
//! - What is sent is the 1024 px thumbnail from the render cache, encoded
//!   again as JPEG, never the original (which is not read or changed).
//! - The tags become the user's keywords (in the catalog and the XMP
//!   sidecars, like typed ones): list tags as they are, new ones under
//!   [`NEW_TAG_PARENT`]. They cost money, so they are kept like the user's
//!   own work, not as a cache that the on-device AI may replace.
//! - The API key and the model are looked up for each job ([`Claude`]), so
//!   a key saved in Settings works at once.

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
/// The Messages API.
pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// The API version sent.
pub const API_VERSION: &str = "2026-10-21";
/// Most new tags (not on the list) Claude may give one photo.
pub const MAX_NEW_TAGS: usize = 5;
/// The parent of the new tags: `AI|Eiffel Tower`.
pub const NEW_TAG_PARENT: &str = "AI";
/// Longest new tag kept (characters).
const MAX_TAG_CHARS: usize = 40;

/// The Claude models the user can choose from (Settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClaudeModel {
    #[default]
    Opus55,
    Fable51,
    Haiku55,
}

impl ClaudeModel {
    pub const ALL: [ClaudeModel; 3] = [Self::Opus55, Self::Fable51, Self::Haiku55];

    /// The API's model id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Opus55 => "claude-opus-5-5",
            Self::Fable51 => "claude-fable-5-1",
            Self::Haiku55 => "claude-haiku-5-5",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    /// For people: "Claude Opus 5.5".
    pub fn name(self) -> &'static str {
        match self {
            Self::Opus55 => "Claude Opus 5.5",
            Self::Fable51 => "Claude Fable 5.1",
            Self::Haiku55 => "Claude Haiku 5.5",
        }
    }

    /// Roughly what one photo costs, in US cents (about 1,500 image and
    /// 1,500 prompt tokens in, a few hundred out, at the list price).
    pub fn cents_per_photo(self) -> f64 {
        match self {
            Self::Opus55 => 2.0,
            Self::Fable51 => 6.0,
            Self::Haiku55 => 0.1,
        }
    }

    /// Whether the request asks for the API's fallback when the model
    /// declines (Haiku has none).
    fn fallbacks(self) -> bool {
        !matches!(self, Self::Haiku55)
    }
}

/// Does `s` look like a Claude API key? (Checked before it is saved.)
pub fn looks_like_key(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("sk-ant-")
        && s.len() >= 20
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// What Claude gave one photo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClaudeTags {
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

/// Tags one photo (the JPEG bytes) from the tag names given.
pub trait Tagger: Send + Sync {
    fn tag(&self, jpeg: &[u8], names: &[String]) -> Result<ClaudeTags, AiError>;
}

type KeyFn = dyn Fn() -> Option<String> + Send + Sync;
type ModelFn = dyn Fn() -> ClaudeModel + Send + Sync;

/// [`Tagger`] with the Claude API. The key and the model are asked for
/// each photo.
pub struct Claude {
    http: Arc<dyn Http>,
    key: Arc<KeyFn>,
    model: Arc<ModelFn>,
}

impl Claude {
    pub fn new(http: Arc<dyn Http>, key: Arc<KeyFn>, model: Arc<ModelFn>) -> Self {
        Self { http, key, model }
    }
}

impl Tagger for Claude {
    fn tag(&self, jpeg: &[u8], names: &[String]) -> Result<ClaudeTags, AiError> {
        let key = (self.key)().ok_or(AiError::NoApiKey)?;
        let model = (self.model)();
        let body = request(model, jpeg, names).to_string();
        let mut headers = vec![
            ("content-type", "application/json"),
            ("x-api-key", key.as_str()),
            ("anthropic-version", API_VERSION),
        ];
        if model.fallbacks() {
            headers.push(("anthropic-beta", "server-side-fallback-2026-07-01"));
        }
        let (status, text) = self
            .http
            .post(API_URL, &headers, body.as_bytes())
            .map_err(|e| AiError::Cloud {
                message: format!("could not reach Claude: {e}"),
                retry: true,
            })?;
        parse_response(status, &text, names)
    }
}

const SYSTEM: &str = "You tag photos in a family's personal photo library so they can find them later.

Put in `tags` every tag from the list that clearly applies to the photo. Leave out any you are unsure of.

Then put in `new_tags` up to 5 short tags of your own for clear, specific things the list does not cover: a landmark or place you recognise with confidence, an object, an animal's kind, an activity, an occasion. Write each in Title Case, singular, in at most three words (\"Eiffel Tower\", \"Birthday Cake\", \"Sailing\"). Leave `new_tags` empty when the list covers the photo.

Never name or guess who a person is, and never tag a person's age, ethnicity, religion, health or other personal traits.";

/// The request body for one photo.
pub fn request(model: ClaudeModel, jpeg: &[u8], names: &[String]) -> Value {
    let mut body = json!({
        "model": model.id(),
        "max_tokens": 16000,
        "system": SYSTEM,
        "output_config": {
            "effort": "low",
            "format": {
                "type": "json_schema",
                "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["tags", "new_tags"],
                    "properties": {
                        "tags": {"type": "array", "items": {"type": "string", "enum": names}},
                        "new_tags": {"type": "array", "items": {"type": "string"}},
                    },
                },
            },
        },
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image", "source": {
                    "type": "base64", "media_type": "image/jpeg", "data": base64(jpeg),
                }},
                {"type": "text", "text": "Tag this photo."},
            ],
        }],
    });
    if model.fallbacks() {
        body["fallbacks"] = json!("default");
    }
    body
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    new_tags: Vec<String>,
}

/// Read the API's answer: tags kept only if they are on the list, new tags
/// cleaned up and at most [`MAX_NEW_TAGS`].
pub fn parse_response(status: u16, body: &str, names: &[String]) -> Result<ClaudeTags, AiError> {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let api_message = v["error"]["message"].as_str().unwrap_or("").to_owned();
    let fail = |message: String, retry: bool| AiError::Cloud { message, retry };
    match status {
        200 => {}
        401 | 403 => {
            return Err(fail(
                "Claude did not accept the API key: check it in Settings".into(),
                false,
            ))
        }
        429 | 529 => return Err(fail("Claude is busy: trying again soon".into(), true)),
        s if s >= 500 => return Err(fail(format!("Claude had a problem ({s})"), true)),
        s => {
            let m = if api_message.is_empty() {
                format!("Claude refused the request ({s})")
            } else {
                format!("Claude refused the request: {api_message}")
            };
            return Err(fail(m, false));
        }
    }
    match v["stop_reason"].as_str() {
        Some("refusal") => return Err(fail("Claude declined to tag this photo".into(), false)),
        Some("max_tokens") => return Err(fail("Claude's answer was cut short".into(), true)),
        _ => {}
    }
    let text = v["content"]
        .as_array()
        .and_then(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .rev()
                .find_map(|b| b["text"].as_str())
        })
        .ok_or_else(|| fail("Claude's answer had no tags".into(), true))?;
    let answer: Answer = serde_json::from_str(text)
        .map_err(|e| fail(format!("Claude's answer could not be read: {e}"), true))?;
    let mut out = ClaudeTags {
        model: v["model"].as_str().unwrap_or("").to_owned(),
        ..ClaudeTags::default()
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

/// The keyword paths for what Claude gave: list tags (and the tags they
/// imply, as on-device: Portrait gives People), then new ones under
/// [`NEW_TAG_PARENT`].
pub fn keyword_paths(t: &ClaudeTags) -> Result<Vec<TagPath>, AiError> {
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

/// The `ai.claude` job: send the asset's thumbnail to `tagger` and add what
/// it gives to the asset's keywords (those it has stay). Returns the
/// keywords added, as `A|B`.
pub fn tag_asset(
    cat: &mut Catalog,
    lib: &Library,
    cache: &Cache,
    tagger: &dyn Tagger,
    asset: AssetId,
) -> Result<Vec<String>, AiError> {
    let img = analyze::thumbnail(cat, cache, asset)?;
    let jpeg = crate::render::still::encode_jpeg(&img)?;
    drop(img);
    let names = labels::parse_names(labels::TSV)?;
    let got = tagger.tag(&jpeg, &names)?;
    let paths = keyword_paths(&got)?;
    let mut svc = MetadataService::new(lib, cat);
    let mut keywords = svc.user_tags(asset).map_err(lib_err)?;
    let mut added = Vec::new();
    for p in paths {
        if !keywords.contains(&p) {
            added.push(p.to_lightroom());
            keywords.push(p);
        }
    }
    if !added.is_empty() {
        svc.set_user_tags(asset, &keywords).map_err(lib_err)?;
    }
    tracing::info!(asset = %asset, model = %got.model, "tagged with Claude: {added:?}");
    Ok(added)
}

fn lib_err(e: crate::library::LibraryError) -> AiError {
    match e {
        crate::library::LibraryError::Catalog(e) => AiError::Catalog(e),
        e => AiError::Image(e.to_string()),
    }
}

/// How far tagging some photos with Claude has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClaudeStatus {
    /// Photos asked for.
    pub items: u64,
    /// Of those, tagged.
    pub done: u64,
    /// Still to send (queued, waiting for a thumbnail or being sent).
    pub pending: u64,
    /// Not tagged, for good.
    pub failed: u64,
}

/// Queue `ids` to be tagged with Claude (again, if they were before).
pub fn enqueue(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    let now = crate::jobs::now_ms();
    let jobs: Vec<NewJob<'_>> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| NewJob {
            kind: JOB_AI_CLAUDE,
            asset_id: *id,
            rank: 0,
            sort_key: i as i64,
        })
        .collect();
    Ok(cat.write(|tx| tx.jobs().enqueue(&jobs, Priority::Recent, now, true))?)
}

/// How `ids` are getting on, and the latest problem (a failed photo's, or
/// one being tried again).
pub fn status(cat: &Catalog, ids: &[AssetId]) -> Result<(ClaudeStatus, Option<String>), AiError> {
    use crate::catalog::JobState;
    let mut st = ClaudeStatus {
        items: ids.len() as u64,
        ..ClaudeStatus::default()
    };
    let mut problem = None;
    cat.read(|tx| {
        for id in ids {
            let Some(job) = tx.jobs().find(JOB_AI_CLAUDE, *id)? else {
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

/// Stop: drop the queued jobs of `ids` (photos tagged stay tagged).
pub fn stop(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    Ok(cat.write(|tx| tx.jobs().remove_queued_for(JOB_AI_CLAUDE, ids))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["People", "Beach", "Dog", "Portrait"]
            .map(String::from)
            .to_vec()
    }

    fn ok(text: &str) -> String {
        json!({
            "model": "claude-opus-5-5",
            "stop_reason": "end_turn",
            "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": text}],
        })
        .to_string()
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
    fn keys_are_checked_before_they_are_saved() {
        assert!(looks_like_key("sk-ant-api03-abcdefghijklmnop_QR-12"));
        assert!(looks_like_key("  sk-ant-api03-abcdefghijklmnop  "));
        assert!(!looks_like_key("sk-ant-short"));
        assert!(!looks_like_key("hello there, this is not a key"));
        assert!(!looks_like_key("sk-ant-api03-abc def ghi jkl mno"));
    }

    #[test]
    fn the_request_sends_the_photo_the_list_and_a_schema() {
        let r = request(ClaudeModel::Opus55, b"foo", &names());
        assert_eq!(r["model"], "claude-opus-5-5");
        assert_eq!(r["fallbacks"], "default");
        assert_eq!(r["output_config"]["effort"], "low");
        let schema = &r["output_config"]["format"]["schema"];
        assert_eq!(schema["properties"]["tags"]["items"]["enum"][3], "Portrait");
        assert_eq!(schema["additionalProperties"], false);
        let content = &r["messages"][0]["content"];
        assert_eq!(content[0]["source"]["media_type"], "image/jpeg");
        assert_eq!(content[0]["source"]["data"], "Zm9v");
        // No sampling settings (rejected on this API version).
        assert!(r.get("temperature").is_none());
        let h = request(ClaudeModel::Haiku55, b"foo", &names());
        assert_eq!(h["model"], "claude-haiku-5-5");
        assert!(h.get("fallbacks").is_none());
        assert_eq!(
            ClaudeModel::parse("claude-fable-5-1"),
            Some(ClaudeModel::Fable51)
        );
        assert_eq!(ClaudeModel::parse("gpt"), None);
    }

    #[test]
    fn the_answer_keeps_list_tags_and_tidies_new_ones() {
        let body = ok(r#"{"tags": ["Beach", "Unicorn", "beach", "People"],
                         "new_tags": ["Eiffel  Tower", "dog", "A|B", "", "Kite", "Sailing",
                                      "Sand Castle", "Sunset Glow", "Too Many",
                                      "a very very long tag that goes on and on and on"]}"#);
        let t = parse_response(200, &body, &names()).unwrap();
        assert_eq!(t.tags, ["Beach", "People", "Dog"]);
        assert_eq!(
            t.new_tags,
            ["Eiffel Tower", "AB", "Kite", "Sailing", "Sand Castle"]
        );
        assert_eq!(t.model, "claude-opus-5-5");
    }

    #[test]
    fn problems_say_whether_trying_again_helps() {
        let retry = |s: u16, b: &str| match parse_response(s, b, &names()) {
            Err(AiError::Cloud { retry, message }) => (retry, message),
            other => panic!("{other:?}"),
        };
        assert!(!retry(401, "{}").0);
        assert!(retry(401, "{}").1.contains("API key"));
        assert!(retry(429, "{}").0);
        assert!(retry(529, "{}").0);
        assert!(retry(500, "").0);
        let (r, m) = retry(400, r#"{"error": {"message": "image too large"}}"#);
        assert!(!r && m.contains("image too large"), "{m}");
        let refused = json!({"stop_reason": "refusal", "content": []}).to_string();
        assert_eq!(
            retry(200, &refused),
            (false, "Claude declined to tag this photo".into())
        );
        assert!(retry(200, &ok("not json")).0);
    }

    #[test]
    fn keywords_get_implied_tags_and_new_ones_go_under_ai() {
        let t = ClaudeTags {
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
    type Sent = (String, Vec<(String, String)>, String);

    struct Fake(std::sync::Mutex<Vec<Sent>>);

    impl Http for Fake {
        fn post(
            &self,
            url: &str,
            headers: &[(&str, &str)],
            body: &[u8],
        ) -> Result<(u16, String), String> {
            self.0.lock().map_err(|e| e.to_string())?.push((
                url.into(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                String::from_utf8_lossy(body).into_owned(),
            ));
            Ok((200, ok(r#"{"tags": ["Dog"], "new_tags": []}"#)))
        }
    }

    #[test]
    fn claude_sends_the_key_and_needs_one() {
        let http = Arc::new(Fake(Default::default()));
        let c = Claude::new(
            http.clone(),
            Arc::new(|| Some("sk-ant-test-key-000000".into())),
            Arc::new(|| ClaudeModel::Fable51),
        );
        assert_eq!(c.tag(b"x", &names()).unwrap().tags, ["Dog"]);
        let sent = http.0.lock().unwrap();
        let (url, headers, body) = &sent[0];
        assert_eq!(url, API_URL);
        let h = |k: &str| {
            headers
                .iter()
                .find(|(a, _)| a == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(h("x-api-key"), Some("sk-ant-test-key-000000"));
        assert_eq!(h("anthropic-version"), Some(API_VERSION));
        assert_eq!(h("anthropic-beta"), Some("server-side-fallback-2026-07-01"));
        assert!(body.contains("claude-fable-5-1"));
        let none = Claude::new(
            http.clone(),
            Arc::new(|| None),
            Arc::new(ClaudeModel::default),
        );
        assert!(matches!(none.tag(b"x", &names()), Err(AiError::NoApiKey)));
    }
}
