//! `mandate-agent`: a small personal coding agent that lives on its own Sprite.
//!
//! It thinks with an Ollama model and acts on GitHub, and it can do neither
//! directly: both go through the Mandate Warden, signed with the agent's key.
//! The Warden decides; a denial comes back to the model as the tool result.
//!
//! ```text
//! mandate-agent "Triage the open issues and label anything that is a bug"
//! ```
//! Env: `MANDATE_AGENT_KEY`, `MANDATE_WARDEN_URL`, `MANDATE_REPO`
//! (default dicethedev/mandate-demo), `MANDATE_MODEL` (default gpt-oss:120b),
//! `MANDATE_MAX_STEPS` (default 12).

#[path = "../auth.rs"]
#[allow(dead_code)]
mod auth;
#[path = "../client.rs"]
mod client;

use std::process::ExitCode;

use serde_json::{json, Value};

use crate::client::WardenClient;

const MAX_TOOL_OUTPUT: usize = 6000;

#[tokio::main]
async fn main() -> ExitCode {
    let task = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if task.trim().is_empty() {
        eprintln!("usage: mandate-agent \"<task>\"");
        return ExitCode::FAILURE;
    }
    let client = match WardenClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mandate-agent: {e}");
            return ExitCode::FAILURE;
        }
    };
    let repo = var("MANDATE_REPO", "dicethedev/mandate-demo");
    let model = var("MANDATE_MODEL", "gpt-oss:120b");
    let max_steps: usize = var("MANDATE_MAX_STEPS", "12").parse().unwrap_or(12);

    println!("agent {} · model {model} · repo {repo}", client.address());
    println!("task: {task}\n");

    let mut messages = vec![
        json!({ "role": "system", "content": system_prompt(&repo) }),
        json!({ "role": "user", "content": task }),
    ];

    for _ in 0..max_steps {
        let request = json!({
            "model": model,
            "stream": false,
            "messages": messages,
            "tools": [github_tool()],
        });
        let reply = match client
            .call("ollama", "POST", "api/chat", request.to_string().as_bytes())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("model call failed: {e}");
                return ExitCode::FAILURE;
            }
        };
        if reply.decision == "deny" || !(200..300).contains(&reply.status) {
            eprintln!(
                "✗ model call refused (HTTP {} · mandate: {}): {}",
                reply.status, reply.decision, reply.body
            );
            return ExitCode::from(2);
        }
        let parsed: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
        let message = parsed.get("message").cloned().unwrap_or(Value::Null);
        let content = message.get("content").and_then(Value::as_str).unwrap_or("");
        let calls = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        messages.push(message.clone());

        if calls.is_empty() {
            println!("\n{}", content.trim());
            return ExitCode::SUCCESS;
        }
        if !content.trim().is_empty() {
            println!("· {}", content.trim());
        }
        for call in calls {
            let args = call
                .pointer("/function/arguments")
                .cloned()
                .unwrap_or(Value::Null);
            let result = run_github(&client, &args).await;
            messages.push(json!({
                "role": "tool",
                "tool_name": "github_request",
                "content": result,
            }));
        }
    }
    println!("\nstopped after {max_steps} steps");
    ExitCode::SUCCESS
}

fn var(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn system_prompt(repo: &str) -> String {
    format!(
        "You are a personal coding agent running on a Fly.io Sprite. You work on the \
         GitHub repository {repo}. Your only way to act is the github_request tool, which \
         calls the GitHub REST API (paths like repos/{repo}/issues). Every call passes \
         through the Mandate Warden, which enforces exactly what your owner has authorized. \
         If a call is denied, do not look for a way around it: say what was denied and why, \
         then carry on with what you are allowed to do or stop. To write a file, send \
         `content_text` with the plain text instead of base64 `content`. Keep your final \
         answer short."
    )
}

fn github_tool() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "github_request",
            "description": "Call the GitHub REST API through the Mandate Warden.",
            "parameters": {
                "type": "object",
                "properties": {
                    "method": { "type": "string", "enum": ["GET", "POST", "PATCH", "PUT", "DELETE"] },
                    "path": { "type": "string", "description": "API path without leading slash, e.g. repos/OWNER/REPO/issues" },
                    "body": { "type": "object", "description": "JSON body for writes" }
                },
                "required": ["method", "path"]
            }
        }
    })
}

async fn run_github(client: &WardenClient, args: &Value) -> String {
    let args = match args {
        // Some models send arguments as a JSON string.
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        other => other.clone(),
    };
    let method = args.get("method").and_then(Value::as_str).unwrap_or("GET");
    let path = args.get("path").and_then(Value::as_str).unwrap_or("");
    let mut body = args.get("body").cloned().unwrap_or(Value::Null);
    if let Some(obj) = body.as_object_mut() {
        if let Some(text) = obj.remove("content_text") {
            let text = text.as_str().unwrap_or_default().to_string();
            obj.insert("content".into(), Value::String(base64(text.as_bytes())));
        }
    }
    let bytes = if body.is_null() {
        Vec::new()
    } else {
        body.to_string().into_bytes()
    };

    match client.call("github", method, path, &bytes).await {
        Ok(r) => {
            let mark = match r.decision.as_str() {
                "allow" => "✓ allow",
                "deny" => "✗ DENY ",
                _ => "! error",
            };
            println!("{mark}  {method} {path}  (HTTP {})", r.status);
            if r.decision == "deny" {
                let why: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
                if let Some(m) = why.get("message").and_then(Value::as_str) {
                    println!("         {m}");
                }
            }
            let mut out = format!(
                "HTTP {} · mandate decision: {}\n{}",
                r.status, r.decision, r.body
            );
            if out.len() > MAX_TOOL_OUTPUT {
                let mut cut = MAX_TOOL_OUTPUT;
                while !out.is_char_boundary(cut) {
                    cut -= 1;
                }
                out.truncate(cut);
                out.push_str("\n…(truncated)");
            }
            out
        }
        Err(e) => {
            println!("! error  {method} {path}: {e}");
            format!("request failed: {e}")
        }
    }
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
