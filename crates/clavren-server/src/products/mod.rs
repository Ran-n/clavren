// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.963172
//+ Revised: 	2026/11/03 04:54:15.963172
// -----------------------------------------------------------------------------

//! Products: the applications that use Clavren. A product's `code` is the audience of every token
//! issued for it, so it can't change once the product has licences; a product with licences is
//! deactivated, never deleted.

pub mod message_template;
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewProduct {
    pub code: String,
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
    /// The product to store, checked and tidied.
    pub fn into_product(self, id: Uuid, now: DateTime<Utc>) -> Result<Product, ApiError> {
        let template = self.message_template.unwrap_or_else(|| message_template::DEFAULT.to_string());
        Ok(Product {
            id,
            code: code(&self.code)?,
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
