// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.553798
//+ Revised: 	2026/11/03 04:54:18.708486
// -----------------------------------------------------------------------------

//! The `clavren-admin` commands.

use clavren_protocol::encoding;
use serde_json::json;
use uuid::Uuid;

use super::enrolment::{self, Enrolment};
use super::terminal::Terminal;
use super::{CliContext, CliError};
use crate::activations::tokens;
use crate::admin_auth::repository::{self, AdminRecord, Credentials};
use crate::admin_auth::{is_valid_username, recovery_codes, sessions, totp};
use crate::audit::{self, AuditEntry};

pub async fn create_admin(context: &CliContext, terminal: &mut dyn Terminal, username: &str) -> Result<(), CliError> {
    if !is_valid_username(username) {
        return Err(CliError::InvalidUsername(username.to_string()));
    }
    if repository::find_by_username(&context.pool, username).await?.is_some() {
        return Err(CliError::UsernameTaken(username.to_string()));
    }
    let enrolment = enrolment::enrol(terminal, username, context.clock.as_ref()).await?;
    let id = Uuid::now_v7();
    let credentials = credentials(context, id, &enrolment)?;
    let now = context.clock.now();
    let mut transaction = context.pool.begin().await?;
    repository::insert(&mut *transaction, id, username, &credentials, now).await?;
    repository::replace_recovery_codes(&mut transaction, id, &credentials.recovery_code_hashes).await?;
    audit::record(&mut *transaction, AuditEntry::admin_account("admin.created", None, Some(id), None, now)).await?;
    transaction.commit().await?;
    tracing::info!(admin = %username, "admin created with the CLI");
    enrolment::show_recovery_codes(terminal, &enrolment.recovery_codes);
    terminal.say(&format!("\nAdmin {username} created. Sign in to the dashboard with the password and an app code."));
    Ok(())
}

/// A new password, a new authenticator and new recovery codes, for an admin who lost access.
/// Signs them out everywhere.
pub async fn reset_password(context: &CliContext, terminal: &mut dyn Terminal, username: &str) -> Result<(), CliError> {
    let admin = existing(context, username).await?;
    let enrolment = enrolment::enrol(terminal, &admin.username, context.clock.as_ref()).await?;
    let credentials = credentials(context, admin.id, &enrolment)?;
    let now = context.clock.now();
    let mut transaction = context.pool.begin().await?;
    repository::set_password(&mut *transaction, admin.id, &credentials.password_hash).await?;
    repository::set_totp(
        &mut *transaction,
        admin.id,
        &credentials.totp_secret_ciphertext,
        &credentials.totp_secret_nonce,
        credentials.totp_last_used_step,
    )
    .await?;
    repository::replace_recovery_codes(&mut transaction, admin.id, &credentials.recovery_code_hashes).await?;
    let revoked = sessions::revoke_all(&mut *transaction, admin.id, None, now).await?;
    let entry = AuditEntry::admin_account("admin.password_reset", None, Some(admin.id), None, now)
        .with_metadata(json!({ "sessions_revoked": revoked }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %admin.username, "admin credentials reset with the CLI");
    enrolment::show_recovery_codes(terminal, &enrolment.recovery_codes);
    terminal.say(&format!("\n{} has a new password and authenticator, and was signed out everywhere.", admin.username));
    Ok(())
}

pub async fn disable_admin(context: &CliContext, terminal: &mut dyn Terminal, username: &str) -> Result<(), CliError> {
    let admin = existing(context, username).await?;
    let now = context.clock.now();
    let mut transaction = context.pool.begin().await?;
    if !repository::set_disabled(&mut *transaction, admin.id, Some(now)).await? {
        terminal.say(&format!("{} is already disabled.", admin.username));
        return Ok(());
    }
    let revoked = sessions::revoke_all(&mut *transaction, admin.id, None, now).await?;
    let entry = AuditEntry::admin_account("admin.disabled", None, Some(admin.id), None, now)
        .with_metadata(json!({ "sessions_revoked": revoked }));
    audit::record(&mut *transaction, entry).await?;
    transaction.commit().await?;
    tracing::info!(admin = %admin.username, "admin disabled with the CLI");
    terminal.say(&format!("{} is disabled and was signed out everywhere.", admin.username));
    Ok(())
}

pub async fn enable_admin(context: &CliContext, terminal: &mut dyn Terminal, username: &str) -> Result<(), CliError> {
    let admin = existing(context, username).await?;
    let now = context.clock.now();
    let mut transaction = context.pool.begin().await?;
    if !repository::set_disabled(&mut *transaction, admin.id, None).await? {
        terminal.say(&format!("{} is already enabled.", admin.username));
        return Ok(());
    }
    audit::record(&mut *transaction, AuditEntry::admin_account("admin.enabled", None, Some(admin.id), None, now))
        .await?;
    transaction.commit().await?;
    tracing::info!(admin = %admin.username, "admin enabled with the CLI");
    terminal.say(&format!("{} can sign in again.", admin.username));
    Ok(())
}

pub async fn list_admins(context: &CliContext, terminal: &mut dyn Terminal) -> Result<(), CliError> {
    let admins = repository::list(&context.pool).await?;
    if admins.is_empty() {
        terminal.say("No admins yet. Create one with: clavren-admin create-admin <username>");
        return Ok(());
    }
    terminal.say(&format!("{:<24} {:<9} {:<17} {}", "USERNAME", "STATUS", "CREATED", "LAST LOGIN"));
    for admin in &admins {
        let status = if admin.disabled_at.is_some() { "disabled" } else { "active" };
        let last_login =
            admin.last_login_at.map_or_else(|| "never".to_string(), |at| at.format("%Y-%m-%d %H:%M").to_string());
        let created = admin.created_at.format("%Y-%m-%d %H:%M");
        terminal.say(&format!("{:<24} {status:<9} {created:<17} {last_login}", admin.username));
    }
    Ok(())
}

/// The public keys of the signing key in use and of the next one: applications should trust both,
/// so the server can move to the next key once no older application is in use.
pub fn show_public_key(context: &CliContext, terminal: &mut dyn Terminal) {
    let (current, next) = (context.signing_key_version, context.signing_key_version + 1);
    for (version, role) in [(current, "in use"), (next, "next")] {
        let key = encoding::encode(tokens::public_key(&context.keys, version).as_bytes());
        terminal.say(&format!("{} ({role}): {key}", tokens::kid(version)));
    }
    terminal.say(&format!(
        "Build applications that trust both. Once every copy in use has {}, set CLAVREN_SIGNING_KEY_VERSION={next}.",
        tokens::kid(next)
    ));
}

async fn existing(context: &CliContext, username: &str) -> Result<AdminRecord, CliError> {
    repository::find_by_username(&context.pool, username)
        .await?
        .ok_or_else(|| CliError::UnknownAdmin(username.to_string()))
}

fn credentials(context: &CliContext, admin_id: Uuid, enrolment: &Enrolment) -> Result<Credentials, CliError> {
    let sealed = totp::seal(&context.keys, admin_id, &enrolment.totp_secret)?;
    let recovery_code_hashes = enrolment
        .recovery_codes
        .iter()
        .filter_map(|code| recovery_codes::normalize(code))
        .map(|code| recovery_codes::hash(&code))
        .collect();
    Ok(Credentials {
        password_hash: enrolment.password_hash.clone(),
        totp_secret_ciphertext: sealed.ciphertext,
        totp_secret_nonce: sealed.nonce.to_vec(),
        totp_last_used_step: enrolment.totp_step,
        recovery_code_hashes,
    })
}
