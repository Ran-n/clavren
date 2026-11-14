// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:27.231573
//+ Revised: 	2026/11/14 12:05:27.231573
// -----------------------------------------------------------------------------

//! The list, unknown IDs, and what every endpoint needs.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn unknown_or_malformed_ids_are_not_found(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for path in [format!("/api/admin/products/{}", Uuid::now_v7()), "/api/admin/products/not-an-id".to_string()] {
        for (method, body) in
            [(Method::GET, None), (Method::PATCH, Some(json!({ "name": "X" }))), (Method::DELETE, None)]
        {
            let reply = admin.call(method.clone(), &path, body).await;
            assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "not_found"), "{method} {path}");
        }
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_list_pages_by_name_and_filters_by_state(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for (code, name) in [("c", "charlie"), ("a", "Alpha"), ("b", "bravo"), ("d", "Delta"), ("e", "echo")] {
        create(&admin, code, name).await;
    }
    let echo = admin.call(Method::GET, "/api/admin/products?limit=100", None).await.body["data"][4].clone();
    admin
        .call(Method::PATCH, &format!("/api/admin/products/{}", id_of(&echo)), Some(json!({ "is_active": false })))
        .await;

    let mut names = Vec::new();
    let mut path = "/api/admin/products?limit=2".to_string();
    loop {
        let reply = admin.call(Method::GET, &path, None).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        names.extend(
            reply.body["data"].as_array().unwrap().iter().map(|product| product["name"].as_str().unwrap().to_string()),
        );
        match reply.body["next_cursor"].as_str() {
            Some(cursor) => path = format!("/api/admin/products?limit=2&after={cursor}"),
            None => break,
        }
    }
    assert_eq!(names, ["Alpha", "bravo", "charlie", "Delta", "echo"]);

    // a cursor from a list ordered by ID alone would continue at the wrong place
    let entries = admin.call(Method::GET, "/api/admin/audit?limit=1", None).await;
    let foreign = entries.body["next_cursor"].as_str().expect("creating the products left audit entries");
    let mixed = admin.call(Method::GET, &format!("/api/admin/products?after={foreign}"), None).await;
    assert_eq!((mixed.status, mixed.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    let by_name = admin.call(Method::GET, "/api/admin/products?limit=1", None).await;
    let keyed = by_name.body["next_cursor"].as_str().unwrap();
    for list in ["licences", "activations", "applications", "audit"] {
        let mixed = admin.call(Method::GET, &format!("/api/admin/{list}?after={keyed}"), None).await;
        assert_eq!((mixed.status, mixed.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{list}");
    }

    let inactive = admin.call(Method::GET, "/api/admin/products?active=false", None).await;
    assert_eq!(inactive.body["data"].as_array().unwrap().len(), 1);
    assert_eq!(inactive.body["next_cursor"], Value::Null);
    for bad in ["?limit=0", "?limit=101", "?after=garbage", "?active=maybe", "?sort=name"] {
        let reply = admin.call(Method::GET, &format!("/api/admin/products{bad}"), None).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn every_endpoint_needs_a_session_and_changes_need_csrf(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = create(&admin, "a", "A").await;
    let path = format!("/api/admin/products/{}", id_of(&product));
    for (method, path) in [(Method::GET, "/api/admin/products"), (Method::GET, path.as_str())] {
        let reply = Call::new(method, path).send(&admin.fixture.app).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }
    for (method, path) in
        [(Method::POST, "/api/admin/products"), (Method::PATCH, path.as_str()), (Method::DELETE, path.as_str())]
    {
        let reply = Call::new(method.clone(), path).json(json!({})).send(&admin.fixture.app).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{method} {path}");
        let reply = Call::new(method.clone(), path)
            .signed_in(&admin.browser)
            .csrf("")
            .json(json!({}))
            .send(&admin.fixture.app)
            .await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}
