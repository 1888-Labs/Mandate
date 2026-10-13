use std::fmt;
use std::path::PathBuf;

use mandate_crypto::SignatureError;

/// Something went wrong running a command (as opposed to Mandate saying no).
#[derive(Debug)]
pub enum CliError {
    Io(PathBuf, std::io::Error),
    CorruptFile(PathBuf, serde_json::Error),
    InvalidName(String),
    PrincipalExists(String),
    UnknownPrincipal(String),
    MissingKey(String),
    UnknownMandate(String),
    AmbiguousMandate(String),
    NotAnOwner { principal: String, resource: String },
    ParentUnusable { mandate: String, why: String },
    Usage(String),
    Crypto(SignatureError),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
            Self::CorruptFile(path, e) => write!(f, "{} is not valid: {e}", path.display()),
            Self::InvalidName(n) => write!(
                f,
                "invalid name `{n}`: use lowercase letters, digits, `-` and `_`"
            ),
            Self::PrincipalExists(n) => write!(f, "principal `{n}` already exists"),
            Self::UnknownPrincipal(n) => write!(
                f,
                "unknown principal `{n}`. Create it with: mandate principal create {n}"
            ),
            Self::MissingKey(n) => write!(f, "no local signing key for `{n}`"),
            Self::UnknownMandate(m) => write!(f, "no mandate matches `{m}`"),
            Self::AmbiguousMandate(m) => {
                write!(
                    f,
                    "`{m}` matches more than one mandate; use more characters"
                )
            }
            Self::NotAnOwner {
                principal,
                resource,
            } => write!(
                f,
                "{principal} is not an owner of {resource}, so it cannot issue a root mandate.\n\
                 To trust it: mandate resource own {resource} --owner {principal}"
            ),
            Self::ParentUnusable { mandate, why } => {
                write!(f, "cannot delegate from {mandate}: it is {why}")
            }
            Self::Usage(msg) => f.write_str(msg),
            Self::Crypto(e) => write!(f, "{e} ({})", e.code()),
        }
    }
}

impl std::error::Error for CliError {}

impl From<SignatureError> for CliError {
    fn from(e: SignatureError) -> Self {
        Self::Crypto(e)
    }
}
