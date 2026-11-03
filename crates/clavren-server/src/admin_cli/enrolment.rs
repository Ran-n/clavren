// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.614538
//+ Revised: 	2026/11/03 04:54:18.768761
// -----------------------------------------------------------------------------

//! Setting up an admin's credentials at the keyboard: a password typed twice, an authenticator app
//! confirmed with a code, and a fresh set of recovery codes. Used by `create-admin` and
//! `reset-password`.

use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

use super::CliError;
use super::terminal::Terminal;
use crate::admin_auth::{passwords, recovery_codes, totp};
use crate::clock::Clock;

pub const ATTEMPTS: usize = 3;

pub struct Enrolment {
    pub password_hash: String,
    pub totp_secret: Vec<u8>,
    /// The step of the confirming code, recorded as used.
    pub totp_step: u64,
    pub recovery_codes: Vec<String>,
}

pub async fn enrol(terminal: &mut dyn Terminal, username: &str, clock: &dyn Clock) -> Result<Enrolment, CliError> {
    let password = ask_new_password(terminal)?;
    let password_hash = passwords::hash(password).await?;
    let totp_secret = totp::new_secret()?.to_vec();
    let totp_step = set_up_authenticator(terminal, username, &totp_secret, clock)?;
    let recovery_codes = recovery_codes::generate()?;
    Ok(Enrolment { password_hash, totp_secret, totp_step, recovery_codes })
}

fn ask_new_password(terminal: &mut dyn Terminal) -> Result<String, CliError> {
    for _ in 0..ATTEMPTS {
        let password = terminal.ask_secret("New password: ")?;
        if let Err(problem) = passwords::validate_new(&password) {
            terminal.say(&format!("{problem}."));
            continue;
        }
        if terminal.ask_secret("Repeat the password: ")? == password {
            return Ok(password);
        }
        terminal.say("The passwords don't match.");
    }
    Err(CliError::NoPassword)
}

fn set_up_authenticator(
    terminal: &mut dyn Terminal,
    username: &str,
    secret: &[u8],
    clock: &dyn Clock,
) -> Result<u64, CliError> {
    let uri = totp::setup_uri(secret, username)?;
    terminal.say("\nScan this QR code with an authenticator app (Aegis, Google Authenticator, 1Password…):\n");
    terminal.say(&qr_code(&uri));
    terminal.say(&format!("If you can't scan it, enter this key by hand: {}\n", totp::secret_text(secret)));
    for _ in 0..ATTEMPTS {
        let code = terminal.ask("Code shown in the app: ")?;
        if let Some(step) = totp::matching_step(secret, username, code.trim(), clock.now()) {
            return Ok(step);
        }
        terminal.say("That code doesn't match. Check that the phone's clock is right, and type the newest code.");
    }
    Err(CliError::NoAuthenticator)
}

/// Light modules drawn as blocks: on a dark terminal (the usual) that gives dark-on-light, which
/// every scanner reads.
fn qr_code(uri: &str) -> String {
    match QrCode::new(uri.as_bytes()) {
        Ok(code) => code.render::<Dense1x2>().dark_color(Dense1x2::Light).light_color(Dense1x2::Dark).build(),
        Err(_) => format!("(no QR code could be drawn; use this address instead: {uri})"),
    }
}

pub fn show_recovery_codes(terminal: &mut dyn Terminal, codes: &[String]) {
    terminal.say("\nRecovery codes. Each one works once instead of an app code, if you lose your phone.");
    terminal.say("Store them somewhere safe now: they won't be shown again.\n");
    codes.iter().for_each(|code| terminal.say(&format!("  {code}")));
}
