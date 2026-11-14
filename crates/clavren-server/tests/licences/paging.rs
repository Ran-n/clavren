// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:26.629803
//+ Revised: 	2026/11/14 12:05:26.629803
// -----------------------------------------------------------------------------

//! Pages in either order, and related records expanded.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn pages_follow_either_order_to_the_end(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let now = admin.fixture.clock.now();
    for days in [30, 10, 20] {
        issue(&admin, json!({ "product": product, "expires_at": (now + chrono::Duration::days(days)).to_rfc3339() }))
            .await;
    }
    issue(&admin, json!({ "product": product, "quantity": 2 })).await;
    let revoked = issue(&admin, json!({ "product": product })).await["licences"][0]["id"].as_str().unwrap().to_string();
    admin.call(Method::POST, &format!("/api/admin/licences/{revoked}/revoke"), Some(json!({}))).await;

    let walk = |query: &'static str| {
        let admin = &admin;
        async move {
            let mut seen = Vec::new();
            let mut page = format!("?{query}&limit=2");
            loop {
                let reply = admin.get(&format!("/api/admin/licences{page}")).await;
                assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
                seen.extend(reply.body["data"].as_array().unwrap().iter().cloned());
                let Some(cursor) = reply.body["next_cursor"].as_str() else { return seen };
                page = format!("?{query}&limit=2&after={cursor}");
            }
        }
    };
    let days = |licences: &[Value]| -> Vec<Option<i64>> {
        licences
            .iter()
            .map(|licence| {
                licence["expires_at"]
                    .as_str()
                    .map(|at| (chrono::DateTime::parse_from_rfc3339(at).unwrap().to_utc() - now).num_days())
            })
            .collect()
    };
    assert_eq!(days(&walk("order=expiry&not_state=revoked").await), [Some(10), Some(20), Some(30), None, None]);
    assert_eq!(walk("order=expiry").await.len(), 6);
    assert_eq!(walk("order=newest").await.len(), 6);
    let live = walk("order=newest&not_state=revoked").await;
    let distinct: std::collections::HashSet<&str> = live.iter().map(|l| l["id"].as_str().unwrap()).collect();
    assert_eq!((live.len(), distinct.len()), (5, 5), "no repeats");
    assert!(!distinct.contains(revoked.as_str()));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn related_records_expand_on_request(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let issued = issue(&admin, json!({ "product": product, "customer": { "name": "Ana" } })).await;
    let id = issued["licences"][0]["id"].as_str().unwrap();

    let plain = admin.get(&format!("/api/admin/licences/{id}")).await.body;
    assert_eq!((plain["product"].as_str(), plain["active_activation"].clone()), (Some(product.as_str()), Value::Null));
    let keys: Vec<&str> = plain.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "active_activation",
            "active_computers",
            "created_at",
            "customer",
            "expires_at",
            "id",
            "internal_notes",
            "is_test",
            "price",
            "product",
            "revocation_reason",
            "revoked_at",
            "state"
        ]
    );

    let expanded = admin.get(&format!("/api/admin/licences/{id}?expand=product&expand=customer")).await.body;
    assert_eq!(expanded["product"]["code"], "avalio-lite");
    assert_eq!(expanded["customer"]["name"], "Ana");
    let listed = list(&admin, "?expand=product,customer,active_activation").await;
    assert_eq!(listed[0]["product"]["id"], product.as_str());
    assert_eq!(admin.get(&format!("/api/admin/licences/{}", Uuid::now_v7())).await.status, StatusCode::NOT_FOUND);
}
