// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 11:42:03.889848
//+ Revised: 	2026/11/14 11:59:04.443014
// -----------------------------------------------------------------------------

//! What a request may say about a product, new or changed, and the checks every field goes
//! through before it reaches the database.

use super::checks::*;
use super::{Product, message_template};
use crate::http::error::ApiError;
use crate::http::fields;
use crate::http::patch::Patch;
use crate::money::{Price, PriceColumns};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

pub(super) const MAX_NAME: usize = 200;

pub(super) const MAX_DESCRIPTION: usize = 2000;

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
    #[serde(default)]
    pub accepts_applications: Option<bool>,
    #[serde(default)]
    pub key_instructions: Option<String>,
    /// Missing: the server's `CLAVREN_TOKEN_DAYS`.
    #[serde(default, deserialize_with = "whole_days")]
    pub offline_days: Option<i32>,
    #[serde(default)]
    pub price: Option<Price>,
    /// Issue one test licence with the product; on unless `false`.
    #[serde(default = "yes")]
    pub test_licence: bool,
}

pub(super) fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ProductChanges {
    pub code: Option<String>,
    pub name: Option<String>,
    pub description: Patch<String>,
    pub message_template: Option<String>,
    pub is_active: Option<bool>,
    pub accepts_applications: Option<bool>,
    pub key_instructions: Patch<String>,
    #[serde(deserialize_with = "whole_days")]
    pub offline_days: Option<i32>,
    pub price: Patch<Price>,
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
    /// tidied; `default_offline_days` when the request gives none.
    pub fn into_product(
        self,
        id: Uuid,
        now: DateTime<Utc>,
        code: &str,
        default_offline_days: i32,
    ) -> Result<Product, ApiError> {
        let template = self.message_template.unwrap_or_else(|| message_template::DEFAULT.to_string());
        Ok(Product {
            id,
            code: self::code(code)?,
            name: fields::required("name", &self.name, MAX_NAME)?,
            description: fields::optional("description", self.description.as_deref(), MAX_DESCRIPTION)?,
            message_template: template_text(&template)?,
            is_active: self.is_active.unwrap_or(true),
            accepts_applications: self.accepts_applications.unwrap_or(false),
            key_instructions: key_instructions(self.key_instructions.as_deref())?,
            offline_days: offline_days(self.offline_days.unwrap_or(default_offline_days))?,
            price: PriceColumns::from(self.price),
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
            accepts_applications: self.accepts_applications.unwrap_or(product.accepts_applications),
            key_instructions: match self.key_instructions {
                Patch::Set(text) => key_instructions(Some(&text))?,
                other => other.apply(product.key_instructions.clone()),
            },
            offline_days: self.offline_days.map(offline_days).transpose()?.unwrap_or(product.offline_days),
            price: PriceColumns::from(self.price.apply(product.price.price())),
            updated_at: now,
            ..product.clone()
        };
        let fields = changed_fields(&product, &changed);
        Ok((if fields.is_empty() { product } else { changed }, fields))
    }
}

pub(super) fn changed_fields(before: &Product, after: &Product) -> Vec<&'static str> {
    [
        ("code", before.code != after.code),
        ("name", before.name != after.name),
        ("description", before.description != after.description),
        ("message_template", before.message_template != after.message_template),
        ("is_active", before.is_active != after.is_active),
        ("accepts_applications", before.accepts_applications != after.accepts_applications),
        ("key_instructions", before.key_instructions != after.key_instructions),
        ("offline_days", before.offline_days != after.offline_days),
        ("price", before.price != after.price),
    ]
    .into_iter()
    .filter_map(|(field, changed)| changed.then_some(field))
    .collect()
}
