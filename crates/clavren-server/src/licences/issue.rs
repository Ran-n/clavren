// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.199013
//+ Revised: 	2026/11/14 11:42:03.226060
// -----------------------------------------------------------------------------

//! Issuing licences: up to 1,000 keys for one product and (optionally) one customer, existing or
//! typed in place, in one transaction. Every key gets a lookup hash and an encrypted copy; a key
//! whose hash already exists (80 random bits: practically never) is simply generated again. A
//! new product gets one test licence the same way, in the transaction that creates it.

use chrono::{DateTime, Utc};
use clavren_protocol::licence_key::LicenceKey;
use serde::Deserialize;
use sqlx::PgConnection;
use uuid::Uuid;

use super::batch::{Batch, insert_audited};
use super::{MAX_BATCH, MAX_NOTES, secrets};
use crate::audit::Actor;
use crate::crypto::random::RandomError;
use crate::customers::{self, Customer, CustomerChoice};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;
use crate::money::Price;
use crate::products::{self, Product};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueRequest {
    pub product: Uuid,
    #[serde(default)]
    pub customer: Option<CustomerChoice>,
    #[serde(default = "one")]
    pub quantity: usize,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub internal_notes: Option<String>,
    /// Test licences: for trying the application, on any number of computers.
    #[serde(default)]
    pub test: bool,
    /// What each key is charged. Missing: the product's price (none for test licences); `null`:
    /// none recorded.
    #[serde(default)]
    pub price: Patch<Price>,
}

fn one() -> usize {
    1
}

pub struct Issued {
    pub keys: Vec<(Uuid, LicenceKey)>,
    pub product: Product,
    pub customer: Option<Customer>,
}

pub async fn issue(state: &AppState, actor: &Actor, request: IssueRequest) -> Result<Issued, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let issued = issue_in(state, &mut transaction, actor, request).await?;
    transaction.commit().await?;
    Ok(issued)
}

/// Where new keys come from: the operating system's random numbers, except in tests of what
/// happens when a key already exists.
pub type KeySource<'a> = &'a mut (dyn FnMut() -> Result<LicenceKey, RandomError> + Send);

/// [`issue`], in the caller's transaction (approving a licence application issues its licence and
/// records the approval together).
pub async fn issue_in(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    request: IssueRequest,
) -> Result<Issued, ApiError> {
    issue_with_keys(state, connection, actor, request, &mut secrets::generate).await
}

/// [`issue_in`], with the keys from `generate`.
pub async fn issue_with_keys(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    request: IssueRequest,
    generate: KeySource<'_>,
) -> Result<Issued, ApiError> {
    if !(1..=MAX_BATCH).contains(&request.quantity) {
        return Err(ApiError::invalid_request(format!("quantity must be between 1 and {MAX_BATCH}")));
    }
    if request.expires_at.is_some_and(|at| at <= actor.at) {
        return Err(ApiError::invalid_request("expires_at must be in the future"));
    }
    let notes = fields::optional("internal_notes", request.internal_notes.as_deref(), MAX_NOTES)?;
    if request.test && matches!(request.price, Patch::Set(_)) {
        return Err(ApiError::invalid_request("test licences can't have a price"));
    }
    let product = products::repository::find_for_share(&mut *connection, request.product)
        .await?
        .ok_or_else(|| ApiError::invalid_request("product doesn't name an existing product"))?;
    if !product.is_active {
        return Err(ApiError::conflict("product_inactive", "licences can't be issued for an inactive product"));
    }
    let customer = match request.customer {
        Some(choice) => Some(customers::choose(&mut *connection, actor, choice).await?),
        None => None,
    };
    let price = match request.price {
        // a test licence is never a sale, whatever the product costs
        Patch::Keep if request.test => None,
        Patch::Keep => product.price.price(),
        other => other.apply(None),
    };
    let new = Batch {
        product: &product,
        customer: customer.as_ref(),
        expires_at: request.expires_at,
        notes: notes.as_deref(),
        is_test: request.test,
        price: price.as_ref(),
    };
    let keys = insert_audited(state, connection, actor, &new, request.quantity, generate).await?;
    Ok(Issued { keys, product, customer })
}

/// One test licence for `product` (just created in this transaction): no customer, permanent.
pub async fn issue_test_licence(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    product: &Product,
) -> Result<(Uuid, LicenceKey), ApiError> {
    let new = Batch { product, customer: None, expires_at: None, notes: None, is_test: true, price: None };
    let mut keys = insert_audited(state, connection, actor, &new, 1, &mut secrets::generate).await?;
    keys.pop().ok_or_else(|| ApiError::internal("the test licence wasn't issued"))
}
