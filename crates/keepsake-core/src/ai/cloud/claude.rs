//! Claude (Anthropic), through the Messages API (ADR-036).

use serde_json::{json, Value};

use super::{base64, fail, http_error, tag_task, tidy, CloudModel, CloudTags, Provider, Task};
use crate::ai::AiError;

/// The Messages API.
pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// The API version sent.
pub const API_VERSION: &str = "2026-10-21";
/// The beta that lets a declined request fall back to another model.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

/// Whether the request asks for the API's fallback when the model declines
/// (Haiku has none).
fn fallbacks(model: CloudModel) -> bool {
    !matches!(model, CloudModel::ClaudeHaiku55)
}

/// The URL, headers and body for one photo.
pub(crate) fn call(
    model: CloudModel,
    key: &str,
    jpeg: &[u8],
    task: &Task,
) -> (String, Vec<(&'static str, String)>, Value) {
    let mut headers = vec![
        ("content-type", "application/json".to_owned()),
        ("x-api-key", key.to_owned()),
        ("anthropic-version", API_VERSION.to_owned()),
    ];
    if fallbacks(model) {
        headers.push(("anthropic-beta", FALLBACK_BETA.to_owned()));
    }
    (API_URL.to_owned(), headers, body(model, jpeg, task))
}

/// The request body for tagging one photo.
pub fn request(model: CloudModel, jpeg: &[u8], names: &[String]) -> Value {
    body(model, jpeg, &tag_task(Provider::Claude, names))
}

/// The request body for one photo and `task`.
pub(crate) fn body(model: CloudModel, jpeg: &[u8], task: &Task) -> Value {
    let mut body = json!({
        "model": model.id(),
        "max_tokens": 16000,
        "system": task.system,
        "output_config": {
            "effort": "low",
            "format": {"type": "json_schema", "schema": task.schema},
        },
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image", "source": {
                    "type": "base64", "media_type": "image/jpeg", "data": base64(jpeg),
                }},
                {"type": "text", "text": task.ask},
            ],
        }],
    });
    if fallbacks(model) {
        body["fallbacks"] = json!("default");
    }
    body
}

/// Read the API's answer to the tagging question.
pub fn parse_response(status: u16, body: &str, names: &[String]) -> Result<CloudTags, AiError> {
    let (text, model) = answer(status, body, &tag_task(Provider::Claude, names))?;
    tidy(Provider::Claude, &text, names, &model)
}

/// The answer's JSON text and the model that gave it.
pub(crate) fn answer(status: u16, body: &str, task: &Task) -> Result<(String, String), AiError> {
    const P: Provider = Provider::Claude;
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if status != 200 {
        return Err(http_error(
            P,
            status,
            v["error"]["message"].as_str().unwrap_or(""),
        ));
    }
    match v["stop_reason"].as_str() {
        Some("refusal") => {
            return Err(fail(
                format!("Claude declined to {} this photo", task.verb),
                false,
            ))
        }
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
        .ok_or_else(|| fail(format!("Claude's answer had no {}", task.noun), true))?;
    Ok((
        text.to_owned(),
        v["model"].as_str().unwrap_or("").to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{names, tagger, Fake};
    use super::super::Tagger;
    use super::*;

    fn ok(text: &str) -> String {
        json!({
            "model": "claude-opus-5-5",
            "stop_reason": "end_turn",
            "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": text}],
        })
        .to_string()
    }

    #[test]
    fn the_request_sends_the_photo_the_list_and_a_schema() {
        let r = request(CloudModel::ClaudeOpus55, b"foo", &names());
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
        let h = request(CloudModel::ClaudeHaiku55, b"foo", &names());
        assert_eq!(h["model"], "claude-haiku-5-5");
        assert!(h.get("fallbacks").is_none());
    }

    #[test]
    fn the_answer_is_read_from_the_last_text_block() {
        let t = parse_response(
            200,
            &ok(r#"{"tags": ["Beach", "Unicorn"], "new_tags": ["Kite"]}"#),
            &names(),
        )
        .unwrap();
        assert_eq!(t.tags, ["Beach"]);
        assert_eq!(t.new_tags, ["Kite"]);
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
    fn claude_sends_the_key_and_the_headers() {
        let http = Fake::new(200, ok(r#"{"tags": ["Dog"], "new_tags": []}"#));
        let t = tagger(http.clone(), CloudModel::ClaudeFable51);
        assert_eq!(
            t.tag(Provider::Claude, b"x", &names()).unwrap().tags,
            ["Dog"]
        );
        let (url, _, body) = http.last();
        assert_eq!(url, API_URL);
        assert_eq!(
            http.header("x-api-key").as_deref(),
            Some("claude-test-key-000000")
        );
        assert_eq!(
            http.header("anthropic-version").as_deref(),
            Some(API_VERSION)
        );
        assert_eq!(
            http.header("anthropic-beta").as_deref(),
            Some(FALLBACK_BETA)
        );
        assert!(body.contains("claude-fable-5-1"));
    }
}
