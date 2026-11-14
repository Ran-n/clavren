// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.342000
//+ Revised: 	2026/11/14 11:36:30.837189
// -----------------------------------------------------------------------------

//! Customers: who received a key. Only administrators see them. Deleting one erases their name,
//! email and notes; their licences keep working, without the link. The audit log refers to
//! customers by ID only, so nothing personal is left behind. A licence's customer is picked, or
//! typed in place as a new one, the same way when issuing and when editing ([`choose`]).

pub mod repository;
mod routes;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::audit::{self, Actor, AuditEntry};
use crate::database::violates_unique;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;
pub use routes::routes;

pub(crate) const ENTITY: &str = "customer";
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

/// An existing customer's ID, or a new customer typed in place.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CustomerChoice {
    Existing(Uuid),
    New(NewCustomer),
}

/// The customer `choice` names: an existing one (share-locked until the transaction ends, so it
/// can't be deleted meanwhile), or a new one created now (and audited). Call it before locking
/// licences: deleting a customer locks the customer first, then its licences.
pub async fn choose(
    connection: &mut PgConnection,
    actor: &Actor,
    choice: CustomerChoice,
) -> Result<Customer, ApiError> {
    match choice {
        CustomerChoice::Existing(id) => repository::find_for_share(&mut *connection, id)
            .await?
            .ok_or_else(|| ApiError::invalid_request("customer doesn't name an existing customer")),
        CustomerChoice::New(new) => {
            let customer = new.into_customer(Uuid::now_v7(), actor.at)?;
            create(connection, actor, &customer).await?;
            Ok(customer)
        }
    }
}

/// Saves a new customer and audits it.
pub(crate) async fn create(connection: &mut PgConnection, actor: &Actor, customer: &Customer) -> Result<(), ApiError> {
    repository::insert(&mut *connection, customer).await.map_err(email_taken)?;
    audit::record(connection, AuditEntry::by(actor, "customer.created", ENTITY, customer.id)).await?;
    Ok(())
}

/// The database refusing a second customer with an email (ignoring case) as the conflict it is.
pub(crate) fn email_taken(error: sqlx::Error) -> ApiError {
    if violates_unique(&error, repository::EMAIL_CONSTRAINT) {
        ApiError::conflict("customer_email_taken", "another customer already has this email")
    } else {
        error.into()
    }
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
mod tests;
