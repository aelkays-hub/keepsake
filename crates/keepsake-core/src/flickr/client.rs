//! Flickr's API (ADR-046): OAuth 1.0a sign-in (request token, the user
//! allows Keepsake on flickr.com and is shown a code, access token), a
//! check that the sign-in works (`flickr.test.login`) and the upload API
//! (`https://up.flickr.com/services/upload/`).
//!
//! Requests go through [`Http`] (tests use a stand-in: nothing is sent),
//! named `Keepsake/<major>.<minor>` with no web address or personal detail.
//! The API key, its secret and the sign-in's token are passed in for each
//! call; they are never logged or put in an error.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::oauth;
use super::FlickrError;
use crate::ai::cloud::Http;

pub const REQUEST_TOKEN_URL: &str = "https://www.flickr.com/services/oauth/request_token";
pub const AUTHORIZE_URL: &str = "https://www.flickr.com/services/oauth/authorize";
pub const ACCESS_TOKEN_URL: &str = "https://www.flickr.com/services/oauth/access_token";
pub const REST_URL: &str = "https://www.flickr.com/services/rest/";
pub const UPLOAD_URL: &str = "https://up.flickr.com/services/upload/";

/// How Keepsake names itself to Flickr: `Keepsake/0.7`.
pub fn user_agent() -> String {
    let v = env!("CARGO_PKG_VERSION");
    let short: Vec<&str> = v.split('.').take(2).collect();
    format!("Keepsake/{}", short.join("."))
}

/// The user's own Flickr app (from Flickr's App Garden): its key and
/// secret.
#[derive(Clone, PartialEq, Eq)]
pub struct App {
    pub key: String,
    pub secret: String,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("App { .. }")
    }
}

/// A signed-in Flickr account: the access token and its secret, and who
/// it is.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub token: String,
    pub secret: String,
    /// Flickr's id of the user (`21207597@N07`).
    pub nsid: String,
    pub username: String,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Account {{ username: {:?}, .. }}", self.username)
    }
}

/// A sign-in under way: the request token (kept in memory only), and the
/// page where the user allows Keepsake.
#[derive(Clone, PartialEq, Eq)]
pub struct Pending {
    pub token: String,
    pub secret: String,
    pub authorize_url: String,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pending { .. }")
    }
}

/// Who may see an uploaded photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privacy {
    Public,
    FriendsFamily,
    Friends,
    Family,
    #[default]
    Private,
}

impl Privacy {
    pub const ALL: [Privacy; 5] = [
        Self::Public,
        Self::FriendsFamily,
        Self::Friends,
        Self::Family,
        Self::Private,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::FriendsFamily => "friends_family",
            Self::Friends => "friends",
            Self::Family => "family",
            Self::Private => "private",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == s)
    }

    /// For people.
    pub fn name(self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::FriendsFamily => "Friends and family",
            Self::Friends => "Friends",
            Self::Family => "Family",
            Self::Private => "Private (only you)",
        }
    }

    /// Flickr's `is_public`, `is_friend`, `is_family`.
    pub fn flags(self) -> (bool, bool, bool) {
        match self {
            Self::Public => (true, false, false),
            Self::FriendsFamily => (false, true, true),
            Self::Friends => (false, true, false),
            Self::Family => (false, false, true),
            Self::Private => (false, false, false),
        }
    }
}

/// What goes with an uploaded photo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UploadParams {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub privacy: Privacy,
}

/// The time (Unix seconds) and a nonce for a request.
pub type Stamp = dyn Fn() -> (i64, String) + Send + Sync;

/// Flickr's API over [`Http`].
pub struct Flickr {
    http: Arc<dyn Http>,
    stamp: Arc<Stamp>,
}

/// The photo's page: `https://www.flickr.com/photos/<nsid>/<id>/`.
pub fn photo_url(nsid: &str, photo_id: &str) -> String {
    format!(
        "https://www.flickr.com/photos/{}/{}/",
        oauth::encode(nsid),
        oauth::encode(photo_id)
    )
}

/// What Flickr's error codes mean for the user.
pub fn explain(code: i64, msg: &str) -> String {
    let hint = match code {
        5 => " Flickr does not take this kind of file.",
        6 => " Your Flickr account's upload limit is reached (a free account keeps 1,000 photos and videos).",
        8 => " The file is larger than Flickr allows (200 MB for a photo).",
        96..=99 => " Sign in to Flickr again in Settings → Flickr.",
        100 => " Check the API key and secret in Settings → Flickr.",
        105 | 106 => " Flickr is not available right now; Keepsake tries again later.",
        _ => "",
    };
    format!("Flickr said: {msg} (error {code}).{hint}")
}

