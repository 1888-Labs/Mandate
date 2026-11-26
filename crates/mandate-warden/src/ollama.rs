//! Turns an Ollama API call into the action, resource and model Mandate checks.
//!
//! The model is the `recipient`, so an owner can pin which models an agent
//! may think with through `allowedRecipients`.

use mandate_core::ResourceId;
use serde_json::Value;

use crate::github::Operation;

/// The single resource every Ollama call is checked against.
pub const RESOURCE: &str = "ollama:cloud";

/// Classify `method` + `path` (no leading slash, no query) with its JSON body.
pub fn classify(method: &str, path: &str, body: &Value) -> Operation {
    let method = method.to_ascii_uppercase();
    let path = path.trim_matches('/');
    let model = body.get("model").and_then(Value::as_str).map(String::from);
    let (action, recipient) = match (method.as_str(), path) {
        ("POST", "api/chat") => ("model.chat".to_string(), model),
        ("POST", "api/generate") => ("model.generate".to_string(), model),
        ("GET", "api/tags") => ("model.list".to_string(), None),
        (m, p) => (
            format!(
                "other.{}.{}",
                m.to_ascii_lowercase(),
                p.split('/').next_back().unwrap_or("root")
            ),
            None,
        ),
    };
    Operation {
        action,
        // Infallible: RESOURCE is a constant with a `kind:` prefix.
        resource: RESOURCE
            .parse::<ResourceId>()
            .unwrap_or_else(|_| unreachable!()),
        recipient,
        // Model calls change nothing upstream: no checkpoint, no restore.
        write: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_carries_its_model() {
        let op = classify("POST", "api/chat", &json!({"model": "gpt-oss:120b"}));
        assert_eq!(op.action, "model.chat");
        assert_eq!(op.recipient.as_deref(), Some("gpt-oss:120b"));
        assert_eq!(op.resource.to_string(), RESOURCE);
        assert!(!op.write);
    }

    #[test]
    fn unknown_calls_get_ungrantable_actions() {
        let op = classify("DELETE", "api/delete", &json!({"model": "x"}));
        assert_eq!(op.action, "other.delete.delete");
        assert_eq!(
            classify("GET", "api/tags", &Value::Null).action,
            "model.list"
        );
    }
}
