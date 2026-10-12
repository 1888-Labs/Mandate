use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The thing a mandate grants authority over.
///
/// Written as `kind:id`. The kind is everything before the first `:`,
/// so `mcp:github:create_issue` has kind `mcp` and id `github:create_issue`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceId {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceParseError {
    MissingSeparator,
    EmptyKind,
    EmptyId,
}

impl fmt::Display for ResourceParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::MissingSeparator => "resource must be written as kind:id",
            Self::EmptyKind => "resource kind must not be empty",
            Self::EmptyId => "resource id must not be empty",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for ResourceParseError {}

impl FromStr for ResourceId {
    type Err = ResourceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind, id) = s
            .split_once(':')
            .ok_or(ResourceParseError::MissingSeparator)?;
        if kind.is_empty() {
            return Err(ResourceParseError::EmptyKind);
        }
        if id.is_empty() {
            return Err(ResourceParseError::EmptyId);
        }
        Ok(Self {
            kind: kind.to_string(),
            id: id.to_string(),
        })
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}

/// Limits attached to a mandate.
///
/// `None` means the mandate places no restriction of that kind.
/// A delegated child may never turn a `Some` restriction back into `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Constraints {
    pub max_amount: Option<u64>,
    pub allowed_recipients: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_resource_examples_from_spec() {
        let cases = [
            ("wallet:treasury-demo", "wallet", "treasury-demo"),
            ("mcp:github:create_issue", "mcp", "github:create_issue"),
            ("api:billing:refund", "api", "billing:refund"),
            ("database:customers:read", "database", "customers:read"),
        ];
        for (input, kind, id) in cases {
            let r: ResourceId = input.parse().unwrap();
            assert_eq!(r.kind, kind);
            assert_eq!(r.id, id);
            assert_eq!(r.to_string(), input);
        }
    }

    #[test]
    fn rejects_malformed_resources() {
        assert_eq!(
            "treasury".parse::<ResourceId>(),
            Err(ResourceParseError::MissingSeparator)
        );
        assert_eq!(
            ":treasury".parse::<ResourceId>(),
            Err(ResourceParseError::EmptyKind)
        );
        assert_eq!(
            "wallet:".parse::<ResourceId>(),
            Err(ResourceParseError::EmptyId)
        );
    }

    #[test]
    fn resource_serializes_as_kind_and_id_object() {
        let r: ResourceId = "wallet:treasury-demo".parse().unwrap();
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "wallet", "id": "treasury-demo" })
        );
        let back: ResourceId = serde_json::from_value(json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn resources_differ_by_kind_or_id() {
        let a: ResourceId = "wallet:treasury".parse().unwrap();
        assert_ne!(a, "wallet:another-wallet".parse().unwrap());
        assert_ne!(a, "database:treasury".parse().unwrap());
    }

    #[test]
    fn constraints_use_camel_case_json() {
        let c = Constraints {
            max_amount: Some(300),
            allowed_recipients: Some(vec!["AWS".to_string()]),
        };
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "maxAmount": 300, "allowedRecipients": ["AWS"] })
        );
        let back: Constraints = serde_json::from_value(json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn unrestricted_constraints_round_trip() {
        let c = Constraints::default();
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "maxAmount": null, "allowedRecipients": null })
        );
        let back: Constraints = serde_json::from_value(json).unwrap();
        assert_eq!(back, c);
    }
}
