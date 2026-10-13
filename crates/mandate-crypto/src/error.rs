use std::fmt;

/// Why a signature could not be produced or verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignatureError {
    /// An issuer or subject is not a lowercase `0x` + 40-hex-digit address.
    NonCanonicalAddress,
    /// The stored mandate ID does not match the mandate's content.
    IdMismatch,
    /// The signature is not 65 bytes of `r || s || v` with low `s`.
    MalformedSignature,
    /// No public key could be recovered from the signature.
    Unrecoverable,
    /// The signature was made by someone other than the issuer.
    IssuerMismatch,
    /// The signing backend failed.
    SigningFailed,
}

impl SignatureError {
    /// Stable machine-readable code for APIs and logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NonCanonicalAddress => "NON_CANONICAL_ADDRESS",
            Self::IdMismatch => "ID_MISMATCH",
            Self::MalformedSignature => "MALFORMED_SIGNATURE",
            Self::Unrecoverable => "UNRECOVERABLE_SIGNATURE",
            Self::IssuerMismatch => "ISSUER_MISMATCH",
            Self::SigningFailed => "SIGNING_FAILED",
        }
    }
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NonCanonicalAddress => "address must be lowercase 0x-prefixed 20-byte hex",
            Self::IdMismatch => "mandate ID does not match mandate content",
            Self::MalformedSignature => "signature must be 65 bytes r||s||v with low s",
            Self::Unrecoverable => "could not recover a public key from the signature",
            Self::IssuerMismatch => "signature was not made by the issuer",
            Self::SigningFailed => "signing failed",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for SignatureError {}
