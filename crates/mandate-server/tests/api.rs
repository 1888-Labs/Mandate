//! The HTTP API end to end, with real signatures and an in-memory backend.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use mandate_core::{
    Constraints, Mandate, MandateId, PrincipalId, ResourceId, Revocation, TrustAnchors,
    UnsignedMandate, UnsignedRevocation,
};
use mandate_crypto::{address_of, generate_signing_key, sign_mandate, sign_revocation, SigningKey};
use mandate_server::backend::{Backend, BackendError, MemoryBackend, Published};
use mandate_server::{router, AppState, Clock};
use serde_json::{json, Value};
use tower::ServiceExt;

const NOW: u64 = 1_800_000_000;
const HOUR: u64 = 3600;

struct Demo {
    app: Router,
    clock: Arc<AtomicU64>,
    alice: SigningKey,
    finance: SigningKey,
    invoice: SigningKey,
}

fn treasury() -> ResourceId {
    "wallet:treasury-demo".parse().unwrap()
}

fn demo_with<B: Backend>(backend: B) -> Demo {
    let alice = generate_signing_key();
    let clock = Arc::new(AtomicU64::new(NOW));
    let c = clock.clone();
    let clock_fn: Clock = Arc::new(move || c.load(Ordering::SeqCst));
    let app = router(AppState {
        backend,
        anchors: TrustAnchors::new().trust(treasury(), address_of(&alice)),
        clock: clock_fn,
    });
    Demo {
        app,
        clock,
        alice,
        finance: generate_signing_key(),
        invoice: generate_signing_key(),
    }
}

