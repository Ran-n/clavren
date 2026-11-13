// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:16.338815
//+ Revised: 	2026/11/12 23:57:45.272804
// -----------------------------------------------------------------------------

//! `GET /api/admin/overview`: the numbers on the dashboard's first page. Test licences, their
//! computers and their checks are left out: they are no customer's. What keys were charged is
//! summed per currency (never converted), over the licences that still exist.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::admin_auth::guard::Admin;
use crate::applications;
use crate::backups::{self, BackupRun};
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::money::Currency;

/// A backup older than this is shown as a warning.
pub const BACKUP_WARNING_AGE: TimeDelta = TimeDelta::hours(48);

pub fn routes() -> Router<AppState> {
    Router::new().route("/overview", get(overview))
}

#[derive(Debug, Serialize, FromRow)]
struct Counts {
    active_products: i64,
    inactive_products: i64,
    unused: i64,
    redeemed: i64,
    revoked: i64,
    /// Computers with an active activation.
    active_computers: i64,
    /// Of those, the ones that checked in (or activated) in the last 24 hours.
    computers_checked_last_24h: i64,
    /// Successful checks in the last 24 hours, counted by the hour: the current hour and the 23
    /// before it.
    checks_last_24h: i64,
}

#[derive(Debug, Serialize)]
struct Overview {
    #[serde(flatten)]
    counts: Counts,
    last_backup: Option<BackupRun>,
    /// No backup at all, or the last one is over 48 hours old.
    backup_overdue: bool,
    /// Licence applications waiting for a decision.
    pending_applications: i64,
    /// What keys were charged, per currency, A to Z.
    charged: Vec<Charged>,
    /// Licences (not test ones) with no recorded price.
    unpriced: i64,
    /// The currency the dashboard preselects for prices (`CLAVREN_CURRENCY`).
    default_currency: Currency,
}

/// The sums of one currency, in its minor unit. A free key counts as 0.
#[derive(Debug, Serialize, FromRow)]
struct Charged {
    currency: String,
    last_30_days: i64,
    total: i64,
}

async fn overview(State(state): State<AppState>, admin: Admin) -> Result<Json<Overview>, ApiError> {
    let since = admin.actor.at - TimeDelta::hours(24);
    let counts: Counts = sqlx::query_as(
        "SELECT (SELECT count(*) FROM products WHERE is_active) AS active_products,
                (SELECT count(*) FROM products WHERE NOT is_active) AS inactive_products,
                count(*) FILTER (WHERE s.state = 'unused') AS unused,
                count(*) FILTER (WHERE s.state = 'redeemed') AS redeemed,
                count(*) FILTER (WHERE s.state = 'revoked') AS revoked,
                (SELECT count(*) FROM activations a JOIN licenses al ON al.id = a.license_id
                 WHERE a.revoked_at IS NULL AND NOT al.is_test) AS active_computers,
                (SELECT count(*) FROM activations a JOIN licenses al ON al.id = a.license_id
                 WHERE a.revoked_at IS NULL AND NOT al.is_test AND COALESCE(a.last_checked_at, a.activated_at) > $1)
                    AS computers_checked_last_24h,
                (SELECT COALESCE(sum(checks), 0)::bigint FROM check_counts WHERE hour > $1) AS checks_last_24h
         FROM license_states s JOIN licenses l ON l.id = s.license_id
         WHERE NOT l.is_test",
    )
    .bind(since)
    .fetch_one(&state.pool)
    .await?;
    let last_backup = backups::latest(&state.pool, 1).await?.pop();
    let pending_applications = applications::repository::pending_count(&state.pool).await?;
    let backup_overdue = last_backup.as_ref().is_none_or(|run| overdue(run.finished_at, admin.actor.at));
    let charged: Vec<Charged> = sqlx::query_as(
        "SELECT price_currency AS currency,
                COALESCE(sum(price_amount) FILTER (WHERE created_at > $1), 0)::bigint AS last_30_days,
                sum(price_amount)::bigint AS total
         FROM licenses WHERE NOT is_test AND price_amount IS NOT NULL
         GROUP BY price_currency ORDER BY price_currency",
    )
    .bind(admin.actor.at - TimeDelta::days(30))
    .fetch_all(&state.pool)
    .await?;
    let unpriced: i64 = sqlx::query_scalar("SELECT count(*) FROM licenses WHERE NOT is_test AND price_amount IS NULL")
        .fetch_one(&state.pool)
        .await?;
    let default_currency = state.settings.licensing.default_currency.clone();
    Ok(Json(Overview {
        counts,
        last_backup,
        backup_overdue,
        pending_applications,
        charged,
        unpriced,
        default_currency,
    }))
}

fn overdue(finished_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now - finished_at > BACKUP_WARNING_AGE
}
