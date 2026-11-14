// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:02:51.350079
//+ Revised: 	2026/11/14 12:02:51.350079
// -----------------------------------------------------------------------------

//! How long a computer works offline follows its product's limit.

use super::*;

async fn set_offline_days(running: &Running, days: i32) {
    sqlx::query("UPDATE products SET offline_days = $1 WHERE code = $2")
        .bind(days)
        .bind(PRODUCT)
        .execute(&running.pool)
        .await
        .unwrap();
}

/// When the computer's licence stops working offline.
fn offline_until(outcome: &Outcome) -> chrono::DateTime<Utc> {
    match outcome {
        Outcome::Licensed { offline_until, .. } => *offline_until,
        other => panic!("not licensed: {other:?}"),
    }
}

#[sqlx::test(migrations = false)]
async fn a_7_day_product_locks_after_8_days_offline_even_on_a_valid_licence(
    _pool: PgPoolOptions,
    options: PgConnectOptions,
) {
    let running = start(options).await;
    let (_, key) = issue(&running).await;
    set_offline_days(&running, 7).await;
    let clock = Arc::new(ManualClock::new(Utc::now()));
    let client = computer_with_clock(&running, clock.clone());
    let licensed = on_computer(&client, move |client| client.activate(&key).unwrap()).await;
    let until = offline_until(&licensed);
    assert!((until - (clock.now() + TimeDelta::days(7))).num_seconds().abs() < 60, "{until}");

    clock.set(clock.now() + TimeDelta::days(8));
    assert_eq!(on_computer(&client, |client| client.status()).await, Outcome::NeedsCheck, "offline too long");
    assert!(on_computer(&client, |client| client.check().unwrap()).await.is_licensed(), "online again, it runs");

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}

#[sqlx::test(migrations = false)]
async fn a_lowered_limit_applies_at_the_next_check(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (_, key) = issue(&running).await;
    let clock = Arc::new(ManualClock::new(Utc::now()));
    let client = computer_with_clock(&running, clock.clone());
    let first = offline_until(&on_computer(&client, move |client| client.activate(&key).unwrap()).await);
    assert!((first - (clock.now() + TimeDelta::days(30))).num_seconds().abs() < 60, "{first}");

    set_offline_days(&running, 7).await;
    let checked = offline_until(&on_computer(&client, |client| client.check().unwrap()).await);
    assert!((checked - (clock.now() + TimeDelta::days(7))).num_seconds().abs() < 60, "{checked}");

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}

#[sqlx::test(migrations = false)]
async fn a_revoked_licence_on_a_365_day_product_locks_at_the_next_check_online(
    _pool: PgPoolOptions,
    options: PgConnectOptions,
) {
    let running = start(options).await;
    let (licence, key) = issue(&running).await;
    set_offline_days(&running, 365).await;
    let client = computer(&running);
    assert!(on_computer(&client, move |client| client.activate(&key).unwrap()).await.is_licensed());
    sqlx::query("UPDATE licenses SET revoked_at = now() WHERE id = $1")
        .bind(licence)
        .execute(&running.pool)
        .await
        .unwrap();
    assert_eq!(on_computer(&client, |client| client.check().unwrap()).await, Outcome::Revoked);
    assert_eq!(on_computer(&client, |client| client.status()).await, Outcome::Revoked);

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}
