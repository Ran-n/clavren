// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:26.727029
//+ Revised: 	2026/11/14 12:05:26.727029
// -----------------------------------------------------------------------------

//! What each licence was charged.

use super::*;

async fn priced_product(admin: &Admin, code: &str, amount: i64) -> String {
    let id = product(admin, code).await;
    let path = format!("/api/admin/products/{id}");
    let reply =
        admin.call(Method::PATCH, &path, Some(json!({ "price": { "amount": amount, "currency": "EUR" } }))).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    id
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn issuing_takes_the_products_price_by_default(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = priced_product(&admin, "avalio-lite", 1999).await;
    let issued = issue(&admin, json!({ "product": product, "quantity": 2 })).await;
    for licence in issued["licences"].as_array().unwrap() {
        assert_eq!(licence["price"], json!({ "amount": 1999, "currency": "EUR" }));
    }
    let test = issue(&admin, json!({ "product": product, "test": true })).await;
    assert_eq!(test["licences"][0]["price"], Value::Null, "a test licence is never a sale");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn issuing_with_another_price_or_none(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let priced = priced_product(&admin, "avalio-lite", 1999).await;
    let other = json!({ "amount": 1500, "currency": "USD" });
    let issued = issue(&admin, json!({ "product": priced, "price": other })).await;
    assert_eq!(issued["licences"][0]["price"], other);
    let none = issue(&admin, json!({ "product": priced, "price": null })).await;
    assert_eq!(none["licences"][0]["price"], Value::Null);
    let unpriced = product(&admin, "free").await;
    assert_eq!(issue(&admin, json!({ "product": unpriced })).await["licences"][0]["price"], Value::Null);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_test_licence_with_a_price_is_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let body = json!({ "product": product, "test": true, "price": { "amount": 1, "currency": "EUR" } });
    let reply = admin.call(Method::POST, "/api/admin/licences", Some(body)).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    assert!(reply.body["message"].as_str().unwrap().contains("test licences can't have a price"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_licence_price_is_edited_in_place_and_in_bulk(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let id = issue(&admin, json!({ "product": product })).await["licences"][0]["id"].as_str().unwrap().to_string();
    let test = issue(&admin, json!({ "product": product, "test": true })).await["licences"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let price = json!({ "amount": 2500, "currency": "EUR" });
    let edited = admin.call(Method::PATCH, &format!("/api/admin/licences/{id}"), Some(json!({ "price": price }))).await;
    assert_eq!((edited.status, &edited.body["price"]), (StatusCode::OK, &price));
    let refused =
        admin.call(Method::PATCH, &format!("/api/admin/licences/{test}"), Some(json!({ "price": price }))).await;
    assert_eq!((refused.status, refused.error()), (StatusCode::CONFLICT, "test_licence_price"));

    let free = json!({ "amount": 0, "currency": "EUR" });
    let bulk = admin
        .call(Method::POST, "/api/admin/licences/bulk/edit", Some(json!({ "ids": [id, test], "price": free })))
        .await;
    assert_eq!(bulk.status, StatusCode::OK, "{:?}", bulk.body);
    assert_eq!((&bulk.body["updated"], &bulk.body["kept_test"]), (&json!(1), &json!(1)));
    assert_eq!(admin.get(&format!("/api/admin/licences/{id}")).await.body["price"], free);
    assert_eq!(admin.get(&format!("/api/admin/licences/{test}")).await.body["price"], Value::Null);
    let fields: Vec<Value> = sqlx::query_scalar(
        "SELECT metadata FROM audit_logs WHERE action = 'licence.updated' AND entity_id = $1 ORDER BY created_at, id",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .fetch_all(admin.pool())
    .await
    .unwrap();
    assert_eq!(fields, vec![json!({ "fields": ["price"] }); 2]);
    let cleared = admin.call(Method::PATCH, &format!("/api/admin/licences/{id}"), Some(json!({ "price": null }))).await;
    assert_eq!(cleared.body["price"], Value::Null);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_csv_ends_with_price_and_currency(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = priced_product(&admin, "avalio-lite", 1999).await;
    let priced = issue(&admin, json!({ "product": product })).await["licences"][0]["id"].clone();
    let unpriced = issue(&admin, json!({ "product": product, "price": null })).await["licences"][0]["id"].clone();
    let reply =
        admin.call(Method::POST, "/api/admin/licences/export", Some(json!({ "ids": [priced, unpriced] }))).await;
    let lines: Vec<&str> = reply.text.trim_end().split("\r\n").collect();
    assert!(lines[0].ends_with(",test,price,currency"), "{}", lines[0]);
    let priced_line = lines.iter().find(|line| line.starts_with(priced.as_str().unwrap())).unwrap();
    assert!(priced_line.ends_with(",false,19.99,EUR"), "{priced_line}");
    let unpriced_line = lines.iter().find(|line| line.starts_with(unpriced.as_str().unwrap())).unwrap();
    assert!(unpriced_line.ends_with(",false,,"), "{unpriced_line}");
}
