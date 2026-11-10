// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:12.127772
//+ Revised: 	2026/11/10 17:59:19.667390
// -----------------------------------------------------------------------------

use super::*;

const NOW: i64 = 1_800_000_000;

fn key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

fn trusted() -> TrustedKeys {
    TrustedKeys::from([("v1".to_string(), key(1).verifying_key())])
}

fn claims() -> Claims {
    Claims {
        iss: "https://licences.example.com".into(),
        aud: "avalio-lite".into(),
        sub: Uuid::from_u128(1),
        act: Uuid::from_u128(2),
        dev: "device".into(),
        lic_exp: None,
        test: false,
        iat: NOW,
        exp: NOW + 30 * 86_400,
        jti: Uuid::from_u128(3),
    }
}

#[test]
fn a_signed_token_verifies_and_gives_its_claims_back() {
    let claims = claims();
    let token = sign(&claims, "v1", &key(1));
    assert_eq!(token.split('.').count(), 3);
    assert_eq!(verify(&token, &trusted(), "avalio-lite", NOW), Ok(claims));
}

#[test]
fn the_header_names_the_algorithm_and_key() {
    let token = sign(&claims(), "v2", &key(1));
    let header: serde_json::Value =
        serde_json::from_slice(&encoding::decode(token.split('.').next().unwrap()).unwrap()).unwrap();
    assert_eq!(header, serde_json::json!({ "alg": "EdDSA", "typ": "JWT", "kid": "v2" }));
}

#[test]
fn a_permanent_licence_has_no_licence_expiry_claim() {
    let token = sign(&claims(), "v1", &key(1));
    let body: serde_json::Value =
        serde_json::from_slice(&encoding::decode(token.split('.').nth(1).unwrap()).unwrap()).unwrap();
    let names: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(names, ["act", "aud", "dev", "exp", "iat", "iss", "jti", "sub"]);
}

#[test]
fn a_test_licence_says_so_in_its_token() {
    let claims = Claims { test: true, ..claims() };
    let token = sign(&claims, "v1", &key(1));
    let body: serde_json::Value =
        serde_json::from_slice(&encoding::decode(token.split('.').nth(1).unwrap()).unwrap()).unwrap();
    assert_eq!(body["test"], serde_json::json!(true));
    assert_eq!(verify(&token, &trusted(), "avalio-lite", NOW), Ok(claims));
}

#[test]
fn a_token_without_the_test_claim_is_for_an_ordinary_licence() {
    let mut body = serde_json::to_value(claims()).unwrap();
    body.as_object_mut().unwrap().remove("test");
    let read: Claims = serde_json::from_value(body).unwrap();
    assert!(!read.test, "tokens from servers before test licences read as ordinary ones");
}

#[test]
fn tokens_from_unknown_keys_or_with_bad_signatures_are_refused() {
    assert_eq!(verify(&sign(&claims(), "v2", &key(1)), &trusted(), "avalio-lite", NOW), Err(TokenError::UnknownKey));
    assert_eq!(verify(&sign(&claims(), "v1", &key(2)), &trusted(), "avalio-lite", NOW), Err(TokenError::BadSignature));
}

#[test]
fn tampered_claims_are_refused() {
    let token = sign(&claims(), "v1", &key(1));
    let parts: Vec<&str> = token.split('.').collect();
    let longer = Claims { exp: NOW + 365 * 86_400, ..claims() };
    let forged_claims = encoding::encode(&serde_json::to_vec(&longer).unwrap());
    let forged = format!("{}.{forged_claims}.{}", parts[0], parts[2]);
    assert_eq!(verify(&forged, &trusted(), "avalio-lite", NOW), Err(TokenError::BadSignature));
}

#[test]
fn tokens_for_another_product_or_expired_are_refused() {
    let token = sign(&claims(), "v1", &key(1));
    assert_eq!(verify(&token, &trusted(), "other", NOW), Err(TokenError::WrongProduct));
    assert_eq!(verify(&token, &trusted(), "avalio-lite", NOW + 30 * 86_400), Err(TokenError::Expired));
}

#[test]
fn malformed_tokens_are_refused() {
    let token = sign(&claims(), "v1", &key(1));
    let header_none = encoding::encode(br#"{"alg":"none","typ":"JWT","kid":"v1"}"#);
    let rest: Vec<&str> = token.splitn(2, '.').collect();
    for bad in
        [String::new(), "a.b".into(), format!("{token}.extra"), format!("{header_none}.{}", rest[1]), "x.y.z".into()]
    {
        assert_eq!(verify(&bad, &trusted(), "avalio-lite", NOW), Err(TokenError::Malformed), "{bad}");
    }
}

#[test]
fn unverified_claims_are_read_even_from_a_token_that_would_be_refused() {
    let claims = Claims { lic_exp: Some(NOW + 5), ..claims() };
    let token = sign(&claims, "v9", &key(2));
    assert_eq!(unverified_claims(&token), Some(claims));
    assert_eq!(unverified_claims("garbage"), None);
}
