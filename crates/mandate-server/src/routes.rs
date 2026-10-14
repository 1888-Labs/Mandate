//! HTTP handlers. The server holds no private keys: clients submit
//! documents they signed themselves. The server verifies them before
//! publishing, so invalid ones never reach Arkiv or cost gas.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mandate_core::{
    authorize, can_delegate, find_mandate, find_valid_revocation, verify_chain,
    AuthorizationRequest, Mandate, MandateId, Revocation, TrustAnchors,
};
use mandate_crypto::{verify_mandate_signature, verify_revocation_signature, EthereumVerifier};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::api::{decision_json, denial_json, mandate_status_json, ApiError, AuthorizeBody};
use crate::backend::{Backend, BackendError};
use crate::demo::{Demo, DemoError};
use crate::{snapshot, web};

pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct AppState<B> {
    pub backend: B,
    pub anchors: TrustAnchors,
    pub clock: Clock,
    /// Demo mode for the web UI (`DEMO_KEYS`), if enabled.
    pub demo: Option<Demo>,
    /// The adapter's live event stream, relayed at `/v1/events`.
    pub events_url: Option<String>,
    /// The built web UI (`apps/web/dist`), served at `/`.
    pub web_dir: Option<std::path::PathBuf>,
}

type Shared<B> = State<Arc<AppState<B>>>;
type ApiResult = Result<(StatusCode, Json<Value>), ApiError>;

pub fn router<B: Backend>(state: AppState<B>) -> Router {
    let web_dir = state
        .web_dir
        .clone()
        .filter(|d| d.join("index.html").is_file());
    let app = Router::new()
        .route("/health", get(health::<B>))
        .route("/v1/events", get(events::<B>))
        .route("/v1/demo", get(demo_info::<B>))
        .route("/v1/demo/sandbox", post(demo_sandbox::<B>))
        .route("/v1/demo/sandbox/{id}/revoke", post(demo_revoke::<B>))
        .route("/v1/authorize", post(authorize_handler::<B>))
        .route("/v1/mandates", post(create_mandate::<B>))
        .route("/v1/mandates/{id}", get(get_mandate::<B>))
        .route("/v1/mandates/{id}/chain", get(get_chain::<B>))
        .route("/v1/mandates/{id}/delegate", post(delegate::<B>))
        .route("/v1/mandates/{id}/revoke", post(revoke::<B>))
        .route("/v1/agents/{id}/mandates", get(agent_mandates::<B>))
        // Unknown API paths are JSON 404s, not the web app.
        .route("/v1/{*rest}", axum::routing::any(api_not_found))
        .with_state(Arc::new(state));
    // Everything else is the single-page web app.
    match web_dir {
        Some(dir) => app.fallback_service(
            tower_http::services::ServeDir::new(&dir)
                .fallback(tower_http::services::ServeFile::new(dir.join("index.html"))),
        ),
        None => app.fallback(web::not_built),
    }
}

async fn api_not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "no such API endpoint")
}

fn unavailable(e: BackendError) -> ApiError {
    match e {
        BackendError::Rejected { code, message } => {
            warn!(code = %code, "backend rejected the request");
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "PUBLISH_REJECTED",
                message,
            )
            .with(json!({ "backendError": code }))
        }
        BackendError::Unavailable(m) => {
            warn!(error = %m, "authorization state unavailable");
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "STATE_UNAVAILABLE",
                format!("authorization state could not be read: {m}"),
            )
        }
    }
}

fn bad_json(e: JsonRejection) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "INVALID_REQUEST", e.body_text())
}

fn parse_id(raw: &str) -> Result<MandateId, ApiError> {
    let ok = raw.len() == 66
        && raw.starts_with("0x")
        && raw[2..]
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if ok {
        Ok(MandateId(raw.to_string()))
    } else {
        Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAMETER",
            "mandate id must be 0x + 64 lowercase hex",
        ))
    }
}

/// The verified copy of mandate `id` and a snapshot of its chain.
async fn load<B: Backend>(
    state: &AppState<B>,
    id: &MandateId,
) -> Result<(Mandate, snapshot::Snapshot), ApiError> {
    let snap = snapshot::for_mandate(&state.backend, id)
        .await
        .map_err(unavailable)?;
    let mandate = find_mandate(&snap.store, &EthereumVerifier, id)
        .filter(|m| verify_mandate_signature(m).is_ok())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "MANDATE_NOT_FOUND",
                format!("no valid mandate {id}"),
            )
        })?;
    Ok((mandate, snap))
}

async fn health<B: Backend>(State(state): Shared<B>) -> Json<Value> {
    Json(json!({ "ok": true, "backend": state.backend.name() }))
}

