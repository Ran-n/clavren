// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:26.148552
//+ Revised: 	2026/11/14 12:05:26.148552
// -----------------------------------------------------------------------------

//! Showing a key again: reveal, message and the CSV export.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn reveal_and_message_show_the_key_and_are_audited(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let (id, key) = licences.issue(1, json!({ "name": "Ana" })).await.remove(0);
    let (bare, bare_key) = licences.issue(1, Value::Null).await.remove(0);

    let revealed = licences.post(&format!("/{id}/reveal"), json!({})).await;
    assert_eq!(revealed.body["key"], key.as_str());
    let message = licences.post(&format!("/{id}/message"), json!({})).await;
    assert_eq!(message.body["message"], format!("Dear Ana: {key} (Avalio Lite)"));
    let message = licences.post(&format!("/{bare}/message"), json!({})).await;
    assert_eq!(message.body["message"], format!("Dear : {bare_key} (Avalio Lite)"));

    licences.patch(&bare, json!({ "customer": { "name": "Bea" } })).await;
    let message = licences.post(&format!("/{bare}/message"), json!({})).await;
    assert_eq!(message.body["message"], format!("Dear Bea: {bare_key} (Avalio Lite)"), "a customer added later");

    assert_eq!(
        (licences.audited("licence.key_revealed").await, licences.audited("licence.message_copied").await),
        (1, 3)
    );
    let unknown = licences.post(&format!("/{}/reveal", Uuid::now_v7()), json!({})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_export_is_safe_csv_with_the_keys_still_held(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let (ana, ana_key) = licences.issue(1, json!({ "name": "López, Ana", "email": "ana@example.com" })).await.remove(0);
    let (revoked, _) = licences.issue(1, Value::Null).await.remove(0);
    licences
        .admin
        .call(
            Method::PATCH,
            &format!("/api/admin/licences/{ana}"),
            Some(json!({ "internal_notes": "=HYPERLINK(\"x\")" })),
        )
        .await;
    licences.post(&format!("/{revoked}/revoke"), json!({})).await;

    let reply = licences.post("/export", json!({ "ids": [ana, revoked] })).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    assert_eq!(reply.headers[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    assert!(
        reply.headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"clavren-licences-")
    );
    let lines: Vec<&str> = reply.text.split("\r\n").collect();
    assert_eq!(
        lines[0],
        "licence_id,product_code,customer_name,customer_email,key,state,issued_at,expires_at,notes,test,price,currency"
    );
    let ana_line = lines.iter().find(|line| line.starts_with(&ana)).unwrap();
    assert!(ana_line.contains(&format!(",avalio-lite,\"López, Ana\",ana@example.com,{ana_key},unused,")), "{ana_line}");
    assert!(ana_line.ends_with(",\"'=HYPERLINK(\"\"x\"\")\",false,,"), "{ana_line}");
    let revoked_line = lines.iter().find(|line| line.starts_with(&revoked)).unwrap();
    assert!(revoked_line.contains(",,,,revoked,"), "no key left: {revoked_line}");
    assert_eq!(licences.audited("licence.exported").await, 2);

    let by_filter = licences.post("/export", json!({ "filter": { "state": "unused" } })).await;
    assert_eq!(by_filter.text.split("\r\n").filter(|line| !line.is_empty()).count(), 2, "header and Ana");
    for body in [
        json!({}),
        json!({ "ids": [ana], "filter": {} }),
        json!({ "ids": [] }),
        json!({ "filter": { "state": "lost" } }),
    ] {
        assert_eq!(licences.post("/export", body.clone()).await.status, StatusCode::BAD_REQUEST, "{body}");
    }
    let missing = licences.post("/export", json!({ "ids": [Uuid::now_v7()] })).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

/// Regression: a licence that isn't revoked but had lost its stored key was exported with an
/// empty key cell, like a revoked one.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_export_fails_when_a_licence_that_isnt_revoked_has_no_key(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let (id, _) = licences.issue(1, Value::Null).await.remove(0);
    sqlx::query("DELETE FROM license_key_secrets WHERE license_id = $1")
        .bind(id.parse::<Uuid>().unwrap())
        .execute(licences.admin.pool())
        .await
        .unwrap();
    let reply = licences.post("/export", json!({ "ids": [id] })).await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR, "{}", reply.text);
    assert!(!reply.text.contains("licence_id,"), "no CSV: {}", reply.text);
    assert_eq!(licences.audited("licence.exported").await, 0);
}
