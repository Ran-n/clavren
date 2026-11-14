// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:25.917382
//+ Revised: 	2026/11/14 12:05:25.917382
// -----------------------------------------------------------------------------

//! Editing one licence in place.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn expiry_and_notes_are_edited_in_place(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    let path = format!("/api/admin/licences/{id}");
    let end = (licences.admin.fixture.clock.now() + chrono::Duration::days(365)).to_rfc3339();

    let reply =
        licences.admin.call(Method::PATCH, &path, Some(json!({ "expires_at": end, "internal_notes": " Paid " }))).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["internal_notes"], "Paid");
    assert!(reply.body["expires_at"].is_string());
    let reply = licences.admin.call(Method::PATCH, &path, Some(json!({ "expires_at": null }))).await;
    assert_eq!(
        (reply.body["expires_at"].clone(), reply.body["internal_notes"].as_str()),
        (Value::Null, Some("Paid")),
        "permanent again"
    );
    let expanded = licences
        .admin
        .call(Method::PATCH, &format!("{path}?expand=product"), Some(json!({ "internal_notes": "Paid in full" })))
        .await;
    assert_eq!(expanded.body["product"]["code"], "avalio-lite", "the answer expands like a read");
    let wrong = licences
        .admin
        .call(Method::PATCH, &format!("{path}?expand=owner"), Some(json!({ "internal_notes": "x" })))
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST, "checked before anything changes");

    let reply = licences.admin.call(Method::PATCH, &path, Some(json!({}))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    let reply = licences
        .admin
        .call(Method::PATCH, &format!("/api/admin/licences/{}", Uuid::now_v7()), Some(json!({ "internal_notes": "x" })))
        .await;
    assert_eq!((reply.status, reply.body.get("ids")), (StatusCode::NOT_FOUND, None));
    assert_eq!(licences.audited("licence.updated").await, 3);
}

/// Regression: an edit that changed nothing was still written and audited as `licence.updated`,
/// though the API promises it leaves the record and the audit log alone.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_edit_that_changes_nothing_leaves_no_trace(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let ids: Vec<String> = licences.issue(2, Value::Null).await.into_iter().map(|(id, _)| id).collect();
    assert_eq!(licences.patch(&ids[0], json!({ "internal_notes": "Kept" })).await.status, StatusCode::OK);
    assert_eq!(licences.audited("licence.updated").await, 1);

    let same = licences.patch(&ids[0], json!({ "internal_notes": "Kept", "expires_at": null })).await;
    assert_eq!((same.status, same.body["internal_notes"].clone()), (StatusCode::OK, json!("Kept")));
    assert_eq!(licences.audited("licence.updated").await, 1, "nothing changed, nothing audited");

    let reply = licences.post("/bulk/edit", json!({ "ids": ids, "internal_notes": "Kept" })).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(licences.audited("licence.updated").await, 2, "only the licence that changed");
    let audited: Vec<String> = sqlx::query_scalar(
        "SELECT entity_id::text FROM audit_logs WHERE action = 'licence.updated' ORDER BY created_at, entity_id",
    )
    .fetch_all(licences.admin.pool())
    .await
    .unwrap();
    let mut expected = ids.clone();
    expected.sort();
    let mut audited = audited;
    audited.sort();
    assert_eq!(audited, expected, "one entry for each licence, once");
    assert_eq!(licences.get(&ids[1]).await["internal_notes"], "Kept");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_revocation_reason_is_corrected(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let ids: Vec<String> = licences.issue(2, Value::Null).await.into_iter().map(|(id, _)| id).collect();
    licences.post(&format!("/{}/revoke", ids[0]), json!({ "reason": "dup" })).await;

    let reply = licences.patch(&ids[0], json!({ "revocation_reason": " duplicate order " })).await;
    assert_eq!((reply.status, reply.body["revocation_reason"].as_str()), (StatusCode::OK, Some("duplicate order")));
    let reply = licences.patch(&ids[0], json!({ "revocation_reason": null })).await;
    assert_eq!(reply.body["revocation_reason"], Value::Null);
    let live = licences.patch(&ids[1], json!({ "revocation_reason": "nope" })).await;
    assert_eq!((live.status, live.error()), (StatusCode::CONFLICT, "licence_not_revoked"));
    let long = licences.patch(&ids[0], json!({ "revocation_reason": "x".repeat(501) })).await;
    assert_eq!(long.status, StatusCode::BAD_REQUEST);
    let metadata = licences.audit_metadata().await;
    assert!(!metadata.contains("duplicate order"), "no reason text in the audit log");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_product_and_derived_fields_cant_be_edited(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    let before = licences.get(&id).await;
    for field in ["product", "key", "state", "created_at", "revoked_at", "id"] {
        let reply = licences.patch(&id, json!({ field: null, "internal_notes": "x" })).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{field}");
    }
    assert_eq!(licences.get(&id).await, before);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_revoked_licence_can_be_relabelled(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    licences.post(&format!("/{id}/revoke"), json!({})).await;
    let reply = licences.patch(&id, json!({ "customer": { "name": "Carla" }, "internal_notes": "Refund sent" })).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(
        (reply.body["state"].as_str(), reply.body["customer"]["name"].as_str()),
        (Some("revoked"), Some("Carla"))
    );
}
