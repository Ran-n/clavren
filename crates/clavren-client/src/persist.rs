// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:48:57.374613
//+ Revised: 	2026/11/14 11:48:57.374613
// -----------------------------------------------------------------------------

//! Saving the state: what the store held when this program last read or wrote it, so two
//! copies of the application never save over each other, and the time seen saved now and then.

use crate::client::Client;
use crate::state::State;
use crate::store::StoreError;
use chrono::{DateTime, TimeDelta, Utc};
use std::sync::PoisonError;
use zeroize::Zeroize;

/// [`Client::save_time_seen`] saves the time seen once it moved this far: a few minutes are
/// nothing next to [`ROLLBACK_TOLERANCE`].
const SEEN_SAVE_STEP: TimeDelta = TimeDelta::minutes(10);

/// What the store holds, as far as this program knows: a save only goes through while that still
/// holds, so two copies of the application never save over each other's licence.
pub(crate) enum Stored {
    /// These bytes (`None`: nothing yet).
    Known(Option<Vec<u8>>),
    /// Something this program couldn't make sense of (damaged, or another account's): it starts
    /// over and saves over it.
    Unknown,
}

impl Stored {
    fn replace(&mut self, new: Self) {
        if let Self::Known(Some(bytes)) = std::mem::replace(self, new) {
            let mut bytes = bytes;
            bytes.zeroize();
        }
    }
}

/// The state saved as `bytes`; a damaged one counts as none.
pub(crate) fn parse_state(bytes: &[u8]) -> State {
    serde_json::from_slice(bytes).unwrap_or_else(|error| {
        log::warn!("the saved licence state is damaged ({error}); starting over");
        State::default()
    })
}

/// What [`Client::save_time_seen`] compares with the state to decide whether to save.
#[derive(Clone, Copy)]
pub(crate) struct Seen {
    latest: Option<DateTime<Utc>>,
    rewound_seconds: i64,
}

impl Seen {
    pub(crate) fn of(state: &State) -> Self {
        Self { latest: state.latest_seen, rewound_seconds: state.rewound_seconds }
    }
}

impl Client {
    /// Saves the latest time seen once it moved [`SEEN_SAVE_STEP`] past the saved one, and any
    /// step back of the clock, so a clock set back between runs is noticed too. The background
    /// checker calls it every few minutes and when it stops; an application without one calls it
    /// off the UI thread.
    pub fn save_time_seen(&self) -> Result<(), StoreError> {
        let mut state = self.lock();
        let saved = *self.saved_seen.lock().unwrap_or_else(PoisonError::into_inner);
        let moved = match (state.latest_seen, saved.latest) {
            (Some(latest), Some(saved)) => latest - saved >= SEEN_SAVE_STEP,
            (latest, saved) => latest.is_some() && saved.is_none(),
        };
        if !moved && state.rewound_seconds == saved.rewound_seconds {
            return Ok(());
        }
        let seen = state.clone();
        self.commit(&mut state, seen)
    }

    /// Saves `new` and makes it the state; `state` (the locked state) stays as it was when the
    /// save fails, so nothing is ever kept in memory that isn't on the disk. When another copy of
    /// the application saved since this one last read the store, nothing is saved over it: this
    /// copy takes what the other saved, and the caller's change is lost
    /// ([`StoreError::ChangedElsewhere`]).
    pub(crate) fn commit(&self, state: &mut State, new: State) -> Result<(), StoreError> {
        if let Some(reason) = &self.unreadable {
            return Err(StoreError::Unreadable(reason.clone()));
        }
        let mut bytes = serde_json::to_vec(&new).map_err(|_| StoreError::Corrupt)?;
        let mut stored = self.stored.lock().unwrap_or_else(PoisonError::into_inner);
        let saved = match &*stored {
            Stored::Known(expected) => self.store.save_if_unchanged(expected.as_deref(), &bytes),
            Stored::Unknown => self.store.save(&bytes).map(|()| true),
        };
        match saved {
            Ok(true) => {
                stored.replace(Stored::Known(Some(bytes)));
                *self.saved_seen.lock().unwrap_or_else(PoisonError::into_inner) = Seen::of(&new);
                *state = new;
                Ok(())
            }
            Ok(false) => {
                bytes.zeroize();
                self.adopt_stored(state, &mut stored);
                Err(StoreError::ChangedElsewhere)
            }
            Err(error) => {
                bytes.zeroize();
                Err(error)
            }
        }
    }

    /// Takes what another copy of the application saved as this copy's state.
    fn adopt_stored(&self, state: &mut State, stored: &mut Stored) {
        match self.store.load() {
            Ok(bytes) => {
                log::warn!("another copy of the application changed the saved licence; loading it again");
                *state = bytes.as_deref().map(parse_state).unwrap_or_default();
                *self.saved_seen.lock().unwrap_or_else(PoisonError::into_inner) = Seen::of(state);
                stored.replace(Stored::Known(bytes));
            }
            // still what this copy last knew; the next save finds the difference again
            Err(error) => log::error!("the licence another copy of the application saved can't be read: {error}"),
        }
    }
}