/// Whether Flickr's error code is worth trying again later.
fn retry_code(code: i64) -> bool {
    matches!(code, 3 | 105 | 106)
}

/// Whether the error code means the sign-in no longer works.
fn signed_out_code(code: i64) -> bool {
    matches!(code, 96..=100)
}

/// A file name safe in a form header: ASCII letters, digits, `.-_`.
fn safe_file_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.trim_matches(['_', '.']).is_empty() {
        "photo.jpg".into()
    } else {
        s
    }
}

/// The media type of an upload, by its file name.
fn content_type(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}

impl Flickr {
    pub fn new(http: Arc<dyn Http>) -> Self {
        Self {
            http,
            stamp: Arc::new(|| {
                (
                    chrono::Utc::now().timestamp(),
                    uuid::Uuid::new_v4().simple().to_string(),
                )
            }),
        }
    }

    /// Use this time and nonce (tests).
    pub fn with_stamp(mut self, stamp: Arc<Stamp>) -> Self {
        self.stamp = stamp;
        self
    }

    fn headers() -> Vec<(&'static str, String)> {
        vec![("User-Agent", user_agent())]
    }

    /// A signed GET: the parameters and signature in the query string.
    fn signed_get(
        &self,
        url: &str,
        app: &App,
        token: Option<(&str, &str)>,
        extra: &[(&str, &str)],
    ) -> Result<String, FlickrError> {
        let (ts, nonce) = (self.stamp)();
        let mut p = oauth::oauth_params(&app.key, token.map(|t| t.0), ts, &nonce);
        p.extend(
            extra
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        );
        let sig = oauth::signature("GET", url, &p, &app.secret, token.map_or("", |t| t.1));
        p.push(("oauth_signature".into(), sig));
        let full = format!("{url}?{}", oauth::query(&p));
        let h = Self::headers();
        let hs: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (status, body) = self
            .http
            .get(&full, &hs)
            .map_err(|e| FlickrError::Network(scrub(&e, app, token)))?;
        Self::check_status(status, &body)?;
        Ok(body)
    }

    fn check_status(status: u16, body: &str) -> Result<(), FlickrError> {
        if (200..300).contains(&status) {
            return Ok(());
        }
        let said = oauth::form_value(&oauth::parse_form(body), "oauth_problem")
            .map(str::to_owned)
            .unwrap_or_else(|| body.chars().take(200).collect::<String>().trim().to_owned());
        Err(match status {
            401 => FlickrError::SignedOut(format!(
                "Flickr did not accept the sign-in ({said}). Check the API key and secret, and sign in again."
            )),
            429 | 500..=599 => FlickrError::Network(format!("Flickr answered {status}: {said}")),
            _ => FlickrError::Refused(format!("Flickr answered {status}: {said}")),
        })
    }

    /// Step 1 of signing in: a request token, and the page where the user
    /// allows Keepsake to upload (`perms=write`). Flickr then shows a code
    /// to type into Keepsake (`oob`: no local web server).
    pub fn request_token(&self, app: &App) -> Result<Pending, FlickrError> {
        let body = self.signed_get(REQUEST_TOKEN_URL, app, None, &[("oauth_callback", "oob")])?;
        let f = oauth::parse_form(&body);
        let (Some(token), Some(secret)) = (
            oauth::form_value(&f, "oauth_token"),
            oauth::form_value(&f, "oauth_token_secret"),
        ) else {
            return Err(FlickrError::Unexpected(
                "Flickr's answer had no request token".into(),
            ));
        };
        Ok(Pending {
            token: token.to_owned(),
            secret: secret.to_owned(),
            authorize_url: format!(
                "{AUTHORIZE_URL}?oauth_token={}&perms=write",
                oauth::encode(token)
            ),
        })
    }

    /// Step 2: the code Flickr showed gives the access token.
    pub fn access_token(
        &self,
        app: &App,
        pending: &Pending,
        verifier: &str,
    ) -> Result<Account, FlickrError> {
        let v = verifier.trim();
        if v.is_empty() {
            return Err(FlickrError::Refused(
                "Type the code Flickr showed after you allowed Keepsake.".into(),
            ));
        }
        let body = self.signed_get(
            ACCESS_TOKEN_URL,
            app,
            Some((&pending.token, &pending.secret)),
            &[("oauth_verifier", v)],
        )?;
        let f = oauth::parse_form(&body);
        let get = |k: &str| oauth::form_value(&f, k).map(str::to_owned);
        match (
            get("oauth_token"),
            get("oauth_token_secret"),
            get("user_nsid"),
        ) {
            (Some(token), Some(secret), Some(nsid)) => Ok(Account {
                token,
                secret,
                username: get("username").unwrap_or_else(|| nsid.clone()),
                nsid,
            }),
            _ => Err(FlickrError::Unexpected(
                "Flickr's answer had no access token".into(),
            )),
        }
    }

