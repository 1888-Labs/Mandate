//! Core protocol types and authorization semantics for Mandate.
//!
//! This crate owns the meaning of a mandate. It has no knowledge of
//! storage, networking or Arkiv.

pub mod capability;
pub mod encoding;
pub mod mandate;
pub mod principal;

mod hex_bytes;

pub use capability::{Constraints, ResourceId, ResourceParseError};
pub use encoding::{canonical_encoding, compute_mandate_id};
pub use mandate::{Mandate, MandateId, UnsignedMandate};
pub use principal::PrincipalId;
