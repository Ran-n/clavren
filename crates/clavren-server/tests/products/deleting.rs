// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:27.135661
//+ Revised: 	2026/11/14 12:05:27.135661
// -----------------------------------------------------------------------------

//! Deleting a product, with and without licences.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_product_without_licences_is_deleted(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let unused = create(&admin, "unused", "Unused").await;
    let usage = admin.get(&format!("/api/admin/products/{}/usage", id_of(&unused))).await;
    assert_eq!((usage.status, usage.body), (StatusCode::OK, json!({ "licences": 0, "in_use": 0 })));
    let reply = admin.call(Method::DELETE, &format!("/api/admin/products/{}", id_of(&unused)), None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = admin.call(Method::GET, &format!("/api/admin/products/{}", id_of(&unused)), None).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "not_found"));
    let (action, _, metadata) = audit(&admin).await.pop().unwrap();
    assert_eq!((action.as_str(), metadata), ("product.deleted", json!({ "licences": 0, "in_use": 0 })));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_product_is_deleted_with_its_licences(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let doomed = create(&admin, "doomed", "Doomed").await;
    let other = create(&admin, "other", "Other").await;
    let issue = |product: &Value, quantity: usize, customer: Value| {
        let body = json!({ "product": id_of(product), "quantity": quantity, "customer": customer });
        let admin = &admin;
        async move { admin.create("/api/admin/licences", body).await }
    };
    let issued = issue(&doomed, 3, json!({ "name": "Ana" })).await;
    let ids: Vec<Uuid> =
        issued["licences"].as_array().unwrap().iter().map(|l| l["id"].as_str().unwrap().parse().unwrap()).collect();
    let kept = issue(&other, 1, Value::Null).await;
    sqlx::query(
        "INSERT INTO activations (license_id, device_public_key, device_name, platform, app_version)
         VALUES ($1, $2, 'PC', 'windows', '1.0.0')",
    )
    .bind(ids[1])
    .bind([9u8; 32].as_slice())
    .execute(admin.pool())
    .await
    .unwrap();
    admin.call(Method::POST, &format!("/api/admin/licences/{}/revoke", ids[2]), Some(json!({}))).await;

    let path = format!("/api/admin/products/{}", id_of(&doomed));
    let usage = admin.get(&format!("{path}/usage")).await;
    assert_eq!(usage.body, json!({ "licences": 3, "in_use": 1 }));
    let reply = admin.call(Method::DELETE, &path, None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);

    let left = |sql: &'static str| {
        let ids = ids.clone();
        let admin = &admin;
        async move { sqlx::query_scalar::<_, i64>(sql).bind(ids).fetch_one(admin.pool()).await.unwrap() }
    };
    assert_eq!(left("SELECT count(*) FROM licenses WHERE id = ANY($1)").await, 0);
    assert_eq!(left("SELECT count(*) FROM license_key_secrets WHERE license_id = ANY($1)").await, 0);
    assert_eq!(left("SELECT count(*) FROM activations WHERE license_id = ANY($1)").await, 0);
    assert_eq!(admin.get(&path).await.status, StatusCode::NOT_FOUND);
    let kept_id = kept["licences"][0]["id"].as_str().unwrap();
    assert_eq!(
        admin.get(&format!("/api/admin/licences/{kept_id}")).await.status,
        StatusCode::OK,
        "other products keep theirs"
    );
    let customer = issued["customer"]["id"].as_str().unwrap();
    assert_eq!(admin.get(&format!("/api/admin/customers/{customer}")).await.status, StatusCode::OK, "customers stay");

    let (action, _, metadata) = audit(&admin).await.pop().unwrap();
    assert_eq!((action.as_str(), metadata), ("product.deleted", json!({ "licences": 3, "in_use": 1 })));
    let licences_deleted: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'licence.deleted'")
        .fetch_one(admin.pool())
        .await
        .unwrap();
    assert_eq!(licences_deleted, 3);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn deleting_or_counting_an_unknown_product_is_404(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let path = format!("/api/admin/products/{}", Uuid::now_v7());
    assert_eq!(admin.call(Method::DELETE, &path, None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(admin.get(&format!("{path}/usage")).await.status, StatusCode::NOT_FOUND);
}
