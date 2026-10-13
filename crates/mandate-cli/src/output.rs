//! Human-readable output. Every decision says what happened and why.

use mandate_core::{
    authorization::MAX_CHAIN_DEPTH, can_delegate, find_mandate, find_valid_revocation,
    AuthorizationDecision, AuthorizationRequest, ChainFault, Constraints, DelegationError,
    DenyReason, Mandate, MandateId, MemoryMandateStore, Revocation, TrustAnchors,
};
use mandate_crypto::{verify_mandate_signature, EthereumVerifier};

use crate::state::{short, State};
use crate::time::{format_utc, relative};

pub fn row(label: &str, value: impl std::fmt::Display) {
    println!("  {label:<12} {value}");
}

pub fn money(amount: u64) -> String {
    format!("${amount}")
}

/// `≤ $300 → AWS`, `any amount → AWS, GitHub`, `≤ $50 → anyone`.
pub fn limits(c: &Constraints) -> String {
    let amount = c
        .max_amount
        .map_or_else(|| "any amount".to_string(), |a| format!("≤ {}", money(a)));
    let to = match &c.allowed_recipients {
        None => "anyone".to_string(),
        Some(r) if r.is_empty() => "no one".to_string(),
        Some(r) => r.join(", "),
    };
    format!("{amount} → {to}")
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// The chain from root to `leaf`, using the copy of each mandate that
/// verifies. The second value is a parent that could not be found.
pub fn resolve_chain(
    store: &MemoryMandateStore,
    leaf: Mandate,
) -> (Vec<Mandate>, Option<MandateId>) {
    let mut chain = vec![leaf];
    let mut missing = None;
    while let Some(parent_id) = chain.last().and_then(|m| m.parent.clone()) {
        if chain.len() > MAX_CHAIN_DEPTH {
            break;
        }
        match find_mandate(store, &EthereumVerifier, &parent_id) {
            Some(parent) => chain.push(parent),
            None => {
                missing = Some(parent_id);
                break;
            }
        }
    }
    chain.reverse();
    (chain, missing)
}

/// Why a single link in a chain is or isn't usable, checked in the same
/// order as the authorization engine.
pub enum LinkStatus {
    Valid,
    BadSignature,
    BrokenLink(DelegationError),
    UntrustedRoot,
    Expired,
    Revoked(Revocation),
}

pub fn link_status(
    store: &MemoryMandateStore,
    anchors: &TrustAnchors,
    parent: Option<&Mandate>,
    m: &Mandate,
    now: u64,
) -> LinkStatus {
    if verify_mandate_signature(m).is_err() {
        return LinkStatus::BadSignature;
    }
    match parent {
        Some(p) => {
            if let Err(e) = can_delegate(p, m) {
                return LinkStatus::BrokenLink(e);
            }
        }
        None => {
            if !anchors.is_trusted(&m.resource, &m.issuer) {
                return LinkStatus::UntrustedRoot;
            }
        }
    }
    if now >= m.expires_at {
        return LinkStatus::Expired;
    }
    match find_valid_revocation(m, store, &EthereumVerifier) {
        Some(r) => LinkStatus::Revoked(r),
        None => LinkStatus::Valid,
    }
}

fn describe_status(state: &State, m: &Mandate, status: &LinkStatus) -> String {
    match status {
        LinkStatus::Valid => "✓ valid".into(),
        LinkStatus::BadSignature => "✗ invalid signature".into(),
        LinkStatus::BrokenLink(e) => format!("✗ invalid delegation: {} ({e})", e.code()),
        LinkStatus::UntrustedRoot => format!(
            "✗ untrusted root: {} is not an owner of {}",
            state.name_of(&m.issuer),
            m.resource
        ),
        LinkStatus::Expired => format!("✗ expired {}", format_utc(m.expires_at)),
        LinkStatus::Revoked(r) => format!(
            "✗ revoked by {}{}",
            state.name_of(&r.issuer),
            r.reason
                .as_deref()
                .map_or_else(String::new, |why| format!(" ({why})"))
        ),
    }
}

/// ```text
///   alice
///    └── finance-agent  [≤ $1000 → AWS]
///         └── invoice-agent  [≤ $300 → AWS]
/// ```
pub fn print_tree(state: &State, chain: &[Mandate]) {
    if let Some(root) = chain.first() {
        println!("  {}", state.name_of(&root.issuer));
    }
    for (depth, m) in chain.iter().enumerate() {
        let indent = " ".repeat(3 + 5 * depth);
        println!(
            "{indent}└── {}  [{}]",
            state.name_of(&m.subject),
            limits(&m.constraints)
        );
    }
}

pub fn print_mandate_summary(m: &Mandate, now: u64) {
    row("Action", &m.action);
    row("Resource", &m.resource);
    row("Limits", limits(&m.constraints));
    row("Delegable", yes_no(m.delegable));
    row(
        "Expires",
        format!(
            "{} ({})",
            format_utc(m.expires_at),
            relative(m.expires_at, now)
        ),
    );
    row("ID", &m.id);
}

pub fn print_delegation_rejected(
    state: &State,
    e: DelegationError,
    parent: &Mandate,
    child: &Mandate,
) {
    println!("✗ DELEGATION REJECTED  {}\n", e.code());
    println!("  {e}.\n");
    row(
        "Parent",
        format!(
            "{}  {} → {}  [{}]  expires {}",
            state.alias(&parent.id),
            state.name_of(&parent.issuer),
            state.name_of(&parent.subject),
            limits(&parent.constraints),
            format_utc(parent.expires_at)
        ),
    );
    row(
        "Requested",
        format!(
            "{} → {}  [{}]  expires {}",
            state.name_of(&child.issuer),
            state.name_of(&child.subject),
            limits(&child.constraints),
            format_utc(child.expires_at)
        ),
    );
    println!("\n  A delegated mandate may only narrow its parent's authority. Nothing was stored.");
}

fn path(state: &State, path: &[mandate_core::PrincipalId]) -> String {
    path.iter()
        .map(|p| state.name_of(p))
        .collect::<Vec<_>>()
        .join(" → ")
}

fn request_line(req: &AuthorizationRequest) -> String {
    let mut line = req.action.clone();
    if let Some(a) = req.amount {
        line.push_str(&format!(" {}", money(a)));
    }
    if let Some(r) = &req.recipient {
        line.push_str(&format!(" → {r}"));
    }
    format!("{line}  on {}", req.resource)
}

pub fn print_decision(state: &State, req: &AuthorizationRequest, decision: &AuthorizationDecision) {
    let agent = state.name_of(&req.subject);
    match decision {
        AuthorizationDecision::Allow(e) => {
            println!("✓ AUTHORIZED\n");
            row("Agent", &agent);
            row("Request", request_line(req));
            row("Authority", state.name_of(&e.root_principal));
            row("Delegation", path(state, &e.delegation_path));
            row("Limits", limits(&e.effective_constraints));
            row("Mandate", state.alias(&e.mandate_id));
        }
        AuthorizationDecision::Deny(d) => {
            println!("✗ DENIED  {}\n", d.reason.code());
            row("Agent", &agent);
            row("Request", request_line(req));
            explain(state, req, &d.reason);
            if d.delegation_path.len() > 1 {
                row("Delegation", path(state, &d.delegation_path));
            }
            if let Some(id) = &d.mandate_id {
                row("Mandate", state.alias(id));
            }
        }
    }
}

fn explain(state: &State, req: &AuthorizationRequest, reason: &DenyReason) {
    let by = |rid: &mandate_core::RevocationId| {
        state
            .revocations
            .iter()
            .find(|r| &r.id == rid)
            .map_or_else(String::new, |r| format!(" by {}", state.name_of(&r.issuer)))
    };
    match reason {
        DenyReason::NoMandate => row(
            "Reason",
            format!(
                "{} holds no mandate for {} on {}",
                state.name_of(&req.subject),
                req.action,
                req.resource
            ),
        ),
        DenyReason::InvalidSignature { mandate_id } => row(
            "Reason",
            format!("{} has an invalid signature", state.alias(mandate_id)),
        ),
        DenyReason::InvalidDelegation { mandate_id, fault } => {
            let why = match fault {
                ChainFault::Delegation(e) => format!("{} ({e})", e.code()),
                ChainFault::MissingParent(p) => format!("parent {} not found", short(&p.0)),
                ChainFault::TooDeep => format!("chain longer than {MAX_CHAIN_DEPTH} hops"),
            };
            row(
                "Reason",
                format!(
                    "{} is not a valid delegation: {why}",
                    state.alias(mandate_id)
                ),
            )
        }
        DenyReason::UntrustedRoot { mandate_id, issuer } => row(
            "Reason",
            format!(
                "chain starts at {}, issued by {}, who is not an owner of {}",
                state.alias(mandate_id),
                state.name_of(issuer),
                req.resource
            ),
        ),
        DenyReason::Expired {
            mandate_id,
            expires_at,
        } => row(
            "Reason",
            format!(
                "{} expired {}",
                state.alias(mandate_id),
                format_utc(*expires_at)
            ),
        ),
        DenyReason::Revoked {
            mandate_id,
            revocation_id,
        } => {
            row(
                "Revoked",
                format!("{}{}", state.alias(mandate_id), by(revocation_id)),
            );
            row("Revocation", revocation_id);
        }
        DenyReason::AncestorRevoked {
            revoked,
            revocation_id,
        } => {
            row(
                "Revoked",
                format!(
                    "{}{} (an ancestor)",
                    state.alias(revoked),
                    by(revocation_id)
                ),
            );
            row("Revocation", revocation_id);
        }
        DenyReason::ActionNotAllowed => row("Reason", "action not allowed"),
        DenyReason::ResourceNotAllowed => row("Reason", "resource not allowed"),
        DenyReason::AmountExceeded { requested, maximum } => {
            row(
                "Requested",
                requested.map_or_else(|| "no amount given".to_string(), money),
            );
            row("Maximum", money(*maximum));
        }
        DenyReason::RecipientNotAllowed { requested, allowed } => {
            row("Recipient", requested.as_deref().unwrap_or("none given"));
            let allowed = if allowed.is_empty() {
                "no one".to_string()
            } else {
                allowed.join(", ")
            };
            row("Allowed", allowed);
        }
    }
}

pub fn print_inspect(state: &State, store: &MemoryMandateStore, m: &Mandate, now: u64) {
    println!("Mandate {}\n", state.alias(&m.id));
    row("ID", &m.id);
    row(
        "Issuer",
        format!("{} ({})", state.name_of(&m.issuer), m.issuer),
    );
    row(
        "Subject",
        format!("{} ({})", state.name_of(&m.subject), m.subject),
    );
    row("Action", &m.action);
    row("Resource", &m.resource);
    row("Limits", limits(&m.constraints));
    row("Delegable", yes_no(m.delegable));
    row(
        "Parent",
        m.parent
            .as_ref()
            .map_or_else(|| "none (root grant)".to_string(), |p| state.alias(p)),
    );
    row("Issued", format_utc(m.issued_at));
    row(
        "Expires",
        format!(
            "{} ({})",
            format_utc(m.expires_at),
            relative(m.expires_at, now)
        ),
    );
    row(
        "Signature",
        match verify_mandate_signature(m) {
            Ok(()) => format!("✓ valid, signed by {}", state.name_of(&m.issuer)),
            Err(e) => format!("✗ {} ({e})", e.code()),
        },
    );

    let (chain, missing) = resolve_chain(store, m.clone());
    let anchors = state.anchors();
    let mut parent = None;
    let mut verdict = "✓ usable".to_string();
    if let Some(p) = missing {
        verdict = format!("✗ parent {} not found", short(&p.0));
    } else {
        for link in &chain {
            let status = link_status(store, &anchors, parent, link, now);
            if !matches!(status, LinkStatus::Valid) {
                let which = if link.id == m.id {
                    "this mandate".to_string()
                } else {
                    state.alias(&link.id)
                };
                verdict = format!("{} — {which}", describe_status(state, link, &status));
                break;
            }
            parent = Some(link);
        }
    }
    row("Status", verdict);
    if chain.len() > 1 {
        println!(
            "\n  See the full chain with: mandate chain {}",
            state.alias(&m.id)
        );
    }
}

pub fn print_chain(state: &State, store: &MemoryMandateStore, leaf: Mandate, now: u64) {
    let leaf_alias = state.alias(&leaf.id);
    let (chain, missing) = resolve_chain(store, leaf);
    let anchors = state.anchors();
    println!("Chain for {leaf_alias}\n");
    if let Some(p) = &missing {
        println!("  ✗ parent {} not found\n   ⋮", short(&p.0));
    }
    let mut parent: Option<&Mandate> = None;
    let mut broken: Option<String> = missing.map(|_| "a parent is missing".to_string());
    for m in &chain {
        let status = link_status(store, &anchors, parent, m, now);
        if parent.is_none() {
            println!("  {}", state.name_of(&m.issuer));
        }
        println!(
            "   │  {}  {} {}{}  expires {}",
            state.alias(&m.id),
            m.action,
            limits(&m.constraints),
            if m.delegable { "  delegable" } else { "" },
            format_utc(m.expires_at)
        );
        println!("   │      {}", describe_status(state, m, &status));
        println!("   ▼");
        println!("  {}", state.name_of(&m.subject));
        if broken.is_none() && !matches!(status, LinkStatus::Valid) {
            broken = Some(format!("{} is not usable", state.alias(&m.id)));
        }
        parent = Some(m);
    }
    println!();
    match broken {
        None => row(
            "Result",
            format!("✓ chain valid: {leaf_alias} grants authority"),
        ),
        Some(why) => row(
            "Result",
            format!("✗ chain broken: {why}, so {leaf_alias} grants nothing"),
        ),
    }
}
