//! Core protocol types and authorization semantics for Mandate.
//!
//! This crate owns the meaning of a mandate. It has no knowledge of
//! storage, networking or Arkiv.

pub mod capability;
pub mod mandate;
pub mod principal;

mod hex_bytes;

pub use capability::{Constraints, ResourceId, ResourceParseError};
pub use mandate::{Mandate, MandateId};
pub use principal::PrincipalId;
