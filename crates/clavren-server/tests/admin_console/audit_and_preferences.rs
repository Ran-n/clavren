// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:24.193082
//+ Revised: 	2026/11/14 12:05:24.193082
// -----------------------------------------------------------------------------

//! The audit log and the saved preferences.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_audit_log_filters_and_names_admins(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, _) = licensing.issue(json!({})).await;
    licensing.admin_post(&format!("/api/admin/licences/{licence}/revoke"), json!({})).await;

    let all = licensing.admin.get("/api/admin/audit?limit=100").await.body;
    let actions: Vec<&str> =
        all["data"].as_array().unwrap().iter().map(|entry| entry["action"].as_str().unwrap()).collect();
    assert_eq!(&actions[..3], ["licence.revoked", "licence.issued", "product.created"], "newest first");
    assert_eq!(all["data"][0]["ip_address"], "198.51.100.10");

    let licence_entries = licensing.admin.get("/api/admin/audit?action=licence.").await.body;
    assert_eq!(licence_entries["data"].as_array().unwrap().len(), 2);
    for literal in ["%.", "_icence.", "licence"] {
        let reply = licensing.admin.get(&format!("/api/admin/audit?action={}", literal.replace('%', "%25"))).await;
        assert_eq!(
            reply.body["data"].as_array().unwrap().len(),
            0,
            "{literal}: no wildcards, and no family without a dot"
        );
    }
    let one = licensing
        .admin
        .get(&format!("/api/admin/audit?entity={licence}&action=licence.revoked&expand=admin_user"))
        .await
        .body;
    assert_eq!(one["data"][0]["admin_user"]["username"], "ran");
    let none = licensing
        .admin
        .get(&format!(
            "/api/admin/audit?from={}",
            urlencode(&(licensing.now() + chrono::Duration::hours(1)).to_rfc3339())
        ))
        .await
        .body;
    assert!(none["data"].as_array().unwrap().is_empty());
    assert_eq!(licensing.admin.get("/api/admin/audit?from=yesterday").await.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn preferences_are_saved_restored_and_checked(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let defaults = licensing.admin.get("/api/admin/preferences").await.body;
    assert_eq!(defaults, json!({ "version": 1, "theme": "system", "tables": {} }));

    let chosen = json!({
        "version": 1,
        "theme": "dark",
        "last_page": "/licences",
        "tables": { "licences": {
            "filters": { "state": "unused" }, "search": "López", "order": "expiry", "page_size": 25, "columns": ["key", "customer"],
            "issue": { "open": true, "product": licensing.product_id, "quantity": 3, "expires_at": "2027-12-31" }
        } }
    });
    let saved = licensing.admin.call(Method::PUT, "/api/admin/preferences", Some(chosen.clone())).await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.body);
    assert_eq!(licensing.admin.get("/api/admin/preferences").await.body, chosen, "restored on the next visit");

    for table in ["licences", "customers", "products", "activations", "applications", "audit", "backups"] {
        let remembered =
            json!({ "version": 1, "tables": { table: { "filters": { "state": "pending" }, "page_size": 100 } } });
        let saved = licensing.admin.call(Method::PUT, "/api/admin/preferences", Some(remembered)).await;
        assert_eq!(saved.status, StatusCode::OK, "{table}: {:?}", saved.body);
    }
    let saved = licensing.admin.call(Method::PUT, "/api/admin/preferences", Some(chosen.clone())).await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.body);
    for bad in [
        json!({ "version": 1, "tables": { "payments": { "page_size": 25 } } }),
        json!({ "version": 1, "tables": { "licences": { "page_size": 101 } } }),
        json!({ "version": 1, "theme": "neon" }),
        json!({ "version": 1, "tables": { "licences": { "search": "BRN-79YN-44XQ-W025-377Z" } } }),
        json!({ "version": 1, "tables": { "licences": { "page_size": 5 } } }),
        json!({ "version": 3 }),
        json!({ "version": 1, "colour": "red" }),
        json!({ "version": 1, "tables": { "customers": { "issue": { "open": true } } } }),
        json!({ "version": 1, "tables": { "licences": { "issue": { "open": true, "quantity": 0 } } } }),
        json!({ "version": 1, "tables": { "licences": { "issue": { "open": true, "notes": "School order" } } } }),
        json!({ "version": 1, "tables": { "licences": { "issue": { "open": true, "customer": "Ana López" } } } }),
        json!({ "version": 1, "tables": { "licences": { "issue": { "open": true, "expires_at": "someday" } } } }),
    ] {
        let reply = licensing.admin.call(Method::PUT, "/api/admin/preferences", Some(bad.clone())).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
    assert_eq!(licensing.admin.get("/api/admin/preferences").await.body, chosen, "nothing invalid was saved");
}
