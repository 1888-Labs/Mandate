//! JSON shapes of the HTTP API. Every code is stable and machine-readable;
//! every denial also carries a human-readable `message`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use mandate_core::{
    AuthorizationDecision, ChainFault, Denial, DenyReason, Mandate, PrincipalId, ResourceId,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// `POST /v1/authorize`. The server supplies the time. A client-chosen
/// timestamp would let an agent backdate requests past expiry, so unknown
/// fields (including `timestamp`) are rejected.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizeBody {
    pub subject: PrincipalId,
    pub action: String,
    pub resource: ResourceId,
    #[serde(default)]
    pub amount: Option<u64>,
    #[serde(default)]
    pub recipient: Option<String>,
}

/// An API error: `{ "error": CODE, "message": … }` plus optional detail.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub detail: Option<Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with(mut self, detail: Value) -> Self {
        self.detail = Some(detail);
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = Map::new();
        body.insert("error".into(), self.code.into());
        body.insert("message".into(), self.message.into());
        if let Some(Value::Object(extra)) = self.detail {
            body.extend(extra);
        }
        (self.status, Json(Value::Object(body))).into_response()
    }
}

pub fn decision_json(decision: &AuthorizationDecision) -> Value {
    match decision {
        AuthorizationDecision::Allow(e) => json!({
            "decision": "ALLOW",
            "mandateId": e.mandate_id,
            "rootPrincipal": e.root_principal,
            "delegationPath": e.delegation_path,
            "chain": e.chain,
            "effectiveConstraints": e.effective_constraints,
        }),
        AuthorizationDecision::Deny(d) => {
            let mut v = denial_json(d);
            if let Value::Object(map) = &mut v {
                map.insert("decision".into(), "DENY".into());
            }
            v
        }
    }
}

/// `{ reason, message, mandateId?, delegationPath, …details }`
pub fn denial_json(d: &Denial) -> Value {
    let mut map = Map::new();
    map.insert("reason".into(), d.reason.code().into());
    map.insert("message".into(), describe(&d.reason).into());
    if let Some(id) = &d.mandate_id {
        map.insert("mandateId".into(), json!(id));
    }
    map.insert("delegationPath".into(), json!(d.delegation_path));
    let details = match &d.reason {
        DenyReason::NoMandate | DenyReason::ActionNotAllowed | DenyReason::ResourceNotAllowed => {
            json!({})
        }
        DenyReason::InvalidSignature { mandate_id } => json!({ "invalidMandateId": mandate_id }),
        DenyReason::InvalidDelegation { mandate_id, fault } => {
            let mut v = json!({ "invalidMandateId": mandate_id, "fault": fault_code(fault) });
            if let (ChainFault::MissingParent(p), Value::Object(m)) = (fault, &mut v) {
                m.insert("missingParentId".into(), json!(p));
            }
            v
        }
        DenyReason::UntrustedRoot { mandate_id, issuer } => {
            json!({ "rootMandateId": mandate_id, "rootIssuer": issuer })
        }
        DenyReason::Expired {
            mandate_id,
            expires_at,
        } => json!({ "expiredMandateId": mandate_id, "expiresAt": expires_at }),
        DenyReason::Revoked {
            mandate_id,
            revocation_id,
        } => json!({ "revokedMandateId": mandate_id, "revocationId": revocation_id }),
        DenyReason::AncestorRevoked {
            revoked,
            revocation_id,
        } => json!({ "revokedMandateId": revoked, "revocationId": revocation_id }),
        DenyReason::AmountExceeded { requested, maximum } => {
            json!({ "requestedAmount": requested, "maximumAmount": maximum })
        }
        DenyReason::RecipientNotAllowed { requested, allowed } => {
            json!({ "requestedRecipient": requested, "allowedRecipients": allowed })
        }
    };
    if let Value::Object(extra) = details {
        map.extend(extra);
    }
    Value::Object(map)
}

fn fault_code(fault: &ChainFault) -> &'static str {
    match fault {
        ChainFault::Delegation(e) => e.code(),
        ChainFault::MissingParent(_) => "MISSING_PARENT",
        ChainFault::TooDeep => "TOO_DEEP",
    }
}

fn describe(reason: &DenyReason) -> String {
    match reason {
        DenyReason::NoMandate => "The agent holds no mandate for this action and resource.".into(),
        DenyReason::InvalidSignature { .. } => {
            "A mandate in the chain has an invalid signature.".into()
        }
        DenyReason::InvalidDelegation { fault, .. } => match fault {
            ChainFault::Delegation(e) => {
                format!("A link in the chain is not a valid delegation: {e}.")
            }
            ChainFault::MissingParent(_) => {
                "A parent mandate in the chain could not be found.".into()
            }
            ChainFault::TooDeep => "The delegation chain is too long.".into(),
        },
        DenyReason::UntrustedRoot { .. } => {
            "The chain does not start with a trusted owner of the resource.".into()
        }
        DenyReason::Expired { .. } => "A mandate in the chain has expired.".into(),
        DenyReason::Revoked { .. } => "The agent's mandate has been revoked.".into(),
        DenyReason::AncestorRevoked { .. } => {
            "A mandate above the agent's in the chain has been revoked.".into()
        }
        DenyReason::ActionNotAllowed => "The action is not allowed.".into(),
        DenyReason::ResourceNotAllowed => "The resource is not allowed.".into(),
        DenyReason::AmountExceeded {
            requested: Some(r),
            maximum,
        } => format!("Requested {r} exceeds the delegated maximum of {maximum}."),
        DenyReason::AmountExceeded { maximum, .. } => {
            format!("No amount given, but a maximum of {maximum} applies.")
        }
        DenyReason::RecipientNotAllowed {
            requested: Some(r), ..
        } => format!("Recipient {r} is not allowed."),
        DenyReason::RecipientNotAllowed { .. } => {
            "No recipient given, but recipients are restricted.".into()
        }
    }
}

/// A mandate plus its current status (`ACTIVE` or a deny code).
pub fn mandate_status_json(m: &Mandate, status: Result<(), &Denial>) -> Value {
    let mut v = json!({ "mandate": m });
    if let Value::Object(map) = &mut v {
        match status {
            Ok(()) => {
                map.insert("status".into(), "ACTIVE".into());
            }
            Err(d) => {
                map.insert("status".into(), d.reason.code().into());
                map.insert("denial".into(), denial_json(d));
            }
        }
    }
    v
}
