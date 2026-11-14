// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:27.329064
//+ Revised: 	2026/11/14 12:05:27.329064
// -----------------------------------------------------------------------------

//! Key instructions, offline days and the price.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn key_instructions_round_trip_and_are_audited(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "avalio-lite", "Avalio Lite").await;
    assert_eq!(product["key_instructions"], Value::Null);
    let path = format!("/api/admin/products/{}", id_of(&product));
    let text = "Write to sales@example.com.

Schools: https://example.com/schools";
    let set = admin.call(Method::PATCH, &path, Some(json!({ "key_instructions": text }))).await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.body);
    assert_eq!(set.body["key_instructions"], text);
    assert_eq!(admin.get(&path).await.body["key_instructions"], text);
    let long = admin.call(Method::PATCH, &path, Some(json!({ "key_instructions": "x".repeat(2001) }))).await;
    assert_eq!((long.status, &long.body["error"]), (StatusCode::BAD_REQUEST, &json!("invalid_request")));
    let cleared = admin.call(Method::PATCH, &path, Some(json!({ "key_instructions": null }))).await;
    assert_eq!(cleared.body["key_instructions"], Value::Null);
    let updates: Vec<Value> =
        audit(&admin).await.into_iter().filter(|(action, ..)| action == "product.updated").map(|(.., m)| m).collect();
    assert_eq!(updates, [json!({ "fields": ["key_instructions"] }), json!({ "fields": ["key_instructions"] })]);
    let created = admin
        .create(
            "/api/admin/products",
            json!({ "code": "b", "name": "B", "key_instructions": "Ask Ana", "test_licence": false }),
        )
        .await;
    assert_eq!(created["key_instructions"], "Ask Ana");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn offline_days_start_at_the_server_default_and_change_within_2_to_3650(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "avalio-lite", "Avalio Lite").await;
    assert_eq!(product["offline_days"], 30, "CLAVREN_TOKEN_DAYS, 30 unless set");
    let week = admin
        .create(
            "/api/admin/products",
            json!({ "code": "lab", "name": "Lab", "offline_days": 7, "test_licence": false }),
        )
        .await;
    assert_eq!(week["offline_days"], 7);

    let path = format!("/api/admin/products/{}", id_of(&product));
    for days in [7, 30] {
        let changed = admin.call(Method::PATCH, &path, Some(json!({ "offline_days": days }))).await;
        assert_eq!((changed.status, &changed.body["offline_days"]), (StatusCode::OK, &json!(days)));
    }
    for bad in [json!(1), json!(3651), json!(2.5), Value::Null, json!("x")] {
        let refused = admin.call(Method::PATCH, &path, Some(json!({ "offline_days": bad }))).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(refused.body["error"], "invalid_request", "{bad}");
        assert!(refused.body["message"].as_str().unwrap().contains("offline_days"), "{bad}: {:?}", refused.body);
    }
    let renamed = admin.call(Method::PATCH, &path, Some(json!({ "name": "Avalio" }))).await;
    assert_eq!(renamed.body["offline_days"], 30, "left out, it stays");
    let updates: Vec<Value> =
        audit(&admin).await.into_iter().filter(|(action, ..)| action == "product.updated").map(|(.., m)| m).collect();
    assert_eq!(
        updates,
        [json!({ "fields": ["offline_days"] }), json!({ "fields": ["offline_days"] }), json!({ "fields": ["name"] })]
    );
    let bad_create = admin
        .call(
            Method::POST,
            "/api/admin/products",
            Some(json!({ "name": "X", "offline_days": 1, "test_licence": false })),
        )
        .await;
    assert_eq!(bad_create.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_price_is_set_changed_and_cleared(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "avalio-lite", "Avalio Lite").await;
    assert_eq!(product["price"], Value::Null);
    let path = format!("/api/admin/products/{}", id_of(&product));
    let price = json!({ "amount": 1999, "currency": "EUR" });
    let set = admin.call(Method::PATCH, &path, Some(json!({ "price": price }))).await;
    assert_eq!((set.status, &set.body["price"]), (StatusCode::OK, &price));
    assert_eq!(admin.get(&path).await.body["price"], price);
    let free = admin.call(Method::PATCH, &path, Some(json!({ "price": { "amount": 0, "currency": "EUR" } }))).await;
    assert_eq!(free.body["price"]["amount"], 0, "free is a price");
    let cleared = admin.call(Method::PATCH, &path, Some(json!({ "price": null }))).await;
    assert_eq!(cleared.body["price"], Value::Null);
    let updates: Vec<Value> =
        audit(&admin).await.into_iter().filter(|(action, ..)| action == "product.updated").map(|(.., m)| m).collect();
    assert_eq!(updates, vec![json!({ "fields": ["price"] }); 3]);
    let created = admin
        .create("/api/admin/products", json!({ "code": "b", "name": "B", "price": price, "test_licence": false }))
        .await;
    assert_eq!(created["price"], price);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_bad_price_is_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "avalio-lite", "Avalio Lite").await;
    let path = format!("/api/admin/products/{}", id_of(&product));
    for (bad, words) in [
        (json!({ "amount": -1, "currency": "EUR" }), "between 0 and 99999999"),
        (json!({ "amount": 100_000_000, "currency": "EUR" }), "between 0 and 99999999"),
        (json!({ "amount": 1, "currency": "eur" }), "three-letter ISO 4217 code"),
        (json!({ "amount": 1, "currency": "EURO" }), "three-letter ISO 4217 code"),
        (json!({ "amount": 19.99, "currency": "EUR" }), "amount"),
        (json!({ "amount": 1 }), "currency"),
    ] {
        let reply = admin.call(Method::PATCH, &path, Some(json!({ "price": bad }))).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
        assert!(reply.body["message"].as_str().unwrap().contains(words), "{bad}: {:?}", reply.body);
    }
    assert_eq!(admin.get(&path).await.body["price"], Value::Null);
}