    /// A signed REST call (JSON); Flickr's `stat: fail` becomes an error.
    pub(crate) fn rest(
        &self,
        app: &App,
        account: &Account,
        method: &str,
        args: &[(&str, &str)],
    ) -> Result<Value, FlickrError> {
        let mut extra = vec![
            ("method", method),
            ("format", "json"),
            ("nojsoncallback", "1"),
        ];
        extra.extend_from_slice(args);
        let body = self.signed_get(
            REST_URL,
            app,
            Some((&account.token, &account.secret)),
            &extra,
        )?;
        let v: Value = serde_json::from_str(&body)
            .map_err(|_| FlickrError::Unexpected("Flickr's answer was not JSON".into()))?;
        if v["stat"] == "ok" {
            return Ok(v);
        }
        let code = v["code"].as_i64().unwrap_or(0);
        let msg = v["message"].as_str().unwrap_or("unknown error");
        Err(Self::coded(code, msg))
    }

    fn coded(code: i64, msg: &str) -> FlickrError {
        let text = explain(code, msg);
        if signed_out_code(code) {
            FlickrError::SignedOut(text)
        } else if retry_code(code) {
            FlickrError::Network(text)
        } else {
            FlickrError::Refused(text)
        }
    }

    /// Whether the sign-in works: the account's user name.
    pub fn test_login(&self, app: &App, account: &Account) -> Result<String, FlickrError> {
        let v = self.rest(app, account, "flickr.test.login", &[])?;
        Ok(v["user"]["username"]["_content"]
            .as_str()
            .unwrap_or(&account.username)
            .to_owned())
    }

    /// Upload one photo; returns its Flickr id.
    pub fn upload(
        &self,
        app: &App,
        account: &Account,
        photo: &[u8],
        file_name: &str,
        params: &UploadParams,
    ) -> Result<String, FlickrError> {
        let (is_public, is_friend, is_family) = params.privacy.flags();
        let flag = |b: bool| if b { "1" } else { "0" }.to_owned();
        let (ts, nonce) = (self.stamp)();
        let mut p = oauth::oauth_params(&app.key, Some(&account.token), ts, &nonce);
        p.push(("title".into(), params.title.clone()));
        if !params.description.is_empty() {
            p.push(("description".into(), params.description.clone()));
        }
        let tags = super::tags::to_argument(&params.tags);
        if !tags.is_empty() {
            p.push(("tags".into(), tags));
        }
        p.push(("is_public".into(), flag(is_public)));
        p.push(("is_friend".into(), flag(is_friend)));
        p.push(("is_family".into(), flag(is_family)));
        // A photo, not a screenshot or art (Flickr's `content_type` 1).
        p.push(("content_type".into(), "1".into()));
        // The photo itself is not signed (Flickr's upload API).
        let sig = oauth::signature("POST", UPLOAD_URL, &p, &app.secret, &account.secret);
        p.push(("oauth_signature".into(), sig));
        let boundary = format!("keepsake-{}", uuid::Uuid::new_v4().simple());
        let body = multipart(&boundary, &p, &safe_file_name(file_name), photo);
        let ctype = format!("multipart/form-data; boundary={boundary}");
        let mut h = Self::headers();
        h.push(("Content-Type", ctype));
        let hs: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (status, text) = self
            .http
            .post(UPLOAD_URL, &hs, &body)
            .map_err(|e| FlickrError::Network(scrub(&e, app, Some((&account.token, "")))))?;
        Self::check_status(status, &text)?;
        parse_upload(&text)
    }
}

/// Remove the API key and token from a message (a transport error may
/// quote the URL).
fn scrub(msg: &str, app: &App, token: Option<(&str, &str)>) -> String {
    let mut m = msg.to_owned();
    for s in [Some(app.key.as_str()), token.map(|t| t.0)]
        .into_iter()
        .flatten()
        .filter(|s| s.len() >= 4)
    {
        m = m.replace(s, "…");
        m = m.replace(&oauth::encode(s), "…");
    }
    // Drop the query: it holds the signature.
    if let Some(i) = m.find('?') {
        let end = m[i..].find(char::is_whitespace).map_or(m.len(), |j| i + j);
        m.replace_range(i..end, "");
    }
    m
}

