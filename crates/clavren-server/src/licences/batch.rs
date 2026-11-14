// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:42:03.141761
//+ Revised: 	2026/11/14 11:42:03.141761
// -----------------------------------------------------------------------------

//! Inserting a batch of new licences with their keys: each key hashed for lookup and stored
//! encrypted, a key that collides drawn again, and every licence audited.

use super::issue::KeySource;
use super::secrets;
use super::writes::{self, NewLicences};
use crate::audit::{self, Actor};
use crate::customers::Customer;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::money::Price;
use crate::products::Product;
use chrono::{DateTime, Utc};
use clavren_protocol::licence_key::LicenceKey;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

/// How many times a batch is retried for keys whose hash already existed.
pub(super) const KEY_ATTEMPTS: usize = 3;

pub(super) struct Batch<'a> {
    pub(super) product: &'a Product,
    pub(super) customer: Option<&'a Customer>,
    pub(super) expires_at: Option<DateTime<Utc>>,
    pub(super) notes: Option<&'a str>,
    pub(super) is_test: bool,
    pub(super) price: Option<&'a Price>,
}

/// [`insert_licences`], with one `licence.issued` audit entry per licence.
pub(super) async fn insert_audited(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    batch: &Batch<'_>,
    quantity: usize,
    generate: KeySource<'_>,
) -> Result<Vec<(Uuid, LicenceKey)>, ApiError> {
    let keys = insert_licences(state, connection, actor, batch, quantity, generate).await?;
    let ids: Vec<Uuid> = keys.iter().map(|(id, _)| *id).collect();
    let metadata = json!({
        "product": batch.product.id,
        "customer": batch.customer.map(|customer| customer.id),
        "test": batch.is_test,
    });
    audit::record_for_each(&mut *connection, actor, "licence.issued", "licence", &ids, metadata).await?;
    Ok(keys)
}

pub(super) async fn insert_licences(
    state: &AppState,
    connection: &mut PgConnection,
    actor: &Actor,
    batch: &Batch<'_>,
    quantity: usize,
    generate: KeySource<'_>,
) -> Result<Vec<(Uuid, LicenceKey)>, ApiError> {
    let mut pending: Vec<Uuid> = (0..quantity).map(|_| Uuid::now_v7()).collect();
    let mut issued = Vec::with_capacity(quantity);
    for _ in 0..KEY_ATTEMPTS {
        let keys: Vec<LicenceKey> =
            pending.iter().map(|_| generate()).collect::<Result<_, _>>().map_err(ApiError::internal)?;
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
            price: batch.price,
        };
        let inserted = writes::insert(&mut *connection, &new).await?;
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

pub(super) async fn store_keys(
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
    writes::insert_secrets(connection, &ids, &ciphertexts, &nonces, secrets::ENCRYPTION_VERSION).await?;
    Ok(())
}