async fn authorize_handler<B: Backend>(
    State(state): Shared<B>,
    body: Result<Json<AuthorizeBody>, JsonRejection>,
) -> ApiResult {
    let Json(body) = body.map_err(bad_json)?;
    let request = AuthorizationRequest {
        subject: body.subject,
        action: body.action,
        resource: body.resource,
        amount: body.amount,
        recipient: body.recipient,
        timestamp: (state.clock)(),
    };
    let snap = snapshot::for_request(&state.backend, &request)
        .await
        .map_err(unavailable)?;
    let decision = authorize(&request, &snap.store, &EthereumVerifier, &state.anchors);
    match &decision {
        mandate_core::AuthorizationDecision::Allow(e) => info!(
            subject = %request.subject, action = %request.action, resource = %request.resource,
            mandate = %e.mandate_id, "authorization allowed"
        ),
        mandate_core::AuthorizationDecision::Deny(d) => info!(
            subject = %request.subject, action = %request.action, resource = %request.resource,
            reason = d.reason.code(), "authorization denied"
        ),
    }
    Ok((StatusCode::OK, Json(decision_json(&decision))))
}

async fn create_mandate<B: Backend>(
    State(state): Shared<B>,
    body: Result<Json<Mandate>, JsonRejection>,
) -> ApiResult {
    let Json(m) = body.map_err(bad_json)?;
    let reject = |status, code, msg: &str| {
        warn!(mandate = %m.id, code, "mandate rejected");
        ApiError::new(status, code, msg.to_string())
    };
    if let Err(e) = verify_mandate_signature(&m) {
        return Err(reject(
            StatusCode::BAD_REQUEST,
            "INVALID_SIGNATURE",
            &e.to_string(),
        ));
    }
    if m.parent.is_some() {
        return Err(reject(
            StatusCode::BAD_REQUEST,
            "NOT_A_ROOT_MANDATE",
            "mandates with a parent go to POST /v1/mandates/{parent}/delegate",
        ));
    }
    if !state.anchors.is_trusted(&m.resource, &m.issuer) {
        return Err(reject(
            StatusCode::FORBIDDEN,
            "UNTRUSTED_ROOT",
            "the issuer is not a trusted owner of this resource",
        ));
    }
    if (state.clock)() >= m.expires_at {
        return Err(reject(
            StatusCode::BAD_REQUEST,
            "EXPIRED",
            "the mandate has already expired",
        ));
    }
    let published = publish_mandate(&state.backend, &m).await?;
    info!(mandate = %m.id, issuer = %m.issuer, subject = %m.subject, "mandate created");
    Ok((
        status_for(published.created),
        Json(json!({ "mandateId": m.id, "published": published })),
    ))
}

async fn delegate<B: Backend>(
    State(state): Shared<B>,
    Path(parent_id): Path<String>,
    body: Result<Json<Mandate>, JsonRejection>,
) -> ApiResult {
    let parent_id = parse_id(&parent_id)?;
    let Json(child) = body.map_err(bad_json)?;
    if child.parent.as_ref() != Some(&parent_id) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "PARENT_MISMATCH",
            "the mandate's parent must be the mandate in the URL",
        ));
    }
    if let Err(e) = verify_mandate_signature(&child) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_SIGNATURE",
            e.to_string(),
        ));
    }
    let now = (state.clock)();
    let (parent, snap) = load(&state, &parent_id).await?;
    if let Err(d) = verify_chain(
        parent.clone(),
        &snap.store,
        &EthereumVerifier,
        &state.anchors,
        now,
    ) {
        warn!(parent = %parent_id, reason = d.reason.code(), "delegation from unusable parent rejected");
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "PARENT_UNUSABLE",
            "the parent mandate no longer grants authority",
        )
        .with(json!({ "parentDenial": denial_json(&d) })));
    }
    if let Err(e) = can_delegate(&parent, &child) {
        warn!(parent = %parent_id, child = %child.id, reason = e.code(), "delegation rejected");
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "DELEGATION_REJECTED",
            format!("{e}: a delegated mandate may only narrow its parent's authority"),
        )
        .with(json!({ "reason": e.code() })));
    }
    if now >= child.expires_at {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "EXPIRED",
            "the mandate has already expired",
        ));
    }
    let published = publish_mandate(&state.backend, &child).await?;
    info!(parent = %parent_id, mandate = %child.id, subject = %child.subject, "mandate delegated");
    Ok((
        status_for(published.created),
        Json(json!({ "mandateId": child.id, "published": published })),
    ))
}

