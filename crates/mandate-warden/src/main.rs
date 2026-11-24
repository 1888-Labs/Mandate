//! Mandate Warden: the only way an agent reaches the outside world.
//!
//! Runs on its own Fly.io Sprite, the only Sprite allowed to use the
//! connectors. Every call an agent makes arrives signed by the agent's key;
//! the Warden checks it against the agent's mandates and forwards it
//! through the connector only if it is allowed.

mod auth;
mod github;
mod sprites;
mod state;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::{Json, Router};
use mandate_core::{AuthorizationDecision, AuthorizationRequest, DenyReason, Mandate};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::auth::ReplayGuard;
use crate::sprites::Sprites;
use crate::state::{DecisionRecord, GrantRequest, WardenState};

const GITHUB_PREFIX: &str = "/v1/gateway/github/";

struct Config {
    github_connection: Option<String>,
    admin_token: Option<String>,
    agent_sprite: String,
    checkpoint_on_write: bool,
    restore_on_deny: bool,
}

struct App {
    config: Config,
    state: Mutex<WardenState>,
    replay: Mutex<ReplayGuard>,
    sprites: Sprites,
}

type Shared = Arc<App>;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn flag(key: &str) -> bool {
    env(key).is_some_and(|v| matches!(v.as_str(), "1" | "true" | "yes"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let data = PathBuf::from(env("WARDEN_DATA").unwrap_or_else(|| "warden-data".into()));
    let state = WardenState::open(data).expect("open warden state");
    let config = Config {
        github_connection: env("GITHUB_CONNECTION_ID"),
        admin_token: env("WARDEN_ADMIN_TOKEN"),
        agent_sprite: env("AGENT_SPRITE").unwrap_or_else(|| "mandate-agent".into()),
        checkpoint_on_write: flag("WARDEN_CHECKPOINT_ON_WRITE"),
        restore_on_deny: flag("WARDEN_RESTORE_ON_DENY"),
    };
    let sprites = Sprites::new(
        env("SPRITES_GATEWAY").unwrap_or_else(|| "https://api.sprites.dev/v1/gateway".into()),
        env("SPRITES_API").unwrap_or_else(|| "https://api.sprites.dev".into()),
        env("SPRITES_TOKEN"),
    );
    info!(owner = %state.owner(), admin = config.admin_token.is_some(), manage = sprites.can_manage(), "warden starting");

    let app = Arc::new(App {
        config,
        state: Mutex::new(state),
        replay: Mutex::new(ReplayGuard::default()),
        sprites,
    });
    let router = Router::new()
        .route("/", get(home))
        .route("/health", get(|| async { "ok" }))
        .route("/v1/state", get(snapshot))
        .route("/v1/mandates", post(submit_mandate))
        .route("/v1/admin/mandates", post(grant))
        .route("/v1/admin/revoke", post(revoke))
        .route("/v1/admin/agent/checkpoint", post(checkpoint_agent))
        .route("/v1/admin/agent/restore", post(restore_agent))
        .route("/v1/gateway/github/{*path}", any(github))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(app);

    let port = env("PORT").unwrap_or_else(|| "8080".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("bind");
    info!("listening on :{port}");
    axum::serve(listener, router).await.expect("serve");
}

async fn home(State(app): State<Shared>) -> Html<String> {
    let owner = app.state.lock().await.owner();
    Html(format!(
        "<!doctype html><meta charset=utf-8><title>Mandate Warden</title>\
         <body style=\"font:16px system-ui;max-width:40rem;margin:3rem auto;padding:0 1rem\">\
         <h1>Mandate Warden</h1><p>Every outbound call from the agent is checked here first.</p>\
         <p>Owner: <code>{owner}</code></p><p><a href=\"/v1/state\">Live state (JSON)</a></p>"
    ))
}

async fn snapshot(State(app): State<Shared>) -> Json<Value> {
    Json(app.state.lock().await.snapshot())
}

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "error": code, "message": message.into() })),
    )
        .into_response()
}

fn require_admin(app: &App, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = &app.config.admin_token else {
        return Err(error(
            StatusCode::FORBIDDEN,
            "ADMIN_DISABLED",
            "set WARDEN_ADMIN_TOKEN to enable admin endpoints",
        ));
    };
    let given = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if given != Some(expected.as_str()) {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "admin token required",
        ));
    }
    Ok(())
}

async fn grant(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<GrantRequest>,
) -> Response {
    if let Err(r) = require_admin(&app, &headers) {
        return r;
    }
    match app.state.lock().await.grant(req, now()) {
        Ok(m) => (StatusCode::CREATED, Json(m)).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, "BAD_GRANT", e),
    }
}

#[derive(Deserialize)]
struct SubmitBody {
    mandate: Mandate,
    #[serde(default)]
    label: Option<String>,
}

