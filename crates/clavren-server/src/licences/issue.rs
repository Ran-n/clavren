// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.199013
//+ Revised: 	2026/11/10 17:59:20.902238
// -----------------------------------------------------------------------------

//! Issuing licences: up to 1,000 keys for one product and (optionally) one customer, existing or
//! typed in place, in one transaction. Every key gets a lookup hash and an encrypted copy; a key
//! whose hash already exists (80 random bits: practically never) is simply generated again. A
//! new product gets one test licence the same way, in the transaction that creates it.

use chrono::{DateTime, Utc};
use clavren_protocol::licence_key::LicenceKey;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::repository::{self, NewLicences};
use super::{MAX_BATCH, MAX_NOTES, secrets};
use crate::audit::{self, Actor};
use crate::customers::{self, Customer, CustomerChoice};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::fields;
use crate::products::{self, Product};

/// How many times a batch is retried for keys whose hash already existed.
const KEY_ATTEMPTS: usize = 3;

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
    if !(1..=MAX_BATCH).contains(&request.quantity) {
        return Err(ApiError::invalid_request(format!("quantity must be between 1 and {MAX_BATCH}")));
    }
    if request.expires_at.is_some_and(|at| at <= actor.at) {
        return Err(ApiError::invalid_request("expires_at must be in the future"));
    }
    let notes = fields::optional("internal_notes", request.internal_notes.as_deref(), MAX_NOTES)?;
    let mut transaction = state.pool.begin().await?;
    let product = products::repository::find_for_share(&mut transaction, request.product)
        .await?
        .ok_or_else(|| ApiError::invalid_request("product doesn't name an existing product"))?;
    if !product.is_active {
        return Err(ApiError::conflict("product_inactive", "licences can't be issued for an inactive product"));
    }
    let customer = match request.customer {
        Some(choice) => Some(customers::choose(&mut transaction, actor, choice).await?),
        None => None,
    };
    let new = Batch {
        product: &product,
        customer: customer.as_ref(),
        expires_at: request.expires_at,
        notes: notes.as_deref(),
        is_test: request.test,
    };
    let keys = insert_audited(state, &mut transaction, actor, &new, request.quantity).await?;
    transaction.commit().await?;
    Ok(Issued { keys, product, customer })
}

/// One test licence for `product` (just created in this transaction): no customer, permanent.
pub async fn issue_test_licence(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    product: &Product,
) -> Result<(Uuid, LicenceKey), ApiError> {
    let new = Batch { product, customer: None, expires_at: None, notes: None, is_test: true };
    let mut keys = insert_audited(state, connection, actor, &new, 1).await?;
    keys.pop().ok_or_else(|| ApiError::internal("the test licence wasn't issued"))
}

struct Batch<'a> {
    product: &'a Product,
    customer: Option<&'a Customer>,
    expires_at: Option<DateTime<Utc>>,
    notes: Option<&'a str>,
    is_test: bool,
}

/// [`insert_licences`], with one `licence.issued` audit entry per licence.
async fn insert_audited(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    batch: &Batch<'_>,
    quantity: usize,
) -> Result<Vec<(Uuid, LicenceKey)>, ApiError> {
    let keys = insert_licences(state, connection, actor, batch, quantity).await?;
    let ids: Vec<Uuid> = keys.iter().map(|(id, _)| *id).collect();
    let metadata = json!({
        "product": batch.product.id,
        "customer": batch.customer.map(|customer| customer.id),
        "test": batch.is_test,
    });
    audit::record_for_each(&mut *connection, actor, "licence.issued", "licence", &ids, metadata).await?;
    Ok(keys)
}

async fn insert_licences(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    batch: &Batch<'_>,
    quantity: usize,
) -> Result<Vec<(Uuid, LicenceKey)>, ApiError> {
    let mut pending: Vec<Uuid> = (0..quantity).map(|_| Uuid::now_v7()).collect();
    let mut issued = Vec::with_capacity(quantity);
    for _ in 0..KEY_ATTEMPTS {
        let keys: Vec<LicenceKey> =
            pending.iter().map(|_| secrets::generate()).collect::<Result<_, _>>().map_err(ApiError::internal)?;
        let hashes: Vec<Vec<u8>> = keys.iter().map(|key| secrets::lookup_hash(&state.keys, key).to_vec()).collect();
        let new = NewLicences {
            ids: &pending,
            key_hashes: &hashes,
            product_id: batch.product.id,
            customer_id: batch.customer.map(|customer| customer.id),
            created_at: actor.at,
            expires_at: batch.expires_at,
            internal_notes: batch.notes,
            is_test: batch.is_test,
        };
        let inserted = repository::insert(&mut *connection, &new).await?;
        let (done, retry): (Vec<_>, Vec<_>) = pending.into_iter().zip(keys).partition(|(id, _)| inserted.contains(id));
        store_keys(state, connection, batch.product.id, &done).await?;
        issued.extend(done);
        if retry.is_empty() {
            return Ok(issued);
        }
        tracing::warn!(count = retry.len(), "generated keys collided with existing ones; generating them again");
        pending = retry.into_iter().map(|(id, _)| id).collect();
    }
    Err(ApiError::internal("licence keys kept colliding with existing ones"))
}

async fn store_keys(
    state: &AppState,
    connection: &mut PgConnection,
    product_id: Uuid,
    keys: &[(Uuid, LicenceKey)],
) -> Result<(), ApiError> {
    let mut ids = Vec::with_capacity(keys.len());
    let (mut ciphertexts, mut nonces) = (Vec::with_capacity(keys.len()), Vec::with_capacity(keys.len()));
    for (id, key) in keys {
        let sealed = secrets::seal(&state.keys, key, *id, product_id).map_err(ApiError::internal)?;
        ids.push(*id);
        ciphertexts.push(sealed.ciphertext);
        nonces.push(sealed.nonce.to_vec());
    }
    repository::insert_secrets(connection, &ids, &ciphertexts, &nonces, secrets::ENCRYPTION_VERSION).await?;
    Ok(())
}
