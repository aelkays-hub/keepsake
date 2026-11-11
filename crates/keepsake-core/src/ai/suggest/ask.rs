//! Asking an AI for edit suggestions (ADR-042): the photo's 1024 px
//! thumbnail and what Keepsake measured go to the provider the user picked
//! (the keys and models of Tag with AI), which answers with suggestions for
//! Photoshop and Topaz Photo, held to a schema. The requests and the
//! reading of the answers are those of [`crate::ai::cloud`].

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use super::measure::Measures;
use super::{rules, AiAdvice, Suggestion};
use crate::ai::cloud::{self, Http, KeyFn, ModelFn, Provider, Task};
use crate::ai::AiError;

/// Most suggestions kept from one answer.
pub const MAX_SUGGESTIONS: usize = 8;
/// Longest title, and longest other text, kept (characters).
const MAX_TITLE: usize = 80;
const MAX_TEXT: usize = 700;

/// What every provider is told.
pub(crate) const SYSTEM: &str = "You help a keen amateur photographer decide how to improve one photo from their personal library. They edit in Adobe Photoshop (with Camera Raw) and in Topaz Photo.

You are given a small preview of the photo (1024 pixels on its long side) and measurements their photo library made on that preview. Look at what the photo is of and what would make it better: exposure and tone, colour, noise, sharpness, straightening and cropping, distractions to remove, and faces. Fine noise and exact focus cannot be judged from a small preview, so lean on the measurements for those and say so when you are unsure.

Put in `summary` one or two plain sentences on the photo's strengths and what would help it most. Put in `suggestions` up to 6 edits, the most useful first, and only edits this photo needs: a good photo may need none. For each:
- `title`: what to do, in at most six words (\"Recover the sky\", \"Remove the sign on the left\").
- `why`: one sentence on what you see in this photo that calls for it.
- `photoshop`: how to do it in Photoshop, naming the exact tool or menu path and a value to start from (\"Camera Raw > Light: Highlights -50\", \"Remove tool (J): paint over the sign\", \"Crop tool (C): 4:5, cut the empty left third\").
- `topaz`: how to do it in Topaz Photo, naming its tool (Denoise, Sharpen, Super Focus, Recover Faces, Adjust Lighting, Balance Color, Remove, Upscale) and a setting to start from. Leave it empty when Topaz Photo has no tool for the edit.

Write for someone who knows both programs' basics. Never name or guess who a person is, and never comment on a person's age, ethnicity, religion, health or other personal traits; \"the person on the left\" is enough.";

/// The answer's JSON schema.
pub(crate) fn schema() -> Value {
    let text = json!({"type": "string"});
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "suggestions"],
        "properties": {
            "summary": text,
            "suggestions": {"type": "array", "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["title", "why", "photoshop", "topaz"],
                "properties": {
                    "title": text, "why": text, "photoshop": text, "topaz": text,
                },
            }},
        },
    })
}

/// What the photo is sent with: the question and the measurements.
pub(crate) fn ask(m: &Measures) -> String {
    let mut out = String::from(
        "Suggest edits for this photo.\n\nMeasured on this preview by the photo library:\n",
    );
    for line in rules::describe(m) {
        out.push_str(&format!("- {}: {}\n", line.label, line.value));
    }
    out.push_str(&format!(
        "- Tones: darkest 5% below {:.0}% grey, brightest 5% above {:.0}% grey\n",
        m.p5 * 100.0,
        m.p95 * 100.0
    ));
    out
}

/// The question for the providers.
pub(crate) fn task(m: &Measures) -> Task {
    Task {
        system: SYSTEM,
        ask: ask(m),
        schema: schema(),
        name: "edit_suggestions",
        verb: "look at",
        noun: "suggestions",
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    suggestions: Vec<AnswerItem>,
}

#[derive(Deserialize)]
struct AnswerItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    why: String,
    #[serde(default)]
    photoshop: String,
    #[serde(default)]
    topaz: String,
}

/// One line of plain text, at most `max` characters.
fn clean(s: &str, max: usize) -> String {
    let one: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if one.chars().count() <= max {
        return one;
    }
    let mut cut: String = one.chars().take(max - 1).collect();
    cut.push('…');
    cut
}

