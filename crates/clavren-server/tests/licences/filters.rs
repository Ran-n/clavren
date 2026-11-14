// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:26.535774
//+ Revised: 	2026/11/14 17:13:09.604075
// -----------------------------------------------------------------------------

//! Searching, filtering and excluding in the list and the export.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn search_finds_a_key_however_it_is_typed_and_names_and_ids(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "avalio-lite").await;
    let ana = issue(
        &admin,
        json!({ "product": product, "customer": { "name": "Ana López", "email": "ana@school.example" } }),
    )
    .await;
    issue(&admin, json!({ "product": product, "customer": { "name": "Bruno" } })).await;
    let ana_licence = &ana["licences"][0];
    let key = ana_licence["key"].as_str().unwrap();

    let sloppy = key.to_lowercase().replace('-', " ").replace('0', "o");
    for query in
        [key.to_string(), sloppy, "lópez".into(), "SCHOOL".into(), ana_licence["id"].as_str().unwrap().to_string()]
    {
        let found = list(&admin, &format!("?q={}", urlencode(&query))).await;
        assert_eq!(found.len(), 1, "{query}");
        assert_eq!(found[0]["id"], ana_licence["id"], "{query}");
    }
    assert!(list(&admin, "?q=BRN-0000-0000-0000-0000").await.is_empty());
    assert_eq!(list(&admin, "?q=%20").await.len(), 2);
}

/// Four licences: Ana's (avalio, expiring in 10 days), one of avalio expiring in 100 days, and two
/// permanent ones of `other` without a customer, the second of them revoked.
struct Mixed {
    avalio: String,
    other: String,
    ana: Value,
    permanent: Value,
    revoked_id: String,
}

