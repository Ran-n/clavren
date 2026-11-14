// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.638641
//+ Revised: 	2026/11/14 12:05:23.998886
// -----------------------------------------------------------------------------

//! `clavren-admin` against a real database, driven through a scripted terminal: creating admins,
//! resetting, disabling and listing them, and every way the conversation can go wrong.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

mod accounts;
#[path = "../common/mod.rs"]
mod common;

use std::sync::Arc;

use clavren::admin_auth::totp;
use clavren::admin_cli::{CliError, Command};
use clavren::clock::ManualClock;
use clavren::crypto::key_ring::KeyRing;
use clavren::crypto::master_key::MasterKey;
use common::admin::{Answer, PASSWORD, run_cli, seed_admin, smooth_enrolment, text};
use common::{TEST_MASTER_KEY, test_start};
use sqlx::PgPool;
use uuid::Uuid;

fn clock() -> Arc<ManualClock> {
    Arc::new(ManualClock::new(test_start()))
}

fn create(username: &str) -> Command {
    Command::CreateAdmin { username: username.into() }
}

async fn admin_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM admin_users").fetch_one(pool).await.unwrap()
}

async fn audit_actions(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT action FROM audit_logs ORDER BY created_at, action").fetch_all(pool).await.unwrap()
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn create_admin_stores_only_protected_credentials(pool: PgPool) {
    let clock = clock();
    let (result, terminal) = run_cli(&pool, &clock, create("ran"), "ran", smooth_enrolment(PASSWORD)).await;
    result.unwrap();

    let (id, hash, ciphertext, nonce, step): (Uuid, String, Vec<u8>, Vec<u8>, Option<i64>) = sqlx::query_as(
        "SELECT id, password_hash, totp_secret_ciphertext, totp_secret_nonce, totp_last_used_step FROM admin_users",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(hash.starts_with("$argon2id$") && !hash.contains(PASSWORD));
    let keys = KeyRing::new(MasterKey::from_bytes(TEST_MASTER_KEY));
    assert_eq!(totp::open(&keys, id, &ciphertext, &nonce).unwrap(), terminal.printed_secret().unwrap());
    assert!(step.is_some(), "the confirming code must count as used");
    let stored_codes: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_recovery_codes WHERE used_at IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_codes, 10);
    assert_eq!(terminal.printed_codes().len(), 10);
    assert!(terminal.output.contains('█') || terminal.output.contains('▀'), "no QR code drawn");
    assert_eq!(audit_actions(&pool).await, ["admin.created"]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn usernames_are_validated_and_unique_ignoring_case(pool: PgPool) {
    let clock = clock();
    let (result, _) = run_cli(&pool, &clock, create("Ran Hash"), "Ran Hash", vec![]).await;
    assert!(matches!(result, Err(CliError::InvalidUsername(_))));

    seed_admin(&pool, &clock, "ran").await;
    let (result, terminal) = run_cli(&pool, &clock, create("ran"), "ran", smooth_enrolment(PASSWORD)).await;
    assert!(matches!(result, Err(CliError::UsernameTaken(_))));
    assert!(terminal.prompts.is_empty(), "asked for a password before checking the name");
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn a_mismatched_or_short_password_is_asked_again(pool: PgPool) {
    let clock = clock();
    let answers = vec![
        text("too short"),
        text(PASSWORD),
        text("something else entirely"),
        text(PASSWORD),
        text(PASSWORD),
        Answer::CurrentCode,
    ];
    let (result, terminal) = run_cli(&pool, &clock, create("ran"), "ran", answers).await;
    result.unwrap();
    assert!(terminal.output.contains("at least 12 characters"), "{}", terminal.output);
    assert!(terminal.output.contains("don't match"), "{}", terminal.output);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn three_failed_passwords_change_nothing(pool: PgPool) {
    let clock = clock();
    let answers = vec![text("short"), text("short"), text("short")];
    let (result, _) = run_cli(&pool, &clock, create("ran"), "ran", answers).await;
    assert!(matches!(result, Err(CliError::NoPassword)));
    assert_eq!(admin_count(&pool).await, 0);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn three_wrong_codes_change_nothing(pool: PgPool) {
    let clock = clock();
    let answers = vec![text(PASSWORD), text(PASSWORD), text("000000"), text("abc"), text("")];
    let (result, terminal) = run_cli(&pool, &clock, create("ran"), "ran", answers).await;
    assert!(matches!(result, Err(CliError::NoAuthenticator)));
    assert!(result.unwrap_err().to_string().contains("nothing was changed"));
    assert!(terminal.output.contains("doesn't match"));
    assert_eq!(admin_count(&pool).await, 0);
    assert!(audit_actions(&pool).await.is_empty());
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn running_out_of_input_is_an_error_not_a_hang(pool: PgPool) {
    let clock = clock();
    let (result, _) = run_cli(&pool, &clock, create("ran"), "ran", vec![text(PASSWORD)]).await;
    assert!(matches!(result, Err(CliError::Input(_))));
    assert_eq!(admin_count(&pool).await, 0);
}

/// Regression: the recovery codes were printed after the account was saved, and a failed print
/// was ignored: with stdout closed the account existed, its codes never seen.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn recovery_codes_that_cannot_be_shown_change_nothing(pool: PgPool) {
    use clavren::admin_cli;
    use common::admin::{ScriptedTerminal, cli_context};

    let clock = clock();
    let mut terminal = ScriptedTerminal::new("ran", clock.clone(), smooth_enrolment(PASSWORD));
    terminal.output_closed = true;
    let result = admin_cli::run(&create("ran"), &cli_context(&pool, &clock), &mut terminal).await;
    let error = result.unwrap_err();
    assert!(matches!(error, CliError::Output(_)), "{error}");
    assert!(error.to_string().contains("nothing was changed"), "{error}");
    assert_eq!(admin_count(&pool).await, 0);
    assert!(audit_actions(&pool).await.is_empty());

    let before = seed_admin(&pool, &clock, "ran").await;
    let hash_of = || async {
        sqlx::query_scalar::<_, String>("SELECT password_hash FROM admin_users").fetch_one(&pool).await.unwrap()
    };
    let old_hash = hash_of().await;
    let mut terminal = ScriptedTerminal::new("ran", clock.clone(), smooth_enrolment("another long password 9"));
    terminal.output_closed = true;
    let command = Command::ResetPassword { username: "ran".into() };
    let result = admin_cli::run(&command, &cli_context(&pool, &clock), &mut terminal).await;
    assert!(matches!(result, Err(CliError::Output(_))));
    assert_eq!(hash_of().await, old_hash, "the old password still works");
    let codes_left: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_recovery_codes WHERE admin_user_id = $1")
        .bind(before.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(codes_left, 10, "and so do the old recovery codes");
    assert!(!audit_actions(&pool).await.contains(&"admin.password_reset".to_string()));
}
