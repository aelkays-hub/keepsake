//! Upload to Flickr (ADR-046) through the catalog and the render service's
//! job queue, with a stand-in for Flickr's servers (the `Http` trait):
//! always run, also in CI. Nothing is sent anywhere, and the originals
//! stay as they were.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use image::{DynamicImage, Rgb, RgbImage};
use keepsake_core::ai::cloud::Http;
use keepsake_core::catalog::{
    AssetId, AssetTag, Catalog, JobState, TagPath, TagSource, VolumeKind,
};
use keepsake_core::flickr::{
    self, client, oauth, Account, App, Flickr, FlickrError, Privacy, Request, State, TagOptions,
    Uploader, JOB_FLICKR_UPLOAD,
};
use keepsake_core::library::Library;
use keepsake_core::rebuild::add_volume;
use keepsake_core::render::{Cache, RenderOptions, RenderService, RenderTools};

/// What the stand-in was sent.
#[derive(Debug, Clone)]
struct Sent {
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// Stands in for Flickr: records requests, answers each from a script.
struct FakeFlickr {
    sent: Mutex<Vec<Sent>>,
    answers: Mutex<Vec<Result<(u16, String), String>>>,
}

impl FakeFlickr {
    fn new(answers: Vec<Result<(u16, String), String>>) -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            answers: Mutex::new(answers),
        })
    }

    fn answer(&self) -> Result<(u16, String), String> {
        let mut a = self.answers.lock().unwrap();
        if a.is_empty() {
            Err("no answer scripted".into())
        } else {
            a.remove(0)
        }
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }
}

impl Http for FakeFlickr {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<(u16, String), String> {
        self.sent.lock().unwrap().push(Sent {
            url: url.into(),
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            body: body.to_vec(),
        });
        self.answer()
    }

    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<(u16, String), String> {
        self.post(url, headers, &[])
    }
}

const OK_UPLOAD: &str =
    "<?xml version=\"1.0\" encoding=\"utf-8\" ?>\n<rsp stat=\"ok\">\n<photoid>53300000001</photoid>\n</rsp>\n";

fn app() -> App {
    App {
        key: "0123456789abcdef0123456789abcdef".into(),
        secret: "app-secret-fedcba98".into(),
    }
}

fn account() -> Account {
    Account {
        token: "72157700000000000-0123456789abcdef".into(),
        secret: "token-secret-7654".into(),
        nsid: "12345678@N00".into(),
        username: "keepsake-test".into(),
    }
}

fn fixed_stamp() -> Arc<client::Stamp> {
    Arc::new(|| (1_700_000_000, "nonce123".to_owned()))
}

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    cat: Catalog,
    media: PathBuf,
    cache: Cache,
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        Rgb([(x % 255) as u8, (y % 255) as u8, 90])
    }));
    keepsake_core::render::still::encode_jpeg(&img).unwrap()
}

impl World {
    fn new(files: &[&str]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let media = tmp.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        for f in files {
            std::fs::write(media.join(f), jpeg(64, 48)).unwrap();
        }
        let lib = Library::create(&tmp.path().join("lib")).unwrap();
        let mut cat = lib.open_catalog().unwrap();
        add_volume(
            &mut cat,
            &lib,
            "Media",
            VolumeKind::External,
            &media,
            true,
            None,
        )
        .unwrap();
        let cache = Cache::new(tmp.path().join("cache"));
        Self {
            _tmp: tmp,
            lib,
            cat,
            media,
            cache,
        }
    }

    fn asset(&self, name: &str) -> AssetId {
        let path = std::fs::canonicalize(self.media.join(name)).unwrap();
        self.lib.locate(&self.cat, &path).unwrap().unwrap().asset_id
    }

    fn tag(&mut self, id: AssetId, keyword: &str) {
        let path = TagPath::parse_lightroom(keyword).unwrap();
        self.cat
            .write(|tx| {
                tx.tags()
                    .set_for_asset(id, TagSource::User, &[AssetTag::user(path)])
            })
            .unwrap();
    }

    fn run(&self, up: Uploader) {
        let tools = RenderTools {
            exiftool: PathBuf::from("none"),
            ffprobe: PathBuf::from("none"),
            ffmpeg: PathBuf::from("none"),
            dcraw: PathBuf::from("none"),
        };
        let opts = RenderOptions {
            workers: 1,
            proxy_workers: 0,
            max_bytes: u64::MAX,
            pause_on_battery: false,
            until_empty: true,
            forced_encoder: None,
            evict_every: 25,
            ai: false,
            ai_gpu: false,
            faces: false,
            aircraft: false,
        };
        RenderService::new(&self.lib, tools, self.cache.clone(), opts)
            .with_flickr(Arc::new(up))
            .run(&AtomicBool::new(false), &mut |_| {})
            .unwrap();
    }

