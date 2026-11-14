// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:23.902141
//+ Revised: 	2026/11/14 12:05:23.902141
// -----------------------------------------------------------------------------

//! Commands on existing admins: reset, disable, enable, list, and the public key.

use super::*;

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
