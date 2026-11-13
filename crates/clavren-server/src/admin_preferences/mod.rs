// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.060231
//+ Revised: 	2026/11/11 16:32:50.358554
// -----------------------------------------------------------------------------

//! What the dashboard remembers for each admin, on any browser: theme, the last page, per table
//! its filters, search, order, page size and columns, and whether the Issue panel is open with
//! the product, quantity and expiry chosen in it (never its customer or notes). Stored as JSON, checked against a
//! versioned schema; unknown fields are refused, and so is anything that looks like a licence key,
//! so a key typed into a search box is never saved.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use clavren_protocol::licence_key::LicenceKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::admin_auth::guard::Admin;
use crate::http::AppState;
use crate::http::error::ApiError;
use crate::http::extract::ApiJson;

pub const VERSION: u32 = 1;
const TABLES: [&str; 7] = ["licences", "customers", "products", "activations", "applications", "audit", "backups"];
const MAX_TEXT: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub version: u32,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_page: Option<String>,
    #[serde(default)]
    pub tables: BTreeMap<String, Table>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    #[serde(default)]
    pub filters: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    /// The Issue panel, on the licences table only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<IssueDraft>,
}

/// The Issue panel as the admin left it. Only what isn't personal or free text: the customer and
/// the notes are never remembered.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueDraft {
    pub open: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<NaiveDate>,
}

impl Preferences {
    pub fn check(&self) -> Result<(), ApiError> {
        if self.version != VERSION {
            return Err(ApiError::invalid_request(format!("preferences must be version {VERSION}")));
        }
        if self.last_page.as_deref().is_some_and(|page| !page.starts_with('/') || page.len() > MAX_TEXT) {
            return Err(ApiError::invalid_request("last_page must be a dashboard path"));
        }
        for (name, table) in &self.tables {
            if !TABLES.contains(&name.as_str()) {
                return Err(ApiError::invalid_request(format!("{name} isn't a dashboard table")));
            }
            table.check(name)?;
        }
        Ok(())
    }
}

