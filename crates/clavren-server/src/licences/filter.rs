// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.136897
//+ Revised: 	2026/11/10 17:59:20.765018
// -----------------------------------------------------------------------------

//! Which licences a list or an export covers: product, customer, state, expiry, and a search that
//! reads its text as a key (found by its hash, never stored or compared in plain), a licence ID, or
//! part of a customer's name or email, and whether it is a test licence. Each of these can also
//! exclude (`not_state`, `not_q`, `not_test`, …): the licences it matches are hidden instead.

use chrono::{DateTime, Utc};
use clavren_protocol::licence_key::LicenceKey;
use serde::Deserialize;
use uuid::Uuid;

use super::State;
use super::secrets::lookup_hash;
use crate::crypto::key_ring::KeyRing;
use crate::http::fields::contains_pattern;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expiry {
    /// No expiry date.
    Permanent,
    /// Expires within the next 30 days.
    Expiring,
    /// Already past its expiry date.
    Expired,
}

impl Expiry {
    fn as_str(self) -> &'static str {
        match self {
            Self::Permanent => "permanent",
            Self::Expiring => "expiring",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Most recently issued first.
    #[default]
    Newest,
    /// Soonest expiry first; permanent licences last.
    Expiry,
}

/// The filter fields of a list or export request.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterParams {
    pub product: Option<Uuid>,
    pub customer: Option<Uuid>,
    pub state: Option<State>,
    pub expiry: Option<Expiry>,
    pub q: Option<String>,
    /// `true`: only test licences; `false`: only the others.
    pub test: Option<bool>,
    pub not_product: Option<Uuid>,
    pub not_customer: Option<Uuid>,
    pub not_state: Option<State>,
    pub not_expiry: Option<Expiry>,
    pub not_q: Option<String>,
    pub not_test: Option<bool>,
}

/// What one side of a [`Filter`] matches; an unset field matches everything.
#[derive(Debug, Clone, Default)]
pub struct Criteria {
    pub product: Option<Uuid>,
    pub customer: Option<Uuid>,
    pub state: Option<&'static str>,
    pub expiry: Option<&'static str>,
    pub key_hash: Option<Vec<u8>>,
    pub licence_id: Option<Uuid>,
    pub pattern: Option<String>,
    pub test: Option<bool>,
}

/// [`FilterParams`] ready for the query: the licences matching `only` and not matching `except`.
#[derive(Debug, Clone)]
pub struct Filter {
    pub only: Criteria,
    pub except: Criteria,
    pub now: DateTime<Utc>,
}

/// One side of [`FilterParams`], as typed.
struct Side<'a> {
    product: Option<Uuid>,
    customer: Option<Uuid>,
    state: Option<State>,
    expiry: Option<Expiry>,
    q: Option<&'a str>,
    test: Option<bool>,
}

impl Criteria {
    fn new(keys: &KeyRing, side: Side<'_>) -> Self {
        let q = side.q.map(str::trim).filter(|q| !q.is_empty());
        Self {
            product: side.product,
            customer: side.customer,
            state: side.state.map(State::as_str),
            expiry: side.expiry.map(Expiry::as_str),
            key_hash: q.and_then(LicenceKey::parse).map(|key| lookup_hash(keys, &key).to_vec()),
            licence_id: q.and_then(|q| Uuid::parse_str(q).ok()),
            pattern: q.map(contains_pattern),
            test: side.test,
        }
    }
}

impl FilterParams {
    pub fn into_filter(self, keys: &KeyRing, now: DateTime<Utc>) -> Filter {
        let only = Side {
            product: self.product,
            customer: self.customer,
            state: self.state,
            expiry: self.expiry,
            q: self.q.as_deref(),
            test: self.test,
        };
        let except = Side {
            product: self.not_product,
            customer: self.not_customer,
            state: self.not_state,
            expiry: self.not_expiry,
            q: self.not_q.as_deref(),
            test: self.not_test,
        };
        Filter { only: Criteria::new(keys, only), except: Criteria::new(keys, except), now }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::master_key::MasterKey;

    fn keys() -> KeyRing {
        KeyRing::new(MasterKey::from_bytes([1; 32]))
    }

    fn filter(q: &str) -> Criteria {
        FilterParams { q: Some(q.into()), ..FilterParams::default() }.into_filter(&keys(), Utc::now()).only
    }

    #[test]
    fn a_typed_key_is_searched_by_its_hash() {
        let found = filter("brn 79yn 44xq w025 377z");
        assert_eq!(found.key_hash.map(|hash| hash.len()), Some(32));
        assert_eq!(found.licence_id, None);
        assert!(!found.pattern.unwrap().contains("BRN79YN"), "the pattern is the typed text, for names");
    }

    #[test]
    fn an_id_is_searched_as_an_id_and_text_as_a_name() {
        let id = Uuid::now_v7();
        assert_eq!(filter(&id.to_string()).licence_id, Some(id));
        let name = filter(" López ");
        assert_eq!((name.key_hash, name.licence_id, name.pattern.as_deref()), (None, None, Some("%López%")));
        assert_eq!(filter("   ").pattern, None);
    }

    #[test]
    fn an_exclusion_is_read_like_a_search() {
        let except = |q: &str| {
            let filter =
                FilterParams { not_q: Some(q.into()), ..FilterParams::default() }.into_filter(&keys(), Utc::now());
            assert!(filter.only.pattern.is_none() && filter.only.key_hash.is_none(), "only stays empty");
            filter.except
        };
        assert_eq!(except("brn 79yn 44xq w025 377z").key_hash.map(|hash| hash.len()), Some(32));
        let id = Uuid::now_v7();
        assert_eq!(except(&id.to_string()).licence_id, Some(id));
        assert_eq!(except(" López ").pattern.as_deref(), Some("%López%"));
        let blank = except("   ");
        assert_eq!((blank.key_hash, blank.licence_id, blank.pattern), (None, None, None));
    }

    #[test]
    fn test_licences_are_kept_or_hidden() {
        let read = |json: &str| serde_json::from_str::<FilterParams>(json).unwrap().into_filter(&keys(), Utc::now());
        let only = read(r#"{"test": true}"#);
        assert_eq!((only.only.test, only.except.test), (Some(true), None));
        let hidden = read(r#"{"not_test": true}"#);
        assert_eq!((hidden.only.test, hidden.except.test), (None, Some(true)));
        assert_eq!(read("{}").only.test, None, "unset matches every licence");
    }
}
