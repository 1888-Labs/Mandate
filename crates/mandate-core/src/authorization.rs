//! The authorization engine: may this subject perform this action now?
//!
//! For every candidate mandate held by the subject, the engine resolves
//! the full chain up to its root and checks, for every link:
//!
//! 1. the signature and ID are valid,
//! 2. each edge is a valid delegation (`can_delegate`),
//! 3. the root was issued by a trusted owner of the resource,
//! 4. no mandate in the chain has expired,
//! 5. no mandate in the chain has been revoked by its issuer,
//!
//! and then evaluates the request against the chain's effective
//! constraints. Every outcome carries an explanation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::capability::{Constraints, ResourceId};
use crate::delegation::can_delegate;
use crate::error::DelegationError;
use crate::mandate::{Mandate, MandateId};
use crate::principal::PrincipalId;
use crate::revocation::{Revocation, RevocationId};
use crate::store::{MandateStore, SignatureVerifier};

/// Longest chain (in delegation hops) the engine will follow.
pub const MAX_CHAIN_DEPTH: usize = 16;

/// An agent asking to perform an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRequest {
    pub subject: PrincipalId,
    pub action: String,
    pub resource: ResourceId,
    pub amount: Option<u64>,
    pub recipient: Option<String>,
    /// Unix seconds at which the action would happen.
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationDecision {
    Allow(AuthorizationEvidence),
    Deny(Denial),
}

impl AuthorizationDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow(_))
    }
}

/// Why a request was allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationEvidence {
    /// The subject's own mandate that authorized the request.
    pub mandate_id: MandateId,
    /// Issuer of the root mandate: the ultimate source of authority.
    pub root_principal: PrincipalId,
    /// Root principal, then every subject down to the requester.
    pub delegation_path: Vec<PrincipalId>,
    /// Mandate IDs from root to leaf.
    pub chain: Vec<MandateId>,
    /// The tightest limits along the chain.
    pub effective_constraints: Constraints,
}

/// Why a request was denied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    pub reason: DenyReason,
    /// The subject's mandate that came closest to authorizing the request.
    pub mandate_id: Option<MandateId>,
    /// As much of the delegation path as could be resolved, top first.
    pub delegation_path: Vec<PrincipalId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// The subject holds no mandate for this action and resource.
    NoMandate,
    /// A mandate in the chain has a bad signature or a forged ID.
    InvalidSignature {
        mandate_id: MandateId,
    },
    /// The chain is broken or a link escalates authority.
    InvalidDelegation {
        mandate_id: MandateId,
        fault: ChainFault,
    },
    /// The chain's root was not issued by a trusted owner of the resource.
    UntrustedRoot {
        mandate_id: MandateId,
        issuer: PrincipalId,
    },
    /// A mandate in the chain has expired.
    Expired {
        mandate_id: MandateId,
        expires_at: u64,
    },
    /// The subject's own mandate was revoked.
    Revoked {
        mandate_id: MandateId,
        revocation_id: RevocationId,
    },
    /// A mandate above the subject's was revoked. `revoked` is the
    /// highest revoked mandate in the chain.
    AncestorRevoked {
        revoked: MandateId,
        revocation_id: RevocationId,
    },
    ActionNotAllowed,
    ResourceNotAllowed,
    /// Over the effective limit, or no amount given where a limit applies.
    AmountExceeded {
        requested: Option<u64>,
        maximum: u64,
    },
    /// Recipient not allowed, or none given where recipients are restricted.
    RecipientNotAllowed {
        requested: Option<String>,
        allowed: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainFault {
    /// An edge fails `can_delegate`.
    Delegation(DelegationError),
    /// A referenced parent mandate could not be found.
    MissingParent(MandateId),
    /// The chain is longer than `MAX_CHAIN_DEPTH`.
    TooDeep,
}

impl DenyReason {
    /// Stable machine-readable code for APIs and logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoMandate => "NO_MANDATE",
            Self::InvalidSignature { .. } => "INVALID_SIGNATURE",
            Self::InvalidDelegation { .. } => "INVALID_DELEGATION",
            Self::UntrustedRoot { .. } => "UNTRUSTED_ROOT",
            Self::Expired { .. } => "EXPIRED",
            Self::Revoked { .. } => "REVOKED",
            Self::AncestorRevoked { .. } => "ANCESTOR_REVOKED",
            Self::ActionNotAllowed => "ACTION_NOT_ALLOWED",
            Self::ResourceNotAllowed => "RESOURCE_NOT_ALLOWED",
            Self::AmountExceeded { .. } => "AMOUNT_EXCEEDED",
            Self::RecipientNotAllowed { .. } => "RECIPIENT_NOT_ALLOWED",
        }
    }

    /// How close a candidate got to authorizing the request. When every
    /// candidate is denied, the closest one explains the denial, so a
    /// planted forgery cannot hide the real reason.
    fn progress(&self) -> u8 {
        match self {
            Self::NoMandate => 0,
            Self::ActionNotAllowed | Self::ResourceNotAllowed => 1,
            Self::InvalidSignature { .. } => 2,
            Self::InvalidDelegation { .. } => 3,
            Self::UntrustedRoot { .. } => 4,
            Self::Expired { .. } => 5,
            Self::Revoked { .. } | Self::AncestorRevoked { .. } => 6,
            Self::AmountExceeded { .. } | Self::RecipientNotAllowed { .. } => 7,
        }
    }
}

