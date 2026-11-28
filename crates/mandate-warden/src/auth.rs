//! Proving which agent sent a request.
//!
//! An agent signs a digest of the request with its key (EIP-191
//! `personal_sign`). The Warden recovers the signer and treats that address,
//! not anything the agent claims, as the mandate subject.

use std::collections::VecDeque;

use mandate_core::PrincipalId;
use mandate_crypto::recover_signer;
use sha3::{Digest, Keccak256};

/// How far a request's timestamp may drift from the Warden's clock.
pub const MAX_SKEW_SECS: u64 = 60;

/// The 32-byte digest an agent signs for one call.
///
/// `keccak256("mandate-call/1\n" METHOD "\n" path_and_query "\n" timestamp "\n" 0x<keccak256(body)>)`
pub fn request_digest(method: &str, path_and_query: &str, timestamp: u64, body: &[u8]) -> [u8; 32] {
    let body_hash = hex::encode(Keccak256::digest(body));
    let text = format!(
        "mandate-call/1\n{}\n{}\n{}\n0x{}",
        method.to_ascii_uppercase(),
        path_and_query,
        timestamp,
        body_hash
    );
    Keccak256::digest(text.as_bytes()).into()
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    Missing(&'static str),
    BadTimestamp,
    Stale,
    BadSignature,
    SignerMismatch,
    Replay,
}

impl AuthError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing(_) => "MISSING_AUTH",
            Self::BadTimestamp => "BAD_TIMESTAMP",
            Self::Stale => "STALE_REQUEST",
            Self::BadSignature => "BAD_SIGNATURE",
            Self::SignerMismatch => "SIGNER_MISMATCH",
            Self::Replay => "REPLAYED_REQUEST",
        }
    }
}

/// Remembers recent signatures so a captured request can't be replayed
/// inside the skew window.
#[derive(Default)]
pub struct ReplayGuard {
    seen: VecDeque<(u64, Vec<u8>)>,
}

impl ReplayGuard {
    fn check_and_remember(&mut self, now: u64, signature: &[u8]) -> bool {
        while self
            .seen
            .front()
            .is_some_and(|(t, _)| now.saturating_sub(*t) > 2 * MAX_SKEW_SECS)
        {
            self.seen.pop_front();
        }
        if self.seen.iter().any(|(_, s)| s == signature) {
            return false;
        }
        self.seen.push_back((now, signature.to_vec()));
        true
    }
}

/// Verify the agent headers and return the signer.
#[allow(clippy::too_many_arguments)]
pub fn verify(
    claimed: Option<&str>,
    timestamp: Option<&str>,
    signature: Option<&str>,
    method: &str,
    path_and_query: &str,
    body: &[u8],
    now: u64,
    replay: &mut ReplayGuard,
) -> Result<PrincipalId, AuthError> {
    let claimed = claimed.ok_or(AuthError::Missing("x-mandate-agent"))?;
    let timestamp: u64 = timestamp
        .ok_or(AuthError::Missing("x-mandate-timestamp"))?
        .parse()
        .map_err(|_| AuthError::BadTimestamp)?;
    if now.abs_diff(timestamp) > MAX_SKEW_SECS {
        return Err(AuthError::Stale);
    }
    let sig_hex = signature.ok_or(AuthError::Missing("x-mandate-signature"))?;
    let sig = hex::decode(sig_hex.trim_start_matches("0x")).map_err(|_| AuthError::BadSignature)?;
    let digest = request_digest(method, path_and_query, timestamp, body);
    let signer = recover_signer(&digest, &sig).map_err(|_| AuthError::BadSignature)?;
    if !signer.0.eq_ignore_ascii_case(claimed) {
        return Err(AuthError::SignerMismatch);
    }
    if !replay.check_and_remember(now, &sig) {
        return Err(AuthError::Replay);
    }
    Ok(signer)
}

/// Digest an owner signs for one admin request.
///
/// `keccak256("mandate-admin/1\n" METHOD "\n" path_and_query "\n" timestamp "\n" 0x<keccak256(body)>)`
///
/// Distinct from [`request_digest`] so an agent signature cannot be replayed
/// as an admin request, or the other way around.
pub fn admin_request_digest(
    method: &str,
    path_and_query: &str,
    timestamp: u64,
    body: &[u8],
) -> [u8; 32] {
    let body_hash = hex::encode(Keccak256::digest(body));
    let text = format!(
        "mandate-admin/1\n{}\n{}\n{}\n0x{}",
        method.to_ascii_uppercase(),
        path_and_query,
        timestamp,
        body_hash
    );
    Keccak256::digest(text.as_bytes()).into()
}

#[derive(Debug, PartialEq, Eq)]
pub enum AdminAuthError {
    Missing(&'static str),
    BadTimestamp,
    Stale,
    BadSignature,
    NotOwner,
    Replay,
}

impl AdminAuthError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing(_) => "MISSING_ADMIN_AUTH",
            Self::BadTimestamp => "BAD_TIMESTAMP",
            Self::Stale => "STALE_REQUEST",
            Self::BadSignature => "BAD_SIGNATURE",
            Self::NotOwner => "NOT_OWNER",
            Self::Replay => "REPLAYED_REQUEST",
        }
    }
}

