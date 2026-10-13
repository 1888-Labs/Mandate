//! One function per CLI command.

use std::path::Path;

use mandate_core::{
    authorize, can_delegate, find_mandate, find_valid_revocation, AuthorizationRequest,
    Constraints, Mandate, ResourceId, UnsignedMandate, UnsignedRevocation,
};
use mandate_crypto::{
    address_of, generate_signing_key, sign_mandate, sign_revocation, verify_mandate_signature,
    EthereumVerifier,
};

use crate::error::CliError;
use crate::output::{self, row};
use crate::state::{valid_name, Keys, State};
use crate::time::{nonce, now, parse_duration};

/// Whether Mandate said yes. Errors are reported separately.
pub enum Outcome {
    Ok,
    Denied,
}

pub struct GrantArgs {
    pub from: String,
    pub to: String,
    pub action: String,
    pub resource: String,
    pub max_amount: Option<u64>,
    pub recipients: Vec<String>,
    pub delegable: bool,
    pub expires_in: String,
}

pub struct DelegateArgs {
    pub mandate: String,
    pub from: String,
    pub to: String,
    pub max_amount: Option<u64>,
    pub recipients: Vec<String>,
    pub delegable: bool,
    pub expires_in: Option<String>,
}

pub struct CheckArgs {
    pub agent: String,
    pub action: String,
    pub resource: String,
    pub amount: Option<u64>,
    pub recipient: Option<String>,
    pub at: Option<u64>,
}

fn parse_resource(s: &str) -> Result<ResourceId, CliError> {
    s.parse()
        .map_err(|e| CliError::Usage(format!("invalid resource `{s}`: {e}")))
}

fn expiry(now: u64, duration: &str) -> Result<u64, CliError> {
    now.checked_add(parse_duration(duration)?)
        .ok_or_else(|| CliError::Usage(format!("duration `{duration}` is too long")))
}

fn load_mandate(state: &State, reference: &str) -> Result<Mandate, CliError> {
    let id = state.resolve(reference)?;
    find_mandate(&state.store(), &EthereumVerifier, &id)
        .ok_or_else(|| CliError::UnknownMandate(reference.to_string()))
}

pub fn principal_create(home: &Path, name: &str) -> Result<Outcome, CliError> {
    if !valid_name(name) {
        return Err(CliError::InvalidName(name.to_string()));
    }
    let mut state = State::load(home)?;
    let mut keys = Keys::load(home)?;
    if state.principals.contains_key(name) {
        return Err(CliError::PrincipalExists(name.to_string()));
    }
    let key = generate_signing_key();
    let address = address_of(&key);
    // Save the key first: a principal must never exist without its key.
    keys.insert(name, &key);
    keys.save(home)?;
    state.principals.insert(name.to_string(), address.clone());
    state.save(home)?;

    println!("Principal {name} created\n");
    row("Address", &address);
    row(
        "Key",
        format!(
            "{} (development key, never commit)",
            home.join("keys.json").display()
        ),
    );
    Ok(Outcome::Ok)
}

pub fn principal_list(home: &Path) -> Result<Outcome, CliError> {
    let state = State::load(home)?;
    let keys = Keys::load(home)?;
    if state.principals.is_empty() {
        println!("No principals yet. Create one with: mandate principal create <name>");
    }
    for (name, address) in &state.principals {
        let key = if keys.get(name).is_ok() {
            "key: local"
        } else {
            "key: none"
        };
        println!("  {name:<16} {address}  {key}");
    }
    Ok(Outcome::Ok)
}

pub fn resource_own(home: &Path, resource: &str, owner: &str) -> Result<Outcome, CliError> {
    let resource = parse_resource(resource)?;
    let mut state = State::load(home)?;
    let address = state.address(owner)?;
    state
        .owners
        .entry(resource.to_string())
        .or_default()
        .insert(address);
    state.save(home)?;
    println!("{owner} now owns {resource}: it may issue root mandates for it.");
    Ok(Outcome::Ok)
}

pub fn resource_list(home: &Path) -> Result<Outcome, CliError> {
    let state = State::load(home)?;
    if state.owners.is_empty() {
        println!(
            "No resources yet. Register one with: mandate resource own <kind:id> --owner <name>"
        );
    }
    for (resource, owners) in &state.owners {
        let names: Vec<String> = owners.iter().map(|o| state.name_of(o)).collect();
        println!("  {resource:<28} owned by {}", names.join(", "));
    }
    Ok(Outcome::Ok)
}

pub fn grant(home: &Path, args: GrantArgs) -> Result<Outcome, CliError> {
    let mut state = State::load(home)?;
    let keys = Keys::load(home)?;
    let issuer = state.address(&args.from)?;
    let subject = state.address(&args.to)?;
    let resource = parse_resource(&args.resource)?;
    if !state.anchors().is_trusted(&resource, &issuer) {
        return Err(CliError::NotAnOwner {
            principal: args.from,
            resource: resource.to_string(),
        });
    }
    let key = keys.get(&args.from)?;
    let now = now();
    let unsigned = UnsignedMandate {
        issuer,
        subject,
        action: args.action,
        resource,
        constraints: Constraints {
            max_amount: args.max_amount,
            allowed_recipients: (!args.recipients.is_empty()).then_some(args.recipients),
        },
        parent: None,
        delegable: args.delegable,
        issued_at: now,
        expires_at: expiry(now, &args.expires_in)?,
        nonce: nonce(),
    };
    let mandate = sign_mandate(&unsigned, &key)?;
    let alias = state.add_mandate(mandate.clone());
    state.save(home)?;

    println!("Mandate {alias} created\n");
    output::print_tree(&state, std::slice::from_ref(&mandate));
    println!();
    output::print_mandate_summary(&mandate, now);
    Ok(Outcome::Ok)
}

