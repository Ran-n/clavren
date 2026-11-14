// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 12:05:27.616868
//+ Revised: 	2026/11/14 12:05:27.616868
// -----------------------------------------------------------------------------

//! Every table's columns, against the documented schema.

use super::*;

/// Pins every table's columns. Adding one means checking it against the BCNF table in
/// docs/design-decisions.md (no copies of facts stored elsewhere) and updating both.
#[sqlx::test(migrator = "clavren::database::MIGRATOR")]
async fn table_columns_match_the_documented_schema(pool: PgPool) {
    let expected: &[(&str, &[&str])] = &[
        (
            "activations",
            &[
                "id",
                "license_id",
                "device_public_key",
                "device_fingerprint_hash",
                "device_name",
                "platform",
                "app_version",
                "activated_at",
                "last_checked_at",
                "revoked_at",
                "revocation_reason",
            ],
        ),
        ("admin_preferences", &["admin_user_id", "preferences", "updated_at"]),
        ("admin_recovery_codes", &["id", "admin_user_id", "code_hash", "used_at"]),
        (
            "admin_sessions",
            &["id", "admin_user_id", "token_hash", "created_at", "last_seen_at", "expires_at", "revoked_at"],
        ),
        (
            "admin_users",
            &[
                "id",
                "username",
                "password_hash",
                "totp_secret_ciphertext",
                "totp_secret_nonce",
                "totp_last_used_step",
                "created_at",
                "last_login_at",
                "disabled_at",
            ],
        ),
        (
            "audit_logs",
            &["id", "admin_user_id", "action", "entity_type", "entity_id", "metadata", "ip_address", "created_at"],
        ),
        ("backup_runs", &["id", "finished_at", "archive_name", "size_bytes"]),
        ("check_counts", &["hour", "checks"]),
        ("customers", &["id", "name", "email", "notes", "created_at", "updated_at"]),
        (
            "license_applications",
            &[
                "id",
                "product_id",
                "name",
                "email",
                "message",
                "source",
                "secret_hash",
                "created_at",
                "decided_at",
                "decision",
                "license_id",
                "rejection_reason",
                "key_collected_at",
            ],
        ),
        ("license_key_secrets", &["license_id", "ciphertext", "nonce", "encryption_version"]),
        (
            "licenses",
            &[
                "id",
                "product_id",
                "customer_id",
                "key_lookup",
                "created_at",
                "expires_at",
                "revoked_at",
                "revocation_reason",
                "internal_notes",
                "is_test",
                "price_amount",
                "price_currency",
            ],
        ),
        (
            "products",
            &[
                "id",
                "code",
                "name",
                "description",
                "message_template",
                "is_active",
                "created_at",
                "updated_at",
                "accepts_applications",
                "key_instructions",
                "offline_days",
                "price_amount",
                "price_currency",
            ],
        ),
        ("verification_challenges", &["id", "activation_id", "nonce_hash", "created_at", "expires_at", "consumed_at"]),
    ];
    for (table, columns) in expected {
        let actual: Vec<String> = sqlx::query_scalar(
            "SELECT column_name::text FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = $1 ORDER BY ordinal_position",
        )
        .bind(table)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(actual, *columns, "columns of {table}");
    }
}
