//! HTTP handlers. The server holds no private keys: clients submit
//! documents they signed themselves. The server verifies them before
//! publishing, so invalid ones never reach Arkiv or cost gas.

use std::sync::Arc;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mandate_core::{
    authorize, can_delegate, find_mandate, verify_chain, AuthorizationRequest, Mandate, MandateId,
    Revocation, TrustAnchors,
};
use mandate_crypto::{verify_mandate_signature, verify_revocation_signature, EthereumVerifier};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::api::{decision_json, denial_json, mandate_status_json, ApiError, AuthorizeBody};
use crate::backend::{Backend, BackendError};
use crate::snapshot;

pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct AppState<B> {
    pub backend: B,
    pub anchors: TrustAnchors,
    pub clock: Clock,
}

type Shared<B> = State<Arc<AppState<B>>>;
type ApiResult = Result<(StatusCode, Json<Value>), ApiError>;

pub fn router<B: Backend>(state: AppState<B>) -> Router {
    Router::new()
        .route("/health", get(health::<B>))
        .route("/v1/authorize", post(authorize_handler::<B>))
        .route("/v1/mandates", post(create_mandate::<B>))
        .route("/v1/mandates/{id}", get(get_mandate::<B>))
        .route("/v1/mandates/{id}/chain", get(get_chain::<B>))
        .route("/v1/mandates/{id}/delegate", post(delegate::<B>))
        .route("/v1/mandates/{id}/revoke", post(revoke::<B>))
        .route("/v1/agents/{id}/mandates", get(agent_mandates::<B>))
        .with_state(Arc::new(state))
}

fn unavailable(e: BackendError) -> ApiError {
    warn!(error = %e, "authorization state unavailable");
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "STATE_UNAVAILABLE",
        format!("authorization state could not be read: {e}"),
    )
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
) -> Result<(Mandate, mandate_core::MemoryMandateStore), ApiError> {
    let store = snapshot::for_mandate(&state.backend, id)
        .await
        .map_err(unavailable)?;
    let mandate = find_mandate(&store, &EthereumVerifier, id)
        .filter(|m| verify_mandate_signature(m).is_ok())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "MANDATE_NOT_FOUND",
                format!("no valid mandate {id}"),
            )
        })?;
    Ok((mandate, store))
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
    let store = snapshot::for_request(&state.backend, &request)
        .await
        .map_err(unavailable)?;
    let decision = authorize(&request, &store, &EthereumVerifier, &state.anchors);
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
    let (parent, store) = load(&state, &parent_id).await?;
    if let Err(d) = verify_chain(
        parent.clone(),
        &store,
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
    let (mandate, store) = load(&state, &id).await?;
    let status = verify_chain(
        mandate.clone(),
        &store,
        &EthereumVerifier,
        &state.anchors,
        (state.clock)(),
    );
    Ok((
        StatusCode::OK,
        Json(mandate_status_json(&mandate, status.as_ref().map(|_| ()))),
    ))
}

async fn get_chain<B: Backend>(State(state): Shared<B>, Path(id): Path<String>) -> ApiResult {
    let id = parse_id(&id)?;
    let (mandate, store) = load(&state, &id).await?;
    // Every link we can resolve, root first, whether or not it is valid.
    let mut links = vec![mandate.clone()];
    while let Some(parent) = links
        .last()
        .and_then(|m| m.parent.as_ref())
        .and_then(|p| find_mandate(&store, &EthereumVerifier, p))
    {
        if links.len() > mandate_core::authorization::MAX_CHAIN_DEPTH {
            break;
        }
        links.push(parent);
    }
    links.reverse();
    let status = verify_chain(
        mandate,
        &store,
        &EthereumVerifier,
        &state.anchors,
        (state.clock)(),
    );
    let mut body = json!({
        "mandateId": id,
        "valid": status.is_ok(),
        "links": links,
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

async fn agent_mandates<B: Backend>(
    State(state): Shared<B>,
    Path(agent): Path<String>,
) -> ApiResult {
    let agent = mandate_crypto::parse_address(&agent).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAMETER",
            "agent must be an address",
        )
    })?;
    let leaves = state
        .backend
        .mandates_for_subject(&agent, None, None)
        .await
        .map_err(unavailable)?;
    let store = snapshot::for_leaves(&state.backend, leaves.clone())
        .await
        .map_err(unavailable)?;
    let now = (state.clock)();
    let mut seen = std::collections::BTreeSet::new();
    let mandates: Vec<Value> = leaves
        .into_iter()
        .filter(|m| m.subject == agent && seen.insert((m.id.clone(), m.signature.clone())))
        .map(|m| {
            let status = verify_chain(m.clone(), &store, &EthereumVerifier, &state.anchors, now);
            mandate_status_json(&m, status.as_ref().map(|_| ()))
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