async fn submit_mandate(State(app): State<Shared>, Json(body): Json<SubmitBody>) -> Response {
    match app.state.lock().await.submit(body.mandate, body.label) {
        Ok(()) => (StatusCode::CREATED, Json(json!({ "ok": true }))).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, "BAD_MANDATE", e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevokeBody {
    mandate_id: String,
    #[serde(default)]
    reason: Option<String>,
}

async fn revoke(
    State(app): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<RevokeBody>,
) -> Response {
    if let Err(r) = require_admin(&app, &headers) {
        return r;
    }
    match app
        .state
        .lock()
        .await
        .revoke(&body.mandate_id, body.reason, now())
    {
        Ok(r) => (StatusCode::CREATED, Json(r)).into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, "BAD_REVOCATION", e),
    }
}

#[derive(Deserialize, Default)]
struct CheckpointBody {
    #[serde(default)]
    comment: Option<String>,
    /// Remember the new checkpoint as the known-good state to restore to.
    #[serde(default)]
    mark_good: bool,
}

async fn checkpoint_agent(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Option<Json<CheckpointBody>>,
) -> Response {
    if let Err(r) = require_admin(&app, &headers) {
        return r;
    }
    let body = body.map(|b| b.0).unwrap_or_default();
    let comment = body
        .comment
        .unwrap_or_else(|| "known good (mandate warden)".into());
    if let Err(e) = app
        .sprites
        .checkpoint(&app.config.agent_sprite, &comment)
        .await
    {
        return error(StatusCode::BAD_GATEWAY, "CHECKPOINT_FAILED", e);
    }
    let latest = latest_checkpoint(&app).await;
    if body.mark_good {
        let mut state = app.state.lock().await;
        state.good_checkpoint = latest.clone();
        let _ = state.save();
    }
    Json(json!({ "checkpoint": latest, "markedGood": body.mark_good })).into_response()
}

#[derive(Deserialize, Default)]
struct RestoreBody {
    #[serde(default)]
    checkpoint: Option<String>,
}

async fn restore_agent(
    State(app): State<Shared>,
    headers: HeaderMap,
    body: Option<Json<RestoreBody>>,
) -> Response {
    if let Err(r) = require_admin(&app, &headers) {
        return r;
    }
    let requested = body.and_then(|b| b.0.checkpoint);
    let target = match requested {
        Some(c) => Some(c),
        None => app.state.lock().await.good_checkpoint.clone(),
    };
    let Some(target) = target else {
        return error(
            StatusCode::BAD_REQUEST,
            "NO_CHECKPOINT",
            "no checkpoint given and none marked good",
        );
    };
    match app.sprites.restore(&app.config.agent_sprite, &target).await {
        Ok(_) => Json(json!({ "restored": target })).into_response(),
        Err(e) => error(StatusCode::BAD_GATEWAY, "RESTORE_FAILED", e),
    }
}

/// Newest checkpoint ID of the agent Sprite, if the API lists one.
async fn latest_checkpoint(app: &App) -> Option<String> {
    let list = app
        .sprites
        .list_checkpoints(&app.config.agent_sprite)
        .await
        .ok()?;
    let items = list
        .as_array()
        .cloned()
        .or_else(|| list.get("checkpoints").and_then(Value::as_array).cloned())?;
    items
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str))
        .filter(|id| !id.starts_with("auto-") && *id != "Current")
        .max_by_key(|id| id.trim_start_matches('v').parse::<u64>().unwrap_or(0))
        .map(String::from)
}

/// Human-readable explanation of a denial.
fn explain(reason: &DenyReason) -> String {
    match reason {
        DenyReason::NoMandate => {
            "No mandate grants this agent this action on this resource.".into()
        }
        DenyReason::InvalidSignature { .. } => {
            "A mandate in the chain has an invalid signature.".into()
        }
        DenyReason::InvalidDelegation { .. } => {
            "The delegation chain is broken or escalates authority.".into()
        }
        DenyReason::UntrustedRoot { issuer, .. } => {
            format!("The chain's root was issued by {issuer}, not the owner.")
        }
        DenyReason::Expired { expires_at, .. } => {
            format!("A mandate in the chain expired at {expires_at}.")
        }
        DenyReason::Revoked { .. } => "The agent's mandate was revoked.".into(),
        DenyReason::AncestorRevoked { revoked, .. } => {
            format!("A mandate above the agent's ({revoked}) was revoked.")
        }
        DenyReason::ActionNotAllowed => "The mandate does not cover this action.".into(),
        DenyReason::ResourceNotAllowed => "The mandate does not cover this resource.".into(),
        DenyReason::AmountExceeded { maximum, .. } => {
            format!("Over the mandate's limit of {maximum}.")
        }
        DenyReason::RecipientNotAllowed { requested, allowed } => format!(
            "Target {} is not one of the allowed: {}.",
            requested.as_deref().unwrap_or("(none)"),
            allowed.join(", ")
        ),
    }
}

