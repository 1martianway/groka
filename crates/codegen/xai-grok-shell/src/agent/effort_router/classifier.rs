//! Fail-open LLM judge: structured JSON on grok-4.6 at low effort.
//!
//! The judge is only consulted when the hybrid cascade is ambiguous. Timeout,
//! parse failure, or a missing client all fall back to the heuristic leaf.

use super::ClassifierVerdict;
use crate::sampling::{
    Client as OaiCompatClient, ConversationItem, ConversationRequest, ConversationToolChoice,
    ToolSpec,
};
use xai_grok_sampling_types::ReasoningEffort;

const JUDGE_SYSTEM: &str = "\
You classify how much reasoning effort a coding-agent turn needs.
Reply only via the route_effort tool.
low = greeting, thanks, trivial yes/no, or a one-line follow-up that needs no planning.
medium = ordinary explanation, small edit, or a well-specified implementation.
high = debug, architecture, multi-file design, races, security, or an underspecified hard task.
Use the prompt meaning, not keyword matching. Prefer medium when unsure.";

/// Build the judge request. Always grok-4.6 (or the session's current model
/// — callers pass the catalog default, which is grok-4.6) at low effort.
pub fn build_classifier_request(
    model: &str,
    prompt: &str,
    stage_summary: Option<&str>,
) -> ConversationRequest {
    let mut user = format!(
        "<user_query>\n{}\n</user_query>",
        truncate_chars(prompt.trim(), 2000)
    );
    if let Some(stage) = stage_summary {
        user.push_str("\n<stage>\n");
        user.push_str(stage);
        user.push_str("\n</stage>");
    }
    let mut req = ConversationRequest::from_items(vec![
        ConversationItem::system(JUDGE_SYSTEM),
        ConversationItem::user(user),
    ])
    .with_model(model)
    .with_tools(vec![ToolSpec {
        name: "route_effort".to_owned(),
        description: Some("Pick the reasoning effort for this turn".to_owned()),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["effort", "confidence", "reason"],
            "properties": {
                "effort": {
                    "type": "string",
                    "enum": ["low", "medium", "high"]
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0
                },
                "reason": {
                    "type": "string",
                    "description": "At most 12 words"
                }
            },
            "additionalProperties": false
        }),
    }])
    .with_max_output_tokens(64)
    .with_temperature(0.0)
    .with_tool_choice(ConversationToolChoice::Function("route_effort".to_owned()));
    req.reasoning_effort = Some(ReasoningEffort::Low);
    req
}

/// Parse a judge payload from tool-call arguments or raw assistant text.
pub fn parse_classifier_verdict(raw: &str) -> Option<ClassifierVerdict> {
    let value = extract_json(raw)?;
    let effort = value
        .get("effort")
        .and_then(|v| v.as_str())
        .and_then(parse_effort)?;
    let confidence = value
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.6) as f32;
    let reason = value
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .chars()
        .take(80)
        .collect::<String>();
    Some(ClassifierVerdict {
        effort,
        confidence: confidence.clamp(0.0, 1.0),
        reason,
    })
}

fn parse_effort(s: &str) -> Option<ReasoningEffort> {
    match s.trim().to_ascii_lowercase().as_str() {
        "low" | "minimal" | "none" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" | "xhigh" | "max" => Some(ReasoningEffort::High),
        _ => None,
    }
}

fn extract_json(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(v);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&trimmed[start..=end]).ok()
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

/// Run the judge with a hard timeout. `None` = fall open.
pub async fn classify_with_timeout(
    client: OaiCompatClient,
    request: ConversationRequest,
    timeout: std::time::Duration,
) -> Option<ClassifierVerdict> {
    let response = tokio::time::timeout(timeout, client.conversation_collect(request))
        .await
        .ok()?
        .ok()?;
    if let Some(a) = response.assistant()
        && let Some(call) = a.tool_calls.first()
    {
        return parse_classifier_verdict(call.arguments.as_ref());
    }
    response
        .assistant()
        .and_then(|a| parse_classifier_verdict(a.content.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let v = parse_classifier_verdict(
            r#"{"effort":"high","confidence":0.9,"reason":"multi-file race"}"#,
        )
        .unwrap();
        assert_eq!(v.effort, ReasoningEffort::High);
        assert!((v.confidence - 0.9).abs() < f32::EPSILON);
        assert_eq!(v.reason, "multi-file race");
    }

    #[test]
    fn parses_json_in_prose() {
        let v = parse_classifier_verdict(
            "sure {\"effort\":\"low\",\"confidence\":0.8,\"reason\":\"just a greeting\"} thanks",
        )
        .unwrap();
        assert_eq!(v.effort, ReasoningEffort::Low);
    }

    #[test]
    fn rejects_bad_effort_token() {
        assert!(parse_classifier_verdict(r#"{"effort":"quantum","confidence":1}"#).is_none());
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_classifier_verdict("").is_none());
        assert!(parse_classifier_verdict("not json").is_none());
    }

    #[test]
    fn maps_aliases_into_triad() {
        let v = parse_classifier_verdict(r#"{"effort":"xhigh","confidence":1,"reason":"x"}"#).unwrap();
        assert_eq!(v.effort, ReasoningEffort::High);
        let v = parse_classifier_verdict(r#"{"effort":"minimal","confidence":1,"reason":"x"}"#)
            .unwrap();
        assert_eq!(v.effort, ReasoningEffort::Low);
    }

    #[test]
    fn request_is_low_effort_on_passed_model() {
        let req = build_classifier_request("grok-4.6", "hi", None);
        assert_eq!(req.model.as_deref(), Some("grok-4.6"));
        assert_eq!(req.reasoning_effort, Some(ReasoningEffort::Low));
        assert_eq!(req.max_output_tokens, Some(64));
    }
}
