//! Configuration from the environment.

use mandate_core::{ResourceId, TrustAnchors};
use mandate_crypto::parse_address;

/// Parse `TRUST_ANCHORS`: `kind:id=0xaddr,0xaddr;kind:id=0xaddr`.
pub fn parse_trust_anchors(spec: &str) -> Result<TrustAnchors, String> {
    let mut anchors = TrustAnchors::new();
    for entry in spec.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let (resource, owners) = entry
            .rsplit_once('=')
            .ok_or_else(|| format!("`{entry}`: expected resource=0xowner[,0xowner]"))?;
        let resource: ResourceId = resource
            .trim()
            .parse()
            .map_err(|e| format!("`{resource}`: {e}"))?;
        for owner in owners.split(',').map(str::trim).filter(|o| !o.is_empty()) {
            let owner = parse_address(owner).map_err(|_| format!("`{owner}` is not an address"))?;
            anchors = anchors.trust(resource.clone(), owner);
        }
    }
    Ok(anchors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandate_core::PrincipalId;

    #[test]
    fn parses_trust_anchors() {
        let a = "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf";
        let b = "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf";
        let anchors = parse_trust_anchors(&format!(
            " wallet:treasury-demo={a},{b} ; mcp:github:create_issue={b}"
        ))
        .unwrap();
        let treasury: ResourceId = "wallet:treasury-demo".parse().unwrap();
        let github: ResourceId = "mcp:github:create_issue".parse().unwrap();
        let a = PrincipalId(a.to_ascii_lowercase());
        let b = PrincipalId(b.into());
        assert!(anchors.is_trusted(&treasury, &a));
        assert!(anchors.is_trusted(&treasury, &b));
        assert!(anchors.is_trusted(&github, &b));
        assert!(!anchors.is_trusted(&github, &a));
        assert_eq!(parse_trust_anchors("").unwrap(), TrustAnchors::new());
    }

    #[test]
    fn rejects_bad_trust_anchors() {
        for bad in [
            "wallet:x",
            "nocolon=0x7e5f4552091a69125d5dfcb7b8c2659029395bdf",
            "wallet:x=alice",
        ] {
            assert!(parse_trust_anchors(bad).is_err(), "{bad}");
        }
    }
}