/// Read the answer's JSON text: text tidied, suggestions without a title or
/// without steps for either program dropped, at most [`MAX_SUGGESTIONS`].
pub(crate) fn tidy(
    provider: Provider,
    text: &str,
    model: &str,
    now_ms: i64,
) -> Result<AiAdvice, AiError> {
    let answer: Answer = serde_json::from_str(text.trim())
        .map_err(|e| cloud::fail(format!("{provider}'s answer could not be read: {e}"), true))?;
    let suggestions: Vec<Suggestion> = answer
        .suggestions
        .iter()
        .map(|s| Suggestion {
            title: clean(&s.title, MAX_TITLE),
            why: clean(&s.why, MAX_TEXT),
            photoshop: clean(&s.photoshop, MAX_TEXT),
            topaz: clean(&s.topaz, MAX_TEXT),
        })
        .filter(|s| !s.title.is_empty() && !(s.photoshop.is_empty() && s.topaz.is_empty()))
        .take(MAX_SUGGESTIONS)
        .collect();
    Ok(AiAdvice {
        provider: provider.id().to_owned(),
        model: model.to_owned(),
        summary: clean(&answer.summary, MAX_TEXT),
        suggestions,
        made_ms: now_ms,
    })
}

/// Advises on one photo (the JPEG bytes and its measurements) with one
/// provider's AI.
pub trait Adviser: Send + Sync {
    fn advise(&self, provider: Provider, jpeg: &[u8], m: &Measures) -> Result<AiAdvice, AiError>;
}

/// [`Adviser`] with the providers' APIs. The key and the model are asked
/// for each photo, so a change in Settings applies at once.
pub struct CloudAdviser {
    http: Arc<dyn Http>,
    key: Arc<KeyFn>,
    model: Arc<ModelFn>,
}

impl CloudAdviser {
    pub fn new(http: Arc<dyn Http>, key: Arc<KeyFn>, model: Arc<ModelFn>) -> Self {
        Self { http, key, model }
    }
}

impl Adviser for CloudAdviser {
    fn advise(&self, provider: Provider, jpeg: &[u8], m: &Measures) -> Result<AiAdvice, AiError> {
        let (text, model) = cloud::send(
            self.http.as_ref(),
            self.key.as_ref(),
            self.model.as_ref(),
            provider,
            jpeg,
            &task(m),
        )?;
        tidy(provider, &text, &model, crate::jobs::now_ms())
    }
}

#[cfg(test)]
mod tests {
    use super::super::rules::tests::fine;
    use super::*;
    use crate::ai::cloud::tests::Fake;
    use crate::ai::cloud::{claude, gemini, openai, CloudModel};

