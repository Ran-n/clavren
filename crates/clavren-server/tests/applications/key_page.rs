// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:25.320607
//+ Revised: 	2026/11/14 12:05:25.320607
// -----------------------------------------------------------------------------

//! A product's key page.

use super::*;

async fn key_page(applying: &Applying, code: &str) -> common::api::Reply {
    common::api::Call::new(Method::GET, &format!("/api/v1/applications/products/{code}"))
        .send(applying.licensing.app())
        .await
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_key_page_says_how_to_get_a_key_for_an_active_product(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let admin = applying.admin();
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    let text = "Buy one at https://example.com/shop";
    admin.call(Method::PATCH, &path, Some(json!({ "key_instructions": text, "description": "Marks" }))).await;

    let open = key_page(&applying, PRODUCT).await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.body);
    assert_eq!(open.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!((&open.body["code"], &open.body["name"]), (&json!(PRODUCT), &json!("Avalio Lite")));
    assert_eq!((&open.body["description"], &open.body["key_instructions"]), (&json!("Marks"), &json!(text)));
    assert_eq!(open.body["accepts_applications"], true);
    assert!(open.body["server_time"].is_string());

    admin.call(Method::PATCH, &path, Some(json!({ "accepts_applications": false }))).await;
    let closed = key_page(&applying, PRODUCT).await.body;
    assert_eq!((&closed["accepts_applications"], &closed["key_instructions"]), (&json!(false), &json!(text)));

    admin.call(Method::PATCH, &path, Some(json!({ "key_instructions": null }))).await;
    let bare = key_page(&applying, PRODUCT).await.body;
    assert_eq!((&bare["accepts_applications"], &bare["key_instructions"]), (&json!(false), &Value::Null));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_inactive_or_unknown_product_has_no_key_page(pool: PgPool) {
    let applying = Applying::new(pool).await;
    let path = format!("/api/admin/products/{}", applying.licensing.product_id);
    applying.admin().call(Method::PATCH, &path, Some(json!({ "is_active": false }))).await;
    let (logs, _guard) = capture_logs();
    let mut bodies = Vec::new();
    for code in [PRODUCT, "no-such-product"] {
        let reply = key_page(&applying, code).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{code}");
        assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
        assert!(reply.body["server_time"].is_string());
        let mut body = reply.body.clone();
        body.as_object_mut().unwrap().remove("server_time");
        bodies.push(body);
    }
    assert_eq!(bodies[0], bodies[1], "inactive and unknown read the same");
    assert_eq!(bodies[0]["error"], "product_unavailable");
    assert!(logs.text().contains("key page refused"), "{}", logs.text());
}
