//! ChatGPT (OpenAI), through the Responses API (ADR-038).
//!
//! The photo goes as a base64 data URL; the answer is held to the tag
//! schema with Structured Outputs (`text.format`, strict). `store: false`
//! asks OpenAI not to keep the response (it still keeps API data up to 30
//! days to check for abuse, and does not train on it).

use serde_json::{json, Value};

use super::{base64, fail, http_error, schema, tidy, CloudModel, CloudTags, Provider, ASK, SYSTEM};
use crate::ai::AiError;

/// The Responses API.
pub const API_URL: &str = "https://api.openai.com/v1/responses";

/// Error codes of a 429 that mean the account is out of credit or over a
/// limit: trying again does not help.
const OVER_LIMIT: [&str; 6] = [
    "insufficient_quota",
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
    "organization_usage_limit_exceeded",
    "project_usage_limit_exceeded",
];

/// The URL, headers and body for one photo.
pub fn call(
    model: CloudModel,
    key: &str,
    jpeg: &[u8],
    names: &[String],
) -> (String, Vec<(&'static str, String)>, Value) {
    let headers = vec![
        ("content-type", "application/json".to_owned()),
        ("authorization", format!("Bearer {key}")),
    ];
    (API_URL.to_owned(), headers, request(model, jpeg, names))
}

/// The request body for one photo.
pub fn request(model: CloudModel, jpeg: &[u8], names: &[String]) -> Value {
    json!({
        "model": model.id(),
        "instructions": SYSTEM,
        "input": [{
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": format!("data:image/jpeg;base64,{}", base64(jpeg)),
                    "detail": "auto",
                },
                {"type": "input_text", "text": ASK},
            ],
        }],
        "text": {"format": {
            "type": "json_schema",
            "name": "photo_tags",
            "strict": true,
            "schema": schema(names),
        }},
        "reasoning": {"effort": "low"},
        "max_output_tokens": 16000,
        "store": false,
    })
}