impl Table {
    fn check(&self, name: &str) -> Result<(), ApiError> {
        let texts = self.filters.iter().flat_map(|(key, value)| [key, value]).chain(&self.search).chain(&self.order);
        let columns = self.columns.iter().flatten();
        let mut all = texts.chain(columns);
        if all.any(|text| text.chars().count() > MAX_TEXT) {
            return Err(ApiError::invalid_request(format!(
                "{name}: a remembered value is longer than {MAX_TEXT} characters"
            )));
        }
        if self.filters.len() > 20 || self.columns.as_ref().is_some_and(|columns| columns.len() > 30) {
            return Err(ApiError::invalid_request(format!("{name}: too many filters or columns")));
        }
        if self.page_size.is_some_and(|size| !(10..=100).contains(&size)) {
            return Err(ApiError::invalid_request(format!("{name}: page_size must be between 10 and 100")));
        }
        if let Some(issue) = &self.issue {
            if name != "licences" {
                return Err(ApiError::invalid_request(format!("{name}: only the licences table has an issue panel")));
            }
            let most = crate::licences::MAX_BATCH;
            if issue.quantity.is_some_and(|quantity| !(1..=most).contains(&(quantity as usize))) {
                return Err(ApiError::invalid_request(format!("{name}: issue.quantity must be between 1 and {most}")));
            }
        }
        let looks_like_a_key =
            self.search.iter().chain(self.filters.values()).any(|text| LicenceKey::parse(text).is_some());
        if looks_like_a_key {
            return Err(ApiError::invalid_request("licence keys are never saved in preferences"));
        }
        Ok(())
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/preferences", get(read).put(replace))
}

/// The admin's preferences; the defaults (theme following the system) until they save any.
async fn read(State(state): State<AppState>, admin: Admin) -> Result<Json<Value>, ApiError> {
    let saved: Option<Value> = sqlx::query_scalar("SELECT preferences FROM admin_preferences WHERE admin_user_id = $1")
        .bind(admin.actor.admin_id)
        .fetch_optional(&state.pool)
        .await?;
    let defaults =
        || serde_json::to_value(Preferences { version: VERSION, ..Preferences::default() }).unwrap_or_default();
    Ok(Json(saved.filter(|value| value.as_object().is_some_and(|object| !object.is_empty())).unwrap_or_else(defaults)))
}

async fn replace(
    State(state): State<AppState>,
    admin: Admin,
    ApiJson(preferences): ApiJson<Preferences>,
) -> Result<Json<Preferences>, ApiError> {
    preferences.check()?;
    let value = serde_json::to_value(&preferences).map_err(ApiError::internal)?;
    sqlx::query(
        "INSERT INTO admin_preferences (admin_user_id, preferences, updated_at) VALUES ($1, $2, $3)
         ON CONFLICT (admin_user_id) DO UPDATE SET preferences = $2, updated_at = $3",
    )
    .bind(admin.actor.admin_id)
    .bind(value)
    .bind(admin.actor.at)
    .execute(&state.pool)
    .await?;
    Ok(Json(preferences))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_table(table: Table) -> Preferences {
        Preferences {
            version: VERSION,
            tables: BTreeMap::from([("licences".to_string(), table)]),
            ..Preferences::default()
        }
    }

    #[test]
    fn sensible_preferences_pass() {
        let table = Table {
            filters: BTreeMap::from([("state".into(), "unused".into())]),
            search: Some("López".into()),
            order: Some("expiry".into()),
            page_size: Some(50),
            columns: Some(vec!["key".into(), "customer".into()]),
            issue: Some(IssueDraft {
                open: true,
                product: Some(Uuid::nil()),
                quantity: Some(1000),
                expires_at: NaiveDate::from_ymd_opt(2027, 12, 31),
            }),
        };
        let preferences = Preferences { theme: Theme::Dark, last_page: Some("/licences".into()), ..with_table(table) };
        assert!(preferences.check().is_ok());
    }

    #[test]
    fn keys_are_never_remembered() {
        let search = with_table(Table { search: Some("brn 79yn 44xq w025 377z".into()), ..Table::default() });
        assert!(search.check().is_err());
        let filter = with_table(Table {
            filters: BTreeMap::from([("q".into(), "BRN-79YN-44XQ-W025-377Z".into())]),
            ..Table::default()
        });
        assert!(filter.check().is_err());
    }

    #[test]
    fn odd_preferences_are_refused() {
        let refused = [
            Preferences { version: 2, ..Preferences::default() },
            Preferences { version: VERSION, last_page: Some("https://evil.example".into()), ..Preferences::default() },
            with_table(Table { page_size: Some(1000), ..Table::default() }),
            with_table(Table { search: Some("x".repeat(201)), ..Table::default() }),
            with_table(Table { columns: Some(vec!["c".into(); 31]), ..Table::default() }),
            with_table(Table {
                issue: Some(IssueDraft { quantity: Some(0), ..IssueDraft::default() }),
                ..Table::default()
            }),
            with_table(Table {
                issue: Some(IssueDraft { quantity: Some(1001), ..IssueDraft::default() }),
                ..Table::default()
            }),
            Preferences {
                version: VERSION,
                tables: BTreeMap::from([(
                    "customers".to_string(),
                    Table { issue: Some(IssueDraft::default()), ..Table::default() },
                )]),
                ..Preferences::default()
            },
            Preferences {
                version: VERSION,
                tables: BTreeMap::from([("secrets".to_string(), Table::default())]),
                ..Preferences::default()
            },
        ];
        for preferences in refused {
            assert!(preferences.check().is_err(), "{preferences:?}");
        }
        assert!(serde_json::from_str::<Preferences>(r#"{"version":1,"colour":"red"}"#).is_err(), "unknown fields");
    }
}
