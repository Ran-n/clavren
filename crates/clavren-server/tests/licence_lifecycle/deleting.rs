// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:26.244269
//+ Revised: 	2026/11/14 12:05:26.244269
// -----------------------------------------------------------------------------

//! Deleting licences, one and many.

use super::*;

async fn rows_left(licences: &Licences, ids: &[String]) -> (i64, i64, i64) {
    let ids: Vec<Uuid> = ids.iter().map(|id| id.parse().unwrap()).collect();
    let count = |sql: &'static str| {
        let ids = ids.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).bind(ids).fetch_one(licences.admin.pool()).await.unwrap() }
    };
    (
        count("SELECT count(*) FROM licenses WHERE id = ANY($1)").await,
        count("SELECT count(*) FROM license_key_secrets WHERE license_id = ANY($1)").await,
        count("SELECT count(*) FROM activations WHERE license_id = ANY($1)").await,
    )
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_licence_is_deleted_in_any_state(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let issued = licences.issue(3, Value::Null).await;
    let ids: Vec<String> = issued.iter().map(|(id, _)| id.clone()).collect();
    licences.activate(&ids[1]).await;
    licences.post(&format!("/{}/revoke", ids[2]), json!({})).await;

    for id in &ids {
        let reply = licences.admin.call(Method::DELETE, &format!("/api/admin/licences/{id}"), None).await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
        assert_eq!(licences.admin.get(&format!("/api/admin/licences/{id}")).await.status, StatusCode::NOT_FOUND);
    }
    assert_eq!(rows_left(&licences, &ids).await, (0, 0, 0), "licence, key and history are gone");
    let was: Vec<(String, String)> = sqlx::query_as(
        "SELECT entity_id::text, metadata->>'was' FROM audit_logs WHERE action = 'licence.deleted' ORDER BY created_at",
    )
    .fetch_all(licences.admin.pool())
    .await
    .unwrap();
    let expected: Vec<(String, String)> =
        ids.iter().cloned().zip(["unused", "redeemed", "revoked"].map(String::from)).collect();
    assert_eq!(was, expected);

    let activation = json!({ "product": "avalio-lite", "key": issued[0].1, "device_public_key": clavren_protocol::encoding::encode(&[7u8; 32]), "device_name": "PC", "platform": "windows", "app_version": "1.0.0" });
    let reply = licences.admin.call(Method::POST, "/api/v1/activations", Some(activation)).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::NOT_FOUND, "invalid_key"), "its key reads as unknown");
    let again = licences.admin.call(Method::DELETE, &format!("/api/admin/licences/{}", ids[0]), None).await;
    assert_eq!((again.status, again.body.get("ids")), (StatusCode::NOT_FOUND, None));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bulk_delete_counts_and_is_all_or_nothing(pool: PgPool) {
    let licences = Licences::new(pool).await;
    let ids: Vec<String> = licences.issue(3, Value::Null).await.into_iter().map(|(id, _)| id).collect();
    licences.activate(&ids[0]).await;

    let dry = licences.post("/bulk/delete", json!({ "ids": ids, "dry_run": true })).await;
    assert_eq!((dry.status, dry.body.clone()), (StatusCode::OK, json!({ "deleted": 3, "in_use": 1 })));
    assert_eq!(rows_left(&licences, &ids).await, (3, 3, 1), "a dry run deletes nothing");
    assert_eq!(licences.audited("licence.deleted").await, 0);

    let ghost = Uuid::now_v7().to_string();
    let missing = licences.post("/bulk/delete", json!({ "ids": [ids[0], ghost] })).await;
    assert_eq!((missing.status, missing.body["ids"].clone()), (StatusCode::NOT_FOUND, json!([ghost])));
    assert_eq!(rows_left(&licences, &ids).await, (3, 3, 1), "nothing deleted");
    let too_many: Vec<String> = (0..1001).map(|_| Uuid::now_v7().to_string()).collect();
    for body in [
        json!({ "ids": [] }),
        json!({ "ids": too_many }),
        json!({ "ids": [ids[0], ids[0]] }),
        json!({ "ids": ids, "force": true }),
    ] {
        assert_eq!(licences.post("/bulk/delete", body).await.status, StatusCode::BAD_REQUEST);
    }

    let reply = licences.post("/bulk/delete", json!({ "ids": ids })).await;
    assert_eq!((reply.status, reply.body.clone()), (StatusCode::OK, json!({ "deleted": 3, "in_use": 1 })));
    assert_eq!(rows_left(&licences, &ids).await, (0, 0, 0));
    assert_eq!(licences.audited("licence.deleted").await, 3, "one entry per licence");
}
