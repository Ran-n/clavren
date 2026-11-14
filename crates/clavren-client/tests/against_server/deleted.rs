// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:02:51.472072
//+ Revised: 	2026/11/14 12:02:51.472072
// -----------------------------------------------------------------------------

//! A licence or a product deleted on the server locks the computer, and an approved
//! application activates by itself.

use super::*;

#[sqlx::test(migrations = false)]
async fn a_deleted_licences_computer_locks(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (licence, key) = issue(&running).await;
    let client = computer(&running);
    assert!(on_computer(&client, move |client| client.activate(&key).unwrap()).await.is_licensed());

    let mut connection = running.pool.acquire().await.unwrap();
    writes::delete(&mut connection, &[licence]).await.unwrap();
    assert_eq!(on_computer(&client, |client| client.check().unwrap()).await, Outcome::Revoked, "a definitive refusal");
    assert_eq!(on_computer(&client, |client| client.status()).await, Outcome::Revoked, "the token is gone");

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}

#[sqlx::test(migrations = false)]
async fn a_deleted_products_computer_locks(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (licence, key) = issue(&running).await;
    let client = computer(&running);
    assert!(on_computer(&client, move |client| client.activate(&key).unwrap()).await.is_licensed());

    let product: Uuid = sqlx::query_scalar("SELECT product_id FROM licenses WHERE id = $1")
        .bind(licence)
        .fetch_one(&running.pool)
        .await
        .unwrap();
    let mut transaction = running.pool.begin().await.unwrap();
    let ids = repository::ids_of_product(&mut transaction, product).await.unwrap();
    writes::delete(&mut transaction, &ids).await.unwrap();
    clavren::products::repository::delete(&mut *transaction, product).await.unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(on_computer(&client, |client| client.check().unwrap()).await, Outcome::Revoked, "a definitive refusal");
    assert_eq!(on_computer(&client, |client| client.status()).await, Outcome::Revoked, "the token is gone");

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}

#[sqlx::test(migrations = false)]
async fn an_approved_application_activates_by_itself(_pool: PgPoolOptions, options: PgConnectOptions) {
    let running = start(options).await;
    let (licence, _) = issue(&running).await;
    sqlx::query("UPDATE products SET accepts_applications = TRUE").execute(&running.pool).await.unwrap();
    let client = computer(&running);
    let applicant =
        Applicant { name: "Ana López".into(), email: "ana@example.com".into(), message: Some("For my class".into()) };
    assert_eq!(on_computer(&client, move |client| client.apply(&applicant).unwrap()).await, Applied::Sent);
    assert_eq!(on_computer(&client, |client| client.check().unwrap()).await, Outcome::ApplicationPending);

    // the administrator approves it with the licence issued above
    sqlx::query("UPDATE license_applications SET decided_at = now(), decision = 'approved', license_id = $1")
        .bind(licence)
        .execute(&running.pool)
        .await
        .unwrap();
    assert!(on_computer(&client, |client| client.check().unwrap()).await.is_licensed());
    let activated: i64 = sqlx::query_scalar("SELECT count(*) FROM activations WHERE license_id = $1")
        .bind(licence)
        .fetch_one(&running.pool)
        .await
        .unwrap();
    assert_eq!(activated, 1);
    assert!(on_computer(&client, |client| client.check().unwrap()).await.is_licensed(), "its daily check works");

    running.stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), running.serving).await.unwrap().unwrap();
}
