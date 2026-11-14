// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:35.730978
//+ Revised: 	2026/11/14 00:38:42.703164
// -----------------------------------------------------------------------------

//! The CLI's conversation with the person at the keyboard, behind a trait so tests can script it.

use std::io::{self, BufRead, Write};

pub trait Terminal {
    /// Says something that helps but isn't needed: nothing depends on it being read.
    fn say(&mut self, text: &str);
    /// Shows something the person must get (recovery codes): fails when it can't be written.
    fn show(&mut self, text: &str) -> io::Result<()> {
        self.say(text);
        Ok(())
    }
    fn ask(&mut self, prompt: &str) -> io::Result<String>;
    /// Like `ask`, without echoing what is typed.
    fn ask_secret(&mut self, prompt: &str) -> io::Result<String>;
}

pub struct StdTerminal;

impl Terminal for StdTerminal {
    fn say(&mut self, text: &str) {
        let mut out = io::stdout().lock();
        // progress and hints: with stdout closed there is nobody to tell, and whatever must be
        // read goes through `show`
        let _ = writeln!(out, "{text}");
    }

    fn show(&mut self, text: &str) -> io::Result<()> {
        let mut out = io::stdout().lock();
        writeln!(out, "{text}")?;
        out.flush()
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