    fn job(&self, id: AssetId) -> Option<keepsake_core::catalog::Job> {
        self.cat
            .read(|tx| tx.jobs().find(JOB_FLICKR_UPLOAD, id))
            .unwrap()
    }
}

fn uploader(http: Arc<FakeFlickr>, signed_in: bool) -> Uploader {
    Uploader::new(
        http.clone(),
        Arc::new(|| Some(app())),
        Arc::new(move || signed_in.then(account)),
    )
    .with_client(Flickr::new(http).with_stamp(fixed_stamp()))
}

/// The form fields of a multipart body (not the photo).
fn fields(body: &[u8]) -> Vec<(String, String)> {
    let s = String::from_utf8_lossy(body);
    let mut out = Vec::new();
    for part in s.split("Content-Disposition: form-data; name=\"").skip(1) {
        let (name, rest) = part.split_once('"').unwrap();
        if name == "photo" {
            continue;
        }
        let value = rest
            .split_once("\r\n\r\n")
            .unwrap()
            .1
            .split_once("\r\n--")
            .unwrap()
            .0;
        out.push((name.to_owned(), value.to_owned()));
    }
    out
}

fn value<'a>(f: &'a [(String, String)], k: &str) -> Option<&'a str> {
    f.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

#[test]
fn a_photo_is_uploaded_with_its_tags_signed_and_recorded() {
    let mut w = World::new(&["DSC01234.JPG"]);
    let id = w.asset("DSC01234.JPG");
    w.tag(id, "Aircraft|Registration|C-FEJC");
    let before = std::fs::read(w.media.join("DSC01234.JPG")).unwrap();

    let p = flickr::prepare(&w.cat, id, TagOptions::default()).unwrap();
    assert_eq!(p.title, "DSC01234");
    assert_eq!(p.tags, ["C-FEJC"]);
    assert!(p.uploads.is_empty());

    let mut tags = p.tags.clone();
    tags.push("Air Canada".into());
    let n = flickr::enqueue(
        &mut w.cat,
        &[Request {
            asset: id,
            title: "Landing".into(),
            description: String::new(),
            tags,
        }],
        Privacy::Public,
    )
    .unwrap();
    assert_eq!(n, 1);
    assert!(matches!(
        flickr::state(&w.cat, id).unwrap(),
        State::Waiting { retry: None }
    ));

    let http = FakeFlickr::new(vec![Ok((200, OK_UPLOAD.into()))]);
    w.run(uploader(http.clone(), true));

    let sent = http.sent();
    assert_eq!(sent.len(), 1);
    let s = &sent[0];
    assert_eq!(s.url, client::UPLOAD_URL);
    let ua = s.headers.iter().find(|(k, _)| k == "User-Agent").unwrap();
    assert_eq!(ua.1, client::user_agent());
    let f = fields(&s.body);
    assert_eq!(value(&f, "title"), Some("Landing"));
    assert_eq!(value(&f, "tags"), Some("C-FEJC \"Air Canada\""));
    assert_eq!(value(&f, "is_public"), Some("1"));
    assert_eq!(value(&f, "is_friend"), Some("0"));
    assert_eq!(value(&f, "oauth_token"), Some(account().token.as_str()));
    // The photo is the original's bytes, as they are.
    let raw = &s.body;
    assert!(raw
        .windows(before.len())
        .any(|win| win == before.as_slice()));
    // The signature is that of every field but itself (and the photo).
    let sig = value(&f, "oauth_signature").unwrap().to_owned();
    let signed: Vec<(String, String)> = f
        .iter()
        .filter(|(k, _)| k != "oauth_signature")
        .cloned()
        .collect();
    assert_eq!(
        oauth::signature(
            "POST",
            client::UPLOAD_URL,
            &signed,
            &app().secret,
            &account().secret
        ),
        sig
    );
    // Nothing secret travels but the token (the key's secret and the
    // token's secret only sign).
    let body = String::from_utf8_lossy(raw);
    assert!(!body.contains(&app().secret) && !body.contains(&account().secret));

    assert_eq!(w.job(id).unwrap().state, JobState::Done);
    let ups = w.cat.read(|tx| tx.flickr().uploads(id)).unwrap();
    assert_eq!(ups.len(), 1);
    assert_eq!(ups[0].photo_id, "53300000001");
    assert_eq!(
        ups[0].url,
        "https://www.flickr.com/photos/12345678%40N00/53300000001/"
    );
    assert_eq!(ups[0].privacy, "public");
    assert!(w.cat.read(|tx| tx.flickr().queued(id)).unwrap().is_none());
    assert!(matches!(flickr::state(&w.cat, id).unwrap(), State::Done(_)));
    // The window now says it is on Flickr.
    let again = flickr::prepare(&w.cat, id, TagOptions::default()).unwrap();
    assert_eq!(again.uploads.len(), 1);
    // The original is unchanged.
    assert_eq!(std::fs::read(w.media.join("DSC01234.JPG")).unwrap(), before);
}

