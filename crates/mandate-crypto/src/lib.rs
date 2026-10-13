//! Ethereum-compatible signatures for Mandate.
//!
//! An issuer signs a mandate by `personal_sign`-ing (EIP-191) its 32-byte
//! mandate digest with a secp256k1 key. Verification recovers the signer's
//! address and requires it to equal `mandate.issuer`.

pub mod error;
pub mod keys;
pub mod signing;
pub mod verification;

pub use error::SignatureError;
pub use keys::{address_of, generate_signing_key, is_canonical_address, parse_address, SigningKey};
pub use signing::sign_mandate;
pub use verification::{recover_signer, verify_mandate_signature};
