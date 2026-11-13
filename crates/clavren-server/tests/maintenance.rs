// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.821456
//+ Revised: 	2026/11/11 17:53:41.844917
// -----------------------------------------------------------------------------

//! The hourly housekeeping task: which sessions it deletes, and that it stops when told to.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod common;

use std::time::Duration;

use clavren::maintenance::{self, Cleaned};
use common::api::{Browser, Fixture};
use common::{CapturedLogs, capture_logs};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

async fn session_count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM admin_sessions").fetch_one(fixture.pool()).await.unwrap()
}

const REVOKE: &str = "UPDATE admin_sessions SET revoked_at = last_seen_at WHERE token_hash = $1";
const EXPIRE: &str = "UPDATE admin_sessions SET expires_at = last_seen_at WHERE token_hash = $1";

async fn mark(fixture: &Fixture, browser: &Browser, statement: &'static str) {
    sqlx::query(statement)
        .bind(blake3::hash(browser.cookie.as_bytes()).as_bytes().as_slice())
        .execute(fixture.pool())
        .await
        .unwrap();
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn only_sessions_that_can_never_be_used_again_are_deleted(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    fixture.sign_in().await; // left alone for more than 12 hours
    fixture.clock.advance(Duration::from_secs(6 * 3600));
    let revoked = fixture.sign_in().await;
    mark(&fixture, &revoked, REVOKE).await;
    let expired = fixture.sign_in().await;
    mark(&fixture, &expired, EXPIRE).await;
    let live = fixture.sign_in().await;
    fixture.clock.advance(Duration::from_secs(6 * 3600));

    assert_eq!(
        maintenance::run_once(&fixture.state).await.unwrap(),
        Cleaned { sessions: 3, challenges: 0, check_counts: 0 }
    );
    assert_eq!(session_count(&fixture).await, 1);
    let still_there: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_sessions WHERE token_hash = $1")
        .bind(blake3::hash(live.cookie.as_bytes()).as_bytes().as_slice())
        .fetch_one(fixture.pool())
        .await
        .unwrap();
    assert_eq!(still_there, 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn old_login_failures_are_forgotten(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    common::api::login(&fixture.app, "ran", "wrong password", "123456").await;
    assert_eq!(fixture.state.login_failures.tracked_keys(), 2);
    fixture.clock.advance(Duration::from_secs(16 * 60));
    maintenance::run_once(&fixture.state).await.unwrap();
    assert_eq!(fixture.state.login_failures.tracked_keys(), 0);
}

/// Waits until `line` was logged `times` times; fails instead of hanging if it never is.
async fn wait_for(logs: &CapturedLogs, line: &str, times: usize) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while logs.text().matches(line).count() < times {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{line:?} not logged {times} times:
{}",
            logs.text()
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn the_task_runs_at_once_then_stops_when_cancelled(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let (logs, _guard) = capture_logs();
    let stop = CancellationToken::new();
    let task = maintenance::spawn(fixture.state.clone(), stop.clone(), Duration::from_secs(3600));

    wait_for(&logs, "maintenance done", 1).await;
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), task).await.expect("the task didn't stop").unwrap();
    assert!(logs.text().contains("maintenance task stopped"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_failed_run_is_logged_and_the_task_keeps_going(pool: PgPool) {
    let fixture = Fixture::new(pool).await;
    let (logs, _guard) = capture_logs();
    fixture.pool().close().await;
    let stop = CancellationToken::new();
    let task = maintenance::spawn(fixture.state.clone(), stop.clone(), Duration::from_millis(10));

    wait_for(&logs, "maintenance failed", 2).await;
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), task).await.expect("the task didn't stop").unwrap();
}