fn demo() -> Demo {
    demo_with(MemoryBackend::new())
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

impl Demo {
    fn m1(&self) -> Mandate {
        sign_mandate(
            &UnsignedMandate {
                issuer: address_of(&self.alice),
                subject: address_of(&self.finance),
                action: "transfer".into(),
                resource: treasury(),
                constraints: Constraints {
                    max_amount: Some(1000),
                    allowed_recipients: Some(vec!["AWS".into()]),
                },
                parent: None,
                delegable: true,
                issued_at: NOW,
                expires_at: NOW + 48 * HOUR,
                nonce: 1_791_852_453_273_973_432, // > 2^53, like the CLI's
            },
            &self.alice,
        )
        .unwrap()
    }

    fn m2_with(&self, m1: &Mandate, max: u64) -> Mandate {
        sign_mandate(
            &UnsignedMandate {
                issuer: address_of(&self.finance),
                subject: address_of(&self.invoice),
                constraints: Constraints {
                    max_amount: Some(max),
                    allowed_recipients: Some(vec!["AWS".into()]),
                },
                parent: Some(m1.id.clone()),
                delegable: false,
                expires_at: NOW + 24 * HOUR,
                nonce: 2,
                ..m1.unsigned()
            },
            &self.finance,
        )
        .unwrap()
    }

    fn revocation(&self, of: &Mandate, key: &SigningKey) -> Revocation {
        sign_revocation(
            &UnsignedRevocation {
                mandate_id: of.id.clone(),
                issuer: address_of(key),
                revoked_at: NOW + HOUR,
                reason: Some("user_revoked".into()),
            },
            key,
        )
        .unwrap()
    }

    async fn ask(&self, amount: u64, recipient: &str) -> Value {
        let (status, body) = call(
            &self.app,
            "POST",
            "/v1/authorize",
            Some(json!({
                "subject": address_of(&self.invoice),
                "action": "transfer",
                "resource": { "kind": "wallet", "id": "treasury-demo" },
                "amount": amount,
                "recipient": recipient,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    /// Publish M1 and M2 through the API.
    async fn setup(&self) -> (Mandate, Mandate) {
        let m1 = self.m1();
        let m2 = self.m2_with(&m1, 300);
        let (s, b) = call(&self.app, "POST", "/v1/mandates", Some(json!(m1))).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        let uri = format!("/v1/mandates/{}/delegate", m1.id);
        let (s, b) = call(&self.app, "POST", &uri, Some(json!(m2))).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        (m1, m2)
    }
}

#[tokio::test]
async fn full_demo_flow() {
    let d = demo();
    let (m1, m2) = d.setup().await;
    let path = json!([
        address_of(&d.alice),
        address_of(&d.finance),
        address_of(&d.invoice)
    ]);

    // 3. $200 → AWS
    let r = d.ask(200, "AWS").await;
    assert_eq!(r["decision"], "ALLOW", "{r}");
    assert_eq!(r["mandateId"], json!(m2.id));
    assert_eq!(r["rootPrincipal"], json!(address_of(&d.alice)));
    assert_eq!(r["delegationPath"], path);
    assert_eq!(
        r["effectiveConstraints"],
        json!({ "maxAmount": 300, "allowedRecipients": ["AWS"] })
    );

    // 4. $700 → AWS
    let r = d.ask(700, "AWS").await;
    assert_eq!(r["decision"], "DENY");
    assert_eq!(r["reason"], "AMOUNT_EXCEEDED");
    assert_eq!(r["requestedAmount"], 700);
    assert_eq!(r["maximumAmount"], 300);

    // 5. $100 → RandomVendor
    let r = d.ask(100, "RandomVendor").await;
    assert_eq!(r["reason"], "RECIPIENT_NOT_ALLOWED");
    assert_eq!(r["allowedRecipients"], json!(["AWS"]));

    // 6. Alice revokes M1
    let rev = d.revocation(&m1, &d.alice);
    let uri = format!("/v1/mandates/{}/revoke", m1.id);
    let (s, b) = call(&d.app, "POST", &uri, Some(json!(rev))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");

    // 7. $100 → AWS: ANCESTOR_REVOKED, naming M1 and the path
    let r = d.ask(100, "AWS").await;
    assert_eq!(r["reason"], "ANCESTOR_REVOKED", "{r}");
    assert_eq!(r["revokedMandateId"], json!(m1.id));
    assert_eq!(r["revocationId"], json!(rev.id));
    assert_eq!(r["delegationPath"], path);

    // 8. The chain explains it
    let (s, chain) = call(
        &d.app,
        "GET",
        &format!("/v1/mandates/{}/chain", m2.id),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(chain["valid"], false);
    assert_eq!(chain["links"][0]["id"], json!(m1.id));
    assert_eq!(chain["links"][1]["id"], json!(m2.id));
    assert_eq!(chain["denial"]["reason"], "ANCESTOR_REVOKED");
}

#[tokio::test]
async fn large_nonce_survives_the_api() {
    let d = demo();
    let (m1, _) = d.setup().await;
    let (_, got) = call(&d.app, "GET", &format!("/v1/mandates/{}", m1.id), None).await;
    assert_eq!(got["mandate"]["nonce"], json!(1_791_852_453_273_973_432u64));
    assert_eq!(got["status"], "ACTIVE");
}

#[tokio::test]
async fn inspection_endpoints() {
    let d = demo();
    let (m1, m2) = d.setup().await;

    let (s, chain) = call(
        &d.app,
        "GET",
        &format!("/v1/mandates/{}/chain", m2.id),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(chain["valid"], true);
    assert_eq!(chain["effectiveConstraints"]["maxAmount"], 300);

    let uri = format!("/v1/agents/{}/mandates", address_of(&d.invoice));
    let (s, list) = call(&d.app, "GET", &uri, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list["mandates"].as_array().unwrap().len(), 1);
    assert_eq!(list["mandates"][0]["mandate"]["id"], json!(m2.id));
    assert_eq!(list["mandates"][0]["status"], "ACTIVE");

    let (s, _) = call(
        &d.app,
        "GET",
        &format!("/v1/mandates/0x{}", "ab".repeat(32)),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(&d.app, "GET", "/v1/mandates/nope", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, health) = call(&d.app, "GET", "/health", None).await;
    assert_eq!(health["backend"], "memory");
    let _ = m1;
}

#[tokio::test]
async fn invalid_documents_are_never_published() {
    let d = demo();
    let m1 = d.m1();

    // Forged: content changed after signing.
    let mut forged = m1.clone();
    forged.constraints.max_amount = Some(1_000_000);
    let (s, b) = call(&d.app, "POST", "/v1/mandates", Some(json!(forged))).await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("INVALID_SIGNATURE"))
    );

    // Self-issued root by a non-owner.
    let selfie = sign_mandate(
        &UnsignedMandate {
            issuer: address_of(&d.invoice),
            subject: address_of(&d.invoice),
            ..m1.unsigned()
        },
        &d.invoice,
    )
    .unwrap();
    let (s, b) = call(&d.app, "POST", "/v1/mandates", Some(json!(selfie))).await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("UNTRUSTED_ROOT"))
    );

    // A child sent to the root endpoint.
    let (s, b) = call(
        &d.app,
        "POST",
        "/v1/mandates",
        Some(json!(d.m2_with(&m1, 1))),
    )
    .await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("NOT_A_ROOT_MANDATE"))
    );

    // An expired root.
    d.clock.store(m1.expires_at, Ordering::SeqCst);
    let (s, b) = call(&d.app, "POST", "/v1/mandates", Some(json!(m1))).await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("EXPIRED"))
    );

    // Client-chosen timestamps are refused.
    let (s, b) = call(
        &d.app,
        "POST",
        "/v1/authorize",
        Some(json!({
            "subject": address_of(&d.invoice), "action": "transfer",
            "resource": { "kind": "wallet", "id": "treasury-demo" }, "timestamp": 0,
        })),
    )
    .await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("INVALID_REQUEST"))
    );
}