impl fmt::Display for DenyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Who may issue root mandates for each resource.
///
/// Without this, any principal could sign a root mandate to itself and
/// grant itself unlimited authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustAnchors {
    owners: BTreeMap<ResourceId, BTreeSet<PrincipalId>>,
}

impl TrustAnchors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Trust `owner` to issue root mandates for `resource`.
    pub fn trust(mut self, resource: ResourceId, owner: PrincipalId) -> Self {
        self.owners.entry(resource).or_default().insert(owner);
        self
    }

    pub fn is_trusted(&self, resource: &ResourceId, issuer: &PrincipalId) -> bool {
        self.owners
            .get(resource)
            .is_some_and(|owners| owners.contains(issuer))
    }
}

/// Decide whether `request` is authorized.
///
/// Allows if any of the subject's mandates fully authorizes the request.
/// Otherwise denies with the explanation from the candidate that got
/// closest (see `DenyReason::progress`).
pub fn authorize<S, V>(
    request: &AuthorizationRequest,
    store: &S,
    verifier: &V,
    anchors: &TrustAnchors,
) -> AuthorizationDecision
where
    S: MandateStore,
    V: SignatureVerifier,
{
    let mut candidates: Vec<Mandate> = store
        .find_mandates(&request.subject, &request.action, &request.resource)
        .into_iter()
        .filter(|m| m.subject == request.subject)
        .collect();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));

    let mut best: Option<Denial> = None;
    for leaf in candidates {
        match evaluate(request, leaf, store, verifier, anchors) {
            Ok(evidence) => return AuthorizationDecision::Allow(evidence),
            Err(denial) => {
                let closer = best
                    .as_ref()
                    .is_none_or(|b| denial.reason.progress() > b.reason.progress());
                if closer {
                    best = Some(denial);
                }
            }
        }
    }

    AuthorizationDecision::Deny(best.unwrap_or(Denial {
        reason: DenyReason::NoMandate,
        mandate_id: None,
        delegation_path: vec![request.subject.clone()],
    }))
}

/// Check one candidate mandate and its whole chain against the request.
fn evaluate<S, V>(
    request: &AuthorizationRequest,
    leaf: Mandate,
    store: &S,
    verifier: &V,
    anchors: &TrustAnchors,
) -> Result<AuthorizationEvidence, Denial>
where
    S: MandateStore,
    V: SignatureVerifier,
{
    let leaf_id = leaf.id.clone();
    let deny = |reason: DenyReason, chain_root_first: &[Mandate]| Denial {
        reason,
        mandate_id: Some(leaf_id.clone()),
        delegation_path: delegation_path(chain_root_first),
    };

    if leaf.action != request.action {
        return Err(deny(DenyReason::ActionNotAllowed, &[leaf]));
    }
    if leaf.resource != request.resource {
        return Err(deny(DenyReason::ResourceNotAllowed, &[leaf]));
    }

    // Resolve leaf → root, verifying every mandate as it is fetched.
    let mut chain: Vec<Mandate> = Vec::new();
    let mut current = leaf;
    loop {
        let current_id = current.id.clone();
        let parent_id = current.parent.clone();
        let signature_ok = verifier.verify_mandate(&current);
        chain.push(current);

        let fault = if !signature_ok {
            None
        } else if let Some(parent_id) = parent_id {
            if chain.len() > MAX_CHAIN_DEPTH {
                Some(ChainFault::TooDeep)
            } else {
                match store.get_mandate(&parent_id) {
                    Some(parent) => {
                        current = parent;
                        continue;
                    }
                    None => Some(ChainFault::MissingParent(parent_id)),
                }
            }
        } else {
            break; // reached a root
        };

        chain.reverse();
        let reason = match fault {
            None => DenyReason::InvalidSignature {
                mandate_id: current_id,
            },
            Some(fault) => DenyReason::InvalidDelegation {
                mandate_id: current_id,
                fault,
            },
        };
        return Err(deny(reason, &chain));
    }
    chain.reverse();
    let chain = chain; // root first from here on

    for edge in chain.windows(2) {
        if let Err(e) = can_delegate(&edge[0], &edge[1]) {
            let reason = DenyReason::InvalidDelegation {
                mandate_id: edge[1].id.clone(),
                fault: ChainFault::Delegation(e),
            };
            return Err(deny(reason, &chain));
        }
    }

    let Some(root) = chain.first() else {
        // The loop above always pushes the leaf, so this cannot happen.
        return Err(deny(DenyReason::NoMandate, &chain));
    };
    if !anchors.is_trusted(&root.resource, &root.issuer) {
        let reason = DenyReason::UntrustedRoot {
            mandate_id: root.id.clone(),
            issuer: root.issuer.clone(),
        };
        return Err(deny(reason, &chain));
    }

    if let Some(expired) = chain.iter().find(|m| request.timestamp >= m.expires_at) {
        let reason = DenyReason::Expired {
            mandate_id: expired.id.clone(),
            expires_at: expired.expires_at,
        };
        return Err(deny(reason, &chain));
    }

    for (depth, m) in chain.iter().enumerate() {
        let Some(revocation) = valid_revocation(m, store, verifier) else {
            continue;
        };
        let is_leaf = depth + 1 == chain.len();
        let reason = if is_leaf {
            DenyReason::Revoked {
                mandate_id: m.id.clone(),
                revocation_id: revocation.id,
            }
        } else {
            DenyReason::AncestorRevoked {
                revoked: m.id.clone(),
                revocation_id: revocation.id,
            }
        };
        return Err(deny(reason, &chain));
    }

    let effective = effective_constraints(&chain);

    if let Some(maximum) = effective.max_amount {
        if request.amount.is_none_or(|a| a > maximum) {
            let reason = DenyReason::AmountExceeded {
                requested: request.amount,
                maximum,
            };
            return Err(deny(reason, &chain));
        }
    }

    if let Some(allowed) = &effective.allowed_recipients {
        let ok = request
            .recipient
            .as_ref()
            .is_some_and(|r| allowed.contains(r));
        if !ok {
            let reason = DenyReason::RecipientNotAllowed {
                requested: request.recipient.clone(),
                allowed: allowed.clone(),
            };
            return Err(deny(reason, &chain));
        }
    }

    Ok(AuthorizationEvidence {
        mandate_id: leaf_id,
        root_principal: root.issuer.clone(),
        delegation_path: delegation_path(&chain),
        chain: chain.iter().map(|m| m.id.clone()).collect(),
        effective_constraints: effective,
    })
}