    const ANSWER: &str = r#"{"summary": "A strong portrait;\nthe sky is blown.",
        "suggestions": [
          {"title": "Recover the sky", "why": "The sky is white.",
           "photoshop": "Camera Raw > Light: Highlights -50", "topaz": ""},
          {"title": "", "why": "No title", "photoshop": "x", "topaz": "y"},
          {"title": "No steps", "why": "w", "photoshop": " ", "topaz": ""},
          {"title": "Soften the grain", "why": "Grain in the shadows.",
           "photoshop": "", "topaz": "Denoise: Standard"}]}"#;

    fn adviser(http: Arc<Fake>) -> CloudAdviser {
        CloudAdviser::new(
            http,
            Arc::new(|p: Provider| Some(format!("{}-test-key-000000", p.id()))),
            Arc::new(Provider::default_model),
        )
    }

    #[test]
    fn the_answer_is_tidied_and_empty_suggestions_dropped() {
        let a = tidy(Provider::Claude, ANSWER, "claude-opus-5-5", 7).unwrap();
        assert_eq!(a.summary, "A strong portrait; the sky is blown.");
        assert_eq!(a.provider, "claude");
        assert_eq!(a.model, "claude-opus-5-5");
        assert_eq!(a.made_ms, 7);
        let titles: Vec<&str> = a.suggestions.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Recover the sky", "Soften the grain"]);
        assert_eq!(a.suggestions[1].topaz, "Denoise: Standard");
        // Long text is cut, and there are never more than the most.
        let many: Vec<Value> = (0..20)
            .map(|i| json!({"title": format!("T{i}"), "why": "w".repeat(2000), "photoshop": "p", "topaz": ""}))
            .collect();
        let text = json!({"summary": "", "suggestions": many}).to_string();
        let a = tidy(Provider::Gemini, &text, "m", 0).unwrap();
        assert_eq!(a.suggestions.len(), MAX_SUGGESTIONS);
        assert_eq!(a.suggestions[0].why.chars().count(), MAX_TEXT);
        assert!(a.suggestions[0].why.ends_with('…'));
        match tidy(Provider::OpenAi, "not json", "m", 0) {
            Err(AiError::Cloud {
                retry: true,
                message,
            }) => {
                assert!(message.starts_with("ChatGPT's answer"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_question_carries_the_measurements() {
        let q = ask(&fine());
        assert!(q.starts_with("Suggest edits for this photo."));
        assert!(q.contains("- Brightness: Fine (middle tone 44%)"), "{q}");
        assert!(q.contains("- Level: No tilt found"));
        assert!(q.contains("darkest 5% below 8% grey, brightest 5% above 90% grey"));
        assert!(SYSTEM.contains("Never name or guess who a person is"));
    }

    #[test]
    fn every_provider_gets_the_photo_the_question_and_the_schema() {
        let t = task(&fine());
        let c = claude::body(CloudModel::ClaudeOpus55, b"foo", &t);
        assert_eq!(c["system"], SYSTEM);
        assert_eq!(c["messages"][0]["content"][0]["source"]["data"], "Zm9v");
        assert_eq!(c["messages"][0]["content"][1]["text"], t.ask.as_str());
        let cs = &c["output_config"]["format"]["schema"];
        assert_eq!(
            cs["properties"]["suggestions"]["items"]["required"][3],
            "topaz"
        );
        let o = openai::body(CloudModel::Gpt61Sol, b"foo", &t);
        assert_eq!(o["text"]["format"]["name"], "edit_suggestions");
        assert_eq!(o["text"]["format"]["strict"], true);
        // Strict answers need every object closed.
        let os = &o["text"]["format"]["schema"];
        assert_eq!(os["additionalProperties"], false);
        assert_eq!(
            os["properties"]["suggestions"]["items"]["additionalProperties"],
            false
        );
        // Gemini takes the schema without `additionalProperties`, at any depth.
        let g = gemini::body(b"foo", &t);
        let gs = &g["generationConfig"]["responseJsonSchema"];
        assert!(gs.get("additionalProperties").is_none());
        assert!(gs["properties"]["suggestions"]["items"]
            .get("additionalProperties")
            .is_none());
        assert_eq!(
            gs["properties"]["suggestions"]["items"]["required"][0],
            "title"
        );
        assert_eq!(g["contents"][0]["parts"][1]["text"], t.ask.as_str());
    }

    #[test]
    fn each_provider_answers_through_its_own_api() {
        let claude = json!({
            "model": "claude-opus-5-5", "stop_reason": "end_turn",
            "content": [{"type": "text", "text": ANSWER}],
        });
        let openai = json!({
            "model": "gpt-6.1-sol", "status": "completed",
            "output": [{"type": "message", "content": [{"type": "output_text", "text": ANSWER}]}],
        });
        let gemini = json!({
            "modelVersion": "gemini-3.6-flash",
            "candidates": [{"finishReason": "STOP", "content": {"parts": [{"text": ANSWER}]}}],
        });
        for (p, body, model) in [
            (Provider::Claude, claude, "claude-opus-5-5"),
            (Provider::OpenAi, openai, "gpt-6.1-sol"),
            (Provider::Gemini, gemini, "gemini-3.6-flash"),
        ] {
            let http = Fake::new(200, body.to_string());
            let a = adviser(http.clone()).advise(p, b"x", &fine()).unwrap();
            assert_eq!(a.provider, p.id());
            assert_eq!(a.model, model);
            assert_eq!(a.suggestions.len(), 2, "{p}");
            let (_, _, sent) = http.last();
            assert!(sent.contains("Suggest edits for this photo."), "{p}");
        }
    }

    #[test]
    fn problems_speak_of_the_photo_not_of_tags() {
        let refused = json!({"stop_reason": "refusal", "content": []}).to_string();
        let http = Fake::new(200, refused);
        match adviser(http).advise(Provider::Claude, b"x", &fine()) {
            Err(AiError::Cloud {
                retry: false,
                message,
            }) => {
                assert_eq!(message, "Claude declined to look at this photo");
            }
            other => panic!("{other:?}"),
        }
        let empty = json!({"model": "m", "stop_reason": "end_turn", "content": []}).to_string();
        match adviser(Fake::new(200, empty)).advise(Provider::Claude, b"x", &fine()) {
            Err(AiError::Cloud {
                retry: true,
                message,
            }) => {
                assert_eq!(message, "Claude's answer had no suggestions");
            }
            other => panic!("{other:?}"),
        }
        // No key: nothing is sent.
        let http = Fake::new(200, String::new());
        let none = CloudAdviser::new(
            http.clone(),
            Arc::new(|_| None),
            Arc::new(Provider::default_model),
        );
        assert!(matches!(
            none.advise(Provider::Gemini, b"x", &fine()),
            Err(AiError::NoApiKey(Provider::Gemini))
        ));
        assert!(http.sent.lock().unwrap().is_empty());
    }
}