async fn github(State(app): State<Shared>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let body: Bytes = match axum::body::to_bytes(body, 5 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "BODY_TOO_LARGE",
                "request body over 5 MiB",
            )
        }
    };
    let method = parts.method.as_str().to_string();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let path = parts
        .uri
        .path()
        .strip_prefix(GITHUB_PREFIX)
        .unwrap_or("")
        .to_string();
    let upstream_path = match parts.uri.query() {
        Some(q) => format!("{path}?{q}"),
        None => path.clone(),
    };
    let header = |k: &str| parts.headers.get(k).and_then(|v| v.to_str().ok());
    let t = now();

    // 1. Who is asking? The signer, not what the agent claims.
    let subject = {
        let mut replay = app.replay.lock().await;
        auth::verify(
            header("x-mandate-agent"),
            header("x-mandate-timestamp"),
            header("x-mandate-signature"),
            &method,
            &path_and_query,
            &body,
            t,
            &mut replay,
        )
    };
    let subject = match subject {
        Ok(s) => s,
        Err(e) => return error(StatusCode::UNAUTHORIZED, e.code(), format!("{e:?}")),
    };

    // 2. What are they asking to do?
    let json_body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let op = github::classify(&method, &path, &json_body);
    let request = AuthorizationRequest {
        subject: subject.clone(),
        action: op.action.clone(),
        resource: op.resource.clone(),
        amount: None,
        recipient: op.recipient.clone(),
        timestamp: t,
    };

    let mut record = DecisionRecord {
        at: t,
        agent: subject.0.clone(),
        method: method.clone(),
        path: format!("/{path}"),
        action: op.action.clone(),
        resource: op.resource.to_string(),
        recipient: op.recipient.clone(),
        decision: String::new(),
        reason: None,
        mandate_id: None,
        delegation_path: vec![],
        upstream_status: None,
        note: None,
    };

    // 3. Is it allowed?
    let decision = app.state.lock().await.authorize(&request);
    let evidence = match decision {
        AuthorizationDecision::Allow(evidence) => evidence,
        AuthorizationDecision::Deny(denial) => {
            let message = explain(&denial.reason);
            record.decision = "deny".into();
            record.reason = Some(denial.reason.code().into());
            record.mandate_id = denial.mandate_id.as_ref().map(|m| m.0.clone());
            record.delegation_path = denial.delegation_path.iter().map(|p| p.0.clone()).collect();
            if op.write && app.config.restore_on_deny && app.sprites.can_manage() {
                record.note = Some("agent restored to its last known-good checkpoint".into());
                spawn_restore(app.clone());
            }
            warn!(agent = %subject, action = %op.action, reason = denial.reason.code(), "deny");
            let body = json!({
                "decision": "deny",
                "reason": denial.reason.code(),
                "message": message,
                "action": op.action,
                "resource": op.resource.to_string(),
                "recipient": op.recipient,
                "mandateId": record.mandate_id,
                "delegationPath": record.delegation_path,
                "note": record.note,
            });
            app.state.lock().await.record(record);
            return (
                StatusCode::FORBIDDEN,
                [("x-mandate-decision", "deny")],
                Json(body),
            )
                .into_response();
        }
    };

    record.decision = "allow".into();
    record.mandate_id = Some(evidence.mandate_id.0.clone());
    record.delegation_path = evidence
        .delegation_path
        .iter()
        .map(|p| p.0.clone())
        .collect();

    // 4. Allowed: snapshot the agent before any write, then forward.
    if op.write && app.config.checkpoint_on_write && app.sprites.can_manage() {
        let comment = format!("before {} on {}", op.action, op.resource);
        let done = tokio::time::timeout(
            Duration::from_secs(60),
            app.sprites.checkpoint(&app.config.agent_sprite, &comment),
        )
        .await;
        record.note = Some(match done {
            Ok(Ok(_)) => format!("agent checkpointed: {comment}"),
            Ok(Err(e)) => format!("checkpoint failed: {e}"),
            Err(_) => "checkpoint timed out".into(),
        });
    }

    let Some(connection) = app.config.github_connection.clone() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "NO_CONNECTOR",
            "GITHUB_CONNECTION_ID is not set",
        );
    };
    let upstream = app
        .sprites
        .forward(
            "github",
            &connection,
            &method,
            &upstream_path,
            header("content-type"),
            body.to_vec(),
        )
        .await;
    match upstream {
        Ok(up) => {
            record.upstream_status = Some(up.status);
            info!(agent = %subject, action = %op.action, status = up.status, "allow");
            app.state.lock().await.record(record);
            let mut res = Response::builder()
                .status(up.status)
                .header("x-mandate-decision", "allow")
                .header("x-mandate-mandate-id", evidence.mandate_id.0);
            if let Some(ct) = up.content_type {
                res = res.header("content-type", ct);
            }
            res.body(axum::body::Body::from(up.body))
                .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
        }
        Err(e) => {
            record.note = Some(format!("upstream error: {e}"));
            app.state.lock().await.record(record);
            error(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", e)
        }
    }
}

fn spawn_restore(app: Shared) {
    tokio::spawn(async move {
        let Some(target) = app.state.lock().await.good_checkpoint.clone() else {
            warn!("restore on deny: no known-good checkpoint marked");
            return;
        };
        match app.sprites.restore(&app.config.agent_sprite, &target).await {
            Ok(_) => info!(checkpoint = %target, "agent restored after denied write"),
            Err(e) => warn!(error = %e, "agent restore failed"),
        }
    });
}