/// Read the API's answer.
pub fn parse_response(status: u16, body: &str, names: &[String]) -> Result<CloudTags, AiError> {
    const P: Provider = Provider::OpenAi;
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if status != 200 {
        let code = v["error"]["code"].as_str().unwrap_or("");
        if status == 429 && OVER_LIMIT.contains(&code) {
            return Err(fail(
                "OpenAI says the account is out of credit or over its limit: check it at platform.openai.com".into(),
                false,
            ));
        }
        return Err(http_error(
            P,
            status,
            v["error"]["message"].as_str().unwrap_or(""),
        ));
    }
    if v["status"] == "incomplete" {
        return Err(match v["incomplete_details"]["reason"].as_str() {
            Some("content_filter") => fail("ChatGPT declined to tag this photo".into(), false),
            _ => fail("ChatGPT's answer was cut short".into(), true),
        });
    }
    let parts: Vec<&Value> = v["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|o| o["type"] == "message")
        .flat_map(|o| o["content"].as_array().into_iter().flatten())
        .collect();
    if parts.iter().any(|c| c["type"] == "refusal") {
        return Err(fail("ChatGPT declined to tag this photo".into(), false));
    }
    let text = parts
        .iter()
        .rev()
        .filter(|c| c["type"] == "output_text")
        .find_map(|c| c["text"].as_str())
        .ok_or_else(|| fail("ChatGPT's answer had no tags".into(), true))?;
    tidy(P, text, names, v["model"].as_str().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{names, tagger, Fake};
    use super::super::Tagger;
    use super::*;

    fn ok(text: &str) -> String {
        json!({
            "model": "gpt-6.1-sol",
            "status": "completed",
            "output": [
                {"type": "reasoning", "summary": []},
                {"type": "message", "role": "assistant", "content": [
                    {"type": "output_text", "text": text},
                ]},
            ],
        })
        .to_string()
    }

    #[test]
    fn the_request_sends_the_photo_the_list_and_a_strict_schema() {
        let r = request(CloudModel::Gpt61Sol, b"foo", &names());
        assert_eq!(r["model"], "gpt-6.1-sol");
        assert_eq!(r["store"], false);
        assert_eq!(r["reasoning"]["effort"], "low");
        let content = &r["input"][0]["content"];
        assert_eq!(content[0]["type"], "input_image");
        assert_eq!(content[0]["image_url"], "data:image/jpeg;base64,Zm9v");
        assert_eq!(content[1]["text"], ASK);
        let f = &r["text"]["format"];
        assert_eq!(f["type"], "json_schema");
        assert_eq!(f["strict"], true);
        // Strict mode wants every property required and no others.
        assert_eq!(f["schema"]["additionalProperties"], false);
        assert_eq!(f["schema"]["required"], json!(["tags", "new_tags"]));
        assert_eq!(
            f["schema"]["properties"]["tags"]["items"]["enum"][1],
            "Beach"
        );
        assert_eq!(r["instructions"], SYSTEM);
    }

    #[test]
    fn the_answer_is_read_from_the_message() {
        let t = parse_response(
            200,
            &ok(r#"{"tags": ["dog", "Lion"], "new_tags": ["Frisbee"]}"#),
            &names(),
        )
        .unwrap();
        assert_eq!(t.tags, ["Dog"]);
        assert_eq!(t.new_tags, ["Frisbee"]);
        assert_eq!(t.model, "gpt-6.1-sol");
    }

    #[test]
    fn problems_say_whether_trying_again_helps() {
        let retry = |s: u16, b: &str| match parse_response(s, b, &names()) {
            Err(AiError::Cloud { retry, message }) => (retry, message),
            other => panic!("{other:?}"),
        };
        let bad_key = r#"{"error": {"message": "Incorrect API key provided", "type": "invalid_request_error"}}"#;
        assert_eq!(
            retry(401, bad_key),
            (
                false,
                "ChatGPT did not accept the API key: check it in Settings".into()
            )
        );
        assert!(retry(429, r#"{"error": {"code": "slow_down"}}"#).0);
        let (r, m) = retry(429, r#"{"error": {"code": "insufficient_quota"}}"#);
        assert!(!r && m.contains("out of credit"), "{m}");
        assert!(retry(503, r#"{"error": {"code": "server_is_overloaded"}}"#).0);
        let (r, m) = retry(400, r#"{"error": {"message": "Invalid image"}}"#);
        assert!(!r && m.ends_with("Invalid image"), "{m}");
        let refusal = json!({"status": "completed", "output": [
            {"type": "message", "content": [{"type": "refusal", "refusal": "I can't help"}]},
        ]})
        .to_string();
        assert_eq!(
            retry(200, &refusal),
            (false, "ChatGPT declined to tag this photo".into())
        );
        let short = json!({"status": "incomplete",
                           "incomplete_details": {"reason": "max_output_tokens"}, "output": []})
        .to_string();
        assert!(retry(200, &short).0);
        let filtered = json!({"status": "incomplete",
                              "incomplete_details": {"reason": "content_filter"}, "output": []})
        .to_string();
        assert!(!retry(200, &filtered).0);
        assert!(retry(200, &ok("not json")).0);
        assert!(retry(200, "{}").0, "no message");
    }

    #[test]
    fn chatgpt_sends_the_key_as_a_bearer_token() {
        let http = Fake::new(200, ok(r#"{"tags": ["Beach"], "new_tags": []}"#));
        let t = tagger(http.clone(), CloudModel::Gpt6Luna);
        assert_eq!(
            t.tag(Provider::OpenAi, b"x", &names()).unwrap().tags,
            ["Beach"]
        );
        let (url, _, body) = http.last();
        assert_eq!(url, API_URL);
        assert_eq!(
            http.header("authorization").as_deref(),
            Some("Bearer openai-test-key-000000")
        );
        assert!(body.contains("gpt-6-luna"));
    }
}
