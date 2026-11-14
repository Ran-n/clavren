// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:15.624141
//+ Revised: 	2026/11/14 11:38:23.903746
// -----------------------------------------------------------------------------

//! Database access for licences and their encrypted keys.

use std::sync::LazyLock;

use chrono::DateTime;
use sqlx::postgres::PgArguments;
use sqlx::query::QueryAs;
use sqlx::{FromRow, PgConnection, PgExecutor, Postgres};
use uuid::Uuid;

use super::LicenceRow;
use super::filter::{Criteria, Filter, Order};
use crate::http::pagination::Cursor;

/// Every [`LicenceRow`] column.
const SELECT_LICENCES: &str =
    "SELECT l.id, l.product_id, l.customer_id, s.state, s.active_activation_id, s.active_computers, l.is_test,
            l.created_at, l.expires_at, l.revoked_at, l.revocation_reason, l.internal_notes, l.price_amount,
            l.price_currency
     FROM licenses l
     JOIN license_states s ON s.license_id = l.id
     LEFT JOIN customers c ON c.id = l.customer_id";

/// How many parameters one set of [`Criteria`] takes.
const CRITERIA: usize = 8;
/// The parameter holding `now` in a [`page`] query, after the two sets of criteria ($1 to $16).
const NOW: usize = 2 * CRITERIA + 1;

/// What each [`Criteria`] field matches, from its first parameter `$n` on: the condition, and the
/// parameters that leave it unset when all are `NULL`.
fn matches(n: usize) -> [(String, Vec<usize>); 6] {
    let (product, customer, state, expiry, key, id, pattern) = (n, n + 1, n + 2, n + 3, n + 4, n + 5, n + 6);
    let test = n + 7;
    [
        (format!("l.product_id = ${product}::uuid"), vec![product]),
        (format!("l.customer_id = ${customer}::uuid"), vec![customer]),
        (format!("s.state = ${state}::text"), vec![state]),
        (
            format!(
                "(${expiry}::text = 'permanent' AND l.expires_at IS NULL)
                 OR (${expiry} = 'expiring' AND l.expires_at > ${NOW}::timestamptz
                     AND l.expires_at <= ${NOW} + interval '30 days')
                 OR (${expiry} = 'expired' AND l.expires_at <= ${NOW})"
            ),
            vec![expiry],
        ),
        (
            format!(
                "l.key_lookup = ${key}::bytea OR l.id = ${id}::uuid
                 OR c.name ILIKE ${pattern}::text OR c.email ILIKE ${pattern}"
            ),
            vec![key, id, pattern],
        ),
        (format!("l.is_test = ${test}::boolean"), vec![test]),
    ]
}

/// The `WHERE` condition of a [`Filter`]: every `only` criterion ($1 to $8) holds, and no `except`
/// one ($9 to $16) does. An exclusion is `NOT COALESCE(…, false)` so a licence the criterion can't
/// say anything about (no customer, no expiry) isn't hidden by it.
fn filter_condition() -> String {
    let only = matches(1).map(|(condition, params)| {
        let unset = params.iter().map(|n| format!("${n} IS NULL")).collect::<Vec<_>>().join(" AND ");
        format!("(({unset}) OR {condition})")
    });
    let except = matches(CRITERIA + 1).map(|(condition, _)| format!("NOT COALESCE({condition}, false)"));
    only.into_iter().chain(except).collect::<Vec<_>>().join("\n AND ")
}

/// A [`page`] query in each order; the cursor is $18 (key) and $19 (ID), the count $20.
static NEWEST: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{SELECT_LICENCES} WHERE {} AND ($19::uuid IS NULL OR l.id < $19) ORDER BY l.id DESC LIMIT $20",
        filter_condition()
    )
});
static SOONEST_EXPIRY: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{SELECT_LICENCES} WHERE {}
           AND ($19::uuid IS NULL OR (COALESCE(l.expires_at, 'infinity'), l.id) > ($18::timestamptz, $19))
         ORDER BY COALESCE(l.expires_at, 'infinity'), l.id LIMIT $20",
        filter_condition()
    )
});

