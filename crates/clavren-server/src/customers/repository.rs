// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.438401
//+ Revised: 	2026/11/09 22:30:54.712521
// -----------------------------------------------------------------------------

//! Database access for customers.

use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use super::Customer;
use crate::http::pagination::Cursor;

/// The unique index on `lower(email)`.
pub const EMAIL_CONSTRAINT: &str = "customers_email_unique";

/// How a customer list is ordered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// A to Z, ignoring case.
    #[default]
    Name,
    /// Most recently added first.
    Newest,
}

macro_rules! select_customers {
    ($rest:literal) => {
        concat!(
            "SELECT id, name, email, notes, created_at, updated_at FROM customers
             WHERE ($1::text IS NULL OR name ILIKE $1 OR email ILIKE $1) ",
            $rest
        )
    };
}

pub async fn insert(executor: impl PgExecutor<'_>, customer: &Customer) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO customers (id, name, email, notes, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(customer.id)
    .bind(&customer.name)
    .bind(&customer.email)
    .bind(&customer.notes)
    .bind(customer.created_at)
    .bind(customer.updated_at)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn find(executor: impl PgExecutor<'_>, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
    sqlx::query_as(select_customers!("AND id = $2")).bind(None::<String>).bind(id).fetch_optional(executor).await
}

/// The customer, share-locked until the transaction ends: it can be given to licences in
/// parallel, but not deleted meanwhile (which would leave the licences pointing at nothing).
pub async fn find_for_share(connection: &mut PgConnection, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
    sqlx::query_as(select_customers!("AND id = $2 FOR SHARE"))
        .bind(None::<String>)
        .bind(id)
        .fetch_optional(connection)
        .await
}

pub async fn find_for_update(connection: &mut PgConnection, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
    sqlx::query_as(select_customers!("AND id = $2 FOR UPDATE"))
        .bind(None::<String>)
        .bind(id)
        .fetch_optional(connection)
        .await
}

pub async fn update(executor: impl PgExecutor<'_>, customer: &Customer) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE customers SET name = $2, email = $3, notes = $4, updated_at = $5 WHERE id = $1")
        .bind(customer.id)
        .bind(&customer.name)
        .bind(&customer.email)
        .bind(&customer.notes)
        .bind(customer.updated_at)
        .execute(executor)
        .await?;
    Ok(())
}

/// Deletes the customer; their licences lose the link (`ON DELETE SET NULL`).
pub async fn delete(executor: impl PgExecutor<'_>, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM customers WHERE id = $1").bind(id).execute(executor).await?;
    Ok(())
}

/// Customers matching `pattern` (an `ILIKE` pattern on name or email), after `cursor`.
pub async fn page(
    executor: impl PgExecutor<'_>,
    pattern: Option<&str>,
    order: Order,
    cursor: Option<&Cursor>,
    count: i64,
) -> Result<Vec<Customer>, sqlx::Error> {
    let query = match order {
        Order::Name => sqlx::query_as(select_customers!(
            "AND ($2::text IS NULL OR (lower(name), id) > (lower($2), $3)) ORDER BY lower(name), id LIMIT $4"
        )),
        // IDs are UUIDv7, so their order is the order they were created in
        Order::Newest => {
            sqlx::query_as(select_customers!("AND ($3::uuid IS NULL OR id < $3) ORDER BY id DESC LIMIT $4"))
        }
    };
    query
        .bind(pattern)
        .bind(cursor.and_then(|cursor| cursor.key.clone()))
        .bind(cursor.map(|cursor| cursor.id))
        .bind(count)
        .fetch_all(executor)
        .await
}

pub fn cursor_after(order: Order) -> impl Fn(&Customer) -> Cursor {
    move |customer| match order {
        Order::Name => Cursor { key: Some(customer.name.clone()), id: customer.id },
        Order::Newest => Cursor { key: None, id: customer.id },
    }
}

/// Whether `cursor` came from a page in `order`: a cursor from another order would continue at
/// the wrong place.
pub fn cursor_fits(order: Order, cursor: &Cursor) -> bool {
    match order {
        Order::Name => cursor.key.is_some(),
        Order::Newest => cursor.key.is_none(),
    }
}

pub async fn find_many(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<Customer>, sqlx::Error> {
    sqlx::query_as(select_customers!("AND id = ANY($2)")).bind(None::<String>).bind(ids).fetch_all(executor).await
}
