// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.059583
//+ Revised: 	2026/11/07 19:37:00.056784
// -----------------------------------------------------------------------------

//! Database access for products.

use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::{Product, Usage};
use crate::http::pagination::Cursor;

/// The unique constraint on `products.code`.
pub const CODE_CONSTRAINT: &str = "products_code_key";

macro_rules! select_products {
    ($rest:literal) => {
        concat!(
            "SELECT id, code, name, description, message_template, is_active, created_at, updated_at FROM products ",
            $rest
        )
    };
}

pub async fn insert(executor: impl PgExecutor<'_>, product: &Product) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO products (id, code, name, description, message_template, is_active, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(product.id)
    .bind(&product.code)
    .bind(&product.name)
    .bind(&product.description)
    .bind(&product.message_template)
    .bind(product.is_active)
    .bind(product.created_at)
    .bind(product.updated_at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn find(executor: impl PgExecutor<'_>, id: Uuid) -> Result<Option<Product>, sqlx::Error> {
    sqlx::query_as(select_products!("WHERE id = $1")).bind(id).fetch_optional(executor).await
}

/// The product, locked until the transaction ends, so two edits can't overwrite each other.
pub async fn find_for_update(connection: &mut PgConnection, id: Uuid) -> Result<Option<Product>, sqlx::Error> {
    sqlx::query_as(select_products!("WHERE id = $1 FOR UPDATE")).bind(id).fetch_optional(connection).await
}

pub async fn update(executor: impl PgExecutor<'_>, product: &Product) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE products SET code = $2, name = $3, description = $4, message_template = $5, is_active = $6,
         updated_at = $7 WHERE id = $1",
    )
    .bind(product.id)
    .bind(&product.code)
    .bind(&product.name)
    .bind(&product.description)
    .bind(&product.message_template)
    .bind(product.is_active)
    .bind(product.updated_at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn delete(executor: impl PgExecutor<'_>, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM products WHERE id = $1").bind(id).execute(executor).await?;
    Ok(())
}

pub async fn code_exists(executor: impl PgExecutor<'_>, code: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM products WHERE code = $1)").bind(code).fetch_one(executor).await
}

/// How many licences the product has, and how many of them are in use on a computer.
pub async fn usage(executor: impl PgExecutor<'_>, id: Uuid) -> Result<Usage, sqlx::Error> {
    sqlx::query_as(
        "SELECT count(*) AS licences, count(*) FILTER (WHERE s.state = 'redeemed') AS in_use
         FROM licenses l JOIN license_states s ON s.license_id = l.id
         WHERE l.product_id = $1",
    )
    .bind(id)
    .fetch_one(executor)
    .await
}

/// Products by name, after `cursor`, optionally only active or inactive ones.
pub async fn page(
    executor: impl PgExecutor<'_>,
    active: Option<bool>,
    cursor: Option<&Cursor>,
    count: i64,
) -> Result<Vec<Product>, sqlx::Error> {
    sqlx::query_as(select_products!(
        "WHERE ($1::boolean IS NULL OR is_active = $1)
           AND ($2::text IS NULL OR (lower(name), id) > (lower($2), $3))
         ORDER BY lower(name), id LIMIT $4"
    ))
    .bind(active)
    .bind(cursor.and_then(|cursor| cursor.key.clone()))
    .bind(cursor.map(|cursor| cursor.id))
    .bind(count)
    .fetch_all(executor)
    .await
}

/// Where a page ending at `product` continues.
pub fn cursor_after(product: &Product) -> Cursor {
    Cursor { key: Some(product.name.clone()), id: product.id }
}

pub async fn find_many(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<Product>, sqlx::Error> {
    sqlx::query_as(select_products!("WHERE id = ANY($1)")).bind(ids).fetch_all(executor).await
}

/// The product, share-locked until the transaction ends: licences can be issued for it in
/// parallel, but its code can't change meanwhile.
pub async fn find_for_share(connection: &mut PgConnection, id: Uuid) -> Result<Option<Product>, sqlx::Error> {
    sqlx::query_as(select_products!("WHERE id = $1 FOR SHARE")).bind(id).fetch_optional(connection).await
}
