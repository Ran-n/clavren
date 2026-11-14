// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:26.028509
//+ Revised: 	2026/11/14 12:05:26.028509
// -----------------------------------------------------------------------------

//! Bulk edits, and a licence's customer added, changed and removed.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bulk_edits_change_all_or_nothing(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let ids: Vec<String> = licences.issue(3, Value::Null).await.into_iter().map(|(id, _)| id).collect();
    let ghost = Uuid::now_v7().to_string();

    let reply = licences.post("/bulk/edit", json!({ "ids": [ids[0], ghost, ids[1]], "internal_notes": "Batch" })).await;
    assert_eq!((reply.status, reply.body["ids"].clone()), (StatusCode::NOT_FOUND, json!([ghost])));
    assert_eq!(licences.get(&ids[0]).await["internal_notes"], Value::Null, "nothing changed");

    let reply = licences.post("/bulk/edit", json!({ "ids": ids, "internal_notes": "Batch" })).await;
    assert_eq!((reply.status, reply.body["updated"].clone()), (StatusCode::OK, json!(3)));
    for id in &ids {
        assert_eq!(licences.get(id).await["internal_notes"], "Batch");
    }
    assert_eq!(licences.audited("licence.updated").await, 3, "one entry per licence");
    // Regression: the answer counted the licences asked for, not those that changed.
    let again = licences.post("/bulk/edit", json!({ "ids": ids, "internal_notes": "Batch" })).await;
    assert_eq!((again.status, again.body["updated"].clone()), (StatusCode::OK, json!(0)), "nothing to change");

    let duplicate = licences.post("/bulk/edit", json!({ "ids": [ids[0], ids[0]], "internal_notes": "x" })).await;
    assert_eq!(duplicate.status, StatusCode::BAD_REQUEST);
    let too_many: Vec<String> = (0..1001).map(|_| Uuid::now_v7().to_string()).collect();
    assert_eq!(
        licences.post("/bulk/edit", json!({ "ids": too_many, "internal_notes": "x" })).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        licences.post("/bulk/edit", json!({ "ids": [], "internal_notes": "x" })).await.status,
        StatusCode::BAD_REQUEST
    );

    let ana = licences.customer("Ana", "ana@x.test").await;
    let reply = licences.post("/bulk/edit", json!({ "ids": ids, "customer": ana })).await;
    assert_eq!((reply.status, reply.body["updated"].clone()), (StatusCode::OK, json!(3)));
    for id in &ids {
        assert_eq!(licences.admin.get(&format!("/api/admin/licences/{id}")).await.body["customer"], ana.as_str());
    }
    let new = licences.post("/bulk/edit", json!({ "ids": ids, "customer": { "name": "Bea" } })).await;
    assert_eq!(new.status, StatusCode::OK);
    assert_eq!(licences.audited("customer.created").await, 2, "Ana, then Bea once for the whole batch");

    licences.post(&format!("/{}/revoke", ids[0]), json!({})).await;
    let mixed = licences.post("/bulk/edit", json!({ "ids": [ids[0], ids[1]], "revocation_reason": "Refunded" })).await;
    assert_eq!(
        (mixed.status, mixed.error(), mixed.body["ids"].clone()),
        (StatusCode::CONFLICT, "licence_not_revoked", json!([ids[1]]))
    );
    assert_eq!(licences.get(&ids[0]).await["revocation_reason"], Value::Null, "nothing changed");
    let ghost = licences.post("/bulk/edit", json!({ "ids": [ids[0], ghost], "revocation_reason": "Refunded" })).await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    let reply = licences.post("/bulk/edit", json!({ "ids": [ids[0]], "revocation_reason": "Refunded" })).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(licences.get(&ids[0]).await["revocation_reason"], "Refunded");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_customer_is_added_changed_and_removed(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    let ana = licences.customer("Ana", "ana@x.test").await;

    let reply = licences.patch(&id, json!({ "customer": ana })).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["customer"]["name"], "Ana", "the answer is expanded");
    let listed = licences.admin.get(&format!("/api/admin/licences?customer={ana}")).await;
    assert_eq!(listed.body["data"][0]["id"], id.as_str());

    let reply = licences.patch(&id, json!({ "customer": { "name": "Bea", "email": "bea@x.test" } })).await;
    assert_eq!(
        (reply.body["customer"]["name"].as_str(), reply.body["customer"]["email"].as_str()),
        (Some("Bea"), Some("bea@x.test"))
    );
    let bea = reply.body["customer"]["id"].as_str().unwrap().to_string();

    let reply = licences.patch(&id, json!({ "customer": null })).await;
    assert_eq!((reply.status, reply.body["customer"].clone()), (StatusCode::OK, Value::Null));
    assert_eq!(licences.admin.get(&format!("/api/admin/customers/{bea}")).await.status, StatusCode::OK, "Bea stays");

    assert_eq!(licences.audited("licence.updated").await, 3);
    assert_eq!(licences.audited("customer.created").await, 2, "Ana, made by the test, and Bea");
    let fields: Vec<Value> =
        sqlx::query_scalar("SELECT metadata -> 'fields' FROM audit_logs WHERE action = 'licence.updated'")
            .fetch_all(licences.admin.pool())
            .await
            .unwrap();
    assert!(fields.iter().all(|fields| *fields == json!(["customer"])), "{fields:?}");
    let metadata = licences.audit_metadata().await;
    assert!(!metadata.contains("Bea") && !metadata.contains("bea@x.test"), "{metadata}");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_bad_customer_changes_nothing(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let id = licences.issue(1, Value::Null).await.remove(0).0;
    licences.customer("Ana", "ana@x.test").await;
    let customers = licences.customers().await;

    let unknown = licences.patch(&id, json!({ "customer": Uuid::now_v7() })).await;
    assert_eq!((unknown.status, unknown.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    let taken = licences.patch(&id, json!({ "customer": { "name": "Other Ana", "email": "ana@x.test" } })).await;
    assert_eq!((taken.status, taken.error()), (StatusCode::CONFLICT, "customer_email_taken"));
    for bad in [
        json!({ "customer": 7 }),
        json!({ "customer": { "email": "x@x.test" } }),
        json!({ "customer": { "name": "" } }),
    ] {
        assert_eq!(licences.patch(&id, bad.clone()).await.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert_eq!(licences.get(&id).await["customer"], Value::Null);
    assert_eq!(licences.customers().await, customers);
    assert_eq!(licences.audited("licence.updated").await, 0);
}