#[tokio::test]
async fn escalating_delegation_is_rejected() {
    let d = demo();
    let m1 = d.m1();
    call(&d.app, "POST", "/v1/mandates", Some(json!(m1))).await;
    let uri = format!("/v1/mandates/{}/delegate", m1.id);
    let (s, b) = call(&d.app, "POST", &uri, Some(json!(d.m2_with(&m1, 5000)))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    assert_eq!(b["error"], "DELEGATION_REJECTED");
    assert_eq!(b["reason"], "AMOUNT_ESCALATION");
    // Nothing was stored: InvoiceAgent has no mandate.
    assert_eq!(d.ask(1, "AWS").await["reason"], "NO_MANDATE");

    // Parent in URL must match the document.
    let other = format!("/v1/mandates/0x{}/delegate", "ab".repeat(32));
    let (s, b) = call(&d.app, "POST", &other, Some(json!(d.m2_with(&m1, 1)))).await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("PARENT_MISMATCH"))
    );
}

#[tokio::test]
async fn cannot_delegate_from_revoked_parent() {
    let d = demo();
    let m1 = d.m1();
    call(&d.app, "POST", "/v1/mandates", Some(json!(m1))).await;
    let rev = d.revocation(&m1, &d.alice);
    call(
        &d.app,
        "POST",
        &format!("/v1/mandates/{}/revoke", m1.id),
        Some(json!(rev)),
    )
    .await;
    let uri = format!("/v1/mandates/{}/delegate", m1.id);
    let (s, b) = call(&d.app, "POST", &uri, Some(json!(d.m2_with(&m1, 10)))).await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["error"], "PARENT_UNUSABLE");
    assert_eq!(b["parentDenial"]["reason"], "REVOKED");
}