#[test]
fn flickrs_refusal_is_explained_and_not_retried() {
    let mut w = World::new(&["a.jpg"]);
    let id = w.asset("a.jpg");
    let req = Request {
        asset: id,
        title: "a".into(),
        description: String::new(),
        tags: vec![],
    };
    flickr::enqueue(&mut w.cat, std::slice::from_ref(&req), Privacy::Private).unwrap();
    let http = FakeFlickr::new(vec![Ok((
        200,
        "<rsp stat=\"fail\"><err code=\"6\" msg=\"User exceeded upload limit\" /></rsp>".into(),
    ))]);
    w.run(uploader(http.clone(), true));
    let f = fields(&http.sent()[0].body);
    assert_eq!(value(&f, "is_public"), Some("0"));
    assert_eq!(value(&f, "is_family"), Some("0"));
    let job = w.job(id).unwrap();
    assert_eq!(job.state, JobState::Failed);
    let e = job.error.unwrap();
    assert!(e.contains("upload limit is reached"), "{e}");
    assert!(matches!(
        flickr::state(&w.cat, id).unwrap(),
        State::Failed(_)
    ));
    assert!(w.cat.read(|tx| tx.flickr().uploads(id)).unwrap().is_empty());
}

#[test]
fn a_network_failure_is_tried_again_later() {
    let mut w = World::new(&["a.jpg"]);
    let id = w.asset("a.jpg");
    flickr::enqueue(
        &mut w.cat,
        &[Request {
            asset: id,
            title: "a".into(),
            description: String::new(),
            tags: vec![],
        }],
        Privacy::FriendsFamily,
    )
    .unwrap();
    let http = FakeFlickr::new(vec![Err("connection reset".into())]);
    w.run(uploader(http.clone(), true));
    let job = w.job(id).unwrap();
    assert_eq!(job.state, JobState::Queued, "retried later");
    assert!(job.not_before_ms > 0);
    assert!(matches!(
        flickr::state(&w.cat, id).unwrap(),
        State::Waiting { retry: Some(_) }
    ));
    // What the user chose is still there for the next try.
    let q = w.cat.read(|tx| tx.flickr().queued(id)).unwrap().unwrap();
    assert_eq!(q.privacy, "friends_family");
}

#[test]
fn not_signed_in_nothing_is_sent() {
    let mut w = World::new(&["a.jpg"]);
    let id = w.asset("a.jpg");
    flickr::enqueue(
        &mut w.cat,
        &[Request {
            asset: id,
            title: "a".into(),
            description: String::new(),
            tags: vec![],
        }],
        Privacy::Public,
    )
    .unwrap();
    let http = FakeFlickr::new(vec![]);
    w.run(uploader(http.clone(), false));
    assert!(http.sent().is_empty());
    let job = w.job(id).unwrap();
    assert_eq!(job.state, JobState::Failed);
    assert!(job.error.unwrap().contains("Sign in to Flickr"));
}

#[test]
fn stop_removes_uploads_not_started() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    let req = |id| Request {
        asset: id,
        title: "x".into(),
        description: String::new(),
        tags: vec![],
    };
    assert_eq!(
        flickr::enqueue(&mut w.cat, &[req(a), req(b)], Privacy::Public).unwrap(),
        2
    );
    assert_eq!(flickr::stop(&mut w.cat, &[a]).unwrap(), 1);
    assert!(w.job(a).is_none());
    assert!(w.cat.read(|tx| tx.flickr().queued(a)).unwrap().is_none());
    assert!(matches!(flickr::state(&w.cat, a).unwrap(), State::None));
    assert!(w.job(b).is_some());
}

