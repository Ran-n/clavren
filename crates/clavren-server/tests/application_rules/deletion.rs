// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:24.975966
//+ Revised: 	2026/11/14 12:05:24.975966
// -----------------------------------------------------------------------------

//! What deleting applications, customers and products leaves behind.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_applications_keeps_their_licences(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (approved, secret) = applying.applied(json!({})).await;
    let licence = applying.approve(&approved, json!({})).await.body["licence"]["id"].as_str().unwrap().to_string();
    let (pending, _) = applying.applied(json!({ "email": "bo@example.com" })).await;
    let missing = uuid::Uuid::now_v7().to_string();
    let reply = applying
        .licensing
        .admin_post("/api/admin/applications/bulk/delete", json!({ "ids": [approved, missing] }))
        .await;
    assert_eq!((reply.status, &reply.body["ids"]), (StatusCode::NOT_FOUND, &json!([missing])));
    let reply =
        applying.licensing.admin_post("/api/admin/applications/bulk/delete", json!({ "ids": [approved] })).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let deleted = applying.admin().call(Method::DELETE, &format!("/api/admin/applications/{pending}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(applying.admin().get(&format!("/api/admin/applications/{pending}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(applying.status(&secret).await.error(), "application_unknown");
    assert_eq!(applying.licensing.licence(&licence).await["state"], "unused", "the licence stays");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_a_customer_erases_what_they_applied_with(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (id, _) = applying.applied(json!({})).await;
    let approved =
        applying.approve(&id, json!({ "customer": { "name": "Ana López", "email": "ana@example.com" } })).await;
    let customer = approved.body["licence"]["customer"]["id"].as_str().unwrap().to_string();
    let (other, _) = applying.applied(json!({ "email": "bo@example.com" })).await;
    let deleted = applying.admin().call(Method::DELETE, &format!("/api/admin/customers/{customer}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    assert_eq!(applying.admin().get(&format!("/api/admin/applications/{id}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(applying.application(&other).await["state"], "pending", "someone else's stays");
    let found: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications WHERE email ILIKE 'ana%'")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(found, 0);
}

/// Regression: a waiting or rejected application, and an approved one whose licence was deleted
/// since, have no licence to find them by, so deleting the customer left the name and email they
/// applied with.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_a_customer_erases_their_applications_without_a_licence(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (rejected, _) = applying.applied(json!({ "email": "ANA@example.com" })).await;
    assert_eq!(applying.reject(&rejected, json!({})).await.status, StatusCode::OK);
    let (approved, _) = applying.applied(json!({})).await;
    let reply =
        applying.approve(&approved, json!({ "customer": { "name": "Ana López", "email": "ana@example.com" } })).await;
    let (licence, customer) = (id_of(&reply.body["licence"]), id_of(&reply.body["licence"]["customer"]));
    let gone = applying.admin().call(Method::DELETE, &format!("/api/admin/licences/{licence}"), None).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let (waiting, _) = applying.applied(json!({})).await;
    let (other, _) = applying.applied(json!({ "email": "bo@example.com" })).await;
    assert_eq!(applying.reject(&other, json!({})).await.status, StatusCode::OK);

    let deleted = applying.admin().call(Method::DELETE, &format!("/api/admin/customers/{customer}"), None).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT);
    for erased in [&rejected, &approved, &waiting] {
        let path = format!("/api/admin/applications/{erased}");
        assert_eq!(applying.admin().get(&path).await.status, StatusCode::NOT_FOUND);
    }
    assert_eq!(applying.application(&other).await["state"], "rejected", "someone else's stays");
    let left: Vec<String> =
        sqlx::query_scalar("SELECT email FROM license_applications").fetch_all(applying.admin().pool()).await.unwrap();
    assert_eq!(left, ["bo@example.com"]);
}

/// A customer without an email address has none to find applications by.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_a_customer_without_an_email_keeps_other_applications(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (rejected, _) = applying.applied(json!({})).await;
    assert_eq!(applying.reject(&rejected, json!({})).await.status, StatusCode::OK);
    let customer = applying.admin().create("/api/admin/customers", json!({ "name": "Ana López" })).await;
    let path = format!("/api/admin/customers/{}", id_of(&customer));
    assert_eq!(applying.admin().call(Method::DELETE, &path, None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(applying.application(&rejected).await["state"], "rejected");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_a_product_deletes_its_applications(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let (approved, _) = applying.applied(json!({})).await;
    applying.approve(&approved, json!({})).await;
    applying.applied(json!({ "email": "bo@example.com" })).await;
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    assert_eq!(applying.admin().call(Method::DELETE, &path, None).await.status, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM license_applications")
        .fetch_one(applying.admin().pool())
        .await
        .unwrap();
    assert_eq!(left, 0);
}