pub fn delegate(home: &Path, args: DelegateArgs) -> Result<Outcome, CliError> {
    let mut state = State::load(home)?;
    let keys = Keys::load(home)?;
    let parent = load_mandate(&state, &args.mandate)?;
    let parent_alias = state.alias(&parent.id);
    let now = now();

    // Refuse to build on a mandate that can no longer grant anything.
    let store = state.store();
    let (chain, missing) = output::resolve_chain(&store, parent.clone());
    let unusable = |why: String| CliError::ParentUnusable {
        mandate: parent_alias.clone(),
        why,
    };
    if verify_mandate_signature(&parent).is_err() {
        return Err(unusable("not validly signed".into()));
    }
    if missing.is_some() {
        return Err(unusable("missing one of its own parents".into()));
    }
    if let Some(m) = chain.iter().find(|m| now >= m.expires_at) {
        return Err(unusable(format!("expired ({})", state.alias(&m.id))));
    }
    if let Some(m) = chain
        .iter()
        .find(|m| find_valid_revocation(m, &store, &EthereumVerifier).is_some())
    {
        return Err(unusable(format!("revoked ({})", state.alias(&m.id))));
    }

    let issuer = state.address(&args.from)?;
    let subject = state.address(&args.to)?;
    let key = keys.get(&args.from)?;
    let expires_at = match &args.expires_in {
        Some(d) => expiry(now, d)?,
        None => parent.expires_at,
    };
    let unsigned = UnsignedMandate {
        issuer,
        subject,
        action: parent.action.clone(),
        resource: parent.resource.clone(),
        constraints: Constraints {
            max_amount: args.max_amount.or(parent.constraints.max_amount),
            allowed_recipients: if args.recipients.is_empty() {
                parent.constraints.allowed_recipients.clone()
            } else {
                Some(args.recipients)
            },
        },
        parent: Some(parent.id.clone()),
        delegable: args.delegable,
        issued_at: now,
        expires_at,
        nonce: nonce(),
    };
    let child = sign_mandate(&unsigned, &key)?;
    if let Err(e) = can_delegate(&parent, &child) {
        output::print_delegation_rejected(&state, e, &parent, &child);
        return Ok(Outcome::Denied);
    }
    let alias = state.add_mandate(child.clone());
    state.save(home)?;

    let mut chain = chain;
    chain.push(child.clone());
    println!("Delegation valid. Mandate {alias} created\n");
    output::print_tree(&state, &chain);
    println!();
    output::print_mandate_summary(&child, now);
    Ok(Outcome::Ok)
}

pub fn check(home: &Path, args: CheckArgs) -> Result<Outcome, CliError> {
    let state = State::load(home)?;
    let request = AuthorizationRequest {
        subject: state.address(&args.agent)?,
        action: args.action,
        resource: parse_resource(&args.resource)?,
        amount: args.amount,
        recipient: args.recipient,
        timestamp: args.at.unwrap_or_else(now),
    };
    let decision = authorize(
        &request,
        &state.store(),
        &EthereumVerifier,
        &state.anchors(),
    );
    output::print_decision(&state, &request, &decision);
    Ok(if decision.is_allowed() {
        Outcome::Ok
    } else {
        Outcome::Denied
    })
}

pub fn revoke(home: &Path, reference: &str, reason: String) -> Result<Outcome, CliError> {
    let mut state = State::load(home)?;
    let keys = Keys::load(home)?;
    let mandate = load_mandate(&state, reference)?;
    let alias = state.alias(&mandate.id);
    let issuer_name = state
        .principals
        .iter()
        .find(|(_, addr)| **addr == mandate.issuer)
        .map(|(name, _)| name.clone())
        .ok_or_else(|| CliError::MissingKey(state.name_of(&mandate.issuer)))?;
    let key = keys.get(&issuer_name)?;
    let revocation = sign_revocation(
        &UnsignedRevocation {
            mandate_id: mandate.id.clone(),
            issuer: mandate.issuer.clone(),
            revoked_at: now(),
            reason: Some(reason.clone()),
        },
        &key,
    )?;
    if !state.revocations.iter().any(|r| r.id == revocation.id) {
        state.revocations.push(revocation.clone());
    }
    state.save(home)?;

    println!("Mandate {alias} revoked\n");
    row("Revoked by", &issuer_name);
    row("Reason", &reason);
    row("Revocation", &revocation.id);

    // Show what else stopped working.
    let store = state.store();
    let mut frontier = vec![mandate.id.clone()];
    let mut cut_off = Vec::new();
    while let Some(id) = frontier.pop() {
        for child in store.find_children(&id) {
            if cut_off.len() >= 64 {
                break;
            }
            frontier.push(child.id.clone());
            cut_off.push(child);
        }
    }
    if !cut_off.is_empty() {
        println!("\n  Also cut off (delegated beneath {alias}):");
        for m in &cut_off {
            println!(
                "    {}  {} → {}  [{}]",
                state.alias(&m.id),
                state.name_of(&m.issuer),
                state.name_of(&m.subject),
                output::limits(&m.constraints)
            );
        }
    }
    Ok(Outcome::Ok)
}

pub fn inspect(home: &Path, reference: &str) -> Result<Outcome, CliError> {
    let state = State::load(home)?;
    let mandate = load_mandate(&state, reference)?;
    output::print_inspect(&state, &state.store(), &mandate, now());
    Ok(Outcome::Ok)
}

pub fn chain(home: &Path, reference: &str) -> Result<Outcome, CliError> {
    let state = State::load(home)?;
    let mandate = load_mandate(&state, reference)?;
    output::print_chain(&state, &state.store(), mandate, now());
    Ok(Outcome::Ok)
}
