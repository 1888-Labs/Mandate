use std::fmt;

/// Why a child mandate is not a valid delegation of its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DelegationError {
    /// The parent mandate has `delegable == false`.
    ParentNotDelegable,
    /// The child was not issued by the parent's subject.
    InvalidIssuer,
    /// The child does not point at the parent's ID.
    InvalidParent,
    /// The child grants a different action.
    ActionEscalation,
    /// The child grants a different resource.
    ResourceEscalation,
    /// The child raises or removes the parent's amount limit.
    AmountEscalation,
    /// The child adds recipients or removes the parent's recipient restriction.
    RecipientEscalation,
    /// The child outlives the parent.
    ExpirationEscalation,
}

impl DelegationError {
    /// Stable machine-readable code for APIs and logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ParentNotDelegable => "PARENT_NOT_DELEGABLE",
            Self::InvalidIssuer => "INVALID_ISSUER",
            Self::InvalidParent => "INVALID_PARENT",
            Self::ActionEscalation => "ACTION_ESCALATION",
            Self::ResourceEscalation => "RESOURCE_ESCALATION",
            Self::AmountEscalation => "AMOUNT_ESCALATION",
            Self::RecipientEscalation => "RECIPIENT_ESCALATION",
            Self::ExpirationEscalation => "EXPIRATION_ESCALATION",
        }
    }
}

impl fmt::Display for DelegationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::ParentNotDelegable => "parent mandate is not delegable",
            Self::InvalidIssuer => "child issuer is not the parent's subject",
            Self::InvalidParent => "child does not reference the parent mandate",
            Self::ActionEscalation => "child grants a different action than the parent",
            Self::ResourceEscalation => "child grants a different resource than the parent",
            Self::AmountEscalation => "child amount limit exceeds the parent's",
            Self::RecipientEscalation => "child allows recipients the parent does not",
            Self::ExpirationEscalation => "child expires after the parent",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for DelegationError {}
