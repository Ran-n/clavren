// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.638641
//+ Revised: 	2026/11/03 04:54:23.234458
// -----------------------------------------------------------------------------

//! `clavren-admin` against a real database, driven through a scripted terminal: creating admins,
//! resetting, disabling and listing them, and every way the conversation can go wrong.

#![allow(clippy::unwrap_used, clippy::expect_used)] // a test that fails should panic

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

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn reset_password_replaces_every_credential_and_signs_out(pool: PgPool) {
    let clock = clock();
    let before = seed_admin(&pool, &clock, "ran").await;
    sqlx::query(
        "INSERT INTO admin_sessions (admin_user_id, token_hash, expires_at) VALUES ($1, $2, now() + interval '1 day')",
    )
    .bind(before.id)
    .bind([5u8; 32].as_slice())
    .execute(&pool)
    .await
    .unwrap();

    let new_password = "a brand new passphrase";
    let command = Command::ResetPassword { username: "RAN".into() };
    let (result, terminal) = run_cli(&pool, &clock, command, "ran", smooth_enrolment(new_password)).await;
    result.unwrap();

    assert_ne!(terminal.printed_secret().unwrap(), before.secret);
    let live_sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_sessions WHERE revoked_at IS NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(live_sessions, 0);
    let old_codes_left: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_recovery_codes WHERE code_hash = $1")
        .bind(
            blake3::hash(clavren::admin_auth::recovery_codes::normalize(&before.recovery_codes[0]).unwrap().as_bytes())
                .as_bytes()
                .as_slice(),
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old_codes_left, 0);
    assert!(audit_actions(&pool).await.contains(&"admin.password_reset".to_string()));
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn commands_for_an_unknown_admin_say_so(pool: PgPool) {
    let clock = clock();
    for command in [
        Command::ResetPassword { username: "ghost".into() },
        Command::DisableAdmin { username: "ghost".into() },
        Command::EnableAdmin { username: "ghost".into() },
    ] {
        let (result, _) = run_cli(&pool, &clock, command, "ghost", vec![]).await;
        let error = result.unwrap_err();
        assert!(matches!(error, CliError::UnknownAdmin(_)));
        assert!(error.to_string().contains("list-admins"));
    }
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn disabling_signs_out_and_enabling_restores(pool: PgPool) {
    let clock = clock();
    let admin = seed_admin(&pool, &clock, "ran").await;
    sqlx::query(
        "INSERT INTO admin_sessions (admin_user_id, token_hash, expires_at) VALUES ($1, $2, now() + interval '1 day')",
    )
    .bind(admin.id)
    .bind([6u8; 32].as_slice())
    .execute(&pool)
    .await
    .unwrap();
    let disable = || Command::DisableAdmin { username: "ran".into() };
    let enable = || Command::EnableAdmin { username: "ran".into() };

    let (result, terminal) = run_cli(&pool, &clock, disable(), "ran", vec![]).await;
    result.unwrap();
    assert!(terminal.output.contains("is disabled"));
    let revoked: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_sessions WHERE revoked_at IS NOT NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(revoked, 1);

    let (result, terminal) = run_cli(&pool, &clock, disable(), "ran", vec![]).await;
    result.unwrap();
    assert!(terminal.output.contains("already disabled"));

    let (result, terminal) = run_cli(&pool, &clock, enable(), "ran", vec![]).await;
    result.unwrap();
    assert!(terminal.output.contains("can sign in again"));
    let (result, terminal) = run_cli(&pool, &clock, enable(), "ran", vec![]).await;
    result.unwrap();
    assert!(terminal.output.contains("already enabled"));

    let actions = audit_actions(&pool).await;
    assert_eq!(actions.iter().filter(|action| *action == "admin.disabled").count(), 1);
    assert_eq!(actions.iter().filter(|action| *action == "admin.enabled").count(), 1);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn list_admins_shows_a_table(pool: PgPool) {
    let clock = clock();
    let (result, terminal) = run_cli(&pool, &clock, Command::ListAdmins, "", vec![]).await;
    result.unwrap();
    assert!(terminal.output.contains("No admins yet"));

    seed_admin(&pool, &clock, "zoe").await;
    seed_admin(&pool, &clock, "ana").await;
    run_cli(&pool, &clock, Command::DisableAdmin { username: "zoe".into() }, "zoe", vec![]).await.0.unwrap();

    let (result, terminal) = run_cli(&pool, &clock, Command::ListAdmins, "", vec![]).await;
    result.unwrap();
    let lines: Vec<&str> = terminal.output.lines().collect();
    assert!(lines[0].starts_with("USERNAME") && lines[0].contains("LAST LOGIN"), "{}", terminal.output);
    assert!(lines[1].starts_with("ana ") && lines[1].contains("active") && lines[1].ends_with("never"), "{}", lines[1]);
    assert!(lines[2].starts_with("zoe ") && lines[2].contains("disabled"), "{}", lines[2]);
}

#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn show_public_key_prints_the_key_in_use_and_the_next(pool: PgPool) {
    let clock = clock();
    let (result, terminal) = run_cli(&pool, &clock, Command::ShowPublicKey, "", vec![]).await;
    result.unwrap();
    let keys = KeyRing::new(MasterKey::from_bytes(TEST_MASTER_KEY));
    let expected = |version| {
        clavren_protocol::encoding::encode(clavren::activations::tokens::public_key(&keys, version).as_bytes())
    };
    assert!(terminal.output.contains(&format!("v1 (in use): {}", expected(1))), "{}", terminal.output);
    assert!(terminal.output.contains(&format!("v2 (next): {}", expected(2))), "{}", terminal.output);
    assert!(terminal.output.contains("CLAVREN_SIGNING_KEY_VERSION=2"));
}
