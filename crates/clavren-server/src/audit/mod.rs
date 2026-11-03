// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 19:43:36.274628
//+ Revised: 	2026/11/03 04:54:19.016000
// -----------------------------------------------------------------------------

//! The audit log: who did what to which entity, when, from where. Entries never hold keys,
//! passwords, codes, tokens, or customers' names and emails; entities are referred to by ID.

mod routes;

use std::net::IpAddr;

pub use routes::routes;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

pub struct AuditEntry<'a> {
    /// `None` for actions without a signed-in admin (the CLI, a failed login, an application).
    pub admin_user_id: Option<Uuid>,
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<Uuid>,
    pub metadata: Value,
    pub ip_address: Option<IpAddr>,
    pub at: DateTime<Utc>,
}

/// A signed-in admin doing something through the API: who, from where, and when.
#[derive(Debug, Clone, Copy)]
pub struct Actor {
    pub admin_id: Uuid,
    pub ip: IpAddr,
    pub at: DateTime<Utc>,
}

impl<'a> AuditEntry<'a> {
    /// `actor` did `action` to the `entity_type` with ID `entity_id`.
    pub fn by(actor: &Actor, action: &'a str, entity_type: &'a str, entity_id: Uuid) -> Self {
        Self {
            admin_user_id: Some(actor.admin_id),
            action,
            entity_type,
            entity_id: Some(entity_id),
            metadata: Value::Object(serde_json::Map::new()),
            ip_address: Some(actor.ip),
            at: actor.at,
        }
    }

    /// Something that happened to an admin account. `actor` is the signed-in admin who did it
    /// (`None` for the CLI or a failed login), `subject` the account, when known.
    pub fn admin_account(
        action: &'a str,
        actor: Option<Uuid>,
        subject: Option<Uuid>,
        ip_address: Option<IpAddr>,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            admin_user_id: actor,
            action,
            entity_type: "admin_user",
            entity_id: subject,
            metadata: Value::Object(serde_json::Map::new()),
            ip_address,
            at,
        }
    }

    /// Something an application did (an activation, a refused check), from `ip`.
    pub fn by_application(
        action: &'a str,
        entity_type: &'a str,
        entity_id: Uuid,
        ip: IpAddr,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            admin_user_id: None,
            action,
            entity_type,
            entity_id: Some(entity_id),
            metadata: Value::Object(serde_json::Map::new()),
            ip_address: Some(ip),
            at,
        }
    }

    pub fn with_metadata(self, metadata: Value) -> Self {
        Self { metadata, ..self }
    }
}

pub async fn record(executor: impl PgExecutor<'_>, entry: AuditEntry<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_logs (id, admin_user_id, action, entity_type, entity_id, metadata, ip_address, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7::inet, $8)",
    )
    .bind(Uuid::now_v7())
    .bind(entry.admin_user_id)
    .bind(entry.action)
    .bind(entry.entity_type)
    .bind(entry.entity_id)
    .bind(entry.metadata)
    .bind(entry.ip_address.map(|ip| ip.to_string()))
    .bind(entry.at)
    .execute(executor)
    .await?;
    Ok(())
}

/// One entry per ID, all in one statement: bulk actions audit every item they touch.
pub async fn record_for_each(
    executor: impl PgExecutor<'_>,
    actor: &Actor,
    action: &str,
    entity_type: &str,
    entity_ids: &[Uuid],
    metadata: Value,
) -> Result<(), sqlx::Error> {
    let entry_ids: Vec<Uuid> = entity_ids.iter().map(|_| Uuid::now_v7()).collect();
    sqlx::query(
        "INSERT INTO audit_logs (id, admin_user_id, action, entity_type, entity_id, metadata, ip_address, created_at)
         SELECT entry_id, $3, $4, $5, entity_id, $6, $7::inet, $8
         FROM unnest($1::uuid[], $2::uuid[]) AS items (entry_id, entity_id)",
    )
    .bind(entry_ids)
    .bind(entity_ids)
    .bind(actor.admin_id)
    .bind(action)
    .bind(entity_type)
    .bind(metadata)
    .bind(actor.ip.to_string())
    .bind(actor.at)
    .execute(executor)
    .await?;
    Ok(())
}
