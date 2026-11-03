// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.730978
//+ Revised: 	2026/11/03 04:54:18.896107
// -----------------------------------------------------------------------------

//! The CLI's conversation with the person at the keyboard, behind a trait so tests can script it.

use std::io::{self, BufRead, Write};

pub trait Terminal {
    fn say(&mut self, text: &str);
    fn ask(&mut self, prompt: &str) -> io::Result<String>;
    /// Like `ask`, without echoing what is typed.
    fn ask_secret(&mut self, prompt: &str) -> io::Result<String>;
}

pub struct StdTerminal;

impl Terminal for StdTerminal {
    fn say(&mut self, text: &str) {
        let mut out = io::stdout().lock();
        // a closed stdout leaves nobody to tell
        let _ = writeln!(out, "{text}");
    }

    fn ask(&mut self, prompt: &str) -> io::Result<String> {
        let mut out = io::stdout().lock();
        write!(out, "{prompt}")?;
        out.flush()?;
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "no more input"));
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }

    fn ask_secret(&mut self, prompt: &str) -> io::Result<String> {
        rpassword::prompt_password(prompt)
    }
}