async fn revoke<B: Backend>(
    State(state): Shared<B>,
    Path(id): Path<String>,
    body: Result<Json<Revocation>, JsonRejection>,
) -> ApiResult {
    let id = parse_id(&id)?;
    let Json(r) = body.map_err(bad_json)?;
    if r.mandate_id != id {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "MANDATE_MISMATCH",
            "the revocation must target the mandate in the URL",
        ));
    }
    if let Err(e) = verify_revocation_signature(&r) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_SIGNATURE",
            e.to_string(),
        ));
    }
    let (mandate, _) = load(&state, &id).await?;
    if r.issuer != mandate.issuer {
        warn!(mandate = %id, revoker = %r.issuer, "revocation by non-issuer rejected");
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "NOT_MANDATE_ISSUER",
            "only the mandate's issuer may revoke it",
        ));
    }
    let published = state
        .backend
        .publish_revocation(&r, mandate.expires_at)
        .await
        .map_err(|e| {
            warn!(revocation = %r.id, error = %e, "Arkiv write failed");
            unavailable(e)
        })?;
    info!(mandate = %id, revocation = %r.id, "mandate revoked");
    Ok((
        status_for(published.created),
        Json(json!({ "revocationId": r.id, "mandateId": id, "published": published })),
    ))
}

async fn get_mandate<B: Backend>(State(state): Shared<B>, Path(id): Path<String>) -> ApiResult {
    let id = parse_id(&id)?;
    let (mandate, snap) = load(&state, &id).await?;
    let status = verify_chain(
        mandate.clone(),
        &snap.store,
        &EthereumVerifier,
        &state.anchors,
        (state.clock)(),
    );
    Ok((
        StatusCode::OK,
        Json(mandate_status_json(
            &mandate,
            status.as_ref().map(|_| ()),
            snap.mandates.get(&mandate.id),
        )),
    ))
}

async fn get_chain<B: Backend>(State(state): Shared<B>, Path(id): Path<String>) -> ApiResult {
    let id = parse_id(&id)?;
    let (mandate, snap) = load(&state, &id).await?;
    // Every link we can resolve, root first, whether or not it is valid.
    let mut links = vec![mandate.clone()];
    while let Some(parent) = links
        .last()
        .and_then(|m| m.parent.as_ref())
        .and_then(|p| find_mandate(&snap.store, &EthereumVerifier, p))
    {
        if links.len() > mandate_core::authorization::MAX_CHAIN_DEPTH {
            break;
        }
        links.push(parent);
    }
    links.reverse();
    let status = verify_chain(
        mandate,
        &snap.store,
        &EthereumVerifier,
        &state.anchors,
        (state.clock)(),
    );
    // Where each link lives on Arkiv, and its `$creator`.
    let arkiv: serde_json::Map<String, Value> = links
        .iter()
        .filter_map(|m| Some((m.id.0.clone(), json!(snap.mandates.get(&m.id)?))))
        .collect();
    let revocations: Vec<Value> = links
        .iter()
        .filter_map(|m| find_valid_revocation(m, &snap.store, &EthereumVerifier))
        .map(|r| json!({ "revocation": r, "arkiv": snap.revocations.get(&r.id) }))
        .collect();
    let mut body = json!({
        "mandateId": id,
        "valid": status.is_ok(),
        "links": links,
        "arkiv": arkiv,
        "revocations": revocations,
    });
    if let Value::Object(map) = &mut body {
        match &status {
            Ok(v) => {
                map.insert("delegationPath".into(), json!(v.delegation_path()));
                map.insert(
                    "effectiveConstraints".into(),
                    json!(v.effective_constraints),
                );
            }
            Err(d) => {
                map.insert("delegationPath".into(), json!(d.delegation_path));
                map.insert("denial".into(), denial_json(d));
            }
        }
    }
    Ok((StatusCode::OK, Json(body)))
}

#[derive(serde::Deserialize)]
struct AgentQuery {
    /// Only mandates whose signed expiry is still in the future.
    #[serde(default)]
    active: bool,
}

async fn agent_mandates<B: Backend>(
    State(state): Shared<B>,
    Path(agent): Path<String>,
    Query(q): Query<AgentQuery>,
) -> ApiResult {
    let agent = mandate_crypto::parse_address(&agent).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAMETER",
            "agent must be an address",
        )
    })?;
    let now = (state.clock)();
    let leaves = state
        .backend
        .mandates_for_subject(&agent, None, None, q.active.then_some(now))
        .await
        .map_err(unavailable)?;
    let snap = snapshot::for_leaves(&state.backend, leaves.clone())
        .await
        .map_err(unavailable)?;
    let mut seen = std::collections::BTreeSet::new();
    let mandates: Vec<Value> = leaves
        .into_iter()
        .map(|s| s.doc)
        // Only issuer-published copies made it into the snapshot.
        .filter(|m| snap.mandates.contains_key(&m.id))
        .filter(|m| m.subject == agent && seen.insert((m.id.clone(), m.signature.clone())))
        .map(|m| {
            let status = verify_chain(
                m.clone(),
                &snap.store,
                &EthereumVerifier,
                &state.anchors,
                now,
            );
            mandate_status_json(&m, status.as_ref().map(|_| ()), snap.mandates.get(&m.id))
        })
        .collect();
    Ok((
        StatusCode::OK,
        Json(json!({ "agent": agent, "mandates": mandates })),
    ))
}

