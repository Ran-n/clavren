// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:10.705382
//+ Revised: 	2026/11/03 04:54:10.705382
// -----------------------------------------------------------------------------

//! The time, behind a trait so tests can move it (and set it back).

use std::sync::Mutex;

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// A shared clock is a clock: the application and its tests can hold the same one.
impl<T: Clock + ?Sized> Clock for std::sync::Arc<T> {
    fn now(&self) -> DateTime<Utc> {
        (**self).now()
    }
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock that only moves when told to.
pub struct ManualClock(Mutex<DateTime<Utc>>);

impl ManualClock {
    pub fn new(start: DateTime<Utc>) -> Self {
        Self(Mutex::new(start))
    }

    pub fn set(&self, to: DateTime<Utc>) {
        *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = to;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
