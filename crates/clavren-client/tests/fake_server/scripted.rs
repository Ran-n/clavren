// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 17:00:51.136000
//+ Revised: 	2026/11/14 17:00:51.136000
// -----------------------------------------------------------------------------

//! Answers a test scripts for single paths of the fake server, whatever its mode.

use clavren_client::transport::Answer;
use serde_json::Value;

/// What the paths ending in a given suffix answer.
#[allow(dead_code)] // not every test binary scripts both kinds
#[derive(Clone)]
pub enum Scripted {
    /// This status and body.
    Answer(u16, Value),
    /// Nothing: the connection fails.
    Offline,
}

impl Scripted {
    pub fn answer(&self) -> Result<Answer, String> {
        match self {
            Self::Answer(status, body) => Ok(super::answer(*status, body.clone())),
            Self::Offline => Err("connection refused".into()),
        }
    }
}