#[test]
fn signing_in_takes_a_request_token_then_the_code() {
    let http = FakeFlickr::new(vec![
        Ok((
            200,
            "oauth_callback_confirmed=true&oauth_token=72157-req&oauth_token_secret=reqsecret"
                .into(),
        )),
        Ok((
            200,
            "fullname=Test%20User&oauth_token=72157-acc&oauth_token_secret=accsecret\
             &user_nsid=12345678%40N00&username=tester"
                .into(),
        )),
        Ok((
            200,
            r#"{"user":{"id":"12345678@N00","username":{"_content":"tester"}},"stat":"ok"}"#.into(),
        )),
        Ok((
            200,
            r#"{"stat":"fail","code":98,"message":"Invalid auth token"}"#.into(),
        )),
        Ok((401, "oauth_problem=signature_invalid".into())),
    ]);
    let f = Flickr::new(http.clone()).with_stamp(fixed_stamp());
    let pending = f.request_token(&app()).unwrap();
    assert_eq!(pending.token, "72157-req");
    assert_eq!(
        pending.authorize_url,
        "https://www.flickr.com/services/oauth/authorize?oauth_token=72157-req&perms=write"
    );
    let first = &http.sent()[0].url;
    assert!(first.starts_with(client::REQUEST_TOKEN_URL));
    assert!(first.contains("oauth_callback=oob"));
    assert!(first.contains("oauth_signature="));
    assert!(!first.contains(&app().secret), "the secret only signs");

    assert!(matches!(
        f.access_token(&app(), &pending, "  "),
        Err(FlickrError::Refused(_))
    ));
    let acc = f.access_token(&app(), &pending, "123-456-789").unwrap();
    assert_eq!(acc.nsid, "12345678@N00");
    assert_eq!(acc.username, "tester");
    assert_eq!(acc.token, "72157-acc");
    assert!(http.sent()[1].url.contains("oauth_verifier=123-456-789"));
    assert!(http.sent()[1].url.contains("oauth_token=72157-req"));

    assert_eq!(f.test_login(&app(), &acc).unwrap(), "tester");
    assert!(http.sent()[2].url.contains("method=flickr.test.login"));
    assert!(matches!(
        f.test_login(&app(), &acc),
        Err(FlickrError::SignedOut(_))
    ));
    let e = f.test_login(&app(), &acc).unwrap_err();
    assert!(matches!(e, FlickrError::SignedOut(_)));
    assert!(e.to_string().contains("signature_invalid"));
    // Debug output hides the secrets.
    assert!(!format!("{acc:?} {:?} {pending:?}", app()).contains("secret"));
}

#[test]
fn the_report_lists_what_is_on_flickr_and_what_is_under_way() {
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg"]);
    let (a, b, c) = (w.asset("a.jpg"), w.asset("b.jpg"), w.asset("c.jpg"));
    assert_eq!(flickr::report(&w.cat).unwrap(), flickr::Report::default());
    let req = |id| Request {
        asset: id,
        title: "x".into(),
        description: String::new(),
        tags: vec![],
    };
    flickr::enqueue(&mut w.cat, &[req(a), req(b)], Privacy::Public).unwrap();
    // a is uploaded, b refused.
    let http = FakeFlickr::new(vec![
        Ok((200, OK_UPLOAD.into())),
        Ok((
            200,
            "<rsp stat=\"fail\"><err code=\"5\" msg=\"Filetype was not recognised\" /></rsp>"
                .into(),
        )),
    ]);
    w.run(uploader(http, true));
    // c waits.
    flickr::enqueue(&mut w.cat, &[req(c)], Privacy::Private).unwrap();

    let r = flickr::report(&w.cat).unwrap();
    assert_eq!(r.uploaded.len(), 1);
    assert_eq!(r.uploaded[0].asset_id, a);
    assert_eq!(r.uploaded[0].photo_id, "53300000001");
    let current: Vec<AssetId> = r.current.iter().map(|(id, _)| *id).collect();
    assert_eq!(current, [b, c]);
    assert!(matches!(r.current[0].1, State::Failed(_)));
    assert!(matches!(r.current[1].1, State::Waiting { .. }));
    // Stopped: no longer under way.
    flickr::stop(&mut w.cat, &[c]).unwrap();
    assert_eq!(flickr::report(&w.cat).unwrap().current.len(), 1);
}
