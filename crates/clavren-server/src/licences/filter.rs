// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.136897
//+ Revised: 	2026/11/03 04:54:15.136897
// -----------------------------------------------------------------------------

//! Which licences a list or an export covers: product, customer, state, expiry, and a search that
//! reads its text as a key (found by its hash, never stored or compared in plain), a licence ID, or
//! part of a customer's name or email.

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
}

/// [`FilterParams`] ready for the query.
#[derive(Debug, Clone)]
pub struct Filter {
    pub product: Option<Uuid>,
    pub customer: Option<Uuid>,
    pub state: Option<&'static str>,
    pub expiry: Option<&'static str>,
    pub now: DateTime<Utc>,
    pub key_hash: Option<Vec<u8>>,
    pub licence_id: Option<Uuid>,
    pub pattern: Option<String>,
}

impl FilterParams {
    pub fn into_filter(self, keys: &KeyRing, now: DateTime<Utc>) -> Filter {
        let q = self.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
        Filter {
            product: self.product,
            customer: self.customer,
            state: self.state.map(State::as_str),
            expiry: self.expiry.map(Expiry::as_str),
            now,
            key_hash: q.and_then(LicenceKey::parse).map(|key| lookup_hash(keys, &key).to_vec()),
            licence_id: q.and_then(|q| Uuid::parse_str(q).ok()),
            pattern: q.map(contains_pattern),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::master_key::MasterKey;

    fn filter(q: &str) -> Filter {
        let keys = KeyRing::new(MasterKey::from_bytes([1; 32]));
        FilterParams { q: Some(q.into()), ..FilterParams::default() }.into_filter(&keys, Utc::now())
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
}
