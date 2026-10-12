use std::fmt;

use serde::{Deserialize, Serialize};

/// A human or agent that can issue or receive a mandate.
///
/// In v0.1 a principal is identified by its EVM address (`0x...`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrincipalId(pub String);

impl fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_plain_string() {
        let alice = PrincipalId("0xA11CE".to_string());
        let json = serde_json::to_string(&alice).unwrap();
        assert_eq!(json, "\"0xA11CE\"");

        let back: PrincipalId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, alice);
    }

    #[test]
    fn equality_is_exact() {
        let a = PrincipalId("0xabc".to_string());
        assert_eq!(a, PrincipalId("0xabc".to_string()));
        assert_ne!(a, PrincipalId("0xabd".to_string()));
    }
}
