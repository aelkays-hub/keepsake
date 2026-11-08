//! Gemini (Google), through `generateContent` (ADR-038).
//!
//! Google recommends its newer Interactions API for new work but says
//! `generateContent` "remains fully supported"; it is the documented, stable
//! one. The photo goes inline; the answer is held to the tag schema with
//! `responseJsonSchema`. On the free tier Google may use what is sent to
//! improve its products, so Settings asks for a key from a project with
//! billing.

use serde_json::{json, Value};

use super::{base64, fail, http_error, schema, tidy, CloudModel, CloudTags, Provider, ASK, SYSTEM};
use crate::ai::AiError;

/// The API, up to the model id.
pub const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models/";

/// Why an answer stopped that means Gemini will not tag this photo.
const BLOCKED: [&str; 9] = [
    "SAFETY",
    "RECITATION",
    "BLOCKLIST",
    "PROHIBITED_CONTENT",
    "SPII",
    "IMAGE_SAFETY",
    "IMAGE_PROHIBITED_CONTENT",
    "IMAGE_RECITATION",
    "LANGUAGE",
];

/// The URL for a model.
pub fn url(model: CloudModel) -> String {
    format!("{API_BASE}{}:generateContent", model.id())
}

/// The URL, headers and body for one photo.
pub fn call(
    model: CloudModel,
    key: &str,
    jpeg: &[u8],
    names: &[String],
) -> (String, Vec<(&'static str, String)>, Value) {
    let headers = vec![
        ("content-type", "application/json".to_owned()),
        ("x-goog-api-key", key.to_owned()),
    ];
    (url(model), headers, request(jpeg, names))
}

/// The request body for one photo (the model is in the URL).
pub fn request(jpeg: &[u8], names: &[String]) -> Value {
    // `responseJsonSchema` takes JSON Schema; `additionalProperties` is
    // left out, as Google's docs do not list it for answers. Gemini refuses
    // the tag list as an `enum` ("Request contains an invalid argument"), so
    // the list goes with the question instead; `tidy` keeps only tags on it.
    let mut s = schema(names);
    if let Some(o) = s.as_object_mut() {
        o.remove("additionalProperties");
    }
    if let Some(items) = s["properties"]["tags"]["items"].as_object_mut() {
        items.remove("enum");
    }
    json!({
        "systemInstruction": {"parts": [{"text": SYSTEM}]},
        "contents": [{
            "role": "user",
            "parts": [
                {"inlineData": {"mimeType": "image/jpeg", "data": base64(jpeg)}},
                {"text": format!("{ASK}\n\nThe list (use these names exactly):\n{}", names.join("\n"))},
            ],
        }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseJsonSchema": s,
            "thinkingConfig": {"thinkingLevel": "LOW"},
            "maxOutputTokens": 16000,
        },
    })
}

/// Read the API's answer.
pub fn parse_response(status: u16, body: &str, names: &[String]) -> Result<CloudTags, AiError> {
    const P: Provider = Provider::Gemini;
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if status != 200 {
        let message = v["error"]["message"].as_str().unwrap_or("");
        // A bad key is a 400 with its own reason, not a 401.
        let bad_key = v["error"]["details"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["reason"] == "API_KEY_INVALID")
            || message.contains("API key not valid");
        return Err(if bad_key {
            http_error(P, 401, message)
        } else {
            http_error(P, status, message)
        });
    }
    if v["promptFeedback"]["blockReason"].is_string() {
        return Err(fail("Gemini declined to tag this photo".into(), false));
    }
    let candidate = &v["candidates"][0];
    let reason = candidate["finishReason"].as_str().unwrap_or("");
    if BLOCKED.contains(&reason) {
        return Err(fail("Gemini declined to tag this photo".into(), false));
    }
    if reason == "MAX_TOKENS" {
        return Err(fail("Gemini's answer was cut short".into(), true));
    }
    // The answer's text, without any thoughts.
    let text: String = candidate["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["thought"] != true)
        .filter_map(|p| p["text"].as_str())
        .collect();
    if text.trim().is_empty() {
        return Err(fail("Gemini's answer had no tags".into(), true));
    }
    tidy(P, &text, names, v["modelVersion"].as_str().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{names, tagger, Fake};
    use super::super::Tagger;
    use super::*;

    fn ok(text: &str) -> String {
        json!({
            "modelVersion": "gemini-3.6-flash",
            "candidates": [{
                "finishReason": "STOP",
                "content": {"role": "model", "parts": [
                    {"text": "Looking at the photo…", "thought": true},
                    {"text": text},
                ]},
            }],
        })
        .to_string()
    }

    #[test]
    fn the_request_sends_the_photo_the_list_and_a_schema() {
        let r = request(b"foo", &names());
        let parts = &r["contents"][0]["parts"];
        assert_eq!(parts[0]["inlineData"]["mimeType"], "image/jpeg");
        assert_eq!(parts[0]["inlineData"]["data"], "Zm9v");
        let ask = parts[1]["text"].as_str().unwrap();
        assert!(ask.starts_with(ASK));
        for n in names() {
            assert!(ask.lines().any(|l| l == n), "{n} is in the question");
        }
        assert_eq!(r["systemInstruction"]["parts"][0]["text"], SYSTEM);
        let g = &r["generationConfig"];
        assert_eq!(g["responseMimeType"], "application/json");
        assert_eq!(g["thinkingConfig"]["thinkingLevel"], "LOW");
        let s = &g["responseJsonSchema"];
        // Gemini refuses the list as an enum.
        assert_eq!(s["properties"]["tags"]["items"], json!({"type": "string"}));
        assert_eq!(s["required"], json!(["tags", "new_tags"]));
        assert!(s.get("additionalProperties").is_none());
        assert_eq!(
            url(CloudModel::Gemini35FlashLite),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.5-flash-lite:generateContent"
        );
    }

    #[test]
    fn the_answer_leaves_out_thoughts() {
        let t = parse_response(
            200,
            &ok(r#"{"tags": ["People"], "new_tags": ["Picnic"]}"#),
            &names(),
        )
        .unwrap();
        assert_eq!(t.tags, ["People"]);
        assert_eq!(t.new_tags, ["Picnic"]);
        assert_eq!(t.model, "gemini-3.6-flash");
    }

    #[test]
    fn only_tags_on_the_list_are_kept_without_an_enum() {
        let t = parse_response(
            200,
            &ok(r#"{"tags": ["dog", "Dragon", "People"], "new_tags": []}"#),
            &names(),
        )
        .unwrap();
        assert_eq!(t.tags, ["Dog", "People"]);
    }

    #[test]
    fn problems_say_whether_trying_again_helps() {
        let retry = |s: u16, b: &str| match parse_response(s, b, &names()) {
            Err(AiError::Cloud { retry, message }) => (retry, message),
            other => panic!("{other:?}"),
        };
        let bad_key = json!({"error": {
            "code": 400, "status": "INVALID_ARGUMENT",
            "message": "API key not valid. Please pass a valid API key.",
            "details": [{"reason": "API_KEY_INVALID"}],
        }})
        .to_string();
        assert_eq!(
            retry(400, &bad_key),
            (
                false,
                "Gemini did not accept the API key: check it in Settings".into()
            )
        );
        assert!(retry(429, r#"{"error": {"status": "RESOURCE_EXHAUSTED"}}"#).0);
        assert!(retry(503, r#"{"error": {"status": "UNAVAILABLE"}}"#).0);
        let (r, m) = retry(
            400,
            r#"{"error": {"message": "Unable to process input image"}}"#,
        );
        assert!(!r && m.ends_with("Unable to process input image"), "{m}");
        let blocked = json!({"promptFeedback": {"blockReason": "SAFETY"}}).to_string();
        assert_eq!(
            retry(200, &blocked),
            (false, "Gemini declined to tag this photo".into())
        );
        let unsafe_image = json!({"candidates": [{"finishReason": "IMAGE_SAFETY"}]}).to_string();
        assert!(!retry(200, &unsafe_image).0);
        let short = json!({"candidates": [{"finishReason": "MAX_TOKENS",
                                           "content": {"parts": [{"text": "{\"tags"}]}}]})
        .to_string();
        assert!(retry(200, &short).0);
        assert!(retry(200, &ok("not json")).0);
        assert!(retry(200, "{}").0, "no candidate");
    }

    #[test]
    fn gemini_sends_the_key_in_its_header_and_the_model_in_the_url() {
        let http = Fake::new(200, ok(r#"{"tags": ["Beach"], "new_tags": []}"#));
        let t = tagger(http.clone(), CloudModel::Gemini4Argon);
        assert_eq!(
            t.tag(Provider::Gemini, b"x", &names()).unwrap().tags,
            ["Beach"]
        );
        let (url, _, body) = http.last();
        assert!(url.ends_with("/gemini-4-argon:generateContent"), "{url}");
        assert_eq!(
            http.header("x-goog-api-key").as_deref(),
            Some("gemini-test-key-000000")
        );
        assert!(
            !body.contains("gemini-test-key"),
            "the key is not in the body"
        );
        assert!(!url.contains("key="), "nor in the URL");
    }
}
