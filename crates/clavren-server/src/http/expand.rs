// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.613133
//+ Revised: 	2026/11/03 04:54:13.613133
// -----------------------------------------------------------------------------

//! Stripe-style expansion: a related record is its ID unless the request asks for the whole
//! object with `?expand=<field>`. The parameter repeats (`?expand=product&expand=customer`) and
//! also takes a comma-separated list. Each endpoint says which fields it can expand, and expands
//! a page with one query per field, never one per row. A dotted field (`licence.customer`)
//! expands a field of an expanded record, and implies the record itself.

use std::collections::HashMap;
use std::hash::Hash;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::Serialize;
use uuid::Uuid;

use super::error::ApiError;

#[derive(Debug, Default, Clone)]
pub struct Expand(Vec<String>);

impl Expand {
    pub fn parse(query: &str) -> Self {
        let fields = form_urlencoded::parse(query.as_bytes())
            .filter(|(name, _)| name == "expand")
            .flat_map(|(_, value)| value.split(',').map(|field| field.trim().to_string()).collect::<Vec<_>>())
            .filter(|field| !field.is_empty())
            .collect();
        Self(fields)
    }

    /// Refuses any field outside `allowed`, naming the ones that are allowed.
    pub fn check(&self, allowed: &[&str]) -> Result<(), ApiError> {
        match self.0.iter().find(|field| !allowed.contains(&field.as_str())) {
            Some(field) => Err(ApiError::invalid_request(format!(
                "{field} can't be expanded here; the fields are: {}",
                allowed.join(", ")
            ))),
            None => Ok(()),
        }
    }

    /// Asked for `field`, or for one of its own fields (`field.something`).
    pub fn has(&self, field: &str) -> bool {
        self.0.iter().any(|requested| {
            requested == field || requested.strip_prefix(field).is_some_and(|rest| rest.starts_with('.'))
        })
    }

    /// What to expand inside `field`: `licence.customer` becomes `customer`.
    pub fn nested(&self, field: &str) -> Self {
        Self(
            self.0
                .iter()
                .filter_map(|requested| requested.strip_prefix(field)?.strip_prefix('.'))
                .map(String::from)
                .collect(),
        )
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Expand {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::parse(parts.uri.query().unwrap_or_default()))
    }
}

/// A related record: its ID, or the whole object when expanded.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Expandable<T> {
    Id(Uuid),
    Object(T),
}

impl<T: Clone> Expandable<T> {
    /// The object from `loaded` when it was loaded (the field was expanded), else the ID.
    pub fn of(id: Uuid, loaded: &HashMap<Uuid, T>) -> Self {
        loaded.get(&id).cloned().map_or(Self::Id(id), Self::Object)
    }
}

/// The distinct values of `key` over `rows`, for one `WHERE id = ANY($1)` query.
pub fn distinct<R, K: Eq + Hash + Copy>(rows: &[R], key: impl Fn(&R) -> Option<K>) -> Vec<K> {
    let mut seen = std::collections::HashSet::new();
    rows.iter().filter_map(key).filter(|value| seen.insert(*value)).collect()
}

/// `records` keyed by their ID, to look related records up when expanding.
pub fn by_id<T>(records: Vec<T>, id: impl Fn(&T) -> Uuid) -> HashMap<Uuid, T> {
    records.into_iter().map(|record| (id(&record), record)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_repeat_or_come_comma_separated() {
        let expand = Expand::parse("limit=5&expand=product&expand=customer,%20active_activation&expand=");
        assert!(expand.has("product") && expand.has("customer") && expand.has("active_activation"));
        assert!(!expand.has("limit"));
        assert!(expand.check(&["product", "customer", "active_activation"]).is_ok());
    }

    #[test]
    fn unknown_fields_are_refused() {
        let error = Expand::parse("expand=owner").check(&["product"]).unwrap_err();
        assert_eq!(error.code(), "invalid_request");
        assert!(Expand::parse("").check(&[]).is_ok());
    }

    #[test]
    fn an_expandable_is_an_id_unless_loaded() {
        let id = Uuid::now_v7();
        let loaded = HashMap::from([(id, "object")]);
        assert_eq!(Expandable::of(id, &loaded), Expandable::Object("object"));
        let other = Uuid::now_v7();
        assert_eq!(Expandable::of(other, &loaded), Expandable::Id(other));
        assert_eq!(serde_json::to_value(Expandable::<u8>::Id(other)).unwrap(), serde_json::json!(other));
    }

    #[test]
    fn dotted_fields_expand_inside_an_expanded_record() {
        let expand = Expand::parse("expand=licence.customer,licence.product");
        assert!(expand.has("licence") && !expand.has("licen") && !expand.has("customer"));
        let inner = expand.nested("licence");
        assert!(inner.has("customer") && inner.has("product"));
        assert!(Expand::parse("expand=licence").nested("licence").check(&[]).is_ok(), "nothing inside");
    }

    #[test]
    fn distinct_keeps_first_appearances_in_order() {
        let rows = [Some(1), None, Some(2), Some(1), Some(3)];
        assert_eq!(distinct(&rows, |row| *row), [1, 2, 3]);
    }
}