async fn publish_mandate<B: Backend>(
    backend: &B,
    m: &Mandate,
) -> Result<crate::backend::Published, ApiError> {
    let published = backend.publish_mandate(m).await.map_err(|e| {
        warn!(mandate = %m.id, error = %e, "Arkiv write failed");
        unavailable(e)
    })?;
    info!(mandate = %m.id, entity = ?published.entity_key, "Arkiv write succeeded");
    Ok(published)
}

fn status_for(created: bool) -> StatusCode {
    if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    }
}

/// Relay the adapter's live Arkiv events (Server-Sent Events) to the browser.
async fn events<B: Backend>(State(state): Shared<B>) -> Result<axum::response::Response, ApiError> {
    let url = state.events_url.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "live events need the Arkiv backend",
        )
    })?;
    let upstream = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| unavailable(BackendError::Unavailable(format!("event stream: {e}"))))?;
    axum::response::Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(axum::body::Body::from_stream(upstream.bytes_stream()))
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))
}

fn demo_of<B>(state: &AppState<B>) -> Result<&Demo, ApiError> {
    state.demo.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "DEMO_DISABLED",
            "demo mode is not enabled",
        )
    })
}

fn demo_error(e: DemoError) -> ApiError {
    match e {
        DemoError::RateLimited => ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "DEMO_RATE_LIMITED",
            "the demo's hourly write budget is used up; try again later",
        ),
        DemoError::NoRootMandate => ApiError::new(
            StatusCode::CONFLICT,
            "NO_ROOT_MANDATE",
            "FinanceAgent has no valid mandate from Alice to delegate from",
        ),
        DemoError::NotASandboxMandate => ApiError::new(
            StatusCode::FORBIDDEN,
            "NOT_A_SANDBOX_MANDATE",
            "only sandbox mandates created by this demo can be revoked here",
        ),
        DemoError::Signing(m) => {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "SIGNING_FAILED", m)
        }
        DemoError::Backend(e) => unavailable(e),
    }
}

async fn demo_info<B: Backend>(State(state): Shared<B>) -> ApiResult {
    let demo = demo_of(&state)?;
    let named = |list: Vec<(String, mandate_core::PrincipalId)>| -> Vec<Value> {
        list.into_iter()
            .map(|(name, address)| json!({ "name": name, "address": address }))
            .collect()
    };
    Ok((
        StatusCode::OK,
        Json(json!({
            "resource": demo.resource,
            "principals": named(demo.principals.clone()),
            "sandboxAgents": named(demo.sandbox_agents()),
            "backend": state.backend.name(),
            "liveEvents": state.events_url.is_some(),
            "explorer": "https://tiramisu.explorer.arkiv.network",
            "sandbox": {
                "maxAmount": crate::demo::SANDBOX_MAX_AMOUNT,
                "lifetimeSeconds": crate::demo::SANDBOX_LIFETIME,
            },
        })),
    ))
}

async fn demo_sandbox<B: Backend>(State(state): Shared<B>) -> ApiResult {
    let demo = demo_of(&state)?;
    let (name, mandate, published) = demo
        .create_sandbox(&state.backend, &state.anchors, (state.clock)())
        .await
        .map_err(demo_error)?;
    info!(mandate = %mandate.id, agent = %mandate.subject, "sandbox mandate delegated");
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "agent": { "name": name, "address": mandate.subject },
            "mandate": mandate,
            "published": published,
        })),
    ))
}

async fn demo_revoke<B: Backend>(State(state): Shared<B>, Path(id): Path<String>) -> ApiResult {
    let demo = demo_of(&state)?;
    let id = parse_id(&id)?;
    let (mandate, _) = load(&state, &id).await?;
    let (revocation, published) = demo
        .revoke_sandbox(&state.backend, &mandate, (state.clock)())
        .await
        .map_err(demo_error)?;
    info!(mandate = %id, revocation = %revocation.id, "sandbox mandate revoked");
    Ok((
        StatusCode::CREATED,
        Json(json!({ "revocation": revocation, "published": published })),
    ))
}
