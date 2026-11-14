// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:25.719155
//+ Revised: 	2026/11/14 12:05:25.719155
// -----------------------------------------------------------------------------

//! The rate limit, the hourly counts and what maintenance deletes.

use super::*;

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn one_activation_gets_thirty_checks_an_hour(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);
    for _ in 0..30 {
        assert_eq!(device.check(licensing.app(), &activation).await.status, StatusCode::OK);
    }
    let blocked = device.check(licensing.app(), &activation).await;
    assert_eq!((blocked.status, blocked.error()), (StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn maintenance_deletes_used_and_expired_challenges(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);
    device.check(licensing.app(), &activation).await;
    device.challenge(licensing.app(), &activation).await;
    licensing.admin.fixture.clock.advance(Duration::from_secs(60));
    device.challenge(licensing.app(), &activation).await;
    licensing.admin.fixture.clock.advance(Duration::from_secs(61));

    let cleaned = maintenance::run_once(&licensing.admin.fixture.state).await.unwrap();
    assert_eq!(cleaned.challenges, 2, "the used one and the expired one");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM verification_challenges")
        .fetch_one(licensing.admin.pool())
        .await
        .unwrap();
    assert_eq!(left, 1, "the one still valid stays");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn checks_are_counted_by_the_hour_and_old_hours_are_forgotten(pool: PgPool) {
    let licensing = Licensing::new(pool).await;
    let (_, activation) = activated(&licensing, json!({})).await;
    let device = Device::new(1);
    let clock = &licensing.admin.fixture.clock;
    for _ in 0..2 {
        assert_eq!(device.check(licensing.app(), &activation).await.status, StatusCode::OK);
    }
    clock.advance(Duration::from_secs(3600));
    assert_eq!(device.check(licensing.app(), &activation).await.status, StatusCode::OK);
    let counts: Vec<i64> = sqlx::query_scalar("SELECT checks FROM check_counts ORDER BY hour")
        .fetch_all(licensing.admin.pool())
        .await
        .unwrap();
    assert_eq!(counts, [2, 1], "one row per hour, not per check");

    // 30 days after the first checks: their hour began more than 30 days ago, the next one didn't.
    clock.advance(Duration::from_secs(30 * 24 * 3600 - 3600));
    let cleaned = maintenance::run_once(&licensing.admin.fixture.state).await.unwrap();
    assert_eq!(cleaned.check_counts, 1, "only the hour more than 30 days old");
}
