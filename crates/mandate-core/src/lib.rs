//! Core protocol types and authorization semantics for Mandate.
//!
//! This crate owns the meaning of a mandate. It has no knowledge of
//! storage, networking or Arkiv.

pub mod authorization;
pub mod capability;
pub mod delegation;
pub mod encoding;
pub mod error;
pub mod mandate;
pub mod principal;
pub mod store;

mod hex_bytes;

pub use authorization::{
    authorize, AuthorizationDecision, AuthorizationEvidence, AuthorizationRequest, ChainFault,
    Denial, DenyReason, TrustAnchors,
};
pub use capability::{Constraints, ResourceId, ResourceParseError};
pub use delegation::can_delegate;
pub use encoding::{canonical_encoding, compute_mandate_id, mandate_digest};
pub use error::DelegationError;
pub use mandate::{Mandate, MandateId, UnsignedMandate};
pub use principal::PrincipalId;
pub use store::{MandateStore, SignatureVerifier};
