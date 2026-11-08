// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.963172
//+ Revised: 	2026/11/07 19:36:59.974743
// -----------------------------------------------------------------------------

//! Products: the applications that use Clavren. A product's `code` is the audience of every token
//! issued for it, so it can't change once the product has licences. Deactivating a product pauses
//! it and keeps everything; deleting it deletes its licences and their history too, and the
//! computers using them lock at their next check. Without a code, one is made from the name.

mod automatic_code;
pub mod message_template;
pub mod repository;
mod routes;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::audit::{self, Actor, AuditEntry};
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;
use crate::licences::{self, Deleted};
pub use routes::routes;

const MAX_NAME: usize = 200;
const MAX_DESCRIPTION: usize = 2000;
const MAX_TEMPLATE: usize = 10_000;
const MAX_CODE: usize = 63;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Product {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub message_template: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How many licences a product has, and how many of them are in use on a computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, FromRow)]
pub struct Usage {
    pub licences: i64,
    pub in_use: i64,
}

/// Deletes `product` (locked by the caller's transaction) with every licence it has, their keys
/// and their history, auditing each licence and the product. Set-based: no limit on how many.
pub async fn delete_with_licences(
    connection: &mut PgConnection,
    actor: &Actor,
    product: &Product,
) -> Result<Deleted, ApiError> {
    let ids = licences::repository::ids_of_product(&mut *connection, product.id).await?;
    let deleted = licences::delete_locked(&mut *connection, actor, &ids, false).await?;
    repository::delete(&mut *connection, product.id).await?;
    let metadata = json!({ "licences": deleted.deleted, "in_use": deleted.in_use });
    audit::record(
        &mut *connection,
        AuditEntry::by(actor, "product.deleted", "product", product.id).with_metadata(metadata),
    )
    .await?;
    Ok(deleted)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewProduct {
    /// Missing or blank: made from the name.
    #[serde(default)]
    pub code: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub message_template: Option<String>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ProductChanges {
    pub code: Option<String>,
    pub name: Option<String>,
    pub description: Patch<String>,
    pub message_template: Option<String>,
    pub is_active: Option<bool>,
}

impl NewProduct {
    /// The code as typed, trimmed; `None` when it is missing or blank.
    pub fn typed_code(&self) -> Option<&str> {
        self.code.as_deref().map(str::trim).filter(|code| !code.is_empty())
    }

    /// Checks the name on its own, before a code is made from it.
    pub fn check_name(&self) -> Result<(), ApiError> {
        fields::required("name", &self.name, MAX_NAME).map(drop)
    }

    /// The product to store with `code` (typed or made: both meet the same check), checked and
    /// tidied.
    pub fn into_product(self, id: Uuid, now: DateTime<Utc>, code: &str) -> Result<Product, ApiError> {
        let template = self.message_template.unwrap_or_else(|| message_template::DEFAULT.to_string());
        Ok(Product {
            id,
            code: self::code(code)?,
            name: fields::required("name", &self.name, MAX_NAME)?,
            description: fields::optional("description", self.description.as_deref(), MAX_DESCRIPTION)?,
            message_template: template_text(&template)?,
            is_active: self.is_active.unwrap_or(true),
            created_at: now,
            updated_at: now,
        })
    }
}

impl ProductChanges {
    /// `product` with these changes, and the names of the fields that changed.
    pub fn apply(self, product: Product, now: DateTime<Utc>) -> Result<(Product, Vec<&'static str>), ApiError> {
        let changed = Product {
            code: self.code.as_deref().map(code).transpose()?.unwrap_or_else(|| product.code.clone()),
            name: self
                .name
                .as_deref()
                .map(|name| fields::required("name", name, MAX_NAME))
                .transpose()?
                .unwrap_or_else(|| product.name.clone()),
            description: match self.description {
                Patch::Set(text) => fields::optional("description", Some(&text), MAX_DESCRIPTION)?,
                other => other.apply(product.description.clone()),
            },
            message_template: self
                .message_template
                .as_deref()
                .map(template_text)
                .transpose()?
                .unwrap_or_else(|| product.message_template.clone()),
            is_active: self.is_active.unwrap_or(product.is_active),
            updated_at: now,
            ..product.clone()
        };
        let fields = changed_fields(&product, &changed);
        Ok((if fields.is_empty() { product } else { changed }, fields))
    }
}

fn changed_fields(before: &Product, after: &Product) -> Vec<&'static str> {
    [
        ("code", before.code != after.code),
        ("name", before.name != after.name),
        ("description", before.description != after.description),
        ("message_template", before.message_template != after.message_template),
        ("is_active", before.is_active != after.is_active),
    ]
    .into_iter()
    .filter_map(|(field, changed)| changed.then_some(field))
    .collect()
}

/// Lowercase letters, digits and `-`, starting with a letter or digit; typed in any case.
/// Another product already has the code, typed or made.
fn code_taken_error() -> ApiError {
    ApiError::conflict("product_code_taken", "another product already has this code")
}

fn code(value: &str) -> Result<String, ApiError> {
    let code = value.trim().to_ascii_lowercase();
    let mut characters = code.chars();
    let valid = characters.next().is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && characters.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && code.len() <= MAX_CODE;
    if valid {
        Ok(code)
    } else {
        Err(ApiError::invalid_request(
            "code must be up to 63 lowercase letters, digits and dashes, starting with a letter or digit",
        ))
    }
}

fn template_text(value: &str) -> Result<String, ApiError> {
    if value.chars().count() > MAX_TEMPLATE {
        return Err(ApiError::invalid_request(format!("message_template must be at most {MAX_TEMPLATE} characters")));
    }
    match message_template::unknown_placeholder(value) {
        Some(name) => Err(ApiError::invalid_request(format!(
            "message_template uses {{{name}}}; the placeholders are {{customer_name}}, {{product_name}} and {{key}}"
        ))),
        None => Ok(value.to_string()),
    }
}

#[cfg(test)]
mod tests;
