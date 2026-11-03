// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.673534
//+ Revised: 	2026/11/03 04:54:18.833609
// -----------------------------------------------------------------------------

//! `clavren-admin`: managing administrator accounts on the server itself (there is no
//! registration; the first admin is created here), and printing the keys applications embed.

mod commands;
pub mod enrolment;
pub mod terminal;

use std::sync::Arc;

use clap::{Parser, Subcommand};
use sqlx::PgPool;

use crate::admin_auth::passwords::PasswordError;
use crate::admin_auth::totp::TotpError;
use crate::clock::Clock;
use crate::crypto::key_ring::KeyRing;
use crate::crypto::random::RandomError;
use crate::startup::StartupError;
use terminal::Terminal;

#[derive(Debug, Parser)]
#[command(name = "clavren-admin", version, about = "Manage Clavren's administrator accounts")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create an admin: asks for a password and sets up an authenticator app.
    CreateAdmin { username: String },
    /// Give an admin a new password, authenticator and recovery codes, and sign them out.
    ResetPassword { username: String },
    /// Stop an admin from signing in, and sign them out everywhere.
    DisableAdmin { username: String },
    /// Let a disabled admin sign in again.
    EnableAdmin { username: String },
    /// Show every admin and whether they can sign in.
    ListAdmins,
    /// Print the token-signing public keys applications must trust: the one in use and the next.
    ShowPublicKey,
}

pub struct CliContext {
    pub pool: PgPool,
    pub keys: KeyRing,
    pub clock: Arc<dyn Clock>,
    /// The signing key version the server uses (`CLAVREN_SIGNING_KEY_VERSION`).
    pub signing_key_version: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Startup(#[from] StartupError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("could not read from the keyboard: {0}")]
    Input(#[from] std::io::Error),
    #[error(
        "{0:?} is not a valid username: use lowercase letters, digits, '.', '_' or '-', starting with a letter or digit"
    )]
    InvalidUsername(String),
    #[error("an admin named {0} already exists")]
    UsernameTaken(String),
    #[error("there is no admin named {0}; see clavren-admin list-admins")]
    UnknownAdmin(String),
    #[error("no password was set after {} attempts; nothing was changed", enrolment::ATTEMPTS)]
    NoPassword,
    #[error("the authenticator app wasn't confirmed after {} attempts; nothing was changed", enrolment::ATTEMPTS)]
    NoAuthenticator,
    #[error(transparent)]
    Password(#[from] PasswordError),
    #[error(transparent)]
    Totp(#[from] TotpError),
    #[error(transparent)]
    Random(#[from] RandomError),
}

pub async fn run(command: &Command, context: &CliContext, terminal: &mut dyn Terminal) -> Result<(), CliError> {
    match command {
        Command::CreateAdmin { username } => commands::create_admin(context, terminal, username).await,
        Command::ResetPassword { username } => commands::reset_password(context, terminal, username).await,
        Command::DisableAdmin { username } => commands::disable_admin(context, terminal, username).await,
        Command::EnableAdmin { username } => commands::enable_admin(context, terminal, username).await,
        Command::ListAdmins => commands::list_admins(context, terminal).await,
        Command::ShowPublicKey => {
            commands::show_public_key(context, terminal);
            Ok(())
        }
    }
}