/// A `multipart/form-data` body: the fields, then the photo.
pub fn multipart(
    boundary: &str,
    fields: &[(String, String)],
    file_name: &str,
    photo: &[u8],
) -> Vec<u8> {
    let mut b = Vec::with_capacity(photo.len() + 2048);
    for (k, v) in fields {
        b.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    b.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"{file_name}\"\r\nContent-Type: {}\r\n\r\n",
            content_type(file_name)
        )
        .as_bytes(),
    );
    b.extend_from_slice(photo);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    b
}

/// The upload API's XML answer: the photo id, or Flickr's error.
pub fn parse_upload(xml: &str) -> Result<String, FlickrError> {
    let between = |open: &str, close: &str| {
        let s = xml.find(open)? + open.len();
        let e = xml[s..].find(close)? + s;
        Some(xml[s..e].trim().to_owned())
    };
    if xml.contains("stat=\"ok\"") {
        return between("<photoid>", "</photoid>")
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| FlickrError::Unexpected("Flickr's answer had no photo id".into()));
    }
    let attr = |name: &str| {
        let key = format!("{name}=\"");
        let s = xml.find(&key)? + key.len();
        let e = xml[s..].find('"')? + s;
        Some(xml[s..e].to_owned())
    };
    match (attr("code"), attr("msg")) {
        (Some(code), Some(msg)) => Err(Flickr::coded(code.parse().unwrap_or(0), &unescape(&msg))),
        _ => Err(FlickrError::Unexpected(format!(
            "Flickr's answer was not understood: {}",
            xml.chars().take(200).collect::<String>()
        ))),
    }
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_user_agent_has_no_address() {
        let ua = user_agent();
        assert!(ua.starts_with("Keepsake/"));
        assert_eq!(ua.matches('.').count(), 1, "major.minor only: {ua}");
        assert!(!ua.contains("http") && !ua.contains('@'));
    }

    #[test]
    fn privacy_gives_flickrs_flags() {
        assert_eq!(Privacy::Public.flags(), (true, false, false));
        assert_eq!(Privacy::Private.flags(), (false, false, false));
        assert_eq!(Privacy::FriendsFamily.flags(), (false, true, true));
        for p in Privacy::ALL {
            assert_eq!(Privacy::parse(p.id()), Some(p));
        }
        assert_eq!(Privacy::default(), Privacy::Private);
    }

    #[test]
    fn upload_answers_are_read() {
        assert_eq!(
            parse_upload("<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n<rsp stat=\"ok\">\n<photoid>53312345678</photoid>\n</rsp>").unwrap(),
            "53312345678"
        );
        let e = parse_upload(
            "<rsp stat=\"fail\">\n\t<err code=\"5\" msg=\"Filetype was not recognised\" />\n</rsp>",
        )
        .unwrap_err();
        assert!(matches!(e, FlickrError::Refused(_)));
        assert!(e
            .to_string()
            .contains("Filetype was not recognised (error 5)"));
        let e =
            parse_upload("<rsp stat=\"fail\"><err code=\"98\" msg=\"Invalid auth token\" /></rsp>")
                .unwrap_err();
        assert!(matches!(e, FlickrError::SignedOut(_)));
        let e = parse_upload(
            "<rsp stat=\"fail\"><err code=\"105\" msg=\"Service currently unavailable\" /></rsp>",
        )
        .unwrap_err();
        assert!(e.is_retryable());
        assert!(parse_upload("<html>").is_err());
    }

    #[test]
    fn the_multipart_body_has_the_fields_then_the_photo() {
        let b = multipart(
            "B",
            &[("title".into(), "Fly".into())],
            "a.jpg",
            b"\xff\xd8JPEG",
        );
        let s = String::from_utf8_lossy(&b);
        assert!(
            s.starts_with("--B\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nFly\r\n")
        );
        assert!(
            s.contains("name=\"photo\"; filename=\"a.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n")
        );
        assert!(s.ends_with("JPEG\r\n--B--\r\n"));
        assert_eq!(safe_file_name("Été \"1\".jpg"), "_t___1_.jpg");
        assert_eq!(safe_file_name("..."), "photo.jpg");
    }

    #[test]
    fn errors_do_not_carry_the_key_or_signature() {
        let app = App {
            key: "abcdef0123456789".into(),
            secret: "s3cr3t".into(),
        };
        let m = scrub(
            "https://www.flickr.com/services/rest/?oauth_consumer_key=abcdef0123456789&oauth_signature=xyz: timed out",
            &app,
            None,
        );
        assert!(!m.contains("abcdef0123456789") && !m.contains("xyz"), "{m}");
        assert!(m.contains("timed out"));
    }
}
