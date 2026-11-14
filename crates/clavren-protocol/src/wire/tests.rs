// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:31.986016
//+ Revised: 	2026/11/14 11:36:31.986016
// -----------------------------------------------------------------------------

use super::*;

#[test]
fn codes_read_back_as_written() {
    for code in [
        ErrorCode::InvalidKey,
        ErrorCode::AlreadyActivated,
        ErrorCode::LicenceRevoked,
        ErrorCode::LicenceExpired,
        ErrorCode::ProductUnavailable,
        ErrorCode::ActivationRevoked,
        ErrorCode::ChallengeInvalid,
        ErrorCode::ApplicationPending,
        ErrorCode::ApplicationUnknown,
        ErrorCode::HumanCheckFailed,
        ErrorCode::RateLimited,
        ErrorCode::InvalidRequest,
        ErrorCode::InternalError,
    ] {
        assert_eq!(ErrorCode::parse(code.as_str()), Some(code));
        assert_eq!(serde_json::to_value(code).unwrap(), code.as_str());
    }
    assert_eq!(ErrorCode::parse("teapot"), None);
}
