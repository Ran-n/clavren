// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.454128
//+ Revised: 	2026/11/03 04:54:22.981761
// -----------------------------------------------------------------------------

//! Creating admins the way a person would: through the real CLI commands, with a scripted
//! terminal that reads the secret the CLI prints and answers with real authenticator codes.

use std::collections::VecDeque;
use std::io;
use std::sync::Arc;

use clavren::admin_auth::totp;
use clavren::admin_cli::terminal::Terminal;
use clavren::admin_cli::{self, CliContext, CliError, Command};
use clavren::clock::{Clock, ManualClock};
use clavren::crypto::key_ring::KeyRing;
use clavren::crypto::master_key::MasterKey;
use sqlx::PgPool;
use uuid::Uuid;

use super::TEST_MASTER_KEY;

pub const PASSWORD: &str = "correct horse battery staple";

pub enum Answer {
    Text(String),
    /// The current code for the secret the CLI last printed.
    CurrentCode,
}

pub fn text(value: &str) -> Answer {
    Answer::Text(value.to_string())
}

pub struct ScriptedTerminal {
    answers: VecDeque<Answer>,
    pub output: String,
    pub prompts: Vec<String>,
    username: String,
    clock: Arc<ManualClock>,
}

impl ScriptedTerminal {
    pub fn new(username: &str, clock: Arc<ManualClock>, answers: Vec<Answer>) -> Self {
        Self { answers: answers.into(), output: String::new(), prompts: Vec::new(), username: username.into(), clock }
    }

    /// The Base32 secret the CLI printed for typing in by hand.
    pub fn printed_secret(&self) -> Option<Vec<u8>> {
        let marker = "enter this key by hand: ";
        let start = self.output.rfind(marker)? + marker.len();
        let secret = self.output[start..].lines().next()?;
        totp::secret_from_text(secret)
    }

    /// The recovery codes the CLI printed.
    pub fn printed_codes(&self) -> Vec<String> {
        self.output
            .lines()
            .map(str::trim)
            .filter(|line| line.len() == 31 && line.matches('-').count() == 5)
            .map(String::from)
            .collect()
    }

    fn next(&mut self, prompt: &str) -> io::Result<String> {
        self.prompts.push(prompt.to_string());
        match self.answers.pop_front() {
            Some(Answer::Text(value)) => Ok(value),
            Some(Answer::CurrentCode) => {
                let secret = self.printed_secret().expect("the CLI printed no secret before asking for a code");
                Ok(totp::code_at(&secret, &self.username, self.clock.now()).unwrap())
            }
            None => Err(io::Error::new(io::ErrorKind::UnexpectedEof, format!("no scripted answer for {prompt:?}"))),
        }
    }
}

impl Terminal for ScriptedTerminal {
    fn say(&mut self, text: &str) {
        self.output.push_str(text);
        self.output.push('\n');
    }

    fn ask(&mut self, prompt: &str) -> io::Result<String> {
        self.next(prompt)
    }

    fn ask_secret(&mut self, prompt: &str) -> io::Result<String> {
        self.next(prompt)
    }
}

pub fn cli_context(pool: &PgPool, clock: &Arc<ManualClock>) -> CliContext {
    let shared: Arc<dyn Clock> = clock.clone();
    CliContext {
        pool: pool.clone(),
        keys: KeyRing::new(MasterKey::from_bytes(TEST_MASTER_KEY)),
        clock: shared,
        signing_key_version: 1,
    }
}

/// The answers for a smooth enrolment: the password twice, then a valid code.
pub fn smooth_enrolment(password: &str) -> Vec<Answer> {
    vec![text(password), text(password), Answer::CurrentCode]
}

pub async fn run_cli(
    pool: &PgPool,
    clock: &Arc<ManualClock>,
    command: Command,
    username: &str,
    answers: Vec<Answer>,
) -> (Result<(), CliError>, ScriptedTerminal) {
    let mut terminal = ScriptedTerminal::new(username, clock.clone(), answers);
    let result = admin_cli::run(&command, &cli_context(pool, clock), &mut terminal).await;
    (result, terminal)
}

pub struct SeededAdmin {
    pub id: Uuid,
    pub username: String,
    pub secret: Vec<u8>,
    pub recovery_codes: Vec<String>,
}

impl SeededAdmin {
    pub fn code_now(&self, clock: &ManualClock) -> String {
        totp::code_at(&self.secret, &self.username, clock.now()).unwrap()
    }
}

/// Creates an admin with [`PASSWORD`] through `clavren-admin create-admin`, then moves the clock
/// on a minute, so the code used during setup is no longer the current one.
pub async fn seed_admin(pool: &PgPool, clock: &Arc<ManualClock>, username: &str) -> SeededAdmin {
    let command = Command::CreateAdmin { username: username.into() };
    let (result, terminal) = run_cli(pool, clock, command, username, smooth_enrolment(PASSWORD)).await;
    result.unwrap();
    clock.advance(std::time::Duration::from_secs(60));
    let id = sqlx::query_scalar("SELECT id FROM admin_users WHERE username = $1")
        .bind(username)
        .fetch_one(pool)
        .await
        .unwrap();
    SeededAdmin {
        id,
        username: username.into(),
        secret: terminal.printed_secret().unwrap(),
        recovery_codes: terminal.printed_codes(),
    }
}