fn bind_criteria<'q, O>(
    query: QueryAs<'q, Postgres, O, PgArguments>,
    criteria: &'q Criteria,
) -> QueryAs<'q, Postgres, O, PgArguments> {
    query
        .bind(criteria.product)
        .bind(criteria.customer)
        .bind(criteria.state)
        .bind(criteria.expiry)
        .bind(criteria.key_hash.as_deref())
        .bind(criteria.licence_id)
        .bind(criteria.pattern.as_deref())
        .bind(criteria.test)
}

/// The licences matching `filter`, in `order`, after `cursor`.
pub async fn page(
    executor: impl PgExecutor<'_>,
    filter: &Filter,
    order: Order,
    cursor: Option<&Cursor>,
    count: i64,
) -> Result<Vec<LicenceRow>, sqlx::Error> {
    let sql = match order {
        Order::Newest => NEWEST.as_str(),
        Order::Expiry => SOONEST_EXPIRY.as_str(),
    };
    bind_criteria(bind_criteria(sqlx::query_as(sql), &filter.only), &filter.except)
        .bind(filter.now)
        .bind(cursor.and_then(|cursor| cursor.key.clone()))
        .bind(cursor.map(|cursor| cursor.id))
        .bind(count)
        .fetch_all(executor)
        .await
}

/// Where a page in `order` ending at `licence` continues.
pub fn cursor_after(order: Order) -> impl Fn(&LicenceRow) -> Cursor {
    move |licence| match order {
        Order::Newest => Cursor { key: None, id: licence.id },
        Order::Expiry => Cursor {
            key: Some(licence.expires_at.map_or_else(|| "infinity".to_string(), |at| at.to_rfc3339())),
            id: licence.id,
        },
    }
}

pub fn cursor_fits(order: Order, cursor: &Cursor) -> bool {
    match order {
        Order::Newest => cursor.key.is_none(),
        Order::Expiry => {
            cursor.key.as_deref().is_some_and(|key| key == "infinity" || DateTime::parse_from_rfc3339(key).is_ok())
        }
    }
}

static FIND_MANY: LazyLock<String> =
    LazyLock::new(|| format!("{SELECT_LICENCES} WHERE l.id = ANY($1) ORDER BY l.id DESC"));

pub async fn find_many(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<LicenceRow>, sqlx::Error> {
    sqlx::query_as(FIND_MANY.as_str()).bind(ids).fetch_all(executor).await
}

/// The state of each licence in `ids` that exists, as `(id, state)`.
pub async fn states(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    sqlx::query_as("SELECT license_id, state FROM license_states WHERE license_id = ANY($1) ORDER BY license_id")
        .bind(ids)
        .fetch_all(executor)
        .await
}

/// Every licence of the product, locked until the transaction ends.
pub async fn ids_of_product(connection: &mut PgConnection, product_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM licenses WHERE product_id = $1 ORDER BY id FOR UPDATE")
        .bind(product_id)
        .fetch_all(connection)
        .await
}

/// The test licences among `ids`.
pub async fn test_ids(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM licenses WHERE id = ANY($1) AND is_test ORDER BY id")
        .bind(ids)
        .fetch_all(executor)
        .await
}

/// Locks the licences in `ids` until the transaction ends; returns `(id, revoked)` for those that
/// exist.
pub async fn lock(connection: &mut PgConnection, ids: &[Uuid]) -> Result<Vec<(Uuid, bool)>, sqlx::Error> {
    sqlx::query_as("SELECT id, revoked_at IS NOT NULL FROM licenses WHERE id = ANY($1) ORDER BY id FOR UPDATE")
        .bind(ids)
        .fetch_all(connection)
        .await
}

/// A licence's encrypted key, with what opening it needs.
#[derive(Debug, FromRow)]
pub struct StoredKey {
    pub license_id: Uuid,
    pub product_id: Uuid,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// The encrypted keys of `ids`; revoked licences have none.
pub async fn stored_keys(executor: impl PgExecutor<'_>, ids: &[Uuid]) -> Result<Vec<StoredKey>, sqlx::Error> {
    sqlx::query_as(
        "SELECT k.license_id, l.product_id, k.ciphertext, k.nonce
         FROM license_key_secrets k JOIN licenses l ON l.id = k.license_id
         WHERE k.license_id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(executor)
    .await
}
