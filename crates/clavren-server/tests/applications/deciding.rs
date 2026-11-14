// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:25.219888
//+ Revised: 	2026/11/14 17:13:09.248573
// -----------------------------------------------------------------------------

//! Approving and rejecting, and what the applicant is then told.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn approving_issues_the_licence_the_applicant_collects(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (id, secret) = applying.applied(json!({})).await;
    let expires = (applying.licensing.now() + chrono::Duration::days(300)).to_rfc3339();
    let body = json!({
        "customer": { "name": "Ana López", "email": "ana@example.com" },
        "expires_at": expires,
        "internal_notes": "applied online",
    });
    let approved = applying.approve(&id, body).await;
    assert_eq!(approved.status, StatusCode::OK, "{:?}", approved.body);
    assert_eq!(approved.body["state"], "approved");
    let licence = &approved.body["licence"];
    assert_eq!((&licence["customer"]["email"], &licence["is_test"]), (&json!("ana@example.com"), &json!(false)));
    assert_eq!(licence["internal_notes"], "applied online");
    assert_eq!(approved.body["key_collected_at"], Value::Null);
    let listed = applying.admin().get("/api/admin/applications?state=approved").await;
    assert_eq!(listed.body["data"].as_array().unwrap().len(), 1, "{:?}", listed.body);
    assert_eq!(listed.body["data"][0]["id"].as_str(), Some(id.as_str()));

    let status = applying.status(&secret).await;
    assert_eq!(status.body["status"], "approved");
    let key = status.body["key"].as_str().unwrap();
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(status.body["licence_expires_at"].as_str().unwrap()).unwrap().to_rfc3339(),
        expires
    );
    let device = Device::new(7);
    let activated = device.activate(applying.licensing.app(), PRODUCT, key).await;
    assert_eq!(activated.status, StatusCode::CREATED, "the key works: {:?}", activated.body);
    let revealed = applying
        .licensing
        .admin_post(&format!("/api/admin/licences/{}/reveal", licence["id"].as_str().unwrap()), json!({}))
        .await;
    assert_eq!(revealed.body["key"], key);

    assert_eq!(applying.status(&secret).await.body["key"], key, "the key can be collected again");
    assert!(applying.application(&id).await["key_collected_at"].is_string());
    let actions: Vec<String> =
        audit_actions(applying.admin().pool()).await.into_iter().map(|(action, _)| action).collect();
    assert_eq!(actions, ["application.created", "application.approved", "application.key_collected"], "collected once");
    assert_eq!(applying.admin().get("/api/admin/overview").await.body["pending_applications"], 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_application_is_approved_for_an_existing_customer_or_none(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let customer =
        applying.admin().create("/api/admin/customers", json!({ "name": "Ana", "email": "ana@example.com" })).await;
    let (with_customer, _) = applying.applied(json!({})).await;
    let approved = applying.approve(&with_customer, json!({ "customer": customer["id"] })).await;
    assert_eq!(approved.body["licence"]["customer"]["id"], customer["id"]);
    let clash = applying.applied(json!({})).await.0;
    let taken = applying.approve(&clash, json!({ "customer": { "name": "Ana", "email": "Ana@Example.com" } })).await;
    assert_eq!((taken.status, taken.error()), (StatusCode::CONFLICT, "customer_email_taken"));
    assert_eq!(applying.application(&clash).await["state"], "pending", "nothing was issued");
    let without = applying.approve(&clash, json!({})).await;
    assert_eq!((without.status, &without.body["licence"]["customer"]), (StatusCode::OK, &Value::Null));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_application_is_decided_once(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (approved, _) = applying.applied(json!({})).await;
    applying.approve(&approved, json!({})).await;
    for reply in [applying.approve(&approved, json!({})).await, applying.reject(&approved, json!({})).await] {
        assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "application_decided"));
    }
    let licences: i64 = sqlx::query_scalar("SELECT count(*) FROM licenses WHERE NOT is_test")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(licences, 1);
    let missing = uuid::Uuid::now_v7();
    assert_eq!(applying.approve(&missing.to_string(), json!({})).await.status, StatusCode::NOT_FOUND);
    assert_eq!(applying.reject(&missing.to_string(), json!({})).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_rejection_tells_the_applicant_why(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (id, secret) = applying.applied(json!({})).await;
    let rejected = applying.reject(&id, json!({ "reason": " Only for schools " })).await;
    assert_eq!((rejected.status, &rejected.body["state"]), (StatusCode::OK, &json!("rejected")));
    assert_eq!(rejected.body["rejection_reason"], "Only for schools");
    let status = applying.status(&secret).await.body;
    assert_eq!(
        (&status["status"], &status["reason"], &status["key"]),
        (&json!("rejected"), &json!("Only for schools"), &Value::Null)
    );
    assert!(status["decided_at"].is_string());
    let too_long = applying.applied(json!({ "email": "bo@example.com" })).await.0;
    assert_eq!(applying.reject(&too_long, json!({ "reason": "x".repeat(501) })).await.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_revoked_or_deleted_licence_is_withdrawn(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (revoked, revoked_secret) = applying.applied(json!({})).await;
    let licence = applying.approve(&revoked, json!({})).await.body["licence"]["id"].as_str().unwrap().to_string();
    applying.licensing.admin_post(&format!("/api/admin/licences/{licence}/revoke"), json!({})).await;
    let status = applying.status(&revoked_secret).await.body;
    assert_eq!((&status["status"], &status["key"]), (&json!("withdrawn"), &Value::Null));

    let (deleted, deleted_secret) = applying.applied(json!({ "email": "bo@example.com" })).await;
    let licence = applying.approve(&deleted, json!({})).await.body["licence"]["id"].as_str().unwrap().to_string();
    applying.admin().call(Method::DELETE, &format!("/api/admin/licences/{licence}"), None).await;
    assert_eq!(applying.status(&deleted_secret).await.body["status"], "withdrawn");
    let application = applying.application(&deleted).await;
    assert_eq!((&application["state"], &application["licence"]), (&json!("approved"), &Value::Null));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_wrong_secret_names_no_application(pool: PgPool) {
    let applying = Applying::new(pool).await;
    applying.applied(json!({})).await;
    let random = clavren_protocol::encoding::encode(&[7; 32]);
    for secret in [random.as_str(), "short", "", "!!!"] {
        let reply = applying.status(secret).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "application_unknown"), "{secret:?}");
        assert!(reply.body["server_time"].is_string());
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn approving_records_no_price_unless_given(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    let price = json!({ "amount": 1999, "currency": "EUR" });
    applying.admin().call(Method::PATCH, &path, Some(json!({ "price": price }))).await;
    let (plain, _) = applying.applied(json!({})).await;
    let approved = applying.approve(&plain, json!({})).await;
    assert_eq!(approved.body["licence"]["price"], Value::Null, "an application is usually not a sale");
    let (sold, _) = applying.applied(json!({ "email": "bea@example.com" })).await;
    let approved = applying.approve(&sold, json!({ "price": price })).await;
    assert_eq!(approved.body["licence"]["price"], price);
}
