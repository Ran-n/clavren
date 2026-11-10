// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/09 22:30:57.940226
//+ Revised: 	2026/11/09 23:01:01.002068
// -----------------------------------------------------------------------------

//! Ctrl+C and the processes the launcher starts. While a child runs (the server), the first Ctrl+C
//! is its own: it reaches the child through the shared console and the child shuts down, so the
//! launcher can stop the database afterwards. A Ctrl+C with no child running (at the username
//! prompt), or a second one (a child that doesn't stop), stops the launcher at once, after
//! stopping a database it started. On Windows every child is in a job that ends with the
//! launcher, so closing the window or killing the launcher leaves nothing running either.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::database;

static PRESSES: AtomicUsize = AtomicUsize::new(0);
static CHILD_RUNNING: AtomicBool = AtomicBool::new(false);

/// The exit code of a program stopped by Ctrl+C.
const INTERRUPTED: i32 = 130;

#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// Leave it to the child, which got the same Ctrl+C.
    LeaveToChild,
    StopNow,
}

fn action(presses: usize, child_running: bool) -> Action {
    if child_running && presses == 1 { Action::LeaveToChild } else { Action::StopNow }
}

/// Catches Ctrl+C, and on Windows puts the launcher (and so every child) in a job that ends with it.
pub fn install() -> Result<(), String> {
    #[cfg(windows)]
    if let Err(message) = job::end_children_with_launcher() {
        // still usable: only closing the window would leave the children running
        tracing::warn!(target: "xtask", "{message}; closing this window may leave the server running");
    }
    ctrlc::set_handler(|| {
        let presses = PRESSES.fetch_add(1, Ordering::SeqCst) + 1;
        if action(presses, CHILD_RUNNING.load(Ordering::SeqCst)) == Action::StopNow {
            tracing::warn!(target: "xtask", "stopping now");
            database::stop_if_started();
            std::process::exit(INTERRUPTED);
        }
    })
    .map_err(|error| format!("could not catch Ctrl+C: {error}"))
}

/// Marks a child as running until dropped, so the first Ctrl+C is left to it.
pub struct ChildRunning(());

impl ChildRunning {
    pub fn start() -> Self {
        PRESSES.store(0, Ordering::SeqCst);
        CHILD_RUNNING.store(true, Ordering::SeqCst);
        Self(())
    }
}

impl Drop for ChildRunning {
    fn drop(&mut self) {
        CHILD_RUNNING.store(false, Ordering::SeqCst);
    }
}

#[cfg(windows)]
#[allow(unsafe_code)] // the Windows job API; each call's result is checked
mod job {
    use std::ffi::c_void;

    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    /// The job's handle stays open for the launcher's whole life; Windows closes it when the
    /// launcher ends, however it ends, and that ends every process in the job.
    pub fn end_children_with_launcher() -> Result<(), String> {
        let failed = |what: &str| format!("could not {what}: {}", std::io::Error::last_os_error());
        // SAFETY: plain calls with valid arguments; `info` outlives the call that reads it.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(failed("create a job for the launcher's processes"));
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap_or(u32::MAX);
            if SetInformationJobObject(job, JobObjectExtendedLimitInformation, (&raw const info).cast::<c_void>(), size)
                == 0
            {
                return Err(failed("set up the launcher's job"));
            }
            if AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
                return Err(failed("put the launcher in its job"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, action};

    #[test]
    fn the_first_ctrl_c_is_the_childs() {
        assert_eq!(action(1, true), Action::LeaveToChild);
    }

    /// Regression: Ctrl+C did nothing at the username prompt, nor against a server that didn't stop.
    #[test]
    fn with_no_child_or_a_second_time_it_stops_at_once() {
        assert_eq!(action(1, false), Action::StopNow);
        assert_eq!(action(2, true), Action::StopNow);
    }
}
