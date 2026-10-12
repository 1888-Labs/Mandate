//! Delegation validation: a child mandate may only narrow its parent.

use std::collections::BTreeSet;

use crate::capability::Constraints;
use crate::error::DelegationError;
use crate::mandate::Mandate;

/// Check that `child` is a valid delegation of `parent`.
///
/// A child may only reduce authority. It must keep the same action and
/// resource, may not expire later, may not raise the amount limit, and may
/// not allow recipients the parent does not. A restriction present on the
/// parent can never be dropped by the child.
///
/// This checks the parent–child relationship only. The caller must verify
/// separately that each mandate's ID matches its content, that both
/// signatures are valid, and that neither mandate is expired or revoked.
pub fn can_delegate(parent: &Mandate, child: &Mandate) -> Result<(), DelegationError> {
    if !parent.delegable {
        return Err(DelegationError::ParentNotDelegable);
    }
    if child.parent.as_ref() != Some(&parent.id) {
        return Err(DelegationError::InvalidParent);
    }
    if child.issuer != parent.subject {
        return Err(DelegationError::InvalidIssuer);
    }
    if child.action != parent.action {
        return Err(DelegationError::ActionEscalation);
    }
    if child.resource != parent.resource {
        return Err(DelegationError::ResourceEscalation);
    }
    if !amount_attenuates(&parent.constraints, &child.constraints) {
        return Err(DelegationError::AmountEscalation);
    }
    if !recipients_attenuate(&parent.constraints, &child.constraints) {
        return Err(DelegationError::RecipientEscalation);
    }
    if child.expires_at > parent.expires_at {
        return Err(DelegationError::ExpirationEscalation);
    }
    Ok(())
}

fn amount_attenuates(parent: &Constraints, child: &Constraints) -> bool {
    match (parent.max_amount, child.max_amount) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(p), Some(c)) => c <= p,
    }
}

