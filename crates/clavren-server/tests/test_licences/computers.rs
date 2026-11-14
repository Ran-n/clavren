// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:28.059651
//+ Revised: 	2026/11/14 12:05:28.059651
// -----------------------------------------------------------------------------

//! A test licence on several computers.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_test_licence_works_on_several_computers_and_its_tokens_say_so(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = test_licence(&licensing).await;
    let now = licensing.now().timestamp();
    let mut activations = Vec::new();
    for seed in 1..=3 {
        let device = Device::new(seed);
        let reply = device.activate(licensing.app(), PRODUCT, &key).await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
        assert!(verified(&reply, PRODUCT, now).test);
        activations.push(reply.body["activation"].as_str().unwrap().to_string());
    }
    let distinct: std::collections::HashSet<&String> = activations.iter().collect();
    assert_eq!(distinct.len(), 3, "one activation per computer");

    let again = Device::new(2).activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!(again.status, StatusCode::OK, "the same computer retrying gets its activation back");
    assert_eq!(again.body["activation"], activations[1].as_str());

    let checked = Device::new(1).check(licensing.app(), &activations[0]).await;
    assert_eq!(checked.status, StatusCode::OK, "{:?}", checked.body);
    assert!(verified(&checked, PRODUCT, now).test);

    let computers = |query: &'static str| {
        let admin = &licensing.admin;
        async move { admin.get(&format!("/api/admin/activations{query}")).await.body["data"].as_array().unwrap().len() }
    };
    assert_eq!((computers("?test=true").await, computers("?test=false").await, computers("").await), (3, 0, 3));

    let view = licensing.licence(&licence).await;
    assert_eq!((&view["state"], &view["active_computers"]), (&json!("redeemed"), &json!(3)));
    assert_eq!(view["active_activation"]["id"], activations[2].as_str(), "the latest computer");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn an_ordinary_licence_stays_on_one_computer_and_its_tokens_say_nothing(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, key) = licensing.issue(json!({})).await;
    let first = Device::new(1).activate(licensing.app(), PRODUCT, &key).await;
    assert!(!verified(&first, PRODUCT, licensing.now().timestamp()).test);
    let claims = clavren_protocol::token::unverified_claims(first.body["token"].as_str().unwrap()).unwrap();
    assert!(!claims.test);
    let second = Device::new(2).activate(licensing.app(), PRODUCT, &key).await;
    assert_eq!((second.status, second.error()), (StatusCode::CONFLICT, "already_activated"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn ending_one_computer_leaves_the_others_and_a_reset_ends_them_all(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = test_licence(&licensing).await;
    let first = activate(&licensing, &Device::new(1), &key).await;
    let second = activate(&licensing, &Device::new(2), &key).await;
    let third = activate(&licensing, &Device::new(3), &key).await;

    let ended = licensing.admin_post(&format!("/api/admin/activations/{first}/revoke"), json!({})).await;
    assert_eq!(ended.status, StatusCode::OK, "{:?}", ended.body);
    let refused = Device::new(1).challenge(licensing.app(), &first).await;
    assert_eq!(refused.error(), "activation_revoked");
    assert_eq!(Device::new(2).check(licensing.app(), &second).await.status, StatusCode::OK, "the others go on");
    assert_eq!(licensing.licence(&licence).await["active_computers"], 2);

    let reset = licensing.admin_post(&format!("/api/admin/licences/{licence}/reset"), json!({})).await;
    assert_eq!(reset.status, StatusCode::OK, "{:?}", reset.body);
    for (seed, activation) in [(2, &second), (3, &third)] {
        assert_eq!(Device::new(seed).challenge(licensing.app(), activation).await.error(), "activation_revoked");
    }
    let view = licensing.licence(&licence).await;
    assert_eq!((&view["state"], &view["active_computers"]), (&json!("unused"), &json!(0)));
    let metadata: Vec<Value> =
        sqlx::query_scalar("SELECT metadata FROM audit_logs WHERE action = 'licence.reset' ORDER BY created_at, id")
            .fetch_all(licensing.admin.pool())
            .await
            .unwrap();
    assert_eq!(metadata, [json!({ "activation": first }), json!({})]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn revoking_a_test_licence_locks_every_computer(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (licence, key) = test_licence(&licensing).await;
    let first = activate(&licensing, &Device::new(1), &key).await;
    let second = activate(&licensing, &Device::new(2), &key).await;
    // challenges asked for before the revocation, answered after it
    let mut pending = Vec::new();
    for (seed, activation) in [(1, &first), (2, &second)] {
        let device = Device::new(seed);
        let challenge = device.challenge(licensing.app(), activation).await;
        pending.push((device.check_body(activation, &challenge.body), device, activation));
    }
    licensing.admin_post(&format!("/api/admin/licences/{licence}/revoke"), json!({})).await;
    for (body, device, activation) in pending {
        assert_eq!(device.send_check(licensing.app(), activation, body).await.error(), "licence_revoked");
    }
    assert_eq!(
        Device::new(4).activate(licensing.app(), PRODUCT, &key).await.error(),
        "licence_revoked",
        "and no new one gets in"
    );
}