impl Mixed {
    async fn new(admin: &Admin) -> Self {
        let (avalio, other) = (product(admin, "avalio-lite").await, product(admin, "other").await);
        let now = admin.fixture.clock.now();
        let soon = (now + chrono::Duration::days(10)).to_rfc3339();
        let later = (now + chrono::Duration::days(100)).to_rfc3339();
        let ana = issue(admin, json!({ "product": avalio, "customer": { "name": "Ana" }, "expires_at": soon })).await;
        issue(admin, json!({ "product": avalio, "expires_at": later })).await;
        let permanent = issue(admin, json!({ "product": other })).await;
        let revoked = issue(admin, json!({ "product": other })).await;
        let revoked_id = revoked["licences"][0]["id"].as_str().unwrap().to_string();
        admin.call(Method::POST, &format!("/api/admin/licences/{revoked_id}/revoke"), Some(json!({}))).await;
        Self { avalio, other, ana, permanent, revoked_id }
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn filters_narrow_by_product_customer_state_and_expiry(pool: PgPool) {
    let mut admin = Admin::new(pool).await;
    let Mixed { avalio, other, ana, permanent, revoked_id } = Mixed::new(&admin).await;

    assert_eq!(list(&admin, &format!("?product={avalio}")).await.len(), 2);
    assert_eq!(list(&admin, &format!("?customer={}", ana["customer"]["id"].as_str().unwrap())).await.len(), 1);
    assert_eq!(list(&admin, "?state=revoked").await[0]["id"], revoked_id);
    assert_eq!(list(&admin, "?state=unused").await.len(), 3);
    assert_eq!(list(&admin, "?expiry=expiring").await[0]["id"], ana["licences"][0]["id"]);
    assert_eq!(list(&admin, "?expiry=permanent").await.len(), 2);
    let permanent_id = &permanent["licences"][0]["id"];
    assert_eq!(list(&admin, &format!("?expiry=permanent&state=unused&product={other}")).await[0]["id"], *permanent_id);

    // a day after the first one expired
    admin.wait(Duration::from_secs(11 * 24 * 3600)).await;
    let expired = list(&admin, "?expiry=expired").await;
    assert_eq!((expired.len(), &expired[0]["id"]), (1, &ana["licences"][0]["id"]));
    for bad in ["?state=lost", "?expiry=soon", "?product=nope", "?order=random", "?expand=owner", "?colour=red"] {
        let reply = admin.get(&format!("/api/admin/licences{bad}")).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn exclusions_hide_what_they_name(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let mixed = Mixed::new(&admin).await;
    let ids = |licences: Vec<Value>| licences.iter().map(|l| l["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let ana_id = mixed.ana["licences"][0]["id"].as_str().unwrap().to_string();
    let ana_key = mixed.ana["licences"][0]["key"].as_str().unwrap();

    let live = ids(list(&admin, "?not_state=revoked").await);
    assert_eq!(live.len(), 3);
    assert!(!live.contains(&mixed.revoked_id));
    let avalio = list(&admin, &format!("?not_product={}", mixed.other)).await;
    assert!(avalio.len() == 2 && avalio.iter().all(|l| l["product"] == mixed.avalio.as_str()), "{avalio:?}");
    assert_eq!(list(&admin, "?not_expiry=permanent").await.len(), 2);
    let without_ana =
        ids(list(&admin, &format!("?not_customer={}", mixed.ana["customer"]["id"].as_str().unwrap())).await);
    assert_eq!(without_ana.len(), 3, "licences without a customer aren't hidden");
    assert!(!without_ana.contains(&ana_id));
    assert_eq!(list(&admin, "?not_q=ana").await.len(), 3);
    for hide in [ana_key, ana_id.as_str()] {
        let shown = ids(list(&admin, &format!("?not_q={}", urlencode(hide))).await);
        assert_eq!(shown.len(), 3, "{hide}");
        assert!(!shown.contains(&ana_id), "{hide}");
    }
    assert_eq!(list(&admin, &format!("?state=unused&not_product={}", mixed.other)).await.len(), 2);
    assert_eq!(list(&admin, "?not_q=%20%20").await.len(), 4, "blank hides nothing");
    assert_eq!(list(&admin, "?state=revoked&not_state=revoked").await.len(), 0, "consistent, just empty");
    assert_eq!(list(&admin, &format!("?q=ana&not_q={}", urlencode(ana_key))).await.len(), 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn bad_exclusions_are_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    for bad in ["?not_state=lost", "?not_expiry=soon", "?not_product=nope", "?not_customer=nope"] {
        let reply = admin.get(&format!("/api/admin/licences{bad}")).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{bad}");
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn export_honours_exclusions(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let mixed = Mixed::new(&admin).await;
    let body = json!({ "filter": { "not_state": "revoked" } });
    let reply = admin.call(Method::POST, "/api/admin/licences/export", Some(body)).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text);
    let rows: Vec<&str> = reply.text.split("\r\n").skip(1).filter(|line| !line.is_empty()).collect();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| !row.contains(",revoked,") && !row.starts_with(&mixed.revoked_id)), "{rows:?}");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_export_of_more_than_ten_thousand_licences_is_refused(pool: PgPool) {
    let admin = Admin::new(pool).await;
    let product = product(&admin, "busy").await;
    issue(&admin, json!({ "product": product })).await;
    // 10,000 more like it, each with a lookup hash of its own
    sqlx::query(
        r"INSERT INTO licenses
          SELECT (jsonb_populate_record(l, jsonb_build_object(
              'id', gen_random_uuid(), 'key_lookup', '\x' || md5(n::text) || md5('again' || n::text)))).*
          FROM licenses l, generate_series(1, 10000) n",
    )
    .execute(admin.pool())
    .await
    .unwrap();
    let everything = admin.call(Method::POST, "/api/admin/licences/export", Some(json!({ "filter": {} }))).await;
    assert_eq!((everything.status, everything.error()), (StatusCode::BAD_REQUEST, "invalid_request"));
    assert!(everything.body["message"].as_str().unwrap().contains("more than 10000"), "{:?}", everything.body);
}