fn recipients_attenuate(parent: &Constraints, child: &Constraints) -> bool {
    match (&parent.allowed_recipients, &child.allowed_recipients) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(p), Some(c)) => {
            let allowed: BTreeSet<&str> = p.iter().map(String::as_str).collect();
            c.iter().all(|r| allowed.contains(r.as_str()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mandate::UnsignedMandate;
    use crate::principal::PrincipalId;

    const T1: u64 = 1_760_172_800;

    fn alice() -> PrincipalId {
        PrincipalId("0xa11ce".into())
    }
    fn finance() -> PrincipalId {
        PrincipalId("0xf1".into())
    }
    fn invoice() -> PrincipalId {
        PrincipalId("0x1e".into())
    }

    fn recipients(names: &[&str]) -> Option<Vec<String>> {
        Some(names.iter().map(|s| s.to_string()).collect())
    }

    /// Alice → FinanceAgent: transfer ≤ 1000 to [AWS, GitHub] from wallet:treasury, delegable.
    fn parent() -> Mandate {
        UnsignedMandate {
            issuer: alice(),
            subject: finance(),
            action: "transfer".into(),
            resource: "wallet:treasury".parse().unwrap(),
            constraints: Constraints {
                max_amount: Some(1000),
                allowed_recipients: recipients(&["AWS", "GitHub"]),
            },
            parent: None,
            delegable: true,
            issued_at: 1_760_000_000,
            expires_at: T1,
            nonce: 1,
        }
        .into_mandate(vec![])
    }

    /// A child identical in scope to `parent`, issued by FinanceAgent to InvoiceAgent.
    fn child_unsigned(parent: &Mandate) -> UnsignedMandate {
        UnsignedMandate {
            issuer: parent.subject.clone(),
            subject: invoice(),
            action: parent.action.clone(),
            resource: parent.resource.clone(),
            constraints: parent.constraints.clone(),
            parent: Some(parent.id.clone()),
            delegable: false,
            issued_at: parent.issued_at + 60,
            expires_at: parent.expires_at,
            nonce: 2,
        }
    }

    /// Check `parent` against a child modified by `change`.
    fn delegate_with(change: impl FnOnce(&mut UnsignedMandate)) -> Result<(), DelegationError> {
        let p = parent();
        let mut c = child_unsigned(&p);
        change(&mut c);
        can_delegate(&p, &c.into_mandate(vec![]))
    }

    fn delegate_from(
        parent: &Mandate,
        change: impl FnOnce(&mut UnsignedMandate),
    ) -> Result<(), DelegationError> {
        let mut c = child_unsigned(parent);
        change(&mut c);
        can_delegate(parent, &c.into_mandate(vec![]))
    }

    fn parent_with(change: impl FnOnce(&mut UnsignedMandate)) -> Mandate {
        let mut p = parent().unsigned();
        change(&mut p);
        p.into_mandate(vec![])
    }

    #[test]
    fn identical_scope_is_valid() {
        assert_eq!(delegate_with(|_| {}), Ok(()));
    }

    // --- Amount ---

    #[test]
    fn child_can_reduce_amount() {
        for amount in [1000, 500, 1, 0] {
            assert_eq!(
                delegate_with(|c| c.constraints.max_amount = Some(amount)),
                Ok(()),
                "amount {amount}"
            );
        }
    }

    #[test]
    fn child_cannot_increase_amount() {
        for amount in [1001, 5000, u64::MAX] {
            assert_eq!(
                delegate_with(|c| c.constraints.max_amount = Some(amount)),
                Err(DelegationError::AmountEscalation),
                "amount {amount}"
            );
        }
    }

    #[test]
    fn child_cannot_remove_amount_limit() {
        assert_eq!(
            delegate_with(|c| c.constraints.max_amount = None),
            Err(DelegationError::AmountEscalation)
        );
    }

    #[test]
    fn unlimited_parent_allows_any_child_amount() {
        let p = parent_with(|p| p.constraints.max_amount = None);
        assert_eq!(
            delegate_from(&p, |c| c.constraints.max_amount = None),
            Ok(())
        );
        assert_eq!(
            delegate_from(&p, |c| c.constraints.max_amount = Some(u64::MAX)),
            Ok(())
        );
    }

    // --- Recipients ---

    #[test]
    fn child_can_reduce_recipients() {
        for set in [&["AWS"][..], &["GitHub"], &["AWS", "GitHub"]] {
            assert_eq!(
                delegate_with(|c| c.constraints.allowed_recipients = recipients(set)),
                Ok(()),
                "recipients {set:?}"
            );
        }
    }

    #[test]
    fn child_can_remove_all_recipients() {
        // An empty set is narrower than any set: it allows paying no one.
        assert_eq!(
            delegate_with(|c| c.constraints.allowed_recipients = recipients(&[])),
            Ok(())
        );
    }

    #[test]
    fn child_cannot_add_recipient() {
        for set in [&["AWS", "GitHub", "Coinbase"][..], &["Coinbase"]] {
            assert_eq!(
                delegate_with(|c| c.constraints.allowed_recipients = recipients(set)),
                Err(DelegationError::RecipientEscalation),
                "recipients {set:?}"
            );
        }
    }

    #[test]
    fn child_cannot_remove_recipient_restriction() {
        assert_eq!(
            delegate_with(|c| c.constraints.allowed_recipients = None),
            Err(DelegationError::RecipientEscalation)
        );
    }

    #[test]
    fn recipient_order_and_duplicates_do_not_matter() {
        assert_eq!(
            delegate_with(|c| {
                c.constraints.allowed_recipients = recipients(&["GitHub", "AWS", "AWS"])
            }),
            Ok(())
        );
    }

    #[test]
    fn recipient_names_are_case_sensitive() {
        assert_eq!(
            delegate_with(|c| c.constraints.allowed_recipients = recipients(&["aws"])),
            Err(DelegationError::RecipientEscalation)
        );
    }

    #[test]
    fn unrestricted_parent_allows_any_child_recipients() {
        let p = parent_with(|p| p.constraints.allowed_recipients = None);
        assert_eq!(
            delegate_from(&p, |c| c.constraints.allowed_recipients = None),
            Ok(())
        );
        assert_eq!(
            delegate_from(&p, |c| {
                c.constraints.allowed_recipients = recipients(&["Coinbase"])
            }),
            Ok(())
        );
    }

    // --- Expiration ---

    #[test]
    fn child_can_shorten_expiration() {
        for expires_at in [T1, T1 - 1, T1 - 86_400] {
            assert_eq!(
                delegate_with(|c| c.expires_at = expires_at),
                Ok(()),
                "expires_at {expires_at}"
            );
        }
    }

    #[test]
    fn child_cannot_extend_expiration() {
        for expires_at in [T1 + 1, T1 + 7 * 86_400, u64::MAX] {
            assert_eq!(
                delegate_with(|c| c.expires_at = expires_at),
                Err(DelegationError::ExpirationEscalation),
                "expires_at {expires_at}"
            );
        }
    }

    // --- Action and resource ---

    #[test]
    fn child_cannot_change_resource() {
        assert_eq!(
            delegate_with(|c| c.resource = "wallet:another-wallet".parse().unwrap()),
            Err(DelegationError::ResourceEscalation)
        );
        assert_eq!(
            delegate_with(|c| c.resource = "database:treasury".parse().unwrap()),
            Err(DelegationError::ResourceEscalation)
        );
    }

    #[test]
    fn child_cannot_change_action() {
        assert_eq!(
            delegate_with(|c| c.action = "refund".into()),
            Err(DelegationError::ActionEscalation)
        );
    }

    // --- Chain linkage ---

    #[test]
    fn child_issuer_must_match_parent_subject() {
        // Alice skipping FinanceAgent is not a delegation of M1.
        assert_eq!(
            delegate_with(|c| c.issuer = alice()),
            Err(DelegationError::InvalidIssuer)
        );
        assert_eq!(
            delegate_with(|c| c.issuer = invoice()),
            Err(DelegationError::InvalidIssuer)
        );
    }

    #[test]
    fn child_must_reference_parent_id() {
        assert_eq!(
            delegate_with(|c| c.parent = None),
            Err(DelegationError::InvalidParent)
        );
        let other = parent_with(|p| p.nonce = 99);
        assert_eq!(
            delegate_with(|c| c.parent = Some(other.id.clone())),
            Err(DelegationError::InvalidParent)
        );
    }

    #[test]
    fn non_delegable_parent_cannot_delegate() {
        let p = parent_with(|p| p.delegable = false);
        assert_eq!(
            delegate_from(&p, |c| c.constraints.max_amount = Some(1)),
            Err(DelegationError::ParentNotDelegable)
        );
    }

    #[test]
    fn non_delegable_child_cannot_delegate_further() {
        // InvoiceAgent received a non-delegable mandate and tries to pass it on.
        let m2 = child_unsigned(&parent()).into_mandate(vec![]);
        let result = delegate_from(&m2, |c| c.constraints.max_amount = Some(100));
        assert_eq!(result, Err(DelegationError::ParentNotDelegable));
    }

    // --- Multi-hop ---

    #[test]
    fn grandchild_must_attenuate_its_own_parent() {
        // M1: ≤1000 → M2 (delegable): ≤300 → M3 must be ≤300, not ≤1000.
        let m1 = parent();
        let mut m2 = child_unsigned(&m1);
        m2.constraints.max_amount = Some(300);
        m2.delegable = true;
        let m2 = m2.into_mandate(vec![]);
        assert_eq!(can_delegate(&m1, &m2), Ok(()));

        let m3_ok = delegate_from(&m2, |c| {
            c.subject = PrincipalId("0x3".into());
            c.constraints.max_amount = Some(300);
        });
        assert_eq!(m3_ok, Ok(()));

        let m3_bad = delegate_from(&m2, |c| {
            c.subject = PrincipalId("0x3".into());
            c.constraints.max_amount = Some(1000);
        });
        assert_eq!(m3_bad, Err(DelegationError::AmountEscalation));
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(
            DelegationError::AmountEscalation.code(),
            "AMOUNT_ESCALATION"
        );
        assert_eq!(
            DelegationError::RecipientEscalation.code(),
            "RECIPIENT_ESCALATION"
        );
        assert_eq!(
            DelegationError::ParentNotDelegable.code(),
            "PARENT_NOT_DELEGABLE"
        );
    }
}