#[tokio::test]
async fn only_the_issuer_can_revoke() {
    let d = demo();
    let (m1, _) = d.setup().await;
    let uri = format!("/v1/mandates/{}/revoke", m1.id);

    // FinanceAgent signs a revocation of Alice's M1.
    let (s, b) = call(
        &d.app,
        "POST",
        &uri,
        Some(json!(d.revocation(&m1, &d.finance))),
    )
    .await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::FORBIDDEN, Some("NOT_MANDATE_ISSUER"))
    );

    // A tampered revocation.
    let mut bad = d.revocation(&m1, &d.alice);
    bad.reason = Some("edited".into());
    let (s, b) = call(&d.app, "POST", &uri, Some(json!(bad))).await;
    assert_eq!(
        (s, b["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("INVALID_SIGNATURE"))
    );

    assert_eq!(d.ask(200, "AWS").await["decision"], "ALLOW");
}

#[tokio::test]
async fn expiry_uses_the_server_clock() {
    let d = demo();
    let (_, m2) = d.setup().await;
    d.clock.store(m2.expires_at, Ordering::SeqCst);
    let r = d.ask(200, "AWS").await;
    assert_eq!(r["reason"], "EXPIRED");
    assert_eq!(r["expiredMandateId"], json!(m2.id));
}

/// A backend that can't be reached.
struct DownBackend;

impl Backend for DownBackend {
    fn name(&self) -> &'static str {
        "down"
    }
    async fn mandates_by_ids(&self, _: &[MandateId]) -> Result<Vec<Mandate>, BackendError> {
        Err(BackendError("connection refused".into()))
    }
    async fn mandates_for_subject(
        &self,
        _: &PrincipalId,
        _: Option<&str>,
        _: Option<&ResourceId>,
    ) -> Result<Vec<Mandate>, BackendError> {
        Err(BackendError("connection refused".into()))
    }
    async fn revocations_for(&self, _: &[MandateId]) -> Result<Vec<Revocation>, BackendError> {
        Err(BackendError("connection refused".into()))
    }
    async fn publish_mandate(&self, _: &Mandate) -> Result<Published, BackendError> {
        Err(BackendError("connection refused".into()))
    }
    async fn publish_revocation(&self, _: &Revocation, _: u64) -> Result<Published, BackendError> {
        Err(BackendError("connection refused".into()))
    }
}

#[tokio::test]
async fn unreachable_state_fails_closed() {
    let d = demo_with(DownBackend);
    let (s, b) = call(
        &d.app,
        "POST",
        "/v1/authorize",
        Some(json!({
            "subject": address_of(&d.invoice), "action": "transfer",
            "resource": { "kind": "wallet", "id": "treasury-demo" }, "amount": 1, "recipient": "AWS",
        })),
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(b["error"], "STATE_UNAVAILABLE");
    assert!(
        b.get("decision").is_none(),
        "must never return a decision without state"
    );
}

/// A backend whose revocation lookups fail after mandates load: a partial
/// snapshot must not turn into an ALLOW.
struct NoRevocations(MemoryBackend);

impl Backend for NoRevocations {
    fn name(&self) -> &'static str {
        "flaky"
    }
    async fn mandates_by_ids(&self, ids: &[MandateId]) -> Result<Vec<Mandate>, BackendError> {
        self.0.mandates_by_ids(ids).await
    }
    async fn mandates_for_subject(
        &self,
        s: &PrincipalId,
        a: Option<&str>,
        r: Option<&ResourceId>,
    ) -> Result<Vec<Mandate>, BackendError> {
        self.0.mandates_for_subject(s, a, r).await
    }
    async fn revocations_for(&self, _: &[MandateId]) -> Result<Vec<Revocation>, BackendError> {
        Err(BackendError("timeout".into()))
    }
    async fn publish_mandate(&self, m: &Mandate) -> Result<Published, BackendError> {
        self.0.publish_mandate(m).await
    }
    async fn publish_revocation(&self, r: &Revocation, t: u64) -> Result<Published, BackendError> {
        self.0.publish_revocation(r, t).await
    }
}

#[tokio::test]
async fn failed_revocation_lookup_fails_closed() {
    let d = demo_with(NoRevocations(MemoryBackend::new()));
    let m1 = d.m1();
    // Publishing a root needs no revocation lookup.
    let (s, _) = call(&d.app, "POST", "/v1/mandates", Some(json!(m1))).await;
    assert_eq!(s, StatusCode::CREATED);
    let r = call(
        &d.app,
        "POST",
        "/v1/authorize",
        Some(json!({
            "subject": address_of(&d.finance), "action": "transfer",
            "resource": { "kind": "wallet", "id": "treasury-demo" }, "amount": 1, "recipient": "AWS",
        })),
    )
    .await;
    assert_eq!(r.0, StatusCode::SERVICE_UNAVAILABLE, "{}", r.1);
}
