//! Turns a GitHub REST call into the action, resource and limits Mandate checks.
//!
//! The Warden never forwards a call it could not classify into a mandate
//! request: anything unrecognised becomes an action no mandate grants.

use mandate_core::ResourceId;
use serde_json::Value;

/// What a single GitHub API call is asking to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    /// e.g. `issues.create`, `contents.write`, `read`.
    pub action: String,
    /// `github:owner/repo`, or `github:account` for calls outside a repo.
    pub resource: ResourceId,
    /// The branch a write lands on, checked against `allowedRecipients`.
    pub recipient: Option<String>,
    /// Whether the call changes anything upstream.
    pub write: bool,
}

/// Classify `method` + `path` (no leading slash, no query) with its JSON body.
pub fn classify(method: &str, path: &str, body: &Value) -> Operation {
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let method = method.to_ascii_uppercase();
    let write = !matches!(method.as_str(), "GET" | "HEAD");

    let (resource, rest) = match segments.as_slice() {
        ["repos", owner, repo, rest @ ..] => (format!("github:{owner}/{repo}"), rest.to_vec()),
        _ => {
            let action = if write { "account.write" } else { "read" };
            return Operation {
                action: action.to_string(),
                resource: resource("github:account"),
                recipient: None,
                write,
            };
        }
    };

    let branch = |key: &str| body.get(key).and_then(Value::as_str).map(strip_heads);
    let (action, recipient): (&str, Option<String>) = match (method.as_str(), rest.as_slice()) {
        ("GET" | "HEAD", _) => ("read", None),
        ("POST", ["issues"]) => ("issues.create", None),
        ("POST", ["issues", _, "comments"]) => ("issues.comment", None),
        ("PATCH", ["issues", _]) => ("issues.update", None),
        ("POST", ["issues", _, "labels"]) => ("issues.label", None),
        ("POST", ["pulls"]) => ("pulls.create", branch("base")),
        ("PUT", ["pulls", _, "merge"]) => ("pulls.merge", None),
        ("PUT", ["contents", ..]) => (
            "contents.write",
            Some(branch("branch").unwrap_or_else(|| "main".into())),
        ),
        ("DELETE", ["contents", ..]) => (
            "contents.delete",
            Some(branch("branch").unwrap_or_else(|| "main".into())),
        ),
        ("POST", ["git", "refs"]) => ("refs.create", branch("ref")),
        ("PATCH", ["git", "refs", "heads", name @ ..]) => ("refs.update", Some(name.join("/"))),
        ("DELETE", ["git", "refs", "heads", name @ ..]) => ("refs.delete", Some(name.join("/"))),
        ("POST", ["git", "blobs" | "trees" | "commits"]) => ("git.objects", None),
        ("DELETE", _) => ("delete", None),
        (_, [first, ..]) => return other(&method, first, resource, write),
        (_, []) => return other(&method, "repo", resource, write),
    };

    Operation {
        action: action.to_string(),
        resource: resource_owned(resource),
        recipient,
        write,
    }
}

fn other(method: &str, first: &str, resource: String, write: bool) -> Operation {
    Operation {
        action: format!("other.{}.{first}", method.to_ascii_lowercase()),
        resource: resource_owned(resource),
        recipient: None,
        write,
    }
}

fn strip_heads(s: &str) -> String {
    s.strip_prefix("refs/heads/").unwrap_or(s).to_string()
}

fn resource(s: &str) -> ResourceId {
    s.parse().expect("static resource is valid")
}

fn resource_owned(s: String) -> ResourceId {
    s.parse().expect("github: prefix makes a valid resource")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const REPO: &str = "repos/dicethedev/mandate-demo";

    #[test]
    fn reads_are_read() {
        let op = classify("GET", &format!("{REPO}/issues"), &Value::Null);
        assert_eq!(op.action, "read");
        assert_eq!(op.resource.to_string(), "github:dicethedev/mandate-demo");
        assert!(!op.write);
    }

    #[test]
    fn issue_and_comment_writes() {
        assert_eq!(
            classify("POST", &format!("{REPO}/issues"), &json!({})).action,
            "issues.create"
        );
        assert_eq!(
            classify("POST", &format!("{REPO}/issues/7/comments"), &json!({})).action,
            "issues.comment"
        );
    }

    #[test]
    fn content_writes_carry_their_branch() {
        let op = classify(
            "PUT",
            &format!("{REPO}/contents/README.md"),
            &json!({"branch": "agent/fix"}),
        );
        assert_eq!(op.action, "contents.write");
        assert_eq!(op.recipient.as_deref(), Some("agent/fix"));
        let to_main = classify("PUT", &format!("{REPO}/contents/README.md"), &json!({}));
        assert_eq!(to_main.recipient.as_deref(), Some("main"));
    }

    #[test]
    fn branch_creation_strips_refs_heads() {
        let op = classify(
            "POST",
            &format!("{REPO}/git/refs"),
            &json!({"ref": "refs/heads/agent/fix"}),
        );
        assert_eq!(op.action, "refs.create");
        assert_eq!(op.recipient.as_deref(), Some("agent/fix"));
    }

    #[test]
    fn unknown_calls_get_ungrantable_actions() {
        let op = classify("PUT", &format!("{REPO}/collaborators/eve"), &json!({}));
        assert_eq!(op.action, "other.put.collaborators");
        let acct = classify("POST", "user/repos", &json!({}));
        assert_eq!(acct.action, "account.write");
        assert_eq!(acct.resource.to_string(), "github:account");
    }
}
