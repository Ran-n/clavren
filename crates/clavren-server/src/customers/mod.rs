// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.342000
//+ Revised: 	2026/11/03 04:54:13.342000
// -----------------------------------------------------------------------------

//! Customers: who received a key. Only administrators see them. Deleting one erases their name,
//! email and notes; their licences keep working, without the link. The audit log refers to
//! customers by ID only, so nothing personal is left behind.

pub mod repository;
mod routes;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;
pub use routes::routes;

const MAX_NAME: usize = 200;
const MAX_NOTES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct Customer {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewCustomer {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CustomerChanges {
    pub name: Option<String>,
    pub email: Patch<String>,
    pub notes: Patch<String>,
}

impl NewCustomer {
    pub fn into_customer(self, id: Uuid, now: DateTime<Utc>) -> Result<Customer, ApiError> {
        Ok(Customer {
            id,
            name: fields::required("name", &self.name, MAX_NAME)?,
            email: fields::email(self.email.as_deref())?,
            notes: fields::optional("notes", self.notes.as_deref(), MAX_NOTES)?,
            created_at: now,
            updated_at: now,
        })
    }
}

impl CustomerChanges {
    /// `customer` with these changes, and the names of the fields that changed.
    pub fn apply(self, customer: Customer, now: DateTime<Utc>) -> Result<(Customer, Vec<&'static str>), ApiError> {
        let name = match self.name {
            Some(name) => fields::required("name", &name, MAX_NAME)?,
            None => customer.name.clone(),
        };
        let email = match self.email {
            Patch::Set(email) => fields::email(Some(&email))?,
            other => other.apply(customer.email.clone()),
        };
        let notes = match self.notes {
            Patch::Set(notes) => fields::optional("notes", Some(&notes), MAX_NOTES)?,
            other => other.apply(customer.notes.clone()),
        };
        let fields: Vec<&'static str> =
            [("name", name != customer.name), ("email", email != customer.email), ("notes", notes != customer.notes)]
                .into_iter()
                .filter_map(|(field, changed)| changed.then_some(field))
                .collect();
        if fields.is_empty() {
            return Ok((customer, fields));
        }
        Ok((Customer { name, email, notes, updated_at: now, ..customer }, fields))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn ana() -> Customer {
        let new = NewCustomer {
            name: " Ana López ".into(),
            email: Some(" Ana@Example.com ".into()),
            notes: Some(" ".into()),
        };
        new.into_customer(Uuid::now_v7(), now()).unwrap()
    }

    #[test]
    fn a_new_customer_is_tidied() {
        let customer = ana();
        assert_eq!(customer.name, "Ana López");
        assert_eq!(customer.email.as_deref(), Some("Ana@Example.com"), "the address is kept as typed");
        assert_eq!(customer.notes, None);
    }

    #[test]
    fn invalid_customers_are_refused() {
        for (name, email) in [("", None), ("Ana", Some("not an address")), (&*"x".repeat(201), None)] {
            let new = NewCustomer { name: name.into(), email: email.map(String::from), notes: None };
            assert!(new.into_customer(Uuid::now_v7(), now()).is_err(), "{name:?} {email:?}");
        }
    }

    #[test]
    fn changes_report_what_changed() {
        let later = now() + chrono::Duration::hours(1);
        let changes =
            CustomerChanges { email: Patch::Clear, notes: Patch::Set("Paid by card".into()), ..Default::default() };
        let (customer, fields) = changes.apply(ana(), later).unwrap();
        assert_eq!(fields, ["email", "notes"]);
        assert_eq!(
            (customer.email, customer.notes.as_deref(), customer.updated_at),
            (None, Some("Paid by card"), later)
        );
    }

    #[test]
    fn no_real_change_keeps_the_customer_as_it_was() {
        let before = ana();
        let changes = CustomerChanges {
            name: Some("Ana López  ".into()),
            email: Patch::Set("Ana@Example.com".into()),
            ..Default::default()
        };
        let (customer, fields) = changes.apply(before.clone(), now() + chrono::Duration::hours(1)).unwrap();
        assert!(fields.is_empty());
        assert_eq!(customer, before);
    }

    #[test]
    fn invalid_changes_are_refused() {
        assert!(CustomerChanges { name: Some(" ".into()), ..Default::default() }.apply(ana(), now()).is_err());
        assert!(
            CustomerChanges { email: Patch::Set("nope".into()), ..Default::default() }.apply(ana(), now()).is_err()
        );
        assert!(
            CustomerChanges { notes: Patch::Set("x".repeat(2001)), ..Default::default() }.apply(ana(), now()).is_err()
        );
    }
}
