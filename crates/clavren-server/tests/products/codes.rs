// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 18:09:54.499995
//+ Revised: 	2026/11/14 18:09:54.499995
// -----------------------------------------------------------------------------

//! A product's code: made from its name when none is typed, unique, and fixed once it has
//! licences.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_code_is_made_from_the_name_when_none_is_given(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let typed = create(&admin, "typed", "Typed").await;
    let mut made = Vec::new();
    for body in [
        json!({ "name": "Ñandú Pro" }),
        json!({ "name": "Ñandú Pro", "code": "" }),
        json!({ "name": "Nandu pro", "code": "  " }),
        json!({ "name": "Nandu pro", "code": null }),
        json!({ "name": "!!!" }),
    ] {
        let reply = admin.call(Method::POST, "/api/admin/products", Some(body.clone())).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{body}: {:?}", reply.body);
        made.push(reply.body["code"].as_str().unwrap().to_string());
    }
    assert_eq!(made, ["nandu-pro", "nandu-pro-2", "nandu-pro-3", "nandu-pro-4", "product"]);
    let first = format!(
        "/api/admin/products/{}",
        id_of(
            &admin.get("/api/admin/products?limit=100").await.body["data"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["code"] == "nandu-pro")
                .unwrap()
                .clone()
        )
    );
    let renamed = admin.call(Method::PATCH, &first, Some(json!({ "name": "Something Else" }))).await;
    assert_eq!(renamed.body["code"], "nandu-pro", "renaming never changes the code");

    let audit = audit(&admin).await;
    let typed_entry = audit.iter().find(|(_, id, _)| id.map(|id| id.to_string()) == Some(id_of(&typed))).unwrap();
    assert_eq!(typed_entry.2, json!({}), "a typed code isn't marked");
    let marked = audit
        .iter()
        .filter(|(action, _, metadata)| action == "product.created" && *metadata == json!({ "automatic_code": true }))
        .count();
    assert_eq!(marked, 5);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_name_whose_thousand_codes_are_all_taken_is_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    create(&admin, "acme", "Acme").await;
    // acme-2 to acme-1000: every code a product named Acme could be given
    sqlx::query(
        r"INSERT INTO products
          SELECT (jsonb_populate_record(p, jsonb_build_object('id', gen_random_uuid(), 'code', 'acme-' || n))).*
          FROM products p, generate_series(2, 1000) n",
    )
    .execute(admin.pool())
    .await
    .unwrap();
    let body = json!({ "name": "Acme", "test_licence": false });
    let reply = admin.call(Method::POST, "/api/admin/products", Some(body)).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_taken"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_typed_code_that_is_taken_is_still_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    create(&admin, "avalio", "Avalio").await;
    let reply =
        admin.call(Method::POST, "/api/admin/products", Some(json!({ "code": "avalio", "name": "Other" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_taken"), "never suffixed silently");
    assert_eq!(audit(&admin).await.len(), 1, "no second entry");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_code_is_locked_once_the_product_has_licences(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "avalio-lite", "Avalio Lite").await;
    create(&admin, "other", "Other").await;
    let path = format!("/api/admin/products/{}", id_of(&product));

    let reply = admin.call(Method::PATCH, &path, Some(json!({ "code": "other" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_taken"));
    let reply = admin.call(Method::PATCH, &path, Some(json!({ "code": "avalio" }))).await;
    assert_eq!(reply.body["code"], "avalio", "free to change without licences");

    insert_licence(&admin, &product).await;
    let reply = admin.call(Method::PATCH, &path, Some(json!({ "code": "avalio-lite" }))).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "product_code_locked"));
    let reply = admin.call(Method::PATCH, &path, Some(json!({ "name": "Avalio" }))).await;
    assert_eq!(reply.status, StatusCode::OK, "everything else stays editable");
}