/// The first revocation of `mandate` that actually counts: it targets this
/// mandate, comes from the mandate's own issuer, and is validly signed.
/// Anything else the store returns is ignored.
fn valid_revocation<S, V>(mandate: &Mandate, store: &S, verifier: &V) -> Option<Revocation>
where
    S: MandateStore,
    V: SignatureVerifier,
{
    let mut revocations = store.find_revocations(&mandate.id);
    revocations.sort_by(|a, b| a.id.cmp(&b.id));
    revocations.into_iter().find(|r| {
        r.mandate_id == mandate.id && r.issuer == mandate.issuer && verifier.verify_revocation(r)
    })
}

/// Top issuer, then each subject in order.
fn delegation_path(chain_root_first: &[Mandate]) -> Vec<PrincipalId> {
    let mut path = Vec::with_capacity(chain_root_first.len() + 1);
    if let Some(top) = chain_root_first.first() {
        path.push(top.issuer.clone());
    }
    path.extend(chain_root_first.iter().map(|m| m.subject.clone()));
    path
}

/// The tightest limits along the chain: the lowest amount and the
/// intersection of all recipient sets.
///
/// For a chain that passed `can_delegate` this equals the leaf's own
/// constraints. Computing it over the whole chain keeps the result
/// correct even if that ever stops being true.
fn effective_constraints(chain: &[Mandate]) -> Constraints {
    let mut max_amount: Option<u64> = None;
    let mut recipients: Option<BTreeSet<&str>> = None;
    for m in chain {
        if let Some(limit) = m.constraints.max_amount {
            max_amount = Some(max_amount.map_or(limit, |cur| cur.min(limit)));
        }
        if let Some(list) = &m.constraints.allowed_recipients {
            let set: BTreeSet<&str> = list.iter().map(String::as_str).collect();
            recipients = Some(match recipients {
                None => set,
                Some(cur) => cur.intersection(&set).copied().collect(),
            });
        }
    }
    Constraints {
        max_amount,
        allowed_recipients: recipients.map(|s| s.into_iter().map(String::from).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::{compute_mandate_id, compute_revocation_id};
    use crate::mandate::UnsignedMandate;
    use crate::revocation::UnsignedRevocation;

    // --- Test doubles ---

    /// Accepts a mandate iff its ID is honest and its signature is
    /// `b"signed-by:<issuer>"`. Stands in for real ECDSA in core tests.
    struct FakeVerifier;

    impl SignatureVerifier for FakeVerifier {
        fn verify_mandate(&self, m: &Mandate) -> bool {
            m.id == compute_mandate_id(&m.unsigned())
                && m.signature == format!("signed-by:{}", m.issuer).into_bytes()
        }

        fn verify_revocation(&self, r: &Revocation) -> bool {
            r.id == compute_revocation_id(&r.unsigned())
                && r.signature == format!("signed-by:{}", r.issuer).into_bytes()
        }
    }

    #[derive(Default)]
    struct VecStore(Vec<Mandate>, Vec<Revocation>);

    impl MandateStore for VecStore {
        fn get_mandate(&self, id: &MandateId) -> Option<Mandate> {
            self.0.iter().find(|m| &m.id == id).cloned()
        }

        fn find_mandates(&self, s: &PrincipalId, a: &str, r: &ResourceId) -> Vec<Mandate> {
            self.0
                .iter()
                .filter(|m| &m.subject == s && m.action == a && &m.resource == r)
                .cloned()
                .collect()
        }

        fn find_revocations(&self, id: &MandateId) -> Vec<Revocation> {
            self.1
                .iter()
                .filter(|r| &r.mandate_id == id)
                .cloned()
                .collect()
        }
    }

    fn sign(u: UnsignedMandate) -> Mandate {
        let sig = format!("signed-by:{}", u.issuer).into_bytes();
        u.into_mandate(sig)
    }

    fn forge(u: UnsignedMandate) -> Mandate {
        u.into_mandate(b"signed-by:someone-else".to_vec())
    }

    // --- Scenario: Alice → FinanceAgent ($1000, AWS) → InvoiceAgent ($300, AWS) ---

    const NOW: u64 = 1_760_000_000;
    const HOUR: u64 = 3600;

    fn p(name: &str) -> PrincipalId {
        PrincipalId(name.into())
    }
    fn alice() -> PrincipalId {
        p("alice")
    }
    fn finance() -> PrincipalId {
        p("finance-agent")
    }
    fn invoice() -> PrincipalId {
        p("invoice-agent")
    }
    fn treasury() -> ResourceId {
        "wallet:treasury-demo".parse().unwrap()
    }
    fn aws() -> Option<Vec<String>> {
        Some(vec!["AWS".into()])
    }

    fn anchors() -> TrustAnchors {
        TrustAnchors::new().trust(treasury(), alice())
    }

    fn m1_unsigned() -> UnsignedMandate {
        UnsignedMandate {
            issuer: alice(),
            subject: finance(),
            action: "transfer".into(),
            resource: treasury(),
            constraints: Constraints {
                max_amount: Some(1000),
                allowed_recipients: aws(),
            },
            parent: None,
            delegable: true,
            issued_at: NOW,
            expires_at: NOW + 48 * HOUR,
            nonce: 1,
        }
    }

    fn m2_unsigned(m1: &Mandate) -> UnsignedMandate {
        UnsignedMandate {
            issuer: finance(),
            subject: invoice(),
            constraints: Constraints {
                max_amount: Some(300),
                allowed_recipients: aws(),
            },
            parent: Some(m1.id.clone()),
            delegable: false,
            expires_at: NOW + 24 * HOUR,
            nonce: 2,
            ..m1.unsigned()
        }
    }

    /// (store, m1, m2) for the standard demo chain.
    fn demo() -> (VecStore, Mandate, Mandate) {
        let m1 = sign(m1_unsigned());
        let m2 = sign(m2_unsigned(&m1));
        (VecStore(vec![m1.clone(), m2.clone()], vec![]), m1, m2)
    }

    fn request(amount: u64, recipient: &str) -> AuthorizationRequest {
        AuthorizationRequest {
            subject: invoice(),
            action: "transfer".into(),
            resource: treasury(),
            amount: Some(amount),
            recipient: Some(recipient.into()),
            timestamp: NOW + HOUR,
        }
    }

    fn check(store: &VecStore, req: &AuthorizationRequest) -> AuthorizationDecision {
        authorize(req, store, &FakeVerifier, &anchors())
    }

    fn denial(decision: AuthorizationDecision) -> Denial {
        match decision {
            AuthorizationDecision::Deny(d) => d,
            AuthorizationDecision::Allow(e) => panic!("expected DENY, got ALLOW: {e:?}"),
        }
    }

    // --- Allow ---

    #[test]
    fn valid_chain_authorizes_request() {
        let (store, m1, m2) = demo();
        let AuthorizationDecision::Allow(e) = check(&store, &request(200, "AWS")) else {
            panic!("expected ALLOW");
        };
        assert_eq!(e.mandate_id, m2.id);
        assert_eq!(e.root_principal, alice());
        assert_eq!(e.delegation_path, vec![alice(), finance(), invoice()]);
        assert_eq!(e.chain, vec![m1.id, m2.id]);
        assert_eq!(
            e.effective_constraints,
            Constraints {
                max_amount: Some(300),
                allowed_recipients: aws(),
            }
        );
    }

    #[test]
    fn root_mandate_authorizes_its_own_subject() {
        let (store, _, _) = demo();
        let req = AuthorizationRequest {
            subject: finance(),
            ..request(900, "AWS")
        };
        assert!(check(&store, &req).is_allowed());
    }

    #[test]
    fn amount_at_limit_is_allowed() {
        let (store, _, _) = demo();
        assert!(check(&store, &request(300, "AWS")).is_allowed());
    }

    #[test]
    fn unconstrained_mandate_allows_any_amount_and_recipient() {
        let m = sign(UnsignedMandate {
            subject: invoice(),
            constraints: Constraints::default(),
            ..m1_unsigned()
        });
        let store = VecStore(vec![m], vec![]);
        assert!(check(&store, &request(u64::MAX, "Anyone")).is_allowed());
        let req = AuthorizationRequest {
            amount: None,
            recipient: None,
            ..request(0, "")
        };
        assert!(check(&store, &req).is_allowed());
    }

    #[test]
    fn any_fully_valid_candidate_allows() {
        let (mut store, m1, _) = demo();
        // A second, smaller mandate for the same agent must not block the larger one.
        store.0.push(sign(UnsignedMandate {
            constraints: Constraints {
                max_amount: Some(100),
                allowed_recipients: aws(),
            },
            nonce: 3,
            ..m2_unsigned(&m1)
        }));
        assert!(check(&store, &request(200, "AWS")).is_allowed());
    }

    #[test]
    fn three_level_chain_is_followed() {
        let (mut store, m1, _) = demo();
        let m2 = sign(UnsignedMandate {
            delegable: true,
            ..m2_unsigned(&m1)
        });
        let m3 = sign(UnsignedMandate {
            issuer: invoice(),
            subject: p("payments-bot"),
            constraints: Constraints {
                max_amount: Some(50),
                allowed_recipients: aws(),
            },
            parent: Some(m2.id.clone()),
            delegable: false,
            nonce: 3,
            ..m2.unsigned()
        });
        store.0 = vec![m1.clone(), m2.clone(), m3.clone()];
        let req = AuthorizationRequest {
            subject: p("payments-bot"),
            ..request(50, "AWS")
        };
        let AuthorizationDecision::Allow(e) = check(&store, &req) else {
            panic!("expected ALLOW");
        };
        assert_eq!(
            e.delegation_path,
            vec![alice(), finance(), invoice(), p("payments-bot")]
        );
        assert_eq!(e.chain, vec![m1.id, m2.id, m3.id]);
        assert_eq!(e.effective_constraints.max_amount, Some(50));
    }

    // --- Constraint denials ---

    #[test]
    fn amount_above_limit_is_denied() {
        let (store, _, m2) = demo();
        let d = denial(check(&store, &request(700, "AWS")));
        assert_eq!(
            d.reason,
            DenyReason::AmountExceeded {
                requested: Some(700),
                maximum: 300
            }
        );
        assert_eq!(d.mandate_id, Some(m2.id));
        assert_eq!(d.delegation_path, vec![alice(), finance(), invoice()]);
        assert_eq!(d.reason.code(), "AMOUNT_EXCEEDED");
    }

    #[test]
    fn amount_one_over_limit_is_denied() {
        let (store, _, _) = demo();
        assert!(!check(&store, &request(301, "AWS")).is_allowed());
    }

    #[test]
    fn missing_amount_is_denied_when_limited() {
        let (store, _, _) = demo();
        let req = AuthorizationRequest {
            amount: None,
            ..request(0, "AWS")
        };
        assert_eq!(
            denial(check(&store, &req)).reason,
            DenyReason::AmountExceeded {
                requested: None,
                maximum: 300
            }
        );
    }

    #[test]
    fn invalid_recipient_is_denied() {
        let (store, _, _) = demo();
        let d = denial(check(&store, &request(100, "RandomVendor")));
        assert_eq!(
            d.reason,
            DenyReason::RecipientNotAllowed {
                requested: Some("RandomVendor".into()),
                allowed: vec!["AWS".into()]
            }
        );
        assert_eq!(d.reason.code(), "RECIPIENT_NOT_ALLOWED");
    }

    #[test]
    fn missing_recipient_is_denied_when_restricted() {
        let (store, _, _) = demo();
        let req = AuthorizationRequest {
            recipient: None,
            ..request(100, "")
        };
        assert_eq!(
            denial(check(&store, &req)).reason.code(),
            "RECIPIENT_NOT_ALLOWED"
        );
    }

    // --- Expiry ---

    #[test]
    fn expired_mandate_is_rejected() {
        let (store, _, m2) = demo();
        let mut req = request(200, "AWS");

        req.timestamp = m2.expires_at - 1;
        assert!(check(&store, &req).is_allowed());

        req.timestamp = m2.expires_at;
        assert_eq!(
            denial(check(&store, &req)).reason,
            DenyReason::Expired {
                mandate_id: m2.id.clone(),
                expires_at: m2.expires_at
            }
        );
    }

    #[test]
    fn expired_ancestor_is_reported_first() {
        let m1 = sign(m1_unsigned());
        let m2 = sign(UnsignedMandate {
            expires_at: m1.expires_at,
            ..m2_unsigned(&m1)
        });
        let store = VecStore(vec![m1.clone(), m2], vec![]);
        let req = AuthorizationRequest {
            timestamp: m1.expires_at,
            ..request(200, "AWS")
        };
        assert_eq!(
            denial(check(&store, &req)).reason,
            DenyReason::Expired {
                mandate_id: m1.id.clone(),
                expires_at: m1.expires_at
            }
        );
    }

    // --- Chain integrity ---

    #[test]
    fn no_mandate_is_denied() {
        let (store, _, _) = demo();
        let req = AuthorizationRequest {
            subject: p("stranger"),
            ..request(1, "AWS")
        };
        let d = denial(check(&store, &req));
        assert_eq!(d.reason, DenyReason::NoMandate);
        assert_eq!(d.mandate_id, None);
    }

    #[test]
    fn wrong_action_or_resource_has_no_mandate() {
        let (store, _, _) = demo();
        let req = AuthorizationRequest {
            action: "refund".into(),
            ..request(1, "AWS")
        };
        assert_eq!(denial(check(&store, &req)).reason, DenyReason::NoMandate);

        let req = AuthorizationRequest {
            resource: "wallet:another-wallet".parse().unwrap(),
            ..request(1, "AWS")
        };
        assert_eq!(denial(check(&store, &req)).reason, DenyReason::NoMandate);
    }

    #[test]
    fn forged_leaf_is_denied() {
        let m1 = sign(m1_unsigned());
        let m2 = forge(m2_unsigned(&m1));
        let store = VecStore(vec![m1, m2.clone()], vec![]);
        assert_eq!(
            denial(check(&store, &request(200, "AWS"))).reason,
            DenyReason::InvalidSignature { mandate_id: m2.id }
        );
    }

    #[test]
    fn forged_ancestor_is_denied() {
        // A forged M1 "from Alice" cannot back an honestly signed M2.
        let m1 = forge(m1_unsigned());
        let m2 = sign(m2_unsigned(&m1));
        let store = VecStore(vec![m1.clone(), m2], vec![]);
        assert_eq!(
            denial(check(&store, &request(200, "AWS"))).reason,
            DenyReason::InvalidSignature { mandate_id: m1.id }
        );
    }

    #[test]
    fn missing_parent_is_denied() {
        let (_, m1, m2) = demo();
        let store = VecStore(vec![m2.clone()], vec![]);
        assert_eq!(
            denial(check(&store, &request(200, "AWS"))).reason,
            DenyReason::InvalidDelegation {
                mandate_id: m2.id,
                fault: ChainFault::MissingParent(m1.id)
            }
        );
    }

    #[test]
    fn escalating_link_denies_even_requests_within_parent_limit() {
        // FinanceAgent signs itself a child for $5000. Even a $200 request
        // through it must fail: the chain itself is invalid.
        let m1 = sign(m1_unsigned());
        let m2 = sign(UnsignedMandate {
            constraints: Constraints {
                max_amount: Some(5000),
                allowed_recipients: aws(),
            },
            ..m2_unsigned(&m1)
        });
        let store = VecStore(vec![m1, m2.clone()], vec![]);
        assert_eq!(
            denial(check(&store, &request(200, "AWS"))).reason,
            DenyReason::InvalidDelegation {
                mandate_id: m2.id,
                fault: ChainFault::Delegation(DelegationError::AmountEscalation)
            }
        );
    }

    #[test]
    fn non_delegable_link_breaks_chain() {
        let (mut store, _, m2) = demo();
        let m3 = sign(UnsignedMandate {
            issuer: invoice(),
            subject: p("payments-bot"),
            parent: Some(m2.id.clone()),
            nonce: 3,
            ..m2.unsigned()
        });
        store.0.push(m3);
        let req = AuthorizationRequest {
            subject: p("payments-bot"),
            ..request(10, "AWS")
        };
        assert_eq!(
            denial(check(&store, &req)).reason.code(),
            "INVALID_DELEGATION"
        );
    }

    // --- Trust anchors ---

    #[test]
    fn self_issued_root_is_untrusted() {
        // InvoiceAgent signs itself unlimited authority over Alice's wallet.
        let selfie = sign(UnsignedMandate {
            issuer: invoice(),
            subject: invoice(),
            constraints: Constraints::default(),
            ..m1_unsigned()
        });
        let store = VecStore(vec![selfie.clone()], vec![]);
        assert_eq!(
            denial(check(&store, &request(700, "Anyone"))).reason,
            DenyReason::UntrustedRoot {
                mandate_id: selfie.id,
                issuer: invoice()
            }
        );
    }

    #[test]
    fn owner_is_trusted_only_for_its_own_resource() {
        let m = sign(UnsignedMandate {
            subject: invoice(),
            resource: "wallet:someone-elses".parse().unwrap(),
            ..m1_unsigned()
        });
        let store = VecStore(vec![m], vec![]);
        let req = AuthorizationRequest {
            resource: "wallet:someone-elses".parse().unwrap(),
            ..request(1, "AWS")
        };
        assert_eq!(denial(check(&store, &req)).reason.code(), "UNTRUSTED_ROOT");
    }

    #[test]
    fn denial_explains_closest_candidate() {
        // Next to the real M2, InvoiceAgent also holds a self-issued root and
        // a forged mandate. The $700 denial must still say AMOUNT_EXCEEDED.
        let (mut store, m1, m2) = demo();
        store.0.push(sign(UnsignedMandate {
            issuer: invoice(),
            subject: invoice(),
            constraints: Constraints::default(),
            ..m1_unsigned()
        }));
        store.0.push(forge(UnsignedMandate {
            nonce: 77,
            ..m2_unsigned(&m1)
        }));
        let d = denial(check(&store, &request(700, "AWS")));
        assert_eq!(d.reason.code(), "AMOUNT_EXCEEDED");
        assert_eq!(d.mandate_id, Some(m2.id));
    }

    // --- Robustness ---

    #[test]
    fn mandates_for_other_subjects_are_ignored() {
        /// A store that ignores the subject filter.
        struct SloppyStore(VecStore);
        impl MandateStore for SloppyStore {
            fn get_mandate(&self, id: &MandateId) -> Option<Mandate> {
                self.0.get_mandate(id)
            }
            fn find_mandates(&self, _: &PrincipalId, _: &str, _: &ResourceId) -> Vec<Mandate> {
                self.0 .0.clone()
            }
            fn find_revocations(&self, id: &MandateId) -> Vec<Revocation> {
                self.0.find_revocations(id)
            }
        }
        let (store, _, _) = demo();
        let store = SloppyStore(store);
        // M1 belongs to FinanceAgent; InvoiceAgent asking for $900 must not use it.
        let d = denial(authorize(
            &request(900, "AWS"),
            &store,
            &FakeVerifier,
            &anchors(),
        ));
        assert_eq!(d.reason.code(), "AMOUNT_EXCEEDED");
    }

    /// root → p1 → p2 → … → p{hops}, each delegable, all signed honestly.
    fn chain_of(hops: usize) -> (VecStore, PrincipalId) {
        let mut store = VecStore::default();
        let mut parent = sign(UnsignedMandate {
            subject: p("p0"),
            ..m1_unsigned()
        });
        store.0.push(parent.clone());
        for i in 1..=hops {
            let child = sign(UnsignedMandate {
                issuer: parent.subject.clone(),
                subject: p(&format!("p{i}")),
                parent: Some(parent.id.clone()),
                ..parent.unsigned()
            });
            store.0.push(child.clone());
            parent = child;
        }
        (store, parent.subject)
    }

    #[test]
    fn chain_depth_is_limited() {
        let (store, leaf) = chain_of(MAX_CHAIN_DEPTH);
        let req = AuthorizationRequest {
            subject: leaf,
            ..request(1, "AWS")
        };
        assert!(check(&store, &req).is_allowed());

        let (store, leaf) = chain_of(MAX_CHAIN_DEPTH + 1);
        let req = AuthorizationRequest {
            subject: leaf,
            ..request(1, "AWS")
        };
        let d = denial(check(&store, &req));
        assert!(matches!(
            d.reason,
            DenyReason::InvalidDelegation {
                fault: ChainFault::TooDeep,
                ..
            }
        ));
    }

    #[test]
    fn deny_reason_codes_are_stable() {
        let id = MandateId("0x".into());
        let cases = [
            (DenyReason::NoMandate, "NO_MANDATE"),
            (
                DenyReason::InvalidSignature {
                    mandate_id: id.clone(),
                },
                "INVALID_SIGNATURE",
            ),
            (
                DenyReason::Revoked {
                    mandate_id: id.clone(),
                    revocation_id: RevocationId("0x".into()),
                },
                "REVOKED",
            ),
            (
                DenyReason::AncestorRevoked {
                    revoked: id,
                    revocation_id: RevocationId("0x".into()),
                },
                "ANCESTOR_REVOKED",
            ),
            (DenyReason::ActionNotAllowed, "ACTION_NOT_ALLOWED"),
            (DenyReason::ResourceNotAllowed, "RESOURCE_NOT_ALLOWED"),
        ];
        for (reason, code) in cases {
            assert_eq!(reason.code(), code);
        }
    }
    // --- Revocation (Milestone 6) ---

    fn revoke(m: &Mandate, by: PrincipalId) -> Revocation {
        let issuer = by.clone();
        UnsignedRevocation {
            mandate_id: m.id.clone(),
            issuer,
            revoked_at: NOW + 2 * HOUR,
            reason: Some("user_revoked".into()),
        }
        .into_revocation(format!("signed-by:{by}").into_bytes())
    }

    fn forge_revocation(m: &Mandate) -> Revocation {
        UnsignedRevocation {
            mandate_id: m.id.clone(),
            issuer: m.issuer.clone(),
            revoked_at: NOW + 2 * HOUR,
            reason: None,
        }
        .into_revocation(b"signed-by:someone-else".to_vec())
    }

    #[test]
    fn directly_revoked_mandate_is_denied() {
        let (mut store, _, m2) = demo();
        let r = revoke(&m2, finance());
        store.1.push(r.clone());
        let d = denial(check(&store, &request(100, "AWS")));
        assert_eq!(
            d.reason,
            DenyReason::Revoked {
                mandate_id: m2.id.clone(),
                revocation_id: r.id
            }
        );
        assert_eq!(d.reason.code(), "REVOKED");
        assert_eq!(d.mandate_id, Some(m2.id));
    }

    #[test]
    fn revoked_parent_invalidates_child() {
        // Demo steps 6–7: Alice revokes M1; InvoiceAgent's M2 stops working
        // without being revoked itself.
        let (mut store, m1, m2) = demo();
        assert!(check(&store, &request(100, "AWS")).is_allowed());

        let r = revoke(&m1, alice());
        store.1.push(r.clone());
        let d = denial(check(&store, &request(100, "AWS")));
        assert_eq!(
            d.reason,
            DenyReason::AncestorRevoked {
                revoked: m1.id.clone(),
                revocation_id: r.id
            }
        );
        assert_eq!(d.reason.code(), "ANCESTOR_REVOKED");
        assert_eq!(d.mandate_id, Some(m2.id));
        assert_eq!(d.delegation_path, vec![alice(), finance(), invoice()]);

        // FinanceAgent's own use of M1 is cut off too.
        let req = AuthorizationRequest {
            subject: finance(),
            ..request(100, "AWS")
        };
        assert_eq!(denial(check(&store, &req)).reason.code(), "REVOKED");
    }

    #[test]
    fn revoked_grandparent_invalidates_descendant() {
        let (mut store, leaf) = chain_of(2); // root → p1 → p2
        let req = AuthorizationRequest {
            subject: leaf,
            ..request(1, "AWS")
        };
        assert!(check(&store, &req).is_allowed());

        let root = store.0[0].clone();
        store.1.push(revoke(&root, root.issuer.clone()));
        assert_eq!(
            denial(check(&store, &req)).reason,
            DenyReason::AncestorRevoked {
                revoked: root.id.clone(),
                revocation_id: store.1[0].id.clone()
            }
        );
    }

    #[test]
    fn highest_revoked_ancestor_is_reported() {
        let (mut store, m1, m2) = demo();
        store.1.push(revoke(&m2, finance()));
        store.1.push(revoke(&m1, alice()));
        let d = denial(check(&store, &request(100, "AWS")));
        assert!(matches!(
            d.reason,
            DenyReason::AncestorRevoked { ref revoked, .. } if *revoked == m1.id
        ));
    }

    #[test]
    fn unrelated_revocation_does_not_affect_mandate() {
        let (mut store, _, _) = demo();
        // Alice revokes a different grant she made to FinanceAgent.
        let other = sign(UnsignedMandate {
            nonce: 42,
            ..m1_unsigned()
        });
        store.0.push(other.clone());
        store.1.push(revoke(&other, alice()));
        assert!(check(&store, &request(200, "AWS")).is_allowed());
    }

    #[test]
    fn forged_revocation_is_rejected() {
        let (mut store, m1, _) = demo();
        store.1.push(forge_revocation(&m1));
        assert!(check(&store, &request(200, "AWS")).is_allowed());
    }

    #[test]
    fn revocation_by_non_issuer_is_ignored() {
        // Correctly signed, but by principals who did not issue M1.
        let (mut store, m1, _) = demo();
        store.1.push(revoke(&m1, finance()));
        store.1.push(revoke(&m1, invoice()));
        assert!(check(&store, &request(200, "AWS")).is_allowed());
    }

    #[test]
    fn revocation_with_tampered_content_is_ignored() {
        let (mut store, m1, m2) = demo();
        // A real revocation of M2, relabelled to target M1 (ID recomputed).
        let real = revoke(&m2, finance());
        let mut retargeted = real.unsigned();
        retargeted.mandate_id = m1.id.clone();
        retargeted.issuer = alice();
        store
            .1
            .push(retargeted.into_revocation(real.signature.clone()));
        assert!(check(&store, &request(200, "AWS")).is_allowed());
    }

    #[test]
    fn revocation_returned_for_wrong_mandate_is_ignored() {
        /// A store that returns every revocation for every query.
        struct LeakyStore(VecStore);
        impl MandateStore for LeakyStore {
            fn get_mandate(&self, id: &MandateId) -> Option<Mandate> {
                self.0.get_mandate(id)
            }
            fn find_mandates(&self, s: &PrincipalId, a: &str, r: &ResourceId) -> Vec<Mandate> {
                self.0.find_mandates(s, a, r)
            }
            fn find_revocations(&self, _: &MandateId) -> Vec<Revocation> {
                self.0 .1.clone()
            }
        }
        let (mut store, _, _) = demo();
        let other = sign(UnsignedMandate {
            nonce: 42,
            ..m1_unsigned()
        });
        store.1.push(revoke(&other, alice()));
        let d = authorize(
            &request(200, "AWS"),
            &LeakyStore(store),
            &FakeVerifier,
            &anchors(),
        );
        assert!(d.is_allowed());
    }

    #[test]
    fn forged_revocation_cannot_hide_real_one() {
        // Whatever order the store returns them in, the real one counts.
        let (mut store, m1, _) = demo();
        let real = revoke(&m1, alice());
        store.1.push(forge_revocation(&m1));
        store.1.push(real.clone());
        assert_eq!(
            denial(check(&store, &request(100, "AWS"))).reason.code(),
            "ANCESTOR_REVOKED"
        );
        store.1.reverse();
        assert_eq!(
            denial(check(&store, &request(100, "AWS"))).reason.code(),
            "ANCESTOR_REVOKED"
        );
    }

    #[test]
    fn revocation_applies_regardless_of_revoked_at() {
        // revoked_at is informational. A revocation timestamped after the
        // request (e.g. clock skew) still applies once published.
        let (mut store, m1, _) = demo();
        let r = UnsignedRevocation {
            mandate_id: m1.id.clone(),
            issuer: alice(),
            revoked_at: NOW + 1000 * HOUR,
            reason: None,
        }
        .into_revocation(format!("signed-by:{}", alice()).into_bytes());
        store.1.push(r);
        assert!(!check(&store, &request(100, "AWS")).is_allowed());
    }

    #[test]
    fn revoking_one_branch_leaves_sibling_branch_working() {
        // FinanceAgent gave InvoiceAgent two separate mandates; revoking one
        // leaves the other usable.
        let (mut store, m1, m2) = demo();
        let sibling = sign(UnsignedMandate {
            nonce: 3,
            ..m2_unsigned(&m1)
        });
        store.0.push(sibling.clone());
        store.1.push(revoke(&m2, finance()));
        let AuthorizationDecision::Allow(e) = check(&store, &request(200, "AWS")) else {
            panic!("expected ALLOW via the sibling mandate");
        };
        assert_eq!(e.mandate_id, sibling.id);
    }
}
