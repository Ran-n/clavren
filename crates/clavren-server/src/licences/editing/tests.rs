// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:36:31.512864
//+ Revised: 	2026/11/14 11:36:31.512864
// -----------------------------------------------------------------------------

use super::*;

fn changes(json: &str) -> LicenceChanges {
    serde_json::from_str(json).unwrap()
}

#[test]
fn an_empty_change_is_refused() {
    assert_eq!(parse(changes("{}")).unwrap_err().code(), "invalid_request");
}

#[test]
fn removing_the_customer_alone_is_a_change() {
    let parsed = parse(changes(r#"{"customer": null}"#)).unwrap();
    assert!(matches!(parsed.customer, Some(None)));
    assert_eq!(parsed.fields, ["customer"]);
}

#[test]
fn every_field_is_named_and_text_tidied() {
    let parsed = parse(changes(
        r#"{"customer": {"name": "Bea"}, "expires_at": null, "internal_notes": " ", "revocation_reason": " dup "}"#,
    ))
    .unwrap();
    assert_eq!(parsed.fields, ["customer", "expires_at", "internal_notes", "revocation_reason"]);
    let priced = parse(changes(r#"{"price": {"amount": 1999, "currency": "EUR"}}"#)).unwrap();
    assert_eq!(priced.fields, ["price"]);
    assert!(matches!(priced.changes.price, Some(Some(_))));
    assert_eq!(parse(changes(r#"{"price": null}"#)).unwrap().changes.price, Some(None));
    assert_eq!(parsed.changes.expires_at, Some(None));
    assert_eq!(parsed.changes.internal_notes, Some(None), "blank notes clear them");
    assert_eq!(parsed.changes.revocation_reason, Some(Some("dup".into())));
}

#[test]
fn an_over_long_reason_is_refused() {
    let body = format!(r#"{{"revocation_reason": "{}"}}"#, "x".repeat(MAX_REASON + 1));
    assert_eq!(parse(changes(&body)).unwrap_err().code(), "invalid_request");
}

#[test]
fn read_only_fields_are_unknown() {
    for field in ["product", "key", "state", "created_at", "revoked_at"] {
        let body = format!(r#"{{"{field}": null}}"#);
        assert!(serde_json::from_str::<LicenceChanges>(&body).is_err(), "{field}");
    }
}
