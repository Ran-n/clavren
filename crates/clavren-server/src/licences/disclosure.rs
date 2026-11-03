// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.076583
//+ Revised: 	2026/11/03 04:54:15.076583
// -----------------------------------------------------------------------------

//! The three ways a stored key leaves the server: **Reveal**, **Copy message** and CSV export.
//! Each is audited per licence. A revoked licence has no key left to show.

use std::collections::HashMap;

use clavren_protocol::licence_key::LicenceKey;
use serde_json::json;
use uuid::Uuid;

use super::{LicenceRow, State, repository, secrets};
use crate::audit::{self, Actor};
use crate::customers::{self, Customer};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::expand::{by_id, distinct};
use crate::products::message_template::{self, Values};
use crate::products::{self, Product};

const ENTITY: &str = "licence";
pub const CSV_HEADER: &str =
    "licence_id,product_code,customer_name,customer_email,key,state,issued_at,expires_at,notes";

pub async fn reveal(state: &AppState, actor: &Actor, id: Uuid) -> Result<LicenceKey, ApiError> {
    let key = key_of(state, id).await?;
    audit::record_for_each(&state.pool, actor, "licence.key_revealed", ENTITY, &[id], json!({})).await?;
    Ok(key)
}

/// The product's message with the customer's name and the key filled in.
pub async fn message(state: &AppState, actor: &Actor, id: Uuid) -> Result<String, ApiError> {
    let licence = repository::find_many(&state.pool, &[id]).await?.pop().ok_or_else(ApiError::not_found)?;
    let key = key_of(state, id).await?;
    let product = products::repository::find(&state.pool, licence.product_id).await?.ok_or_else(ApiError::not_found)?;
    let customer = match licence.customer_id {
        Some(customer_id) => customers::repository::find(&state.pool, customer_id).await?,
        None => None,
    };
    audit::record_for_each(&state.pool, actor, "licence.message_copied", ENTITY, &[id], json!({})).await?;
    Ok(fill_message(&product, customer.as_ref(), &[key]))
}

/// The message for several keys sent together: each `{key}` becomes the list of keys, one per line.
pub fn fill_message(product: &Product, customer: Option<&Customer>, keys: &[LicenceKey]) -> String {
    let keys = keys.iter().map(LicenceKey::formatted).collect::<Vec<_>>().join("\n");
    let values = Values {
        customer_name: customer.map_or("", |customer| customer.name.as_str()),
        product_name: &product.name,
        key: &keys,
    };
    message_template::fill(&product.message_template, &values)
}

/// `rows` as CSV, with the keys that still exist. Audited per licence.
pub async fn export(state: &AppState, actor: &Actor, rows: &[LicenceRow]) -> Result<String, ApiError> {
    let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    let products =
        by_id(products::repository::find_many(&state.pool, &distinct(rows, |row| Some(row.product_id))).await?, |p| {
            p.id
        });
    let customers =
        by_id(customers::repository::find_many(&state.pool, &distinct(rows, |row| row.customer_id)).await?, |c| c.id);
    let keys = keys_of(state, &ids).await?;
    let mut csv = String::from(CSV_HEADER);
    csv.push_str("\r\n");
    for row in rows {
        let customer = row.customer_id.and_then(|id| customers.get(&id));
        let cells = [
            row.id.to_string(),
            products.get(&row.product_id).map(|product| product.code.clone()).unwrap_or_default(),
            customer.map(|customer| customer.name.clone()).unwrap_or_default(),
            customer.and_then(|customer| customer.email.clone()).unwrap_or_default(),
            keys.get(&row.id).map(LicenceKey::formatted).unwrap_or_default(),
            row.state().as_str().to_string(),
            row.created_at.to_rfc3339(),
            row.expires_at.map(|at| at.to_rfc3339()).unwrap_or_default(),
            row.internal_notes.clone().unwrap_or_default(),
        ];
        csv.push_str(&cells.iter().map(|cell| csv_cell(cell)).collect::<Vec<_>>().join(","));
        csv.push_str("\r\n");
    }
    audit::record_for_each(&state.pool, actor, "licence.exported", ENTITY, &ids, json!({})).await?;
    tracing::info!(count = ids.len(), "licences exported");
    Ok(csv)
}

/// One CSV cell. A value a spreadsheet would run as a formula (starting with `=`, `+`, `-`, `@`,
/// tab or carriage return) gets a leading `'`; a value with a comma, quote or line break is quoted.
pub fn csv_cell(value: &str) -> String {
    let guarded =
        if value.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{value}") } else { value.to_string() };
    if guarded.contains([',', '"', '\n', '\r']) { format!("\"{}\"", guarded.replace('"', "\"\"")) } else { guarded }
}

async fn key_of(state: &AppState, id: Uuid) -> Result<LicenceKey, ApiError> {
    if let Some(key) = keys_of(state, &[id]).await?.remove(&id) {
        return Ok(key);
    }
    match repository::find_many(&state.pool, &[id]).await?.pop() {
        Some(licence) if licence.state() == State::Revoked => {
            Err(ApiError::conflict("licence_revoked", "the key was deleted when the licence was revoked"))
        }
        _ => Err(ApiError::not_found()),
    }
}

async fn keys_of(state: &AppState, ids: &[Uuid]) -> Result<HashMap<Uuid, LicenceKey>, ApiError> {
    repository::stored_keys(&state.pool, ids)
        .await?
        .into_iter()
        .map(|stored| {
            let key =
                secrets::open(&state.keys, &stored.ciphertext, &stored.nonce, stored.license_id, stored.product_id)
                    .map_err(ApiError::internal)?;
            Ok((stored.license_id, key))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_that_spreadsheets_would_run_are_defused() {
        for (value, written) in [
            ("=SUM(A1)", "'=SUM(A1)"),
            ("+34 600", "'+34 600"),
            ("-1", "'-1"),
            ("@home", "'@home"),
            ("\tx", "'\tx"),
            ("\rx", "\"'\rx\""),
        ] {
            assert_eq!(csv_cell(value), written, "{value:?}");
        }
    }

    #[test]
    fn cells_with_separators_are_quoted() {
        assert_eq!(csv_cell("López, Ana"), "\"López, Ana\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("two\nlines"), "\"two\nlines\"");
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell(""), "");
    }

    #[test]
    fn the_header_has_the_documented_columns() {
        assert_eq!(CSV_HEADER.split(',').count(), 9);
    }
}
