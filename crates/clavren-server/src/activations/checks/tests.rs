// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 22:41:06.641900
//+ Revised: 	2026/11/14 22:41:06.641900
// -----------------------------------------------------------------------------

use chrono::{DateTime, Utc};
use clavren_protocol::wire::ErrorCode;
use uuid::Uuid;

use super::refuse_unusable;
use crate::activations::RevocationReason;
use crate::activations::check_repository::CheckTarget;

fn now() -> DateTime<Utc> {
    "2027-01-15T08:00:00Z".parse().unwrap()
}

fn target() -> CheckTarget {
    CheckTarget {
        id: Uuid::nil(),
        licence_id: Uuid::nil(),
        device_public_key: vec![0; 32],
        revoked_at: None,
        revocation_reason: None,
        licence_revoked_at: None,
        licence_expires_at: None,
        licence_is_test: false,
        product_code: "product".into(),
        product_active: true,
        product_offline_days: 30,
    }
}

fn ended(reason: RevocationReason) -> CheckTarget {
    CheckTarget { revoked_at: Some(now()), revocation_reason: Some(reason.as_str().into()), ..target() }
}

fn refusal(target: &CheckTarget) -> Option<&'static str> {
    refuse_unusable(target, now()).err().map(|error| error.code())
}

#[test]
fn a_good_licence_is_not_refused() {
    assert_eq!(refusal(&target()), None);
}

/// What a check sees when it waited for a revoke to finish: the activation as the revoke left
/// it, next to the licence as it was before.
#[test]
fn an_activation_ended_by_a_revoke_is_a_revoked_licence_even_before_the_licence_shows_it() {
    let racing = ended(RevocationReason::LicenceRevoked);
    assert_eq!(refusal(&racing), Some(ErrorCode::LicenceRevoked.as_str()));
    let settled = CheckTarget { licence_revoked_at: Some(now()), ..racing };
    assert_eq!(refusal(&settled), Some(ErrorCode::LicenceRevoked.as_str()));
}

#[test]
fn an_activation_ended_by_a_reset_is_only_this_computers_end() {
    assert_eq!(refusal(&ended(RevocationReason::Reset)), Some(ErrorCode::ActivationRevoked.as_str()));
}

#[test]
fn an_unavailable_product_and_an_expired_licence_are_refused() {
    let unavailable = CheckTarget { product_active: false, ..target() };
    assert_eq!(refusal(&unavailable), Some(ErrorCode::ProductUnavailable.as_str()));
    let expired = CheckTarget { licence_expires_at: Some(now()), ..target() };
    assert_eq!(refusal(&expired), Some(ErrorCode::LicenceExpired.as_str()));
}