/// Verify that this request was signed by `owner`.
///
/// The recovered signer is the owner address. A claimed address is not trusted.
#[allow(clippy::too_many_arguments)]
pub fn verify_admin(
    owner: &PrincipalId,
    timestamp: Option<&str>,
    signature: Option<&str>,
    method: &str,
    path_and_query: &str,
    body: &[u8],
    now: u64,
    replay: &mut ReplayGuard,
) -> Result<(), AdminAuthError> {
    let timestamp: u64 = timestamp
        .ok_or(AdminAuthError::Missing("x-mandate-admin-timestamp"))?
        .parse()
        .map_err(|_| AdminAuthError::BadTimestamp)?;
    if now.abs_diff(timestamp) > MAX_SKEW_SECS {
        return Err(AdminAuthError::Stale);
    }
    let sig_hex = signature.ok_or(AdminAuthError::Missing("x-mandate-admin-signature"))?;
    let sig =
        hex::decode(sig_hex.trim_start_matches("0x")).map_err(|_| AdminAuthError::BadSignature)?;
    let digest = admin_request_digest(method, path_and_query, timestamp, body);
    let signer = recover_signer(&digest, &sig).map_err(|_| AdminAuthError::BadSignature)?;
    if &signer != owner {
        return Err(AdminAuthError::NotOwner);
    }
    if !replay.check_and_remember(now, &sig) {
        return Err(AdminAuthError::Replay);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandate_crypto::{address_of, generate_signing_key, sign_message};

    fn signed(method: &str, path: &str, ts: u64, body: &[u8]) -> (String, String) {
        let key = generate_signing_key();
        let sig = sign_message(&request_digest(method, path, ts, body), &key).unwrap();
        (address_of(&key).0, format!("0x{}", hex::encode(sig)))
    }

    #[test]
    fn accepts_a_fresh_signed_request_once() {
        let (addr, sig) = signed("POST", "/x", 1000, b"{}");
        let mut guard = ReplayGuard::default();
        let who = verify(
            Some(&addr),
            Some("1000"),
            Some(&sig),
            "POST",
            "/x",
            b"{}",
            1010,
            &mut guard,
        )
        .unwrap();
        assert_eq!(who.0, addr);
        let again = verify(
            Some(&addr),
            Some("1000"),
            Some(&sig),
            "POST",
            "/x",
            b"{}",
            1011,
            &mut guard,
        );
        assert_eq!(again, Err(AuthError::Replay));
    }

    #[test]
    fn rejects_tampered_body_or_stale_time() {
        let (addr, sig) = signed("POST", "/x", 1000, b"{}");
        let mut guard = ReplayGuard::default();
        let tampered = verify(
            Some(&addr),
            Some("1000"),
            Some(&sig),
            "POST",
            "/x",
            b"{\"a\":1}",
            1000,
            &mut guard,
        );
        assert_eq!(tampered, Err(AuthError::SignerMismatch));
        let stale = verify(
            Some(&addr),
            Some("1000"),
            Some(&sig),
            "POST",
            "/x",
            b"{}",
            1000 + 600,
            &mut guard,
        );
        assert_eq!(stale, Err(AuthError::Stale));
    }

    fn admin_signed(method: &str, path: &str, ts: u64, body: &[u8]) -> (PrincipalId, String) {
        let key = generate_signing_key();
        let sig = sign_message(&admin_request_digest(method, path, ts, body), &key).unwrap();
        (address_of(&key), format!("0x{}", hex::encode(sig)))
    }

    #[test]
    fn owner_signature_is_accepted_once() {
        let (owner, sig) = admin_signed("POST", "/v1/admin/mandates", 1000, b"{}");
        let mut guard = ReplayGuard::default();
        verify_admin(
            &owner,
            Some("1000"),
            Some(&sig),
            "POST",
            "/v1/admin/mandates",
            b"{}",
            1005,
            &mut guard,
        )
        .unwrap();
        let again = verify_admin(
            &owner,
            Some("1000"),
            Some(&sig),
            "POST",
            "/v1/admin/mandates",
            b"{}",
            1006,
            &mut guard,
        );
        assert_eq!(again, Err(AdminAuthError::Replay));
    }

    #[test]
    fn admin_rejects_wrong_key_tampered_body_and_stale_time() {
        let (owner, sig) = admin_signed("POST", "/v1/admin/revoke", 1000, b"{}");
        let (other, _) = admin_signed("POST", "/v1/admin/revoke", 1000, b"{}");
        let mut guard = ReplayGuard::default();
        assert_eq!(
            verify_admin(
                &other,
                Some("1000"),
                Some(&sig),
                "POST",
                "/v1/admin/revoke",
                b"{}",
                1000,
                &mut guard,
            ),
            Err(AdminAuthError::NotOwner)
        );
        assert_eq!(
            verify_admin(
                &owner,
                Some("1000"),
                Some(&sig),
                "POST",
                "/v1/admin/revoke",
                b"{\"a\":1}",
                1000,
                &mut guard,
            ),
            Err(AdminAuthError::NotOwner)
        );
        assert_eq!(
            verify_admin(
                &owner,
                Some("1000"),
                Some(&sig),
                "POST",
                "/v1/admin/revoke",
                b"{}",
                1000 + 600,
                &mut guard,
            ),
            Err(AdminAuthError::Stale)
        );
    }

    #[test]
    fn agent_digest_is_not_an_admin_digest() {
        let body = b"{}";
        assert_ne!(
            request_digest("POST", "/v1/admin/mandates", 1, body),
            admin_request_digest("POST", "/v1/admin/mandates", 1, body)
        );
    }
}
