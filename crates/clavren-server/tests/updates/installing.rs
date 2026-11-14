// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:28.272737
//+ Revised: 	2026/11/14 12:05:28.272737
// -----------------------------------------------------------------------------

//! Asking for an update to be installed, and how it then goes.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn installing_writes_the_request_and_audits_it(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let reply = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);

    let written: Value = serde_json::from_slice(&std::fs::read(updates.dir.join("request.json")).unwrap()).unwrap();
    assert_eq!(written, json!({ "version": newer(), "skip_backup": false }));
    assert_eq!(
        updates.audited().await,
        [json!({
            "entity_type": "server",
            "entity_id": null,
            "metadata": { "from": running(), "to": newer(), "skip_backup": false },
        })]
    );

    let body = updates.get().await;
    assert_eq!((&body["update"]["state"], &body["update"]["stage"]), (&json!("running"), &json!("starting")));
    assert_eq!(body["can_install"], false);
    let again = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!((again.status, again.error()), (StatusCode::CONFLICT, "update_running"));

    let csrf = Call::new(Method::POST, "/api/admin/updates/install")
        .signed_in(&updates.admin.browser)
        .csrf("wrong")
        .json(json!({ "version": newer(), "skip_backup": false }))
        .send(&updates.admin.fixture.app)
        .await;
    assert_eq!(csrf.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn installing_is_refused_when_it_cannot_go_ahead(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({ "backup_drive_mounted": false }));
    let refused = |body: Value| async { updates.install(body).await };

    let reply = refused(json!({ "version": running(), "skip_backup": true })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "update_not_available"), "not the newer one");
    let reply = refused(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "backup_required"));
    for body in [
        json!({ "version": format!("v{}", newer()), "skip_backup": true }),
        json!({ "version": "1.0.0; rm -rf /", "skip_backup": true }),
        json!({ "version": newer(), "skip_backup": "yes" }),
        json!({ "version": newer(), "skip_backup": true, "url": "https://evil.example.com" }),
        json!({ "version": newer() }),
    ] {
        let reply = refused(body.clone()).await;
        assert_eq!((reply.status, reply.error()), (StatusCode::BAD_REQUEST, "invalid_request"), "{body}");
    }
    assert!(!exists(&updates.dir, "request.json"));
    assert!(updates.audited().await.is_empty());

    updates.write_status(&json!({ "version": newer(), "state": "running", "stage": "downloading", "message": null,
                 "started_at": "2030-01-15T09:59:00Z", "finished_at": null }));
    let reply = refused(json!({ "version": newer(), "skip_backup": true })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "update_running"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn without_the_updater_there_is_nothing_to_install_with(pool: PgPool) {
    let unsupported = Updates::new(pool, true, false).await;
    unsupported.latest(&newer(), json!({}));
    let body = unsupported.get().await;
    assert_eq!((&body["can_install"], &body["updater_installed"]), (&json!(false), &json!(false)));
    let reply = unsupported.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!((reply.status, reply.error()), (StatusCode::CONFLICT, "updates_unsupported"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn without_a_backup_drive_an_update_must_say_so(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({ "backup_drive_mounted": false }));
    assert_eq!(updates.get().await["backup_drive_mounted"], false);
    let reply = updates.install(json!({ "version": newer(), "skip_backup": true })).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);
    assert_eq!(updates.audited().await[0]["metadata"]["skip_backup"], true);
}

/// Regression: two install requests at once both passed the "already running" check before
/// either was written, so both were audited and the second overwrote the first.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn two_install_requests_at_once_make_one_update(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let body = json!({ "version": newer(), "skip_backup": false });
    let (first, second) = tokio::join!(updates.install(body.clone()), updates.install(body));
    let mut statuses = [first.status, second.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::ACCEPTED, StatusCode::CONFLICT]);
    assert_eq!(updates.audited().await.len(), 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_request_that_cannot_be_written_leaves_no_audit_entry(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    // the temporary file's name is taken by a folder, so writing the request fails
    std::fs::create_dir(updates.dir.join("request.json.new")).unwrap();
    let (logs, _guard) = capture_logs();
    let reply = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(logs.text().contains("request.json"), "{}", logs.text());
    assert!(updates.audited().await.is_empty(), "the audit entry was rolled back");
    assert!(!exists(&updates.dir, "request.json"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_update_status_comes_back(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let started = common::test_start().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for (state, stage, message) in [
        ("running", "backing_up", Value::Null),
        ("failed", "checking", json!("The download didn't match its checksum, so nothing was installed.")),
    ] {
        let status = json!({ "version": newer(), "state": state, "stage": stage, "message": message,
                             "started_at": started, "finished_at": null });
        updates.write_status(&status);
        let body = updates.get().await;
        assert_eq!(body["update"], status, "{state}");
        assert_eq!(body["can_install"], state == "failed");
    }

    // an update to the version that now runs is over: nothing to show
    updates.write_status(&json!({ "version": running(), "state": "succeeded", "stage": "restarting", "message": null,
                 "started_at": started, "finished_at": started }));
    assert_eq!(updates.get().await["update"], Value::Null);
}

/// Regression: with clavren-update.path stopped nothing ever picked the request up, and the
/// dashboard said "starting" with the Update button off for ever.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_request_nobody_picks_up_fails_and_can_be_sent_again(pool: PgPool) {
    let updates = Updates::new(pool, true, true).await;
    updates.latest(&newer(), json!({}));
    let reply = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);
    let three_minutes_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(180);
    std::fs::File::options()
        .write(true)
        .open(updates.dir.join("request.json"))
        .unwrap()
        .set_modified(three_minutes_ago)
        .unwrap();

    let body = updates.get().await;
    assert_eq!((&body["update"]["state"], &body["update"]["version"]), (&json!("failed"), &json!(newer())));
    assert!(body["update"]["message"].as_str().unwrap().contains("clavren-update.path"), "{body}");
    assert_eq!(body["can_install"], true);
    let again = updates.install(json!({ "version": newer(), "skip_backup": false })).await;
    assert_eq!(again.status, StatusCode::ACCEPTED, "{:?}", again.body);
    assert_eq!(updates.get().await["update"]["state"], "running", "a fresh request starts over");
}
